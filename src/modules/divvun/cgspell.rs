use std::{collections::HashMap, fmt::Write as _, sync::Arc};

use async_trait::async_trait;
use cg3::Block;
use divvun_runtime_macros::{rt_command, rt_struct};
use divvunspell::{
    speller::{HfstSpeller, Speller, suggestion::Suggestion},
    transducer::{Transducer, hfst::HfstTransducer},
    vfs::Fs,
};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator as _, ParallelIterator as _};
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
    config: Option<divvunspell::speller::SpellerConfig>,
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

impl TryFrom<divvunspell::speller::SpellerConfig> for SpellerConfig {
    type Error = serde_json::Error;

    fn try_from(value: divvunspell::speller::SpellerConfig) -> Result<Self, Self::Error> {
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
    pub fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, Error> {
        let acc_model_path = kwargs
            .remove("acc_model_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("acc_model_path missing".to_string()))?;
        let err_model_path = kwargs
            .remove("err_model_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("err_model_path missing".to_string()))?;
        let config = match kwargs
            .remove("config")
            .and_then(|x| x.value)
            .map(|x| x.try_as_json())
        {
            Some(Ok(c)) => {
                let config: divvunspell::speller::SpellerConfig = serde_json::from_value(c)
                    .map_err(|e| Error(format!("config arg is not valid SpellerConfig: {}", e)))?;
                Some(config)
            }
            Some(Err(e)) => return Err(Error(format!("config arg is not valid JSON: {}", e))),
            None => None,
        };

        let acc_model_path = context.extract_to_temp_dir(acc_model_path)?;
        let err_model_path = context.extract_to_temp_dir(err_model_path)?;

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
    config: Option<&divvunspell::speller::SpellerConfig>,
) -> String {
    tracing::debug!("cgspell processing word: {}", word);
    let suggestions = match config.as_ref() {
        Some(cfg) => speller.clone().suggest_with_config(word, cfg),
        None => speller.clone().suggest(word),
    };

    tracing::debug!(
        "speller.suggest('{}') returned {} suggestions",
        word,
        suggestions.len()
    );

    for sugg in &suggestions {
        tracing::debug!("  suggestion: '{}' (weight: {})", sugg.value, sugg.weight);
    }

    let mut out = suggestions
        .par_iter()
        .map(|sugg| {
            let chunks = sugg.value.split('#').enumerate().collect::<Vec<_>>();

            chunks.into_par_iter().map(|(i, value)| {
                let form = value.split_ascii_whitespace().next().unwrap();
                tracing::debug!("  analyzing form: '{}'", form);
                let analyses = analyzer.clone().analyze_output(form);
                tracing::debug!("  analyses for '{}': {} results", form, analyses.len());
                ((value.trim_matches('#'), sugg.weight), analyses, i + 1)
            })
        })
        .flatten()
        .collect::<Vec<_>>();

    out.sort_by(|((_, a), _, _), ((_, b), _, _)| a.cmp(b));

    let out = out
        .into_iter()
        .map(|((sugg, weight), analysis, i)| {
            let result = print_readings(&analysis, sugg, weight.0, i);
            tracing::debug!("  print_readings result length: {}", result.len());
            result
        })
        .collect::<Vec<String>>()
        .join("\n");

    tracing::debug!("cgspell final output length for '{}': {}", word, out.len());
    out
}

fn print_readings(analyses: &Vec<Suggestion>, sugg: &str, weight: f32, _indent: usize) -> String {
    let form = sugg.split_ascii_whitespace().next().unwrap();
    let mut ret = String::new();
    if analyses.is_empty() {
        return ret;
    }

    for (analysis, analysis_weight, i) in analyses
        .iter()
        .map(|x| {
            x.value
                .split('#')
                .enumerate()
                .map(|(i, y)| (y, x.weight, i + 1))
        })
        .flatten()
    {
        ret.push_str(&"\t".repeat(i));
        ret.push('"');
        let mut chunks = analysis.split_ascii_whitespace();
        let Some(word_form) = chunks.next() else {
            // This can happen for reasons I do not know.
            continue;
        };
        ret.push_str(&word_form);
        ret.push('"');
        for chunk in chunks {
            ret.push(' ');
            ret.push_str(&chunk);
        }
        write!(
            &mut ret,
            " <spelled> <W:{}> <WA:{}> \"{}\"S\n",
            weight, analysis_weight, form
        )
        .unwrap();
    }
    ret.remove(ret.len() - 1);
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
            let thing = thing.map_err(|e| Error(e.to_string()))?;

            match thing {
                Block::Cohort(c) => {
                    writeln!(&mut out, "\"<{}>\"", c.word_form)
                        .map_err(|e| Error(e.to_string()))?;

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
