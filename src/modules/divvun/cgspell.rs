use std::{collections::HashMap, fmt::Write as _, sync::Arc};

use async_trait::async_trait;
use cg3::Block;
use divvunspell::{
    speller::{suggestion::Suggestion, HfstSpeller, Speller},
    transducer::{hfst::HfstTransducer, Transducer},
    vfs::Fs,
};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator as _, ParallelIterator as _};

use crate::ast;

use super::super::{CommandRunner, Context, Input, InputFut};

pub struct Cgspell {
    _context: Arc<Context>,
    speller: Arc<dyn Speller + Send + Sync>,
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
            let form = sugg.value.split_ascii_whitespace().next().unwrap();
            let analyses = speller.clone().analyze_output(form);
            analyses.into_par_iter().map(move |a| (sugg, a))
        })
        .flatten()
        .map(|(sugg, analysis)| print_readings(&analysis, &sugg, 1))
        .collect::<Vec<String>>()
        .join("\n");

    out
}

fn print_readings(analysis: &Suggestion, sugg: &Suggestion, indent: usize) -> String {
    let form = sugg.value.split_ascii_whitespace().next().unwrap();
    let mut ret = "\t".repeat(indent);
    ret.push('"');
    let mut chunks = analysis.value().split_ascii_whitespace();
    let word_form = chunks.next().unwrap();
    ret.push_str(&word_form);
    ret.push('"');
    for chunk in chunks {
        ret.push(' ');
        ret.push_str(&chunk);
    }
    write!(
        &mut ret,
        " <spelled> <W:{}> <WA:{}> \"{}\"S",
        sugg.weight(),
        analysis.weight(),
        form
    )
    .unwrap();
    ret
}

#[async_trait(?Send)]
impl CommandRunner for Cgspell {
    async fn forward(self: Arc<Self>, input: InputFut) -> Result<Input, anyhow::Error> {
        let input = input.await?.try_into_string()?;
        let output = cg3::Output::new(&input);
        let mut out = String::new();

        for thing in output.clone().iter() {
            let thing = thing?;

            match thing {
                Block::Cohort(c) => {
                    writeln!(&mut out, "\"<{}>\"", c.word_form)?;
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
            // out.push_str("<WAT>");
            out.push('\n');
            // out.push_str("<-WAT->");
        }

        Ok(out.into())
    }

    fn name(&self) -> &'static str {
        "divvun::cgspell"
    }
}
