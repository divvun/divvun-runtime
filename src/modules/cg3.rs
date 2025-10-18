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
    fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        let key = kwargs
            .remove("key")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("key missing".to_string()))?;

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
    pub fn new(
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
    pub fn new(
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

    fn sentences(&self, input: &str) -> Vec<String> {
        match self.mode {
            SentenceMode::SurfaceForm => self.sentences_surface(input),
            SentenceMode::PhonologicalForm => self.sentences_phonological(input),
        }
    }

    fn sentences_phonological(&self, input: &str) -> Vec<String> {
        let output = cg3::Output::new(input);
        let mut sentences = Vec::new();
        let mut current_sentence = Vec::new();

        tracing::debug!("Processing phonological sentences from input: {}", input);

        for block in output.iter().filter_map(Result::ok) {
            match block {
                cg3::Block::Cohort(cohort) => {
                    if let Some(reading) = cohort.readings.first() {
                        // The base_form IS the normalized/phonological form after normalization!
                        let phonological_text = reading.base_form.trim_matches('"');
                        tracing::debug!(
                            "Found cohort: surface={}, base_form={}",
                            cohort.word_form,
                            reading.base_form
                        );
                        current_sentence.push(phonological_text.to_string());
                    }
                }
                cg3::Block::Text(text) => {
                    tracing::debug!("Found text block: {}", text);
                    if text.trim() == "." || text.trim() == "!" || text.trim() == "?" {
                        // Sentence boundary - join current sentence and start new one
                        if !current_sentence.is_empty() {
                            let sentence = current_sentence.join(" ");
                            tracing::debug!("Completed sentence: {}", sentence);
                            sentences.push(sentence);
                            current_sentence.clear();
                        }
                    } else if !text.trim().is_empty() && text.trim() != ":" {
                        current_sentence.push(text.trim().to_string());
                    }
                }
                cg3::Block::Escaped(_) => {
                    // Skip escaped content for sentences
                }
            }
        }

        // Add any remaining sentence
        if !current_sentence.is_empty() {
            let sentence = current_sentence.join(" ");
            tracing::debug!("Final sentence: {}", sentence);
            sentences.push(sentence);
        }

        tracing::debug!("Final sentences: {:?}", sentences);
        sentences
    }

    fn sentences_surface(&self, input: &str) -> Vec<String> {
        let output = cg3::Output::new(input);
        let mut sentences = Vec::new();
        let mut current_sentence = Vec::new();

        for block in output.iter().filter_map(Result::ok) {
            match block {
                cg3::Block::Cohort(cohort) => {
                    current_sentence.push(cohort.word_form.to_string());
                }
                cg3::Block::Text(text) => {
                    if text.trim() == "." || text.trim() == "!" || text.trim() == "?" {
                        // Sentence boundary
                        if !current_sentence.is_empty() {
                            sentences.push(current_sentence.join(" "));
                            current_sentence.clear();
                        }
                    } else if !text.trim().is_empty() {
                        current_sentence.push(text.trim().to_string());
                    }
                }
                cg3::Block::Escaped(_) => {
                    // Skip escaped content
                }
            }
        }

        // Add any remaining sentence
        if !current_sentence.is_empty() {
            sentences.push(current_sentence.join(" "));
        }

        sentences
    }
}

#[async_trait]
impl CommandRunner for Sentences {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.try_into_string()?;

        let output = cg3::Output::new(&input);
        // let result = output.to_string();
        let mut result = String::new();

        tracing::debug!("Processing sentences in mode: {:?}", self.mode);

        // Process each block
        for block in output.iter().filter_map(Result::ok) {
            match block {
                cg3::Block::Cohort(cohort) => {
                    if let Some(reading) = cohort.readings.first() {
                        match self.mode {
                            SentenceMode::SurfaceForm => {
                                result.push_str(&cohort.word_form);
                            }
                            SentenceMode::PhonologicalForm => {
                                tracing::debug!("Processing cohort: {:?}", cohort);
                                tracing::debug!("Reading tags: {:?}", reading.tags);

                                // Look for phonological form in tags ending with "phon
                                let phonological_text = if let Some(phon_tag) =
                                    reading.tags.iter().find(|tag| tag.ends_with("\"phon"))
                                {
                                    // Extract text between quotes: "text"phon -> text
                                    &phon_tag[1..phon_tag.len() - 5]
                                } else {
                                    // Fallback to word form if no phon tag found
                                    &cohort.word_form
                                };

                                tracing::debug!("Using phonological form: {}", phonological_text);
                                result.push_str(phonological_text);
                            }
                        }
                    }
                }
                cg3::Block::Text(text) => {}
                cg3::Block::Escaped(escaped) => {
                    result.push_str(&escaped);
                }
            }
        }

        // let sentences = cg3::Output::new(input)
        //     .sentences()
        //     .collect::<Result<Vec<_>, _>>()
        //     .map_err(|e| Error(e.to_string()))?;
        let sentences = result
            .trim_end_matches('.')
            .split(".")
            .map(|x| x.trim().to_string())
            .collect::<Vec<_>>();
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
    pub fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        tracing::debug!("Creating vislcg3");

        let model_path = kwargs
            .remove("model_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("model_path missing".to_string()))?;
        let model_path = context.extract_to_temp_dir(model_path)?;

        let config = match kwargs
            .remove("config")
            .and_then(|x| x.value)
            .map(|x| x.try_as_json())
        {
            Some(Ok(c)) => {
                let config: Vislcg3Config = serde_json::from_value(c)
                    .map_err(|e| Error(format!("config arg is not valid SpellerConfig: {}", e)))?;
                Some(config)
            }
            Some(Err(e)) => return Err(Error(format!("config arg is not valid JSON: {}", e))),
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
    pub fn new(
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
            serde_json::to_value(&results).map_err(|e| Error(e.to_string()))?,
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
