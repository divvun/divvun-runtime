use super::runner::TestComparison;
use super::{ErrorAnnotatedSentence, ErrorMarkup, TestResult};
use std::io::Write;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

/// Print the results for a single test
pub fn print_test_result(
    test_number: usize,
    total_tests: usize,
    sentence: &ErrorAnnotatedSentence,
    comparison: &TestComparison,
) {
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    
    // Print title
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = writeln!(stdout, "{}", "-".repeat(10));
    let _ = writeln!(stdout, "Test {}/{}: {}", test_number, total_tests, sentence.text);
    let _ = writeln!(stdout, "{}", "-".repeat(10));
    let _ = stdout.reset();
    
    // Print true positives (successes)
    for (expected, actual) in &comparison.true_positives {
        print_success(&mut stdout, test_number, total_tests, TestResult::TruePositive, expected, Some(actual));
    }
    
    // Print false positives 1 (found error but wrong correction)
    for (expected, actual) in &comparison.false_positives_1 {
        print_failure(&mut stdout, test_number, total_tests, TestResult::FalsePositive1, expected, Some(actual));
    }
    
    // Print false positives 2 (found error not in markup)
    for actual in &comparison.false_positives_2 {
        print_failure_fp2(&mut stdout, test_number, total_tests, actual);
    }
    
    // Print false negatives 1 (found error but no correction)
    for (expected, actual) in &comparison.false_negatives_1 {
        print_failure(&mut stdout, test_number, total_tests, TestResult::FalseNegative1, expected, Some(actual));
    }
    
    // Print false negatives 2 (missed error)
    for expected in &comparison.false_negatives_2 {
        print_failure(&mut stdout, test_number, total_tests, TestResult::FalseNegative2, expected, None);
    }
    
    // Print test summary
    print_test_summary(&mut stdout, test_number, comparison);
}

fn print_success(
    stdout: &mut StandardStream,
    test_number: usize,
    total_tests: usize,
    result_type: TestResult,
    expected: &ErrorMarkup,
    actual: Option<&serde_json::Value>,
) {
    // Test info in cyan
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = write!(stdout, "[{}/{}]", test_number, total_tests);
    
    // Status in green
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)));
    let _ = write!(stdout, "[PASS {}] ", result_type.code());
    let _ = stdout.reset();
    
    // Expected error
    let expected_str = format!(
        "{}:({}{})",
        expected.form_as_string(),
        expected.suggestions.join(", "),
        if !expected.comment.is_empty() {
            format!(" ({})", expected.comment)
        } else {
            String::new()
        }
    );
    let _ = write!(stdout, "{} ", expected_str);
    
    // Arrow in blue
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Blue)));
    let _ = write!(stdout, "=> ");
    let _ = stdout.reset();
    
    // Actual error
    if let Some(actual) = actual {
        let suggestions: Vec<String> = actual["suggestions"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        
        let _ = writeln!(
            stdout,
            "{}:[{}]",
            actual["form"].as_str().unwrap_or(""),
            suggestions.join(", ")
        );
    } else {
        let _ = writeln!(stdout, "GramDivvun did not find any errors");
    }
}

fn print_failure(
    stdout: &mut StandardStream,
    test_number: usize,
    total_tests: usize,
    result_type: TestResult,
    expected: &ErrorMarkup,
    actual: Option<&serde_json::Value>,
) {
    // Test info in cyan
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = write!(stdout, "[{}/{}]", test_number, total_tests);
    
    // Status in red
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)));
    let _ = write!(stdout, "[FAIL {}] ", result_type.code());
    let _ = stdout.reset();
    
    // Expected error
    let expected_str = format!(
        "{}:({}{})",
        expected.form_as_string(),
        expected.suggestions.join(", "),
        if !expected.comment.is_empty() {
            format!(" ({})", expected.comment)
        } else {
            String::new()
        }
    );
    let _ = write!(stdout, "{} ", expected_str);
    
    // Arrow in blue
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Blue)));
    let _ = write!(stdout, "=> ");
    let _ = stdout.reset();
    
    // Actual error
    if let Some(actual) = actual {
        let suggestions: Vec<String> = actual["suggestions"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        
        let _ = writeln!(
            stdout,
            "{}:[{}]",
            actual["form"].as_str().unwrap_or(""),
            suggestions.join(", ")
        );
    } else {
        let _ = writeln!(stdout, "GramDivvun did not find any errors");
    }
}

fn print_failure_fp2(
    stdout: &mut StandardStream,
    test_number: usize,
    total_tests: usize,
    actual: &serde_json::Value,
) {
    // Test info in cyan
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = write!(stdout, "[{}/{}]", test_number, total_tests);
    
    // Status in red
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)));
    let _ = write!(stdout, "[FAIL {}] ", TestResult::FalsePositive2.code());
    let _ = stdout.reset();
    
    let _ = write!(stdout, "No errors expected ");
    
    // Arrow in blue
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Blue)));
    let _ = write!(stdout, "=> ");
    let _ = stdout.reset();
    
    // Actual error
    let suggestions: Vec<String> = actual["suggestions"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    
    let _ = writeln!(
        stdout,
        "{}:[{}]",
        actual["form"].as_str().unwrap_or(""),
        suggestions.join(", ")
    );
}

fn print_test_summary(stdout: &mut StandardStream, test_number: usize, comparison: &TestComparison) {
    let passes = comparison.true_positives.len();
    let fails = comparison.false_positives_1.len()
        + comparison.false_positives_2.len()
        + comparison.false_negatives_1.len()
        + comparison.false_negatives_2.len();
    let total = passes + fails;
    
    let _ = write!(stdout, "Test {} - Passes: ", test_number);
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)));
    let _ = write!(stdout, "{}", passes);
    let _ = stdout.reset();
    
    let _ = write!(stdout, ", Fails: ");
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)));
    let _ = write!(stdout, "{}", fails);
    let _ = stdout.reset();
    
    let _ = write!(stdout, ", Total: ");
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = writeln!(stdout, "{}", total);
    let _ = stdout.reset();
    
    let _ = writeln!(stdout);
}

pub fn print_final_summary(total_tests: usize, parse_errors: usize) {
    if parse_errors > 0 {
        println!("\nTests with parse errors: {}", parse_errors);
    }
    println!("Total test sentences: {}", total_tests);
}
