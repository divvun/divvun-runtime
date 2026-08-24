use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    sync::Arc,
};

use crate::modules::cg3::{self, Block};
use async_trait::async_trait;
use divvun_fst::{
    speller::{HfstSpeller, Speller, suggestion::Suggestion},
    transducer::{Transducer as _, hfst::HfstTransducer},
};
use divvun_runtime_macros::{rt_command, rt_struct};
use rayon::iter::{IntoParallelRefIterator as _, ParallelIterator as _};
use serde::{Deserialize, Serialize};

use crate::{ast, modules::Error};

use super::super::{CommandRunner, Context, PipelineValue, PipelineValues};

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
    #[facet(opaque)]
    tags: TagSymbols,
}

/// The acceptor's multi-character symbols, i.e. its CG tags.
///
/// An analysis reaches us as one joined string ("viessu N Sg Nom"), so the only
/// reliable way to tell a tag from literal lemma text is to check it against
/// the transducer's own symbol table. This mirrors divvun-gramcheck's
/// `is_cg_tag`, which treats every symbol longer than one codepoint as a tag
/// and everything else as lemma material.
#[derive(Debug)]
struct TagSymbols {
    symbols: HashSet<String>,
    /// Longest symbol in bytes, so lookups only probe plausible slices.
    max_len: usize,
    /// Byte values a symbol can start with, to skip positions cheaply.
    first_bytes: [bool; 256],
}

impl TagSymbols {
    fn new<'a>(key_table: impl IntoIterator<Item = &'a str>) -> Self {
        let symbols: HashSet<String> = key_table
            .into_iter()
            .filter(|sym| sym.chars().count() > 1)
            .map(str::to_string)
            .collect();

        let max_len = symbols.iter().map(String::len).max().unwrap_or(0);
        let mut first_bytes = [false; 256];
        for sym in symbols.iter() {
            first_bytes[sym.as_bytes()[0] as usize] = true;
        }

        Self {
            symbols,
            max_len,
            first_bytes,
        }
    }

    /// Split an analysis into its lemma and its trailing tags.
    ///
    /// Tags only ever follow the lemma, so the lemma ends at the first position
    /// from which the rest of the analysis parses entirely as tag symbols.
    /// Lemmas may themselves contain spaces (multiword entries), which is why
    /// this can't just split on whitespace (#50).
    fn split_lemma<'a>(&self, analysis: &'a str) -> (&'a str, Vec<&'a str>) {
        let len = analysis.len();
        if self.symbols.is_empty() || len == 0 {
            return (analysis, Vec::new());
        }

        // is_tail[i] == "analysis[i..] is nothing but tags"
        let mut is_tail = vec![false; len + 1];
        is_tail[len] = true;
        let mut lemma_end = len;

        for start in (0..len).rev() {
            if !self.starts_symbol(analysis, start) {
                continue;
            }
            if self.match_symbol(analysis, start, &is_tail).is_some() {
                is_tail[start] = true;
                lemma_end = start;
            }
        }

        let mut tags = Vec::new();
        let mut pos = lemma_end;
        while let Some(end) = self.match_symbol(analysis, pos, &is_tail) {
            tags.push(&analysis[pos..end]);
            pos = end;
        }

        (&analysis[..lemma_end], tags)
    }

    fn starts_symbol(&self, analysis: &str, start: usize) -> bool {
        analysis.is_char_boundary(start) && self.first_bytes[analysis.as_bytes()[start] as usize]
    }

    /// Longest symbol at `start` whose end also begins a pure-tag tail.
    fn match_symbol(&self, analysis: &str, start: usize, is_tail: &[bool]) -> Option<usize> {
        if start >= analysis.len() {
            return None;
        }
        let limit = (start + self.max_len).min(analysis.len());
        (start + 1..=limit).rev().find(|&end| {
            is_tail[end]
                && analysis.is_char_boundary(end)
                && self.symbols.contains(&analysis[start..end])
        })
    }
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

        // Enable verbose mode so each suggestion carries its weight breakdown
        // (lexicon/mutator/reweight). The lexicon component feeds the <WA:> tag
        // instead of a bogus re-analysis weight (#73).
        let config = Some({
            let mut config = config.unwrap_or_else(divvun_fst::speller::SpellerConfig::default);
            config.verbose = true;
            config
        });

        let lexicon = context.load_fst::<HfstTransducer>(&acc_model_path)?;
        let mutator = context.load_fst::<HfstTransducer>(&err_model_path)?;
        let speller = HfstSpeller::new(mutator, lexicon);
        let tags = TagSymbols::new(
            speller
                .lexicon()
                .alphabet()
                .key_table()
                .iter()
                .map(|sym| &**sym),
        );

        Ok(Arc::new(Self {
            _context: context,
            analyzer: speller.clone(),
            speller,
            config,
            tags,
        }) as _)
    }
}

fn do_cgspell(
    speller: Arc<dyn Speller + Sync + Send>,
    analyzer: Arc<dyn Speller + Sync + Send>,
    word: &str,
    config: Option<&divvun_fst::speller::SpellerConfig>,
    tags: &TagSymbols,
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
            // Upstream libdivvun calls `speller->analyseSymbols(corrform, true)`,
            // which runs `Speller::analyseSymbols` -> `mode = Lookup`, a
            // lexicon-only traversal. `analyze_output` is `suggest(..)` with
            // tags, i.e. lexicon o errmodel, so it returns analyses of the
            // suggestion's spelling NEIGHBOURS as well as its own. Those leaked
            // readings are then tagged with this suggestion's "form"S, which
            // makes the CG stream claim e.g. that *cilgegohtet is an infinitive.
            let mut analyses = analyzer.clone().analyze_input(&sugg.value);
            if analyses.is_empty() {
                // The acceptor holds lower-case forms only, so a recased
                // suggestion ("Juohkehaš" at the start of a sentence) has no
                // lexicon-only analysis and would lose every reading -- which
                // deletes the suggestion outright. Retry de-capitalised.
                if let Some(lowered) = decapitalise(&sugg.value) {
                    analyses = analyzer.clone().analyze_input(&lowered);
                }
            }
            tracing::debug!(
                "  suggestion '{}' (weight: {}, details: {:?}) -> {} analyses",
                sugg.value,
                sugg.weight,
                sugg.weight_details,
                analyses.len()
            );
            print_readings(&analyses, sugg, tags)
        })
        .collect::<Vec<String>>()
        .join("")
}

/// Lower-case the first character, or the whole string when it is all upper
/// case. Returns `None` when the form is already lower case.
fn decapitalise(form: &str) -> Option<String> {
    let mut chars = form.chars();
    let first = chars.next()?;
    if !first.is_uppercase() {
        return None;
    }
    let rest: String = chars.collect();
    if !rest.is_empty() && rest.chars().all(|c| !c.is_lowercase()) {
        return Some(form.to_lowercase());
    }
    Some(first.to_lowercase().collect::<String>() + &rest)
}

fn print_readings(analyses: &[Suggestion], sugg: &Suggestion, tags: &TagSymbols) -> String {
    let mut ret = String::new();
    let form = sugg.value.as_str();
    let weight = sugg.weight.0;
    // <WA:> is the suggestion's lexicon (acceptor) weight, taken from the
    // speller's own weight breakdown. Fall back to the per-analysis weight if
    // the breakdown is unavailable (e.g. verbose disabled) (#73).
    let analysis_weight = |fallback: f32| {
        sugg.weight_details
            .as_ref()
            .map(|d| d.lexicon_weight.0)
            .unwrap_or(fallback)
    };

    for analysis in analyses {
        let segments: Vec<&str> = analysis.value.split('#').collect();

        for (idx_from_end, segment) in segments.iter().rev().enumerate() {
            let depth = idx_from_end + 1;
            if segment.is_empty() {
                continue;
            }
            let (lemma, reading_tags) = tags.split_lemma(segment);

            ret.push_str(&"\t".repeat(depth));
            ret.push('"');
            ret.push_str(lemma);
            ret.push('"');
            // Symbols carry their own separator (" Sg" or "+Sg"), so they're
            // written back verbatim.
            for tag in reading_tags {
                ret.push_str(tag);
            }
            if depth == 1 {
                write!(
                    &mut ret,
                    " <W:{}> <WA:{}> <spelled> \"{}\"S",
                    weight,
                    analysis_weight(analysis.weight.0),
                    form
                )
                .unwrap();
            }
            ret.push('\n');
        }
    }

    ret
}

/// Rewrite a CG stream, replacing the readings of unknown cohorts with whatever
/// `spell` produces for their word form. Every emitted line is newline
/// terminated exactly once, so cohorts stay flush against the blank that follows
/// them, as in the incoming stream.
///
/// `spell` is a parameter so the stream layout can be tested without a speller.
fn render_stream(input: &str, mut spell: impl FnMut(&str) -> String) -> Result<String, Error> {
    let output = cg3::Output::new(input);
    let mut out = String::new();

    for thing in output.iter() {
        match thing.map_err(Error::wrap)? {
            Block::Cohort(c) => {
                writeln!(&mut out, "\"<{}>\"", c.word_form).map_err(Error::wrap)?;

                let is_unknown = c
                    .kept()
                    .any(|x| x.tags.contains(&"+?") || x.tags.contains(&"?"));

                let spelled = if is_unknown {
                    spell(c.word_form)
                } else {
                    String::new()
                };

                if is_unknown && !spelled.trim().is_empty() {
                    out.push_str(&spelled);
                } else {
                    // Known word, or an unknown word the speller produced no
                    // suggestions for: keep the original readings so the cohort
                    // doesn't silently lose its (unknown) analysis (#43).
                    // Display re-emits the `;` prefix for --trace removed
                    // readings; hand-rebuilding the line here would promote
                    // them to kept ones.
                    for x in &c.readings {
                        writeln!(&mut out, "{}", x).map_err(Error::wrap)?;
                    }
                }
            }
            Block::Escaped(x) => {
                out.push(':');
                out.push_str(x);
                out.push('\n');
            }
            Block::StreamCmd(x) | Block::Text(x) => {
                out.push_str(x);
                out.push('\n');
            }
        }
    }

    Ok(out)
}

#[async_trait]
impl CommandRunner for Cgspell {
    async fn forward(
        self: Arc<Self>,
        input: PipelineValue,
        _config: Arc<serde_json::Value>,
    ) -> Result<PipelineValues, crate::modules::Error> {
        let input = input.try_into_string()?;

        let out = render_stream(&input, |word_form| {
            do_cgspell(
                self.speller.clone(),
                self.analyzer.clone(),
                word_form,
                self.config.as_ref(),
                &self.tags,
            )
        })?;

        Ok(out.into())
    }

    fn name(&self) -> &'static str {
        "divvun::cgspell"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use divvun_fst::types::Weight;

    /// Tags as a Giella acceptor stores them: the leading space is part of the
    /// symbol.
    fn tags() -> TagSymbols {
        TagSymbols::new([
            "@_EPSILON_SYMBOL_@",
            " N",
            " V",
            " IV",
            " Sg",
            " Nom",
            " Acc",
            " Sem/Hum",
            " Sem/Lang",
            " NomAg",
            "+N",
            "+Sg",
            "+Nom",
            "a",
            "b",
            " ",
        ])
    }

    fn sugg(value: &str, weight: f32) -> Suggestion {
        Suggestion::new(value.into(), Weight(weight), None)
    }

    /// A cohort used to get a trailing newline of its own on top of the one that
    /// ends its last reading, so every cohort in the stream was followed by a
    /// blank line that the rest of the pipeline then carried all the way to the
    /// output.
    #[test]
    fn cohorts_are_not_followed_by_a_blank_line() {
        let input = concat!(
            "\"<Mån>\"\n",
            "\t\"mån\" Pron Sg1 Nom\n",
            ": \n",
            "\"<nuvviDspeller>\"\n",
            "\t\"nuvviDspeller\" ?\n",
            ":\\n\n",
        );

        assert_eq!(
            render_stream(input, |wf| format!("\t\"{wf}\" N <spelled>\n")).unwrap(),
            concat!(
                "\"<Mån>\"\n",
                "\t\"mån\" Pron Sg1 Nom\n",
                ": \n",
                "\"<nuvviDspeller>\"\n",
                "\t\"nuvviDspeller\" N <spelled>\n",
                ":\\n\n",
            )
        );
    }

    /// An unknown word the speller has nothing for keeps its own readings (#43),
    /// and still gets no blank line after it.
    #[test]
    fn unspellable_cohort_keeps_readings_without_a_blank_line() {
        let input = "\"<xyzzy>\"\n\t\"xyzzy\" ?\n:\\n\n";

        assert_eq!(
            render_stream(input, |_| String::new()).unwrap(),
            "\"<xyzzy>\"\n\t\"xyzzy\" ?\n:\\n\n"
        );
    }

    /// A `--trace` removed reading keeps its `;` through cgspell. The re-emit
    /// path used to rebuild reading lines by hand, which would have silently
    /// promoted it to a kept reading.
    #[test]
    fn traced_removed_readings_keep_their_marker() {
        let input = concat!(
            "\"<Mån>\"\n",
            "\t\"mån\" Pron Sg1 Nom\n",
            ";\t\"mån\" Pron Sg1 Gen\n",
            ":\\n\n",
        );

        assert_eq!(render_stream(input, |_| String::new()).unwrap(), input);
    }

    /// `is_unknown` must not fire on a `?` the grammar already removed.
    #[test]
    fn a_removed_unknown_reading_does_not_trigger_the_speller() {
        let input = concat!(
            "\"<Mån>\"\n",
            ";\t\"mån\" ?\n",
            "\t\"mån\" Pron Sg1 Nom\n",
            ":\\n\n",
        );

        assert_eq!(
            render_stream(input, |wf| format!("\t\"{wf}\" N <spelled>\n")).unwrap(),
            input
        );
    }

    #[test]
    fn splits_lemma_from_tags() {
        assert_eq!(
            tags().split_lemma("boahtti N NomAg Sem/Hum Sg Acc"),
            ("boahtti", vec![" N", " NomAg", " Sem/Hum", " Sg", " Acc"])
        );
    }

    #[test]
    fn keeps_multiword_lemma_intact() {
        // #50: " N" is a tag, but the space before "Northern" is literal, so the
        // tail has to fail to parse as tags and stay part of the lemma.
        assert_eq!(
            tags().split_lemma("Divvun speller for Northern Sami"),
            ("Divvun speller for Northern Sami", vec![])
        );
    }

    #[test]
    fn splits_multiword_lemma_from_its_tags() {
        assert_eq!(
            tags().split_lemma("Northern Sami N Sem/Lang Sg Nom"),
            ("Northern Sami", vec![" N", " Sem/Lang", " Sg", " Nom"])
        );
    }

    #[test]
    fn handles_tags_without_a_leading_space() {
        assert_eq!(
            tags().split_lemma("viessu+N+Sg+Nom"),
            ("viessu", vec!["+N", "+Sg", "+Nom"])
        );
    }

    #[test]
    fn treats_everything_as_lemma_without_a_symbol_table() {
        let tags = TagSymbols::new(["a", "b"]);
        assert_eq!(
            tags.split_lemma("Built using HFST 3.17.1"),
            ("Built using HFST 3.17.1", vec![])
        );
    }

    #[test]
    fn splits_on_multibyte_boundaries() {
        assert_eq!(
            tags().split_lemma("sámegiella N Sem/Lang Sg Nom"),
            ("sámegiella", vec![" N", " Sem/Lang", " Sg", " Nom"])
        );
    }

    #[test]
    fn prints_reading_with_multiword_lemma() {
        let form = sugg("Divvun speller for Northern Sami", 1.0);
        let analyses = [form.clone()];

        assert_eq!(
            print_readings(&analyses, &form, &tags()),
            "\t\"Divvun speller for Northern Sami\" <W:1> <WA:1> <spelled> \
             \"Divvun speller for Northern Sami\"S\n"
        );
    }

    #[test]
    fn prints_subreadings_deepest_first() {
        let form = sugg("boazodoallu", 2.0);
        let analyses = [sugg("boazu N Sg Nom#doallu N Sg Nom", 3.0)];

        assert_eq!(
            print_readings(&analyses, &form, &tags()),
            "\t\"doallu\" N Sg Nom <W:2> <WA:3> <spelled> \"boazodoallu\"S\n\
             \t\t\"boazu\" N Sg Nom\n"
        );
    }
}
