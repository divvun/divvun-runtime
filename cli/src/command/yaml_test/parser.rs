/// Parser for error markup syntax
///
/// Parses markup like:
/// - Simple: {error}${correction}
/// - With errorinfo: {error}${errorinfo|correction}
/// - Nested: {{inner}${correction1}}${correction2}
/// - Multiple corrections: {error}${corr1///corr2///corr3}
use super::error_types::ErrorType;
use super::markup::{ErrorMarkup, ErrorSegment};
use super::sentence::ErrorAnnotatedSentence;

#[cfg(test)]
use super::markup::ErrorContent;

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    UnexpectedEndOfInput,
    UnmatchedBrace,
    InvalidErrorSymbol(char),
    MissingCorrection,
    InvalidFormat(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedEndOfInput => write!(f, "Unexpected end of input"),
            ParseError::UnmatchedBrace => write!(f, "Unmatched brace"),
            ParseError::InvalidErrorSymbol(c) => write!(f, "Invalid error symbol: {}", c),
            ParseError::MissingCorrection => write!(f, "Missing correction after error symbol"),
            ParseError::InvalidFormat(s) => write!(f, "Invalid format: {}", s),
        }
    }
}

impl std::error::Error for ParseError {}

pub type ParseResult<T> = Result<T, ParseError>;

/// Parser for markup syntax
pub struct Parser {
    input: Vec<char>,
    pos: usize,
    byte_pos: usize,
}

impl Parser {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            byte_pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied();
        if let Some(c) = ch {
            self.pos += 1;
            self.byte_pos += c.len_utf8();
        }
        ch
    }

    fn expect(&mut self, expected: char) -> ParseResult<()> {
        match self.advance() {
            Some(ch) if ch == expected => Ok(()),
            Some(ch) => Err(ParseError::InvalidFormat(format!(
                "Expected '{}', found '{}'",
                expected, ch
            ))),
            None => Err(ParseError::UnexpectedEndOfInput),
        }
    }

    /// Parse the entire input and extract all errors with their positions
    pub fn parse_all(&mut self) -> ParseResult<Vec<ErrorMarkup>> {
        let mut errors = Vec::new();

        while self.pos < self.input.len() {
            if self.peek() == Some('{') {
                let error = self.parse_error()?;
                errors.push(error);
            } else {
                self.advance();
            }
        }

        Ok(errors)
    }

    /// Parse a single error markup: {content}SYMBOL{correction}
    fn parse_error(&mut self) -> ParseResult<ErrorMarkup> {
        let start_byte = self.byte_pos;
        self.expect('{')?;

        // Parse the error content (can be text or nested errors)
        let content = self.parse_content()?;

        self.expect('}')?;

        // Get the error type symbol
        let symbol = self.advance().ok_or(ParseError::UnexpectedEndOfInput)?;
        let error_type =
            ErrorType::from_symbol(symbol).ok_or(ParseError::InvalidErrorSymbol(symbol))?;

        // Parse correction: {errorinfo|correction} or {correction}
        self.expect('{')?;
        let (comment, suggestions) = self.parse_correction()?;
        self.expect('}')?;

        let end_byte = self.byte_pos;

        let mut error = match content {
            Content::Text(text) => ErrorMarkup::new(text, start_byte, end_byte, error_type),
            Content::Segments(segments) => {
                ErrorMarkup::new_nested(segments, start_byte, end_byte, error_type)
            }
        };

        if let Some(c) = comment {
            error.comment = c;
        }
        error.suggestions = suggestions;

        Ok(error)
    }

    /// Parse content inside braces (can be text, nested errors, or mix)
    fn parse_content(&mut self) -> ParseResult<Content> {
        let mut segments = Vec::new();
        let mut current_text = String::new();
        let mut has_nested_errors = false;

        while let Some(ch) = self.peek() {
            match ch {
                '{' => {
                    // Save any accumulated text
                    if !current_text.is_empty() {
                        segments.push(ErrorSegment::Text(current_text.clone()));
                        current_text.clear();
                    }

                    // Parse nested error
                    let nested_error = self.parse_error()?;
                    segments.push(ErrorSegment::Error(Box::new(nested_error)));
                    has_nested_errors = true;
                }
                '}' => {
                    // End of content
                    break;
                }
                _ => {
                    current_text.push(ch);
                    self.advance();
                }
            }
        }

        if has_nested_errors {
            // Add any remaining text
            if !current_text.is_empty() {
                segments.push(ErrorSegment::Text(current_text));
            }
            Ok(Content::Segments(segments))
        } else {
            Ok(Content::Text(current_text))
        }
    }

    /// Parse correction: errorinfo|correction or just correction
    /// Can also handle multiple corrections separated by ///
    fn parse_correction(&mut self) -> ParseResult<(Option<String>, Vec<String>)> {
        let mut buffer = String::new();

        while let Some(ch) = self.peek() {
            if ch == '}' {
                break;
            }
            buffer.push(ch);
            self.advance();
        }

        // Check if there's errorinfo (contains |)
        if let Some(pipe_pos) = buffer.find('|') {
            let errorinfo = buffer[..pipe_pos].to_string();
            let corrections_part = &buffer[pipe_pos + 1..];

            // Split multiple corrections by ///
            let suggestions: Vec<String> = corrections_part
                .split("///")
                .map(|s| s.to_string())
                .collect();

            Ok((Some(errorinfo), suggestions))
        } else {
            // No errorinfo, just correction(s)
            let suggestions: Vec<String> = buffer.split("///").map(|s| s.to_string()).collect();

            Ok((None, suggestions))
        }
    }
}

#[derive(Debug, Clone)]
enum Content {
    Text(String),
    Segments(Vec<ErrorSegment>),
}

/// Parse a string containing error markup and return a Sentence
pub fn parse_markup(input: &str) -> ParseResult<ErrorAnnotatedSentence> {
    let mut parser = Parser::new(input);
    let errors = parser.parse_all()?;
    let plain_text = extract_plain_text(input);
    Ok(ErrorAnnotatedSentence::with_errors(plain_text, errors))
}

/// Extract plain text from markup by removing all error markup syntax
/// Keeps all text including before, between, and after errors
fn extract_plain_text(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            // Extract the error form (content between { and })
            let mut form = String::new();
            let mut brace_depth = 1;

            for c in chars.by_ref() {
                if c == '{' {
                    brace_depth += 1;
                    form.push(c);
                } else if c == '}' {
                    brace_depth -= 1;
                    if brace_depth == 0 {
                        break;
                    }
                    form.push(c);
                } else {
                    form.push(c);
                }
            }

            // Skip the error symbol
            chars.next();

            // Skip the correction part {correction}
            if chars.peek() == Some(&'{') {
                chars.next(); // consume '{'
                let mut brace_depth = 1;
                for c in chars.by_ref() {
                    if c == '{' {
                        brace_depth += 1;
                    } else if c == '}' {
                        brace_depth -= 1;
                        if brace_depth == 0 {
                            break;
                        }
                    }
                }
            }

            // Recursively process the form if it contains nested markup
            if form.contains('{') {
                result.push_str(&extract_plain_text(&form));
            } else {
                result.push_str(&form);
            }
        } else {
            // Include all text outside of markup
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_error() {
        let input = "{čohke}¢{čohkke}";
        let sentence = parse_markup(input).unwrap();

        assert_eq!(sentence.text, "čohke");
        assert_eq!(sentence.error_count(), 1);
        assert_eq!(sentence.errors[0].errortype, ErrorType::Errorortreal);
        assert_eq!(sentence.errors[0].suggestions, vec!["čohkke"]);
    }

    #[test]
    fn test_error_with_errorinfo() {
        let input = "{jne.}${adv,typo|jna.}";
        let sentence = parse_markup(input).unwrap();

        assert_eq!(sentence.text, "jne.");
        assert_eq!(sentence.error_count(), 1);
        assert_eq!(sentence.errors[0].errortype, ErrorType::Errorort);
        assert_eq!(sentence.errors[0].comment, "adv,typo");
        assert_eq!(sentence.errors[0].suggestions, vec!["jna."]);
    }

    #[test]
    fn test_multiple_corrections() {
        let input = "{leimme}£{leimmet///leat}";
        let sentence = parse_markup(input).unwrap();

        assert_eq!(sentence.text, "leimme");
        assert_eq!(sentence.error_count(), 1);
        assert_eq!(sentence.errors[0].errortype, ErrorType::Errormorphsyn);
        assert_eq!(sentence.errors[0].suggestions, vec!["leimmet", "leat"]);
    }

    #[test]
    fn test_nested_error() {
        let input = "{{epoxi}${noun,cons|epoksy} lim}¢{noun,mix|epoksylim}";
        let sentence = parse_markup(input).unwrap();

        assert_eq!(sentence.text, "epoxi lim");
        assert_eq!(sentence.error_count(), 1);
        assert_eq!(sentence.errors[0].errortype, ErrorType::Errorortreal);
        assert_eq!(sentence.errors[0].comment, "noun,mix");

        match &sentence.errors[0].form {
            ErrorContent::Nested(segments) => {
                assert_eq!(segments.len(), 2);
                match &segments[0] {
                    ErrorSegment::Error(inner) => {
                        assert_eq!(inner.errortype, ErrorType::Errorort);
                        assert_eq!(inner.comment, "noun,cons");
                    }
                    _ => panic!("Expected Error segment"),
                }
            }
            _ => panic!("Expected nested content"),
        }
    }

    #[test]
    fn test_text_with_error() {
        let input = "Some text {error}${correction} more text";
        let sentence = parse_markup(input).unwrap();

        assert_eq!(sentence.text, "Some text error more text");
        assert_eq!(sentence.error_count(), 1);
        assert_eq!(sentence.errors[0].errortype, ErrorType::Errorort);
    }

    #[test]
    fn test_multiple_errors_in_text() {
        let input = "{error1}${corr1} text {error2}¢{corr2}";
        let sentence = parse_markup(input).unwrap();

        assert_eq!(sentence.text, "error1 text error2");
        assert_eq!(sentence.error_count(), 2);
        assert_eq!(sentence.errors[0].errortype, ErrorType::Errorort);
        assert_eq!(sentence.errors[1].errortype, ErrorType::Errorortreal);
    }

    #[test]
    fn test_empty_correction() {
        let input = "{ovtta}¥{num,redun|}";
        let sentence = parse_markup(input).unwrap();

        assert_eq!(sentence.text, "ovtta");
        assert_eq!(sentence.error_count(), 1);
        assert_eq!(sentence.errors[0].comment, "num,redun");
        assert_eq!(sentence.errors[0].suggestions, vec![""]);
    }
}
