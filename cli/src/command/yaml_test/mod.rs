mod error_types;
mod markup;
mod parser;
mod sentence;
mod yaml_file;

pub use error_types::ErrorType;
pub use markup::{ErrorContent, ErrorMarkup, ErrorSegment};
pub use parser::{parse_markup, ParseError};
pub use sentence::ErrorAnnotatedSentence;
pub use yaml_file::{Config, YamlTestFile};

use crate::cli::YamlTestArgs;
use crate::shell::Shell;

pub async fn yaml_test(_shell: &mut Shell, args: YamlTestArgs) -> anyhow::Result<()> {
    let yaml_file = YamlTestFile::from_file(args.yaml_file.to_str().unwrap())?;
    
    println!("Loaded test file:");
    println!("  Config: {:?}", yaml_file.config);
    println!("  Number of tests: {}", yaml_file.tests.len());
    
    if let Some(variant) = yaml_file.variant() {
        println!("  Variant: {}", variant);
    }
    
    // Parse all tests
    let parsed_tests = yaml_file.parse_tests();
    let mut success_count = 0;
    let mut error_count = 0;
    
    for (i, result) in parsed_tests.iter().enumerate() {
        match result {
            Ok(sentence) => {
                success_count += 1;
                println!("Test {}: {} (errors: {})", i + 1, sentence.text, sentence.error_count());
            }
            Err(e) => {
                error_count += 1;
                println!("Test {} failed to parse: {:?}", i + 1, e);
            }
        }
    }
    
    println!("\nSummary:");
    println!("  Successfully parsed: {}", success_count);
    println!("  Failed to parse: {}", error_count);
    
    Ok(())
}
