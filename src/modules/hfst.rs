use std::{borrow::Cow, collections::HashMap, sync::Arc, thread::JoinHandle};

use async_trait::async_trait;
use divvun_runtime_macros::rt_command;
use tokio::sync::{
    Mutex,
    mpsc::{self, Receiver, Sender},
};

use std::path::Path;

use hfst::hfst_flag_diacritics::FdOperation;
use hfst::hfst_input_stream::HfstInputStream;
use hfst::hfst_transducer::AnyTransducer;
use hfst::pmatch::PmatchContainer;
use hfst::pmatch_tokenize::{
    OutputFormat, TokenizeInputSettings, TokenizeSettings, process_input_stream,
};
use hfst::transducer::IStream;

use crate::ast;

use super::{CommandRunner, Context, PipelineValue, PipelineValues, SharedPipelineValueFut};

/// Load an optimized-lookup transducer for morphological lookup, wrapped in a
/// `Mutex` for interior mutability — the native `lookup_fd_*` methods take
/// `&mut self`, but callers hold the transducer behind a shared `&self`.
pub(crate) fn load_lookup(
    path: &Path,
) -> Result<std::sync::Mutex<AnyTransducer>, crate::modules::Error> {
    let mut stream = HfstInputStream::new_filename(&path.to_string_lossy()).map_err(|e| {
        crate::modules::Error::msg(format!("failed to open transducer {}: {e}", path.display()))
    })?;
    let transducer = stream.read().map_err(|e| {
        crate::modules::Error::msg(format!("failed to read transducer {}: {e}", path.display()))
    })?;
    match &transducer {
        AnyTransducer::OlW(_) | AnyTransducer::OlU(_) => {}
        _ => {
            return Err(crate::modules::Error::msg(format!(
                "transducer {} is not an optimized-lookup transducer",
                path.display()
            )));
        }
    }
    Ok(std::sync::Mutex::new(transducer))
}

/// Flag-diacritic-aware lookup. Returns one output string per result path,
/// keeping only the non-diacritic symbols (`is_diacritic == false`) or only the
/// flag-diacritic symbols (`is_diacritic == true`). Mirrors the old FFI
/// wrapper's `lookup_fd(input, -1, 10.0)` + `FdOperation::is_diacritic` filter.
pub(crate) fn lookup_tags(
    transducer: &std::sync::Mutex<AnyTransducer>,
    input: &str,
    is_diacritic: bool,
) -> Vec<String> {
    let mut guard = transducer.lock().unwrap();
    let paths = match &mut *guard {
        AnyTransducer::OlW(t) => t.lookup_fd_string(input, -1, 10.0),
        AnyTransducer::OlU(t) => t.lookup_fd_string(input, -1, 10.0),
        _ => return Vec::new(),
    };
    let Ok(paths) = paths else {
        return Vec::new();
    };
    paths
        .into_iter()
        .map(|path| {
            path.second
                .iter()
                .filter(|sym| FdOperation::is_diacritic(sym.as_str()) == is_diacritic)
                .map(|sym| sym.as_str())
                .collect::<String>()
        })
        .collect()
}

/// The giellacg tokenizer settings divvun-runtime uses — mirrors the C++
/// `hfst-tokenize --giella-cg` `init_settings()` the old FFI wrapper hard-coded.
fn giellacg_settings() -> TokenizeSettings {
    TokenizeSettings {
        output_format: OutputFormat::giellacg,
        print_weights: true,
        print_all: true,
        dedupe: true,
        max_weight_classes: i32::MAX,
        tokenize_multichar: false,
        ..TokenizeSettings::default()
    }
}

/// Load a pmatch (`.pmhfst`) tokenizer container with single-codepoint
/// tokenization (i.e. `tokenize_multichar == false`).
fn load_tokenizer(path: &Path) -> Result<PmatchContainer, crate::modules::Error> {
    let mut file = std::fs::File::open(path).map_err(|e| {
        crate::modules::Error::msg(format!("failed to open tokenizer {}: {e}", path.display()))
    })?;
    let mut stream = IStream::new(&mut file as &mut dyn std::io::Read);
    let mut container = PmatchContainer::new_from_stream(&mut stream).map_err(|e| {
        crate::modules::Error::msg(format!("failed to load tokenizer {}: {e}", path.display()))
    })?;
    container.set_verbose(false);
    container.set_single_codepoint_tokenization(true);
    Ok(container)
}

/// Tokenize a single input string into CG3 (giellacg) output. Drives the same
/// `process_input_0delim` path the C++ wrapper used (newline-delimited
/// `match_and_print`, `<STREAMCMD:FLUSH>` on NUL).
fn run_tokenizer(
    container: &mut PmatchContainer,
    settings: &TokenizeSettings,
    input: &str,
) -> String {
    let input_settings = TokenizeInputSettings {
        superblanks: false,
        verbose: false,
        ..TokenizeInputSettings::default()
    };
    let mut output: Vec<u8> = Vec::new();
    let mut msg = std::io::sink();
    let mut reader = std::io::Cursor::new(input.as_bytes());
    process_input_stream(
        container,
        &mut reader,
        &mut output,
        &mut msg,
        settings,
        &input_settings,
    );
    String::from_utf8_lossy(&output).into_owned()
}

/// HFST tokenizer
#[derive(facet::Facet)]
pub struct Tokenize {
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
    module = "hfst",
    name = "tokenize",
    input = [String],
    output = "String",
    kind = "cg3",
    args = [model_path = "Path"]
)]
impl Tokenize {
    pub async fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, crate::modules::Error> {
        tracing::debug!("Creating tokenize");
        let model_path = kwargs
            .remove("model_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| {
                crate::modules::Error::msg("model_path missing")
                    .at("pipeline.json", "/args/model_path")
            })?;
        let model_path = context.extract_to_temp_dir(model_path).await?;

        let (input_tx, mut input_rx) = mpsc::channel(1);
        let (output_tx, output_rx) = mpsc::channel(1);

        let thread = std::thread::spawn(move || {
            tracing::debug!("init hfst tokenizer BEFORE");
            let settings = giellacg_settings();
            let mut container = load_tokenizer(&model_path).expect("failed to load hfst tokenizer");
            tracing::debug!("init hfst tokenizer");

            loop {
                let Some(Some(input)): Option<Option<String>> = input_rx.blocking_recv() else {
                    break;
                };

                let output = run_tokenizer(&mut container, &settings, &input);
                output_tx.blocking_send(Some(output)).unwrap();
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

#[cfg(feature = "mod-ssml")]
#[derive(Clone, Debug, Default)]
struct SsmlFrame {
    // Effective overrides at this scope. None = inherit / unset.
    // Strings hold the SSML wire form (e.g. "fast", "+20Hz") so we
    // can round-trip them verbatim into CG3 tag values.
    prosody_rate: Option<String>,
    prosody_pitch: Option<String>,
    prosody_volume: Option<String>,
    prosody_range: Option<String>,
    prosody_contour: Option<String>,
    prosody_duration: Option<String>,
    voice_name: Option<String>,
    voice_gender: Option<String>,
    voice_age: Option<u8>,
    voice_variant: Option<u32>,
    voice_lang: Option<String>,
    lang: Option<String>,
    emphasis: Option<String>,
    say_as: Option<String>,
    say_as_format: Option<String>,
    say_as_detail: Option<String>,
    phoneme_ph: Option<String>,
    phoneme_alphabet: Option<String>,
    suppress: bool,
}

#[cfg(feature = "mod-ssml")]
impl Tokenize {
    async fn forward_ssml(
        self: Arc<Self>,
        input: String,
    ) -> Result<PipelineValues, crate::modules::Error> {
        use ssml_parser::ParserEvent;
        use ssml_parser::elements::ParsedElement;

        let events: Vec<ParserEvent> = tokio::task::spawn_blocking(move || {
            ssml_parser::parse_ssml(&input)
                .map(|s| s.event_iter().collect::<Vec<_>>())
                .map_err(|e| crate::modules::Error::msg(e.to_string()))
        })
        .await
        .unwrap()?;

        let mut output_rx = self.output_rx.lock().await;
        let mut fragments: Vec<String> = Vec::new();
        let mut pending_break_ms: u32 = 0;
        let mut stack: Vec<SsmlFrame> = vec![SsmlFrame::default()];

        for event in events {
            match event {
                ParserEvent::Text(s) => {
                    let frame = stack.last().unwrap().clone();
                    if frame.suppress {
                        continue;
                    }
                    let trimmed = s.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let fragment = self
                        .tokenize_one(&mut output_rx, trimmed.to_string(), &frame)
                        .await;

                    if pending_break_ms > 0 && !fragments.is_empty() {
                        let last_idx = fragments.len() - 1;
                        fragments[last_idx] =
                            inject_break_after_tag(&fragments[last_idx], pending_break_ms);
                        pending_break_ms = 0;
                    }

                    fragments.push(fragment);
                }
                ParserEvent::Empty(ParsedElement::Break(attrs)) => {
                    pending_break_ms = pending_break_ms.saturating_add(break_ms(&attrs));
                }
                ParserEvent::Open(elem) => {
                    let parent = stack.last().unwrap().clone();
                    let mut frame = parent.clone();

                    match &elem {
                        ParsedElement::Prosody(attrs) => {
                            if let Some(v) = attrs.rate.as_ref() {
                                frame.prosody_rate = Some(v.to_string());
                            }
                            if let Some(v) = attrs.pitch.as_ref() {
                                frame.prosody_pitch = Some(v.to_string());
                            }
                            if let Some(v) = attrs.volume.as_ref() {
                                frame.prosody_volume = Some(v.to_string());
                            }
                            if let Some(v) = attrs.range.as_ref() {
                                frame.prosody_range = Some(v.to_string());
                            }
                            if let Some(v) = attrs.contour.as_ref() {
                                frame.prosody_contour = Some(v.to_string());
                            }
                            if let Some(v) = attrs.duration.as_ref() {
                                frame.prosody_duration = Some(v.to_string());
                            }
                        }
                        ParsedElement::Voice(attrs) => {
                            if !attrs.name.is_empty() {
                                frame.voice_name = Some(attrs.name.join(","));
                            }
                            if let Some(g) = attrs.gender {
                                frame.voice_gender = Some(g.to_string());
                            }
                            if let Some(a) = attrs.age {
                                frame.voice_age = Some(a);
                            }
                            if let Some(v) = attrs.variant {
                                frame.voice_variant = Some(v.get() as u32);
                            }
                            if !attrs.languages.is_empty() {
                                let joined = attrs
                                    .languages
                                    .iter()
                                    .map(|p| p.to_string())
                                    .collect::<Vec<_>>()
                                    .join(",");
                                frame.voice_lang = Some(joined);
                            }
                        }
                        ParsedElement::Lang(attrs) => {
                            frame.lang = Some(attrs.lang.clone());
                        }
                        ParsedElement::Emphasis(attrs) => {
                            if let Some(level) = attrs.level {
                                frame.emphasis = Some(level.to_string());
                            }
                        }
                        ParsedElement::SayAs(attrs) => {
                            frame.say_as = Some(attrs.interpret_as.clone());
                            if let Some(f) = attrs.format.as_ref() {
                                frame.say_as_format = Some(f.clone());
                            }
                            if let Some(d) = attrs.detail.as_ref() {
                                frame.say_as_detail = Some(d.clone());
                            }
                        }
                        ParsedElement::Phoneme(attrs) => {
                            frame.phoneme_ph = Some(attrs.ph.clone());
                            if let Some(a) = attrs.alphabet.as_ref() {
                                frame.phoneme_alphabet = Some(a.to_string());
                            }
                        }
                        ParsedElement::Sub(attrs) => {
                            // The alias is spoken in place of the inner text. Tokenize
                            // the alias now (under the *parent* scope's overrides),
                            // then suppress the inner text events until Close.
                            if !parent.suppress {
                                let trimmed = attrs.alias.trim();
                                if !trimmed.is_empty() {
                                    let fragment = self
                                        .tokenize_one(&mut output_rx, trimmed.to_string(), &parent)
                                        .await;
                                    if pending_break_ms > 0 && !fragments.is_empty() {
                                        let last_idx = fragments.len() - 1;
                                        fragments[last_idx] = inject_break_after_tag(
                                            &fragments[last_idx],
                                            pending_break_ms,
                                        );
                                        pending_break_ms = 0;
                                    }
                                    fragments.push(fragment);
                                }
                            }
                            frame.suppress = true;
                        }
                        _ => {}
                    }

                    stack.push(frame);
                }
                ParserEvent::Close(_) => {
                    if stack.len() > 1 {
                        stack.pop();
                    }
                }
                _ => {}
            }
        }

        if pending_break_ms > 0 && !fragments.is_empty() {
            let last_idx = fragments.len() - 1;
            fragments[last_idx] = inject_break_after_tag(&fragments[last_idx], pending_break_ms);
        }

        Ok(fragments.concat().into())
    }

    async fn tokenize_one(
        &self,
        output_rx: &mut tokio::sync::MutexGuard<'_, Receiver<Option<String>>>,
        text: String,
        frame: &SsmlFrame,
    ) -> String {
        self.input_tx.send(Some(text)).await.expect("input tx send");
        let fragment = output_rx
            .recv()
            .await
            .expect("output rx recv")
            .unwrap_or_default();
        inject_opts_tags(&fragment, frame)
    }
}

#[async_trait]
impl CommandRunner for Tokenize {
    async fn forward(
        self: Arc<Self>,
        input: PipelineValue,
        _config: Arc<serde_json::Value>,
    ) -> Result<PipelineValues, crate::modules::Error> {
        let input = input.try_into_string()?;

        #[cfg(feature = "mod-ssml")]
        if input.trim_start().starts_with("<speak") {
            return self.forward_ssml(input).await;
        }

        self.input_tx
            .send(Some(input))
            .await
            .expect("input tx send");
        let mut output_rx = self.output_rx.lock().await;
        let output = output_rx.recv().await.expect("output rx recv");

        Ok(output.unwrap_or_else(|| "".to_string()).into())
    }

    fn name(&self) -> &'static str {
        "hfst::tokenize"
    }
}

#[cfg(feature = "mod-ssml")]
const DEFAULT_BREAK_MS: u32 = 500;

/// SSML 1.1 §3.2.3 strength → ms. `medium` is anchored to an ordinary
/// inter-sentence pause; the scale walks outward monotonically from there.
#[cfg(feature = "mod-ssml")]
fn strength_to_ms(s: ssml_parser::elements::Strength) -> u32 {
    use ssml_parser::elements::Strength;
    match s {
        Strength::No => 0,
        Strength::ExtraWeak => 100,
        Strength::Weak => 250,
        Strength::Medium => 500,
        Strength::Strong => 1000,
        Strength::ExtraStrong => 1500,
    }
}

#[cfg(feature = "mod-ssml")]
fn break_ms(attrs: &ssml_parser::elements::BreakAttributes) -> u32 {
    // SSML 1.1 §3.2.3: when both `time` and `strength` are present, time
    // governs duration; strength's other prosodic changes we don't model.
    if let Some(td) = &attrs.time {
        return td.duration().as_millis().min(u32::MAX as u128) as u32;
    }
    if let Some(s) = attrs.strength {
        return strength_to_ms(s);
    }
    DEFAULT_BREAK_MS
}

/// Percent-encode characters that would break CG3 tag tokenisation
/// (whitespace, `<`, `>`) or our OPTS-prefix encoding (`;`, `=`, `\x1F`).
/// `%` is encoded too for round-trip integrity. Everything else passes through.
#[cfg(feature = "mod-ssml")]
fn encode_tag_value(s: &str) -> Cow<'_, str> {
    let needs = s.bytes().any(|b| {
        matches!(
            b,
            b' ' | b'\t' | b'\n' | b'\r' | b'<' | b'>' | b';' | b'=' | b'%' | 0x1F
        )
    });
    if !needs {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    for b in s.bytes() {
        match b {
            b' ' | b'\t' | b'\n' | b'\r' | b'<' | b'>' | b';' | b'=' | b'%' | 0x1F => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
            _ => out.push(b as char),
        }
    }
    Cow::Owned(out)
}

/// Build the space-prefixed tag string for the overrides in `frame`. Empty
/// when no fields are set. The tag names mirror the SSML element/attr names
/// (uppercased, hyphen-joined).
#[cfg(feature = "mod-ssml")]
fn append_drt_tag(tags: &mut String, name: &str, value: &str) {
    tags.push_str(" <DRT-");
    tags.push_str(name);
    tags.push(':');
    tags.push_str(&encode_tag_value(value));
    tags.push('>');
}

#[cfg(feature = "mod-ssml")]
fn format_opts_tags(frame: &SsmlFrame) -> String {
    let mut tags = String::new();
    if let Some(v) = frame.prosody_rate.as_ref() {
        append_drt_tag(&mut tags, "PROSODY-RATE", v);
    }
    if let Some(v) = frame.prosody_pitch.as_ref() {
        append_drt_tag(&mut tags, "PROSODY-PITCH", v);
    }
    if let Some(v) = frame.prosody_volume.as_ref() {
        append_drt_tag(&mut tags, "PROSODY-VOLUME", v);
    }
    if let Some(v) = frame.prosody_range.as_ref() {
        append_drt_tag(&mut tags, "PROSODY-RANGE", v);
    }
    if let Some(v) = frame.prosody_contour.as_ref() {
        append_drt_tag(&mut tags, "PROSODY-CONTOUR", v);
    }
    if let Some(v) = frame.prosody_duration.as_ref() {
        append_drt_tag(&mut tags, "PROSODY-DURATION", v);
    }
    if let Some(v) = frame.voice_name.as_ref() {
        append_drt_tag(&mut tags, "VOICE-NAME", v);
    }
    if let Some(v) = frame.voice_gender.as_ref() {
        append_drt_tag(&mut tags, "VOICE-GENDER", v);
    }
    if let Some(n) = frame.voice_age {
        append_drt_tag(&mut tags, "VOICE-AGE", &n.to_string());
    }
    if let Some(n) = frame.voice_variant {
        append_drt_tag(&mut tags, "VOICE-VARIANT", &n.to_string());
    }
    if let Some(v) = frame.voice_lang.as_ref() {
        append_drt_tag(&mut tags, "VOICE-LANG", v);
    }
    if let Some(v) = frame.lang.as_ref() {
        append_drt_tag(&mut tags, "LANG", v);
    }
    if let Some(v) = frame.emphasis.as_ref() {
        append_drt_tag(&mut tags, "EMPHASIS", v);
    }
    if let Some(v) = frame.say_as.as_ref() {
        append_drt_tag(&mut tags, "SAY-AS", v);
    }
    if let Some(v) = frame.say_as_format.as_ref() {
        append_drt_tag(&mut tags, "SAY-AS-FORMAT", v);
    }
    if let Some(v) = frame.say_as_detail.as_ref() {
        append_drt_tag(&mut tags, "SAY-AS-DETAIL", v);
    }
    if let Some(v) = frame.phoneme_ph.as_ref() {
        append_drt_tag(&mut tags, "PHONEME-PH", v);
    }
    if let Some(v) = frame.phoneme_alphabet.as_ref() {
        append_drt_tag(&mut tags, "PHONEME-ALPHABET", v);
    }
    tags
}

/// Append the frame's `<DRT-*:V>` tags to every reading line in this CG3
/// fragment. Applies to *every* cohort because the whole text-run was uttered
/// inside the same SSML scope.
#[cfg(feature = "mod-ssml")]
fn inject_opts_tags(fragment: &str, frame: &SsmlFrame) -> String {
    let tags = format_opts_tags(frame);
    if tags.is_empty() {
        return fragment.to_string();
    }
    let mut out = String::with_capacity(fragment.len() + fragment.lines().count() * tags.len());
    for line in fragment.lines() {
        if line.starts_with('\t') {
            out.push_str(line);
            out.push_str(&tags);
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Append `<DRT-BREAK-AFTER:NNNN>` to every reading line under the last cohort
/// header in this CG3 fragment.
#[cfg(feature = "mod-ssml")]
fn inject_break_after_tag(fragment: &str, ms: u32) -> String {
    let tag = format!("<DRT-BREAK-AFTER:{ms}>");
    let lines: Vec<&str> = fragment.lines().collect();
    let Some(start) = lines.iter().rposition(|l| l.starts_with("\"<")) else {
        return fragment.to_string();
    };

    let mut out = String::with_capacity(fragment.len() + (lines.len() - start) * (tag.len() + 1));
    for (i, line) in lines.iter().enumerate() {
        if i > start && line.starts_with('\t') {
            out.push_str(line);
            out.push(' ');
            out.push_str(&tag);
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[cfg(all(test, feature = "mod-ssml"))]
mod tests {
    use super::*;

    #[test]
    fn inject_into_last_cohort_only() {
        let frag = "\"<Hello>\"\n\t\"Hello\" N <W:0.0>\n\"<world>\"\n\t\"world\" N <W:0.0>\n";
        let out = inject_break_after_tag(frag, 500);
        assert!(
            out.contains("\t\"Hello\" N <W:0.0>\n"),
            "first cohort untouched, got: {out}"
        );
        assert!(
            out.contains("\t\"world\" N <W:0.0> <DRT-BREAK-AFTER:500>\n"),
            "got: {out}"
        );
    }

    #[test]
    fn inject_with_multiple_readings() {
        let frag = "\"<x>\"\n\t\"x\" N <W:0.0>\n\t\"x\" V <W:0.5>\n";
        let out = inject_break_after_tag(frag, 250);
        assert!(out.contains("\t\"x\" N <W:0.0> <DRT-BREAK-AFTER:250>\n"));
        assert!(out.contains("\t\"x\" V <W:0.5> <DRT-BREAK-AFTER:250>\n"));
    }

    #[test]
    fn inject_into_empty_fragment_is_passthrough() {
        assert_eq!(inject_break_after_tag("", 500), "");
    }

    #[test]
    fn break_ms_time_only() {
        let attrs = parse_break("<speak><break time=\"500ms\"/></speak>");
        assert_eq!(break_ms(&attrs), 500);
    }

    #[test]
    fn break_ms_time_fractional_seconds() {
        let attrs = parse_break("<speak><break time=\"1.5s\"/></speak>");
        assert_eq!(break_ms(&attrs), 1500);
    }

    #[test]
    fn break_ms_strength_only() {
        assert_eq!(
            break_ms(&parse_break("<speak><break strength=\"none\"/></speak>")),
            0
        );
        assert_eq!(
            break_ms(&parse_break("<speak><break strength=\"x-weak\"/></speak>")),
            100
        );
        assert_eq!(
            break_ms(&parse_break("<speak><break strength=\"weak\"/></speak>")),
            250
        );
        assert_eq!(
            break_ms(&parse_break("<speak><break strength=\"medium\"/></speak>")),
            500
        );
        assert_eq!(
            break_ms(&parse_break("<speak><break strength=\"strong\"/></speak>")),
            1000
        );
        assert_eq!(
            break_ms(&parse_break(
                "<speak><break strength=\"x-strong\"/></speak>"
            )),
            1500
        );
    }

    #[test]
    fn break_ms_no_attrs_is_medium_default() {
        assert_eq!(break_ms(&parse_break("<speak><break/></speak>")), 500);
    }

    #[test]
    fn break_ms_time_wins_over_strength() {
        assert_eq!(
            break_ms(&parse_break(
                "<speak><break time=\"2s\" strength=\"weak\"/></speak>"
            )),
            2000
        );
    }

    fn parse_break(ssml: &str) -> ssml_parser::elements::BreakAttributes {
        use ssml_parser::ParserEvent;
        use ssml_parser::elements::ParsedElement;
        let parsed = ssml_parser::parse_ssml(ssml).expect("parse");
        for ev in parsed.event_iter() {
            if let ParserEvent::Empty(ParsedElement::Break(attrs)) = ev {
                return attrs;
            }
        }
        panic!("no <break> in {ssml:?}");
    }

    #[test]
    fn inject_opts_prosody_rate() {
        let frag = "\"<a>\"\n\t\"a\" N <W:0.0>\n\"<b>\"\n\t\"b\" N <W:0.0>\n";
        let frame = SsmlFrame {
            prosody_rate: Some("fast".to_string()),
            ..Default::default()
        };
        let out = inject_opts_tags(frag, &frame);
        assert!(out.contains("\t\"a\" N <W:0.0> <DRT-PROSODY-RATE:fast>\n"));
        assert!(out.contains("\t\"b\" N <W:0.0> <DRT-PROSODY-RATE:fast>\n"));
        assert!(out.contains("\"<a>\"\n"), "cohort header untouched: {out}");
    }

    #[test]
    fn inject_opts_multiple_attrs() {
        let frag = "\"<a>\"\n\t\"a\" N <W:0.0>\n";
        let frame = SsmlFrame {
            prosody_rate: Some("fast".to_string()),
            prosody_pitch: Some("high".to_string()),
            voice_gender: Some("female".to_string()),
            voice_age: Some(30),
            emphasis: Some("strong".to_string()),
            ..Default::default()
        };
        let out = inject_opts_tags(frag, &frame);
        assert!(out.contains("<DRT-PROSODY-RATE:fast>"));
        assert!(out.contains("<DRT-PROSODY-PITCH:high>"));
        assert!(out.contains("<DRT-VOICE-GENDER:female>"));
        assert!(out.contains("<DRT-VOICE-AGE:30>"));
        assert!(out.contains("<DRT-EMPHASIS:strong>"));
    }

    #[test]
    fn inject_opts_no_overrides_is_passthrough() {
        let frag = "\"<a>\"\n\t\"a\" N <W:0.0>\n";
        let frame = SsmlFrame::default();
        assert_eq!(inject_opts_tags(frag, &frame), frag);
    }

    #[test]
    fn encode_tag_value_passthrough_simple() {
        assert_eq!(encode_tag_value("fast"), "fast");
        assert_eq!(encode_tag_value("+20Hz"), "+20Hz");
        assert_eq!(encode_tag_value("200"), "200");
        // No percent sign in input → no encoding needed.
        assert_eq!(encode_tag_value("Alice,Bob"), "Alice,Bob");
    }

    #[test]
    fn encode_tag_value_escapes_problematic() {
        // Spaces in contour expressions.
        assert_eq!(
            encode_tag_value("(0%,+20Hz) (50%,+30Hz)"),
            "(0%25,+20Hz)%20(50%25,+30Hz)"
        );
        // Angle brackets must not break CG3 tag tokenisation.
        assert_eq!(encode_tag_value("a<b>c"), "a%3Cb%3Ec");
        // Tab.
        assert_eq!(encode_tag_value("a\tb"), "a%09b");
        // Percent itself must round-trip.
        assert_eq!(encode_tag_value("200%"), "200%25");
    }

    #[test]
    fn inject_opts_encodes_problematic_values() {
        let frag = "\"<a>\"\n\t\"a\" N <W:0.0>\n";
        let frame = SsmlFrame {
            prosody_contour: Some("(0%,+20Hz) (50%,+30Hz)".to_string()),
            ..Default::default()
        };
        let out = inject_opts_tags(frag, &frame);
        assert!(
            out.contains("<DRT-PROSODY-CONTOUR:(0%25,+20Hz)%20(50%25,+30Hz)>"),
            "got: {out}"
        );
    }
}
