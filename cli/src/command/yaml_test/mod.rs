mod error_types;
mod markup;
mod output;
mod parser;
mod runner;
mod sentence;
mod test_result;
mod yaml_file;

pub use error_types::ErrorType;
pub use markup::{ErrorContent, ErrorMarkup, ErrorSegment};
pub use parser::{parse_markup, ParseError};
pub use sentence::ErrorAnnotatedSentence;
pub use test_result::TestResult;
pub use yaml_file::{Config, YamlTestFile};

use crate::cli::YamlTestArgs;
use crate::shell::Shell;
use divvun_runtime::bundle::Bundle;

pub async fn yaml_test(_shell: &mut Shell, args: YamlTestArgs) -> anyhow::Result<()> {
    let yaml_file = YamlTestFile::from_file(args.yaml_file.to_str().unwrap())?;
    
    println!("Loaded test file:");
    println!("  Config: {:?}", yaml_file.config);
    println!("  Number of tests: {}", yaml_file.tests.len());
    
    if let Some(variant) = yaml_file.variant() {
        println!("  Variant: {}", variant);
    }
    
    // Display bundle path and pipeline selection
    if let Some(ref path) = args.path {
        println!("  Bundle path: {}", path.display());
    } else {
        println!("  Bundle path: . (current directory)");
    }
    
    if let Some(ref pipeline) = args.pipeline {
        println!("  Pipeline: {}", pipeline);
    } else {
        println!("  Pipeline: (default)");
    }
    
    // Load the bundle
    let path = args
        .path
        .as_ref()
        .cloned()
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    
    let bundle = if path.extension().map(|x| x.as_encoded_bytes()) == Some(b"drb") {
        if let Some(ref pipeline_name) = args.pipeline {
            Bundle::from_bundle_named(&path, pipeline_name)?
        } else {
            Bundle::from_bundle(&path)?
        }
    } else {
        if let Some(ref pipeline_name) = args.pipeline {
            Bundle::from_path_named(&path, pipeline_name)?
        } else {
            Bundle::from_path(&path)?
        }
    };
    
    let config = serde_json::Value::Object(serde_json::Map::new());
    
    // Parse all tests
    let parsed_tests = yaml_file.parse_tests();
    let total_tests = parsed_tests.len();
    let mut parse_error_count = 0;
    let mut total_counts = std::collections::HashMap::new();
    total_counts.insert(TestResult::TruePositive, 0);
    total_counts.insert(TestResult::TrueNegative, 0);
    total_counts.insert(TestResult::FalsePositive1, 0);
    total_counts.insert(TestResult::FalsePositive2, 0);
    total_counts.insert(TestResult::FalseNegative1, 0);
    total_counts.insert(TestResult::FalseNegative2, 0);
    
    for (i, result) in parsed_tests.iter().enumerate() {
        let test_number = i + 1;
        
        match result {
            Ok(sentence) => {
                match runner::run_test(sentence, &bundle, config.clone()).await {
                    Ok(comparison) => {
                        output::print_test_result(test_number, total_tests, sentence, &comparison);
                        
                        // Accumulate counts
                        *total_counts.get_mut(&TestResult::TruePositive).unwrap() += comparison.count(TestResult::TruePositive);
                        *total_counts.get_mut(&TestResult::TrueNegative).unwrap() += comparison.count(TestResult::TrueNegative);
                        *total_counts.get_mut(&TestResult::FalsePositive1).unwrap() += comparison.count(TestResult::FalsePositive1);
                        *total_counts.get_mut(&TestResult::FalsePositive2).unwrap() += comparison.count(TestResult::FalsePositive2);
                        *total_counts.get_mut(&TestResult::FalseNegative1).unwrap() += comparison.count(TestResult::FalseNegative1);
                        *total_counts.get_mut(&TestResult::FalseNegative2).unwrap() += comparison.count(TestResult::FalseNegative2);
                    }
                    Err(e) => {
                        parse_error_count += 1;
                        println!("Test {}/{} failed to run: {}", test_number, total_tests, e);
                    }
                }
            }
            Err(e) => {
                parse_error_count += 1;
                println!("Test {}/{} failed to parse: {:?}", test_number, total_tests, e);
            }
        }
    }
    
    if parse_error_count > 0 {
        println!("\nTests with parse/run errors: {}", parse_error_count);
    }
    
    output::print_final_summary(&total_counts);
    
    Ok(())
}
