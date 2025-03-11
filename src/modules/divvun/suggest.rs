use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc};

use async_trait::async_trait;

use cg3::Block;
use heck::ToTitleCase as _;
use serde::Serialize;
use tokio::io::AsyncWriteExt;

use crate::{
    ast,
    modules::{Error, SharedInputFut},
};

use super::super::{CommandRunner, Context, Input};

// pub struct Suggest {
//     _context: Arc<Context>,
//     model_path: PathBuf,
//     error_xml_path: PathBuf,
// }

// impl Suggest {
//     pub fn new(
//         context: Arc<Context>,
//         mut kwargs: HashMap<String, ast::Arg>,
//     ) -> Result<Arc<dyn CommandRunner + Send + Sync>, Error> {
//         tracing::debug!("Creating suggest");
//         let model_path = kwargs
//             .remove("model_path")
//             .and_then(|x| x.value)
//             .ok_or_else(|| Error("model_path missing".to_string()))?;
//         let error_xml_path = kwargs
//             .remove("error_xml_path")
//             .and_then(|x| x.value)
//             .ok_or_else(|| Error("error_xml_path missing".to_string()))?;

//         let model_path = context.extract_to_temp_dir(model_path)?;
//         let error_xml_path = context.extract_to_temp_dir(error_xml_path)?;

//         // let generator = Arc::new(hfst::Transducer::new(model_path));

//         Ok(Arc::new(Self {
//             _context: context,
//             model_path,
//             error_xml_path,
//         }) as _)
//     }
// }

// #[async_trait]
// impl CommandRunner for Suggest {
//     async fn forward(self: Arc<Self>, input: SharedInputFut) -> Result<Input, Error> {
//         let input = input
//             .await?
//             .try_into_string()
//             .unwrap();

//         let mut child = tokio::process::Command::new("divvun-suggest")
//             .arg("--verbose")
//             .arg("--json")
//             .arg(&self.model_path)
//             // .arg(&self.error_xml_path)
//             .stdin(Stdio::piped())
//             .stdout(Stdio::piped())
//             .spawn()
//             .map_err(|e| {
//                 tracing::debug!("suggest ({}): {e:?}", self.model_path.display());
//                 e
//             })
//             .unwrap();

//         let mut stdin = child.stdin.take().unwrap();
//         let input0 = input.clone();
//         tokio::spawn(async move {
//             stdin.write_all(input0.as_bytes()).await.unwrap();
//         });

//         let output = child
//             .wait_with_output()
//             .await
//             .unwrap();

//         let mut child = tokio::process::Command::new("divvun-suggest")
//             .arg("--verbose")
//             .arg(&self.model_path)
//             // .arg(&self.error_xml_path)
//             .stdin(Stdio::piped())
//             .stdout(Stdio::piped())
//             .spawn()
//             .map_err(|e| {
//                 tracing::debug!("suggest ({}): {e:?}", self.model_path.display());
//                 e
//             })
//             .unwrap();

//         let mut stdin = child.stdin.take().unwrap();
//         tokio::spawn(async move {
//             stdin.write_all(input.as_bytes()).await.unwrap();
//         });

//         let output2 = child
//             .wait_with_output()
//             .await
//             .unwrap();
//         // let output: serde_json::Value =
//         //     serde_json::from_slice(output.stdout.as_slice()).unwrap();
//         let output2 = String::from_utf8(output2.stdout.as_slice().to_vec()).unwrap();
//         let output = String::from_utf8(output.stdout.as_slice().to_vec()).unwrap();
//         Ok(format!("{}\n===\n{}", output2, output).into())
//     }

//     fn name(&self) -> &'static str {
//         "divvun::suggest"
//     }
// }

pub struct Suggest {
    _context: Arc<Context>,
    generator: Arc<hfst::Transducer>,
    error_xml_path: PathBuf,
}

impl Suggest {
    pub fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, Error> {
        tracing::debug!("Creating suggest");
        let model_path = kwargs
            .remove("model_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("model_path missing".to_string()))?;
        let error_xml_path = kwargs
            .remove("error_xml_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("error_xml_path missing".to_string()))?;

        let model_path = context.extract_to_temp_dir(model_path)?;
        let error_xml_path = context.extract_to_temp_dir(error_xml_path)?;

        let generator = Arc::new(hfst::Transducer::new(model_path));

        Ok(Arc::new(Self {
            _context: context,
            generator,
            error_xml_path,
        }) as _)
    }
}

#[derive(Debug, Serialize)]
struct SuggestResult<'a> {
    word: &'a str,
    char_offset: usize,
    utf16_offset: usize,
    byte_offset: usize,
}

#[derive(Debug, Serialize)]
struct SuggestOutput<'a> {
    results: Vec<SuggestResult<'a>>,
    input: String,
}

#[async_trait]
impl CommandRunner for Suggest {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.try_into_string()?;
        // let input = cg3::Output::new(&input);

        let suggester = Suggester::new(
            self.generator.clone(),
            Default::default(),
            "se".into(),
            false,
            true,
        );

        let output = tokio::task::spawn_blocking(move || {
            let mut writer = std::io::BufWriter::new(Vec::new());

            suggester.run(&input, &mut writer, RunMode::RunJson);
            writer.into_inner().unwrap()
        })
        .await
        .unwrap();

        let output = String::from_utf8(output).unwrap();

        Ok(output.into())

        // let original_input = input
        //     .iter()
        //     .filter_map(|x| match x {
        //         Ok(Block::Cohort(x)) => Some(Ok(x.word_form)),
        //         Ok(Block::Escaped(x)) => Some(Ok(x)),
        //         Ok(Block::Text(_)) => None,
        //         Err(e) => Some(Err(e)),
        //     })
        //     .collect::<Result<Vec<_>, _>>()
        //     .map_err(|e| Error(e.to_string()))?;

        // let mut char_offset = 0usize;
        // let mut byte_offset = 0usize;
        // let mut utf16_offset = 0usize;

        // let results = input
        //     .iter()
        //     .filter_map(|x| match x {
        //         Ok(Block::Cohort(x)) => {
        //             // proc_reading(self.)
        //             // x.readings.iter().map(|x| {
        //             //     let tags = self.generator.lookup_tags(x.raw_line));
        //             // });
        //             // // let sforms = );

        //             let out = SuggestResult {
        //                 word: x.word_form,
        //                 char_offset,
        //                 byte_offset,
        //                 utf16_offset,
        //             };

        //             char_offset += x.word_form.chars().count();
        //             byte_offset += x.word_form.as_bytes().len();
        //             utf16_offset += x.word_form.encode_utf16().count();
        //             Some(Ok(out))
        //         }
        //         Ok(Block::Escaped(x)) => {
        //             char_offset += x.chars().count();
        //             byte_offset += x.as_bytes().len();
        //             utf16_offset += x.encode_utf16().count();
        //             None
        //         }
        //         Ok(Block::Text(_)) => None,
        //         Err(e) => Some(Err(e)),
        //     })
        //     .collect::<Result<Vec<_>, _>>()
        //     .map_err(|e| Error(e.to_string()))?;

        // let output = serde_json::to_string(&SuggestOutput {
        //     input: original_input.join(""),
        //     results,
        // })
        // .unwrap();
        // Ok(output.into())
    }

    fn name(&self) -> &'static str {
        "divvun::suggest"
    }
}

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;
use std::io::{BufRead, Write};
static CG_LINE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r#"^"#,
        r#"(\"<(.*)>\".*"#,                         // wordform, group 2
        r#"|(\t+)(\"[^\"]*\"\S*)((?:\s+\S+)*)\s*"#, // reading, group 3, 4, 5
        r#"|:(.*)"#,                                // blank, group 6
        r#"|(<STREAMCMD:FLUSH>)"#,                  // flush, group 7
        r#"|(;\t+.*)"#,                             // traced reading, group 8
        r#")"#,
    ))
    .unwrap()
});

static CG_TAGS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#""[^"]*"S?|[^ ]+"#).unwrap());

static CG_TAG_TYPE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r#"^"#,
        r#"("#,                // Group 1: Dependencies, comments
        r#"|&(.+?)"#,          // Group 2: Errtype
        r#"|R:(.+):([0-9]+)"#, // Group 3 & 4: Relation name and target
        r#"|ID:([0-9]+)"#,     // Group 5: Relation ID
        r#"|"([^"]+)"S"#,      // Group 6: Reading word-form
        r#"|(<fixedcase>)"#,   // Group 7: Fixed Casing
        r#"|"<([^>]+)>""#,     // Group 8: Broken word-form from MWE-split
        r#"|[cC][oO]&(.+)"#,   // Group 9: COERROR errtype (for example co&err-agr)
        r#"|@"#,               // Syntactic tag
        r#"|Sem/"#,            // Semantic tag
        r#"|§"#,               // Semantic role
        r#"|<"#,               // Weights (<W:0>) and such
        r#"|ADD:"#,
        r#"|PROTECT:"#,
        r#"|UNPROTECT:"#,
        r#"|MAP:"#,
        r#"|REPLACE:"#,
        r#"|SELECT:"#,
        r#"|REMOVE:"#,
        r#"|IFF:"#,
        r#"|APPEND:"#,
        r#"|SUBSTITUTE:"#,
        r#"|REMVARIABLE:"#,
        r#"|SETVARIABLE:"#,
        r#"|DELIMIT:"#,
        r#"|MATCH:"#,
        r#"|SETPARENT:"#,
        r#"|SETCHILD:"#,
        r#"|ADDRELATION[:\(]"#,
        r#"|SETRELATION[:\(]"#,
        r#"|REMRELATION[:\(]"#,
        r#"|ADDRELATIONS[:\(]"#,
        r#"|SETRELATIONS[:\(]"#,
        r#"|REMRELATIONS[:\(]"#,
        r#"|MOVE:"#,
        r#"|MOVE-AFTER:"#,
        r#"|MOVE-BEFORE:"#,
        r#"|SWITCH:"#,
        r#"|REMCOHORT:"#,
        r#"|UNMAP:"#,
        r#"|COPY:"#,
        r#"|ADDCOHORT:"#,
        r#"|ADDCOHORT-AFTER:"#,
        r#"|ADDCOHORT-BEFORE:"#,
        r#"|EXTERNAL:"#,
        r#"|EXTERNAL-ONCE:"#,
        r#"|EXTERNAL-ALWAYS:"#,
        r#"|REOPEN-MAPPINGS:"#,
        r#").*"#,
    ))
    .unwrap()
});

static MSG_TEMPLATE_REL: Lazy<Regex> = Lazy::new(|| Regex::new(r#"^\$[0-9]+$"#).unwrap());

static DELETE_REL: Lazy<Regex> = Lazy::new(|| Regex::new(r#"^DELETE[0-9]*$"#).unwrap());

static LEFT_RIGHT_DELETE_REL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^(LEFT|RIGHT|DELETE[0-9]*)$"#).unwrap());

enum LineType {
    WordformL,
    ReadingL,
    BlankL,
}

static ALL_MATCHES_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#""[^"]*"S?|[^ ]+"#).unwrap());

#[derive(Debug, Default, Clone)]
struct Reading {
    suggest: bool,
    ana: String,                 // for generating suggestions from this reading
    errtypes: HashSet<String>,   // the error tag(s) (without leading ampersand)
    coerrtypes: HashSet<String>, // the COERROR error tag(s) (without leading ampersand)
    sforms: Vec<String>,
    rels: HashMap<String, u32>, // rels[relname] = target.id
    id: u32,                    // id is 0 if unset, otherwise the relation id of this word
    wf: String,                 // tag of type "wordform"S for use with &SUGGESTWF
    suggestwf: bool,
    coerror: bool, // cohorts that are not the "core" of the underline never become Err's; message template offsets refer to the cohort of the Err
    added: AddedStatus,
    fixedcase: bool, // don't change casing on suggestions if we have this tag
    line: String,    // The (unchanged) input lines which created this Reading
}

#[derive(Debug, Default, Clone)]
struct Cohort {
    form: String,
    pos: usize, // position in text
    id: u32,    // CG relation id
    readings: Vec<Reading>,
    errtypes: HashSet<String>, // the error tag(s) of all readings (without leading ampersand)
    coerrtypes: HashSet<String>, // the COERROR error tag(s) of all readings (without leading ampersand)
    added: AddedStatus,
    raw_pre_blank: String, // blank before cohort, in CG stream format (initial colon, brackets, escaped newlines)
    errs: Vec<Err>,
    trace_removed_readings: String, // lines prefixed with `;` by `vislcg3 -t`
}

impl Cohort {
    fn is_empty(&self) -> bool {
        self.form.is_empty()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
enum AddedStatus {
    #[default]
    NotAdded,
    AddedAfterBlank,
    AddedBeforeBlank,
}

fn proc_subreading(reading: &cg3::Reading) -> Reading {
    let mut r = Reading::default();
    tracing::debug!("Subreading: {:?}", reading);
    // let lemma_beg = line.find('\"').unwrap_or(0);
    // let lemma_end = line[lemma_beg..].find("\" ").unwrap_or(0) + lemma_beg;
    // let lemma = &line[lemma_beg + 1..lemma_end];
    // let tags = &line[lemma_end + 2..];
    let mut gentags: Vec<String> = Vec::new(); // tags we generate with
    r.id = 0; // CG-3 id's start at 1, should be safe. Want sum types :-/
    r.wf = String::new();
    r.suggest = true;
    r.suggestwf = false;
    r.added = AddedStatus::NotAdded;
    r.coerror = false;
    r.fixedcase = false;

    for tag in reading.tags.iter() {
        if tag.starts_with("ID:") {
            r.id = tag[3..].parse().unwrap();
        } else if tag.starts_with("R:") {
            let mut parts = tag[2..].split(':');
            let relname = parts.next().unwrap();
            let target = parts.next().unwrap().parse().unwrap();
            r.rels.insert(relname.to_string(), target);
        } else if tag.ends_with("S") {
            r.sforms.push(tag[1..tag.len() - 2].to_string());
        } else if tag.starts_with("<fixedcase>") {
            r.fixedcase = true;
        } else if tag.starts_with("<") {
            r.wf = tag[1..tag.len() - 1].to_string();
        } else if tag.starts_with("co&") {
            r.coerrtypes.insert(tag.to_string());
        } else if tag.starts_with("&") {
            r.errtypes.insert(tag.to_string());
        } else {
            gentags.push(tag.to_string());
        }
    }

    // r#"("#,                // Group 1: Dependencies, comments
    // r#"|&(.+?)"#,          // Group 2: Errtype
    // r#"|R:(.+):([0-9]+)"#, // Group 3 & 4: Relation name and target
    // r#"|ID:([0-9]+)"#,     // Group 5: Relation ID
    // r#"|"([^"]+)"S"#,      // Group 6: Reading word-form
    // r#"|(<fixedcase>)"#,   // Group 7: Fixed Casing
    // r#"|"<([^>]+)>""#,     // Group 8: Broken word-form from MWE-split
    // r#"|[cC][oO]&(.+)"#,   // Group 9: COERROR errtype (for example co&err-agr)
    // for cap in ALL_MATCHES_RE.find_iter(tags) {
    //     let tag = cap.as_str();
    //     if let Some(cap) = CG_TAG_TYPE.captures(tag) {
    //         tracing::debug!("Tag type: {:?}", cap);
    //         if tag == "COERROR" {
    //             r.coerror = true;
    //         } else if tag.starts_with("&") {
    //             if tag == "&SUGGEST" {
    //                 r.suggest = true;
    //             } else if tag == "&SUGGESTWF" {
    //                 r.suggestwf = true;
    //             } else if tag == "&ADDED" || tag == "&ADDED-AFTER-BLANK" {
    //                 r.added = AddedStatus::AddedAfterBlank;
    //             } else if tag == "&ADDED-BEFORE-BLANK" {
    //                 r.added = AddedStatus::AddedBeforeBlank;
    //             } else if tag == "&LINK" || tag == "&COERROR" {
    //                 // &LINK kept for backward-compatibility
    //                 r.coerror = true;
    //             } else {
    //                 r.errtypes.insert(tag.to_string());
    //             }
    //         } else if let Some(target) = cap.get(4).and_then(|m| m.as_str().parse::<u32>().ok()) {
    //             r.rels.insert(cap[3].to_string(), target);
    //         } else if let Some(id) = cap.get(5).and_then(|m| m.as_str().parse::<u32>().ok()) {
    //             r.id = id;
    //         } else if cap.get(6).is_some() {
    //             r.wf = cap[6].to_string();
    //         } else if cap.get(7).is_some() {
    //             r.fixedcase = true;
    //         } else if cap.get(8).is_some() {
    //             r.wf = cap[8].to_string();
    //         } else if cap.get(9).is_some() {
    //             r.coerrtypes.insert(cap[9].to_string());
    //         }
    //     } else {
    //         gentags.push(tag.to_string());
    //     }
    // }
    let tagsplus = gentags.join("+");
    r.ana = format!("{}+{}", reading.base_form, tagsplus);
    if r.suggestwf {
        r.sforms.push(r.wf.clone());
    }
    r
}

fn proc_reading(generator: &hfst::Transducer, cohort: &cg3::Cohort) -> Reading {
    let mut subs = Vec::new();
    for reading in &cohort.readings {
        subs.push(proc_subreading(reading));
    }
    // for subline in line.lines().rev() {
    //     subs.push(proc_subreading(subline));
    // }
    let mut r = Reading::default();
    // r.line = line.to_string();
    let n_subs = subs.len();
    for (i, sub) in subs.into_iter().enumerate() {
        r.ana += &sub.ana;
        if i + 1 != n_subs {
            r.ana.push('#');
        }
        r.errtypes.extend(sub.errtypes);
        r.coerrtypes.extend(sub.coerrtypes);
        r.rels.extend(sub.rels);
        // higher sub can override id if set; doesn't seem like cg3 puts ids on them though
        if r.id == 0 {
            r.id = sub.id;
        }
        r.suggest = r.suggest || sub.suggest; //;|| generate_all_readings;
        r.suggestwf = r.suggestwf || sub.suggestwf;
        r.coerror = r.coerror || sub.coerror;
        r.added = if r.added == AddedStatus::NotAdded {
            sub.added
        } else {
            r.added
        };
        r.sforms.extend(sub.sforms);
        if r.wf.is_empty() {
            r.wf = sub.wf;
        }
        r.fixedcase |= sub.fixedcase;
    }

    // TODO: Is this the thing that is actually providing the suggestions? Seems odd.
    if r.suggest {
        let paths = generator.lookup_tags(&r.ana, false);
        for p in paths {
            r.sforms.push(p);
        }
    }

    // if (r.suggest) {
    // 	const HfstPaths1L paths(generator.lookup_fd({ r.ana }, -1, 10.0));
    // 	for (auto& p : *paths) {
    // 		stringstream form;
    // 		for (auto& symbol : p.second) {
    // 			if (!hfst::FdOperation::is_diacritic(symbol)) {
    // 				form << symbol;
    // 			}
    // 		}
    // 		r.sforms.emplace_back(form.str());
    // 	}
    // }
    r
}

type Lang = String;
type Msg = (String, String); // (title, description)
type ErrId = String;
type ErrRe = Regex;

#[derive(Debug, Default, Clone, serde::Serialize)]
struct Err {
    form: String,
    beg: usize,
    end: usize,
    err: ErrId,
    msg: Msg,
    rep: Vec<String>,
}

#[derive(Debug, Default, Clone)]
struct Sentence {
    cohorts: Vec<Cohort>,
    ids_cohorts: HashMap<u32, usize>, // mapping from cohort relation id's to their position in Sentence.cohort vector
    text: String,
    runstate: RunState,
    raw_final_blank: String, // blank after last cohort, in CG stream format (initial colon, brackets, escaped newlines)
    errs: Vec<Err>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlushOn {
    Nul,
    NulAndDelimiters,
}

// Default value for Suggest.delimiters:
fn default_delimiters() -> HashSet<String> {
    [".", "?", "!"].iter().map(|s| s.to_string()).collect()
}

fn rel_on_match<F>(rels: &HashMap<String, u32>, name: &Regex, sentence: &Sentence, mut fn_: F)
where
    F: FnMut(&str, usize, &Cohort),
{
    for (rel_name, target_id) in rels.iter() {
        if name.is_match(rel_name) {
            if let Some(&i_t) = sentence.ids_cohorts.get(target_id) {
                if i_t < sentence.cohorts.len() {
                    fn_(rel_name, i_t, &sentence.cohorts[i_t]);
                } else {
                    tracing::debug!(
                        "WARNING: Couldn't find relation target for {}:{}",
                        rel_name,
                        target_id
                    );
                }
            } else {
                tracing::debug!(
                    "WARNING: Couldn't find relation target for {}:{}",
                    rel_name,
                    target_id
                );
            }
        }
    }
}

/**
 * Calculate the left/right bounds of the error underline, as indices into sentence.
 */
fn squiggle_bounds(
    rels: &HashMap<String, u32>,
    sentence: &Sentence,
    i_fallback: usize,
    fallback: &Cohort,
) -> (usize, usize) {
    let mut left = i_fallback;
    let mut right = i_fallback;
    // If we have several relation targets, prefer leftmost if LEFT, rightmost if RIGHT:
    rel_on_match(
        rels,
        &LEFT_RIGHT_DELETE_REL,
        sentence,
        |relname, i_trg, trg| {
            if trg.id == 0 {
                return; // unexpected, CG should always give id's to relation targets
            }
            if i_trg < left {
                left = i_trg;
            }
            if i_trg > right {
                right = i_trg;
            }
        },
    );
    if left < 0 {
        tracing::debug!(
            "WARNING: Left underline boundary relation target {} out of bounds",
            left
        );
        left = 0;
    }
    if right >= sentence.cohorts.len() {
        tracing::debug!(
            "WARNING: Right underline relation target {} out of bounds",
            right
        );
        right = sentence.cohorts.len() - 1;
    }
    (left, right)
}

/**
 * Return the readings of Cohort trg that have ErrId err_id and apply
 * some change; fallback to all the readings if no match.
 */
fn readings_with_errtype(trg: &Cohort, err_id: &str, applies_deletion: bool) -> Vec<Reading> {
    let filtered: Vec<Reading> = trg
        .readings
        .iter()
        .filter(|tr| {
            let has_our_errtag = tr.errtypes.contains(err_id) || tr.coerrtypes.contains(err_id);
            let applies_change =
                tr.added != AddedStatus::NotAdded || !tr.sforms.is_empty() || applies_deletion;
            has_our_errtag && applies_change
        })
        .cloned()
        .collect();
    if filtered.is_empty() {
        let not_just_other_errtype: Vec<Reading> = trg
            .readings
            .iter()
            .filter(|tr| {
                let has_our_errtag = tr.errtypes.contains(err_id) || tr.coerrtypes.contains(err_id);
                let no_errtags = tr.errtypes.is_empty() && tr.coerrtypes.is_empty();
                no_errtags || has_our_errtag
            })
            .cloned()
            .collect();
        not_just_other_errtype
    } else {
        filtered
    }
}

/**
 * CG relations unfortunately go from Cohort to Cohort. But we may
 * have several readings on one cohort with different error tags, only
 * one of which should apply the R:DELETE1 relation. We check the
 * target of the delete relation, if it has the same error tag, then
 * the relation applies. (If there's no ambiguity, we can always
 * delete).
 */
fn do_delete(
    trg: &Cohort,
    err_id: &str,
    src_errtypes: &HashSet<String>,
    deletions: &HashSet<u32>,
) -> bool {
    if !deletions.contains(&trg.id) {
        // There is no deletion of this target cohort
        return false;
    }
    if src_errtypes.len() < 2 {
        // Just one error type, no need to disambiguate which one has the relation
        return true;
    }
    // There are several err_id's on src; we should only delete
    // trg in err_id replacement if trg has err_id
    for tr in &trg.readings {
        if tr.errtypes.contains(err_id) || tr.coerrtypes.contains(err_id) {
            return true;
        }
    }
    // But what if source and target have no matching errtypes at all?
    let mut trg_errtypes_w_co: HashSet<String> =
        trg.errtypes.union(&trg.coerrtypes).cloned().collect();
    let errtypes_isect: HashSet<_> = trg_errtypes_w_co
        .intersection(src_errtypes)
        .cloned()
        .collect();
    if errtypes_isect.is_empty() {
        // No matching err types at all on trg, we can't filter on errtype, allow deletion
        return true;
    } else {
        // Not found with this errtype, but there is another possible match, don't allow deletion:
        return false;
    }
}

fn both_spaces(lhs: char, rhs: char) -> bool {
    lhs == rhs && lhs == ' '
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Casing {
    Lower,
    Title,
    Upper,
    Mixed,
}

fn get_casing(input: &str) -> Casing {
    if input.is_empty() {
        return Casing::Mixed;
    }

    let mut seen_upper = false;
    let mut seen_lower = false;
    let mut fst_upper = false;
    let mut non_fst_upper = false;

    for (i, c) in input.chars().enumerate() {
        if c.is_uppercase() {
            if seen_lower || seen_upper {
                non_fst_upper = true;
            } else if i == 0 {
                fst_upper = true;
            }
            seen_upper = true;
        }
        if c.is_lowercase() {
            seen_lower = true;
        }
    }

    if !seen_upper {
        Casing::Lower
    } else if !seen_lower {
        Casing::Upper
    } else if fst_upper && !non_fst_upper {
        Casing::Title
    } else {
        Casing::Mixed
    }
}

fn with_casing(fixedcase: bool, input_casing: Casing, input: &str) -> String {
    if fixedcase {
        return input.to_string();
    }
    match input_casing {
        Casing::Title => input.to_title_case(),
        Casing::Upper => input.to_uppercase(),
        Casing::Lower => input.to_lowercase(),
        Casing::Mixed => input.to_string(),
    }
}

fn build_squiggle_replacement(
    r: &Reading,
    err_id: &str,
    i_c: usize,
    src: &Cohort,
    sentence: &Sentence,
    orig_beg: usize,
    orig_end: usize,
    i_left: usize,
    i_right: usize,
    verbose: bool,
) -> Option<((usize, usize), Vec<String>)> {
    let mut beg = orig_beg;
    let mut end = orig_end;
    let mut deletions = HashSet::new();
    let mut src_applies_deletion = false;
    rel_on_match(&r.rels, &DELETE_REL, sentence, |relname, i_t, trg| {
        deletions.insert(trg.id);
        if trg.errtypes.contains(err_id) || trg.coerrtypes.contains(err_id) {
            src_applies_deletion = true;
        }
    });
    // let mut add = HashMap::new();
    if verbose {
        tracing::debug!("=== err_id=\t{} ===", err_id);
        tracing::debug!("r.id=\t{}", r.id);
        tracing::debug!("src.id=\t{}", src.id);
        tracing::debug!("i_c=\t{}", i_c);
        tracing::debug!("left=\t{}", i_left);
        tracing::debug!("right=\t{}", i_right);
    }
    let mut reps = vec![String::new()];
    let mut reps_suggestwf = vec![]; // If we're doing SUGGESTWF, we ignore reps
    let mut prev_added_before_blank = String::new();
    for i in i_left..=i_right {
        let trg = &sentence.cohorts[i];
        let casing = get_casing(&trg.form);
        if verbose {
            tracing::debug!("i=\t{}", i);
            tracing::debug!("trg.form=\t'{}'", trg.form);
            tracing::debug!("trg.id=\t{}", trg.id);
            tracing::debug!("trg.raw_pre_blank=\t'{}'", trg.raw_pre_blank);
        }
        let mut rep_this_trg = vec![];
        let del = do_delete(trg, err_id, &src.errtypes, &deletions);
        if del {
            rep_this_trg.push(String::new());
            if verbose {
                tracing::debug!("\t\tdelete=\t{}", trg.form);
            }
        }
        let mut added_before_blank = false;
        let applies_deletion = trg.id == src.id && src_applies_deletion;
        let mut trg_beg = trg.pos;
        let mut trg_end = trg.pos + trg.form.len();
        for tr in readings_with_errtype(trg, err_id, applies_deletion) {
            if verbose {
                tracing::debug!("tr.line=\t{}", tr.line);
            }
            if tr.added == AddedStatus::AddedBeforeBlank {
                if i == 0 {
                    tracing::warn!("Saw &ADDED-BEFORE-BLANK on initial word, ignoring");
                    continue;
                }
                let pretrg = &sentence.cohorts[i - 1];
                trg_beg = pretrg.pos + pretrg.form.len();
                added_before_blank = true;
            }
            if tr.added != AddedStatus::NotAdded {
                trg_end = trg_beg;
            }
            if verbose {
                tracing::debug!("r.wf='{}'", tr.wf);
                tracing::debug!("r.coerror={}", tr.coerror);
                tracing::debug!("r.suggestwf={}", tr.suggestwf);
                tracing::debug!("r.suggest={}\t{}", tr.suggest, tr.line);
            }
            if !del {
                for sf in &tr.sforms {
                    let form_with_casing = with_casing(tr.fixedcase, casing.clone(), sf);
                    rep_this_trg.push(form_with_casing.clone());
                    if tr.suggestwf {
                        if i == i_c {
                            reps_suggestwf.push(form_with_casing.clone());
                        } else {
                            tracing::warn!("Saw &SUGGESTWF on non-central (co-)cohort, ignoring");
                        }
                    }
                    if verbose {
                        tracing::debug!("\t\tsform=\t'{}'", sf);
                    }
                }
            }
        }
        beg = std::cmp::min(beg, trg_beg);
        end = std::cmp::max(end, trg_end);
        let mut reps_next = vec![];
        for rep in &reps {
            let pre_blank = if i == i_left || added_before_blank {
                String::new()
            } else {
                clean_blank(&format!("{}{}", prev_added_before_blank, trg.raw_pre_blank))
            };
            if rep_this_trg.is_empty() {
                reps_next.push(format!("{}{}{}", rep, pre_blank, trg.form));
            } else {
                for form in &rep_this_trg {
                    reps_next.push(format!("{}{}{}", rep, pre_blank, form));
                }
            }
        }
        reps = reps_next;
        prev_added_before_blank = if added_before_blank {
            trg.raw_pre_blank.clone()
        } else {
            String::new()
        };
    }
    for rep in &mut reps {
        *rep = rep.split_whitespace().collect::<Vec<&str>>().join(" ");
        rep.truncate(rep.trim_end().len());
        rep.drain(..rep.trim_start().len());
    }
    if verbose {
        for sf in &reps {
            tracing::debug!("reps sf=\t'{}'\t{},{}", sf, beg, end);
        }
    }
    Some((
        (beg, end),
        if reps_suggestwf.is_empty() {
            reps
        } else {
            reps_suggestwf
        },
    ))
}

type ToggleIds = HashMap<ErrId, Msg>;
type ToggleRes = Vec<(ErrRe, Msg)>;
type MsgMap = HashMap<Lang, (ToggleIds, ToggleRes)>;

fn clean_blank(raw: &str) -> String {
    let mut escaped = false;
    let mut bol = true; // at beginning of line
    let mut text = String::new();
    for c in raw.chars() {
        if bol && c == ':' {
            bol = false; // skip initial :
        } else if escaped {
            if c == 'n' {
                text.push('\n');
            } else {
                text.push(c);
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c != '[' && c != ']' {
            text.push(c);
            escaped = false;
        } else if c == '\n' {
            text.push(c);
            bol = true;
        }
    }
    text
}
fn expand_errs(errs: &mut Vec<Err>, text: &str) {
    if errs.len() < 2 {
        return;
    }
    // First expand "backwards" towards errors with lower beg's:
    errs.sort_unstable_by_key(|e| e.beg);
    for i in 1..errs.len() {
        let (left, right) = errs.split_at_mut(i);
        let e = &mut right[0];
        for f in left.iter().rev() {
            if f.beg < e.beg && f.end >= e.beg {
                let len = e.beg - f.beg;
                let add = &text[f.beg..e.beg];
                e.form = format!("{}{}", add, e.form);
                e.beg -= len;
                e.rep.iter_mut().for_each(|r| *r = format!("{}{}", add, r));
            }
        }
    }
    // Then expand "forwards" towards errors with higher end's:
    errs.sort_unstable_by_key(|e| e.end);
    for i in (0..errs.len() - 1).rev() {
        let (left, right) = errs.split_at_mut(i + 1);
        let e = &mut left[i];
        for f in right.iter() {
            if f.end > e.end && f.beg <= e.end {
                let len = f.end - e.end;
                let add = &text[e.end..f.end];
                e.form = format!("{}{}", e.form, add);
                e.end += len;
                e.rep.iter_mut().for_each(|r| *r = format!("{}{}", r, add));
            }
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum RunState {
    #[default]
    Flushing,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunMode {
    RunCg,
    RunJson,
    RunAutoCorrect,
}

fn sort_message_langs(msgs: &MsgMap, prefer: &str) -> Vec<String> {
    let mut sorted: Vec<String> = msgs.keys().cloned().collect();
    sorted.sort_by(|a, b| {
        if a == prefer {
            std::cmp::Ordering::Less
        } else if b == prefer {
            std::cmp::Ordering::Greater
        } else {
            a.cmp(b)
        }
    });
    sorted
}

struct Suggester {
    pub msgs: MsgMap,
    pub locale: String,

    sortedmsglangs: Vec<String>, // invariant: contains all and only the keys of msgs
    generator: Arc<hfst::Transducer>,
    ignores: HashSet<String>,
    includes: HashSet<String>,
    delimiters: HashSet<String>, // run_sentence(NulAndDelimiters) will return after seeing a cohort with one of these forms
    hard_limit: usize, // run_sentence(NulAndDelimiters) will always flush after seeing this many cohorts
    generate_all_readings: bool,
    verbose: bool,
}

impl Suggester {
    pub fn new(
        generator: Arc<hfst::Transducer>,
        msgs: MsgMap,
        locale: String,
        verbose: bool,
        generate_all_readings: bool,
    ) -> Self {
        Suggester {
            sortedmsglangs: sort_message_langs(&msgs, &locale),
            msgs,
            locale: locale.clone(),
            generator,
            delimiters: default_delimiters(),
            generate_all_readings,
            verbose,
            hard_limit: 1000,
            ignores: HashSet::new(),
            includes: HashSet::new(),
        }
    }

    fn run<W: Write>(&self, reader: &str, writer: &mut W, mode: RunMode) {
        tracing::debug!("run");
        let sentence = self.run_sentence(reader, FlushOn::Nul);

        tracing::debug!("{:?}", sentence);

        match mode {
            RunMode::RunJson => {
                let json = serde_json::to_string(&sentence.errs).unwrap();
                writer.write_all(json.as_bytes()).unwrap();
            }
            RunMode::RunAutoCorrect => {
                let mut offset = 0;
                let text = &sentence.text;
                for err in &sentence.errs {
                    if err.beg > offset {
                        writer.write_all(&text.as_bytes()[offset..err.beg]).unwrap();
                    }
                    if let Some(r) = err.rep.first() {
                        writer.write_all(r.as_bytes()).unwrap();
                    } else {
                        writer.write_all(err.form.as_bytes()).unwrap();
                    }
                    offset = err.end;
                }
                writer.write_all(&text.as_bytes()[offset..]).unwrap();
            }
            RunMode::RunCg => {
                // ... Implement the CG mode output here ...
            }
        }

        if sentence.runstate == RunState::Flushing {
            writer.write_all(&[0]).unwrap();
            writer.flush().unwrap();
        }
    }

    fn cohort_errs(
        &self,
        err_id: &str,
        i_c: usize,
        c: &Cohort,
        sentence: &Sentence,
        text: &str,
    ) -> Option<Err> {
        tracing::debug!("COHORT ERRS: {text:?}");
        if c.is_empty()
            || matches!(
                c.added,
                AddedStatus::AddedAfterBlank | AddedStatus::AddedBeforeBlank
            )
        {
            return None;
        } else if self.ignores.contains(err_id) {
            return None;
        } else if !self.includes.is_empty() && !self.includes.contains(err_id) {
            return None;
        }

        // Begin set msg:
        let mut msg = (String::new(), String::new());
        for mlang in sort_message_langs(&self.msgs, &self.locale) {
            if msg.1.is_empty() && mlang != self.locale {
                tracing::warn!(
                    "No <description> for \"{}\" in xml:lang '{}', trying '{}'",
                    err_id,
                    self.locale,
                    mlang
                );
            }
            if let Some((toggle_ids, toggle_res)) = self.msgs.get(&mlang) {
                if let Some(m) = toggle_ids.get(err_id) {
                    msg = m.clone();
                } else {
                    for (re, m) in toggle_res {
                        if re.is_match(err_id) {
                            // Only consider full matches:
                            msg = m.clone();
                            break;
                        }
                    }
                }
            }
            if !msg.1.is_empty() {
                break;
            }
        }
        if msg.1.is_empty() {
            tracing::debug!(
                "WARNING: No <description> for \"{}\" in any xml:lang",
                err_id
            );
            msg.1 = err_id.to_string();
        }
        if msg.0.is_empty() {
            msg.0 = err_id.to_string();
        }
        // TODO: Make suitable structure on creating MsgMap instead?
        msg.0 = msg.0.replace("$1", &c.form);
        msg.1 = msg.1.replace("$1", &c.form);
        for r in &c.readings {
            if !r.errtypes.is_empty() && !r.errtypes.contains(err_id) {
                continue; // there is some other error on this reading
            }
            rel_on_match(
                &r.rels,
                &MSG_TEMPLATE_REL,
                sentence,
                |relname, _i_t, trg| {
                    msg.0 = msg.0.replace(relname, &trg.form);
                    msg.1 = msg.1.replace(relname, &trg.form);
                },
            );
        }
        // End set msg
        // Begin set beg, end, form, rep:
        let mut beg = c.pos;
        let mut end = c.pos + c.form.len();
        let mut rep = Vec::new();
        for r in &c.readings {
            if !r.errtypes.contains(err_id) {
                continue; // We consider sforms of &SUGGEST readings in build_squiggle_replacement
            }
            // If there are LEFT/RIGHT added relations, add suggestions with those concatenated to our form
            // TODO: What about our current suggestions of the same error tag? Currently just using wordform
            let squiggle = squiggle_bounds(&r.rels, sentence, i_c, c);
            if let Some((bounds, sforms)) = build_squiggle_replacement(
                r,
                err_id,
                i_c,
                c,
                sentence,
                beg,
                end,
                squiggle.0,
                squiggle.1,
                self.verbose,
            ) {
                beg = bounds.0;
                end = bounds.1;
                rep.extend(sforms);
            }
        }
        tracing::debug!("WE CRY HERE: {text:?}");
        // Avoid unchanging replacements:
        let form = &text[beg..end];
        rep.retain(|r| r != form);
        // No duplicates:
        rep.sort();
        rep.dedup();
        if !rep.is_empty() {
            msg.0 = msg.0.replace("€1", &rep[0]);
            msg.1 = msg.1.replace("€1", &rep[0]);
        }
        Some(Err {
            form: form.to_string(),
            beg,
            end,
            err: err_id.to_string(),
            msg,
            rep,
        })
    }

    fn mk_errs(&self, sentence: &mut Sentence) {
        tracing::debug!("mk_errs {:?}", sentence.text);
        let text = &sentence.text;
        let mut errs = vec![];
        let s = sentence.clone();
        for (i_c, c) in sentence.cohorts.iter_mut().enumerate() {
            // tracing::debug!("C? {:?}", c);
            let mut c_errtypes = HashSet::new();
            for r in &c.readings {
                if r.coerror {
                    // Needed for backwards-compatibility with `COERROR &errtag` readings
                    continue;
                }
                c_errtypes.extend(r.errtypes.iter());
            }
            for errtype in c_errtypes {
                if errtype.is_empty() {
                    continue;
                }
                // This clone is needed because weird mutable dancing.
                let err = self.cohort_errs(errtype, i_c, c, &s, text);
                if let Some(err) = err {
                    c.errs.push(err.clone());
                    errs.push(err);
                    // sentence.errs.push(err);
                }
            }
        }
        sentence.errs.extend(errs);
        expand_errs(&mut sentence.errs, &text);
    }

    fn run_sentence(&self, reader: &str, flush_on: FlushOn) -> Sentence {
        let input = cg3::Output::new(reader.trim());
        for line in input.iter() {
            let line = line.unwrap();
            match line {
                Block::Cohort(cohort) => {
                    // proc_reading(generator, line, generate_all_readings);
                    // println!("Cohort: {:?}", cohort.word_form);
                    // println!("Readings: {:?}", cohort.readings);
                    // for reading in cohort.readings {
                    //     println!("Reading: {:?}", reading);
                    // }
                    println!("cohort: {:?}", cohort);
                }
                Block::Escaped(text) => println!("escaped: {text:?}"),
                Block::Text(text) => println!("text: {text:?}"),
            }
        }
        // let mut pos = 0;
        // let mut c = Cohort::default();
        let mut sentence = Sentence::default();
        // sentence.runstate = RunState::Eof;

        // let mut line = String::new();
        // let mut raw_blank = String::new(); // for CG output format
        // let mut readinglines = String::new();
        // reader.read_line(&mut line).unwrap(); // TODO: Why do I need at least one read_line before writing after flushing?

        // tracing::debug!("LINE: {}", line);

        // loop {
        //     let result = CG_LINE.captures(&line);
        //     // tracing::debug!("{:?}", result);

        //     let result3 = result
        //         .as_ref()
        //         .and_then(|m| m.get(3))
        //         .map_or(0, |m| m.as_str().len());
        //     let result8 = result
        //         .as_ref()
        //         .and_then(|m| m.get(8))
        //         .map_or(0, |m| m.as_str().len());

        //     if !readinglines.is_empty() && // Reached end of readings
        //        (result.is_none() || (result3 <= 1 && result8 <= 1))
        //     {
        //         let reading = proc_reading(
        //             &self.generator,
        //             readinglines.trim(),
        //             self.generate_all_readings,
        //         );
        //         readinglines.clear();
        //         c.errtypes.extend(reading.errtypes.iter().cloned());
        //         c.coerrtypes.extend(reading.coerrtypes.iter().cloned());
        //         if reading.id != 0 {
        //             c.id = reading.id;
        //         }
        //         c.added = match reading.added {
        //             AddedStatus::NotAdded => c.added,
        //             _ => reading.added,
        //         };
        //         c.readings.push(reading);
        //         if flush_on == FlushOn::NulAndDelimiters {
        //             if self.delimiters.contains(&c.form) {
        //                 sentence.runstate = RunState::Flushing;
        //             }
        //             if sentence.cohorts.len() >= self.hard_limit {
        //                 // We only respect hard_limit when flushing on delimiters (for the Nul only case we assume the calling API ensures requests are of reasonable size)
        //                 tracing::warn!(
        //                     "Hard limit of {} cohorts reached - forcing break.",
        //                     self.hard_limit
        //                 );
        //                 sentence.runstate = RunState::Flushing;
        //             }
        //         }
        //     }

        //     if let Some(caps) = result {
        //         if caps.get(2).is_some() || caps.get(6).is_some() {
        //             // tracing::debug!("wordform: {:?}", caps.get(2).unwrap());
        //             // wordform or blank: reset Cohort
        //             c.pos = pos;
        //             if !c.is_empty() {
        //                 std::mem::swap(&mut c.raw_pre_blank, &mut raw_blank);
        //                 raw_blank.clear();
        //                 sentence.cohorts.push(c.clone());
        //                 if c.id != 0 {
        //                     sentence
        //                         .ids_cohorts
        //                         .insert(c.id, sentence.cohorts.len() - 1);
        //                 }
        //             }
        //             if c.added != AddedStatus::NotAdded {
        //                 pos += c.form.len();
        //                 sentence.text.push_str(&c.form);
        //             }
        //             c = Cohort::default();
        //         }

        //         if caps.get(2).is_some() {
        //             // wordform
        //             tracing::debug!("Setting wordform");
        //             c.form = caps.get(2).unwrap().as_str().to_string();
        //         } else if caps.get(3).is_some() {
        //             // reading
        //             tracing::debug!("Reading");
        //             readinglines.push_str(&line);
        //             readinglines.push('\n');
        //         } else if caps.get(6).is_some() {
        //             // blank
        //             raw_blank.push_str(&line);
        //             let blank = clean_blank(caps.get(6).unwrap().as_str());
        //             pos += blank.len();
        //             sentence.text.push_str(&blank);
        //         } else if caps.get(7).is_some() {
        //             // flush
        //             sentence.runstate = RunState::Flushing;
        //         } else if caps.get(8).is_some() {
        //             // traced removed reading
        //             c.trace_removed_readings.push_str(&line);
        //             c.trace_removed_readings.push('\n');
        //         }
        //         // Blank lines without the prefix don't go into text output!
        //     }

        //     if sentence.runstate == RunState::Flushing {
        //         break;
        //     }

        //     line.clear();
        //     if reader.read_line(&mut line).unwrap() == 0 {
        //         break;
        //     }
        //     tracing::debug!("LINE: {}", line);
        // }

        // if !readinglines.is_empty() {
        //     let reading = proc_reading(&self.generator, &readinglines, self.generate_all_readings);
        //     readinglines.clear();
        //     c.errtypes.extend(reading.errtypes.iter().cloned());
        //     c.coerrtypes.extend(reading.coerrtypes.iter().cloned());
        //     if reading.id != 0 {
        //         c.id = reading.id;
        //     }
        //     c.added = match reading.added {
        //         AddedStatus::NotAdded => c.added,
        //         _ => reading.added,
        //     };
        //     c.readings.push(reading);
        // }

        // c.pos = pos;
        // if !c.is_empty() {
        //     std::mem::swap(&mut c.raw_pre_blank, &mut raw_blank);
        //     raw_blank.clear();
        //     sentence.cohorts.push(c.clone());
        //     if c.id != 0 {
        //         sentence
        //             .ids_cohorts
        //             .insert(c.id, sentence.cohorts.len() - 1);
        //     }
        // }
        // if c.added != AddedStatus::NotAdded {
        //     pos += c.form.len();
        //     sentence.text.push_str(&c.form);
        // }
        // sentence.raw_final_blank = raw_blank;

        // self.mk_errs(&mut sentence);
        sentence
    }
}
// {"errs":[["badjel",33,39,"lex-bokte-not-badjel","lex-bokte-not-badjel",["bokte","bokteba","boktebahal","boktebahan","boktebai","bokteban","boktebas","boktebason","boktebat","boktebe","boktebehal","boktebehan","boktebeson","boktege","boktegen","bokteges","boktegis","boktehal","boktehan","boktemat","boktemis","boktenai","bokteson"],"lex-bokte-not-badjel"]],"text":"sáddejuvvot báhpirat interneahta badjel.\n"}
