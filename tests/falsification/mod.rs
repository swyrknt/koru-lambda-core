/// Falsification Test Suite - Helpers and Utilities
///
/// This module provides shared utilities for all falsification tests.
use koru_lambda_core::Distinction;

/// Test helper utilities
#[allow(dead_code)]
pub mod helpers {
    use super::*;

    /// Verifies that two distinctions have identical IDs
    pub fn assert_identical_ids(d1: &Distinction, d2: &Distinction, path_description: &str) {
        assert_eq!(
            d1.to_hex(),
            d2.to_hex(),
            "FALSIFIED: Path dependence detected. {} yielded different results.\n  Path 1 ID: {}\n  Path 2 ID: {}",
            path_description,
            d1.to_hex(),
            d2.to_hex()
        );
    }

    /// Reports successful test completion
    pub fn report_sustained(test_name: &str, details: &str) {
        println!("\nHypothesis sustained.");
        println!("  Test: {}", test_name);
        println!("  {}", details);
    }
}
