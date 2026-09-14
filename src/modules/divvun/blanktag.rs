use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use divvun_runtime_macros::rt_command;
use hfst::hfst_transducer::AnyTransducer;

use crate::{ast, modules::Error};

use super::super::{CommandRunner, Context, PipelineValue, PipelineValues};
use crate::modules::cg3::{self, Output};

/// Blank tag annotator for CG3 streams
#[derive(facet::Facet)]
pub struct Blanktag {
    #[facet(opaque)]
    _context: Arc<Context>,
    /// The whitespace FST, shared across every instance loading the same file.
    /// Lookups take `&self` with per-call scratch, so this command needs no
    /// worker thread: forward() runs the work wherever it is called from, and
    /// concurrent callers do not serialize behind each other.
    #[facet(opaque)]
    analyzer: Arc<AnyTransducer>,
}

#[rt_command(
    module = "divvun",
    name = "blanktag",
    input = [String],
    output = "String",
    kind = "cg3",
    args = [model_path = "Path"]
)]
impl Blanktag {
    pub async fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, Error> {
        let model_path = kwargs
            .remove("model_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| {
                Error::msg("model_path missing").at("pipeline.json", "/args/model_path")
            })?;

        let analyzer = crate::modules::hfst::load_lookup(&context, &model_path).await?;

        Ok(Arc::new(Self {
            _context: context,
            analyzer,
        }) as _)
    }
}

const BOS: &str = "__DIVVUN_BOS__";
const EOS: &str = "__DIVVUN_EOS__";

const BOSMARK: cg3::Block<'static> = cg3::Block::Text(BOS);
const EOSMARK: cg3::Block<'static> = cg3::Block::Text(EOS);

/// Emit buffered blanks, dropping the BOS/EOS markers the whitespace FST needs
/// but the stream must never see. `Text` is not part of the stream, so it is
/// marked with `;`; everything else goes out as it came in.
fn emit_blanks(output: &mut String, blocks: &[cg3::Block]) {
    for block in blocks {
        match block {
            cg3::Block::Text(BOS | EOS) => {}
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
            cg3::Block::StreamCmd(c) => {
                output.push_str(c);
                output.push('\n');
            }
            cg3::Block::Cohort(_) => {}
        }
    }
}

fn blanktag(analyzer: &AnyTransducer, input: &str) -> String {
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
                    emit_blanks(&mut output, &preblank);

                    output.push_str(&process_cohort(analyzer, &preblank, &postblank, &c));

                    std::mem::swap(&mut preblank, &mut postblank);
                    postblank.clear();

                    tracing::debug!("after cohort: pre:{:?} post:{:?}", preblank, postblank);
                }

                cur_cohort = Some(cohort);
            }
            cg3::Block::Text(x) | cg3::Block::Escaped(x) | cg3::Block::StreamCmd(x) => {
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

    emit_blanks(&mut output, &preblank);

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
        emit_blanks(&mut output, &postblank);
    }

    output
}

fn process_cohort(
    analyzer: &AnyTransducer,
    preblank: &[cg3::Block],
    postblank: &[cg3::Block],
    cohort: &cg3::Cohort,
) -> String {
    let mut ret = String::new();

    if cohort.word_form.is_empty() {
        return ret;
    }

    // Include the BOS/EOS markers (Text blocks) as well as real superblanks
    // (Escaped blocks) in the FST lookup, matching libdivvun's blanktag. The
    // whitespace FST recognises __DIVVUN_BOS__/__DIVVUN_EOS__, which lets it tell
    // a sentence-initial token (e.g. a leading "(") from a token with a genuinely
    // missing space before it. They are still stripped from the output (#18).
    let preblank_text = preblank
        .iter()
        .filter_map(|x| match x {
            cg3::Block::Escaped(t) | cg3::Block::Text(t) => Some(*t),
            _ => None,
        })
        .collect::<Vec<_>>();
    let postblank_text = postblank
        .iter()
        .filter_map(|x| match x {
            cg3::Block::Escaped(t) | cg3::Block::Text(t) => Some(*t),
            _ => None,
        })
        .collect::<Vec<_>>();

    let lookup_string = format!(
        "{}\"<{}>\"{}",
        preblank_text.join(""),
        cohort.word_form,
        postblank_text.join("")
    );
    let tags = crate::modules::hfst::lookup_tags(analyzer, &lookup_string, false);
    let other_tags = crate::modules::hfst::lookup_tags(analyzer, &lookup_string, true);

    tracing::debug!("lookup_string: {:?}", lookup_string);
    tracing::debug!("tags: {:?}", tags);
    tracing::debug!("other_tags: {:?}", other_tags);

    ret.push_str("\"<");
    ret.push_str(&cohort.word_form);
    ret.push_str(">\"\n");

    for reading in &cohort.readings {
        // A --trace removed reading is passed through untouched rather than
        // enhanced, matching libdivvun's blanktag (blanktag.cpp:90). Display
        // re-emits its `;` prefix; rebuilding the line by hand below would not.
        if reading.removed {
            ret.push_str(&reading.to_string());
            ret.push('\n');
            continue;
        }

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
        input: PipelineValue,
        _config: Arc<serde_json::Value>,
    ) -> Result<PipelineValues, crate::modules::Error> {
        let input = input.try_into_string()?;

        // CPU-bound FST walking; keep it off the async threads.
        let analyzer = Arc::clone(&self.analyzer);
        let output = tokio::task::spawn_blocking(move || blanktag(&analyzer, &input))
            .await
            .map_err(|e| Error::msg(format!("divvun::blanktag: {e}")))?;

        Ok(output.into())
    }

    fn name(&self) -> &'static str {
        "divvun::blanktag"
    }
}
