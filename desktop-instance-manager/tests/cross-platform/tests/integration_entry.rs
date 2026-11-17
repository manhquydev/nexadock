//! Cargo integration test target executing all cross-platform suites and strict post-fix regression gates.

use dim_cross_platform_tests::harness::TestVerdict;
use dim_cross_platform_tests::suites::{
    adapter_contracts, cross_platform, data_safety, lifecycle, profile_mgmt, reliability,
};

fn assert_no_unexpected_failures(outcomes: &[dim_cross_platform_tests::harness::TestCaseOutcome]) {
    let unexpected: Vec<_> = outcomes
        .iter()
        .filter(|o| o.verdict == TestVerdict::FailUnexpected)
        .collect();
    assert!(
        unexpected.is_empty(),
        "Unexpected test harness failures: {unexpected:#?}"
    );
}

#[test]
fn suite_01_process_lifecycle_matrix() {
    let outcomes = lifecycle::run_lifecycle_suite(0x2026_1117);
    assert_eq!(outcomes.len(), 9);
    assert_no_unexpected_failures(&outcomes);
}

#[test]
fn suite_02_profile_management_matrix() {
    let outcomes = profile_mgmt::run_profile_management_suite();
    assert_eq!(outcomes.len(), 8);
    assert_no_unexpected_failures(&outcomes);
}

#[test]
fn suite_03_data_safety_matrix() {
    let outcomes = data_safety::run_data_safety_suite(0x2026_1117);
    assert_eq!(outcomes.len(), 8);
    assert_no_unexpected_failures(&outcomes);
}

#[test]
fn suite_04_reliability_matrix_fast_ci() {
    // Runs 25 soak cycles in standard `cargo test` for fast developer feedback;
    // full 100-cycle soak runs via `xf-test-runner --cycles 100`.
    let outcomes = reliability::run_reliability_suite(25, 0x2026_1117);
    assert_eq!(outcomes.len(), 8);
    assert_no_unexpected_failures(&outcomes);
}

#[test]
fn suite_05_cross_platform_differences_matrix() {
    let outcomes = cross_platform::run_cross_platform_suite(0x2026_1117);
    assert_eq!(outcomes.len(), 12);
    assert_no_unexpected_failures(&outcomes);
}

#[test]
fn suite_06_adapter_and_cli_contracts_matrix() {
    let outcomes = adapter_contracts::run_adapter_contract_suite();
    assert_eq!(outcomes.len(), 10);
    assert_no_unexpected_failures(&outcomes);
}

#[test]
#[ignore = "Strict post-fix regression gate: fails while DEFECT-01..DEFECT-10 remain unpatched in product adapters"]
fn strict_zero_product_defects_gate() {
    let all = dim_cross_platform_tests::suites::run_selected_suites("all", 25, 0x2026_1117);
    let reproduced: Vec<_> = all
        .iter()
        .filter(|o| o.verdict == TestVerdict::FailDefectReproduced)
        .collect();
    assert!(
        reproduced.is_empty(),
        "Unresolved product defects still reproduced: {reproduced:#?}"
    );
}
