pub(crate) mod fluent_loader;
pub(crate) mod shared_box;

pub(crate) use shared_box::SharedBox;

// Public API functions - for external users of this crate
pub fn parse_accept_language(header: &str) -> Vec<(unic_langid::LanguageIdentifier, f32)> {
    let mut languages = Vec::new();

    for entry in header.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }

        let (lang_tag, quality) = if let Some(semicolon_pos) = entry.find(';') {
            let lang_part = entry[..semicolon_pos].trim();
            let q_part = entry[semicolon_pos + 1..].trim();

            let quality = if let Some(q_value) = q_part.strip_prefix("q=") {
                q_value.parse::<f32>().unwrap_or(1.0).clamp(0.0, 1.0)
            } else {
                1.0
            };

            (lang_part, quality)
        } else {
            (entry, 1.0)
        };

        if let Ok(lang_id) = lang_tag.parse::<unic_langid::LanguageIdentifier>() {
            languages.push((lang_id, quality));
        }
    }

    // Sort by quality value (highest first)
    languages.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    languages
}

#[cfg(test)]
mod tests {
    use super::*;
    use unic_langid::langid;

    #[test]
    fn test_parse_accept_language_simple() {
        let result = parse_accept_language("en-US");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, langid!("en-US"));
        assert_eq!(result[0].1, 1.0);
    }

    #[test]
    fn test_parse_accept_language_with_quality() {
        let result = parse_accept_language("en-US,en;q=0.9,se;q=0.8");
        assert_eq!(result.len(), 3);

        // Should be sorted by quality (highest first)
        assert_eq!(result[0].0, langid!("en-US"));
        assert_eq!(result[0].1, 1.0);

        assert_eq!(result[1].0, langid!("en"));
        assert_eq!(result[1].1, 0.9);

        assert_eq!(result[2].0, langid!("se"));
        assert_eq!(result[2].1, 0.8);
    }

    #[test]
    fn test_parse_accept_language_complex() {
        let result = parse_accept_language("fr-CH, fr;q=0.9, en;q=0.8, de;q=0.7, *;q=0.5");
        assert_eq!(result.len(), 4); // * is not a valid language identifier

        assert_eq!(result[0].0, langid!("fr-CH"));
        assert_eq!(result[0].1, 1.0);

        assert_eq!(result[1].0, langid!("fr"));
        assert_eq!(result[1].1, 0.9);

        assert_eq!(result[2].0, langid!("en"));
        assert_eq!(result[2].1, 0.8);

        assert_eq!(result[3].0, langid!("de"));
        assert_eq!(result[3].1, 0.7);
    }

    #[test]
    fn test_parse_accept_language_empty() {
        let result = parse_accept_language("");
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_parse_accept_language_invalid() {
        let result = parse_accept_language("invalid-lang-tag, en");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, langid!("en"));
        assert_eq!(result[0].1, 1.0);
    }

    #[test]
    fn test_parse_accept_language_quality_bounds() {
        let result = parse_accept_language("en;q=1.5,se;q=-0.1");
        assert_eq!(result.len(), 2);

        // Quality should be clamped to [0.0, 1.0]
        assert_eq!(result[0].1, 1.0); // clamped from 1.5
        assert_eq!(result[1].1, 0.0); // clamped from -0.1
    }

    #[test]
    fn test_parse_accept_language_whitespace() {
        let result = parse_accept_language(" en-US , fr ; q=0.8 , de ");
        assert_eq!(result.len(), 3);

        // Both en-US and de have quality 1.0, fr has quality 0.8
        // So the first two should be the 1.0 quality languages, last should be fr
        assert_eq!(result[0].1, 1.0);
        assert_eq!(result[1].1, 1.0);
        assert_eq!(result[2].0, langid!("fr"));
        assert_eq!(result[2].1, 0.8);

        // Verify that the 1.0 quality languages are present (order may vary)
        let quality_1_langs: std::collections::HashSet<_> =
            result[..2].iter().map(|x| &x.0).collect();
        assert!(quality_1_langs.contains(&langid!("en-US")));
        assert!(quality_1_langs.contains(&langid!("de")));
    }
}
