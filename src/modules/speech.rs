use std::{
    collections::HashMap,
    fs::create_dir_all,
    sync::{Arc, OnceLock},
    thread::JoinHandle,
};

use async_trait::async_trait;
use divvun_speech::{Device, DivvunSpeech, Options, SymbolSet};
use memmap2::Mmap;
use pathos::AppDirs;
use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};

use crate::{
    ast,
    modules::{Arg, Command, Error, Module, Ty},
};

use super::{CommandRunner, Context, Input, SharedInputFut};

pub static CELL: OnceLock<(
    mpsc::Sender<Option<String>>,
    Mutex<mpsc::Receiver<Vec<u8>>>,
    std::thread::JoinHandle<()>,
)> = OnceLock::new();

inventory::submit! {
    Module {
        name: "speech",
        commands: &[
            Command {
                name: "tts",
                input: &[Ty::String],
                args: &[
                    Arg {
                        name: "voice_model",
                        ty: Ty::Path
                    },
                    Arg {
                        name: "univnet_model",
                        ty: Ty::Path
                    },
                    Arg {
                        name: "speaker",
                        ty: Ty::Int
                    },
                    Arg {
                        name: "alphabet",
                        ty: Ty::String,
                    }
                ],
                init: Tts::new,
                returns: Ty::Bytes,
            }
        ]
    }
}

struct Tts {
    voice_model: Mmap,
    vocoder_model: Mmap,
    speaker: i32,
    speech: DivvunSpeech<'static>,
}

impl Tts {
    pub fn new(
        context: Arc<Context>,
        kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        let voice_model = kwargs
            .get("voice_model")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("Missing voice_model".to_string()))?;
        let univnet_model = kwargs
            .get("univnet_model")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("Missing univnet_model".to_string()))?;
        let speaker = kwargs
            .get("speaker")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_int())
            .map(|x| x as i32)
            .ok_or_else(|| Error("Missing speaker".to_string()))?;
        let alphabet = kwargs
            .get("alphabet")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("Missing alphabet".to_string()))?;

        let voice_model = context.memory_map_file(voice_model)?;
        let vocoder_model = context.memory_map_file(univnet_model)?;

        let speech = unsafe {
            DivvunSpeech::from_memory_map(
                &voice_model,
                &vocoder_model,
                match &*alphabet {
                    "sme" => divvun_speech::SME_EXPANDED,
                    "smj" => divvun_speech::SMJ_EXPANDED,
                    "sma" => divvun_speech::SMA_EXPANDED,
                    other => return Err(Error(format!("Unknown alphabet: {other}"))),
                },
                Device::Cpu,
            )
        }.map_err(|e| Error(e.to_string()))?;

        Ok(Arc::new(Self {
            voice_model,
            vocoder_model,
            speaker,
            speech,
        }))
    }
}

#[async_trait]
impl CommandRunner for Tts {
    async fn forward(
        self: Arc<Self>,
        input: SharedInputFut,
        config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.await?.try_into_string()?;
        let speaker = config
            .get("speaker")
            .and_then(|x| x.as_i64())
            .map(|x| x as i32)
            .unwrap_or(self.speaker);

        let this = self.clone();
        let value = tokio::task::spawn_blocking(move ||  {
            let tensor = this.speech.forward(&input, Options {
                pace: 1.05,
                speaker,
            }).map_err(|e| Error(e.to_string()))?;

            DivvunSpeech::generate_wav(tensor).map_err(|e| Error(e.to_string()))
        }).await.map_err(|e| Error(e.to_string()))??;

        Ok(value.into())
    }

    fn name(&self) -> &'static str {
        "speech::tts"
    }
}
