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
    
    // Deduplicate errors with identical ranges (start, end)
    let mut seen_ranges = std::collections::HashSet::new();
    let mut deduped_errors = Vec::new();
    for error in actual_errors {
        if let (Some(start), Some(end)) = (error["start"].as_u64(), error["end"].as_u64()) {
            let range = (start, end);
            if !seen_ranges.contains(&range) {
                seen_ranges.insert(range);
                deduped_errors.push(error.clone());
            }
        }
    }
    let actual_errors = &deduped_errors;
    
    let mut true_positives = Vec::new();
    let mut false_positives_1 = Vec::new();
    let mut false_negatives_1 = Vec::new();
    let mut false_negatives_2 = Vec::new();
    
    let mut matched_actual_indices = std::collections::HashSet::new();
    
    // Track how many expected errors matched each actual error (for quotation marks)
    let mut actual_match_counts: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    
    // Check each expected error against actual errors
    for expected in &sentence.errors {
        let mut found_match = false;
        
        for (idx, actual) in actual_errors.iter().enumerate() {
            if !has_same_range_and_error(expected, actual)? {
                continue;
            }
            
            matched_actual_indices.insert(idx);
            *actual_match_counts.entry(idx).or_insert(0) += 1;
            
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
    let actual_form = actual["form"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'form' field"))?;
    let expected_form = expected.form_as_string();
    
    // Check for quotation mark errors - they need special handling
    let error_id = actual.get("error_id").and_then(|v| v.as_str());
    if error_id == Some("err-quotation-marks") {
        // For quotation marks, the grammar checker includes adjacent text in the range
        // but the error markup only marks the quotation mark itself.
        
        // Check if the expected form is a quotation mark (straight, curly, apostrophe, or guillemet)
        let is_quote = expected_form == "\"" 
            || expected_form == "\u{201C}" // left curly quote "
            || expected_form == "\u{201D}" // right curly quote "
            || expected_form == "'"         // apostrophe
            || expected_form == "\u{2018}" // left single quote '
            || expected_form == "\u{2019}" // right single quote '
            || expected_form == "\u{00AB}" // left guillemet «
            || expected_form == "\u{00BB}"; // right guillemet »
        
        if is_quote {
            // Check if expected error overlaps with actual error range
            // The expected quote can be:
            // - At the start: expected.start == actual_start
            // - At the end: expected.end == actual_end
            // - Anywhere within: expected.start >= actual_start && expected.end <= actual_end
            let at_or_near_start = expected.start == actual_start && expected.end <= actual_end;
            let at_or_near_end = expected.end == actual_end && expected.start >= actual_start;
            // Also check if the expected quote is just inside the actual range boundaries
            let near_boundary = (expected.start == actual_start || expected.end == actual_end)
                && expected.start >= actual_start 
                && expected.end <= actual_end;
            
            if at_or_near_start || at_or_near_end || near_boundary {
                // Verify the actual form contains the expected quotation mark
                return Ok(actual_form.contains(&expected_form));
            }
        }
    }
    
    // Standard comparison: exact range and form match
    if expected.start != actual_start || expected.end != actual_end {
        return Ok(false);
    }
    
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
    
    // Check for quotation mark errors - they need special suggestion handling
    let error_id = actual.get("error_id").and_then(|v| v.as_str());
    if error_id == Some("err-quotation-marks") {
        // For quotation marks, check if the expected correction is contained in the actual suggestion
        return Ok(expected.suggestions.iter().any(|exp_sug| {
            actual_suggestion_strings.iter().any(|act_sug| {
                // Check if suggestion contains the expected quote or vice versa
                act_sug.contains(exp_sug) || exp_sug.contains(act_sug)
            })
        }));
    }
    
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
    
    // Check for quotation mark errors - they need special suggestion handling
    let error_id = actual.get("error_id").and_then(|v| v.as_str());
    if error_id == Some("err-quotation-marks") {
        // For quotation marks, use containment check
        let has_match = expected.suggestions.iter().any(|exp_sug| {
            actual_suggestion_strings.iter().any(|act_sug| {
                act_sug.contains(exp_sug) || exp_sug.contains(act_sug)
            })
        });
        // Return true if NO match (wrong correction)
        return Ok(!has_match);
    }
    
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
