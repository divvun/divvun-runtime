use std::{collections::HashMap, fmt::Write as _, sync::Arc};

use async_trait::async_trait;
use cg3::Block;
use divvun_fst::{
    speller::{HfstSpeller, Speller, suggestion::Suggestion},
    transducer::{TransducerLoader, hfst::HfstTransducer},
    vfs::Fs,
};
use divvun_runtime_macros::{rt_command, rt_struct};
use rayon::iter::{IntoParallelRefIterator as _, ParallelIterator as _};
use serde::{Deserialize, Serialize};

use crate::{ast, modules::Error};

use super::super::{CommandRunner, Context, Input};

/// CG3-integrated spelling checker
#[derive(facet::Facet)]
pub struct Cgspell {
    #[facet(opaque)]
    _context: Arc<Context>,
    #[facet(opaque)]
    speller: Arc<dyn Speller + Send + Sync>,
    #[facet(opaque)]
    analyzer: Arc<dyn Speller + Send + Sync>,
    #[facet(opaque)]
    config: Option<divvun_fst::speller::SpellerConfig>,
}

/// configurable extra penalties for edit distance
#[rt_struct(module = "divvun")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReweightingConfig {
    start_penalty: f32,
    end_penalty: f32,
    mid_penalty: f32,
}

/// finetuning configuration of the spelling correction algorithms
#[rt_struct(module = "divvun")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpellerConfig {
    /// upper limit for suggestions given
    #[serde(default)]
    pub n_best: Option<usize>,
    /// upper limit for weight of any suggestion
    #[serde(default)]
    pub max_weight: Option<f64>,
    /// weight distance between best suggestion and worst
    #[serde(default)]
    pub beam: Option<f64>,
    /// extra penalties for different edit distance type errors
    #[serde(default)]
    pub reweight: Option<ReweightingConfig>,
    /// some parallel stuff?
    #[serde(default)]
    pub node_pool_size: usize,
    /// used when suggesting unfinished word parts
    #[serde(default)]
    pub continuation_marker: Option<String>,
    /// whether we try to recase mispelt word before other suggestions
    #[serde(default)]
    pub recase: bool,
}

impl TryFrom<divvun_fst::speller::SpellerConfig> for SpellerConfig {
    type Error = serde_json::Error;

    fn try_from(value: divvun_fst::speller::SpellerConfig) -> Result<Self, Self::Error> {
        let json = serde_json::to_value(value)?;
        let config: SpellerConfig = serde_json::from_value(json)?;
        Ok(config)
    }
}

#[rt_command(
    module = "divvun",
    name = "cgspell",
    input = [String],
    output = "String",
    kind = "cg3",
    args = [err_model_path = "Path", acc_model_path = "Path", config? = "SpellerConfig"]
)]
impl Cgspell {
    pub async fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, Error> {
        let acc_model_path = kwargs
            .remove("acc_model_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| {
                Error::msg("acc_model_path missing").at("pipeline.json", "/args/acc_model_path")
            })?;
        let err_model_path = kwargs
            .remove("err_model_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| {
                Error::msg("err_model_path missing").at("pipeline.json", "/args/err_model_path")
            })?;
        let config = match kwargs
            .remove("config")
            .and_then(|x| x.value)
            .map(|x| x.try_as_json())
        {
            Some(Ok(c)) => {
                let config: divvun_fst::speller::SpellerConfig = serde_json::from_value(c)
                    .map_err(|e| {
                        Error::msg(format!("config arg is not valid SpellerConfig: {}", e))
                            .at("pipeline.json", "/args/config")
                    })?;
                Some(config)
            }
            Some(Err(e)) => {
                return Err(Error::msg(format!("config arg is not valid JSON: {}", e))
                    .at("pipeline.json", "/args/config"));
            }
            None => None,
        };

        let acc_model_path = context.extract_to_temp_dir(acc_model_path).await?;
        let err_model_path = context.extract_to_temp_dir(err_model_path).await?;

        let lexicon = HfstTransducer::from_path(&Fs, acc_model_path).unwrap();
        let mutator = HfstTransducer::from_path(&Fs, err_model_path).unwrap();
        let speller = HfstSpeller::new(mutator, lexicon);

        Ok(Arc::new(Self {
            _context: context,
            analyzer: speller.clone(),
            speller,
            config,
        }) as _)
    }
}

fn do_cgspell(
    speller: Arc<dyn Speller + Sync + Send>,
    analyzer: Arc<dyn Speller + Sync + Send>,
    word: &str,
    config: Option<&divvun_fst::speller::SpellerConfig>,
) -> String {
    tracing::debug!("cgspell processing word: {}", word);
    let suggestions = match config {
        Some(cfg) => speller.clone().suggest_with_config(word, cfg),
        None => speller.clone().suggest(word),
    };

    tracing::debug!(
        "speller.suggest('{}') returned {} suggestions",
        word,
        suggestions.len()
    );

    suggestions
        .par_iter()
        .map(|sugg| {
            let analyses = analyzer.clone().analyze_output(&sugg.value);
            tracing::debug!(
                "  suggestion '{}' (weight: {}) -> {} analyses",
                sugg.value,
                sugg.weight,
                analyses.len()
            );
            print_readings(&analyses, &sugg.value, sugg.weight.0)
        })
        .collect::<Vec<String>>()
        .join("")
}

fn print_readings(analyses: &[Suggestion], form: &str, weight: f32) -> String {
    let mut ret = String::new();

    for analysis in analyses {
        let segments: Vec<&str> = analysis.value.split('#').collect();

        for (idx_from_end, segment) in segments.iter().rev().enumerate() {
            let depth = idx_from_end + 1;
            let mut chunks = segment.split_ascii_whitespace();
            let Some(lemma) = chunks.next() else {
                continue;
            };

            ret.push_str(&"\t".repeat(depth));
            ret.push('"');
            ret.push_str(lemma);
            ret.push('"');
            for chunk in chunks {
                ret.push(' ');
                ret.push_str(chunk);
            }
            if depth == 1 {
                write!(
                    &mut ret,
                    " <W:{}> <WA:{}> <spelled> \"{}\"S",
                    weight, analysis.weight, form
                )
                .unwrap();
            }
            ret.push('\n');
        }
    }

    ret
}

#[async_trait]
impl CommandRunner for Cgspell {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.try_into_string()?;
        let output = cg3::Output::new(&input);
        let mut out = String::new();

        for thing in output.clone().iter() {
            let thing = thing.map_err(Error::wrap)?;

            match thing {
                Block::Cohort(c) => {
                    writeln!(&mut out, "\"<{}>\"", c.word_form).map_err(Error::wrap)?;

                    let is_unknown = c
                        .readings
                        .iter()
                        .any(|x| x.tags.contains(&"+?") || x.tags.contains(&"?"));

                    if !is_unknown {
                        c.readings
                            .iter()
                            .map(|x| {
                                format!(
                                    "{}\"{}\" {}\n",
                                    "\t".repeat(x.depth),
                                    x.base_form,
                                    x.tags.join(" ")
                                )
                            })
                            .for_each(|x| out.push_str(&x));
                    } else {
                        out.push_str(&do_cgspell(
                            self.speller.clone(),
                            self.analyzer.clone(),
                            c.word_form,
                            self.config.as_ref(),
                        ));
                    }
                }
                Block::Escaped(x) => {
                    out.push(':');
                    out.push_str(&x);
                }
                Block::Text(x) => {
                    out.push_str(&x);
                }
            }
            out.push('\n');
        }

        Ok(out.into())
    }

    fn name(&self) -> &'static str {
        "divvun::cgspell"
    }
}
