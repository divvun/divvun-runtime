use std::collections::HashSet;

use crate::modules::cg3;

pub fn default_sentence_breakers() -> HashSet<String> {
    [".", "!", "?"].iter().map(|s| s.to_string()).collect()
}

pub fn is_sentence_boundary(cohort: &cg3::Cohort<'_>, breakers: &HashSet<String>) -> bool {
    if !breakers.contains(cohort.word_form) {
        return false;
    }
    cohort
        .readings
        .iter()
        .any(|r| r.tags.iter().any(|t| *t == "CLB"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_first_cohort<F: FnOnce(&cg3::Cohort<'_>)>(cg: &str, f: F) {
        let output = cg3::Output::new(cg);
        for block in output.iter().filter_map(Result::ok) {
            if let cg3::Block::Cohort(c) = block {
                f(&c);
                return;
            }
        }
        panic!("no cohort parsed from {cg:?}");
    }

    #[test]
    fn period_with_clb_is_boundary() {
        with_first_cohort("\"<.>\"\n\t\".\" CLB <W:0.0>\n", |c| {
            assert!(is_sentence_boundary(c, &default_sentence_breakers()));
        });
    }

    #[test]
    fn comma_with_clb_is_not_boundary() {
        with_first_cohort("\"<,>\"\n\t\",\" CLB <W:0.0>\n", |c| {
            assert!(!is_sentence_boundary(c, &default_sentence_breakers()));
        });
    }

    #[test]
    fn period_without_clb_is_not_boundary() {
        with_first_cohort("\"<.>\"\n\t\".\" PUNCT <W:0.0>\n", |c| {
            assert!(!is_sentence_boundary(c, &default_sentence_breakers()));
        });
    }

    #[test]
    fn question_mark_respects_custom_whitelist() {
        with_first_cohort("\"<?>\"\n\t\"?\" CLB <W:0.0>\n", |c| {
            let only_period: HashSet<String> = [".".to_string()].into_iter().collect();
            assert!(!is_sentence_boundary(c, &only_period));
        });
    }

    #[test]
    fn bang_with_clb_is_boundary() {
        with_first_cohort("\"<!>\"\n\t\"!\" CLB <W:0.0>\n", |c| {
            assert!(is_sentence_boundary(c, &default_sentence_breakers()));
        });
    }
}
