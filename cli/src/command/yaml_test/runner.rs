use super::{ErrorAnnotatedSentence, ErrorMarkup, TestResult};
use divvun_runtime::{bundle::Bundle, modules::Input};
use futures_util::StreamExt;
use serde_json::Value;

/// Result of comparing expected errors with actual errors from grammar checker
#[derive(Debug)]
pub struct TestComparison {
    pub true_positives: Vec<(ErrorMarkup, Value)>,
    pub true_negatives: bool,
    pub false_positives_1: Vec<(ErrorMarkup, Value)>,
    pub false_positives_2: Vec<Value>,
    pub false_negatives_1: Vec<(ErrorMarkup, Value)>,
    pub false_negatives_2: Vec<ErrorMarkup>,
}

impl TestComparison {
    pub fn passed(&self) -> bool {
        self.false_positives_1.is_empty()
            && self.false_positives_2.is_empty()
            && self.false_negatives_1.is_empty()
            && self.false_negatives_2.is_empty()
    }

    pub fn count(&self, result: TestResult) -> usize {
        match result {
            TestResult::TruePositive => self.true_positives.len(),
            TestResult::TrueNegative => if self.true_negatives { 1 } else { 0 },
            TestResult::FalsePositive1 => self.false_positives_1.len(),
            TestResult::FalsePositive2 => self.false_positives_2.len(),
            TestResult::FalseNegative1 => self.false_negatives_1.len(),
            TestResult::FalseNegative2 => self.false_negatives_2.len(),
        }
    }

    pub fn total_count(&self) -> usize {
        self.true_positives.len()
            + if self.true_negatives { 1 } else { 0 }
            + self.false_positives_1.len()
            + self.false_positives_2.len()
            + self.false_negatives_1.len()
            + self.false_negatives_2.len()
    }
}

/// Run a single test sentence through the grammar checker and compare results
pub async fn run_test(
    sentence: &ErrorAnnotatedSentence,
    bundle: &Bundle,
    config: Value,
) -> anyhow::Result<TestComparison> {
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
) -> anyhow::Result<TestComparison> {
    let actual_errors = output_json["errors"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No 'errors' array in output"))?;
    
    let mut true_positives = Vec::new();
    let mut false_positives_1 = Vec::new();
    let mut false_negatives_1 = Vec::new();
    let mut false_negatives_2 = Vec::new();
    
    let mut matched_actual_indices = std::collections::HashSet::new();
    
    // Check each expected error against actual errors
    for expected in &sentence.errors {
        let mut found_match = false;
        
        for (idx, actual) in actual_errors.iter().enumerate() {
            if !has_same_range_and_error(expected, actual)? {
                continue;
            }
            
            matched_actual_indices.insert(idx);
            
            if has_suggestions_with_hit(expected, actual)? {
                // TP: Found the error with correct suggestion
                true_positives.push((expected.clone(), actual.clone()));
                found_match = true;
                break;
            } else if has_suggestions_without_hit(expected, actual)? {
                // FP1: Found the error but suggested wrong correction
                false_positives_1.push((expected.clone(), actual.clone()));
                found_match = true;
                break;
            } else if has_no_suggestions(expected, actual)? {
                // FN1: Found the error but has no correction
                false_negatives_1.push((expected.clone(), actual.clone()));
                found_match = true;
                break;
            }
        }
        
        if !found_match {
            // FN2: Did not find the expected error at all
            false_negatives_2.push(expected.clone());
        }
    }
    
    // FP2: Actual errors that don't match any expected error
    let false_positives_2: Vec<Value> = actual_errors
        .iter()
        .enumerate()
        .filter(|(idx, _)| !matched_actual_indices.contains(idx))
        .map(|(_, err)| err.clone())
        .collect();
    
    // TN: No expected errors and no actual errors
    let true_negatives = sentence.errors.is_empty() && actual_errors.is_empty();
    
    Ok(TestComparison {
        true_positives,
        true_negatives,
        false_positives_1,
        false_positives_2,
        false_negatives_1,
        false_negatives_2,
    })
}

/// Check if the errors have the same range and error text
fn has_same_range_and_error(expected: &ErrorMarkup, actual: &Value) -> anyhow::Result<bool> {
    let actual_start = actual["start"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing 'start' field"))? as usize;
    let actual_end = actual["end"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing 'end' field"))? as usize;
    
    if expected.start != actual_start || expected.end != actual_end {
        return Ok(false);
    }
    
    let actual_form = actual["form"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'form' field"))?;
    
    let expected_form = expected.form_as_string();
    Ok(actual_form == expected_form)
}

/// Check if markup error correction exists in grammar checker error
fn has_suggestions_with_hit(expected: &ErrorMarkup, actual: &Value) -> anyhow::Result<bool> {
    let actual_suggestions = actual["suggestions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Missing 'suggestions' field"))?;
    
    if actual_suggestions.is_empty() {
        return Ok(false);
    }
    
    if !has_same_range_and_error(expected, actual)? {
        return Ok(false);
    }
    
    let actual_suggestion_strings: Vec<String> = actual_suggestions
        .iter()
        .filter_map(|s| s.as_str().map(|s| s.to_string()))
        .collect();
    
    Ok(expected.suggestions.iter().any(|exp_sug| {
        actual_suggestion_strings.contains(exp_sug)
    }))
}

/// Check if grammar checker found the error but suggested wrong correction
fn has_suggestions_without_hit(expected: &ErrorMarkup, actual: &Value) -> anyhow::Result<bool> {
    if !has_same_range_and_error(expected, actual)? {
        return Ok(false);
    }
    
    let actual_suggestions = actual["suggestions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Missing 'suggestions' field"))?;
    
    if actual_suggestions.is_empty() {
        return Ok(false);
    }
    
    let actual_suggestion_strings: Vec<String> = actual_suggestions
        .iter()
        .filter_map(|s| s.as_str().map(|s| s.to_string()))
        .collect();
    
    Ok(!expected.suggestions.iter().any(|exp_sug| {
        actual_suggestion_strings.contains(exp_sug)
    }))
}

/// Check if grammar checker found the error but provided no suggestions
fn has_no_suggestions(expected: &ErrorMarkup, actual: &Value) -> anyhow::Result<bool> {
    if !has_same_range_and_error(expected, actual)? {
        return Ok(false);
    }
    
    let actual_suggestions = actual["suggestions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Missing 'suggestions' field"))?;
    
    Ok(actual_suggestions.is_empty())
}
