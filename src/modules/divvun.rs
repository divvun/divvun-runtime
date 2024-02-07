use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc, thread::JoinHandle};

use async_trait::async_trait;
use tokio::{
    io::AsyncWriteExt,
    sync::{
        mpsc::{self, Receiver, Sender},
        Mutex,
    },
};

use crate::{
    ast,
    modules::{Arg, Command, Module, Ty},
};

use super::{cg3::CG_LINE, CommandRunner, Context, Input, InputFut};

inventory::submit! {
    Module {
        name: "divvun",
        commands: &[
            Command {
                name: "blanktag",
                args: &[Arg { name: "model_path", ty: Ty::Path }],
                init: Blanktag::new,
            },
            Command {
                name: "cgspell",
                args: &[
                    Arg {name: "err_model_path", ty: Ty::Path },
                    Arg {name: "acc_model_path", ty: Ty::Path },
                ],
                init: Cgspell::new,
            },
            Command {
                name: "suggest",
                args: &[
                    Arg {name: "model_path", ty: Ty::Path },
                    Arg {name: "error_xml_path", ty: Ty::Path },
                ],
                init: Suggest::new,
            }
        ]
    }
}

pub struct Blanktag {
    _context: Arc<Context>,
    input_tx: Sender<Option<String>>,
    output_rx: Mutex<Receiver<Option<String>>>,
    _thread: JoinHandle<()>,
}

impl Blanktag {
    pub fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner>, anyhow::Error> {
        let model_path = kwargs
            .remove("model_path")
            .and_then(|x| x.value)
            .ok_or_else(|| anyhow::anyhow!("model_path missing"))?;

        let model_path = context.extract_to_temp_dir(model_path)?;

        let (input_tx, mut input_rx) = mpsc::channel(1);
        let (output_tx, output_rx) = mpsc::channel(1);

        let thread = std::thread::spawn(move || {
            let analyzer = hfst::Transducer::new(model_path);

            loop {
                let Some(Some(input)): Option<Option<String>> = input_rx.blocking_recv() else {
                    break;
                };

                output_tx
                    .blocking_send(Some(blanktag(&analyzer, &input)))
                    .unwrap();
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

const BOSMARK: &str = "__DIVVUN_BOS__";
const EOSMARK: &str = "__DIVVUN_EOS__";

fn blanktag(analyzer: &hfst::Transducer, input: &str) -> String {
    let mut preblank: Vec<&str> = Vec::new();
    let mut postblank: Vec<&str> = vec![BOSMARK];
    let mut wf: &str = "";
    let mut readings: Vec<&str> = Vec::new();

    let mut output = String::with_capacity(input.len());

    for line in input.lines() {
        let matches = CG_LINE.captures_iter(line).collect::<Vec<_>>();

        if matches.is_empty() {
            output.push_str(line);
            output.push('\n');
            continue;
        }

        for m in matches {
            if let Some(x) = m.get(2) {
                output.push_str(&process(analyzer, &preblank, &wf, &postblank, &readings));
                std::mem::swap(&mut preblank, &mut postblank);
                wf = m.get(1).map(|x| x.as_str()).unwrap_or_default();
                readings.clear();
                postblank.clear();
            } else if m.get(3).map(|x| !x.is_empty()).unwrap_or(false)
                || m.get(8).map(|x| !x.is_empty()).unwrap_or(false)
            {
                readings.push(line);
            } else if m.get(7).map(|x| !x.is_empty()).unwrap_or(false) {
                output.push_str(&process(analyzer, &preblank, &wf, &postblank, &readings));
                std::mem::swap(&mut preblank, &mut postblank);
                wf = "";
                readings.clear();
                postblank.clear();
                output.push_str(&process(analyzer, &preblank, &wf, &postblank, &readings));
                preblank.clear();
                output.push_str(line);
                output.push('\n');
            } else if let Some(value) = m.get(6).filter(|x| !x.is_empty()).map(|x| x.as_str()) {
                postblank.push(value);
            }
        }
    }

    postblank.push(EOSMARK);
    output.push_str(&process(analyzer, &preblank, &wf, &postblank, &readings));
    std::mem::swap(&mut preblank, &mut postblank);
    wf = "";
    readings.clear();
    postblank.clear();
    output.push_str(&process(analyzer, &preblank, &wf, &postblank, &readings));

    output
}

fn process(
    analyzer: &hfst::Transducer,
    preblank: &[&str],
    wf: &str,
    postblank: &[&str],
    readings: &[&str],
) -> String {
    let mut ret = String::new();

    for b in preblank.iter() {
        if *b != BOSMARK && *b != EOSMARK {
            ret.push_str(":");
            ret.push_str(b);
            ret.push_str("\n");
        }
    }

    if wf.is_empty() {
        return ret;
    }

    let tags = analyzer.lookup_tags(&format!(
        "{}{}{}",
        preblank.join(""),
        wf,
        postblank.join("")
    ));

    ret.push_str(wf);
    ret.push('\n');

    for r in readings.iter() {
        if r.chars().next() == Some(';') {
            ret.push_str(r);
            ret.push('\n');
        } else {
            ret.push_str(r);
            for t in tags.iter() {
                ret.push(' ');
                ret.push_str(t);
            }
            ret.push('\n');
        }
    }

    ret
}

#[async_trait(?Send)]
impl CommandRunner for Blanktag {
    async fn forward(self: Arc<Self>, input: InputFut) -> Result<Input, anyhow::Error> {
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
        "divvun::blanktag"
    }
}

pub struct Cgspell {
    _context: Arc<Context>,
    acc_model_path: PathBuf,
    err_model_path: PathBuf,
}

impl Cgspell {
    pub fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner>, anyhow::Error> {
        let acc_model_path = kwargs
            .remove("acc_model_path")
            .and_then(|x| x.value)
            .ok_or_else(|| anyhow::anyhow!("acc_model_path missing"))?;
        let err_model_path = kwargs
            .remove("err_model_path")
            .and_then(|x| x.value)
            .ok_or_else(|| anyhow::anyhow!("err_model_path missing"))?;

        let acc_model_path = context.extract_to_temp_dir(acc_model_path)?;
        let err_model_path = context.extract_to_temp_dir(err_model_path)?;

        Ok(Arc::new(Self {
            _context: context,
            acc_model_path,
            err_model_path,
        }) as _)
    }
}

#[async_trait(?Send)]
impl CommandRunner for Cgspell {
    async fn forward(self: Arc<Self>, input: InputFut) -> Result<Input, anyhow::Error> {
        let input = input.await?.try_into_string()?;

        let mut child = tokio::process::Command::new("divvun-cgspell")
            .arg(&self.err_model_path)
            .arg(&self.acc_model_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| {
                eprintln!("divvun-cgspell ({}): {e:?}", self.acc_model_path.display());
                e
            })?;

        let mut stdin = child.stdin.take().unwrap();
        tokio::spawn(async move {
            stdin.write_all(input.as_bytes()).await.unwrap();
        });

        let output = child.wait_with_output().await?;
        if !output.status.success() {
            anyhow::bail!("Error")
        }

        let output = String::from_utf8(output.stdout)?;
        Ok(output.into())
    }

    fn name(&self) -> &'static str {
        "divvun::cgspell"
    }
}

pub struct Suggest {
    _context: Arc<Context>,
    model_path: PathBuf,
    error_xml_path: PathBuf,
}

impl Suggest {
    pub fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner>, anyhow::Error> {
        let model_path = kwargs
            .remove("model_path")
            .and_then(|x| x.value)
            .ok_or_else(|| anyhow::anyhow!("model_path missing"))?;
        let error_xml_path = kwargs
            .remove("error_xml_path")
            .and_then(|x| x.value)
            .ok_or_else(|| anyhow::anyhow!("error_xml_path missing"))?;

        let model_path = context.extract_to_temp_dir(model_path)?;
        let error_xml_path = context.extract_to_temp_dir(error_xml_path)?;

        Ok(Arc::new(Self {
            _context: context,
            model_path,
            error_xml_path,
        }) as _)
    }
}

#[async_trait(?Send)]
impl CommandRunner for Suggest {
    async fn forward(self: Arc<Self>, input: InputFut) -> Result<Input, anyhow::Error> {
        let input = input.await?.try_into_string()?;

        let mut child = tokio::process::Command::new("divvun-suggest")
            .arg("--json")
            .arg(&self.model_path)
            .arg(&self.error_xml_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| {
                eprintln!("suggest ({}): {e:?}", self.model_path.display());
                e
            })?;

        let mut stdin = child.stdin.take().unwrap();
        tokio::spawn(async move {
            stdin.write_all(input.as_bytes()).await.unwrap();
        });

        let output = child.wait_with_output().await?;

        let output = String::from_utf8(output.stdout)?;
        Ok(output.into())
    }
    fn name(&self) -> &'static str {
        "divvun::suggest"
    }
}
