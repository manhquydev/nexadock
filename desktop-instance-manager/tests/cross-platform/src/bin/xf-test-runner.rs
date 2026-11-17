//! Standalone CLI test & telemetry runner (`xf-test-runner`).
//!
//! Usage examples:
//!   cargo run --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner -- \
//!     --suite all --cycles 100 --seed 0x20261117 \
//!     --json-out plans/reports/reliability/windows-host-execution-evidence.json \
//!     --metrics-out plans/reports/reliability/metrics-benchmark-windows.json \
//!     --defects-out plans/reports/reliability/defect-reproductions.json

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use dim_core::redact_sensitive_text;
use dim_cross_platform_tests::harness::build_run_summary;
use dim_cross_platform_tests::metrics::collect_fixture_benchmark_report;
use dim_cross_platform_tests::suites::{collect_reproduced_defects, run_selected_suites};

struct RunnerCli {
    suite: String,
    cycles: usize,
    metrics_runs: usize,
    seed: u64,
    skip_metrics: bool,
    fail_on_product_defects: bool,
    json_out: Option<PathBuf>,
    metrics_out: Option<PathBuf>,
    defects_out: Option<PathBuf>,
}

fn parse_cli() -> RunnerCli {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut cli = RunnerCli {
        suite: "all".to_owned(),
        cycles: 100,
        metrics_runs: 10,
        seed: 0x2026_1117,
        skip_metrics: false,
        fail_on_product_defects: false,
        json_out: None,
        metrics_out: None,
        defects_out: None,
    };

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--suite" if i + 1 < args.len() => {
                i += 1;
                cli.suite = args[i].clone();
            }
            "--cycles" if i + 1 < args.len() => {
                i += 1;
                cli.cycles = args[i].parse::<usize>().unwrap_or(100).max(10);
            }
            "--metrics-runs" if i + 1 < args.len() => {
                i += 1;
                cli.metrics_runs = args[i].parse::<usize>().unwrap_or(10).max(3);
            }
            "--seed" if i + 1 < args.len() => {
                i += 1;
                let s = &args[i];
                cli.seed = if let Some(hex) = s.strip_prefix("0x") {
                    u64::from_str_radix(hex, 16).unwrap_or(0x2026_1117)
                } else {
                    s.parse::<u64>().unwrap_or(0x2026_1117)
                };
            }
            "--skip-metrics" => cli.skip_metrics = true,
            "--fail-on-product-defects" => cli.fail_on_product_defects = true,
            "--json-out" if i + 1 < args.len() => {
                i += 1;
                cli.json_out = Some(PathBuf::from(&args[i]));
            }
            "--metrics-out" if i + 1 < args.len() => {
                i += 1;
                cli.metrics_out = Some(PathBuf::from(&args[i]));
            }
            "--defects-out" if i + 1 < args.len() => {
                i += 1;
                cli.defects_out = Some(PathBuf::from(&args[i]));
            }
            _ => {}
        }
        i += 1;
    }

    cli
}

fn write_pretty_json<T: serde::Serialize>(path: &Path, val: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
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

fn main() -> ExitCode {
    let cli = parse_cli();
    let started = Instant::now();

    println!(
        "=== Desktop Instance Manager — Cross-Platform Reliability Runner (suite={}, cycles={}, seed=0x{:08x}) ===",
        cli.suite, cli.cycles, cli.seed
    );

    let only_metrics = cli.suite.eq_ignore_ascii_case("metrics");
    let outcomes = if only_metrics {
        Vec::new()
    } else {
        run_selected_suites(&cli.suite, cli.cycles, cli.seed)
    };

    for o in &outcomes {
        println!(
            "  [{:<24}] {:<10} ({:>5} ms) {}",
            o.verdict.as_str(),
            o.id,
            o.duration_ms,
            o.title
        );
    }

    let defects = collect_reproduced_defects(&outcomes);
    let total_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let summary = build_run_summary(outcomes, cli.seed, total_ms, defects.clone());

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

    if !cli.skip_metrics
        && (cli.suite.eq_ignore_ascii_case("all") || only_metrics || cli.metrics_out.is_some())
    {
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
        "=== Summary: total={} | pass={} | defects_reproduced={} | skipped_host={} | unexpected_fail={} | elapsed={}ms ===",
        summary.total_cases,
        summary.passed_cases,
        summary.defects_reproduced_cases,
        summary.skipped_host_cases,
        summary.unexpected_failure_cases,
        summary.total_duration_ms
    );

    if summary.unexpected_failure_cases > 0 {
        return ExitCode::from(1);
    }
    if cli.fail_on_product_defects && summary.defects_reproduced_cases > 0 {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}
