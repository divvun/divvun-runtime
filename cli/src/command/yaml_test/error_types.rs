/// Error type symbols and their corresponding error categories
/// 
/// These symbols are used in the markup syntax to indicate different types of errors:
/// - Basic syntax: {error}${correction}
/// - Nested syntax: {{error1}${correction_of_error1}}£{correction_of_correction1}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorType {
    /// Orthographic error (spelling)
    Errorort,
    /// Real orthographic error
    Errorortreal,
    /// Lexical error
    Errorlex,
    /// Morphosyntactic error
    Errormorphsyn,
    /// Syntactic error
    Errorsyn,
    /// Generic error
    Error,
    /// Language error
    Errorlang,
    /// Format error
    Errorformat,
}

impl ErrorType {
    /// Get the symbol character for this error type
    pub fn symbol(&self) -> char {
        match self {
            ErrorType::Errorort => '$',
            ErrorType::Errorortreal => '¢',
            ErrorType::Errorlex => '€',
            ErrorType::Errormorphsyn => '£',
            ErrorType::Errorsyn => '¥',
            ErrorType::Error => '§',
            ErrorType::Errorlang => '∞',
            ErrorType::Errorformat => '‰',
        }
    }

    /// Parse an error type from a symbol character
    pub fn from_symbol(symbol: char) -> Option<ErrorType> {
        match symbol {
            '$' => Some(ErrorType::Errorort),
            '¢' => Some(ErrorType::Errorortreal),
            '€' => Some(ErrorType::Errorlex),
            '£' => Some(ErrorType::Errormorphsyn),
            '¥' => Some(ErrorType::Errorsyn),
            '§' => Some(ErrorType::Error),
            '∞' => Some(ErrorType::Errorlang),
            '‰' => Some(ErrorType::Errorformat),
            _ => None,
        }
    }

    /// Get all valid error type symbols
    pub fn all_symbols() -> Vec<char> {
        vec!['$', '¢', '€', '£', '¥', '§', '∞', '‰']
    }

    /// Get the name of the error type as a string
    pub fn name(&self) -> &'static str {
        match self {
            ErrorType::Errorort => "errorort",
            ErrorType::Errorortreal => "errorortreal",
            ErrorType::Errorlex => "errorlex",
            ErrorType::Errormorphsyn => "errormorphsyn",
            ErrorType::Errorsyn => "errorsyn",
            ErrorType::Error => "error",
            ErrorType::Errorlang => "errorlang",
            ErrorType::Errorformat => "errorformat",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_roundtrip() {
        for error_type in [
            ErrorType::Errorort,
            ErrorType::Errorortreal,
            ErrorType::Errorlex,
            ErrorType::Errormorphsyn,
            ErrorType::Errorsyn,
            ErrorType::Error,
            ErrorType::Errorlang,
            ErrorType::Errorformat,
        ] {
            let symbol = error_type.symbol();
            assert_eq!(ErrorType::from_symbol(symbol), Some(error_type));
        }
    }

    #[test]
    fn test_invalid_symbol() {
        assert_eq!(ErrorType::from_symbol('x'), None);
        assert_eq!(ErrorType::from_symbol('@'), None);
    }

    #[test]
    fn test_name() {
        assert_eq!(ErrorType::Errorort.name(), "errorort");
        assert_eq!(ErrorType::Errormorphsyn.name(), "errormorphsyn");
    }
}
