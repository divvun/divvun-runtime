use std::{collections::HashMap, io::Read, sync::Arc};

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
    pub fn new(context: Arc<Context>, pattern: &str, default_locale: &str) -> Result<Self, Error> {
        let mut bundles = HashMap::new();
        let files = context.load_files_glob(pattern)?;

        for (path, mut reader) in files {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| Error("Invalid filename".to_string()))?;

            // Extract language code from filename like "errors-en.ftl" -> "en"
            if let Some(lang_code) = extract_language_code(filename) {
                let mut content = String::new();
                reader
                    .read_to_string(&mut content)
                    .map_err(|e| Error(format!("Failed to read file {}: {}", filename, e)))?;

                // Try to parse the Fluent resource, but continue even if it fails
                match FluentResource::try_new(content) {
                    Ok(resource) => {
                        let lang_id: LanguageIdentifier = lang_code.parse().map_err(|e| {
                            Error(format!("Invalid language identifier {}: {}", lang_code, e))
                        })?;

                        let mut bundle = FluentBundle::new_concurrent(vec![lang_id]);
                        match bundle.add_resource(resource) {
                            Ok(_) => {
                                tracing::info!("Successfully loaded Fluent resource: {}", filename);
                            }
                            Err(errors) => {
                                // Check if errors are only "Overriding" errors (which are non-fatal)
                                let non_fatal = errors.iter().all(|e| {
                                    matches!(e, fluent_bundle::FluentError::Overriding { .. })
                                });
                                if non_fatal {
                                    tracing::debug!(
                                        "Fluent resource {} has overriding messages (normal for localization): {:?}",
                                        filename,
                                        errors
                                    );
                                } else {
                                    tracing::warn!(
                                        "Fluent resource {} has errors: {:?}",
                                        filename,
                                        errors
                                    );
                                }
                            }
                        }
                        // Add the bundle regardless of overriding errors
                        bundles.insert(lang_code, Arc::new(bundle));
                    }
                    Err((_, errors)) => {
                        tracing::warn!(
                            "Failed to parse Fluent resource {}: {} error(s). Skipping this file.",
                            filename,
                            errors.len()
                        );
                        for (i, error) in errors.iter().enumerate() {
                            tracing::warn!("  Error {}: {:?}", i + 1, error);
                        }
                    }
                }
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

    pub fn get_message(
        &self,
        locale: Option<&str>,
        message_id: &str,
        args: Option<&FluentArgs>,
    ) -> Result<(String, String), Error> {
        let locale = locale.unwrap_or(&self.default_locale);

        // If no bundles loaded, fall back to error ID
        if self.bundles.is_empty() {
            tracing::debug!(
                "No Fluent bundles available, falling back to error ID: {}",
                message_id
            );
            return Ok((message_id.to_string(), message_id.to_string()));
        }

        let bundle = self
            .bundles
            .get(locale)
            .or_else(|| self.bundles.get(&self.default_locale))
            .or_else(|| self.bundles.values().next()) // Use any available bundle as last resort
            .ok_or_else(|| {
                Error(format!(
                    "No bundle found for locale {} or default {}",
                    locale, self.default_locale
                ))
            })?;

        let message = bundle.get_message(message_id).ok_or_else(|| {
            Error(format!(
                "Message {} not found in locale {}",
                message_id, locale
            ))
        })?;

        let pattern = message
            .value()
            .ok_or_else(|| Error(format!("Message {} has no value", message_id)))?;

        let title = bundle.format_pattern(pattern, args, &mut vec![]);

        // Try to get description from .desc attribute
        let desc_pattern = message
            .attributes()
            .find(|attr| attr.id() == "desc")
            .and_then(|attr| Some(attr.value()));

        let description = if let Some(desc_pattern) = desc_pattern {
            bundle.format_pattern(desc_pattern, args, &mut vec![])
        } else {
            title.clone()
        };

        Ok((title.into_owned(), description.into_owned()))
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
}
