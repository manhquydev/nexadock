//! Standalone CLI test, single-case reproducer & controlled regression runner (`xf-test-runner`).
//!
//! Usage examples:
//!   # Non-destructive baseline audit (writes NO JSON files by default):
//!   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- \
//!     --mode baseline-audit --priority p0
//!
//!   # Mandatory post-fix release gate (blocks on FAIL_DEFECT_REPRODUCED and FAIL_UNEXPECTED):
//!   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- \
//!     --mode release-regression --priority p0
//!
//!   # Fast single-case reproducer:
//!   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- \
//!     --mode release-regression --case LC-08

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use dim_core::{redact_sensitive_text, PlatformKind};
use dim_cross_platform_tests::harness::{
    build_run_summary, detect_fixed_candidates, dim_root_dir, is_protected_baseline_report_path,
    CaseSelectionFilter, PriorityFilter, PriorityTier, RegressionMode, TestVerdict,
};
use dim_cross_platform_tests::metrics::collect_fixture_benchmark_report;
use dim_cross_platform_tests::suites::{collect_reproduced_defects, run_filtered_suites};
use serde_json::Value;

struct RunnerCli {
    mode: RegressionMode,
    suite: String,
    priority: PriorityFilter,
    raw_cases: Vec<String>,
    cycles: usize,
    metrics_runs: usize,
    seed: u64,
    skip_metrics: bool,
    require_native_os: bool,
    overwrite_baseline_reports: bool,
    report_out: Option<PathBuf>,
    json_out: Option<PathBuf>,
    metrics_out: Option<PathBuf>,
    defects_out: Option<PathBuf>,
    show_help: bool,
}

fn is_valid_suite_name(suite: &str) -> bool {
    matches!(
        suite.trim().to_ascii_lowercase().as_str(),
        "" | "all"
            | "lifecycle"
            | "profile"
            | "profile_management"
            | "safety"
            | "data_safety"
            | "reliability"
            | "cross-platform"
            | "cross_platform"
            | "contracts"
            | "contract"
            | "metrics"
    )
}

fn print_help() {
    println!(
        "\
xf-test-runner — Desktop Instance Manager Cross-Platform Reliability & Regression Gate

OPTIONS:
  --mode <baseline-audit|release-regression>   Execution gate mode (default: baseline-audit)
  --priority <p0|p1|p2|p0-p1|p0-p2|all>        Mission C2 priority tier filter (default: all)
  --case <ID[,ID...]>                          Case-insensitive single/multi-case filter (LC-01..CT-10)
  --suite <name>                               Suite filter (all, lifecycle, profile, safety, reliability, cross-platform, contracts, metrics)
  --require-native-os                          Exit 3 if any selected test is SKIPPED_HOST_PLATFORM or STATIC_CONFIRMED_NON_HOST
  --report-out <path>                          Write structured JSON run report to <path>
  --overwrite-baseline-reports                 Explicitly allow overwriting frozen C1 baseline JSON files
  --cycles <N>                                 Soak cycle count for REL-02 (default: 100, min: 10)
  --metrics-runs <N>                           Telemetry benchmark runs (default: 10, min: 3)
  --seed <0xHEX|DEC>                           Deterministic PRNG seed (default: 0x20261117)
  --skip-metrics                               Skip fixture scaling benchmark
  --fail-on-product-defects                    Alias upgrading --mode to release-regression
  --help, -h                                   Print this help message"
    );
}

fn next_arg_value(args: &[String], idx: &mut usize, flag: &str) -> Result<String, String> {
    if *idx + 1 >= args.len() {
        return Err(format!("missing value for {flag}"));
    }
    *idx += 1;
    Ok(args[*idx].clone())
}

fn parse_seed_str(s: &str) -> Result<u64, String> {
    let trimmed = s.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).map_err(|e| format!("invalid hex --seed '{s}': {e}"))
    } else {
        trimmed
            .parse::<u64>()
            .map_err(|e| format!("invalid --seed '{s}': {e}"))
    }
}

fn parse_cli() -> Result<RunnerCli, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut cli = RunnerCli {
        mode: RegressionMode::BaselineAudit,
        suite: "all".to_owned(),
        priority: PriorityFilter::All,
        raw_cases: Vec::new(),
        cycles: 100,
        metrics_runs: 10,
        seed: 0x2026_1117,
        skip_metrics: false,
        require_native_os: false,
        overwrite_baseline_reports: false,
        report_out: None,
        json_out: None,
        metrics_out: None,
        defects_out: None,
        show_help: false,
    };

    let mut i = 0;
    while i < args.len() {
        let raw = &args[i];
        let (flag, inline_val) = match raw.split_once('=') {
            Some((k, v)) if k.starts_with('-') => (k, Some(v.to_owned())),
            _ => (raw.as_str(), None),
        };

        let mut take_val = |f: &str| -> Result<String, String> {
            if let Some(v) = inline_val.clone() {
                Ok(v)
            } else {
                next_arg_value(&args, &mut i, f)
            }
        };

        match flag {
            "--help" | "-h" => cli.show_help = true,
            "--mode" => {
                let v = take_val("--mode")?;
                cli.mode = RegressionMode::parse(&v)?;
            }
            "--priority" => {
                let v = take_val("--priority")?;
                cli.priority = PriorityFilter::parse(&v)?;
            }
            "--case" => {
                let v = take_val("--case")?;
                cli.raw_cases.push(v);
            }
            "--suite" => {
                let v = take_val("--suite")?;
                if !is_valid_suite_name(&v) {
                    return Err(format!("unknown --suite '{v}'"));
                }
                cli.suite = v;
            }
            "--cycles" => {
                let v = take_val("--cycles")?;
                let parsed = v
                    .parse::<usize>()
                    .map_err(|e| format!("invalid --cycles '{v}': {e}"))?;
                cli.cycles = parsed.max(10);
            }
            "--metrics-runs" => {
                let v = take_val("--metrics-runs")?;
                let parsed = v
                    .parse::<usize>()
                    .map_err(|e| format!("invalid --metrics-runs '{v}': {e}"))?;
                cli.metrics_runs = parsed.max(3);
            }
            "--seed" => {
                let v = take_val("--seed")?;
                cli.seed = parse_seed_str(&v)?;
            }
            "--skip-metrics" => cli.skip_metrics = true,
            "--fail-on-product-defects" => cli.mode = RegressionMode::ReleaseRegression,
            "--require-native-os" => cli.require_native_os = true,
            "--overwrite-baseline-reports" => cli.overwrite_baseline_reports = true,
            "--report-out" => {
                let v = take_val("--report-out")?;
                cli.report_out = Some(PathBuf::from(v));
            }
            "--json-out" => {
                let v = take_val("--json-out")?;
                cli.json_out = Some(PathBuf::from(v));
            }
            "--metrics-out" => {
                let v = take_val("--metrics-out")?;
                cli.metrics_out = Some(PathBuf::from(v));
            }
            "--defects-out" => {
                let v = take_val("--defects-out")?;
                cli.defects_out = Some(PathBuf::from(v));
            }
            other => {
                return Err(format!(
                    "unrecognized CLI argument '{other}' (pass --help for supported options)"
                ));
            }
        }
        i += 1;
    }

    Ok(cli)
}

fn write_pretty_json<T: serde::Serialize>(path: &Path, val: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let raw_text = serde_json::to_string_pretty(val).map_err(|e| e.to_string())?;
    let user_home = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from);
    // Redact both standard and JSON-escaped user home paths so committed reports never leak usernames.
    let mut sanitized = redact_sensitive_text(&raw_text, user_home.as_deref());
    if let Some(home) = &user_home {
        let escaped = home.display().to_string().replace('\\', "\\\\");
        if !escaped.is_empty() {
            sanitized = sanitized.replace(&escaped, "<USER_HOME>");
        }
    }
    fs::write(path, format!("{sanitized}\n")).map_err(|e| e.to_string())
}

fn default_baseline_report_paths() -> (PathBuf, PathBuf, PathBuf) {
    let reports_dir = dim_root_dir()
        .join("plans")
        .join("reports")
        .join("reliability");
    match PlatformKind::current() {
        PlatformKind::Windows => (
            reports_dir.join("windows-host-execution-evidence.json"),
            reports_dir.join("metrics-benchmark-windows.json"),
            reports_dir.join("defect-reproductions.json"),
        ),
        PlatformKind::MacOs => (
            reports_dir.join("macos-host-execution-evidence.json"),
            reports_dir.join("metrics-benchmark-macos.json"),
            reports_dir.join("defect-reproductions-macos.json"),
        ),
        PlatformKind::Linux => (
            reports_dir.join("ubuntu-host-execution-evidence.json"),
            reports_dir.join("metrics-benchmark-ubuntu.json"),
            reports_dir.join("defect-reproductions-ubuntu.json"),
        ),
    }
}

fn main() -> ExitCode {
    let mut cli = match parse_cli() {
        Ok(c) => c,
        Err(err) => {
            eprintln!("CLI argument error: {err}");
            return ExitCode::from(1);
        }
    };

    if cli.show_help {
        print_help();
        return ExitCode::SUCCESS;
    }

    let case_ids = match CaseSelectionFilter::parse_case_ids(&cli.raw_cases) {
        Ok(ids) => ids,
        Err(err) => {
            eprintln!("Invalid --case filter: {err}");
            return ExitCode::from(1);
        }
    };

    if cli.overwrite_baseline_reports
        && cli.report_out.is_none()
        && cli.json_out.is_none()
        && cli.metrics_out.is_none()
        && cli.defects_out.is_none()
    {
        let (j_default, m_default, d_default) = default_baseline_report_paths();
        cli.json_out = Some(j_default);
        cli.metrics_out = Some(m_default);
        cli.defects_out = Some(d_default);
    }

    // Hard safety guard: never allow overwriting frozen C1 baseline JSON files unless
    // `--overwrite-baseline-reports` was explicitly passed!
    for candidate_path in [
        cli.report_out.as_deref(),
        cli.json_out.as_deref(),
        cli.metrics_out.as_deref(),
        cli.defects_out.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if is_protected_baseline_report_path(candidate_path) && !cli.overwrite_baseline_reports {
            eprintln!(
                "Refusing to overwrite frozen C1 baseline report '{}' without --overwrite-baseline-reports (use --report-out <new-path> for C2/post-fix runs).",
                candidate_path.display()
            );
            return ExitCode::from(1);
        }
    }

    let filter = CaseSelectionFilter {
        suite: cli.suite.clone(),
        priority: cli.priority,
        case_ids,
    };

    let cases_label = if filter.case_ids.is_empty() {
        "all".to_owned()
    } else {
        filter.case_ids.join(",")
    };

    let started = Instant::now();
    println!(
        "=== Desktop Instance Manager — Cross-Platform Reliability Runner (mode={}, suite={}, priority={}, cases={}, cycles={}, seed=0x{:08x}) ===",
        cli.mode.as_str(),
        cli.suite,
        cli.priority.as_str(),
        cases_label,
        cli.cycles,
        cli.seed
    );

    let only_metrics = cli.suite.eq_ignore_ascii_case("metrics");
    let outcomes = if only_metrics {
        Vec::new()
    } else {
        run_filtered_suites(&filter, cli.cycles, cli.seed)
    };

    if !only_metrics && outcomes.is_empty() {
        eprintln!(
            "No test cases matched filter (suite={}, priority={}, cases={})",
            cli.suite,
            cli.priority.as_str(),
            cases_label
        );
        return ExitCode::from(1);
    }

    for o in &outcomes {
        let tier = PriorityTier::for_case(&o.id).as_str();
        let defect_tag = o
            .defect_id
            .as_deref()
            .map(|d| format!(" [defect={d}]"))
            .unwrap_or_default();
        let verif_tag = o
            .evidence
            .get("verification_kind")
            .and_then(Value::as_str)
            .map(|vk| format!(" [verification_kind={vk}]"))
            .unwrap_or_default();

        println!(
            "  [{:<24}] [{:<16}] {:<10} ({:>5} ms) {}{}{}",
            o.verdict.as_str(),
            tier,
            o.id,
            o.duration_ms,
            o.title,
            defect_tag,
            verif_tag
        );
    }

    let fixed_candidates = detect_fixed_candidates(&outcomes);
    if cli.mode == RegressionMode::BaselineAudit {
        for (cid, did) in &fixed_candidates {
            println!(
                "  [FIXED_CANDIDATE] {cid} ({did}) transitioned FAIL_DEFECT_REPRODUCED -> PASS (run --mode release-regression to enforce gate)"
            );
        }
    }

    let defects = collect_reproduced_defects(&outcomes);
    let total_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let summary = build_run_summary(outcomes, cli.seed, total_ms, defects.clone());

    if let Some(rep_path) = &cli.report_out {
        let extended_payload = serde_json::json!({
            "mode": cli.mode.as_str(),
            "priority_filter": cli.priority.as_str(),
            "case_filter": filter.case_ids,
            "require_native_os": cli.require_native_os,
            "fixed_candidates": fixed_candidates
                .iter()
                .map(|(cid, did)| serde_json::json!({ "test_id": cid, "defect_id": did }))
                .collect::<Vec<_>>(),
            "summary": &summary,
        });
        if let Err(err) = write_pretty_json(rep_path, &extended_payload) {
            eprintln!(
                "Failed writing report JSON to {}: {err}",
                rep_path.display()
            );
            return ExitCode::from(1);
        }
    }

    if let Some(out_path) = &cli.json_out {
        if let Err(err) = write_pretty_json(out_path, &summary) {
            eprintln!(
                "Failed writing JSON summary to {}: {err}",
                out_path.display()
            );
            return ExitCode::from(1);
        }
    }

    if let Some(def_path) = &cli.defects_out {
        let payload = serde_json::json!({
            "captured_at_utc": summary.executed_at_utc,
            "host_platform": summary.host_platform,
            "seed": summary.seed,
            "defect_count": defects.len(),
            "defects": defects,
        });
        if let Err(err) = write_pretty_json(def_path, &payload) {
            eprintln!(
                "Failed writing defects report to {}: {err}",
                def_path.display()
            );
            return ExitCode::from(1);
        }
    }

    let should_run_metrics = !cli.skip_metrics
        && (only_metrics || cli.metrics_out.is_some() || !filter.is_scoped_subset());
    if should_run_metrics {
        println!(
            "--- Running Real OS Fixture Telemetry & Scaling Benchmark (runs={}) ---",
            cli.metrics_runs
        );
        match collect_fixture_benchmark_report(cli.seed, cli.metrics_runs) {
            Ok(metrics) => {
                println!(
                    "  [METRICS] Coordinator+Node p50 startup={:.2}ms, p50 shutdown={:.2}ms, failure_rate={:.4}",
                    metrics.coordinator_node_fixture_startup.p50_ms,
                    metrics.coordinator_node_fixture_shutdown.p50_ms,
                    metrics.failure_rate
                );
                if let Some(native) = &metrics.native_rust_fixture_startup {
                    println!(
                        "  [METRICS] Native Rust fixture p50 startup={:.2}ms, p95={:.2}ms",
                        native.p50_ms, native.p95_ms
                    );
                }
                if let Some(m_path) = &cli.metrics_out {
                    if let Err(err) = write_pretty_json(m_path, &metrics) {
                        eprintln!("Failed writing metrics JSON to {}: {err}", m_path.display());
                        return ExitCode::from(1);
                    }
                }
            }
            Err(err) => {
                eprintln!("Metrics collection failed: {err}");
                return ExitCode::from(1);
            }
        }
    }

    println!(
        "=== Summary (mode={}): total={} | pass={} | defects_reproduced={} | fixed_candidates={} | skipped_host={} | unexpected_fail={} | elapsed={}ms ===",
        cli.mode.as_str(),
        summary.total_cases,
        summary.passed_cases,
        summary.defects_reproduced_cases,
        fixed_candidates.len(),
        summary.skipped_host_cases,
        summary.unexpected_failure_cases,
        summary.total_duration_ms
    );

    match cli.mode {
        RegressionMode::BaselineAudit => {
            if summary.unexpected_failure_cases > 0 {
                return ExitCode::from(1);
            }
            if cli.require_native_os && has_incomplete_native_os_coverage(&summary.outcomes) {
                print_incomplete_native_os_table(&summary.outcomes);
                return ExitCode::from(3);
            }
            ExitCode::SUCCESS
        }
        RegressionMode::ReleaseRegression => {
            let blocking: Vec<_> = summary
                .outcomes
                .iter()
                .filter(|o| {
                    matches!(
                        o.verdict,
                        TestVerdict::FailDefectReproduced | TestVerdict::FailUnexpected
                    )
                })
                .collect();

            if !blocking.is_empty() {
                println!(
                    "=== RELEASE_GATE_FAILED (mode=release-regression; EXPECTED_FAILURE / FAIL_DEFECT_REPRODUCED never passes) ==="
                );
                println!(
                    "  {:<10} | {:<16} | {:<24} | {:<10} | {:<26} | IMPLICATED FILES",
                    "CASE", "PRIORITY", "VERDICT", "DEFECT", "VERIFICATION_KIND"
                );
                println!("  {}", "-".repeat(118));
                for o in blocking {
                    let tier = PriorityTier::for_case(&o.id).as_str();
                    let def = o.defect_id.as_deref().unwrap_or("UNEXPECTED");
                    let verif = o
                        .evidence
                        .get("verification_kind")
                        .and_then(Value::as_str)
                        .unwrap_or("HOST_RUNTIME");
                    let files = if o.affected_files.is_empty() {
                        "-".to_owned()
                    } else {
                        o.affected_files.join(", ")
                    };
                    println!(
                        "  {:<10} | {:<16} | {:<24} | {:<10} | {:<26} | {}",
                        o.id,
                        tier,
                        o.verdict.as_str(),
                        def,
                        verif,
                        files
                    );
                }
                return ExitCode::from(2);
            }

            if cli.require_native_os && has_incomplete_native_os_coverage(&summary.outcomes) {
                print_incomplete_native_os_table(&summary.outcomes);
                return ExitCode::from(3);
            }

            println!(
                "=== RELEASE_GATE_PASSED (passed={} / total={}) ===",
                summary.passed_cases, summary.total_cases
            );
            ExitCode::SUCCESS
        }
    }
}

fn has_incomplete_native_os_coverage(
    outcomes: &[dim_cross_platform_tests::harness::TestCaseOutcome],
) -> bool {
    outcomes.iter().any(|o| {
        o.verdict == TestVerdict::SkippedHostPlatform
            || o.evidence.get("verification_kind").and_then(Value::as_str)
                == Some("STATIC_CONFIRMED_NON_HOST")
    })
}

fn print_incomplete_native_os_table(
    outcomes: &[dim_cross_platform_tests::harness::TestCaseOutcome],
) {
    println!(
        "=== RELEASE_GATE_INCOMPLETE_NATIVE_OS (--require-native-os blocked non-host skip/static cases) ==="
    );
    for o in outcomes {
        let is_non_host_static = o.evidence.get("verification_kind").and_then(Value::as_str)
            == Some("STATIC_CONFIRMED_NON_HOST");
        if o.verdict == TestVerdict::SkippedHostPlatform || is_non_host_static {
            let verif = o
                .evidence
                .get("verification_kind")
                .and_then(Value::as_str)
                .unwrap_or("SKIPPED_HOST_PLATFORM");
            println!(
                "  [NATIVE_OS_REQUIRED] {:<10} verdict={} verification_kind={}",
                o.id,
                o.verdict.as_str(),
                verif
            );
        }
    }
}
