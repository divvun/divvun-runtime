use std::{
    collections::HashMap,
    fs::create_dir_all,
    sync::{Arc, OnceLock},
    thread::JoinHandle,
};

use async_trait::async_trait;
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
                        name: "hifigan_model",
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
    input_tx: Sender<Option<String>>,
    output_rx: Mutex<Receiver<Vec<u8>>>,
    _thread: JoinHandle<()>,
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
        let hifigan_model = kwargs
            .get("hifigan_model")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("Missing hifigan_model".to_string()))?;
        let univnet_model = kwargs
            .get("univnet_model")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("Missing univnet_model".to_string()))?;
        let speaker = kwargs
            .get("speaker")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_int())
            .ok_or_else(|| Error("Missing speaker".to_string()))?;
        let alphabet = kwargs
            .get("alphabet")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("Missing alphabet".to_string()))?;

        let voice_model = context.extract_to_temp_dir(voice_model)?;
        let hifigan_model = context.extract_to_temp_dir(hifigan_model)?;
        let univnet_model = context.extract_to_temp_dir(univnet_model)?;

        use pyo3::prelude::*;
        use pyo3::types::IntoPyDict;

        let (input_tx, mut input_rx) = mpsc::channel(1);
        let (output_tx, output_rx) = mpsc::channel(1);

        use pathos::UserDirs;
        let app_dirs = pathos::user::AppDirs::new("Divvun Runtime").unwrap();
        create_dir_all(app_dirs.data_dir()).unwrap();
        let venv_dir = app_dirs.data_dir().join("tts-venv");

        let thread = std::thread::spawn(move || {
            let py_res: PyResult<()> = Python::with_gil(|py| {
                let log = ::oslog::OsLog::new("nu.necessary.DivvunExtension", "category");
                let sys = py.import("sys")?;
                let os = py.import("os")?;

                log.error("INSIDE PY");

                // let syspath: &PyList = sys.getattr("path").unwrap().downcast().unwrap();
                // syspath
                //     .append(if cfg!(windows) {
                //         venv_dir.join("Lib").join("site-packages")
                //     } else {
                //         venv_dir
                //             .join("lib")
                //             .join("python3.11")
                //             .join("site-packages")
                //     })
                //     .unwrap();

                let locals = &[("sys", sys), ("os", os)].into_py_dict(py);

                // Suppress the logging spam
                if !std::env::var("DEBUG").is_ok() {
                    py.run(
                        r#"
f = open(os.devnull, 'w')
sys.stdout = f
sys.stderr = f           
sys.__stdout__ = f
sys.__stderr__ = f
                "#,
                        None,
                        Some(locals),
                    )?;
                }

                let path: Vec<String> = sys.getattr("path").unwrap().extract()?;
                log.error(&format!("PATH: {:?}", path));

                let code = format!(
                    r#"divvun_speech.Synthesizer("cpu", {:?}, {:?}, {:?}, speaker={}, alphabet={:?})"#,
                    voice_model.to_string_lossy(),
                    hifigan_model.to_string_lossy(),
                    univnet_model.to_string_lossy(),
                    speaker,
                    alphabet,
                );

                //                 log.error("import divvun speech");
                //                 let x = py.run(
                //                     r#"
                // import divvun_speech
                // "#,
                //                     None,
                //                     None,
                //                 );

                let ds_mod = match py.import("divvun_speech") {
                    Ok(v) => v,
                    Err(e) => {
                        log.error(&format!("ERROR: {:?}", e.value_bound(py).to_string()));
                        let tb = e.traceback_bound(py).unwrap();
                        let s = tb.format().unwrap();
                        log.error(&format!("{s}"));
                        // for (i, x) in s.lines().collect::<Vec<_>>().chunks(5).enumerate() {
                        // log.error(&format!("{i}: {}", x.join("\n")));
                        // }
                        panic!("LOL");
                    }
                };
                let locals = [("divvun_speech", ds_mod)].into_py_dict(py);
                log.error("Is it after the import?");
                let syn = py.eval(&code, None, Some(locals))?;

                log.error("syn done");
                let code = "syn.speak(\"\")".to_string();

                log.error("Attempting to init");
                // This forces the thread to init before getting a first message.
                let _ignored = py.eval(&code, None, Some([("syn", syn)].into_py_dict(py)));

                log.error("Speech initialised.");

                loop {
                    println!("In loop");

                    // let input_rx = input_rx.clone();
                    let msg = py.allow_threads(|| {
                        let Some(Some(input)): Option<Option<String>> = input_rx.blocking_recv()
                        else {
                            return None;
                        };
                        Some(input)
                    });

                    let Some(input) = msg else {
                        break;
                    };

                    // TODO: violently replace all known hidden spaces.
                    let input: String = input.replace('\u{00ad}', "");

                    let code = format!("syn.speak({input:?})");

                    // TODO: match all the errors, and grab the stacktrace

                    log.error("Eval time");
                    let result = match py.eval(&code, None, Some([("syn", syn)].into_py_dict(py))) {
                        Ok(v) => v,
                        Err(e) => {
                            log.error(&format!("MCPLS PY ERROR {:?}", e));
                            output_tx.blocking_send(vec![]).expect("blocking send");
                            continue;
                        }
                    };

                    let wav_bytes: Vec<u8> = result.extract().expect("wav bytes");

                    log.error("Sending");
                    output_tx.blocking_send(wav_bytes).expect("blocking send");
                }

                Ok(())
            });

            if let Err(e) = py_res {
                let log = ::oslog::OsLog::new("nu.necessary.DivvunExtension", "category");
                Python::with_gil(|py| {
                    log.error(&format!("ERROR: {:?}", e.value_bound(py).to_string()));
                    let tb = e.traceback_bound(py).unwrap();
                    let s = tb.format().unwrap();
                    log.error(&format!("{s}"));
                });

                panic!("python failed")
            }
        });

        Ok(Arc::new(Self {
            input_tx,
            output_rx: Mutex::new(output_rx),
            _thread: thread,
        }) as _)
    }
}

#[async_trait]
impl CommandRunner for Tts {
    async fn forward(
        self: Arc<Self>,
        input: SharedInputFut,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.await?.try_into_string()?;

        self.input_tx
            .send(Some(input))
            .await
            .expect("input tx send");
        let mut output_rx = self.output_rx.lock().await;
        let value = output_rx.recv().await.expect("output rx recv");

        eprintln!("Got value");
        Ok(value.into())
    }

    fn name(&self) -> &'static str {
        "speech::tts"
    }
}
