//! Cargo integration test target executing all cross-platform suites and strict post-fix regression gates.

use std::collections::HashSet;
use std::path::Path;

use dim_cross_platform_tests::harness::{
    detect_fixed_candidates, is_protected_baseline_report_path, CaseSelectionFilter,
    PriorityFilter, PriorityTier, RegressionMode, TestCaseOutcome, TestVerdict, CANONICAL_CASE_IDS,
    KNOWN_BASELINE_DEFECTS,
};
use dim_cross_platform_tests::suites::{
    adapter_contracts, cross_platform, data_safety, lifecycle, profile_mgmt, reliability,
    run_filtered_suites, run_selected_suites,
};

fn assert_no_unexpected_failures(outcomes: &[TestCaseOutcome]) {
    let unexpected: Vec<_> = outcomes
        .iter()
        .filter(|o| o.verdict == TestVerdict::FailUnexpected)
        .collect();
    assert!(
        unexpected.is_empty(),
        "Unexpected test harness failures: {unexpected:#?}"
    );
}

fn assert_release_gate_clean(scope_label: &str, outcomes: &[TestCaseOutcome]) {
    let blocking: Vec<_> = outcomes
        .iter()
        .filter(|o| {
            matches!(
                o.verdict,
                TestVerdict::FailDefectReproduced | TestVerdict::FailUnexpected
            )
        })
        .collect();
    assert!(
        blocking.is_empty(),
        "{scope_label} release gate blocked by unresolved product defects or failures: {blocking:#?}"
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
fn suite_07_controlled_regression_filter_and_mode_contracts() {
    assert_eq!(CANONICAL_CASE_IDS.len(), 55);
    let unique: HashSet<&str> = CANONICAL_CASE_IDS.iter().copied().collect();
    assert_eq!(unique.len(), 55);
    assert_eq!(KNOWN_BASELINE_DEFECTS.len(), 11);

    let p0_count = CANONICAL_CASE_IDS
        .iter()
        .filter(|&&id| PriorityTier::for_case(id) == PriorityTier::P0)
        .count();
    let p1_count = CANONICAL_CASE_IDS
        .iter()
        .filter(|&&id| PriorityTier::for_case(id) == PriorityTier::P1)
        .count();
    let p2_count = CANONICAL_CASE_IDS
        .iter()
        .filter(|&&id| PriorityTier::for_case(id) == PriorityTier::P2)
        .count();
    let guard_count = CANONICAL_CASE_IDS
        .iter()
        .filter(|&&id| PriorityTier::for_case(id) == PriorityTier::RegressionGuard)
        .count();

    assert_eq!(p0_count, 4);
    assert_eq!(p1_count, 6);
    assert_eq!(p2_count, 1);
    assert_eq!(guard_count, 44);

    assert_eq!(
        RegressionMode::parse("baseline-audit").unwrap(),
        RegressionMode::BaselineAudit
    );
    assert_eq!(
        RegressionMode::parse("RELEASE-REGRESSION").unwrap(),
        RegressionMode::ReleaseRegression
    );
    assert!(RegressionMode::parse("permissive").is_err());

    let parsed_cases =
        CaseSelectionFilter::parse_case_ids(&["lc-08, ct-10".to_owned(), "LC-08".to_owned()])
            .unwrap();
    assert_eq!(parsed_cases, vec!["LC-08".to_owned(), "CT-10".to_owned()]);
    assert!(CaseSelectionFilter::parse_case_ids(&["NO-SUCH-99".to_owned()]).is_err());

    assert!(is_protected_baseline_report_path(Path::new(
        "plans/reports/reliability/windows-host-execution-evidence.json"
    )));
    assert!(is_protected_baseline_report_path(Path::new(
        "plans\\reports\\reliability\\defect-reproductions.json"
    )));
    assert!(!is_protected_baseline_report_path(Path::new(
        "plans/reports/reliability/post-fix-windows-evidence.json"
    )));

    let single_filter = CaseSelectionFilter {
        suite: "all".to_owned(),
        priority: PriorityFilter::All,
        case_ids: vec!["CT-04".to_owned()],
    };
    let single_outcomes = run_filtered_suites(&single_filter, 10, 0x2026_1117);
    assert_eq!(single_outcomes.len(), 1);
    assert_eq!(single_outcomes[0].id, "CT-04");
    assert_eq!(single_outcomes[0].verdict, TestVerdict::Pass);
    assert!(detect_fixed_candidates(&single_outcomes).is_empty());
}

#[test]
#[ignore = "Mandatory P0 post-fix release gate: fails if any P0 case (LC-07, LC-08, PM-08, CT-10) reproduces a defect or fails"]
fn release_gate_p0_mandatory() {
    let filter = CaseSelectionFilter {
        suite: "all".to_owned(),
        priority: PriorityFilter::P0,
        case_ids: Vec::new(),
    };
    let outcomes = run_filtered_suites(&filter, 25, 0x2026_1117);
    assert_eq!(outcomes.len(), 4);
    assert_release_gate_clean("P0 mandatory", &outcomes);
}

#[test]
#[ignore = "Mandatory P0+P1 post-fix release gate: fails if any P0 or P1 case reproduces a defect or fails"]
fn release_gate_p0_p1_mandatory() {
    let filter = CaseSelectionFilter {
        suite: "all".to_owned(),
        priority: PriorityFilter::P0P1,
        case_ids: Vec::new(),
    };
    let outcomes = run_filtered_suites(&filter, 25, 0x2026_1117);
    assert_eq!(outcomes.len(), 10);
    assert_release_gate_clean("P0+P1 mandatory", &outcomes);
}

#[test]
#[ignore = "Strict post-fix regression gate: fails while DEFECT-01..DEFECT-10 remain unpatched in product adapters"]
fn strict_zero_product_defects_gate() {
    let all = run_selected_suites("all", 25, 0x2026_1117);
    assert_eq!(all.len(), 55);
    assert_release_gate_clean("Full 55-case zero-defect", &all);
}
