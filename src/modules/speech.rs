use std::{
    collections::HashMap,
    fs::create_dir_all,
    sync::{Arc, OnceLock},
    thread::JoinHandle,
};

use async_trait::async_trait;
use pathos::AppDirs;
use pyo3::{types::PyList, PyResult, Python};
use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};
use wav_io::{header::WavData, resample, writer};

use crate::{
    ast,
    modules::{Arg, Command, Error, Module, Ty},
    PYTHON,
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
            .and_then(|x| x.value.as_deref())
            .ok_or_else(|| Error("Missing voice_model".to_string()))?;
        let hifigan_model = kwargs
            .get("hifigan_model")
            .and_then(|x| x.value.as_deref())
            .ok_or_else(|| Error("Missing hifigan_model".to_string()))?;
        let univnet_model = kwargs
            .get("univnet_model")
            .and_then(|x| x.value.as_deref())
            .ok_or_else(|| Error("Missing univnet_model".to_string()))?;

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
                let sys = py.import("sys")?;
                let os = py.import("os")?;

                let syspath: &PyList = sys.getattr("path").unwrap().downcast().unwrap();
                syspath
                    .append(if cfg!(windows) {
                        venv_dir.join("Lib").join("site-packages")
                    } else {
                        venv_dir
                            .join("lib")
                            .join("python3.11")
                            .join("site-packages")
                    })
                    .unwrap();

                let locals = &[("sys", sys), ("os", os)].into_py_dict(py);

                // Suppress the logging spam
                if std::env::var("DEBUG").is_err() {
                    py.run(
                        r#"
f = open(os.devnull, 'w')
sys.stdout = f
sys.stderr = f           
                "#,
                        None,
                        Some(locals),
                    )?;
                }

                let path: Vec<String> = sys.getattr("path").unwrap().extract()?;
                // println!("{:?}", path);

                let code = format!(
                    r#"divvun_speech.Synthesizer("cpu", {:?}, {:?}, {:?})"#,
                    voice_model.to_string_lossy(),
                    hifigan_model.to_string_lossy(),
                    univnet_model.to_string_lossy()
                );

                let locals = [("divvun_speech", py.import("divvun_speech")?)].into_py_dict(py);
                let syn = py.eval(&code, None, Some(locals))?;

                let code = "syn.speak(\"\")".to_string();

                // This forces the thread to init before getting a first message.
                let _ignored = py.eval(&code, None, Some([("syn", syn)].into_py_dict(py)));

                eprintln!("Speech initialised.");

                loop {
                    eprintln!("In loop");
                    let Some(Some(input)): Option<Option<String>> = input_rx.blocking_recv() else {
                        break;
                    };
                    // TODO: violently replace all known hidden spaces.
                    let input: String = input.replace('\u{00ad}', "");

                    let code = format!("syn.speak({input:?})");

                    eprintln!("Eval time");
                    let result = match py.eval(&code, None, Some([("syn", syn)].into_py_dict(py))) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("MCPLS PY ERROR {:?}", e);
                            output_tx.blocking_send(vec![]).expect("blocking send");
                            continue;
                        }
                    };

                    let wav_bytes: Vec<u8> = result.extract().expect("wav bytes");

                    let mut reader = wav_io::reader::Reader::from_vec(wav_bytes).unwrap();

                    let header = reader.read_header().unwrap();
                    let samples = reader.get_samples_f32().unwrap();

                    let mut wav = WavData { header, samples };

                    wav.samples = wav_io::utils::mono_to_stereo(wav.samples);
                    wav.header.channels = 2;
                    wav.samples = resample::linear(
                        wav.samples,
                        wav.header.channels,
                        wav.header.sample_rate,
                        44100,
                    );
                    wav.header.sample_rate = 44100;

                    let wav_bytes = writer::to_bytes(&wav.header, &wav.samples).unwrap();

                    eprintln!("Sending");
                    output_tx.blocking_send(wav_bytes).expect("blocking send");
                }

                Ok(())
            });

            if let Err(e) = py_res {
                eprintln!("MCPLS: {:?}", e);
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
