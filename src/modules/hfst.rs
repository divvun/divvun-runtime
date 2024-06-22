use std::{collections::HashMap, sync::Arc, thread::JoinHandle};

use async_trait::async_trait;
use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};

use crate::{
    ast,
    modules::{Arg, Command, Module, Ty},
};

use super::{CommandRunner, Context, Input, SharedInputFut};

inventory::submit! {
    Module {
        name: "hfst",
        commands: &[
            Command {
                name: "tokenize",
                input: &[Ty::String],
                args: &[Arg { name: "model_path", ty: Ty::Path }],
                init: Tokenize::new,
                returns: Ty::String,
            }
        ]
    }
}

pub struct Tokenize {
    _context: Arc<Context>,
    input_tx: Sender<Option<String>>,
    output_rx: Mutex<Receiver<Option<String>>>,
    _thread: JoinHandle<()>,
}

impl Tokenize {
    pub fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, crate::modules::Error> {
        tracing::debug!("Creating tokenize");
        let model_path = kwargs
            .remove("model_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| crate::modules::Error("model_path missing".to_string()))?;
        let model_path = context.extract_to_temp_dir(model_path)?;

        let (input_tx, mut input_rx) = mpsc::channel(1);
        let (output_tx, output_rx) = mpsc::channel(1);

        let thread = std::thread::spawn(move || {
            tracing::debug!("init hfst tokenizer BEFORE");
            let tokenizer = hfst::Tokenizer::new(model_path).unwrap();
            tracing::debug!("init hfst tokenizer");

            loop {
                let Some(Some(input)): Option<Option<String>> = input_rx.blocking_recv() else {
                    break;
                };

                output_tx.blocking_send(tokenizer.tokenize(&input)).unwrap();
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
impl CommandRunner for Tokenize {
    async fn forward(
        self: Arc<Self>,
        input: SharedInputFut,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.await?.try_into_string()?;

        self.input_tx
            .send(Some(input))
            .await
            .expect("input tx send");
        let mut output_rx = self.output_rx.lock().await;
        let output = output_rx.recv().await.expect("output rx recv");

        Ok(output.unwrap_or_else(|| "".to_string()).into())
    }

    fn name(&self) -> &'static str {
        "hfst::tokenize"
    }
}
