/// YAML test file parsing for grammar checker tests
///
/// Parses YAML files with structure:
/// ```yaml
/// Config:
///   Spec: ../pipespec.xml
///   Variants: [smegram-dev]
/// Tests:
///   - "sentence with {error}${correction}"
///   - "another sentence"
/// ```
use serde::{Deserialize, Serialize};
use super::sentence::ErrorAnnotatedSentence;
use super::parser::parse_markup;

/// Configuration section of the YAML test file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(rename = "Spec")]
    pub spec: Option<String>,
    
    #[serde(rename = "Variants")]
    pub variants: Vec<String>,
}

/// Top-level structure of a YAML test file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct YamlTestFile {
    #[serde(rename = "Config")]
    pub config: Config,
    
    #[serde(rename = "Tests")]
    pub tests: Vec<String>,
}

impl YamlTestFile {
    /// Parse a YAML test file from a string
    pub fn from_str(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }
    
    /// Load a YAML test file from a file path
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(Self::from_str(&content)?)
    }
    
    /// Get the variant (first/only one from the Variants array)
    pub fn variant(&self) -> Option<&str> {
        self.config.variants.first().map(|s| s.as_str())
    }
    
    /// Parse all test sentences into ErrorAnnotatedSentence structures
    pub fn parse_tests(&self) -> Vec<Result<ErrorAnnotatedSentence, super::parser::ParseError>> {
        self.tests.iter()
            .map(|test| parse_markup(test))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_yaml() {
        let yaml = r#"
Config:
  Spec: ../pipespec.xml
  Variants: [smegram-dev]

Tests:
  - "Plain sentence"
  - "{error}${correction}"
"#;
        
        let parsed = YamlTestFile::from_str(yaml).unwrap();
        
        assert_eq!(parsed.config.spec, Some("../pipespec.xml".to_string()));
        assert_eq!(parsed.config.variants, vec!["smegram-dev"]);
        assert_eq!(parsed.variant(), Some("smegram-dev"));
        assert_eq!(parsed.tests.len(), 2);
        assert_eq!(parsed.tests[0], "Plain sentence");
        assert_eq!(parsed.tests[1], "{error}${correction}");
    }
    
    #[test]
    fn test_parse_tests() {
        let yaml = r#"
Config:
  Variants: [smegram-dev]

Tests:
  - "Plain sentence"
  - "{čohke}¢{čohkke}"
"#;
        
        let parsed = YamlTestFile::from_str(yaml).unwrap();
        let sentences = parsed.parse_tests();
        
        assert_eq!(sentences.len(), 2);
        
        // First sentence should parse successfully
        let sentence1 = sentences[0].as_ref().unwrap();
        assert_eq!(sentence1.text, "Plain sentence");
        assert_eq!(sentence1.error_count(), 0);
        
        // Second sentence should have an error
        let sentence2 = sentences[1].as_ref().unwrap();
        assert_eq!(sentence2.text, "čohke");
        assert_eq!(sentence2.error_count(), 1);
    }
}
