use std::{collections::HashMap, sync::Arc, thread::JoinHandle};

use async_trait::async_trait;
use divvun_runtime_macros::rt_command;
use divvunspell::{
    speller::Speller,
    transducer::{Transducer, thfst::MemmapThfstTransducer},
    vfs::Fs,
};
use tokio::sync::{
    Mutex,
    mpsc::{self, Receiver, Sender},
};

use crate::ast;

use super::{CommandRunner, Context, Error, Input, SharedInputFut};

struct Suggest {
    _context: Arc<Context>,
    input_tx: Sender<Option<String>>,
    output_rx: Mutex<Receiver<Option<String>>>,
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
    pub fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, Error> {
        use divvunspell::tokenizer::Tokenize as _;

        let lexicon_path = kwargs
            .remove("lexicon_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("lexicon_path missing".to_string()))?;
        let mutator_path = kwargs
            .remove("mutator_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("mutator_path missing".to_string()))?;

        let lexicon_path = context.extract_to_temp_dir(lexicon_path)?;
        let mutator_path = context.extract_to_temp_dir(mutator_path)?;

        let (input_tx, mut input_rx) = mpsc::channel(1);
        let (output_tx, output_rx) = mpsc::channel(1);

        let thread =
            std::thread::spawn(move || {
                let lexicon = MemmapThfstTransducer::from_path(&Fs, lexicon_path).unwrap();
                let mutator = MemmapThfstTransducer::from_path(&Fs, mutator_path).unwrap();
                let speller = divvunspell::speller::HfstSpeller::new(mutator, lexicon);

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
        input: Input,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
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
