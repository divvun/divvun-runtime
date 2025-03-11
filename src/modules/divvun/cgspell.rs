use std::{collections::HashMap, fmt::Write as _, sync::Arc};

use async_trait::async_trait;
use cg3::Block;
use divvunspell::{
    speller::{suggestion::Suggestion, HfstSpeller, Speller},
    transducer::{hfst::HfstTransducer, Transducer},
    vfs::Fs,
};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator as _, ParallelIterator as _};

use crate::{
    ast,
    modules::{Error, SharedInputFut},
};

use super::super::{CommandRunner, Context, Input};

pub struct Cgspell {
    _context: Arc<Context>,
    speller: Arc<dyn Speller + Send + Sync>,
}

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

        let acc_model_path = context.extract_to_temp_dir(acc_model_path)?;
        let err_model_path = context.extract_to_temp_dir(err_model_path)?;

        let lexicon = HfstTransducer::from_path(&Fs, err_model_path).unwrap();
        let mutator = HfstTransducer::from_path(&Fs, acc_model_path).unwrap();
        let speller = HfstSpeller::new(mutator, lexicon);

        Ok(Arc::new(Self {
            _context: context,
            speller,
        }) as _)
    }
}

fn do_cgspell(speller: Arc<dyn Speller + Sync + Send>, word: &str) -> String {
    let is_correct = speller.clone().is_correct(word);

    if is_correct {
        return String::new();
    }

    let suggestions = speller.clone().suggest(word);

    let out = suggestions
        .par_iter()
        .map(|sugg| {
            let chunks = sugg.value.split('#').enumerate().collect::<Vec<_>>();

            chunks.into_par_iter().map(|(i, value)| {
                let form = value.split_ascii_whitespace().next().unwrap();
                let analyses = speller.clone().analyze_output(form);
                ((value, sugg.weight), analyses, i + 1)
            })
        })
        .flatten()
        .map(|((sugg, weight), analysis, i)| print_readings(&analysis, sugg, weight, i))
        .collect::<Vec<String>>()
        .join("\n");

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
        let word_form = chunks.next().unwrap();
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
                    out.push_str(&do_cgspell(self.speller.clone(), c.word_form));
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
