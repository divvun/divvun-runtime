use super::super::{CommandRunner, Context, Input};
use crate::{ast, modules::Error, util::fluent_loader::FluentLoader};
use async_trait::async_trait;
use divvun_runtime_macros::rt_command;
use fluent_bundle::FluentArgs;
use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashSet;
use std::hash::Hash;
use std::io::Write;
use std::ops::Deref;
use std::{collections::HashMap, fs, sync::Arc};

#[derive(Deserialize)]
struct ErrorJsonEntry {
    id: Option<String>,
    re: Option<String>,
}

fn load_error_mappings(context: &Arc<Context>) -> Result<IndexMap<String, Vec<Id>>, Error> {
    // Try to find errors.json in the bundle
    let errors_json_path = context.extract_to_temp_dir("errors.json")?;

    if !errors_json_path.exists() {
        tracing::debug!("No errors.json found, using empty error mappings");
        return Ok(IndexMap::new());
    }

    let content = fs::read_to_string(&errors_json_path)
        .map_err(|e| Error(format!("Failed to read errors.json: {}", e)))?;

    let raw_mappings: IndexMap<String, Vec<ErrorJsonEntry>> = serde_json::from_str(&content)
        .map_err(|e| Error(format!("Failed to parse errors.json: {}", e)))?;

    let mut mappings = IndexMap::new();

    for (key, entries) in raw_mappings {
        let mut ids = Vec::new();
        for entry in entries {
            if let Some(explicit_id) = entry.id {
                ids.push(Id::Explicit(explicit_id));
            } else if let Some(regex_pattern) = entry.re {
                match Regex::new(&regex_pattern) {
                    Ok(regex) => ids.push(Id::Regex(regex)),
                    Err(e) => {
                        tracing::error!(
                            "Invalid regex pattern '{}' for key '{}': {}",
                            regex_pattern,
                            key,
                            e
                        );
                        continue;
                    }
                }
            }
        }
        mappings.insert(key, ids);
    }

    tracing::debug!("Loaded {} error mappings from errors.json", mappings.len());
    Ok(mappings)
}

#[derive(Debug, Clone)]
pub enum Id {
    Explicit(String),
    Regex(Regex),
}

impl PartialEq for Id {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Id::Explicit(a), Id::Explicit(b)) => a == b,
            (Id::Regex(a), Id::Regex(b)) => a.as_str() == b.as_str(),
            _ => false,
        }
    }
}

impl Eq for Id {}

impl Hash for Id {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Id::Explicit(value) => value.hash(state),
            Id::Regex(regex) => regex.as_str().hash(state),
        }
    }
}

impl Id {
    pub fn matches(&self, tag: &str) -> bool {
        match self {
            Id::Explicit(value) => value == tag,
            Id::Regex(regex) => regex.is_match(tag),
        }
    }
}

pub struct Suggest {
    _context: Arc<Context>,
    generator: Arc<hfst::Transducer>,
    fluent_loader: FluentLoader,
    error_mappings: Arc<IndexMap<String, Vec<Id>>>,
}

#[rt_command(
    module = "divvun",
    name = "suggest",
    input = [String],
    output = "Json",
    args = [model_path = "Path"],
    // assets = [
    //     required("errors.json"),
    //     required(r"errors-.*\.ftl")
    // ]
)]
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

        let model_path = context.extract_to_temp_dir(model_path)?;

        let generator = Arc::new(hfst::Transducer::new(model_path));

        // Always use errors-*.ftl pattern for loading Fluent files
        let fluent_loader = FluentLoader::new(context.clone(), "errors-*.ftl", "en")?;

        // Load error mappings from errors.json
        let error_mappings = Arc::new(load_error_mappings(&context)?);

        Ok(Arc::new(Self {
            _context: context,
            generator,
            fluent_loader,
            error_mappings,
        }) as _)
    }

    pub fn error_mappings(&self) -> &Arc<IndexMap<String, Vec<Id>>> {
        &self.error_mappings
    }

    pub fn error_preferences(&self, language_tags: &[&str]) -> IndexMap<String, String> {
        let mut prefs = IndexMap::new();

        for key in self.error_mappings.keys() {
            let mut best_msg: Option<String> = None;
            for lang in language_tags {
                match self.fluent_loader.get_message(Some(&lang), key, None) {
                    Ok((title, _)) => {
                        best_msg = Some(title);
                    }
                    Err(_) => {
                        continue;
                    }
                }
            }

            if best_msg.is_none() {
                // Try default language
                match self.fluent_loader.get_message(None, key, None) {
                    Ok((title, _)) => {
                        best_msg = Some(title);
                    }
                    Err(_) => {}
                }
            }

            if let Some(msg) = best_msg {
                prefs.insert(key.clone(), msg);
            } else {
                prefs.insert(key.clone(), key.clone());
            }
        }

        prefs
    }
}

#[async_trait]
impl CommandRunner for Suggest {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.try_into_string()?;
        // let input = cg3::Output::new(&input);

        // Check config for locales array
        let locale = if let Some(locales_array) = config.get("locales").and_then(|v| v.as_array()) {
            // Extract locale strings from the array
            let locales: Vec<String> = locales_array
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();

            // Find the first available locale from the prioritized list
            self.fluent_loader
                .find_first_available_locale(&locales)
                .unwrap_or_else(|| "en".to_string())
        } else {
            // No locales provided, use default
            "en".to_string()
        };

        let fluent_loader = self.fluent_loader.clone();
        let generator = self.generator.clone();
        let error_mappings = self.error_mappings.clone();

        let output = tokio::task::spawn_blocking(move || {
            let encoding = config.get("encoding").and_then(|v| v.as_str());
            let ignores =
                if let Some(ignore_tags_array) = config.get("ignore").and_then(|v| v.as_array()) {
                    let ignore_tags = ignore_tags_array
                        .iter()
                        .filter_map(|v| v.as_str())
                        .filter_map(|s| error_mappings.get(s))
                        .map(|ids| ids.clone())
                        .flatten()
                        .collect::<HashSet<Id>>();
                    Some(ignore_tags)
                } else {
                    None
                };

            let suggester = Suggester::new(
                generator,
                locale,
                false,
                &fluent_loader,
                ignores.map(IdSet),
                None,
            );

            let mut writer = std::io::BufWriter::new(Vec::new());

            suggester.run(&input, &mut writer, encoding);
            writer.into_inner().unwrap()
        })
        .await
        .unwrap();

        let output = String::from_utf8(output).unwrap();

        Ok(output.into())
    }

    fn name(&self) -> &'static str {
        "divvun::suggest"
    }
}

static DELETE_REL: Lazy<Regex> = Lazy::new(|| Regex::new(r#"^DELETE[0-9]*$"#).unwrap());

static LEFT_RIGHT_DELETE_REL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^(LEFT|RIGHT|DELETE[0-9]*)$"#).unwrap());

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
    fixedcase: bool,      // don't change casing on suggestions if we have this tag
    drop_pre_blank: bool, // whether to drop the pre-blank of this cohort
    line: String,         // The (unchanged) input lines which created this Reading
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

fn proc_subreading(reading: &cg3::Reading, generate_all_readings: bool) -> Reading {
    let mut r = Reading::default();
    tracing::debug!("Subreading: {:?}", reading);

    let mut gentags: Vec<String> = Vec::new(); // tags we generate with
    r.id = 0; // CG-3 id's start at 1, should be safe. Want sum types :-/
    r.wf = String::new();
    r.suggest = false || generate_all_readings;
    r.suggestwf = false;
    r.added = AddedStatus::NotAdded;
    r.coerror = false;
    r.fixedcase = false;
    let mut delete_self = false; // may be changed by DELETE tag

    for tag in reading.tags.iter() {
        tracing::debug!("Processing tag: {}", tag);
        if *tag == "&LINK" || *tag == "&COERROR" || *tag == "COERROR" {
            // &LINK and COERROR kept for backward-compatibility
            r.coerror = true;
        } else if *tag == "DROP-PRE-BLANK" {
            r.drop_pre_blank = true;
        } else if *tag == "&SUGGEST" || *tag == "SUGGEST" || *tag == "@SUGGEST" {
            // &SUGGEST kept for backward-compatibility
            r.suggest = true;
            tracing::debug!("Set r.suggest = true for tag: {}", tag);
        } else if *tag == "&SUGGESTWF" || *tag == "SUGGESTWF" || *tag == "@SUGGESTWF" {
            // &SUGGESTWF kept for backward-compatibility
            r.suggestwf = true;
        } else if *tag == "&ADDED" || *tag == "ADDED" {
            r.added = AddedStatus::AddedAfterBlank; // C++ uses AddedEnsureBlanks, but we'll use AddedAfterBlank
        } else if *tag == "&ADDED-AFTER-BLANK" || *tag == "ADDED-AFTER-BLANK" {
            r.added = AddedStatus::AddedAfterBlank;
        } else if *tag == "&ADDED-BEFORE-BLANK" || *tag == "ADDED-BEFORE-BLANK" {
            r.added = AddedStatus::AddedBeforeBlank;
        } else if *tag == "DELETE" {
            // Shorthand: the tag DELETE means R:DELETE:id_of_this_cohort
            delete_self = true;
        } else if tag.starts_with("ID:") {
            if let Ok(id) = tag[3..].parse::<u32>() {
                r.id = id;
            } else {
                tracing::warn!("Couldn't parse ID integer: {}", tag);
            }
        } else if tag.starts_with("R:") {
            let parts: Vec<&str> = tag[2..].split(':').collect();
            if parts.len() >= 2 {
                if let Ok(target) = parts[1].parse::<u32>() {
                    r.rels.insert(parts[0].to_string(), target);
                } else {
                    tracing::warn!("Couldn't parse relation target integer: {}", tag);
                }
            }
        } else if tag.starts_with("\"") && tag.ends_with("\"S") && tag.len() > 2 {
            // Reading word-form: "form"S
            r.wf = tag[1..tag.len() - 2].to_string();
        } else if *tag == "<fixedcase>" {
            r.fixedcase = true;
        } else if tag.starts_with("\"<") && tag.ends_with(">\"") && tag.len() > 3 {
            // Broken word-form from MWE-split: "<form>"
            r.wf = tag[2..tag.len() - 2].to_string();
        } else if tag.starts_with("co&")
            || tag.starts_with("cO&")
            || tag.starts_with("Co&")
            || tag.starts_with("CO&")
        {
            // COERROR errtype (case insensitive)
            r.coerrtypes.insert(tag[3..].to_string());
        } else if tag.starts_with("&") {
            // Regular errtype
            r.errtypes.insert(tag[1..].to_string());
        } else if tag.starts_with("#")
            || tag.starts_with("@")
            || tag.starts_with("Sem/")
            || tag.starts_with("§")
            || tag.starts_with("<")
            || tag.starts_with("ADD:")
            || tag.starts_with("PROTECT:")
            || tag.starts_with("UNPROTECT:")
            || tag.starts_with("MAP:")
            || tag.starts_with("REPLACE:")
            || tag.starts_with("SELECT:")
            || tag.starts_with("REMOVE:")
            || tag.starts_with("IFF:")
            || tag.starts_with("APPEND:")
            || tag.starts_with("SUBSTITUTE:")
            || tag.starts_with("REMVARIABLE:")
            || tag.starts_with("SETVARIABLE:")
            || tag.starts_with("DELIMIT:")
            || tag.starts_with("MATCH:")
            || tag.starts_with("SETPARENT:")
            || tag.starts_with("SETCHILD:")
            || tag.starts_with("ADDRELATION")
            || tag.starts_with("SETRELATION")
            || tag.starts_with("REMRELATION")
            || tag.starts_with("ADDRELATIONS")
            || tag.starts_with("SETRELATIONS")
            || tag.starts_with("REMRELATIONS")
            || tag.starts_with("MOVE:")
            || tag.starts_with("MOVE-AFTER:")
            || tag.starts_with("MOVE-BEFORE:")
            || tag.starts_with("SWITCH:")
            || tag.starts_with("REMCOHORT:")
            || tag.starts_with("UNMAP:")
            || tag.starts_with("COPY:")
            || tag.starts_with("ADDCOHORT")
            || tag.starts_with("EXTERNAL")
            || tag.starts_with("REOPEN-MAPPINGS:")
        {
            // These are CG meta-tags that don't go into the generator
            // Skip them
        } else {
            // Regular morphological tag - add to generator tags
            gentags.push(tag.to_string());
        }
    }

    if delete_self {
        r.rels.insert("DELETE".to_string(), r.id);
    }

    let tagsplus = gentags.join("+");
    r.ana = format!("{}+{}", reading.base_form, tagsplus);
    tracing::debug!("Built analysis: {}", r.ana);
    tracing::debug!("r.suggest = {}, r.suggestwf = {}", r.suggest, r.suggestwf);
    if r.suggestwf {
        r.sforms.push(r.wf.clone());
    }
    r
}

fn proc_reading(
    generator: &hfst::Transducer,
    cohort: &cg3::Cohort,
    generate_all_readings: bool,
) -> Reading {
    let mut subs = Vec::new();
    for reading in &cohort.readings {
        subs.push(proc_subreading(reading, generate_all_readings));
    }

    let mut r = Reading::default();
    let n_subs = subs.len();

    for (i, sub) in subs.iter().enumerate() {
        r.ana += &sub.ana;
        if i + 1 != n_subs {
            r.ana.push('#');
        }
        r.errtypes.extend(sub.errtypes.clone());
        r.coerrtypes.extend(sub.coerrtypes.clone());
        r.rels.extend(sub.rels.clone());
        // higher sub can override id if set; doesn't seem like cg3 puts ids on them though
        if r.id == 0 {
            r.id = sub.id;
        }
        r.suggest = r.suggest || sub.suggest || generate_all_readings;
        r.suggestwf = r.suggestwf || sub.suggestwf;
        r.coerror = r.coerror || sub.coerror;
        r.added = if r.added == AddedStatus::NotAdded {
            sub.added
        } else {
            r.added
        };
        r.sforms.extend(sub.sforms.clone());
        if r.wf.is_empty() {
            r.wf = sub.wf.clone();
        }
        r.fixedcase |= sub.fixedcase;
        r.drop_pre_blank |= sub.drop_pre_blank;
    }

    // HashMap naturally deduplicates by key, so no explicit dedupe needed

    // Generate suggestions for individual subreadings that have suggest=true
    for (i, sub) in subs.iter().enumerate() {
        if sub.suggest {
            tracing::debug!("Generating suggestions for subreading {}: {}", i, sub.ana);

            // If the analysis contains "?" (unknown), try different strategies
            let mut paths = generator.lookup_tags(&sub.ana, false);
            if paths.is_empty() && sub.ana.contains("+?") {
                tracing::debug!("Analysis contains +?, trying base form only");
                // Try with just the base form part (everything before the first +)
                if let Some(base_form_pos) = sub.ana.find('+') {
                    let base_form = &sub.ana[..base_form_pos];
                    tracing::debug!("Trying base form lookup: {}", base_form);
                    paths = generator.lookup_tags(base_form, false);
                    tracing::debug!("Base form lookup returned {} paths", paths.len());
                }
            }

            tracing::debug!("HFST lookup returned {} paths", paths.len());
            for path in paths {
                tracing::debug!("Adding suggestion: {}", path);
                r.sforms.push(path);
            }
        }
    }

    // Deduplicate suggestions
    r.sforms.sort();
    r.sforms.dedup();
    tracing::debug!("Total suggestions after deduplication: {}", r.sforms.len());

    r
}

type Msg = (String, String); // (title, description)
type ErrId = String;

#[derive(Debug, Default, Clone, serde::Serialize)]
struct Err {
    form: String,
    beg: usize,
    end: usize,
    err: ErrId,
    msg: Msg,
    rep: Vec<String>,
}

/// Output structure for JSON serialization with position encoding support
#[derive(Debug, Clone, serde::Serialize)]
struct ErrOutput {
    form: String,
    beg: usize,
    end: usize,
    err: String,
    msg: (String, String),
    rep: Vec<String>,
}

impl ErrOutput {
    /// Create ErrOutput from Err with byte positions
    fn from_err_bytes(err: &Err) -> Self {
        Self {
            form: err.form.clone(),
            beg: err.beg,
            end: err.end,
            err: err.err.clone(),
            msg: err.msg.clone(),
            rep: err.rep.clone(),
        }
    }

    /// Create ErrOutput from Err with UTF-16 positions
    fn from_err_utf16(err: &Err, text: &str) -> Self {
        Self {
            form: err.form.clone(),
            beg: byte_to_utf16_offset(text, err.beg),
            end: byte_to_utf16_offset(text, err.end),
            err: err.err.clone(),
            msg: err.msg.clone(),
            rep: err.rep.clone(),
        }
    }
}

#[derive(Debug, Default, Clone)]
struct Sentence {
    cohorts: Vec<Cohort>,
    ids_cohorts: HashMap<u32, usize>, // mapping from cohort relation id's to their position in Sentence.cohort vector
    text: String,
    // runstate: RunState,
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
        |_relname, i_trg, trg| {
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
    let trg_errtypes_w_co: HashSet<String> = trg.errtypes.union(&trg.coerrtypes).cloned().collect();
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

    for c in input.chars() {
        if c.is_uppercase() {
            if seen_lower || seen_upper {
                non_fst_upper = true;
            } else {
                fst_upper = true;
            }
            seen_upper = true;
        }
        if c.is_lowercase() {
            seen_lower = true;
        }
    }

    if !seen_upper && !seen_lower {
        Casing::Mixed // No letters found, preserve original casing
    } else if !seen_upper {
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
        Casing::Title => {
            let mut chars: Vec<char> = input.chars().collect();
            if let Some(first_alpha_pos) = chars.iter().position(|c| c.is_alphabetic()) {
                chars[first_alpha_pos] = chars[first_alpha_pos]
                    .to_uppercase()
                    .next()
                    .unwrap_or(chars[first_alpha_pos]);
            }
            chars.into_iter().collect()
        }
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
) -> Option<((usize, usize), Vec<String>)> {
    let mut beg = orig_beg;
    let mut end = orig_end;
    let mut deletions = HashSet::new();
    let mut src_applies_deletion = false;
    rel_on_match(&r.rels, &DELETE_REL, sentence, |_relname, _i_t, trg| {
        deletions.insert(trg.id);
        if trg.errtypes.contains(err_id) || trg.coerrtypes.contains(err_id) {
            src_applies_deletion = true;
        }
    });

    // let mut add = HashMap::new();
    tracing::trace!("=== err_id=\t{} ===", err_id);
    tracing::trace!("r.id=\t{}", r.id);
    tracing::trace!("src.id=\t{}", src.id);
    tracing::trace!("i_c=\t{}", i_c);
    tracing::trace!("left=\t{}", i_left);
    tracing::trace!("right=\t{}", i_right);

    let mut reps = vec![String::new()];
    let mut prev_added_before_blank = String::new();
    for i in i_left..=i_right {
        let trg = &sentence.cohorts[i];
        let casing = get_casing(&trg.form);
        tracing::debug!("Form: '{}' detected casing: {:?}", trg.form, casing);

        tracing::trace!("i=\t{}", i);
        tracing::trace!("trg.form=\t'{}'", trg.form);
        tracing::trace!("trg.id=\t{}", trg.id);
        tracing::trace!("trg.raw_pre_blank=\t'{}'", trg.raw_pre_blank);

        let mut rep_this_trg = vec![];
        let del = do_delete(trg, err_id, &src.errtypes, &deletions);
        if del {
            rep_this_trg.push(String::new());
            tracing::trace!("\t\tdelete=\t{}", trg.form);
        }
        let mut added_before_blank = false;
        let applies_deletion = trg.id == src.id && src_applies_deletion;
        let mut trg_beg = trg.pos;
        let mut trg_end = trg.pos + trg.form.len();
        for tr in readings_with_errtype(trg, err_id, applies_deletion) {
            tracing::trace!("tr.line=\t{}", tr.line);

            if tr.added == AddedStatus::AddedBeforeBlank {
                if i == 0 {
                    tracing::warn!("Saw &ADDED-BEFORE-BLANK on initial word, ignoring");
                    continue;
                }
                let pretrg = &sentence.cohorts[i - 1];
                trg_beg = pretrg.pos + pretrg.form.len();
                added_before_blank = true;
            }
            added_before_blank |= tr.drop_pre_blank;
            if tr.added != AddedStatus::NotAdded {
                trg_end = trg_beg;
            }

            tracing::trace!("r.wf='{}'", tr.wf);
            tracing::trace!("r.coerror={}", tr.coerror);
            tracing::trace!("r.suggestwf={}", tr.suggestwf);
            tracing::trace!("r.suggest={}\t{}", tr.suggest, tr.line);

            if !del {
                tracing::debug!("tr.sforms has {} suggestions", tr.sforms.len());
                for sf in &tr.sforms {
                    tracing::debug!(
                        "Original suggestion: '{}', casing: {:?}, fixedcase: {}",
                        sf,
                        casing,
                        tr.fixedcase
                    );
                    let form_with_casing = with_casing(tr.fixedcase, casing.clone(), sf);
                    tracing::debug!("After casing: '{}'", form_with_casing);
                    rep_this_trg.push(form_with_casing.clone());

                    tracing::trace!("\t\tsform=\t'{}'", sf);
                }
            }
        }
        beg = std::cmp::min(beg, trg_beg);
        end = std::cmp::max(end, trg_end);
        let mut reps_next = vec![];
        for rep in &reps {
            // Prepend blank unless at left edge or we have a drop_pre_blank/added_before_blank condition
            let pre_blank = if i == i_left || added_before_blank {
                String::new()
            } else {
                clean_blank(&format!("{}{}", prev_added_before_blank, trg.raw_pre_blank))
            };
            if rep_this_trg.is_empty() {
                // Check if the current rep already contains this form to avoid duplication
                let would_append = format!("{}{}{}", rep, pre_blank, trg.form);
                let already_contains = rep
                    .trim_end()
                    .to_lowercase()
                    .ends_with(&trg.form.to_lowercase());
                if !already_contains {
                    reps_next.push(would_append);
                } else {
                    // The suggestion already covers this cohort, don't append
                    reps_next.push(rep.clone());
                }
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
    tracing::debug!("Reps: {:?}", reps);
    for rep in &mut reps {
        *rep = rep.split_whitespace().collect::<Vec<&str>>().join(" ");
        rep.truncate(rep.trim_end().len());
        let trimmed_start = rep.trim_start();
        let trim_amount = rep.len() - trimmed_start.len();
        rep.drain(..trim_amount);
    }
    for sf in &reps {
        tracing::debug!("reps sf=\t'{}'\t{},{}", sf, beg, end);
    }
    Some(((beg, end), reps))
}

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
fn demote_error_to_coerror(
    source: &Cohort,
    target_errtypes: &mut HashSet<String>,
    target_coerrtypes: &mut HashSet<String>,
) {
    for errtype in &source.errtypes {
        if target_errtypes.remove(errtype) {
            target_coerrtypes.insert(errtype.clone());
        }
    }
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

/// Convert a byte offset to a UTF-16 code unit offset
fn byte_to_utf16_offset(text: &str, byte_offset: usize) -> usize {
    text[..byte_offset].encode_utf16().count()
}

#[repr(transparent)]
#[derive(Default)]
struct IdSet(pub HashSet<Id>);

impl Deref for IdSet {
    type Target = HashSet<Id>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl IdSet {
    pub fn new() -> Self {
        IdSet(HashSet::new())
    }

    fn matches(&self, id_str: &str) -> bool {
        self.0.iter().any(|x| x.matches(id_str))
    }
}

struct Suggester<'a> {
    pub locale: String,
    pub fluent_loader: &'a FluentLoader,

    generator: Arc<hfst::Transducer>,
    ignores: IdSet,
    includes: IdSet,
    delimiters: HashSet<String>, // run_sentence(NulAndDelimiters) will return after seeing a cohort with one of these forms
    hard_limit: usize, // run_sentence(NulAndDelimiters) will always flush after seeing this many cohorts
    generate_all_readings: bool,
}

impl<'a> Suggester<'a> {
    pub fn new(
        generator: Arc<hfst::Transducer>,
        locale: String,
        generate_all_readings: bool,
        fluent_loader: &'a FluentLoader,
        ignores: Option<IdSet>,
        includes: Option<IdSet>,
    ) -> Self {
        Suggester {
            locale: locale.clone(),
            generator,
            delimiters: default_delimiters(),
            generate_all_readings,
            hard_limit: 1000,
            ignores: ignores.unwrap_or_default(),
            includes: includes.unwrap_or_default(),
            fluent_loader,
        }
    }

    fn run<W: Write>(&self, reader: &str, writer: &mut W, encoding: Option<&str>) {
        tracing::debug!("run with input: {:?}", reader);
        let sentence = self.run_sentence(reader, FlushOn::Nul);

        tracing::debug!(
            "Final sentence: cohorts={}, text={:?}, errs={}",
            sentence.cohorts.len(),
            sentence.text,
            sentence.errs.len()
        );

        let output_errs: Vec<ErrOutput> = if encoding == Some("utf-16") {
            sentence
                .errs
                .iter()
                .map(|err| ErrOutput::from_err_utf16(err, &sentence.text))
                .collect()
        } else {
            sentence
                .errs
                .iter()
                .map(|err| ErrOutput::from_err_bytes(err))
                .collect()
        };
        let json = serde_json::to_string(&output_errs).unwrap();
        writer.write_all(json.as_bytes()).unwrap();
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
        } else if self.ignores.matches(err_id) {
            return None;
        } else if !self.includes.is_empty() && !self.includes.matches(err_id) {
            return None;
        }

        // Use FluentLoader for message resolution
        let mut args = FluentArgs::new();
        args.set("1", c.form.as_str());

        let mut msg = match self
            .fluent_loader
            .get_message(Some(&self.locale), err_id, Some(&args))
        {
            Ok((title, desc)) => (title, desc),
            Err(_) => {
                // Fallback to default locale if message not found
                match self.fluent_loader.get_message(None, err_id, Some(&args)) {
                    Ok((title, desc)) => (title, desc),
                    Err(_) => {
                        tracing::debug!("WARNING: No Fluent message for \"{}\"", err_id);
                        (err_id.to_string(), err_id.to_string())
                    }
                }
            }
        };
        // End set msg
        // Begin set beg, end, form, rep:
        let mut beg = c.pos;
        let mut end = c.pos + c.form.len();
        let mut rep = Vec::new();
        for r in &c.readings {
            if !r.errtypes.contains(err_id) {
                continue; // Only process readings with the error tag
            }
            // If there are LEFT/RIGHT added relations, add suggestions with those concatenated to our form
            // TODO: What about our current suggestions of the same error tag? Currently just using wordform
            let squiggle = squiggle_bounds(&r.rels, sentence, i_c, c);
            if let Some((bounds, sforms)) = build_squiggle_replacement(
                r, err_id, i_c, c, sentence, beg, end, squiggle.0, squiggle.1,
            ) {
                beg = bounds.0;
                end = bounds.1;
                rep.extend(sforms);
            }
        }

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

        // Preprocessing, demote target &error to co&error:
        // Sometimes input has &errortag on relation targets instead of
        // co&errortag. We allow that, but we should then treat it as a
        // co&errortag (since the relation source is the "main" error):

        // Collect all the relation targets that need error demotion
        let mut targets_to_demote = Vec::new();
        for i_c in 0..sentence.cohorts.len() {
            for reading in &sentence.cohorts[i_c].readings {
                for (rel_name, &target_id) in &reading.rels {
                    if LEFT_RIGHT_DELETE_REL.is_match(rel_name) {
                        if let Some(&i_t) = sentence.ids_cohorts.get(&target_id) {
                            if i_t < sentence.cohorts.len() && i_t != i_c {
                                targets_to_demote.push((i_t, i_c));
                            }
                        }
                    }
                }
            }
        }

        // Now apply the demotions
        for (i_t, i_c) in targets_to_demote {
            let (source_cohorts, target_cohorts) =
                sentence.cohorts.split_at_mut(std::cmp::max(i_t, i_c));
            let (source, target) = if i_c < i_t {
                (
                    &source_cohorts[i_c],
                    &mut target_cohorts[i_t - source_cohorts.len()],
                )
            } else {
                (
                    &target_cohorts[i_c - source_cohorts.len()],
                    &mut source_cohorts[i_t],
                )
            };

            demote_error_to_coerror(source, &mut target.errtypes, &mut target.coerrtypes);
            for reading in &mut target.readings {
                demote_error_to_coerror(source, &mut reading.errtypes, &mut reading.coerrtypes);
            }
        }

        // Now actually find and mark up all the errors and suggestions:
        let mut errs = vec![];
        let s = sentence.clone();
        for (i_c, c) in sentence.cohorts.iter_mut().enumerate() {
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
                }
            }
        }
        sentence.errs.extend(errs);
        // Postprocessing for overlapping errors:
        expand_errs(&mut sentence.errs, &text);
    }

    fn run_sentence(&self, reader: &str, flush_on: FlushOn) -> Sentence {
        let input = cg3::Output::new(reader.trim());
        let mut sentence = Sentence::default();
        let mut pos = 0;
        let mut raw_blank = String::new(); // Accumulated blank for next cohort
        let mut current_cohort: Option<Cohort> = None; // Current cohort being built (delayed save pattern)
        let mut reading_lines = String::new(); // For multi-line readings

        tracing::debug!("run_sentence with reader length: {}", reader.len());

        for block in input.iter() {
            let block = match block {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("Error parsing CG3 output: {}", e);
                    continue;
                }
            };

            match block {
                cg3::Block::Cohort(cg_cohort) => {
                    tracing::debug!("Processing cohort: {:?}", cg_cohort.word_form);

                    // Save the previous cohort if we have one (delayed save pattern)
                    if let Some(mut cohort) = current_cohort.take() {
                        cohort.pos = pos;

                        // Swap accumulated blank into previous cohort (matching C++ pattern)
                        std::mem::swap(&mut cohort.raw_pre_blank, &mut raw_blank);
                        raw_blank.clear();

                        // Add cohort form to text and update position
                        if cohort.added == AddedStatus::NotAdded {
                            sentence.text.push_str(&cohort.form);
                            pos += cohort.form.len();
                        }

                        // Track ID mapping before pushing
                        if cohort.id != 0 {
                            sentence
                                .ids_cohorts
                                .insert(cohort.id, sentence.cohorts.len());
                        }
                        sentence.cohorts.push(cohort);
                    }

                    // Start building new cohort
                    current_cohort = Some(self.process_cohort(&cg_cohort, pos, String::new()));

                    // Check for flushing conditions
                    if flush_on == FlushOn::NulAndDelimiters {
                        if let Some(ref cohort) = current_cohort {
                            if self.delimiters.contains(&cohort.form) {
                                break;
                            }
                        }
                        if sentence.cohorts.len() >= self.hard_limit {
                            tracing::warn!(
                                "Hard limit of {} cohorts reached - forcing break.",
                                self.hard_limit
                            );
                            break;
                        }
                    }
                }
                cg3::Block::Text(text) => {
                    tracing::debug!("Accumulating text block: {:?}", text);
                    raw_blank.push_str(&text);

                    // Add to sentence text and update position
                    let clean = clean_blank(&text);
                    sentence.text.push_str(&clean);
                    pos += clean.len();
                }
                cg3::Block::Escaped(escaped) => {
                    tracing::debug!("Accumulating escaped block: {:?}", escaped);
                    raw_blank.push_str(&escaped);

                    // Add to sentence text and update position
                    let clean = clean_blank(&escaped);
                    sentence.text.push_str(&clean);
                    pos += clean.len();
                }
            }
        }

        // Save final cohort if we have one (matching C++ end-of-input handling)
        if let Some(mut cohort) = current_cohort {
            cohort.pos = pos;

            // Swap accumulated blank into final cohort
            std::mem::swap(&mut cohort.raw_pre_blank, &mut raw_blank);
            raw_blank.clear();

            // Add cohort form to text
            if cohort.added == AddedStatus::NotAdded {
                sentence.text.push_str(&cohort.form);
                pos += cohort.form.len();
            }

            // Track ID mapping before pushing
            if cohort.id != 0 {
                sentence
                    .ids_cohorts
                    .insert(cohort.id, sentence.cohorts.len());
            }
            sentence.cohorts.push(cohort);
        }

        // Store any remaining blank as final blank
        sentence.raw_final_blank = raw_blank;

        self.mk_errs(&mut sentence);
        sentence
    }

    fn process_cohort(&self, cg_cohort: &cg3::Cohort, pos: usize, raw_pre_blank: String) -> Cohort {
        let mut cohort = Cohort {
            form: cg_cohort.word_form.to_string(),
            pos,
            raw_pre_blank,
            ..Default::default()
        };

        // Process the cohort as a whole to get a single reading
        let reading = proc_reading(&self.generator, cg_cohort, self.generate_all_readings);

        // Accumulate error types from the reading
        cohort.errtypes.extend(reading.errtypes.iter().cloned());
        cohort.coerrtypes.extend(reading.coerrtypes.iter().cloned());

        // Use reading ID if cohort doesn't have one
        if reading.id != 0 {
            cohort.id = reading.id;
        }

        // Use reading added status
        cohort.added = reading.added;

        cohort.readings.push(reading);

        cohort
    }
}
