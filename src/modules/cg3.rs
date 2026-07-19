use std::{collections::HashMap, str::FromStr, sync::Arc, thread::JoinHandle};

use async_trait::async_trait;
use divvun_runtime_macros::{rt_command, rt_struct};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::sync::{
    Mutex,
    mpsc::{self, Receiver, Sender},
};

use crate::ast;

use super::{CommandRunner, Context, Error, PipelineValue, PipelineValues};

// ---------------------------------------------------------------------------
// Native VISL CG-3 engine adapters + stream parser.
//
// The `cg3` dependency is the pure-Rust VISL CG-3 port. It exposes a low-level
// engine (`GrammarApplicator` + `run_grammar_on_text`), not the string-in /
// string-out API this module (and the divvun/speech modules) drive. The
// `Applicator` (vislcg3) and `MweSplit` wrappers below adapt that engine, and
// `Output`/`Block`/`Cohort`/`Reading` parse a CG stream — a pure-Rust concern
// that never touched the old C++ FFI. Everything the rest of the crate needs
// lives here (see `crate::modules::cg3`).
// ---------------------------------------------------------------------------

/// Constraint Grammar 3 disambiguator engine (native VISL CG-3 port).
///
/// The parsed + reindexed grammar is loaded once and kept behind a `Mutex`;
/// each [`run`](Self::run) moves it into a fresh applicator and back out (the
/// engine accumulates per-run window state, and `Grammar` is not `Clone`).
pub struct Applicator {
    grammar: std::sync::Mutex<::cg3::grammar::Grammar>,
    trace: std::sync::atomic::AtomicBool,
}

impl Applicator {
    pub fn new<P: AsRef<std::path::Path>>(path: P) -> Self {
        let path = path.as_ref();
        let buffer = std::fs::read(path)
            .unwrap_or_else(|e| panic!("cg3: cannot read grammar {}: {e}", path.display()));
        Self::from_bytes(&buffer, &path.display().to_string())
    }

    fn from_bytes(buffer: &[u8], source: &str) -> Self {
        use ::cg3::binary_grammar::BinaryGrammar;
        use ::cg3::grammar::Grammar;
        use ::cg3::inlines::is_cg3b;
        use ::cg3::textual_parser::TextualParser;

        let mut grammar: Grammar = if is_cg3b(buffer) {
            let mut parser = BinaryGrammar::binary_grammar(Grammar::default());
            if !matches!(parser.parse_grammar_buffer(buffer), Ok(0)) {
                panic!("cg3: binary grammar {source} could not be parsed");
            }
            parser.grammar
        } else {
            let mut parser = TextualParser::new(Grammar::default(), false);
            if !matches!(parser.parse_grammar_utf8(buffer), Ok(0)) {
                panic!("cg3: textual grammar {source} could not be parsed");
            }
            parser.grammar
        };

        grammar
            .reindex(false, false)
            .unwrap_or_else(|_| panic!("cg3: reindex failed for {source}"));

        Self {
            grammar: std::sync::Mutex::new(grammar),
            trace: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn set_trace(&self, trace: bool) {
        self.trace
            .store(trace, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn run(&self, input: &str) -> Option<String> {
        use ::cg3::format_converter::FormatConverter;
        use ::cg3::grammar::Grammar;
        use ::cg3::grammar_applicator::{GrammarApplicator, cg3_sformat};
        use ::cg3::options::{OPTIONS, options};

        let mut guard = self.grammar.lock().unwrap();
        // Move the grammar into a fresh applicator; `set_grammar`'s tag seeding
        // is idempotent (`add_tag` interns), so reuse across runs is safe.
        let grammar = std::mem::replace(&mut *guard, Grammar::default());

        let base = GrammarApplicator::new(Grammar::default());
        let mut applicator = FormatConverter::new(base);
        applicator.base_mut().cfg.fmt_input = cg3_sformat::CG3SF_CG;
        applicator.base_mut().cfg.fmt_output = cg3_sformat::CG3SF_CG;
        applicator.base_mut().grammar = grammar;

        let mut opts = options();
        if self.trace.load(std::sync::atomic::Ordering::SeqCst) {
            opts[OPTIONS::TRACE as usize].does_occur = true;
        }

        let result = (|| {
            applicator.base_mut().set_grammar().ok()?;
            applicator.base_mut().set_options(&opts).ok()?;
            let mut cursor = std::io::Cursor::new(input.as_bytes().to_vec());
            let mut out: Vec<u8> = Vec::new();
            applicator.run_grammar_on_text(&mut cursor, &mut out).ok()?;
            String::from_utf8(out).ok()
        })();

        // Always reclaim the grammar for the next run, even on failure.
        *guard = std::mem::replace(&mut applicator.base_mut().grammar, Grammar::default());
        result
    }
}

/// Multi-word-expression splitter (native VISL CG-3 port). Builds its own
/// minimal dummy grammar, so a fresh applicator per run is cheap.
pub struct MweSplit {
    _private: (),
}

impl Default for MweSplit {
    fn default() -> Self {
        Self::new()
    }
}

impl MweSplit {
    pub fn new() -> Self {
        Self { _private: () }
    }

    pub fn run(&self, input: &str) -> Option<String> {
        use ::cg3::grammar::Grammar;
        use ::cg3::grammar_applicator::GrammarApplicator;
        use ::cg3::mwesplit_applicator::MweSplitApplicator;

        let base = GrammarApplicator::new(Grammar::default());
        let mut applicator = MweSplitApplicator::new(base);
        applicator.base.cfg.verbosity_level = 0;

        let mut cursor = std::io::Cursor::new(input.as_bytes().to_vec());
        let mut out: Vec<u8> = Vec::new();
        applicator.run_grammar_on_text(&mut cursor, &mut out).ok()?;
        String::from_utf8(out).ok()
    }
}

// --- Pure-Rust CG stream parser (ported from the old FFI wrapper's Rust side;
// never involved the C++ engine). `Output` iterates a CG stream into `Block`s;
// cohorts carry a `word_form` and tab-indented `Reading`s of `tags`. ---

#[derive(Debug, Clone)]
pub struct Output<'a> {
    buf: std::borrow::Cow<'a, str>,
}

#[derive(Debug, Clone)]
pub enum Line<'a> {
    WordForm(&'a str),
    Reading(&'a str),
    Text(&'a str),
}

#[derive(Debug, Clone)]
pub enum Block<'a> {
    Cohort(Cohort<'a>),
    Escaped(&'a str),
    Text(&'a str),
}

#[derive(Clone)]
pub struct Reading<'a> {
    pub raw_line: &'a str,
    pub base_form: &'a str,
    pub tags: Vec<&'a str>,
    pub depth: usize,
}

impl std::fmt::Debug for Reading<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let alt = f.alternate();

        let mut x = f.debug_struct("Reading");
        x.field("base_form", &self.base_form)
            .field("tags", &self.tags)
            .field("depth", &self.depth);

        if alt {
            x.field("raw_line", &self.raw_line).finish()
        } else {
            x.finish_non_exhaustive()
        }
    }
}

impl std::fmt::Display for Reading<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}\"{}\"{}",
            "\t".repeat(self.depth),
            self.base_form,
            self.tags.iter().fold(String::new(), |mut acc, tag| {
                acc.push(' ');
                acc.push_str(tag);
                acc
            })
        )
    }
}

#[derive(Debug, Clone)]
pub struct Cohort<'a> {
    pub word_form: &'a str,
    pub readings: Vec<Reading<'a>>,
}

impl std::fmt::Display for Cohort<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\"<{}>\"", self.word_form)?;
        for reading in &self.readings {
            writeln!(f, "{}", reading)?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Invalid input: line {line}, position {position}, expected {expected}")]
    InvalidInput {
        line: usize,
        position: usize,
        expected: &'static str,
    },
    #[error("Invalid line: {0}")]
    InvalidLine(String),
    #[error("Invalid reading: {0}")]
    InvalidReading(String),
}

impl std::fmt::Display for Output<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for block in self.iter() {
            match block {
                Ok(block) => write!(f, "{}", block)?,
                Err(_) => return Err(std::fmt::Error),
            }
        }
        Ok(())
    }
}

impl<'a> Output<'a> {
    pub fn new<S: Into<std::borrow::Cow<'a, str>>>(buf: S) -> Self {
        let buf = buf.into();
        Self { buf }
    }

    fn lines(&'a self) -> impl Iterator<Item = Line<'a>> {
        let mut lines = self.buf.lines();
        std::iter::from_fn(move || {
            for line in lines.by_ref() {
                return Some(if line.starts_with('"') {
                    Line::WordForm(line)
                } else if line.starts_with('\t') {
                    Line::Reading(line)
                } else {
                    Line::Text(line)
                });
            }
            None
        })
    }

    pub fn sentences(&'a self) -> impl Iterator<Item = Result<String, ParseError>> {
        let mut iter = self.iter();

        std::iter::from_fn(move || {
            let mut sentence = String::new();

            for block in iter.by_ref() {
                let block = match block {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };

                match block {
                    Block::Cohort(cohort) => {
                        sentence.push_str(
                            cohort
                                .readings
                                .first()
                                .map(|r| r.base_form)
                                .unwrap_or(cohort.word_form),
                        );
                        if cohort
                            .readings
                            .first()
                            // Rudimentary check for sentence end. We want to include '.', '?', '!', but not commas.
                            .map(|x| x.base_form != "," && x.tags.contains(&"CLB"))
                            .unwrap_or(false)
                        {
                            return Some(Ok(sentence.trim().to_string()));
                        }
                    }
                    Block::Escaped(text) => {
                        let text = text.replace("\\n", "\n");
                        sentence.push_str(&text);
                    }
                    Block::Text(_text) => {}
                }
            }

            if !sentence.is_empty() {
                return Some(Ok(sentence.trim().to_string()));
            }

            None
        })
    }

    pub fn iter(&'a self) -> impl Iterator<Item = Result<Block<'a>, ParseError>> {
        let mut lines = self.lines().peekable();
        let mut cohort = None;
        let mut text = std::collections::VecDeque::new();

        std::iter::from_fn(move || {
            loop {
                if cohort.is_none() {
                    if let Some(t) = text.pop_front() {
                        return Some(Ok(t));
                    }
                }

                let Some(line) = lines.peek() else {
                    if let Some(cohort) = cohort.take() {
                        return Some(Ok(Block::Cohort(cohort)));
                    }

                    return None;
                };

                let ret = loop {
                    match line {
                        Line::WordForm(x) => {
                            if let Some(cohort) = cohort.take() {
                                return Some(Ok(Block::Cohort(cohort)));
                            }

                            let (Some(start), Some(end)) = (x.find("\"<"), x.find(">\"")) else {
                                return Some(Err(ParseError::InvalidLine(x.to_string())));
                            };

                            let word_form = &x[start + 2..end];

                            cohort = Some(Cohort {
                                word_form,
                                readings: Vec::new(),
                            });

                            break None;
                        }
                        Line::Reading(x) => {
                            let Some(cohort) = cohort.as_mut() else {
                                break Some(Err(ParseError::InvalidReading(x.to_string())));
                            };

                            let Some(depth) = x.rfind('\t') else {
                                break Some(Err(ParseError::InvalidReading(x.to_string())));
                            };

                            let x = &x[depth + 1..];
                            let mut chunks = tokenize_tags(x).into_iter();

                            let base_form = match chunks
                                .next()
                                .ok_or_else(|| ParseError::InvalidReading(x.to_string()))
                            {
                                Ok(v) => v,
                                Err(e) => break Some(Err(e)),
                            };

                            if !(base_form.starts_with('"') && base_form.ends_with('"')) {
                                break Some(Err(ParseError::InvalidReading(x.to_string())));
                            }
                            let base_form = &base_form[1..base_form.len() - 1];

                            cohort.readings.push(Reading {
                                raw_line: x,
                                base_form,
                                tags: chunks.collect(),
                                depth: depth + 1,
                            });

                            break None;
                        }
                        Line::Text(x) => {
                            if let Some(rest) = x.strip_prefix(':') {
                                text.push_back(Block::Escaped(rest));
                            } else {
                                text.push_back(Block::Text(x));
                            }

                            break None;
                        }
                    }
                };

                lines.next();

                if let Some(ret) = ret {
                    return Some(ret);
                }
            }
        })
    }
}

impl std::fmt::Display for Block<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Block::Cohort(cohort) => {
                write!(f, "{}", cohort)
            }
            Block::Escaped(text) => writeln!(f, ":{}", text),
            Block::Text(text) => writeln!(f, "{}", text),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TokenizeState {
    None,
    Token,
    InString,
    EndOfString,
}

fn tokenize_tags(input: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut state = TokenizeState::None;
    let mut cur = 0;

    for (i, c) in input.char_indices() {
        if c == '"' {
            if matches!(state, TokenizeState::None) {
                state = TokenizeState::InString;
                cur = i;
            } else if matches!(state, TokenizeState::InString) {
                state = TokenizeState::EndOfString;
            }
            continue;
        }

        if matches!(state, TokenizeState::EndOfString) && c.is_whitespace() {
            state = TokenizeState::None;
            tokens.push(&input[cur..i]);
            cur = i + 1;
            continue;
        }

        if c.is_whitespace() {
            if matches!(state, TokenizeState::None) {
                cur = i + 1;
            }
            if matches!(state, TokenizeState::Token) {
                tokens.push(&input[cur..i]);
                cur = i + 1;
                state = TokenizeState::None;
            }
            continue;
        } else if matches!(state, TokenizeState::None) {
            state = TokenizeState::Token;
            continue;
        }
    }

    if !matches!(state, TokenizeState::None) {
        tokens.push(&input[cur..]);
    }

    tokens
}

/// CG3 stream command injector
#[derive(facet::Facet)]
pub struct StreamCmd {
    #[facet(opaque)]
    _context: Arc<Context>,
    key: String,
}

#[rt_command(
    module = "cg3",
    name = "streamcmd",
    input = [String],
    output = "String",
    kind = "cg3",
    args = [key = "String"]
)]
impl StreamCmd {
    async fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        let key = kwargs
            .remove("key")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error::msg("key missing").at("pipeline.json", "/args/key"))?;

        Ok(Arc::new(Self {
            _context: context,
            key,
        }) as _)
    }
}

#[async_trait]
impl CommandRunner for StreamCmd {
    async fn forward(
        self: Arc<Self>,
        input: PipelineValue,
        config: Arc<serde_json::Value>,
    ) -> Result<PipelineValues, crate::modules::Error> {
        let input = input.try_into_string()?;

        if self.key != "REMVAR" && self.key != "SETVAR" {
            return Ok(format!("<STREAMCMD:{}>\n{input}", self.key).into());
        }

        let value = match &*config {
            serde_json::Value::Null => {
                return Ok(input.into());
            }
            serde_json::Value::Bool(x) => x.to_string(),
            serde_json::Value::Number(x) => x.to_string(),
            serde_json::Value::String(x) => x.to_string(),
            serde_json::Value::Array(values) => {
                if values.is_empty() {
                    return Ok(input.into());
                }
                values
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            }
            serde_json::Value::Object(map) => {
                if map.is_empty() {
                    return Ok(input.into());
                }

                map.iter()
                    .map(|(k, v)| {
                        match v {
                            serde_json::Value::Null => k.to_string(),
                            serde_json::Value::Bool(x) => format!("{}={}", k, x),
                            serde_json::Value::Number(x) => format!("{}={}", k, x),
                            serde_json::Value::String(x) => format!("{}={}", k, x),
                            serde_json::Value::Array(arr) => {
                                let arr_str = arr
                                    .iter()
                                    .map(|x| x.to_string())
                                    .collect::<Vec<_>>()
                                    .join(",");
                                format!("{}=[{}]", k, arr_str)
                            }
                            serde_json::Value::Object(_) => {
                                // Nested objects are not supported in this context
                                k.to_string()
                            }
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            }
        };

        Ok(format!("<STREAMCMD:{}:{value}>\n{input}", self.key).into())
    }

    fn name(&self) -> &'static str {
        "cg3::streamcmd"
    }
}

/// Multi-word expression splitter
#[derive(facet::Facet)]
pub struct Mwesplit {
    #[facet(opaque)]
    _context: Arc<Context>,
    #[facet(opaque)]
    input_tx: Sender<Option<String>>,
    #[facet(opaque)]
    output_rx: Mutex<Receiver<Option<String>>>,
    #[facet(opaque)]
    _thread: JoinHandle<()>,
}

#[rt_command(
    module = "cg3",
    name = "mwesplit",
    input = [String],
    output = "String",
    kind = "cg3",
    args = []
)]
impl Mwesplit {
    pub async fn new(
        context: Arc<Context>,
        _kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        tracing::debug!("Creating mwesplit");
        let (input_tx, mut input_rx) = mpsc::channel(1);
        let (output_tx, output_rx) = mpsc::channel(1);

        let thread = std::thread::spawn(move || {
            tracing::debug!("init cg3 mwesplit BEFORE");
            let mwesplit = MweSplit::new();
            tracing::debug!("init cg3 mwesplit");

            loop {
                let Some(Some(input)): Option<Option<String>> = input_rx.blocking_recv() else {
                    break;
                };

                output_tx.blocking_send(mwesplit.run(&input)).unwrap();
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

#[derive(facet::Facet, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
enum SentenceMode {
    #[default]
    SurfaceForm,
    PhonologicalForm,
}

impl FromStr for SentenceMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "surface" => Ok(Self::SurfaceForm),
            "phonological" => Ok(Self::PhonologicalForm),
            _ => Err(()),
        }
    }
}

/// Extract sentences from CG3 stream
#[derive(facet::Facet)]
struct Sentences {
    mode: SentenceMode,
}

#[rt_command(
    module = "cg3",
    name = "sentences",
    input = [String],
    output = "ArrayString",
    args = [mode = "String"]
)]
impl Sentences {
    pub async fn new(
        _context: Arc<Context>,
        _kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        let mode = _kwargs
            .get("mode")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string())
            .and_then(|x| x.parse::<SentenceMode>().ok())
            .unwrap_or_default();
        Ok(Arc::new(Self { mode }))
    }
}

fn cohort_text<'a>(cohort: &Cohort<'a>, mode: SentenceMode) -> &'a str {
    match mode {
        SentenceMode::SurfaceForm => cohort.word_form,
        SentenceMode::PhonologicalForm => cohort
            .readings
            .first()
            .and_then(|r| {
                r.tags
                    .iter()
                    .find(|tag| tag.ends_with("\"phon"))
                    .map(|tag| &tag[1..tag.len() - 5])
            })
            .unwrap_or(cohort.word_form),
    }
}

fn after_break_ms(cohort: &Cohort<'_>) -> Option<u32> {
    for r in &cohort.readings {
        for t in &r.tags {
            if let Some(rest) = t
                .strip_prefix("<DRT-BREAK-AFTER:")
                .and_then(|s| s.strip_suffix('>'))
            {
                if let Ok(ms) = rest.parse::<u32>() {
                    return Some(ms);
                }
            }
        }
    }
    None
}

/// Walk a cohort's readings and collect every `<DRT-*:V>` tag (except
/// `<DRT-BREAK-AFTER>` which is consumed separately). Values are
/// percent-decoded. Keys are lowercased with hyphens preserved
/// (e.g. `DRT-PROSODY-RATE` → `prosody-rate`).
fn cohort_opts(cohort: &Cohort<'_>) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    for r in &cohort.readings {
        for t in &r.tags {
            let Some(inner) = t.strip_prefix("<DRT-").and_then(|s| s.strip_suffix('>')) else {
                continue;
            };
            let Some((name, value)) = inner.split_once(':') else {
                continue;
            };
            if name == "BREAK-AFTER" {
                continue;
            }
            let key = name.to_ascii_lowercase();
            let decoded = percent_decode(value);
            out.entry(key).or_insert(decoded);
        }
    }
    out
}

/// Percent-decode `%XX` sequences. Invalid sequences pass through verbatim.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// SSML 1.1 §3.2.4 `<prosody rate>` → multiplier on the synth's `pace`.
/// Accepts the raw SSML string (e.g. `"fast"`, `"200%"`). Returns None for
/// values we can't make sense of.
fn rate_to_pace_multiplier(rate: &str) -> Option<f32> {
    match rate {
        "x-slow" => Some(0.5),
        "slow" => Some(0.75),
        "medium" | "default" => Some(1.0),
        "fast" => Some(1.25),
        "x-fast" => Some(1.5),
        s if s.ends_with('%') => {
            let pct = s[..s.len() - 1].parse::<f32>().ok()?;
            if pct > 0.0 { Some(pct / 100.0) } else { None }
        }
        _ => None,
    }
}

fn format_sentence(text: String, opts: &std::collections::BTreeMap<String, String>) -> String {
    if opts.is_empty() {
        return text;
    }
    // Synthesise `pace=F` from `prosody-rate` for `speech.tts` to consume.
    // The raw `prosody-rate` is also preserved for fidelity / other consumers.
    let mut kvs: Vec<(String, String)> = Vec::with_capacity(opts.len() + 1);
    if let Some(rate) = opts.get("prosody-rate") {
        if let Some(p) = rate_to_pace_multiplier(rate) {
            kvs.push(("pace".to_string(), p.to_string()));
        }
    }
    for (k, v) in opts {
        kvs.push((k.clone(), v.clone()));
    }
    let body = kvs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(";");
    format!("\x1FOPTS:{body}\x1F{text}")
}

fn extract_sentences(
    input: &str,
    mode: SentenceMode,
    breakers: &std::collections::HashSet<String>,
) -> Vec<String> {
    let output = Output::new(input);
    let mut sentences: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut current_opts: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();

    for block in output.iter().filter_map(Result::ok) {
        if let Block::Cohort(cohort) = block {
            // Capture the opts from the first cohort of each sentence.
            if current.is_empty() {
                current_opts = cohort_opts(&cohort);
            }
            current.push(cohort_text(&cohort, mode).to_string());
            if let Some(ms) = after_break_ms(&cohort) {
                sentences.push(format_sentence(current.join(" "), &current_opts));
                current.clear();
                current_opts.clear();
                sentences.push(format!("\x1FBREAK:{ms}\x1F"));
                continue;
            }
            if super::cg3_util::is_sentence_boundary(&cohort, breakers) {
                sentences.push(format_sentence(current.join(" "), &current_opts));
                current.clear();
                current_opts.clear();
            }
        }
    }

    if !current.is_empty() {
        sentences.push(format_sentence(current.join(" "), &current_opts));
    }

    sentences
}

#[async_trait]
impl CommandRunner for Sentences {
    async fn forward(
        self: Arc<Self>,
        input: PipelineValue,
        _config: Arc<serde_json::Value>,
    ) -> Result<PipelineValues, crate::modules::Error> {
        let input = input.try_into_string()?;
        let breakers = super::cg3_util::default_sentence_breakers();
        tracing::debug!("Processing sentences in mode: {:?}", self.mode);
        let sentences = extract_sentences(&input, self.mode, &breakers);
        Ok(sentences.into())
    }

    // fn forward_stream(
    //     self: Arc<Self>,
    //     mut input: PipelineValueRx,
    //     mut output: PipelineValueTx,
    //     config: Arc<serde_json::Value>,
    // ) -> tokio::task::JoinHandle<Result<(), Error>>
    // where
    //     Self: Send + Sync + 'static,
    // {
    //     let this = self.clone();
    //     tokio::spawn(async move {
    //         loop {
    //             let event = input.recv().await.map_err(|e| Error(e.to_string()))?;
    //             let this = this.clone();
    //             match event {
    //                 PipelineEvent::Value(input) => {
    //                     tracing::debug!("INPUT: {:?}", input);
    //                     let event = match this.forward(input, config.clone()).await {
    //                         Ok(event) => event,
    //                         Err(e) => {
    //                             output
    //                                 .send(PipelineEvent::Error(e.clone()))
    //                                 .map_err(|e| Error(e.to_string()))?;
    //                             return Err(e);
    //                         }
    //                     };
    //                     let x = event.try_into_string_array().unwrap();
    //                     for x in x {
    //                         tracing::debug!("SEND OUTPUT: {:?}", x);
    //                         output
    //                             .send(PipelineEvent::Value(PipelineValue::String(x)))
    //                             .map_err(|e| Error(e.to_string()))?;
    //                     }
    //                     output
    //                         .send(PipelineEvent::Finish)
    //                         .map_err(|e| Error(e.to_string()))?;
    //                 }
    //                 PipelineEvent::Finish => {
    //                     output
    //                         .send(PipelineEvent::Finish)
    //                         .map_err(|e| Error(e.to_string()))?;
    //                 }
    //                 PipelineEvent::Error(e) => {
    //                     output
    //                         .send(PipelineEvent::Error(e.clone()))
    //                         .map_err(|e| Error(e.to_string()))?;
    //                     return Err(e);
    //                 }
    //                 PipelineEvent::Close => {
    //                     output
    //                         .send(PipelineEvent::Close)
    //                         .map_err(|e| Error(e.to_string()))?;
    //                     break;
    //                 }
    //             }
    //         }
    //         Ok(())
    //     })
    // }

    fn name(&self) -> &'static str {
        "cg3::sentences"
    }
}

#[async_trait]
impl CommandRunner for Mwesplit {
    async fn forward(
        self: Arc<Self>,
        input: PipelineValue,
        _config: Arc<serde_json::Value>,
    ) -> Result<PipelineValues, crate::modules::Error> {
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
        "cg3::mwesplit"
    }
}

/// Constraint Grammar 3 disambiguator
#[derive(facet::Facet)]
pub struct Vislcg3 {
    #[facet(opaque)]
    _context: Arc<Context>,
    #[facet(opaque)]
    input_tx: Sender<Option<String>>,
    #[facet(opaque)]
    output_rx: Mutex<Receiver<Option<String>>>,
    #[facet(opaque)]
    _thread: JoinHandle<()>,
}

#[rt_struct(module = "cg3")]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Vislcg3Config {
    trace: bool,
}

#[rt_command(
    module = "cg3",
    name = "vislcg3",
    input = [String],
    output = "String",
    kind = "cg3",
    args = [
        model_path = "Path",
        config? = "Vislcg3Config",
    ]
)]
impl Vislcg3 {
    pub async fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        tracing::debug!("Creating vislcg3");

        let model_path = kwargs
            .remove("model_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| {
                Error::msg("model_path missing").at("pipeline.json", "/args/model_path")
            })?;
        let mapped_model = context.memory_map_file(&model_path).await?;

        let config = match kwargs
            .remove("config")
            .and_then(|x| x.value)
            .map(|x| x.try_as_json())
        {
            Some(Ok(c)) => {
                let config: Vislcg3Config = serde_json::from_value(c)
                    .map_err(|e| Error::wrap(e).at("pipeline.json", "/args/config"))?;
                Some(config)
            }
            Some(Err(e)) => {
                return Err(Error::msg(format!("config arg is not valid JSON: {}", e))
                    .at("pipeline.json", "/args/config"));
            }
            None => None,
        };
        let config = config.unwrap_or_default();

        let (input_tx, mut input_rx) = mpsc::channel(1);
        let (output_tx, output_rx) = mpsc::channel(1);

        let thread = std::thread::spawn(move || {
            let model_bytes = mapped_model
                .as_slice()
                .expect("failed to access mapped cg3 grammar");
            let applicator = Applicator::from_bytes(&model_bytes, &model_path);
            applicator.set_trace(config.trace);

            loop {
                let Some(Some(input)): Option<Option<String>> = input_rx.blocking_recv() else {
                    break;
                };

                output_tx.blocking_send(applicator.run(&input)).unwrap();
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

#[async_trait]
impl CommandRunner for Vislcg3 {
    async fn forward(
        self: Arc<Self>,
        input: PipelineValue,
        _config: Arc<serde_json::Value>,
    ) -> Result<PipelineValues, crate::modules::Error> {
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
        "cg3::vislcg3"
    }
}

/// Convert CG3 stream to JSON format
#[derive(facet::Facet)]
pub struct ToJson {
    #[facet(opaque)]
    _context: Arc<Context>,
}

#[rt_command(
    module = "cg3",
    name = "to_json",
    input = [String],
    output = "Json"
)]
impl ToJson {
    pub async fn new(
        _context: Arc<Context>,
        _kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        Ok(Arc::new(Self { _context }))
    }
}

#[async_trait]
impl CommandRunner for ToJson {
    async fn forward(
        self: Arc<Self>,
        input: PipelineValue,
        _config: Arc<serde_json::Value>,
    ) -> Result<PipelineValues, crate::modules::Error> {
        let input = input.try_into_string()?;

        let results = CG_LINE
            .captures_iter(&input)
            .map(|x| x.iter().map(|x| x.map(|x| x.as_str())).collect::<Vec<_>>())
            .collect::<Vec<_>>();

        Ok(PipelineValue::Json(serde_json::to_value(&results).map_err(Error::wrap)?).into())
    }

    fn name(&self) -> &'static str {
        "cg3::to_json"
    }
}

pub static CG_LINE: Lazy<Regex> = Lazy::<Regex>::new(|| {
    Regex::new(
        "^
(\"<(.*)>\".*
|(\t+)(\"[^\"]*\"\\S*)((?:\\s+\\S+)*)\\s*
|:(.*)
|(<STREAMCMD:FLUSH>)
|(;\t+.*)
)",
    )
    .unwrap()
});

#[cfg(test)]
mod sentences_tests {
    use super::*;

    const SOUTH_SAMI: &str = "\"<Gïelejarngi>\"\n\t\"gïelejarnge\" N Sem/Plc Pl Gen <W:0.0> @>N #1->2 \"gïelejarngi\"phon\n: \n\"<darjomh>\"\n\t\"darjome\" N Sem/Feat Pl Nom <W:0.0> @SUBJ> #2->3 \"darjomh\"phon\n: \n\"<viehkiehtieh>\"\n\t\"viehkiehtidh\" <mv> V TV Ind Prs Pl3 <W:0.0> @FMV #3->0 \"viehkiehtieh\"phon\n: \n\"<saemien>\"\n\t\"saemien\" A Attr <W:0.0> @>N #4->5 \"saemien\"phon\n: \n\"<gïelem>\"\n\t\"gïele\" N Sem/Lang_Tool Sg Acc <W:0.0> @<OBJ #5->3 \"gïelem\"phon\n: \n\"<våajnoes>\"\n\t\"våajnoes\" Adv <W:0.0> @<ADVL #6->3 \"våajnoes\"phon\n: \n\"<darjodh>\"\n\t\"darjodh\" <mv> V TV Inf <W:0.0> @IMV #7->3 \"darjodh\"phon\n: \n\"<voengesne>\"\n\t\"voenge\" N Sem/Plc-abstr Sg Ine <W:0.0> @<ADVL #8->7 \"voengesne\"phon\n\"<.>\"\n\t\".\" CLB <W:0.0> #9->3 \".\"phon\n: \n\"<Gïelejarngh>\"\n\t\"gïelejarnge\" N Sem/Plc Pl Nom <W:0.0> @SUBJ> #1->5 \"gïelejarngh\"phon\n: \n\"<sijjen>\"\n\t\"sijjieh\" Pron Logo Pl3 Gen <W:0.0> @>N #2->3 \"sijjen\"phon\n: \n\"<gïeledajvh>\"\n\t\"gïeledajve\" N Sem/Plc Pl Nom <W:0.0> @SUBJ> #3->5 \"gïeledajvh\"phon\n: \n\"<våaroeminie>\"\n\t\"våarome\" N Sem/Semcon Ess <W:0.0> @ADVL> #4->5 \"våaroeminie\"phon\n: \n\"<utnieh>\"\n\t\"utnedh\" <mv> V <TH-Acc-Any><RO-Ess-Any> TV Ind Prs Pl3 <W:0.0> @FMV #5->0 \"utnieh\"phon\n\"<,>\"\n\t\",\" CLB <W:0.0> #6->7 \",\"phon\n: \n\"<jïh>\"\n\t\"jïh\" CC <W:0.0> @CNP #7->5 \"jïh\"phon\n: \n\"<råajvarimmiejgujmie>\"\n\t\"råajvarimmie\" N Sem/Act Pl Com NoUml <W:0.0> @<ADVL #8->5 \"råajvarimmiejgujmie\"phon\n: \n\"<nierhkieh>\"\n\t\"nïerhkedh\" <mv> V TV Ind Prs Pl3 Uml <W:0.0> @FMV #9->5 \"nierhkieh\"phon\n: \n\"<mah>\"\n\t\"mij\" Pron Interr Pl Nom <W:0.0> @SUBJ> #10->11 \"mah\"phon\n: \n\"<leah>\"\n\t\"lea\" <aux> V IV Ind Prs Pl3 <W:0.0> @FAUX #11->5 \"leah\"phon\n: \n\"<daerpiesvoetide>\"\n\t\"daerpiesvoete\" N Sem/Perc-phys Pl Acc <W:0.0> @OBJ> #12->13 \"daerpiesvoetide\"phon\n: \n\"<sjïehtedamme>\"\n\t\"sjïehtedidh\" v1 <mv> V TV PrfPrc <W:0.0> @IMV #13->11 \"sjïehtedamme\"phon\n: \n\"<dejnie>\"\n\t\"dïhte\" Pron Dem Pl Ine <W:0.0> @>N #14->16 \"dejnie\"phon\n: \n\"<ovmessie>\"\n\t\"ovmessie\" A Attr <W:0.0> @>N #15->16 \"ovmessie\"phon\n: \n\"<dajvine>\"\n\t\"dajve\" N Sem/Plc Pl Ine <W:0.0> @<ADVL #16->13 \"dajvine\"phon\n\"<?>\"\n\t\"?\" CLB <W:0.0> #17->5 \"?\"phon\n";

    #[test]
    fn south_sami_splits_into_two_sentences_surface() {
        let breakers = crate::modules::cg3_util::default_sentence_breakers();
        let sentences = extract_sentences(SOUTH_SAMI, SentenceMode::SurfaceForm, &breakers);
        assert_eq!(
            sentences.len(),
            2,
            "expected 2 sentences (split on . and ?), got: {sentences:#?}"
        );
        assert!(
            sentences[0].ends_with('.'),
            "sentence 1 should retain its boundary '.', got: {:?}",
            sentences[0]
        );
        assert!(
            sentences[1].ends_with('?'),
            "sentence 2 should retain its boundary '?', got: {:?}",
            sentences[1]
        );
        assert!(
            sentences[1].contains(','),
            "comma should be retained mid-sentence, not split on, got: {:?}",
            sentences[1]
        );
    }

    #[test]
    fn south_sami_splits_into_two_sentences_phonological() {
        let breakers = crate::modules::cg3_util::default_sentence_breakers();
        let sentences = extract_sentences(SOUTH_SAMI, SentenceMode::PhonologicalForm, &breakers);
        assert_eq!(
            sentences.len(),
            2,
            "expected 2 sentences in phonological mode, got: {sentences:#?}"
        );
        assert!(sentences[0].ends_with('.'));
        assert!(sentences[1].ends_with('?'));
    }

    #[test]
    fn break_tag_emits_sentinel() {
        let cg3 = "\"<Hello>\"\n\t\"Hello\" N <W:0.0> <DRT-BREAK-AFTER:500>\n\"<world>\"\n\t\"world\" N <W:0.0>\n\"<.>\"\n\t\".\" CLB <W:0.0>\n";
        let breakers = crate::modules::cg3_util::default_sentence_breakers();
        let sentences = extract_sentences(cg3, SentenceMode::SurfaceForm, &breakers);
        assert_eq!(
            sentences,
            vec![
                "Hello".to_string(),
                "\x1FBREAK:500\x1F".to_string(),
                "world .".to_string(),
            ],
            "got: {sentences:#?}"
        );
    }

    #[test]
    fn break_tag_without_sentence_punct_still_flushes() {
        let cg3 = "\"<foo>\"\n\t\"foo\" N <W:0.0> <DRT-BREAK-AFTER:250>\n\"<bar>\"\n\t\"bar\" N <W:0.0>\n";
        let breakers = crate::modules::cg3_util::default_sentence_breakers();
        let sentences = extract_sentences(cg3, SentenceMode::SurfaceForm, &breakers);
        assert_eq!(
            sentences,
            vec![
                "foo".to_string(),
                "\x1FBREAK:250\x1F".to_string(),
                "bar".to_string(),
            ]
        );
    }

    #[test]
    fn after_break_ms_helper() {
        // Round-trip a cohort with the break tag and verify extraction.
        let cg3 = "\"<x>\"\n\t\"x\" N <W:0.0> <DRT-BREAK-AFTER:1234>\n";
        let output = Output::new(cg3);
        let cohort = output
            .iter()
            .filter_map(Result::ok)
            .find_map(|b| {
                if let Block::Cohort(c) = b {
                    Some(c)
                } else {
                    None
                }
            })
            .expect("cohort");
        assert_eq!(after_break_ms(&cohort), Some(1234));
    }

    #[test]
    fn prosody_rate_synthesises_pace_in_opts() {
        let cg3 = "\"<Hello>\"\n\t\"Hello\" N <W:0.0> <DRT-PROSODY-RATE:fast>\n\"<world>\"\n\t\"world\" N <W:0.0> <DRT-PROSODY-RATE:fast>\n\"<.>\"\n\t\".\" CLB <W:0.0> <DRT-PROSODY-RATE:fast>\n";
        let breakers = crate::modules::cg3_util::default_sentence_breakers();
        let sentences = extract_sentences(cg3, SentenceMode::SurfaceForm, &breakers);
        assert_eq!(sentences.len(), 1);
        let s = &sentences[0];
        // Both pace=1.25 (computed) and prosody-rate=fast (raw) appear.
        assert!(
            s.starts_with("\x1FOPTS:pace=1.25;prosody-rate=fast\x1F"),
            "got: {s:?}"
        );
        assert!(s.ends_with("Hello world ."), "got: {s:?}");
    }

    #[test]
    fn no_drt_tag_means_no_opts_prefix() {
        let cg3 = "\"<Hello>\"\n\t\"Hello\" N <W:0.0>\n\"<.>\"\n\t\".\" CLB <W:0.0>\n";
        let breakers = crate::modules::cg3_util::default_sentence_breakers();
        let sentences = extract_sentences(cg3, SentenceMode::SurfaceForm, &breakers);
        assert_eq!(sentences, vec!["Hello .".to_string()]);
    }

    #[test]
    fn prosody_rate_and_break_combine() {
        let cg3 = "\"<Hello>\"\n\t\"Hello\" N <W:0.0> <DRT-PROSODY-RATE:slow> <DRT-BREAK-AFTER:500>\n\"<world>\"\n\t\"world\" N <W:0.0>\n\"<.>\"\n\t\".\" CLB <W:0.0>\n";
        let breakers = crate::modules::cg3_util::default_sentence_breakers();
        let sentences = extract_sentences(cg3, SentenceMode::SurfaceForm, &breakers);
        assert_eq!(
            sentences,
            vec![
                "\x1FOPTS:pace=0.75;prosody-rate=slow\x1FHello".to_string(),
                "\x1FBREAK:500\x1F".to_string(),
                "world .".to_string(),
            ]
        );
    }

    #[test]
    fn cohort_opts_collects_many_drt_tags() {
        let cg3 = "\"<a>\"\n\t\"a\" N <W:0.0> <DRT-PROSODY-RATE:fast> <DRT-VOICE-GENDER:female> <DRT-EMPHASIS:strong> <DRT-BREAK-AFTER:500>\n";
        let output = Output::new(cg3);
        let cohort = output
            .iter()
            .filter_map(Result::ok)
            .find_map(|b| {
                if let Block::Cohort(c) = b {
                    Some(c)
                } else {
                    None
                }
            })
            .expect("cohort");
        let opts = cohort_opts(&cohort);
        assert_eq!(opts.get("prosody-rate").map(String::as_str), Some("fast"));
        assert_eq!(opts.get("voice-gender").map(String::as_str), Some("female"));
        assert_eq!(opts.get("emphasis").map(String::as_str), Some("strong"));
        // DRT-BREAK-AFTER is consumed separately, not surfaced through cohort_opts.
        assert!(!opts.contains_key("break-after"));
    }

    #[test]
    fn cohort_opts_percent_decodes() {
        let cg3 = "\"<a>\"\n\t\"a\" N <W:0.0> <DRT-PROSODY-CONTOUR:(0%25,+20Hz)%20(50%25,+30Hz)>\n";
        let output = Output::new(cg3);
        let cohort = output
            .iter()
            .filter_map(Result::ok)
            .find_map(|b| {
                if let Block::Cohort(c) = b {
                    Some(c)
                } else {
                    None
                }
            })
            .expect("cohort");
        let opts = cohort_opts(&cohort);
        assert_eq!(
            opts.get("prosody-contour").map(String::as_str),
            Some("(0%,+20Hz) (50%,+30Hz)")
        );
    }

    #[test]
    fn multi_attr_opts_emits_sorted_keys() {
        let cg3 = "\"<Hello>\"\n\t\"Hello\" N <W:0.0> <DRT-VOICE-GENDER:female> <DRT-PROSODY-RATE:fast> <DRT-EMPHASIS:strong>\n\"<.>\"\n\t\".\" CLB <W:0.0>\n";
        let breakers = crate::modules::cg3_util::default_sentence_breakers();
        let sentences = extract_sentences(cg3, SentenceMode::SurfaceForm, &breakers);
        assert_eq!(sentences.len(), 1);
        // BTreeMap → alphabetical key order, with synthesised `pace` inserted first.
        assert!(
            sentences[0].starts_with(
                "\x1FOPTS:pace=1.25;emphasis=strong;prosody-rate=fast;voice-gender=female\x1F"
            ),
            "got: {:?}",
            sentences[0]
        );
    }
}
