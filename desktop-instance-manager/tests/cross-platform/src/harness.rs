//! Core test execution harness, isolated sandbox RAII cleanup, and structured outcome models.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use dim_core::{
    now_rfc3339_utc, InstanceLauncher, LaunchTarget, PlatformCapabilities, PlatformKind,
    ProfileRecord, ProfileStore,
};
use dim_process_manager::InstanceCoordinator;
use dim_profile_store::{
    unlink_reparse_points_safe, verify_profile_id_marker, FileBackupManager, JsonProfileStore,
};
use dim_xf_fixture::{is_os_pid_alive, sha256_file};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tempfile::TempDir;

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub use dim_platform_linux::LinuxPlatformAdapter as ActiveHostAdapter;
#[cfg(target_os = "macos")]
pub use dim_platform_macos::MacOsPlatformAdapter as ActiveHostAdapter;
#[cfg(target_os = "windows")]
pub use dim_platform_windows::WindowsPlatformAdapter as ActiveHostAdapter;

/// Canonical 55 test case IDs in deterministic suite execution order.
pub const CANONICAL_CASE_IDS: &[&str] = &[
    // Suite 01: Process Lifecycle (9)
    "LC-01",
    "LC-02",
    "LC-03",
    "LC-04",
    "LC-05",
    "LC-06",
    "LC-07",
    "LC-08",
    "LC-09",
    // Suite 02: Profile Management (8)
    "PM-01",
    "PM-02",
    "PM-03",
    "PM-04",
    "PM-05",
    "PM-06",
    "PM-07",
    "PM-08",
    // Suite 03: Data Safety (8)
    "DS-01",
    "DS-02",
    "DS-03",
    "DS-04",
    "DS-05",
    "DS-06",
    "DS-07",
    "DS-08",
    // Suite 04: Reliability & Soak (8)
    "REL-01",
    "REL-02",
    "REL-03",
    "REL-04",
    "REL-05",
    "REL-06",
    "REL-07",
    "REL-08",
    // Suite 05: Cross-Platform Differences (12)
    "XP-WIN-01",
    "XP-WIN-02",
    "XP-WIN-03",
    "XP-WIN-04",
    "XP-MAC-01",
    "XP-MAC-02",
    "XP-MAC-03",
    "XP-MAC-04",
    "XP-LNX-01",
    "XP-LNX-02",
    "XP-LNX-03",
    "XP-LNX-04",
    // Suite 06: Adapter & CLI Contracts (10)
    "CT-01",
    "CT-02",
    "CT-03",
    "CT-04",
    "CT-05",
    "CT-06",
    "CT-07",
    "CT-08",
    "CT-09",
    "CT-10",
];

/// Frozen Mission C1 baseline `(test_id, defect_id)` pairs reproduced at `a2143550935d`.
pub const KNOWN_BASELINE_DEFECTS: &[(&str, &str)] = &[
    ("LC-07", "DEFECT-03"),
    ("LC-08", "DEFECT-04"),
    ("LC-09", "DEFECT-06"),
    ("PM-04", "DEFECT-10"),
    ("PM-08", "DEFECT-09"),
    ("DS-08", "DEFECT-07"),
    ("XP-MAC-03", "DEFECT-02"),
    ("XP-LNX-02", "DEFECT-01"),
    ("CT-08", "DEFECT-08"),
    ("CT-09", "DEFECT-08"),
    ("CT-10", "DEFECT-08"),
];

/// Controlled execution mode governing how `FailDefectReproduced` is gated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RegressionMode {
    /// Baseline audit mode: separates known product defects (`FAIL_DEFECT_REPRODUCED`) from
    /// unexpected failures (`FAIL_UNEXPECTED`) and reports `FIXED_CANDIDATE` transitions.
    BaselineAudit,
    /// Mandatory post-fix release gate: never treats `FAIL_DEFECT_REPRODUCED` as `PASS`;
    /// exits `2` on any defect or unexpected failure (and `3` when `--require-native-os` hits non-host skips).
    ReleaseRegression,
}

impl RegressionMode {
    /// Parses `--mode <baseline-audit|release-regression>` case-insensitively.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "baseline-audit" | "baseline_audit" => Ok(Self::BaselineAudit),
            "release-regression" | "release_regression" => Ok(Self::ReleaseRegression),
            other => Err(format!(
                "invalid --mode '{other}'; expected 'baseline-audit' or 'release-regression'"
            )),
        }
    }

    /// Canonical CLI flag value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BaselineAudit => "baseline-audit",
            Self::ReleaseRegression => "release-regression",
        }
    }
}

/// Mission C2 defect-fix priority tier for each canonical test case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PriorityTier {
    /// P0 critical concurrency, pre-lock orphan, running-delete bypass, and corrupt-store safety (`4` cases).
    P0,
    /// P1 termination, dangling `SingletonLock`, POSIX launch parity, cross-OS backup UID, and CLI parity (`6` cases).
    P1,
    /// P2 Vietnamese Unicode non-ASCII duplicate folding (`1` case).
    P2,
    /// Passing baseline regression guard (`44` cases).
    RegressionGuard,
}

impl PriorityTier {
    /// Resolves the canonical Mission C2 priority tier for a test case ID.
    #[must_use]
    pub fn for_case(case_id: &str) -> Self {
        let norm = case_id.trim().to_ascii_uppercase();
        match norm.as_str() {
            "LC-07" | "LC-08" | "PM-08" | "CT-10" => Self::P0,
            "LC-09" | "DS-08" | "XP-MAC-03" | "XP-LNX-02" | "CT-08" | "CT-09" => Self::P1,
            "PM-04" => Self::P2,
            _ => Self::RegressionGuard,
        }
    }

    /// Short uppercase label for console tables and JSON artifacts.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::P0 => "P0",
            Self::P1 => "P1",
            Self::P2 => "P2",
            Self::RegressionGuard => "REGRESSION_GUARD",
        }
    }
}

/// CLI `--priority` filter selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriorityFilter {
    /// Only P0 cases (`LC-07`, `LC-08`, `PM-08`, `CT-10`).
    P0,
    /// Only P1 cases (`LC-09`, `DS-08`, `XP-MAC-03`, `XP-LNX-02`, `CT-08`, `CT-09`).
    P1,
    /// Only P2 cases (`PM-04`).
    P2,
    /// Combined P0 and P1 mandatory gates (`10` cases).
    P0P1,
    /// All prioritized baseline defect cases (`P0` + `P1` + `P2` = `11` cases).
    P0P2,
    /// Only passing regression guards (`44` cases).
    RegressionGuard,
    /// Every canonical test case (`55` cases).
    All,
}

impl PriorityFilter {
    /// Parses `--priority <p0|p1|p2|p0-p1|p0-p2|regression-guard|all>` case-insensitively.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "p0" => Ok(Self::P0),
            "p1" => Ok(Self::P1),
            "p2" => Ok(Self::P2),
            "p0-p1" | "p0_p1" | "p0,p1" => Ok(Self::P0P1),
            "p0-p2" | "p0_p2" | "defects" => Ok(Self::P0P2),
            "regression-guard" | "regression_guard" | "guard" => Ok(Self::RegressionGuard),
            "all" => Ok(Self::All),
            other => Err(format!(
                "invalid --priority '{other}'; expected p0, p1, p2, p0-p1, p0-p2, regression-guard, or all"
            )),
        }
    }

    /// Canonical CLI flag label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::P0 => "p0",
            Self::P1 => "p1",
            Self::P2 => "p2",
            Self::P0P1 => "p0-p1",
            Self::P0P2 => "p0-p2",
            Self::RegressionGuard => "regression-guard",
            Self::All => "all",
        }
    }

    /// Returns `true` when `tier` is included in this filter.
    #[must_use]
    pub const fn matches_tier(self, tier: PriorityTier) -> bool {
        match self {
            Self::P0 => matches!(tier, PriorityTier::P0),
            Self::P1 => matches!(tier, PriorityTier::P1),
            Self::P2 => matches!(tier, PriorityTier::P2),
            Self::P0P1 => matches!(tier, PriorityTier::P0 | PriorityTier::P1),
            Self::P0P2 => matches!(tier, PriorityTier::P0 | PriorityTier::P1 | PriorityTier::P2),
            Self::RegressionGuard => matches!(tier, PriorityTier::RegressionGuard),
            Self::All => true,
        }
    }
}

/// Composable filter selecting suites, priority tiers, and/or explicit test case IDs.
#[derive(Debug, Clone)]
pub struct CaseSelectionFilter {
    /// Suite selector (`all`, `lifecycle`, `profile_management`, `data_safety`, `reliability`, `cross_platform`, `contract`).
    pub suite: String,
    /// Priority tier selector.
    pub priority: PriorityFilter,
    /// Validated uppercase canonical test IDs when `--case` is provided; empty means no ID restriction.
    pub case_ids: Vec<String>,
}

impl Default for CaseSelectionFilter {
    fn default() -> Self {
        Self {
            suite: "all".to_owned(),
            priority: PriorityFilter::All,
            case_ids: Vec::new(),
        }
    }
}

impl CaseSelectionFilter {
    /// Validates and normalizes comma-separated or repeated `--case <ID[,ID...]>` tokens.
    pub fn parse_case_ids(raw_specs: &[String]) -> Result<Vec<String>, String> {
        if raw_specs.is_empty() {
            return Ok(Vec::new());
        }
        let mut normalized = Vec::new();
        for spec in raw_specs {
            for token in spec.split(',') {
                let candidate = token.trim().to_ascii_uppercase();
                if candidate.is_empty() {
                    continue;
                }
                let Some(&canonical) = CANONICAL_CASE_IDS.iter().find(|&&id| id == candidate)
                else {
                    return Err(format!(
                        "unknown test case ID '{token}'; expected one of LC-01..LC-09, PM-01..PM-08, DS-01..DS-08, REL-01..REL-08, XP-WIN-01..04, XP-MAC-01..04, XP-LNX-01..04, CT-01..CT-10"
                    ));
                };
                if !normalized.iter().any(|existing| existing == canonical) {
                    normalized.push(canonical.to_owned());
                }
            }
        }
        if normalized.is_empty() {
            return Err("--case requires at least one non-empty test case ID".to_owned());
        }
        Ok(normalized)
    }

    /// Returns `true` when this filter narrows execution to a subset of cases (`--case` or `--priority != all` or `--suite != all`).
    #[must_use]
    pub fn is_scoped_subset(&self) -> bool {
        let s = self.suite.trim();
        let all_suites = s.is_empty() || s.eq_ignore_ascii_case("all");
        !all_suites || self.priority != PriorityFilter::All || !self.case_ids.is_empty()
    }

    /// Returns `true` if `case_id` passes both the priority filter and any `--case` list.
    #[must_use]
    pub fn matches_case(&self, case_id: &str) -> bool {
        if !self.priority.matches_tier(PriorityTier::for_case(case_id)) {
            return false;
        }
        if self.case_ids.is_empty() {
            return true;
        }
        self.case_ids
            .iter()
            .any(|wanted| wanted.eq_ignore_ascii_case(case_id))
    }
}

/// Returns `true` if `path` resolves to one of the frozen C1 baseline JSON artifacts in `plans/reports/reliability/`.
#[must_use]
pub fn is_protected_baseline_report_path(path: &Path) -> bool {
    const FROZEN_FILES: &[&str] = &[
        "windows-host-execution-evidence.json",
        "metrics-benchmark-windows.json",
        "defect-reproductions.json",
    ];
    let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if !FROZEN_FILES.iter().any(|f| f.eq_ignore_ascii_case(fname)) {
        return false;
    }
    let normalized = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    normalized.contains("plans/reports/reliability/")
        || path.parent().is_none_or(|p| p.as_os_str().is_empty())
}

/// Identifies known C1 baseline defect cases that transitioned to `PASS` (`FIXED_CANDIDATE`).
#[must_use]
pub fn detect_fixed_candidates(outcomes: &[TestCaseOutcome]) -> Vec<(String, String)> {
    let mut fixed = Vec::new();
    for &(case_id, defect_id) in KNOWN_BASELINE_DEFECTS {
        if outcomes
            .iter()
            .any(|o| o.id.eq_ignore_ascii_case(case_id) && o.verdict == TestVerdict::Pass)
        {
            fixed.push((case_id.to_owned(), defect_id.to_owned()));
        }
    }
    fixed
}

/// Verdict classification for each executed test case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TestVerdict {
    /// Expected invariant or safety contract passed on the host OS.
    Pass,
    /// Product defect or cross-platform/adapter divergence deterministically reproduced.
    FailDefectReproduced,
    /// Unexpected test assertion or harness failure.
    FailUnexpected,
    /// Requires native kernel/runtime of another OS (never falsely marked PASS).
    SkippedHostPlatform,
}

impl TestVerdict {
    /// Short label for Markdown reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::FailDefectReproduced => "FAIL (DEFECT_REPRODUCED)",
            Self::FailUnexpected => "FAIL (UNEXPECTED)",
            Self::SkippedHostPlatform => "SKIPPED (REQUIRES_TARGET_OS)",
        }
    }
}

/// Structured evidence record for a single test scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseOutcome {
    /// Stable test case identifier (e.g. `LC-01`, `PM-04`, `DS-08`, `CT-07`).
    pub id: String,
    /// Suite category (`lifecycle`, `profile_management`, `data_safety`, `reliability`, `cross_platform`, `contract`).
    pub category: String,
    /// Concise test title.
    pub title: String,
    /// Platform scope (`all`, `windows`, `macos`, `linux`, `windows+ps-parity`).
    pub platform_scope: String,
    /// Observed verdict.
    pub verdict: TestVerdict,
    /// Wall-clock execution duration in milliseconds.
    pub duration_ms: u64,
    /// Machine-readable & human-auditable evidence summary.
    pub evidence: Value,
    /// Linked product defect ID when `verdict == FailDefectReproduced`.
    pub defect_id: Option<String>,
    /// Product source files implicated when a defect is reproduced.
    pub affected_files: Vec<String>,
    /// Minimal CLI or fixture reproduction command.
    pub minimal_reproduction: Option<String>,
}

/// Detailed product defect entry aggregated across suites.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReproducedDefectRecord {
    /// Defect identifier (`DEFECT-01` .. `DEFECT-10`).
    pub defect_id: String,
    /// Severity (`HIGH`, `MEDIUM`, `LOW`).
    pub severity: String,
    /// Test case IDs that reproduced this defect.
    pub test_case_ids: Vec<String>,
    /// Concise summary of the root cause and user/reliability impact.
    pub summary: String,
    /// Exact product files and line regions involved.
    pub affected_files: Vec<String>,
    /// Minimal reproduction steps on the synthetic fixture.
    pub minimal_reproduction: String,
    /// Captured runtime evidence payload.
    pub observed_evidence: Value,
}

/// Complete aggregated test run output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessRunSummary {
    /// Overall harness execution health (`true` when `unexpected_failures == 0`).
    pub harness_ok: bool,
    /// Strict gate status (`true` only when `unexpected_failures == 0 && defects_reproduced == 0`).
    pub strict_gate_ok: bool,
    /// Execution timestamp in RFC 3339 UTC.
    pub executed_at_utc: String,
    /// Host platform (`windows`, `macos`, `linux`).
    pub host_platform: String,
    /// Deterministic seed used for this run.
    pub seed: String,
    /// Total test cases registered.
    pub total_cases: usize,
    /// Count of `PASS` cases.
    pub passed_cases: usize,
    /// Count of `FAIL (DEFECT_REPRODUCED)` cases.
    pub defects_reproduced_cases: usize,
    /// Count of `FAIL (UNEXPECTED)` cases.
    pub unexpected_failure_cases: usize,
    /// Count of `SKIPPED (REQUIRES_TARGET_OS)` cases.
    pub skipped_host_cases: usize,
    /// Total wall-clock duration in milliseconds.
    pub total_duration_ms: u64,
    /// Individual test outcomes in deterministic order.
    pub outcomes: Vec<TestCaseOutcome>,
    /// Deduplicated catalog of product defects reproduced during this run.
    pub defects: Vec<ReproducedDefectRecord>,
}

/// Helper builder for timing and recording a single test case cleanly.
pub fn run_case<F>(
    id: &str,
    category: &str,
    title: &str,
    platform_scope: &str,
    f: F,
) -> TestCaseOutcome
where
    F: FnOnce() -> Result<CaseSuccess, String>,
{
    let started = Instant::now();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Ok(done)) => TestCaseOutcome {
            id: id.to_owned(),
            category: category.to_owned(),
            title: title.to_owned(),
            platform_scope: platform_scope.to_owned(),
            verdict: done.verdict,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            evidence: done.evidence,
            defect_id: done.defect_id,
            affected_files: done.affected_files,
            minimal_reproduction: done.minimal_reproduction,
        },
        Ok(Err(err_msg)) => TestCaseOutcome {
            id: id.to_owned(),
            category: category.to_owned(),
            title: title.to_owned(),
            platform_scope: platform_scope.to_owned(),
            verdict: TestVerdict::FailUnexpected,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            evidence: serde_json::json!({ "error": err_msg }),
            defect_id: None,
            affected_files: Vec::new(),
            minimal_reproduction: None,
        },
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                (*s).to_owned()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "test case panicked".to_owned()
            };
            TestCaseOutcome {
                id: id.to_owned(),
                category: category.to_owned(),
                title: title.to_owned(),
                platform_scope: platform_scope.to_owned(),
                verdict: TestVerdict::FailUnexpected,
                duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                evidence: serde_json::json!({ "panic": msg }),
                defect_id: None,
                affected_files: Vec::new(),
                minimal_reproduction: None,
            }
        }
    }
}

/// Conditionally executes `run_case` only when `filter` is `None` or includes `id`.
pub fn run_case_if<F>(
    filter: Option<&CaseSelectionFilter>,
    id: &str,
    category: &str,
    title: &str,
    platform_scope: &str,
    f: F,
) -> Option<TestCaseOutcome>
where
    F: FnOnce() -> Result<CaseSuccess, String>,
{
    if let Some(flt) = filter {
        if !flt.matches_case(id) {
            return None;
        }
    }
    Some(run_case(id, category, title, platform_scope, f))
}

/// Structured return value from inside `run_case`.
#[derive(Debug, Clone)]
pub struct CaseSuccess {
    /// Verdict of the test case.
    pub verdict: TestVerdict,
    /// Evidence JSON object.
    pub evidence: Value,
    /// Optional product defect ID if reproduced.
    pub defect_id: Option<String>,
    /// Optional list of implicated product files.
    pub affected_files: Vec<String>,
    /// Optional minimal reproduction command/steps.
    pub minimal_reproduction: Option<String>,
}

impl CaseSuccess {
    /// Constructs a `PASS` outcome with structured JSON evidence.
    #[must_use]
    pub fn pass(evidence: Value) -> Self {
        Self {
            verdict: TestVerdict::Pass,
            evidence,
            defect_id: None,
            affected_files: Vec::new(),
            minimal_reproduction: None,
        }
    }

    /// Constructs a `SKIPPED (REQUIRES_TARGET_OS)` outcome when current host OS cannot execute a native kernel check.
    #[must_use]
    pub fn skipped_host(required_os: &str, reason: &str) -> Self {
        Self {
            verdict: TestVerdict::SkippedHostPlatform,
            evidence: serde_json::json!({
                "host_os": PlatformKind::current().as_str(),
                "required_os": required_os,
                "reason": reason,
            }),
            defect_id: None,
            affected_files: Vec::new(),
            minimal_reproduction: None,
        }
    }

    /// Constructs a `FAIL (DEFECT_REPRODUCED)` outcome with root-cause file pointers and minimal repro.
    #[must_use]
    pub fn defect(
        defect_id: &str,
        affected_files: &[&str],
        minimal_reproduction: &str,
        evidence: Value,
    ) -> Self {
        Self {
            verdict: TestVerdict::FailDefectReproduced,
            evidence,
            defect_id: Some(defect_id.to_owned()),
            affected_files: affected_files.iter().map(|s| (*s).to_owned()).collect(),
            minimal_reproduction: Some(minimal_reproduction.to_owned()),
        }
    }
}

/// Returns the `desktop-instance-manager` directory root.
#[must_use]
pub fn dim_root_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("tests/cross-platform resides two levels under desktop-instance-manager")
        .to_path_buf()
}

/// Returns the path to `fixtures/cross-platform/xf-fixture.mjs`.
#[must_use]
pub fn xf_fixture_mjs_path() -> PathBuf {
    dim_root_dir()
        .join("fixtures")
        .join("cross-platform")
        .join("xf-fixture.mjs")
}

/// Returns the compiled native `xf-desktop-fixture` binary path (preferring release over debug).
#[must_use]
pub fn xf_fixture_bin_path() -> PathBuf {
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let base = dim_root_dir()
        .join("fixtures")
        .join("cross-platform")
        .join("target");
    let release = base
        .join("release")
        .join(format!("xf-desktop-fixture{ext}"));
    if release.exists() {
        return release;
    }
    base.join("debug").join(format!("xf-desktop-fixture{ext}"))
}

/// Returns the compiled `dim-cli` binary path if built in `desktop-instance-manager/target`.
#[must_use]
pub fn dim_cli_bin_path() -> PathBuf {
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let base = dim_root_dir().join("target");
    let release = base.join("release").join(format!("dim-cli{ext}"));
    if release.exists() {
        return release;
    }
    base.join("debug").join(format!("dim-cli{ext}"))
}

/// Returns the PowerShell Signed-Host Bridge script `scripts/dim-host.ps1`.
#[must_use]
pub fn dim_host_ps1_path() -> PathBuf {
    dim_root_dir().join("scripts").join("dim-host.ps1")
}

/// Builds a `LaunchTarget::NodeFixture` pointing at `xf-fixture.mjs` with custom CLI flags.
#[must_use]
pub fn make_node_fixture_target(extra_args: &[&str]) -> LaunchTarget {
    LaunchTarget::NodeFixture {
        node_path: None,
        script_path: xf_fixture_mjs_path(),
        extra_args: extra_args.iter().map(|s| (*s).to_owned()).collect(),
    }
}

/// RAII temporary sandbox owning an isolated `DIM_DATA_DIR`, coordinator, backup manager,
/// and tracked PID reaper so fault-injection tests never leak processes or files.
pub struct IsolatedSandbox {
    temp_dir: TempDir,
    data_dir: PathBuf,
    /// Store instance rooted inside `data_dir`.
    pub store: Arc<JsonProfileStore>,
    /// Active OS platform adapter.
    pub adapter: Arc<ActiveHostAdapter>,
    /// Coordinator bound to `store` and `adapter`.
    pub coordinator: InstanceCoordinator<ActiveHostAdapter>,
    /// Backup manager wired with the active adapter's disk-space probe.
    pub backups: FileBackupManager,
    tracked_pids: Vec<u32>,
    acl_modified_dirs: Vec<PathBuf>,
}

impl IsolatedSandbox {
    /// Creates a brand-new temporary sandbox completely isolated from user data.
    pub fn new() -> Result<Self, String> {
        let temp_dir = tempfile::Builder::new()
            .prefix("dim-xf-sandbox-")
            .tempdir()
            .map_err(|e| format!("failed to allocate tempdir: {e}"))?;
        let data_dir = temp_dir.path().join("dim-data");
        let store = Arc::new(
            JsonProfileStore::new(data_dir.clone())
                .map_err(|e| format!("failed to init JsonProfileStore: {e}"))?,
        );
        store
            .ensure_initialized()
            .map_err(|e| format!("failed to initialize store directories: {e}"))?;

        let adapter = Arc::new(ActiveHostAdapter::new());
        let coordinator = InstanceCoordinator::new(Arc::clone(&store), Arc::clone(&adapter));
        let probe_adapter = Arc::clone(&adapter);
        let backups = FileBackupManager::new(Arc::clone(&store))
            .with_disk_space_probe(Arc::new(move |p| probe_adapter.free_disk_bytes(p)));

        Ok(Self {
            temp_dir,
            data_dir,
            store,
            adapter,
            coordinator,
            backups,
            tracked_pids: Vec::new(),
            acl_modified_dirs: Vec::new(),
        })
    }

    /// Root path of the temporary sandbox.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Isolated `DIM_DATA_DIR` path inside the sandbox.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Registers an externally or fixture-spawned PID so `Drop` guarantees cleanup.
    pub fn track_pid(&mut self, pid: u32) {
        if pid > 0 && pid != std::process::id() && !self.tracked_pids.contains(&pid) {
            self.tracked_pids.push(pid);
        }
    }

    /// Registers a directory whose ACL/permissions were loosened or tightened so `Drop` restores them.
    pub fn track_acl_dir(&mut self, dir: PathBuf) {
        self.acl_modified_dirs.push(dir);
    }

    /// Resolves the platform-specific isolated `ZaloData` path for a profile directory.
    #[must_use]
    pub fn zalo_data_dir_for(&self, profile_dir: &Path) -> PathBuf {
        match PlatformKind::current() {
            PlatformKind::Windows => profile_dir.join("AppData").join("Roaming").join("ZaloData"),
            PlatformKind::MacOs => profile_dir
                .join("Library")
                .join("Application Support")
                .join("ZaloData"),
            PlatformKind::Linux => profile_dir.join(".config").join("ZaloData"),
        }
    }

    /// Reaps any surviving fixture processes discovered inside `Profiles/` or `tracked_pids`.
    pub fn reap_all_sandbox_processes(&mut self) {
        if let Ok(profiles) = self.store.list_profiles() {
            for rec in profiles {
                let _ = self.coordinator.stop_profile(&rec.id);
                if let Ok(pdir) = self.store.profile_dir_for(&rec) {
                    collect_pids_from_profile_artifacts(&pdir, &mut self.tracked_pids);
                }
            }
        }
        for &pid in &self.tracked_pids {
            if is_os_pid_alive(pid) {
                let _ = force_kill_unprotected_test_pid(pid);
            }
        }
    }
}

impl Drop for IsolatedSandbox {
    fn drop(&mut self) {
        self.reap_all_sandbox_processes();
        for dir in &self.acl_modified_dirs {
            restore_directory_permissions(dir);
        }
        let _ = unlink_reparse_points_safe(self.temp_dir.path());
    }
}

fn collect_pids_from_profile_artifacts(profile_dir: &Path, out: &mut Vec<u32>) {
    let mut push_pid = |p: u32| {
        if p > 0 && p != std::process::id() && !out.contains(&p) {
            out.push(p);
        }
    };

    if let Ok(raw) = fs::read_to_string(profile_dir.join(".runtime-pid")) {
        if let Ok(pid) = raw.trim().parse::<u32>() {
            push_pid(pid);
        } else if let Ok(v) = serde_json::from_str::<Value>(&raw) {
            if let Some(pid) = v
                .get("pid")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
            {
                push_pid(pid);
            }
        }
    }

    let zdirs = [
        profile_dir.join("AppData").join("Roaming").join("ZaloData"),
        profile_dir
            .join("Library")
            .join("Application Support")
            .join("ZaloData"),
        profile_dir.join(".config").join("ZaloData"),
    ];
    for zd in zdirs {
        if !zd.exists() {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(zd.join("lockfile")) {
            if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                if let Some(pid) = v
                    .get("pid")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                {
                    push_pid(pid);
                }
            }
        }
        if let Ok(raw) = fs::read_to_string(zd.join("children.json")) {
            if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                if let Some(arr) = v.get("child_pids").and_then(Value::as_array) {
                    for item in arr {
                        if let Some(pid) = item.as_u64().and_then(|n| u32::try_from(n).ok()) {
                            push_pid(pid);
                        }
                    }
                }
            }
        }
        if let Ok(entries) = fs::read_dir(&zd) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_s = name.to_string_lossy();
                if name_s.starts_with("worker-") && name_s.ends_with(".pid") {
                    if let Ok(raw) = fs::read_to_string(entry.path()) {
                        if let Ok(pid) = raw.trim().parse::<u32>() {
                            push_pid(pid);
                        }
                    }
                }
            }
        }
    }
}

/// Forcefully terminates a test-spawned fixture PID (`TerminateProcess` on Windows, `SIGKILL` on Unix).
pub fn force_kill_unprotected_test_pid(pid: u32) -> bool {
    if pid == 0 || pid == std::process::id() {
        return false;
    }
    #[cfg(windows)]
    {
        const PROCESS_TERMINATE: u32 = 0x0001;
        const SYNCHRONIZE: u32 = 0x0010_0000;
        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(
                dw_desired_access: u32,
                b_inherit_handle: i32,
                dw_process_id: u32,
            ) -> *mut std::ffi::c_void;
            fn TerminateProcess(h_process: *mut std::ffi::c_void, u_exit_code: u32) -> i32;
            fn WaitForSingleObject(h_handle: *mut std::ffi::c_void, dw_milliseconds: u32) -> u32;
            fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
        }
        // SAFETY: `pid` is a tracked test fixture child PID; handle is closed before returning.
        unsafe {
            let h = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, pid);
            if h.is_null() {
                return false;
            }
            let ok = TerminateProcess(h, 1);
            if ok != 0 {
                let _ = WaitForSingleObject(h, 1_500);
            }
            let _ = CloseHandle(h);
            ok != 0
        }
    }
    #[cfg(unix)]
    {
        let Ok(raw) = libc::pid_t::try_from(pid) else {
            return false;
        };
        // SAFETY: `raw` is a tracked positive child fixture PID.
        let rc = unsafe { libc::kill(raw, libc::SIGKILL) };
        rc == 0
    }
}

fn restore_directory_permissions(dir: &Path) {
    if !dir.exists() {
        return;
    }
    #[cfg(windows)]
    {
        let _ = Command::new("icacls")
            .arg(dir)
            .arg("/reset")
            .arg("/T")
            .arg("/C")
            .arg("/Q")
            .output();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(dir) {
            let mut perms = meta.permissions();
            perms.set_mode(0o700);
            let _ = fs::set_permissions(dir, perms);
        }
    }
}

/// Aggregates a list of `TestCaseOutcome` items and defect definitions into a [`HarnessRunSummary`].
#[must_use]
pub fn build_run_summary(
    outcomes: Vec<TestCaseOutcome>,
    seed: u64,
    total_duration_ms: u64,
    defect_catalog: Vec<ReproducedDefectRecord>,
) -> HarnessRunSummary {
    let passed_cases = outcomes
        .iter()
        .filter(|o| o.verdict == TestVerdict::Pass)
        .count();
    let defects_reproduced_cases = outcomes
        .iter()
        .filter(|o| o.verdict == TestVerdict::FailDefectReproduced)
        .count();
    let unexpected_failure_cases = outcomes
        .iter()
        .filter(|o| o.verdict == TestVerdict::FailUnexpected)
        .count();
    let skipped_host_cases = outcomes
        .iter()
        .filter(|o| o.verdict == TestVerdict::SkippedHostPlatform)
        .count();

    HarnessRunSummary {
        harness_ok: unexpected_failure_cases == 0,
        strict_gate_ok: unexpected_failure_cases == 0 && defects_reproduced_cases == 0,
        executed_at_utc: now_rfc3339_utc(),
        host_platform: PlatformKind::current().as_str().to_owned(),
        seed: format!("0x{seed:08x}"),
        total_cases: outcomes.len(),
        passed_cases,
        defects_reproduced_cases,
        unexpected_failure_cases,
        skipped_host_cases,
        total_duration_ms,
        outcomes,
        defects: defect_catalog,
    }
}

/// Snapshot of a sibling bystander profile seeded inside an [`IsolatedSandbox`] to prove
/// P0 fault-injection and defect-reproduction scenarios cause zero collateral damage.
#[derive(Debug, Clone)]
pub struct BystanderSnapshot {
    /// Bystander profile record persisted at seed time.
    pub record: ProfileRecord,
    /// Bystander profile root directory.
    pub profile_dir: PathBuf,
    /// Bystander isolated `ZaloData` directory.
    pub zalo_data_dir: PathBuf,
    /// Bystander canary payload path.
    pub canary_path: PathBuf,
    /// Expected SHA-256 hex digest of `canary_path`.
    pub canary_sha256: String,
}

impl BystanderSnapshot {
    /// Seeds a bystander profile with its isolated layout, `.zalo-profile-id` marker, and canary file.
    pub fn seed(
        sandbox: &IsolatedSandbox,
        name: &str,
        canary_payload: &str,
    ) -> Result<Self, String> {
        let record = sandbox
            .store
            .create_profile(name, "bystander-non-regression")
            .map_err(|e| format!("failed creating bystander profile '{name}': {e}"))?;
        let profile_dir = sandbox
            .store
            .profile_dir_for(&record)
            .map_err(|e| format!("failed resolving bystander profile_dir: {e}"))?;
        sandbox
            .adapter
            .ensure_profile_layout(&record, &profile_dir)
            .map_err(|e| format!("failed initializing bystander layout: {e}"))?;
        let zalo_data_dir = sandbox.zalo_data_dir_for(&profile_dir);
        let canary_path = zalo_data_dir.join("bystander-canary.dat");
        fs::write(&canary_path, canary_payload.as_bytes())
            .map_err(|e| format!("failed writing bystander canary: {e}"))?;
        let canary_sha256 = sha256_file(&canary_path)
            .map_err(|e| format!("failed hashing bystander canary: {e}"))?;

        Ok(Self {
            record,
            profile_dir,
            zalo_data_dir,
            canary_path,
            canary_sha256,
        })
    }

    /// Verifies that the bystander profile's metadata entry, directory, `.zalo-profile-id` marker,
    /// and canary file SHA-256 remain 100% untouched.
    pub fn verify_untouched(&self, sandbox: &IsolatedSandbox) -> Result<Value, String> {
        if !self.profile_dir.is_dir() {
            return Err(format!(
                "bystander profile directory disappeared: {}",
                self.profile_dir.display()
            ));
        }
        verify_profile_id_marker(&self.zalo_data_dir, &self.record.id, true)
            .map_err(|e| format!("bystander .zalo-profile-id marker corrupted: {e}"))?;
        let observed_hash = sha256_file(&self.canary_path)
            .map_err(|e| format!("bystander canary file missing or unreadable: {e}"))?;
        if observed_hash != self.canary_sha256 {
            return Err(format!(
                "bystander canary SHA-256 mismatch: expected {}, got {observed_hash}",
                self.canary_sha256
            ));
        }
        let persisted = sandbox
            .store
            .get_profile(&self.record.id)
            .map_err(|e| format!("bystander metadata entry missing in profiles.json: {e}"))?;
        if persisted.name != self.record.name
            || persisted.directory_name != self.record.directory_name
            || persisted.notes != self.record.notes
        {
            return Err(format!(
                "bystander metadata record was mutated: expected {:?}, got {persisted:?}",
                self.record
            ));
        }

        Ok(serde_json::json!({
            "bystander_profile_id": self.record.id,
            "bystander_directory_intact": true,
            "bystander_marker_verified": true,
            "bystander_canary_sha256_verified": true,
            "bystander_metadata_intact": true,
        }))
    }
}
