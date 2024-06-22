use std::{collections::HashMap, sync::Arc, thread::JoinHandle};

use async_trait::async_trait;

use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};

use crate::{
    ast,
    modules::{Error, SharedInputFut},
};

use super::super::{cg3::CG_LINE, CommandRunner, Context, Input};

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
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, Error> {
        let model_path = kwargs
            .remove("model_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("model_path missing".to_string()))?;

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
            if let Some(_) = m.get(2) {
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

    let tags = analyzer.lookup_tags(
        &format!("{}{}{}", preblank.join(""), wf, postblank.join("")),
        true,
    );

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

#[async_trait]
impl CommandRunner for Blanktag {
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
        let output = output_rx.recv().await.expect("output rx recv");

        Ok(output.unwrap_or_else(|| "".to_string()).into())
    }

    fn name(&self) -> &'static str {
        "divvun::blanktag"
    }
}
