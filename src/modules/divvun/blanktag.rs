use std::{collections::HashMap, sync::Arc, thread::JoinHandle};

use async_trait::async_trait;
use divvun_runtime_macros::rt_command;

use tokio::sync::{
    Mutex,
    mpsc::{self, Receiver, Sender},
};

use crate::{
    ast,
    modules::{Error, SharedInputFut},
};

use super::super::{CommandRunner, Context, Input};
use ::cg3::Output;

pub struct Blanktag {
    _context: Arc<Context>,
    input_tx: Sender<Option<String>>,
    output_rx: Mutex<Receiver<Option<String>>>,
    _thread: JoinHandle<()>,
}

#[rt_command(
    module = "divvun",
    name = "blanktag",
    input = [String],
    output = "String",
    args = [model_path = "Path"]
)]
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

const BOSMARK: cg3::Block<'static> = cg3::Block::Text("__DIVVUN_BOS__");
const EOSMARK: cg3::Block<'static> = cg3::Block::Text("__DIVVUN_EOS__");

fn blanktag(analyzer: &hfst::Transducer, input: &str) -> String {
    let cg_output = Output::new(input);
    let mut output = String::new();
    let mut preblank: Vec<cg3::Block> = vec![BOSMARK];
    let mut postblank: Vec<cg3::Block> = vec![];
    let mut cur_cohort = None;

    for block in cg_output.iter() {
        let block = match block {
            Ok(block) => block,
            Err(_) => continue,
        };

        match block {
            cg3::Block::Cohort(cohort) => {
                if let Some(c) = cur_cohort.take() {
                    let preblank_out = preblank
                        .iter()
                        .filter_map(|x| match x {
                            cg3::Block::Text("__DIVVUN_BOS__")
                            | cg3::Block::Text("__DIVVUN_EOS__") => None,
                            _ => Some(x),
                        })
                        .collect::<Vec<_>>();

                    for p in preblank_out {
                        match p {
                            cg3::Block::Text(t) => {
                                output.push(';');
                                output.push_str(t);
                                output.push('\n');
                            }
                            cg3::Block::Escaped(e) => {
                                output.push(':');
                                output.push_str(e);
                                output.push('\n');
                            }
                            _ => {}
                        }
                    }

                    output.push_str(&process_cohort(analyzer, &preblank, &postblank, &c));

                    std::mem::swap(&mut preblank, &mut postblank);
                    postblank.clear();

                    tracing::debug!("after cohort: pre:{:?} post:{:?}", preblank, postblank);
                }

                cur_cohort = Some(cohort);
            }
            cg3::Block::Text(x) | cg3::Block::Escaped(x) => {
                if cur_cohort.is_none() {
                    tracing::debug!("preblank: {:?}", x);
                    preblank.push(block);
                } else {
                    tracing::debug!("postblank: {:?}", x);
                    postblank.push(block);
                }
            }
        }
    }

    let preblank_out = preblank
        .iter()
        .filter_map(|x| match x {
            cg3::Block::Text("__DIVVUN_BOS__") | cg3::Block::Text("__DIVVUN_EOS__") => None,
            _ => Some(x),
        })
        .collect::<Vec<_>>();

    for p in preblank_out {
        match p {
            cg3::Block::Text(t) => {
                output.push(';');
                output.push_str(t);
                output.push('\n');
            }
            cg3::Block::Escaped(e) => {
                output.push(':');
                output.push_str(e);
                output.push('\n');
            }
            _ => {}
        }
    }

    postblank.push(EOSMARK);

    output.push_str(&process_cohort(
        analyzer,
        &preblank,
        &postblank,
        &cur_cohort.take().unwrap_or_else(|| cg3::Cohort {
            word_form: "",
            readings: Vec::new(),
        }),
    ));

    if postblank.len() > 1 {
        let postblank_out = postblank
            .iter()
            .filter_map(|x| match x {
                cg3::Block::Text("__DIVVUN_BOS__") | cg3::Block::Text("__DIVVUN_EOS__") => None,
                _ => Some(x),
            })
            .collect::<Vec<_>>();

        for p in postblank_out {
            match p {
                cg3::Block::Text(t) => {
                    output.push(';');
                    output.push_str(t);
                    output.push('\n');
                }
                cg3::Block::Escaped(e) => {
                    output.push(':');
                    output.push_str(e);
                    output.push('\n');
                }
                _ => {}
            }
        }
    }

    output
}

fn process_cohort(
    analyzer: &hfst::Transducer,
    preblank: &[cg3::Block],
    _postblank: &[cg3::Block],
    cohort: &cg3::Cohort,
) -> String {
    let mut ret = String::new();

    if cohort.word_form.is_empty() {
        return ret;
    }

    let preblank_text = preblank
        .iter()
        .filter_map(|x| match x {
            cg3::Block::Text(t) => Some(*t),
            _ => None,
        })
        .collect::<Vec<_>>();
    // let postblank_text = postblank.iter().filter_map(|x| match x {
    //     cg3::Block::Text(t) => Some(*t),
    //     _ => None,
    // }).collect::<Vec<_>>();

    let lookup_string = format!("{}\"<{}>\"", preblank_text.join(""), cohort.word_form);
    let tags = analyzer.lookup_tags(&lookup_string, false);
    let other_tags = analyzer.lookup_tags(&lookup_string, true);

    tracing::debug!("lookup_string: {:?}", lookup_string);
    tracing::debug!("tags: {:?}", tags);
    tracing::debug!("other_tags: {:?}", other_tags);

    ret.push_str("\"<");
    ret.push_str(&cohort.word_form);
    ret.push_str(">\"\n");

    for reading in &cohort.readings {
        for _ in 0..reading.depth {
            ret.push('\t');
        }
        ret.push('"');
        ret.push_str(&reading.base_form);
        ret.push('"');

        for tag in &reading.tags {
            ret.push(' ');
            ret.push_str(tag);
        }

        for blanktag in &tags {
            ret.push(' ');
            ret.push_str(blanktag);
        }

        ret.push('\n');
    }

    ret
}

#[async_trait]
impl CommandRunner for Blanktag {
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
        "divvun::blanktag"
    }
}
