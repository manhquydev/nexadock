# ADR-004: Atomic Backup, Non-Destructive Restore & Data Safety Guards

**Status:** Accepted  
**Date:** 2026-11-17  
**Related:** [repository-comparison.md](../research/repository-comparison.md) | [ADR-001](./adr-001-hybrid-isolation-architecture.md) | [ADR-003](./adr-003-process-ownership-and-graceful-shutdown.md)

---

## 1. Context & Data-Loss Threat Vectors

Desktop messaging profiles contain irreplaceable local SQLite message histories (`ftsm_*.db`, `Database/_production/<uid>`), DPAPI-encrypted session partitions (`Partitions/zalo`), and user attachments. Six specific data-safety failure modes were identified during research:
1. **Hot-Copy Corruption:** Copying a profile while `Zalo.exe` or `ZaloCall.exe` holds active SQLite WAL/LevelDB locks yields inconsistent snapshots.
2. **Partial / Truncated Backups & Restores:** Crashing mid-copy leaves incomplete directories that look valid unless staged atomically and cryptographically verified.
3. **Destructive Overwrite on Restore:** Restoring a backup over an existing profile or over primary `%APPDATA%\ZaloData` risks destroying newer chats or creating dual active sessions for the same Zalo account.
4. **Duplicate Account Session Conflict:** Launching two profiles logged into the same Zalo user ID (`zalo_user_id`) triggers server session invalidation or local storage conflicts.
5. **Junction / Symlink Traversal During Delete:** Deleting a profile that contains NTFS directory junctions (`ReparsePoint`) or POSIX symlinks pointing to real `Documents`, `Desktop`, or `Library/Keychains` via naive recursive delete could traverse into and delete the user's real personal files (`ducngfi` highlighted this exact mitigation via `RemoveLinks`).
6. **Path Traversal & Disk Exhaustion:** Unsanitized profile names/IDs (`../escape`) or copying multi-gigabyte `ZaloData` folders onto nearly full drives risks arbitrary deletion or system instability.

---

## 2. Weighted Decision Matrix (Backup, Restore & Storage Safety)

Weights: Zero Silent Corruption / Integrity (`30%`), Non-Destructive Restore (`25%`), Deletion & Traversal Safety (`25%`), Duplicate Account Conflict Prevention (`10%`), Simplicity & Auditability (`10%`).

| Option | Integrity (30%) | Non-Destructive Restore (25%) | Safe Delete / Traversal (25%) | Duplicate Account Guard (10%) | Auditability (10%) | Weighted Score & Verdict |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **A. Direct `xcopy` / Zip In-Place Overwrite** | 1/5 (0.30) | 0/5 (0.00) | 1/5 (0.25) | 0/5 (0.00) | 3/5 (0.30) | **0.85 / 5.00** (Rejected: unsafe) |
| **B. File-Count + Byte-Size Manifest Only (`ducngfi` pattern)** | 3.5/5 (1.05) | 5/5 (1.25) | 5/5 (1.25) | 5/5 (0.50) | 4/5 (0.40) | **4.45 / 5.00** (Strong baseline; lacks cryptographic hashes) |
| **C. Hot-Backup via Volume Shadow Copy (VSS)** | 4/5 (1.20) | 4/5 (1.00) | 3/5 (0.75) | 3/5 (0.30) | 1/5 (0.10) — requires Admin | **3.35 / 5.00** (Rejected: violates `asInvoker`) |
| **D. Offline Atomic `.partial` -> `backup.json` (SHA-256) + Restore-to-New-Profile + Reparse Unlink + Guards** (**Selected**) | **5/5 (1.50)** | **5/5 (1.25)** | **5/5 (1.25)** | **5/5 (0.50)** | **5/5 (0.50)** | **5.00 / 5.00 (Selected)** |

---

## 3. Decision & Mandatory Data-Safety Invariants

### 3.1. Offline-Only Atomic Backup (`BackupManager::backup_profile`)
1. **Running Guard:** Verify the target profile has zero active locking PIDs (`is_running == false`); otherwise refuse immediately (`DimError::ProfileRunning`).
2. **Free Disk Guard (`5 GiB`):** Verify `free_bytes >= payload_estimate + MIN_FREE_DISK_BYTES` (`5 GiB`).
3. **Atomic `.partial` Staging & SHA-256 `backup.json`:**
   - Stage all regular profile files inside `<backups_root>/<slug>_<timestamp>.partial/` (skipping transient runtime locks/symlinks safely without following reparse points).
   - Compute per-file **SHA-256** digest (`sha2 = "0.10"` in Rust; `Get-FileHash -Algorithm SHA256` in PowerShell), total `file_count`, and total `byte_count`.
   - Write `backup.json` (`schema_version = 1`) inside `.partial/`.
   - Atomically rename `.partial/` -> final backup folder `<slug>_<timestamp>/` only after every file and `backup.json` succeed.
4. **Crash Cleanup:** Sweep abandoned `.partial` and `.restore-*` staging directories older than 12 hours cleanly.

### 3.2. Non-Destructive Restore-to-New-Profile-Only (`BackupManager::restore_backup_as_new_profile`)
1. **Manifest & Cryptographic Verification:** Read `backup.json`, reject missing/malformed manifests, and verify every file's existence, byte size, and SHA-256 digest before promoting.
2. **New Profile Allocation:** Always generate a brand-new 8-character hex `id`, sanitized `<slug>_<8hex>` `directory_name`, and `.zalo-profile-id` marker. Restore **never** overwrites an existing managed profile and **never** writes into primary `%APPDATA%\ZaloData`.
3. **Atomic Staging:** Copy into `<profiles_root>/.restore-<timestamp>-<uuid>/`, verify file counts/bytes/hashes, move atomically to `<profiles_root>/<slug>_<8hex>/`, and append the new `ProfileRecord` atomically via `profiles.json.tmp` -> replace.

### 3.3. Duplicate Logged-In Account Guard (`Database/_production/<6+ digits>`)
- Inspect `<profile_zalo_data>/config.json` (`db_path.location`) if externalized, then enumerate `<messages_root>/Database/_production/` for numeric subdirectories of length `>= 6` (`zalo_identity.rs`), selecting the most recently modified account ID as `zalo_user_id`.
- Before launching any profile, verify its detected `zalo_user_id` is **not** currently held by any running managed profile **or** by the user's primary `%APPDATA%\ZaloData` session (`ducngfi` best practice). Refuse simultaneous dual launch of the same account.

### 3.4. Safe Deletion, Junction Unlinking & Input Path Guards
- **Junction / Symlink Unlinking Before Delete:** `unlink_profile_junctions` walks the profile directory **without following reparse points** (`FileAttributes.ReparsePoint` on Windows; `symlink_metadata().file_type().is_symlink()` in Rust), removing every junction/symlink (`Directory.Delete(child, false)` / `remove_dir`) **before** deleting profile contents.
- **Confirmation Gate:** `delete_profile` requires explicit `confirm == true` and `is_running == false`.
- **Path Traversal Guard (`resolve_safe_child`):** Rejects `..`, NUL bytes, empty names, absolute roots, drive prefixes, and non-canonical prefixes in both Rust (`crates/core/src/path_guard.rs`) and PowerShell (`scripts/dim-host.ps1`).
- **Vietnamese Slug Normalization (`slugify_profile_name`):** Maps `đ` -> `d`, `Đ` -> `D`, strips Unicode combining diacritics, and produces ASCII `[a-z0-9-]` slugs paired with `<8hex>` (`Đức - Công việc` -> `duc-cong-viec_<8hex>`).
