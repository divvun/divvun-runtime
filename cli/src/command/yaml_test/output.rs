use super::runner::ErrorComparison;

/// Print test result details when a test fails
pub fn print_comparison(comparison: &ErrorComparison) {
    if !comparison.unmatched_expected.is_empty() {
        println!("    Expected errors not found: {:?}", comparison.unmatched_expected);
    }
    if !comparison.unmatched_actual.is_empty() {
        println!("    Unexpected errors found: {:?}", comparison.unmatched_actual);
    }
}

/// Print the final test summary
pub fn print_summary(pass_count: usize, fail_count: usize, parse_error_count: usize, total: usize) {
    println!("\n{}", "=".repeat(60));
    println!("Summary:");
    println!("  Passed: {}", pass_count);
    println!("  Failed: {}", fail_count);
    println!("  Parse errors: {}", parse_error_count);
    println!("  Total: {}", total);
}
