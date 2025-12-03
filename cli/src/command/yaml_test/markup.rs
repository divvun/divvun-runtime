use serde::{Deserialize, Serialize};
use super::error_types::ErrorType;

/// Represents a marked up error in a sentence
/// 
/// Example markup: {čohke}¢{čohkke}
/// This indicates an error "čohke" with correction "čohkke" of type "errorortreal" (¢)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorMarkup {
    /// The error form/text
    pub form: String,
    
    /// Start position in the sentence (byte offset)
    pub start: usize,
    
    /// End position in the sentence (byte offset)
    pub end: usize,
    
    /// Error type
    pub errortype: ErrorType,
    
    /// Optional comment about the error
    #[serde(default)]
    pub comment: String,
    
    /// List of suggested corrections
    #[serde(default)]
    pub suggestions: Vec<String>,
}

impl ErrorMarkup {
    /// Create a new ErrorMarkup
    pub fn new(
        form: String,
        start: usize,
        end: usize,
        errortype: ErrorType,
    ) -> Self {
        Self {
            form,
            start,
            end,
            errortype,
            comment: String::new(),
            suggestions: Vec::new(),
        }
    }

    /// Create a new ErrorMarkup with suggestions
    pub fn with_suggestions(
        form: String,
        start: usize,
        end: usize,
        errortype: ErrorType,
        suggestions: Vec<String>,
    ) -> Self {
        Self {
            form,
            start,
            end,
            errortype,
            comment: String::new(),
            suggestions,
        }
    }

    /// Add a comment to the error
    pub fn with_comment(mut self, comment: String) -> Self {
        self.comment = comment;
        self
    }

    /// Add a suggestion to the error
    pub fn add_suggestion(&mut self, suggestion: String) {
        self.suggestions.push(suggestion);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_error_markup() {
        let markup = ErrorMarkup::new(
            "čohke".to_string(),
            0,
            6,
            ErrorType::Errorortreal,
        );
        
        assert_eq!(markup.form, "čohke");
        assert_eq!(markup.start, 0);
        assert_eq!(markup.end, 6);
        assert_eq!(markup.errortype, ErrorType::Errorortreal);
        assert_eq!(markup.comment, "");
        assert_eq!(markup.suggestions.len(), 0);
    }

    #[test]
    fn test_with_suggestions() {
        let markup = ErrorMarkup::with_suggestions(
            "čohke".to_string(),
            0,
            6,
            ErrorType::Errorortreal,
            vec!["čohkke".to_string()],
        );
        
        assert_eq!(markup.suggestions, vec!["čohkke"]);
    }

    #[test]
    fn test_json_serialization() {
        let markup = ErrorMarkup::with_suggestions(
            "čohke".to_string(),
            0,
            6,
            ErrorType::Errorortreal,
            vec!["čohkke".to_string()],
        );
        
        let json = serde_json::to_string_pretty(&markup).unwrap();
        let deserialized: ErrorMarkup = serde_json::from_str(&json).unwrap();
        
        assert_eq!(markup, deserialized);
    }
}
