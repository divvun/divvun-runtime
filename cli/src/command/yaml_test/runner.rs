use super::{ErrorAnnotatedSentence, ErrorMarkup};
use divvun_runtime::{bundle::Bundle, modules::Input};
use futures_util::StreamExt;
use serde_json::Value;

/// Result of comparing expected errors with actual errors from grammar checker
#[derive(Debug)]
pub struct ErrorComparison {
    pub all_matched: bool,
    pub matched: Vec<usize>,
    pub unmatched_expected: Vec<usize>,
    pub unmatched_actual: Vec<usize>,
}

/// Run a single test sentence through the grammar checker and compare results
pub async fn run_test(
    sentence: &ErrorAnnotatedSentence,
    bundle: &Bundle,
    config: Value,
) -> anyhow::Result<ErrorComparison> {
    // Run through grammar checker
    let mut pipe = bundle.create(config).await?;
    let mut stream = pipe.forward(Input::String(sentence.text.clone())).await;
    
    if let Some(Ok(Input::Json(output_json))) = stream.next().await {
        compare_errors(sentence, &output_json)
    } else {
        anyhow::bail!("No JSON output from grammar checker")
    }
}

/// Compare expected errors with actual errors from grammar checker output
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

/// Check if an expected error matches an actual error from the grammar checker
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
