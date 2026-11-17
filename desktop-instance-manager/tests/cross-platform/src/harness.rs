//! Core test execution harness, isolated sandbox RAII cleanup, and structured outcome models.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use dim_core::{now_rfc3339_utc, LaunchTarget, PlatformCapabilities, PlatformKind, ProfileStore};
use dim_process_manager::InstanceCoordinator;
use dim_profile_store::{unlink_reparse_points_safe, FileBackupManager, JsonProfileStore};
use dim_xf_fixture::is_os_pid_alive;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tempfile::TempDir;

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub use dim_platform_linux::LinuxPlatformAdapter as ActiveHostAdapter;
#[cfg(target_os = "macos")]
pub use dim_platform_macos::MacOsPlatformAdapter as ActiveHostAdapter;
#[cfg(target_os = "windows")]
pub use dim_platform_windows::WindowsPlatformAdapter as ActiveHostAdapter;

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
