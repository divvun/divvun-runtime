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

const BOSMARK: &str = "__DIVVUN_BOS__";
const EOSMARK: &str = "__DIVVUN_EOS__";

fn blanktag(analyzer: &hfst::Transducer, input: &str) -> String {
    let cg_output = Output::new(input);

    // Collect all blocks first so we can use windows
    let blocks: Vec<_> = cg_output.iter().filter_map(|block| block.ok()).collect();

    let mut preblank: Vec<String> = vec![BOSMARK.to_string()];
    let mut output = String::with_capacity(input.len());

    let mut i = 0;
    while i < blocks.len() {
        match &blocks[i] {
            cg3::Block::Cohort(cohort) => {
                // Look ahead to collect all blanks that come after this cohort
                let mut postblank: Vec<String> = Vec::new();
                let mut j = i + 1;
                while j < blocks.len() {
                    match &blocks[j] {
                        cg3::Block::Text(text) => {
                            postblank.push(text.to_string());
                            j += 1;
                        }
                        cg3::Block::Escaped(_escaped) => {
                            j += 1; // Ignore escaped blocks
                        }
                        cg3::Block::Cohort(_) => {
                            // Next cohort found, stop accumulating blanks
                            break;
                        }
                    }
                }

                // Build reading lines from the cohort structure
                let mut current_readings: Vec<String> = Vec::new();
                for reading in &cohort.readings {
                    let mut reading_line = format!("\t\"{}\"", reading.base_form);
                    for tag in &reading.tags {
                        reading_line.push(' ');
                        reading_line.push_str(tag);
                    }
                    current_readings.push(reading_line);
                }

                // Process this cohort with accumulated postblanks
                output.push_str(&process(
                    analyzer,
                    &preblank,
                    &cohort.word_form,
                    &postblank,
                    &current_readings,
                ));

                // Prepare preblank for next cohort
                std::mem::swap(&mut preblank, &mut postblank);

                // Skip past the blanks we've processed
                i = j;
            }
            cg3::Block::Text(_) => {
                // These should be handled by the lookahead above
                i += 1;
            }
            _ => {}
        }
    }

    // Process final empty cohort to output any remaining blanks
    let empty_readings: Vec<String> = Vec::new();
    let final_postblank = vec![EOSMARK.to_string()];
    output.push_str(&process(
        analyzer,
        &preblank,
        "",
        &final_postblank,
        &empty_readings,
    ));

    output
}

fn process(
    analyzer: &hfst::Transducer,
    preblank: &[String],
    wf: &str,
    postblank: &[String],
    readings: &[String],
) -> String {
    let mut ret = String::new();

    // Output preblank lines (like C++ lines 36-40)
    for b in preblank.iter() {
        if b != BOSMARK && b != EOSMARK {
            ret.push_str(":");
            ret.push_str(b);
            ret.push('\n');
        }
    }

    // If no wordform, just return the blanks (like C++ lines 41-43)
    if wf.is_empty() {
        return ret;
    }

    // Analyze the combination of preblank + wordform + postblank (like C++ line 45)
    // The analyzer expects wordforms with angle brackets like <,> to match its patterns
    let analysis_input = format!("{}\"<{}>\"{}", preblank.join(""), wf, postblank.join(""));

    tracing::debug!("Analyzing input: {:?}", analysis_input);

    let tags = analyzer.lookup_tags(&analysis_input, false);
    tracing::debug!("Found tags: {:?}", tags);

    // Output the wordform line with angle brackets (like C++ line 57)
    ret.push_str("\"<");
    ret.push_str(wf);
    ret.push_str(">\"\n");

    // Output each reading with appended tags (like C++ lines 58-65)
    for reading in readings.iter() {
        if reading.starts_with(';') {
            // Traced reading, don't touch (like C++ lines 59-61)
            ret.push_str(reading);
            ret.push('\n');
        } else {
            // Regular reading, append tags (like C++ lines 62-64)
            ret.push_str(reading);
            for tag in &tags {
                ret.push(' ');
                ret.push_str(tag);
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
