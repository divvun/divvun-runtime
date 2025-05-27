use std::{collections::HashMap, sync::Arc, thread::JoinHandle};

use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};

use crate::{
    ast,
    modules::{Arg, Command, Module, Ty},
};

use super::{CommandRunner, Context, Error, Input, InputEvent, InputRx, InputTx, SharedInputFut};

inventory::submit! {
    Module {
        name: "cg3",
        commands: &[
            Command {
                name: "mwesplit",
                input: &[Ty::String],
                args: &[],
                init: Mwesplit::new,
                returns: Ty::String,
            },
            Command {
                name: "to_json",
                input: &[Ty::String],
                args: &[],
                init: ToJson::new,
                returns: Ty::Json,
            },
            Command {
                name: "vislcg3",
                input: &[Ty::String],
                args: &[
                    Arg {
                        name: "model_path",
                        ty: Ty::Path,
                    },
                ],
                init: Vislcg3::new,
                returns: Ty::String,
            },
            Command {
                name: "sentences",
                input: &[Ty::String],
                args: &[],
                init: Sentences::new,
                returns: Ty::ArrayString,
            }
        ]
    }
}

pub struct Mwesplit {
    _context: Arc<Context>,
    input_tx: Sender<Option<String>>,
    output_rx: Mutex<Receiver<Option<String>>>,
    _thread: JoinHandle<()>,
}

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

struct Sentences;

impl Sentences {
    pub fn new(
        _context: Arc<Context>,
        _kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        Ok(Arc::new(Self))
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
        // let sentences = cg3::Output::new(input)
        //     .sentences()
        //     .collect::<Result<Vec<_>, _>>()
        //     .map_err(|e| Error(e.to_string()))?;
        let sentences = input
            .trim_end_matches('.')
            .split(".")
            .map(|x| x.trim().to_string())
            .collect::<Vec<_>>();
        Ok(sentences.into())
    }

    fn forward_stream(
        self: Arc<Self>,
        mut input: InputRx,
        mut output: InputTx,
        config: Arc<serde_json::Value>,
    ) -> tokio::task::JoinHandle<Result<(), Error>>
    where
        Self: Send + Sync + 'static,
    {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                let event = input.recv().await.map_err(|e| Error(e.to_string()))?;
                let this = this.clone();
                match event {
                    InputEvent::Input(input) => {
                        tracing::debug!("INPUT: {:?}", input);
                        let event = match this.forward(input, config.clone()).await {
                            Ok(event) => event,
                            Err(e) => {
                                output
                                    .send(InputEvent::Error(e.clone()))
                                    .map_err(|e| Error(e.to_string()))?;
                                return Err(e);
                            }
                        };
                        let x = event.try_into_string_array().unwrap();
                        for x in x {
                            tracing::debug!("SEND OUTPUT: {:?}", x);
                            output
                                .send(InputEvent::Input(Input::String(x)))
                                .map_err(|e| Error(e.to_string()))?;
                        }
                        output
                            .send(InputEvent::Finish)
                            .map_err(|e| Error(e.to_string()))?;
                    }
                    InputEvent::Finish => {
                        output
                            .send(InputEvent::Finish)
                            .map_err(|e| Error(e.to_string()))?;
                    }
                    InputEvent::Error(e) => {
                        output
                            .send(InputEvent::Error(e.clone()))
                            .map_err(|e| Error(e.to_string()))?;
                        return Err(e);
                    }
                    InputEvent::Close => {
                        output
                            .send(InputEvent::Close)
                            .map_err(|e| Error(e.to_string()))?;
                        break;
                    }
                }
            }
            Ok(())
        })
    }

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

pub struct Vislcg3 {
    _context: Arc<Context>,
    input_tx: Sender<Option<String>>,
    output_rx: Mutex<Receiver<Option<String>>>,
    _thread: JoinHandle<()>,
}

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

        let (input_tx, mut input_rx) = mpsc::channel(1);
        let (output_tx, output_rx) = mpsc::channel(1);

        let thread = std::thread::spawn(move || {
            let applicator = cg3::Applicator::new(&model_path);

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

pub struct ToJson {
    _context: Arc<Context>,
}

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
