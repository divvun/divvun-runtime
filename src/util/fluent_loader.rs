use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use fluent::FluentResource;
use fluent_bundle::{FluentArgs, concurrent::FluentBundle};
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
                        tracing::warn!(
                            "Fluent resource {} has {} parse error(s); the messages that parsed were still loaded.",
                            filename,
                            errors.len()
                        );
                        // Report each error with file:line:column and a snippet of
                        // the offending line, instead of a raw byte offset (#32).
                        let source = resource.source();
                        for error in &errors {
                            let (line, col) = line_col(source, error.pos.start);
                            let snippet =
                                source.lines().nth(line.saturating_sub(1)).unwrap_or("");
                            tracing::warn!(
                                "  {}:{}:{}: {}\n    {}\n    {}^",
                                filename,
                                line,
                                col,
                                error.kind,
                                snippet,
                                " ".repeat(col.saturating_sub(1))
                            );
                        }
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
