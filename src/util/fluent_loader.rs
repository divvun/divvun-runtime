use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

use fluent::FluentResource;
use fluent_bundle::{FluentArgs, concurrent::FluentBundle};
use fluent_syntax::parser::{ErrorKind, ParserError};
use unic_langid::LanguageIdentifier;

use crate::modules::{Context, Error};

#[derive(Clone)]
pub struct FluentLoader {
    bundles: HashMap<String, Arc<FluentBundle<FluentResource>>>,
    default_locale: String,
}

impl FluentLoader {
    pub async fn new(
        context: Arc<Context>,
        pattern: &str,
        default_locale: &str,
    ) -> Result<Self, Error> {
        let mut bundles = HashMap::new();
        let files = context.load_files_glob(pattern).await?;

        for (path, contents) in files {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| Error::msg("Invalid filename"))?;

            // Extract language code from filename like "errors-en.ftl" -> "en"
            if let Some(lang_code) = extract_language_code(filename) {
                let content = String::from_utf8(contents)
                    .map_err(|e| Error::msg(format!("Failed to read file {}: {}", filename, e)))?;

                // Parse the Fluent resource. On parse errors we keep the
                // partially-parsed resource so a single bad message doesn't drop
                // the whole language file.
                let resource = match FluentResource::try_new(content) {
                    Ok(resource) => resource,
                    Err((resource, errors)) => {
                        // One block for the whole file: each error gets the line
                        // it is actually on, the text of that line, and a caret
                        // under the mistake (#32).
                        tracing::warn!("{}", render_parse_errors(filename, &resource, &errors));
                        resource
                    }
                };

                let lang_id: LanguageIdentifier = lang_code.parse().map_err(|e| {
                    Error::msg(format!("Invalid language identifier {}: {}", lang_code, e))
                })?;

                let mut bundle = FluentBundle::new_concurrent(vec![lang_id]);
                // Don't wrap interpolated values in Unicode bidi isolates (U+2068/U+2069).
                bundle.set_use_isolating(false);
                match bundle.add_resource(resource) {
                    Ok(_) => {
                        tracing::debug!("Successfully loaded Fluent resource: {}", filename);
                    }
                    Err(errors) => {
                        // Check if errors are only "Overriding" errors (which are non-fatal)
                        let non_fatal = errors
                            .iter()
                            .all(|e| matches!(e, fluent_bundle::FluentError::Overriding { .. }));
                        if non_fatal {
                            tracing::debug!(
                                "Fluent resource {} has overriding messages (normal for localization): {:?}",
                                filename,
                                errors
                            );
                        } else {
                            tracing::warn!("Fluent resource {} has errors: {:?}", filename, errors);
                        }
                    }
                }
                // Add the bundle regardless of overriding errors
                bundles.insert(lang_code, Arc::new(bundle));
            }
        }

        if bundles.is_empty() {
            tracing::warn!("No valid Fluent resources loaded from pattern: {}", pattern);
        }

        Ok(Self {
            bundles,
            default_locale: default_locale.to_string(),
        })
    }

    /// Look up a localized message, falling back across locales at the *message*
    /// level rather than the bundle level: each candidate locale in `locales`
    /// (priority order), then the default locale, then any loaded bundle. Returns
    /// the first locale whose bundle actually contains `message_id`, formatting
    /// its value (title) and `.desc` attribute (description). Returns `None` if
    /// no loaded bundle contains the message — callers fall back to the raw id.
    pub fn get_message_localized(
        &self,
        locales: &[&str],
        message_id: &str,
        args: Option<&FluentArgs>,
    ) -> Option<(String, String)> {
        if self.bundles.is_empty() {
            tracing::debug!(
                "No Fluent bundles available, falling back to error ID: {}",
                message_id
            );
            return None;
        }

        let mut seen = HashSet::new();
        let candidates = locales
            .iter()
            .copied()
            .chain(std::iter::once(self.default_locale.as_str()))
            .chain(self.bundles.keys().map(String::as_str));

        for locale in candidates {
            if !seen.insert(locale) {
                continue;
            }
            let Some(bundle) = self.bundles.get(locale) else {
                continue;
            };
            let Some(message) = bundle.get_message(message_id) else {
                continue;
            };
            let Some(pattern) = message.value() else {
                continue;
            };

            let title = bundle.format_pattern(pattern, args, &mut vec![]);
            let description = match message.attributes().find(|attr| attr.id() == "desc") {
                Some(attr) => bundle.format_pattern(attr.value(), args, &mut vec![]),
                None => title.clone(),
            };
            return Some((title.into_owned(), description.into_owned()));
        }

        None
    }

    /// Backwards-compatible single-locale lookup. Delegates to
    /// [`Self::get_message_localized`], so it now also falls back across locales
    /// at the message level rather than erroring when the chosen bundle lacks the
    /// message.
    pub fn get_message(
        &self,
        locale: Option<&str>,
        message_id: &str,
        args: Option<&FluentArgs>,
    ) -> Result<(String, String), Error> {
        let locales: Vec<&str> = locale.into_iter().collect();
        self.get_message_localized(&locales, message_id, args)
            .ok_or_else(|| Error::msg(format!("Message {} not found", message_id)))
    }

    /// Find the first available locale from a prioritized list
    /// Returns the first locale that has a loaded bundle, or None if none match
    pub fn find_first_available_locale(&self, locales: &[String]) -> Option<String> {
        for locale in locales {
            if self.bundles.contains_key(locale) {
                return Some(locale.clone());
            }
        }
        None
    }
}

fn extract_language_code(filename: &str) -> Option<String> {
    // Extract language code from filename like "errors-en.ftl" -> "en"
    if let Some(stem) = filename.strip_suffix(".ftl") {
        if let Some(dash_pos) = stem.rfind('-') {
            return Some(stem[dash_pos + 1..].to_string());
        }
    }
    None
}

/// What actually went wrong at a Fluent parse error, and where.
struct Diagnosis {
    span: Range<usize>,
    message: String,
    help: Option<String>,
}

/// Say what is wrong at the position the parser reported.
///
/// The parser points at the mistake itself, so this only has to phrase it: its
/// own wording is written for someone who knows the Fluent grammar, and these
/// files are generated, so the same few mistakes recur.
fn diagnose(source: &str, error: &ParserError) -> Diagnosis {
    match &error.kind {
        // A `{` holding something that can't be interpolated. Nearly always a
        // mistyped `$`, since these files are full of `{$1}`, `{$2}`.
        ErrorKind::ExpectedInlineExpression | ErrorKind::ExpectedLiteral
            if opens_placeable(source, error.pos.start) =>
        {
            let span = placeable_span(source, error.pos.start);
            Diagnosis {
                message: format!(
                    "`{}` is not a valid placeable",
                    source.get(span.clone()).unwrap_or_default()
                ),
                span,
                help: Some(
                    "a placeable holds a variable (`{$1}`), a message, a term (`{-name}`) \
                     or a literal — check for a mistyped `$`"
                        .to_string(),
                ),
            }
        }
        ErrorKind::ExpectedToken('=') => Diagnosis {
            span: error.pos.clone(),
            message: "expected `=` here".to_string(),
            help: Some("an attribute is written `.name = value`".to_string()),
        },
        ErrorKind::TabIndentation => Diagnosis {
            span: error.pos.clone(),
            message: "line is indented with a tab".to_string(),
            help: Some("Fluent only accepts spaces for indentation".to_string()),
        },
        kind => Diagnosis {
            span: error.pos.clone(),
            message: kind.to_string(),
            help: None,
        },
    }
}

/// Whether the reported position is the first thing inside a `{`.
fn opens_placeable(source: &str, pos: usize) -> bool {
    source[..pos.min(source.len())]
        .trim_end_matches(' ')
        .ends_with('{')
}

/// The whole `{...}`, so the caret covers what the author has to fix rather
/// than the one character the parser stopped on.
fn placeable_span(source: &str, pos: usize) -> Range<usize> {
    let start = source[..pos].rfind('{').unwrap_or(pos);
    let end = source[pos..]
        .find(|c| c == '}' || c == '\n')
        .map_or(source.len(), |i| pos + i + 1);

    start..end
}

/// Render every parse error in a file as one block, rustc-style: location,
/// the offending line, and a caret under the span that is wrong.
fn render_parse_errors(
    filename: &str,
    resource: &FluentResource,
    errors: &[ParserError],
) -> String {
    use std::fmt::Write as _;

    let source = resource.source();
    let mut out = format!(
        "{filename}: {} message(s) could not be parsed and were skipped; \
         the rest of the file loaded.",
        errors.len()
    );

    for error in errors {
        let diagnosis = diagnose(source, error);
        let (line, col) = line_col(source, diagnosis.span.start);
        let text = source.lines().nth(line.saturating_sub(1)).unwrap_or("");
        // A span can run to the end of the discarded slice; only ever underline
        // what is on the line being shown.
        let width = source
            .get(diagnosis.span.clone())
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .count()
            .max(1);
        let gutter = " ".repeat(line.to_string().len());

        let _ = write!(
            &mut out,
            "\n  {filename}:{line}:{col}: {}\n  {line} | {text}\n  {gutter} | {}{}",
            diagnosis.message,
            " ".repeat(col.saturating_sub(1)),
            "^".repeat(width),
        );
        if let Some(help) = diagnosis.help {
            let _ = write!(&mut out, "\n  {gutter} = help: {help}");
        }
    }

    out
}

/// Convert a byte offset in `source` into a 1-based (line, column) pair, with
/// the column counted in characters. Used to turn Fluent parser byte offsets
/// into human-readable locations (#32).
fn line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (idx, ch) in source.char_indices() {
        if idx >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_language_code() {
        assert_eq!(
            extract_language_code("errors-en.ftl"),
            Some("en".to_string())
        );
        assert_eq!(
            extract_language_code("errors-se.ftl"),
            Some("se".to_string())
        );
        assert_eq!(extract_language_code("errors.ftl"), None);
        assert_eq!(
            extract_language_code("errors-en-US.ftl"),
            Some("US".to_string())
        );
    }

    /// Parse `src`, expect it to fail, and render the report the loader logs.
    fn report(src: &str) -> String {
        let (resource, errors) =
            FluentResource::try_new(src.to_string()).expect_err("should not parse");
        render_parse_errors("errors-se.ftl", &resource, &errors)
    }

    #[test]
    fn points_at_an_invalid_placeable() {
        // The parser blames the indentation of the line above; the mistake is
        // the `€` where a `$` was meant.
        let report = report("unreal-girjji = Title\n    .desc = Oaivvildat go \"{€1}\"?\n");

        assert!(
            report.contains("errors-se.ftl:2:28: `{€1}` is not a valid placeable"),
            "{report}"
        );
        assert!(
            report.contains(
                "\n  2 |     .desc = Oaivvildat go \"{€1}\"?\
                 \n    |                            ^^^^"
            ),
            "caret should sit under `{{€1}}`:\n{report}"
        );
        assert!(report.contains("mistyped `$`"), "{report}");
    }

    #[test]
    fn points_at_an_attribute_missing_its_equals() {
        let report = report("msg = Title\n    .example-1 Dohkko sáhttále buot\n");

        // The caret sits where the `=` should have been, not on the attribute.
        assert!(
            report.contains("errors-se.ftl:2:16: expected `=` here"),
            "{report}"
        );
        assert!(
            report.contains("help: an attribute is written `.name = value`"),
            "{report}"
        );
    }

    #[test]
    fn points_at_tab_indentation() {
        let report = report("msg = Title\n\t.example-3 = Loga eanet\n");

        assert!(
            report.contains("errors-se.ftl:2:1: line is indented with a tab"),
            "{report}"
        );
        assert!(report.contains("only accepts spaces"), "{report}");
    }

    #[test]
    fn falls_back_to_the_parser_message() {
        // Nothing recognisable: keep the parser's own wording rather than
        // guessing.
        let report = report("= Value\n");

        assert!(report.contains("errors-se.ftl:1:1: "), "{report}");
        assert!(
            report.contains("1 message(s) could not be parsed"),
            "{report}"
        );
    }

    #[test]
    fn accepts_the_placeables_these_files_actually_use() {
        for src in [
            "msg = Title\n    .desc = The word {$1} means something else.\n",
            "msg = Title\n    .desc = {$1} and {$2}.\n",
            "msg = Title\n    .desc = { $count ->\n        [one] one\n       *[other] many\n    }\n",
            "msg = Title\n    .desc = A brace: {\"{\"}.\n",
        ] {
            assert!(
                FluentResource::try_new(src.to_string()).is_ok(),
                "should parse: {src:?}"
            );
        }
    }

    #[test]
    fn test_line_col() {
        let src = "key1 = a\nkey2 = b\nbad@ = c\n";
        assert_eq!(line_col(src, 0), (1, 1)); // start of file
        assert_eq!(line_col(src, 9), (2, 1)); // first char of line 2
        assert_eq!(line_col(src, 21), (3, 4)); // the '@' on line 3
        // Column is counted in characters, not bytes.
        let utf8 = "á = x\nkéy = y";
        assert_eq!(line_col(utf8, utf8.find("y =").unwrap()), (2, 3));
    }

    #[test]
    fn test_find_first_available_locale() {
        use std::collections::HashMap;
        use std::sync::Arc;

        // Create a mock FluentLoader with some available locales
        let mut bundles = HashMap::new();
        bundles.insert(
            "en".to_string(),
            Arc::new(fluent_bundle::concurrent::FluentBundle::new_concurrent(
                vec![],
            )),
        );
        bundles.insert(
            "se".to_string(),
            Arc::new(fluent_bundle::concurrent::FluentBundle::new_concurrent(
                vec![],
            )),
        );
        bundles.insert(
            "no".to_string(),
            Arc::new(fluent_bundle::concurrent::FluentBundle::new_concurrent(
                vec![],
            )),
        );

        let loader = FluentLoader {
            bundles,
            default_locale: "en".to_string(),
        };

        // Test finding first available from prioritized list
        let preferred = vec!["fr".to_string(), "se".to_string(), "en".to_string()];
        assert_eq!(
            loader.find_first_available_locale(&preferred),
            Some("se".to_string())
        );

        // Test when first preference is available
        let preferred = vec!["en".to_string(), "se".to_string()];
        assert_eq!(
            loader.find_first_available_locale(&preferred),
            Some("en".to_string())
        );

        // Test when no preferences are available
        let preferred = vec!["fr".to_string(), "de".to_string()];
        assert_eq!(loader.find_first_available_locale(&preferred), None);

        // Test with empty list
        let preferred = vec![];
        assert_eq!(loader.find_first_available_locale(&preferred), None);
    }
}
