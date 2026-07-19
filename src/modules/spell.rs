use std::{collections::HashMap, sync::Arc, thread::JoinHandle};

use async_trait::async_trait;
use divvun_fst::{speller::Speller, transducer::thfst::MmapThfstTransducer};
use divvun_runtime_macros::rt_command;
use tokio::sync::{
    Mutex,
    mpsc::{self, Receiver, Sender},
};

use crate::ast;

use super::{CommandRunner, Context, Error, PipelineValue, PipelineValues};

/// Spelling suggestion using divvun_fst
#[derive(facet::Facet)]
struct Suggest {
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
    module = "spell",
    name = "suggest",
    input = [String],
    output = "Json",
    args = [lexicon_path = "Path", mutator_path = "Path"]
)]
impl Suggest {
    pub async fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, Error> {
        use divvun_fst::tokenizer::Tokenize as _;

        let lexicon_path = kwargs
            .remove("lexicon_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| {
                Error::msg("lexicon_path missing").at("pipeline.json", "/args/lexicon_path")
            })?;
        let mutator_path = kwargs
            .remove("mutator_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| {
                Error::msg("mutator_path missing").at("pipeline.json", "/args/mutator_path")
            })?;

        let (input_tx, mut input_rx) = mpsc::channel(1);
        let (output_tx, output_rx) = mpsc::channel(1);

        let model_context = context.clone();
        let thread =
            std::thread::spawn(move || {
                let lexicon = model_context
                    .load_fst::<MmapThfstTransducer>(&lexicon_path)
                    .unwrap();
                let mutator = model_context
                    .load_fst::<MmapThfstTransducer>(&mutator_path)
                    .unwrap();
                let speller = divvun_fst::speller::HfstSpeller::new(mutator, lexicon);

                loop {
                    let Some(Some(input)): Option<Option<String>> = input_rx.blocking_recv() else {
                        break;
                    };

                    let results = input.word_bound_indices().map(|(pos, word)| {
                    let results = speller.clone().suggest(&word);
                    serde_json::json!({ "index": pos, "word": word, "suggestions": results })
                }).collect::<Vec<_>>();

                    let results = serde_json::to_string(&results).unwrap();

                    output_tx.blocking_send(Some(results)).unwrap();
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
impl CommandRunner for Suggest {
    async fn forward(
        self: Arc<Self>,
        input: PipelineValue,
        _config: Arc<serde_json::Value>,
    ) -> Result<PipelineValues, crate::modules::Error> {
        let input = input.try_into_string()?;

        self.input_tx
            .send(Some(input))
            .await
            .expect("input tx send");
        let mut output_rx = self.output_rx.lock().await;
        let value = output_rx.recv().await.expect("output rx recv");

        Ok(value.unwrap_or_default().into())
    }

    fn name(&self) -> &'static str {
        "spell::suggest"
    }
}
