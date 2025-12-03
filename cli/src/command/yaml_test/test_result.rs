/// Test result categories for grammar checking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TestResult {
    /// True Positive: GramDivvun found marked up error and has the suggested correction
    TruePositive,
    /// False Positive 1: GramDivvun found manually marked up error, but corrected wrongly
    FalsePositive1,
    /// False Positive 2: GramDivvun found error which is not manually marked up
    FalsePositive2,
    /// False Negative 1: GramDivvun found manually marked up error, but has no correction
    FalseNegative1,
    /// False Negative 2: GramDivvun did not find manually marked up error
    FalseNegative2,
}

impl TestResult {
    /// Get the short code for this result (tp, fp1, fp2, fn1, fn2)
    pub fn code(&self) -> &'static str {
        match self {
            TestResult::TruePositive => "tp",
            TestResult::FalsePositive1 => "fp1",
            TestResult::FalsePositive2 => "fp2",
            TestResult::FalseNegative1 => "fn1",
            TestResult::FalseNegative2 => "fn2",
        }
    }

    /// Get the explanation for this result
    pub fn explanation(&self) -> &'static str {
        match self {
            TestResult::TruePositive => {
                "GramDivvun found marked up error and has the suggested correction"
            }
            TestResult::FalsePositive1 => {
                "GramDivvun found manually marked up error, but corrected wrongly"
            }
            TestResult::FalsePositive2 => {
                "GramDivvun found error which is not manually marked up"
            }
            TestResult::FalseNegative1 => {
                "GramDivvun found manually marked up error, but has no correction"
            }
            TestResult::FalseNegative2 => {
                "GramDivvun did not find manually marked up error"
            }
        }
    }
}
