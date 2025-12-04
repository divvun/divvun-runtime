use serde::{Deserialize, Serialize};
use super::error_types::ErrorType;

/// Represents the content of an error, which can be either plain text or nested markup
/// 
/// Example of nested markup: {{epoxi}${noun,cons|epoksy} lim}¢{noun,mix|epoksylim}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ErrorContent {
    /// Plain text content
    Text(String),
    /// Nested error markup with text segments
    Nested(Vec<ErrorSegment>),
}

/// A segment within an error, either plain text or a nested error
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ErrorSegment {
    /// Plain text segment
    Text(String),
    /// Nested error markup
    Error(Box<ErrorMarkup>),
}

/// Represents a marked up error in a sentence
/// 
/// Example simple markup: {čohke}¢{čohkke}
/// This indicates an error "čohke" with correction "čohkke" of type "errorortreal" (¢)
/// 
/// Example nested markup: {{epoxi}${noun,cons|epoksy} lim}¢{noun,mix|epoksylim}
/// This indicates a nested error where "epoxi" has its own correction within a larger error
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorMarkup {
    /// The error form/text, which can be plain text or contain nested errors
    pub form: ErrorContent,
    
    /// Start position in the sentence (byte offset)
    pub start: usize,
    
    /// End position in the sentence (byte offset)
    pub end: usize,
    
    /// Error type
    pub errortype: ErrorType,
    
    /// Optional comment about the error
    #[serde(default)]
    pub comment: String,
    
    /// List of suggested corrections (can include errorinfo like "noun,cons")
    #[serde(default)]
    pub suggestions: Vec<String>,
}

impl ErrorMarkup {
    /// Create a new ErrorMarkup with plain text content
    pub fn new(
        form: String,
        start: usize,
        end: usize,
        errortype: ErrorType,
    ) -> Self {
        Self {
            form: ErrorContent::Text(form),
            start,
            end,
            errortype,
            comment: String::new(),
            suggestions: Vec::new(),
        }
    }

    /// Create a new ErrorMarkup with nested content
    pub fn new_nested(
        segments: Vec<ErrorSegment>,
        start: usize,
        end: usize,
        errortype: ErrorType,
    ) -> Self {
        Self {
            form: ErrorContent::Nested(segments),
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
            form: ErrorContent::Text(form),
            start,
            end,
            errortype,
            comment: String::new(),
            suggestions,
        }
    }

    /// Create a new ErrorMarkup with suggestions and comment/errorinfo
    /// The comment typically contains errorinfo like "noun,mix" or "noun,cons"
    pub fn with_suggestions_and_comment(
        form: String,
        start: usize,
        end: usize,
        errortype: ErrorType,
        suggestions: Vec<String>,
        comment: String,
    ) -> Self {
        Self {
            form: ErrorContent::Text(form),
            start,
            end,
            errortype,
            comment,
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
    
    /// Get the form as a string (extracts text from nested structures)
    pub fn form_as_string(&self) -> String {
        match &self.form {
            ErrorContent::Text(s) => s.clone(),
            ErrorContent::Nested(segments) => {
                segments.iter().map(|seg| match seg {
                    ErrorSegment::Text(s) => s.clone(),
                    ErrorSegment::Error(err) => err.form_as_string(),
                }).collect()
            }
        }
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
        
        assert_eq!(markup.form, ErrorContent::Text("čohke".to_string()));
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
    fn test_nested_error_markup() {
        // Example: {{epoxi}${noun,cons|epoksy} lim}¢{noun,mix|epoksylim}
        // Inner error: {epoxi}${noun,cons|epoksy}
        let inner_error = ErrorMarkup::with_suggestions_and_comment(
            "epoxi".to_string(),
            0,
            5,
            ErrorType::Errorort,
            vec!["epoksy".to_string()],
            "noun,cons".to_string(),
        );
        
        // Outer error contains the inner error and " lim"
        let mut outer_markup = ErrorMarkup::new_nested(
            vec![
                ErrorSegment::Error(Box::new(inner_error)),
                ErrorSegment::Text(" lim".to_string()),
            ],
            0,
            10,
            ErrorType::Errorortreal,
        );
        outer_markup.comment = "noun,mix".to_string();
        outer_markup.suggestions = vec!["epoksylim".to_string()];
        
        // Verify structure
        match &outer_markup.form {
            ErrorContent::Nested(segments) => {
                assert_eq!(segments.len(), 2);
                match &segments[0] {
                    ErrorSegment::Error(inner) => {
                        assert_eq!(inner.form, ErrorContent::Text("epoxi".to_string()));
                        assert_eq!(inner.errortype, ErrorType::Errorort);
                        assert_eq!(inner.comment, "noun,cons");
                        assert_eq!(inner.suggestions, vec!["epoksy"]);
                    }
                    _ => panic!("Expected Error segment"),
                }
                match &segments[1] {
                    ErrorSegment::Text(text) => {
                        assert_eq!(text, " lim");
                    }
                    _ => panic!("Expected Text segment"),
                }
            }
            _ => panic!("Expected nested content"),
        }
        assert_eq!(outer_markup.comment, "noun,mix");
        assert_eq!(outer_markup.suggestions, vec!["epoksylim"]);
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
    
    #[test]
    fn test_nested_json_serialization() {
        let inner_error = ErrorMarkup::with_suggestions(
            "epoxi".to_string(),
            0,
            5,
            ErrorType::Errorort,
            vec!["epoksy".to_string()],
        );
        
        let outer_markup = ErrorMarkup::new_nested(
            vec![
                ErrorSegment::Error(Box::new(inner_error)),
                ErrorSegment::Text(" lim".to_string()),
            ],
            0,
            10,
            ErrorType::Errorortreal,
        );
        
        let json = serde_json::to_string_pretty(&outer_markup).unwrap();
        let deserialized: ErrorMarkup = serde_json::from_str(&json).unwrap();
        
        assert_eq!(outer_markup, deserialized);
    }
}
