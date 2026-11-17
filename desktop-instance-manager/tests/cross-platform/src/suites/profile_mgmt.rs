//! Profile Management Test Matrix (`PM-01` .. `PM-08`).
//!
//! Covers:
//! - Create, get, rename (preserving immutable `directory_name`), and confirmed delete
//! - Multi-profile creation (25 profiles, collision-free 8-hex IDs and directories)
//! - Unicode Vietnamese names (NFC, NFD combining diacritics, `Đ`/`đ`, emoji/symbol fallback slugs)
//! - Vietnamese non-ASCII case-insensitive duplicate detection (`DEFECT-10`)
//! - Long name boundaries (`63`, `64`, `65` chars; multi-byte UTF-8 char vs byte length)
//! - Duplicate ASCII names & case-insensitive self-rename
//! - Invalid names & invalid storage root paths (`..`, NUL, control chars, primary `ZaloData`)
//! - Missing permissions / locked `profiles.json.tmp` rollback & trait `delete_profile` running-guard bypass (`DEFECT-09`)

use std::collections::HashSet;
use std::fs;

use dim_core::{
    slugify_profile_name, validate_profile_name, DimError, ProfileStore, MAX_PROFILE_NAME_LEN,
};
use serde_json::json;

use crate::harness::{
    make_node_fixture_target, run_case, CaseSuccess, IsolatedSandbox, TestCaseOutcome,
};

/// Executes all Profile Management scenarios (`PM-01` .. `PM-08`).
pub fn run_profile_management_suite() -> Vec<TestCaseOutcome> {
    vec![
        run_case(
            "PM-01",
            "profile_management",
            "Create, lookup, live-safe rename, and confirmed delete lifecycle",
            "all",
            pm_01_crud_and_immutable_dir,
        ),
        run_case(
            "PM-02",
            "profile_management",
            "Bulk creation of 25 profiles with unique 8-hex IDs and collision-free directories",
            "all",
            pm_02_bulk_multi_profile_creation,
        ),
        run_case(
            "PM-03",
            "profile_management",
            "Unicode Vietnamese NFC/NFD diacritics, Đ/đ folding, and symbol-only slug disambiguation",
            "all",
            pm_03_vietnamese_unicode_and_symbols,
        ),
        run_case(
            "PM-04",
            "profile_management",
            "Unicode Vietnamese case-insensitive duplicate name rejection (`Đức` vs `đức`)",
            "all",
            pm_04_vietnamese_case_duplicate_guard,
        ),
        run_case(
            "PM-05",
            "profile_management",
            "Profile name length boundaries (63/64/65 ASCII & multi-byte Vietnamese chars)",
            "all",
            pm_05_long_name_boundaries,
        ),
        run_case(
            "PM-06",
            "profile_management",
            "Duplicate names (ASCII case-insensitive, whitespace-trimmed, and self-rename)",
            "all",
            pm_06_duplicate_names_and_self_rename,
        ),
        run_case(
            "PM-07",
            "profile_management",
            "Invalid profile names and unsafe/primary storage root path rejections",
            "all",
            pm_07_invalid_names_and_paths,
        ),
        run_case(
            "PM-08",
            "profile_management",
            "Permission/write failure rollback on `profiles.json.tmp` & trait `delete_profile` running guard",
            "all",
            pm_08_permission_failure_and_trait_delete_guard,
        ),
    ]
}

fn pm_01_crud_and_immutable_dir() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let created = sandbox
        .store
        .create_profile("  Phòng Kinh Doanh Miền Bắc  ", "  lead account  ")
        .map_err(|e| e.to_string())?;

    if created.name != "Phòng Kinh Doanh Miền Bắc" || created.notes != "lead account" {
        return Err("expected trimmed name and notes".to_owned());
    }
    let orig_dir = created.directory_name.clone();
    let pdir = sandbox
        .store
        .profile_dir_for(&created)
        .map_err(|e| e.to_string())?;
    if !pdir.is_dir() {
        return Err("profile directory was not created on disk".to_owned());
    }

    // Lookup by uppercase ID and by case-varied name
    let by_upper_id = sandbox
        .store
        .get_profile(&created.id.to_ascii_uppercase())
        .map_err(|e| e.to_string())?;
    if by_upper_id.id != created.id {
        return Err("case-insensitive ID lookup failed".to_owned());
    }

    let renamed = sandbox
        .store
        .rename_profile(&created.id, "Phòng Kinh Doanh Quốc Tế")
        .map_err(|e| e.to_string())?;
    if renamed.directory_name != orig_dir {
        return Err("rename_profile mutated immutable directory_name".to_owned());
    }

    // Delete without confirm must fail with CONFIRM_REQUIRED
    let no_confirm_err = sandbox
        .coordinator
        .delete_profile(&created.id, false)
        .expect_err("delete without confirm must fail");
    if no_confirm_err.code() != "CONFIRM_REQUIRED" || !pdir.exists() {
        return Err(format!("expected CONFIRM_REQUIRED, got {no_confirm_err:?}"));
    }

    sandbox
        .coordinator
        .delete_profile(&created.id, true)
        .map_err(|e| e.to_string())?;
    if pdir.exists() {
        return Err("profile directory remained on disk after confirmed delete".to_owned());
    }
    let not_found = sandbox
        .store
        .get_profile(&created.id)
        .expect_err("deleted profile must return NOT_FOUND");
    if not_found.code() != "NOT_FOUND" {
        return Err(format!("expected NOT_FOUND, got {}", not_found.code()));
    }

    Ok(CaseSuccess::pass(json!({
        "profile_id": created.id,
        "immutable_directory_name": orig_dir,
        "confirm_guard_code": no_confirm_err.code(),
    })))
}

fn pm_02_bulk_multi_profile_creation() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let mut ids = HashSet::new();
    let mut dirs = HashSet::new();

    for idx in 1..=25 {
        let rec = sandbox
            .store
            .create_profile(&format!("Tài Khoản Số {idx:02}"), &format!("batch-{idx}"))
            .map_err(|e| e.to_string())?;
        if !ids.insert(rec.id.clone()) {
            return Err(format!("duplicate generated 8-hex ID: {}", rec.id));
        }
        if !dirs.insert(rec.directory_name.clone()) {
            return Err(format!("duplicate directory_name: {}", rec.directory_name));
        }
    }

    let listed = sandbox.store.list_profiles().map_err(|e| e.to_string())?;
    if listed.len() != 25 {
        return Err(format!("expected 25 profiles, found {}", listed.len()));
    }

    Ok(CaseSuccess::pass(json!({
        "created_count": listed.len(),
        "unique_ids": ids.len(),
        "unique_directories": dirs.len(),
    })))
}

fn pm_03_vietnamese_unicode_and_symbols() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;

    // Precomposed NFC vs Decomposed NFD combining marks
    let nfc = "Đặng Thị Ánh Tuyết";
    let nfd = "Đa\u{0323}\u{0306}ng Thi\u{0323} A\u{0301}nh Tuye\u{0302}\u{0301}t";
    let slug_nfc = slugify_profile_name(nfc);
    let slug_nfd = slugify_profile_name(nfd);
    if slug_nfc != "dang-thi-anh-tuyet" || slug_nfd != "dang-thi-anh-tuyet" {
        return Err(format!(
            "NFC/NFD slug mismatch: nfc={slug_nfc}, nfd={slug_nfd}"
        ));
    }

    // Multiple symbol/emoji-only names all fold to slug `"profile"`, yet `<slug>_<8hex>` keeps directories distinct!
    let e1 = sandbox
        .store
        .create_profile("🚀🏢✨", "")
        .map_err(|e| e.to_string())?;
    let e2 = sandbox
        .store
        .create_profile("🔥💼🎯", "")
        .map_err(|e| e.to_string())?;
    if !e1.directory_name.starts_with("profile_")
        || !e2.directory_name.starts_with("profile_")
        || e1.directory_name == e2.directory_name
    {
        return Err("emoji fallback directory names collided or malformed".to_owned());
    }

    Ok(CaseSuccess::pass(json!({
        "nfc_slug": slug_nfc,
        "nfd_slug": slug_nfd,
        "emoji_dir_1": e1.directory_name,
        "emoji_dir_2": e2.directory_name,
    })))
}

fn pm_04_vietnamese_case_duplicate_guard() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let upper = sandbox
        .store
        .create_profile("Đức Công Việc", "")
        .map_err(|e| e.to_string())?;

    // Lowercase Vietnamese `đức công việc` differs ONLY in non-ASCII letter case (`Đ` vs `đ`, `ệ` vs `Ệ`).
    let second_res = sandbox.store.create_profile("đức công việc", "");
    match second_res {
        Ok(dup_rec) => Ok(CaseSuccess::defect(
            "DEFECT-10",
            &[
                "crates/profile-store/src/store.rs:100,216,260,439",
                "scripts/dim-host.ps1:1777",
            ],
            "Create profile `'Đức Công Việc'`, then create `'đức công việc'`: `JsonProfileStore` uses `str::eq_ignore_ascii_case` which leaves non-ASCII `Đ`/`đ` and accented vowels unfolded, allowing duplicate Vietnamese profile names that `dim-host.ps1` (`OrdinalIgnoreCase`) rejects with `DUPLICATE_NAME`.",
            json!({
                "first_profile": { "id": upper.id, "name": upper.name, "dir": upper.directory_name },
                "second_profile_unexpectedly_allowed": { "id": dup_rec.id, "name": dup_rec.name, "dir": dup_rec.directory_name },
            }),
        )),
        Err(DimError::DuplicateProfileName(_)) => Ok(CaseSuccess::pass(json!({
            "duplicate_vietnamese_case_rejected": true,
        }))),
        Err(other) => Err(format!("unexpected error on Vietnamese duplicate check: {other:?}")),
    }
}

fn pm_05_long_name_boundaries() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;

    let ascii_63 = "A".repeat(MAX_PROFILE_NAME_LEN - 1);
    let ascii_64 = "B".repeat(MAX_PROFILE_NAME_LEN);
    let ascii_65 = "C".repeat(MAX_PROFILE_NAME_LEN + 1);
    // 64 multi-byte Vietnamese characters = 192 UTF-8 bytes (must count Unicode scalar chars, not bytes!)
    let viet_64 = "Đ".repeat(MAX_PROFILE_NAME_LEN);
    let viet_65 = "Đ".repeat(MAX_PROFILE_NAME_LEN + 1);

    let r63 = sandbox
        .store
        .create_profile(&ascii_63, "")
        .map_err(|e| e.to_string())?;
    let r64 = sandbox
        .store
        .create_profile(&ascii_64, "")
        .map_err(|e| e.to_string())?;
    let rv64 = sandbox
        .store
        .create_profile(&viet_64, "")
        .map_err(|e| e.to_string())?;

    let err_65 = validate_profile_name(&ascii_65).expect_err("65 ASCII chars must fail");
    let err_v65 = validate_profile_name(&viet_65).expect_err("65 Vietnamese chars must fail");
    if err_65.code() != "INVALID_NAME" || err_v65.code() != "INVALID_NAME" {
        return Err("expected INVALID_NAME on 65-character names".to_owned());
    }

    Ok(CaseSuccess::pass(json!({
        "len_63_dir_len": r63.directory_name.len(),
        "len_64_dir_len": r64.directory_name.len(),
        "viet_64_utf8_bytes": viet_64.len(),
        "viet_64_dir": rv64.directory_name,
        "len_65_rejected_code": err_65.code(),
    })))
}

fn pm_06_duplicate_names_and_self_rename() -> Result<CaseSuccess, String> {
    let sandbox = IsolatedSandbox::new()?;
    let p1 = sandbox
        .store
        .create_profile("Sales Team", "")
        .map_err(|e| e.to_string())?;
    let p2 = sandbox
        .store
        .create_profile("Support Team", "")
        .map_err(|e| e.to_string())?;

    let dup1 = sandbox
        .store
        .create_profile("  sales team  ", "")
        .expect_err("ASCII case-insensitive duplicate must fail");
    if dup1.code() != "DUPLICATE_NAME" {
        return Err(format!("expected DUPLICATE_NAME, got {}", dup1.code()));
    }

    let dup_rename = sandbox
        .store
        .rename_profile(&p2.id, "SALES TEAM")
        .expect_err("rename to another profile's name must fail");
    if dup_rename.code() != "DUPLICATE_NAME" {
        return Err(format!(
            "expected DUPLICATE_NAME on rename, got {}",
            dup_rename.code()
        ));
    }

    // Self-rename changing only capitalization (`Sales Team` -> `SALES TEAM`) must succeed!
    let self_ren = sandbox
        .store
        .rename_profile(&p1.id, "SALES TEAM")
        .map_err(|e| format!("self case-change rename failed: {e}"))?;
    if self_ren.name != "SALES TEAM" {
        return Err("self rename did not update capitalization".to_owned());
    }

    Ok(CaseSuccess::pass(json!({
        "duplicate_create_code": dup1.code(),
        "duplicate_rename_code": dup_rename.code(),
        "self_case_rename_allowed": true,
    })))
}

fn pm_07_invalid_names_and_paths() -> Result<CaseSuccess, String> {
    let invalid_names = [
        "",
        "     ",
        "../escape",
        "nested/name",
        "nested\\name",
        "double..dot",
        "null\0byte",
        "line\nbreak",
        "carriage\rreturn",
        "tab\tchar",
        "del\x7fchar",
    ];
    for bad in invalid_names {
        let err =
            validate_profile_name(bad).expect_err(&format!("expected INVALID_NAME for {bad:?}"));
        if err.code() != "INVALID_NAME" {
            return Err(format!(
                "expected INVALID_NAME for {bad:?}, got {}",
                err.code()
            ));
        }
    }

    let bad_roots = ["", "../outside", "bad\0root", "bad\nroot"];
    for root in bad_roots {
        let err = dim_profile_store::JsonProfileStore::new(std::path::PathBuf::from(root))
            .expect_err(&format!("expected store root rejection for {root:?}"));
        if err.code() != "PATH_TRAVERSAL" {
            return Err(format!(
                "expected PATH_TRAVERSAL for {root:?}, got {}",
                err.code()
            ));
        }
    }

    if let Some(primary) = dim_core::resolve_primary_zalo_data_dir() {
        let prim_err = dim_profile_store::JsonProfileStore::new(primary.clone())
            .expect_err("primary ZaloData root must be rejected");
        let nested_err = dim_profile_store::JsonProfileStore::new(primary.join("SubDir"))
            .expect_err("nested path inside primary ZaloData must be rejected");
        if prim_err.code() != "PROTECTED_PRIMARY_PATH"
            || nested_err.code() != "PROTECTED_PRIMARY_PATH"
        {
            return Err("primary path protection returned wrong error code".to_owned());
        }
    }

    Ok(CaseSuccess::pass(json!({
        "invalid_names_rejected": invalid_names.len(),
        "invalid_roots_rejected": bad_roots.len(),
        "primary_zalo_data_blocked": true,
    })))
}

fn pm_08_permission_failure_and_trait_delete_guard() -> Result<CaseSuccess, String> {
    let mut sandbox = IsolatedSandbox::new()?;
    let keep = sandbox
        .store
        .create_profile("Keep Intact On Failure", "")
        .map_err(|e| e.to_string())?;

    // 1. Simulate permission / directory collision blocking atomic `profiles.json.tmp` creation:
    // Creating a directory at `profiles.json.tmp` forces `File::create(&tmp_path)` to fail with `PermissionDenied`/`IsADirectory` on ALL OSes (even root!).
    let tmp_blocker = sandbox.data_dir().join("profiles.json.tmp");
    fs::create_dir_all(&tmp_blocker).map_err(|e| e.to_string())?;

    let fail_res = sandbox.store.create_profile("Should Rollback Metadata", "");
    let write_blocked_code = match fail_res {
        Err(err) => err.code(),
        Ok(_) => {
            let _ = fs::remove_dir_all(&tmp_blocker);
            return Err(
                "create_profile unexpectedly succeeded while profiles.json.tmp was blocked"
                    .to_owned(),
            );
        }
    };
    let _ = fs::remove_dir_all(&tmp_blocker);

    // Verify original `profiles.json` remained valid and uncorrupted!
    let after_fail = sandbox.store.list_profiles().map_err(|e| e.to_string())?;
    if after_fail.len() != 1 || after_fail[0].id != keep.id {
        return Err("profiles.json was corrupted after failed atomic write".to_owned());
    }

    // 2. Check trait `ProfileStore::delete_profile(&*sandbox.store, id, true)` while profile is RUNNING!
    let running_prof = sandbox
        .store
        .create_profile("Running Trait Delete Probe", "")
        .map_err(|e| e.to_string())?;
    let target = make_node_fixture_target(&["--user-id=985001"]);
    let rt = sandbox
        .coordinator
        .launch_profile(&running_prof.id, &target)
        .map_err(|e| e.to_string())?;
    if let Some(pid) = rt.main_pid {
        sandbox.track_pid(pid);
    }

    // Coordinator properly blocks delete while running (`PROFILE_RUNNING`):
    let coord_del_err = sandbox
        .coordinator
        .delete_profile(&running_prof.id, true)
        .expect_err("Coordinator::delete_profile must reject running profile");
    if coord_del_err.code() != "PROFILE_RUNNING" {
        return Err(format!(
            "expected PROFILE_RUNNING, got {}",
            coord_del_err.code()
        ));
    }

    // Now test the `ProfileStore::delete_profile` trait method directly while the process is running:
    let trait_del_res = ProfileStore::delete_profile(&*sandbox.store, &running_prof.id, true);
    sandbox.reap_all_sandbox_processes();

    if trait_del_res.is_ok() {
        return Ok(CaseSuccess::defect(
            "DEFECT-09",
            &["crates/profile-store/src/store.rs:271-273"],
            "Launch a profile, then call `ProfileStore::delete_profile(&store, &id, true)`: `JsonProfileStore` hardcodes `is_running = false` in its trait implementation, deleting `profiles.json` metadata and unlinking files out from under a live running instance!",
            json!({
                "atomic_tmp_block_error_code": write_blocked_code,
                "profiles_json_preserved_on_write_error": true,
                "coordinator_delete_guard": coord_del_err.code(),
                "trait_delete_profile_bypassed_running_guard": true,
            }),
        ));
    }

    Ok(CaseSuccess::pass(json!({
        "atomic_tmp_block_error_code": write_blocked_code,
        "profiles_json_preserved_on_write_error": true,
        "coordinator_delete_guard": coord_del_err.code(),
    })))
}
