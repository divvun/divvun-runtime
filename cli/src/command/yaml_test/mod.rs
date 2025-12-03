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
use divvun_runtime::{bundle::Bundle, modules::Input};
use futures_util::StreamExt;
use serde_json::Value;

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
    let mut pass_count = 0;
    let mut fail_count = 0;
    let mut parse_error_count = 0;
    
    for (i, result) in parsed_tests.iter().enumerate() {
        match result {
            Ok(sentence) => {
                println!("\nTest {}: {}", i + 1, sentence.text);
                println!("  Expected errors: {}", sentence.error_count());
                
                // Run through grammar checker
                let mut pipe = bundle.create(config.clone()).await?;
                let mut stream = pipe.forward(Input::String(sentence.text.clone())).await;
                
                if let Some(Ok(Input::Json(output_json))) = stream.next().await {
                    match compare_errors(sentence, &output_json) {
                        Ok(comparison) => {
                            if comparison.all_matched {
                                pass_count += 1;
                                println!("  ✓ PASS");
                            } else {
                                fail_count += 1;
                                println!("  ✗ FAIL");
                                print_comparison(&comparison);
                            }
                        }
                        Err(e) => {
                            fail_count += 1;
                            println!("  ✗ FAIL: {}", e);
                        }
                    }
                } else {
                    fail_count += 1;
                    println!("  ✗ FAIL: No JSON output from grammar checker");
                }
            }
            Err(e) => {
                parse_error_count += 1;
                println!("\nTest {} failed to parse: {:?}", i + 1, e);
            }
        }
    }
    
    println!("\n{}", "=".repeat(60));
    println!("Summary:");
    println!("  Passed: {}", pass_count);
    println!("  Failed: {}", fail_count);
    println!("  Parse errors: {}", parse_error_count);
    println!("  Total: {}", parsed_tests.len());
    
    Ok(())
}

#[derive(Debug)]
struct ErrorComparison {
    all_matched: bool,
    matched: Vec<usize>,
    unmatched_expected: Vec<usize>,
    unmatched_actual: Vec<usize>,
}

fn print_comparison(comparison: &ErrorComparison) {
    if !comparison.unmatched_expected.is_empty() {
        println!("    Expected errors not found: {:?}", comparison.unmatched_expected);
    }
    if !comparison.unmatched_actual.is_empty() {
        println!("    Unexpected errors found: {:?}", comparison.unmatched_actual);
    }
}

fn compare_errors(
    sentence: &ErrorAnnotatedSentence,
    output_json: &Value,
) -> anyhow::Result<ErrorComparison> {
    let errors = output_json["errors"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No 'errors' array in output"))?;
    
    let mut matched = Vec::new();
    let mut unmatched_expected: Vec<usize> = (0..sentence.errors.len()).collect();
    let mut unmatched_actual: Vec<usize> = (0..errors.len()).collect();
    
    // Try to match each expected error with an actual error
    for (exp_idx, expected_err) in sentence.errors.iter().enumerate() {
        let mut best_match: Option<usize> = None;
        
        for (act_idx, actual_err) in errors.iter().enumerate() {
            if !unmatched_actual.contains(&act_idx) {
                continue; // Already matched
            }
            
            if errors_match(expected_err, actual_err)? {
                best_match = Some(act_idx);
                break;
            }
        }
        
        if let Some(act_idx) = best_match {
            matched.push(exp_idx);
            unmatched_expected.retain(|&x| x != exp_idx);
            unmatched_actual.retain(|&x| x != act_idx);
        }
    }
    
    Ok(ErrorComparison {
        all_matched: unmatched_expected.is_empty() && unmatched_actual.is_empty(),
        matched,
        unmatched_expected,
        unmatched_actual,
    })
}

fn errors_match(expected: &ErrorMarkup, actual: &Value) -> anyhow::Result<bool> {
    // Check range (start/end)
    let actual_start = actual["start"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing 'start' field"))? as usize;
    let actual_end = actual["end"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing 'end' field"))? as usize;
    
    if expected.start != actual_start || expected.end != actual_end {
        return Ok(false);
    }
    
    // Check form (the error text)
    let actual_form = actual["form"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'form' field"))?;
    
    let expected_form = expected.form_as_string();
    if actual_form != expected_form {
        return Ok(false);
    }
    
    // Check if at least one expected suggestion is in the actual suggestions
    if !expected.suggestions.is_empty() {
        let actual_suggestions = actual["suggestions"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Missing 'suggestions' field"))?;
        
        let actual_suggestion_strings: Vec<String> = actual_suggestions
            .iter()
            .filter_map(|s| s.as_str().map(|s| s.to_string()))
            .collect();
        
        let has_matching_suggestion = expected.suggestions.iter().any(|exp_sug| {
            actual_suggestion_strings.iter().any(|act_sug| {
                act_sug == exp_sug
            })
        });
        
        if !has_matching_suggestion {
            return Ok(false);
        }
    }
    
    Ok(true)
}
