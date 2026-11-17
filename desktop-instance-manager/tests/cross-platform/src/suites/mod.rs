//! Modular cross-platform test suites covering lifecycle, profile CRUD, data safety,
//! reliability soak, OS-specific isolation boundaries, and adapter/CLI contracts.

pub mod adapter_contracts;
pub mod cross_platform;
pub mod data_safety;
pub mod lifecycle;
pub mod profile_mgmt;
pub mod reliability;

use crate::harness::{
    CaseSelectionFilter, PriorityFilter, ReproducedDefectRecord, TestCaseOutcome,
};

/// Runs every enabled test suite matching `suite_filter` and returns all outcomes.
pub fn run_selected_suites(
    suite_filter: &str,
    soak_cycles: usize,
    seed: u64,
) -> Vec<TestCaseOutcome> {
    let filter = CaseSelectionFilter {
        suite: suite_filter.to_owned(),
        priority: PriorityFilter::All,
        case_ids: Vec::new(),
    };
    run_filtered_suites(&filter, soak_cycles, seed)
}

/// Runs only the suites and individual test cases matching [`CaseSelectionFilter`].
pub fn run_filtered_suites(
    filter: &CaseSelectionFilter,
    soak_cycles: usize,
    seed: u64,
) -> Vec<TestCaseOutcome> {
    let suite_norm = filter.suite.trim().to_ascii_lowercase();
    let run_all = suite_norm.is_empty() || suite_norm == "all";
    let opt = Some(filter);
    let mut out = Vec::new();

    if run_all || suite_norm == "lifecycle" {
        out.extend(lifecycle::run_lifecycle_suite_filtered(seed, opt));
    }
    if run_all || suite_norm == "profile" || suite_norm == "profile_management" {
        out.extend(profile_mgmt::run_profile_management_suite_filtered(opt));
    }
    if run_all || suite_norm == "safety" || suite_norm == "data_safety" {
        out.extend(data_safety::run_data_safety_suite_filtered(seed, opt));
    }
    if run_all || suite_norm == "reliability" {
        out.extend(reliability::run_reliability_suite_filtered(
            soak_cycles,
            seed,
            opt,
        ));
    }
    if run_all || suite_norm == "cross-platform" || suite_norm == "cross_platform" {
        out.extend(cross_platform::run_cross_platform_suite_filtered(seed, opt));
    }
    if run_all || suite_norm == "contracts" || suite_norm == "contract" {
        out.extend(adapter_contracts::run_adapter_contract_suite_filtered(opt));
    }

    out
}

/// Aggregates all `FailDefectReproduced` outcomes into a deduplicated defect catalog.
#[must_use]
pub fn collect_reproduced_defects(outcomes: &[TestCaseOutcome]) -> Vec<ReproducedDefectRecord> {
    use std::collections::BTreeMap;

    let mut map: BTreeMap<String, ReproducedDefectRecord> = BTreeMap::new();
    for o in outcomes {
        let Some(def_id) = &o.defect_id else {
            continue;
        };
        let entry = map
            .entry(def_id.clone())
            .or_insert_with(|| ReproducedDefectRecord {
                defect_id: def_id.clone(),
                severity: defect_severity(def_id).to_owned(),
                test_case_ids: Vec::new(),
                summary: defect_summary(def_id).to_owned(),
                affected_files: o.affected_files.clone(),
                minimal_reproduction: o.minimal_reproduction.clone().unwrap_or_default(),
                observed_evidence: o.evidence.clone(),
            });
        if !entry.test_case_ids.contains(&o.id) {
            entry.test_case_ids.push(o.id.clone());
        }
        for f in &o.affected_files {
            if !entry.affected_files.contains(f) {
                entry.affected_files.push(f.clone());
            }
        }
    }

    map.into_values().collect()
}

fn defect_severity(defect_id: &str) -> &'static str {
    match defect_id {
        "DEFECT-01" | "DEFECT-02" | "DEFECT-03" | "DEFECT-04" | "DEFECT-06" => "HIGH",
        _ => "MEDIUM",
    }
}

fn defect_summary(defect_id: &str) -> &'static str {
    match defect_id {
        "DEFECT-01" => "macOS and Linux adapters check `singleton_lock.exists()` instead of `fs::symlink_metadata()`, failing to detect Chromium's dangling `SingletonLock` (`<hostname>-<pid>`) symlinks after a crash.",
        "DEFECT-02" => "macOS and Linux adapters neither read `.runtime-pid` nor inject deterministic fixture `--user-id` nor poll readiness on launch, causing false `running=false` right after spawn and `DUPLICATE_ZALO_USER_ID` collisions on multi-profile `NodeFixture` launches.",
        "DEFECT-03" => "Slow-starting processes (`--slow-startup-pre-lock-ms`) are invisible to `inspect_profile` before `lockfile` creation because `.runtime-pid` is ignored unless `lockfile` is already exclusively locked, leaving orphan background processes when `stop_profile` is called during startup.",
        "DEFECT-04" => "Missing cross-thread/cross-process synchronization in `InstanceCoordinator::launch_profile` and shared `profiles.json.tmp` path in `JsonProfileStore` / `dim-host.ps1` allows concurrent launches to spawn racing processes and concurrent CLI writes to clobber `profiles.json.tmp`.",
        "DEFECT-05" => "`LinuxPlatformAdapter::focus_or_restore` is a silent no-op `Ok(())` and `MacOsPlatformAdapter::focus_or_restore` ignores headless fixtures without writing `.focus-signal`, diverging from Windows focus signaling.",
        "DEFECT-06" => "`WindowsPlatformAdapter::stop` and `inspect_profile` hardcode an executable name allowlist (`zalo*`, `zocr*`, `node.exe`, `dim-test-fixture*`), refusing to terminate hung `CustomExecutable` processes or non-matching helper children even when `RestartManager` proves they hold the profile's `lockfile`; `MacOsPlatformAdapter::stop` lacks `SIGKILL` escalation and child-tree cleanup altogether.",
        "DEFECT-07" => "`FileBackupManager::backup_profile` does not scan `ZaloData` for post-launch `zalo_user_id` logins, and `restore_backup_as_new_profile` hardcodes Windows `AppData/Roaming/ZaloData` when detecting `zalo_user_id` and writing fallback `.zalo-profile-id`, losing account IDs on macOS/Linux restores.",
        "DEFECT-08" => "Contract divergences between Rust (`dim-cli`/`dim-core`) and PowerShell (`scripts/dim-host.ps1`): slug length cap (`40` vs `32`), restore duplicate-name handling (`DUPLICATE_NAME` vs silent `#2` rename that can exceed 64 chars), missing-arg error codes, and empty/corrupt `profiles.json` handling (`JSON_ERROR` fail-closed vs silent empty reset).",
        "DEFECT-09" => "`JsonProfileStore::delete_profile` (trait implementation) hardcodes `is_running = false`, bypassing the running-instance guard if called directly instead of through `InstanceCoordinator::delete_profile`.",
        "DEFECT-10" => "`JsonProfileStore` duplicate-name and lookup checks use `str::eq_ignore_ascii_case`, failing to fold non-ASCII Vietnamese uppercase/lowercase pairs (`Đức` vs `đức`, `Ánh` vs `ánh`), diverging from PowerShell `StringComparison.OrdinalIgnoreCase`.",
        _ => "Reproduced product behavior or cross-platform contract divergence.",
    }
}
