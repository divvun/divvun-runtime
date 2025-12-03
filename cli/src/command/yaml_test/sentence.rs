use super::markup::ErrorMarkup;
use serde::{Deserialize, Serialize};

/// Represents a sentence with zero or more error markups
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorAnnotatedSentence {
    /// The original text of the sentence
    pub text: String,

    /// List of errors found in the sentence
    #[serde(default)]
    pub errors: Vec<ErrorMarkup>,
}

impl ErrorAnnotatedSentence {
    /// Create a new sentence with no errors
    pub fn new(text: String) -> Self {
        Self {
            text,
            errors: Vec::new(),
        }
    }

    /// Create a new sentence with errors
    pub fn with_errors(text: String, errors: Vec<ErrorMarkup>) -> Self {
        Self { text, errors }
    }

    /// Add an error to the sentence
    pub fn add_error(&mut self, error: ErrorMarkup) {
        self.errors.push(error);
    }

    /// Check if the sentence has any errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Get the number of errors in the sentence
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::error_types::ErrorType;

    #[test]
    fn test_sentence_no_errors() {
        let sentence = ErrorAnnotatedSentence::new("Muittán doložiid".to_string());

        assert_eq!(sentence.text, "Muittán doložiid");
        assert_eq!(sentence.has_errors(), false);
        assert_eq!(sentence.error_count(), 0);
    }

    #[test]
    fn test_sentence_with_errors() {
        let error = ErrorMarkup::with_suggestions(
            "čohke".to_string(),
            0,
            6,
            ErrorType::Errorortreal,
            vec!["čohkke".to_string()],
        );

        let sentence = ErrorAnnotatedSentence::with_errors("čohke is wrong".to_string(), vec![error]);

        assert_eq!(sentence.has_errors(), true);
        assert_eq!(sentence.error_count(), 1);
    }

    #[test]
    fn test_add_error() {
        let mut sentence = ErrorAnnotatedSentence::new("some text".to_string());
        assert_eq!(sentence.error_count(), 0);

        sentence.add_error(ErrorMarkup::new(
            "error".to_string(),
            0,
            5,
            ErrorType::Error,
        ));

        assert_eq!(sentence.error_count(), 1);
    }

    #[test]
    fn test_json_serialization() {
        let error = ErrorMarkup::with_suggestions(
            "čohke".to_string(),
            0,
            6,
            ErrorType::Errorortreal,
            vec!["čohkke".to_string()],
        );

        let sentence = ErrorAnnotatedSentence::with_errors("čohke is wrong".to_string(), vec![error]);

        let json = serde_json::to_string_pretty(&sentence).unwrap();
        let deserialized: ErrorAnnotatedSentence = serde_json::from_str(&json).unwrap();

        assert_eq!(sentence, deserialized);
    }
}
