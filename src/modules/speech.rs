use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use tokio::sync::{mpsc, Mutex};

use super::{Context, InputFut};

static CELL: OnceLock<(
    mpsc::Sender<String>,
    Mutex<mpsc::Receiver<Vec<u8>>>,
    std::thread::JoinHandle<()>,
)> = OnceLock::new();

pub async fn tts(
    context: Arc<Context>,
    input: InputFut<String>,
    voice_model: PathBuf,
    hifigen_model: PathBuf,
) -> anyhow::Result<Vec<u8>> {
    let input = input.await?;

    use pyo3::prelude::*;
    use pyo3::types::IntoPyDict;

    let voice_model = context.path.join(voice_model);
    let hifigen_model = context.path.join(hifigen_model);

    println!("Hi");
    let (input_tx, output_rx, _thread) = CELL.get_or_init(|| {
        let (input_tx, mut input_rx) = mpsc::channel(1);
        let (output_tx, output_rx) = mpsc::channel(1);

        let thread = std::thread::spawn(move || {
            println!("Thread");
            let venv_path = std::env::var("VIRTUAL_ENV").unwrap();

            let _lol: PyResult<()> = Python::with_gil(|py| {
                let sys = py.import("sys")?;
                let version_info = sys.getattr("version_info")?;
                
                let py_ver_major: u32 = version_info.getattr("major")?.extract()?;
                let py_ver_minor: u32 = version_info.getattr("minor")?.extract()?;

                let venv_path = PathBuf::from(venv_path).join(format!("lib/python{}.{}/site-packages", py_ver_major, py_ver_minor));
                py.eval(&format!("sys.path.append({:?})", venv_path), None, Some(&[("sys", sys)].into_py_dict(py)))?;

                let code = format!(
                    r#"divvun_speech.Synthesizer("cpu", {:?}, {:?})"#,
                    voice_model.to_string_lossy(),
                    hifigen_model.to_string_lossy()
                );

                let locals = [("divvun_speech", py.import("divvun_speech")?)].into_py_dict(py);
                let syn = py.eval(&code, None, Some(&locals))?;

                loop {
                    let Some(input) = input_rx.blocking_recv() else {
                        break;
                    };

                    println!("Input: {}", input);
                    let code = format!("syn.speak({input:?})");

                    println!("Doing eval");
                    let wav_bytes: Vec<u8> =
                        py.eval(&code, None, Some(&[("syn", syn)].into_py_dict(py)))?.extract().unwrap();

                    println!("Did eval");
                    output_tx.blocking_send(wav_bytes).unwrap();
                }

                println!("BYE");
                Ok(())
            });

            eprintln!("{:?}", _lol);
        });

        (input_tx, Mutex::new(output_rx), thread)
    });

    input_tx.send(input).await.unwrap();
    let mut output_rx = output_rx.lock().await;
    let value = output_rx.recv().await.unwrap();

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn it_works() {
        let bytes = tts(
            Arc::new(Context {
                path: PathBuf::from("/"),
            }),
            Box::pin(async { Ok("Hello, world!".to_string()) }),
            PathBuf::from("/Users/brendan/git/divvun/divvun-speech-py/voice_female.pt"),
            PathBuf::from("/Users/brendan/git/divvun/divvun-speech-py/hifigan.pt"),
        )
        .await
        .unwrap();

        println!("{:?}", bytes.len());
    }
}
