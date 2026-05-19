use std::{collections::HashMap, str::FromStr, sync::Arc, thread::JoinHandle};

use async_trait::async_trait;
use divvun_runtime_macros::{rt_command, rt_struct};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::sync::{
    Mutex,
    mpsc::{self, Receiver, Sender},
};

use crate::ast;

use super::{CommandRunner, Context, Error, Input};

/// CG3 stream command injector
#[derive(facet::Facet)]
pub struct StreamCmd {
    #[facet(opaque)]
    _context: Arc<Context>,
    key: String,
}

#[rt_command(
    module = "cg3",
    name = "streamcmd",
    input = [String],
    output = "String",
    kind = "cg3",
    args = [key = "String"]
)]
impl StreamCmd {
    async fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        let key = kwargs
            .remove("key")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error::msg("key missing").at("pipeline.json", "/args/key"))?;

        Ok(Arc::new(Self {
            _context: context,
            key,
        }) as _)
    }
}

#[async_trait]
impl CommandRunner for StreamCmd {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.try_into_string()?;

        if self.key != "REMVAR" && self.key != "SETVAR" {
            return Ok(format!("<STREAMCMD:{}>\n{input}", self.key).into());
        }

        let value = match &*config {
            serde_json::Value::Null => {
                return Ok(input.into());
            }
            serde_json::Value::Bool(x) => x.to_string(),
            serde_json::Value::Number(x) => x.to_string(),
            serde_json::Value::String(x) => x.to_string(),
            serde_json::Value::Array(values) => {
                if values.is_empty() {
                    return Ok(input.into());
                }
                values
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            }
            serde_json::Value::Object(map) => {
                if map.is_empty() {
                    return Ok(input.into());
                }

                map.iter()
                    .map(|(k, v)| {
                        match v {
                            serde_json::Value::Null => k.to_string(),
                            serde_json::Value::Bool(x) => format!("{}={}", k, x),
                            serde_json::Value::Number(x) => format!("{}={}", k, x),
                            serde_json::Value::String(x) => format!("{}={}", k, x),
                            serde_json::Value::Array(arr) => {
                                let arr_str = arr
                                    .iter()
                                    .map(|x| x.to_string())
                                    .collect::<Vec<_>>()
                                    .join(",");
                                format!("{}=[{}]", k, arr_str)
                            }
                            serde_json::Value::Object(_) => {
                                // Nested objects are not supported in this context
                                k.to_string()
                            }
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            }
        };

        Ok(format!("<STREAMCMD:{}:{value}>\n{input}", self.key).into())
    }

    fn name(&self) -> &'static str {
        "cg3::streamcmd"
    }
}

/// Multi-word expression splitter
#[derive(facet::Facet)]
pub struct Mwesplit {
    #[facet(opaque)]
    _context: Arc<Context>,
    #[facet(opaque)]
    input_tx: Sender<Option<String>>,
    #[facet(opaque)]
    output_rx: Mutex<Receiver<Option<String>>>,
    #[facet(opaque)]
    _thread: JoinHandle<()>,
}

#[rt_command(
    module = "cg3",
    name = "mwesplit",
    input = [String],
    output = "String",
    kind = "cg3",
    args = []
)]
impl Mwesplit {
    pub async fn new(
        context: Arc<Context>,
        _kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        tracing::debug!("Creating mwesplit");
        let (input_tx, mut input_rx) = mpsc::channel(1);
        let (output_tx, output_rx) = mpsc::channel(1);

        let thread = std::thread::spawn(move || {
            tracing::debug!("init cg3 mwesplit BEFORE");
            let mwesplit = cg3::MweSplit::new();
            tracing::debug!("init cg3 mwesplit");

            loop {
                let Some(Some(input)): Option<Option<String>> = input_rx.blocking_recv() else {
                    break;
                };

                output_tx.blocking_send(mwesplit.run(&input)).unwrap();
            }
        });

        Ok(Arc::new(Self {
            _context: context,
            input_tx,
            output_rx: Mutex::new(output_rx),
            _thread: thread,
        }) as _)
    }
}

#[derive(facet::Facet, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
enum SentenceMode {
    #[default]
    SurfaceForm,
    PhonologicalForm,
}

impl FromStr for SentenceMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "surface" => Ok(Self::SurfaceForm),
            "phonological" => Ok(Self::PhonologicalForm),
            _ => Err(()),
        }
    }
}

/// Extract sentences from CG3 stream
#[derive(facet::Facet)]
struct Sentences {
    mode: SentenceMode,
}

#[rt_command(
    module = "cg3",
    name = "sentences",
    input = [String],
    output = "ArrayString",
    args = [mode = "String"]
)]
impl Sentences {
    pub async fn new(
        _context: Arc<Context>,
        _kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        let mode = _kwargs
            .get("mode")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string())
            .and_then(|x| x.parse::<SentenceMode>().ok())
            .unwrap_or_default();
        Ok(Arc::new(Self { mode }))
    }
}

fn cohort_text<'a>(cohort: &cg3::Cohort<'a>, mode: SentenceMode) -> &'a str {
    match mode {
        SentenceMode::SurfaceForm => cohort.word_form,
        SentenceMode::PhonologicalForm => cohort
            .readings
            .first()
            .and_then(|r| {
                r.tags
                    .iter()
                    .find(|tag| tag.ends_with("\"phon"))
                    .map(|tag| &tag[1..tag.len() - 5])
            })
            .unwrap_or(cohort.word_form),
    }
}

fn extract_sentences(
    input: &str,
    mode: SentenceMode,
    breakers: &std::collections::HashSet<String>,
) -> Vec<String> {
    let output = cg3::Output::new(input);
    let mut sentences: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();

    for block in output.iter().filter_map(Result::ok) {
        if let cg3::Block::Cohort(cohort) = block {
            current.push(cohort_text(&cohort, mode).to_string());
            if super::cg3_util::is_sentence_boundary(&cohort, breakers) {
                sentences.push(current.join(" "));
                current.clear();
            }
        }
    }

    if !current.is_empty() {
        sentences.push(current.join(" "));
    }

    sentences
}

#[async_trait]
impl CommandRunner for Sentences {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.try_into_string()?;
        let breakers = super::cg3_util::default_sentence_breakers();
        tracing::debug!("Processing sentences in mode: {:?}", self.mode);
        let sentences = extract_sentences(&input, self.mode, &breakers);
        Ok(sentences.into())
    }

    // fn forward_stream(
    //     self: Arc<Self>,
    //     mut input: InputRx,
    //     mut output: InputTx,
    //     config: Arc<serde_json::Value>,
    // ) -> tokio::task::JoinHandle<Result<(), Error>>
    // where
    //     Self: Send + Sync + 'static,
    // {
    //     let this = self.clone();
    //     tokio::spawn(async move {
    //         loop {
    //             let event = input.recv().await.map_err(|e| Error(e.to_string()))?;
    //             let this = this.clone();
    //             match event {
    //                 InputEvent::Input(input) => {
    //                     tracing::debug!("INPUT: {:?}", input);
    //                     let event = match this.forward(input, config.clone()).await {
    //                         Ok(event) => event,
    //                         Err(e) => {
    //                             output
    //                                 .send(InputEvent::Error(e.clone()))
    //                                 .map_err(|e| Error(e.to_string()))?;
    //                             return Err(e);
    //                         }
    //                     };
    //                     let x = event.try_into_string_array().unwrap();
    //                     for x in x {
    //                         tracing::debug!("SEND OUTPUT: {:?}", x);
    //                         output
    //                             .send(InputEvent::Input(Input::String(x)))
    //                             .map_err(|e| Error(e.to_string()))?;
    //                     }
    //                     output
    //                         .send(InputEvent::Finish)
    //                         .map_err(|e| Error(e.to_string()))?;
    //                 }
    //                 InputEvent::Finish => {
    //                     output
    //                         .send(InputEvent::Finish)
    //                         .map_err(|e| Error(e.to_string()))?;
    //                 }
    //                 InputEvent::Error(e) => {
    //                     output
    //                         .send(InputEvent::Error(e.clone()))
    //                         .map_err(|e| Error(e.to_string()))?;
    //                     return Err(e);
    //                 }
    //                 InputEvent::Close => {
    //                     output
    //                         .send(InputEvent::Close)
    //                         .map_err(|e| Error(e.to_string()))?;
    //                     break;
    //                 }
    //             }
    //         }
    //         Ok(())
    //     })
    // }

    fn name(&self) -> &'static str {
        "cg3::sentences"
    }
}

#[async_trait]
impl CommandRunner for Mwesplit {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.try_into_string()?;

        self.input_tx
            .send(Some(input))
            .await
            .expect("input tx send");
        let mut output_rx = self.output_rx.lock().await;
        let output = output_rx.recv().await.expect("output rx recv");

        Ok(output.unwrap_or_else(|| "".to_string()).into())
    }

    fn name(&self) -> &'static str {
        "cg3::mwesplit"
    }
}

/// Constraint Grammar 3 disambiguator
#[derive(facet::Facet)]
pub struct Vislcg3 {
    #[facet(opaque)]
    _context: Arc<Context>,
    #[facet(opaque)]
    input_tx: Sender<Option<String>>,
    #[facet(opaque)]
    output_rx: Mutex<Receiver<Option<String>>>,
    #[facet(opaque)]
    _thread: JoinHandle<()>,
}

#[rt_struct(module = "cg3")]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Vislcg3Config {
    trace: bool,
}

#[rt_command(
    module = "cg3",
    name = "vislcg3",
    input = [String],
    output = "String",
    kind = "cg3",
    args = [
        model_path = "Path",
        config? = "Vislcg3Config",
    ]
)]
impl Vislcg3 {
    pub async fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        tracing::debug!("Creating vislcg3");

        let model_path = kwargs
            .remove("model_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| {
                Error::msg("model_path missing").at("pipeline.json", "/args/model_path")
            })?;
        let model_path = context.extract_to_temp_dir(model_path).await?;

        let config = match kwargs
            .remove("config")
            .and_then(|x| x.value)
            .map(|x| x.try_as_json())
        {
            Some(Ok(c)) => {
                let config: Vislcg3Config = serde_json::from_value(c)
                    .map_err(|e| Error::wrap(e).at("pipeline.json", "/args/config"))?;
                Some(config)
            }
            Some(Err(e)) => {
                return Err(Error::msg(format!("config arg is not valid JSON: {}", e))
                    .at("pipeline.json", "/args/config"));
            }
            None => None,
        };
        let config = config.unwrap_or_default();

        let (input_tx, mut input_rx) = mpsc::channel(1);
        let (output_tx, output_rx) = mpsc::channel(1);

        let thread = std::thread::spawn(move || {
            let applicator = cg3::Applicator::new(&model_path);
            applicator.set_trace(config.trace);

            loop {
                let Some(Some(input)): Option<Option<String>> = input_rx.blocking_recv() else {
                    break;
                };

                output_tx.blocking_send(applicator.run(&input)).unwrap();
            }
        });

        Ok(Arc::new(Self {
            _context: context,
            input_tx,
            output_rx: Mutex::new(output_rx),
            _thread: thread,
        }) as _)
    }
}

#[async_trait]
impl CommandRunner for Vislcg3 {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.try_into_string()?;

        self.input_tx
            .send(Some(input))
            .await
            .expect("input tx send");
        let mut output_rx = self.output_rx.lock().await;
        let output = output_rx.recv().await.expect("output rx recv");

        Ok(output.unwrap_or_else(|| "".to_string()).into())
    }

    fn name(&self) -> &'static str {
        "cg3::vislcg3"
    }
}

/// Convert CG3 stream to JSON format
#[derive(facet::Facet)]
pub struct ToJson {
    #[facet(opaque)]
    _context: Arc<Context>,
}

#[rt_command(
    module = "cg3",
    name = "to_json",
    input = [String],
    output = "Json"
)]
impl ToJson {
    pub async fn new(
        _context: Arc<Context>,
        _kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        Ok(Arc::new(Self { _context }))
    }
}

#[async_trait]
impl CommandRunner for ToJson {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.try_into_string()?;

        let results = CG_LINE
            .captures_iter(&input)
            .map(|x| x.iter().map(|x| x.map(|x| x.as_str())).collect::<Vec<_>>())
            .collect::<Vec<_>>();

        Ok(Input::Json(
            serde_json::to_value(&results).map_err(Error::wrap)?,
        ))
    }

    fn name(&self) -> &'static str {
        "cg3::to_json"
    }
}

pub static CG_LINE: Lazy<Regex> = Lazy::<Regex>::new(|| {
    Regex::new(
        "^
(\"<(.*)>\".*
|(\t+)(\"[^\"]*\"\\S*)((?:\\s+\\S+)*)\\s*
|:(.*)
|(<STREAMCMD:FLUSH>)
|(;\t+.*)
)",
    )
    .unwrap()
});

#[cfg(test)]
mod sentences_tests {
    use super::*;

    const SOUTH_SAMI: &str = "\"<Gïelejarngi>\"\n\t\"gïelejarnge\" N Sem/Plc Pl Gen <W:0.0> @>N #1->2 \"gïelejarngi\"phon\n: \n\"<darjomh>\"\n\t\"darjome\" N Sem/Feat Pl Nom <W:0.0> @SUBJ> #2->3 \"darjomh\"phon\n: \n\"<viehkiehtieh>\"\n\t\"viehkiehtidh\" <mv> V TV Ind Prs Pl3 <W:0.0> @FMV #3->0 \"viehkiehtieh\"phon\n: \n\"<saemien>\"\n\t\"saemien\" A Attr <W:0.0> @>N #4->5 \"saemien\"phon\n: \n\"<gïelem>\"\n\t\"gïele\" N Sem/Lang_Tool Sg Acc <W:0.0> @<OBJ #5->3 \"gïelem\"phon\n: \n\"<våajnoes>\"\n\t\"våajnoes\" Adv <W:0.0> @<ADVL #6->3 \"våajnoes\"phon\n: \n\"<darjodh>\"\n\t\"darjodh\" <mv> V TV Inf <W:0.0> @IMV #7->3 \"darjodh\"phon\n: \n\"<voengesne>\"\n\t\"voenge\" N Sem/Plc-abstr Sg Ine <W:0.0> @<ADVL #8->7 \"voengesne\"phon\n\"<.>\"\n\t\".\" CLB <W:0.0> #9->3 \".\"phon\n: \n\"<Gïelejarngh>\"\n\t\"gïelejarnge\" N Sem/Plc Pl Nom <W:0.0> @SUBJ> #1->5 \"gïelejarngh\"phon\n: \n\"<sijjen>\"\n\t\"sijjieh\" Pron Logo Pl3 Gen <W:0.0> @>N #2->3 \"sijjen\"phon\n: \n\"<gïeledajvh>\"\n\t\"gïeledajve\" N Sem/Plc Pl Nom <W:0.0> @SUBJ> #3->5 \"gïeledajvh\"phon\n: \n\"<våaroeminie>\"\n\t\"våarome\" N Sem/Semcon Ess <W:0.0> @ADVL> #4->5 \"våaroeminie\"phon\n: \n\"<utnieh>\"\n\t\"utnedh\" <mv> V <TH-Acc-Any><RO-Ess-Any> TV Ind Prs Pl3 <W:0.0> @FMV #5->0 \"utnieh\"phon\n\"<,>\"\n\t\",\" CLB <W:0.0> #6->7 \",\"phon\n: \n\"<jïh>\"\n\t\"jïh\" CC <W:0.0> @CNP #7->5 \"jïh\"phon\n: \n\"<råajvarimmiejgujmie>\"\n\t\"råajvarimmie\" N Sem/Act Pl Com NoUml <W:0.0> @<ADVL #8->5 \"råajvarimmiejgujmie\"phon\n: \n\"<nierhkieh>\"\n\t\"nïerhkedh\" <mv> V TV Ind Prs Pl3 Uml <W:0.0> @FMV #9->5 \"nierhkieh\"phon\n: \n\"<mah>\"\n\t\"mij\" Pron Interr Pl Nom <W:0.0> @SUBJ> #10->11 \"mah\"phon\n: \n\"<leah>\"\n\t\"lea\" <aux> V IV Ind Prs Pl3 <W:0.0> @FAUX #11->5 \"leah\"phon\n: \n\"<daerpiesvoetide>\"\n\t\"daerpiesvoete\" N Sem/Perc-phys Pl Acc <W:0.0> @OBJ> #12->13 \"daerpiesvoetide\"phon\n: \n\"<sjïehtedamme>\"\n\t\"sjïehtedidh\" v1 <mv> V TV PrfPrc <W:0.0> @IMV #13->11 \"sjïehtedamme\"phon\n: \n\"<dejnie>\"\n\t\"dïhte\" Pron Dem Pl Ine <W:0.0> @>N #14->16 \"dejnie\"phon\n: \n\"<ovmessie>\"\n\t\"ovmessie\" A Attr <W:0.0> @>N #15->16 \"ovmessie\"phon\n: \n\"<dajvine>\"\n\t\"dajve\" N Sem/Plc Pl Ine <W:0.0> @<ADVL #16->13 \"dajvine\"phon\n\"<?>\"\n\t\"?\" CLB <W:0.0> #17->5 \"?\"phon\n";

    #[test]
    fn south_sami_splits_into_two_sentences_surface() {
        let breakers = crate::modules::cg3_util::default_sentence_breakers();
        let sentences = extract_sentences(SOUTH_SAMI, SentenceMode::SurfaceForm, &breakers);
        assert_eq!(
            sentences.len(),
            2,
            "expected 2 sentences (split on . and ?), got: {sentences:#?}"
        );
        assert!(
            sentences[0].ends_with('.'),
            "sentence 1 should retain its boundary '.', got: {:?}",
            sentences[0]
        );
        assert!(
            sentences[1].ends_with('?'),
            "sentence 2 should retain its boundary '?', got: {:?}",
            sentences[1]
        );
        assert!(
            sentences[1].contains(','),
            "comma should be retained mid-sentence, not split on, got: {:?}",
            sentences[1]
        );
    }

    #[test]
    fn south_sami_splits_into_two_sentences_phonological() {
        let breakers = crate::modules::cg3_util::default_sentence_breakers();
        let sentences = extract_sentences(SOUTH_SAMI, SentenceMode::PhonologicalForm, &breakers);
        assert_eq!(
            sentences.len(),
            2,
            "expected 2 sentences in phonological mode, got: {sentences:#?}"
        );
        assert!(sentences[0].ends_with('.'));
        assert!(sentences[1].ends_with('?'));
    }
}
