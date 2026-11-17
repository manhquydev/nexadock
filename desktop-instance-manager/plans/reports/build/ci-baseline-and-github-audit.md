# CI Baseline & GitHub Repository / Actions Audit Report

**Mission:** AGENT MISSION E — Multi-Zalo Cross-Platform Build & Distribution  
**Worktree:** `C:/Users/manhquy/Downloads/zalo-cross-platform-build` (`build/cross-platform-distribution`)  
**Baseline Commit:** `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` (`a5feb4a`)  
**Remote `origin/main` Commit:** `8cf47035f0f06bd97cc99aff80d81fb103af8` (`8cf4703`, local `main` is `ahead 1`)  
**Audit Date:** `2026-11-17`  
**Live GitHub Actions Execution Status:** `CI_LIVE_RUN_STATUS: NOT_EXECUTED (STATICALLY_VALIDATED_ONLY)`  
**Static Workflow Validation Status:** `STATICALLY_VALIDATED_PASS` (`validate-workflow.mjs`)

> [!IMPORTANT]
> **Strict Honesty Disclosure:** Live GitHub Actions execution was **NOT** performed (`CI_LIVE_RUN_STATUS: NOT_EXECUTED (STATICALLY_VALIDATED_ONLY)`). Nothing in this repository or report claims `CI PASS` on remote GitHub-hosted runners. Remote execution is intentionally withheld because `manhquydev/zalo` is a **private repository** subject to metered GitHub Actions minute multipliers, baseline commit `a5feb4a` and branch `build/cross-platform-distribution` are unpushed local commits under mission no-push guardrails, and the local GitHub CLI OAuth token lacks `workflow` and `user` scopes.

---

## 1. Git Repository, Commit Lineage & Worktree Isolation Audit

Read-only inspection of the primary workspace and isolated build worktree established the following exact Git topology:

| Property | Observed Value | Audit Verdict |
| :--- | :--- | :--- |
| **Primary Workspace** | `C:/Users/manhquy/Downloads/zalo` | Untouched on `main` (`git status --porcelain` empty) |
| **Isolated Build Worktree** | `C:/Users/manhquy/Downloads/zalo-cross-platform-build` | Created cleanly via `git worktree add -b build/cross-platform-distribution` |
| **Worktree Branch** | `build/cross-platform-distribution` | Base `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` |
| **Remote URL (`origin`)** | `https://github.com/manhquydev/zalo.git` | Read-only inspected; zero pushes performed |
| **`origin/main` HEAD** | `8cf47035f0f06bd97cc99aff80d81fb103af8` (`chore: initial commit`) | Remote default branch lags local `main` by 1 commit |
| **Local `main` HEAD** | `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` (`feat(hardening): build, verify, and harden desktop-instance-manager Rust/Tauri/bridge stack`) | `main...origin/main [ahead 1]` (local-only commit) |

### Why Remote GitHub Actions Shows Zero Registered Workflows Today
1. In `origin/main` (`8cf4703`), the initial CI file lived at `desktop-instance-manager/.github/workflows/ci.yml` — nested inside a subdirectory where GitHub Actions workflow discovery ignores YAML files (GitHub Actions only indexes root `.github/workflows/*.yml` / `*.yaml`).
2. Local commit `a5feb4a` relocated that workflow to root `.github/workflows/ci.yml`, added `Cargo.lock` and `apps/desktop/src-tauri/Cargo.lock`, and hardened the Rust/Tauri workspace.
3. Because `a5feb4a` has not been pushed to `origin/main` (blocked previously by missing OAuth `workflow` scope and held locally during Mission E), GitHub's remote workflow registry for `manhquydev/zalo` currently reports `total_count: 0`.

---

## 2. Existing CI Baseline Audit (`.github/workflows/ci.yml`) & Trigger Non-Conflict

Existing root workflow `.github/workflows/ci.yml` (`name: MultiZaloDesktop CI`, 172 lines, untouched in Mission E) was audited line-by-line to guarantee zero trigger duplication, cache key collision, or job overlap with the new `.github/workflows/cross-platform-validation.yml`.

### Side-by-Side Workflow Separation Matrix

| Dimension | Existing `.github/workflows/ci.yml` | New `.github/workflows/cross-platform-validation.yml` | Conflict Check |
| :--- | :--- | :--- | :--- |
| **Workflow Name** | `MultiZaloDesktop CI` | `Multi-Zalo Cross-Platform Build & Packaging Validation` | Distinct |
| **`push` Trigger** | `branches: [main, master]` | **None** | Zero duplicate push runs |
| **`pull_request` Trigger** | `branches: [main, master]` | **None** | Zero duplicate PR runs |
| **`schedule` (Cron)** | **None** | **None** (prohibited for private-repo cost safety) | Compliant |
| **`workflow_dispatch`** | Bare manual trigger (no inputs) | Parameterized manual trigger (`enable_windows`, `enable_ubuntu`, `enable_macos_arm64`, `enable_macos_intel`, `bundles`) | Operator-controlled only |
| **Permissions** | Default token permissions | Explicit top-level `permissions: contents: read` (strict read-only) | Hardened |
| **Primary Purpose** | Fast source/PR check (`cargo fmt`, `check`, `clippy`, `test`, `cargo-deny`, Windows fixture/parity tests) | Release-mode cross-OS binary compilation, OS bundler attempts (`.msi`, NSIS, `.app`, `.dmg`, `.deb`, `.AppImage`), binary header inspection, and Section 7 SHA-256 provenance manifests | Complementary, non-overlapping |
| **Linux Apt Packages** | Legacy `libappindicator3-dev` on `ubuntu-latest` | Explicit Ubuntu 24.04 LTS (`ubuntu-24.04`) with `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `patchelf`, `file`, `binutils`, `dpkg-dev`, `libfuse2t64` | Fixes Ubuntu 24.04 Ayatana package requirement |
| **CargoDeny Coverage** | `desktop-instance-manager/Cargo.toml` + `src-tauri/Cargo.toml` | `dependency-audit.mjs` (dual `Cargo.lock` + MSRV + zero-VNG scan) + `EmbarkStudios/cargo-deny-action@v2` on both manifests | Aligned |

---

## 3. Read-Only GitHub CLI, Permissions & Billing Inspection

All GitHub inspections were executed strictly read-only via `C:\Program Files\GitHub CLI\gh.exe`:

### 3.1 Authentication & OAuth Token Scopes (`gh auth status`)
- **Account:** `manhquydev` on `github.com` (`Active account: true`, protocol `https`, credential store `keyring`).
- **Granted Token Scopes:** `'gist'`, `'read:org'`, `'repo'`.
- **Missing Scopes Identified:**
  1. **`workflow` scope absent:** GitHub HTTPS push protection rejects creating or updating `.github/workflows/*.yml` from an OAuth token lacking `workflow` scope.
  2. **`user` / `read:user` scope absent:** GitHub REST billing endpoints (`/users/{username}/settings/billing/actions`) return HTTP `404` without `user` scope (`This API operation needs the "user" scope. To request it, run: gh auth refresh -h github.com -s user`).

### 3.2 Repository Visibility & Access (`gh api repos/manhquydev/zalo`)
```json
{
  "default_branch": "main",
  "full_name": "manhquydev/zalo",
  "owner": "manhquydev",
  "permissions": {
    "admin": true,
    "maintain": true,
    "pull": true,
    "push": true,
    "triage": true
  },
  "private": true,
  "visibility": "private"
}
```
- **Critical Finding:** `manhquydev/zalo` is a **PRIVATE** repository (`"private": true`, `"visibility": "private"`). Unlike public repositories (where standard GitHub-hosted runners are free), every minute of GitHub Actions execution in a private repository draws down the account's monthly Actions quota or incurs per-minute billing.

### 3.3 Repository Actions Configuration (`gh api repos/manhquydev/zalo/actions/permissions`)
```json
{
  "enabled": true,
  "allowed_actions": "all",
  "sha_pinning_required": false
}
```
- **Workflows & Historical Runs (`gh api .../actions/workflows` & `gh run list --repo manhquydev/zalo`):**
  - Registered remote workflows: `0` (`{"total_count": 0, "workflows": []}`)
  - Historical workflow runs: `0`

### 3.4 Billing & Quota Telemetry (`gh api users/manhquydev/settings/billing/actions` & `gh api user`)
- `gh api users/manhquydev/settings/billing/actions` -> `HTTP 404 Not Found` (`This API operation needs the "user" scope`).
- `gh api user --jq '{login, plan, owned_private_repos, total_private_repos}'` -> `{"login":"manhquydev","owned_private_repos":null,"plan":null,"total_private_repos":null}` (plan fields redacted by GitHub API when `user` scope is absent).
- Because remaining monthly private-repo minutes cannot be verified programmatically without `user` scope, triggering remote builds without explicit owner approval is treated as an unbudgeted cost risk and strictly avoided.

---

## 4. Private-Repository Runner Minute Multipliers & Cost Governance

GitHub Actions applies OS-specific minute multipliers to all jobs executed on standard GitHub-hosted runners inside **private repositories** (GitHub Free includes 2,000 billable minutes/month; GitHub Pro includes 3,000 billable minutes/month):

| Runner OS Label | Underlying Architecture | GitHub Private-Repo Minute Multiplier | Est. Cold/Cached Wall-Clock Time | Est. Billable Quota Consumed per Run | Default Dispatch Input in `cross-platform-validation.yml` |
| :--- | :--- | :---: | :---: | :---: | :---: |
| `ubuntu-24.04` (`rust-workspace-validation`) | `x86_64` Linux | **1x** | ~6 min | ~6 min | Always runs on manual dispatch |
| `ubuntu-24.04` (`dependency-audit`) | `x86_64` Linux | **1x** | ~3 min | ~3 min | Always runs on manual dispatch |
| `ubuntu-24.04` (`ubuntu-2404-build`) | `x86_64` Linux | **1x** | ~10 min | ~10 min | `enable_ubuntu: true` |
| `windows-latest` (`windows-msvc-build`) | `x86_64` Windows | **2x** | ~12 min | **~24 min** | `enable_windows: true` |
| `macos-14` (`macos-arm64-build`) | `arm64` Apple Silicon | **10x** | ~14 min | **~140 min** | `enable_macos_arm64: false` *(opt-in)* |
| `macos-14` (`macos-intel-build`) | `arm64` host -> `x86_64-apple-darwin` | **10x** | ~15 min | **~150 min** | `enable_macos_intel: false` *(opt-in)* |
| `ubuntu-24.04` (`artifact-verification`) | `x86_64` Linux | **1x** | ~2 min | ~2 min | Always runs (`if: always()`) |

### Cost Protection Controls Implemented
1. **`workflow_dispatch` Only (Zero Automatic Runs):** Neither `push`, `pull_request`, nor `schedule` triggers exist in `cross-platform-validation.yml`. Every single execution requires intentional human initiation.
2. **Opt-In macOS Gates (`default: false`):** Enabling both macOS jobs consumes ~290 billable minutes per run due to the **10x** macOS multiplier (~14.5% of an entire GitHub Free monthly allowance in one click). Defaulting `enable_macos_arm64` and `enable_macos_intel` to `false` keeps standard Windows + Ubuntu packaging validation at ~45 billable minutes (~86% reduction) until macOS packaging specifically needs remote verification.
3. **Deterministic Lockfile Caching (Zero Secrets):** Cargo registry and build caches use `actions/cache@v4` keyed strictly on `hashFiles('desktop-instance-manager/**/Cargo.toml', 'desktop-instance-manager/Cargo.lock', 'desktop-instance-manager/apps/desktop/src-tauri/Cargo.lock')` to cut repeat compile times without ever embedding secrets in cache keys or paths.
4. **Short Artifact Retention (`retention-days: 7`):** Private repositories also have tight GitHub Actions storage quotas (500 MB on GitHub Free). All uploaded validation manifests and optional dev bundles expire automatically after 7 days.

---

## 5. Architectural Note: Reliable `macos-intel-build` (`x86_64-apple-darwin`) Runner Strategy

A common failure mode in cross-platform GitHub Actions workflows is pinning Intel macOS jobs to `runs-on: macos-13`:
- GitHub Actions retired `macos-12` and placed `macos-13` (x86_64 Intel Mac hardware) on its deprecation/retirement schedule, resulting in severe queue delays, brownouts, or job rejection when `macos-13` is unavailable, while `macos-15-intel` / `macos-15-large` labels vary by billing tier and runner pool availability.
- Meanwhile, `macos-14` (macOS Sonoma on Apple Silicon M1) is GA across standard GitHub plans, ships with Xcode's universal `MacOSX.sdk` (containing complete `x86_64` and `arm64` system frameworks, WebKit, Cocoa, and `clang` cross-linkers), and supports `rustup target add x86_64-apple-darwin` out of the box.

**Implemented Design in `macos-intel-build`:**
- Runs on `macos-14` with `dtolnay/rust-toolchain@stable` configured with `targets: x86_64-apple-darwin`.
- Invokes `build-and-package.mjs --target x86_64-apple-darwin` (which automatically detects cross-architecture compilation `process.arch !== 'x64'` and compiles `--tests` cleanly without failing if Rosetta 2 is absent, while building full release `--target x86_64-apple-darwin` Mach-O executables and Tauri `.app`/`.dmg` bundles).
- Explicitly verifies the resulting binary with `lipo -archs` (`x86_64`) and `inspect-artifact.mjs` (`CPU_TYPE_X86_64`, Mach-O 64-bit `0xfeedfacf`) so Intel Mac target fidelity is cryptographically and structurally proven.

---

## 6. Security, Supply-Chain & Honesty Verification Summary

| Control | Requirement | Status in `.github/workflows/cross-platform-validation.yml` |
| :--- | :--- | :--- |
| **Live CI Status Reporting** | Never claim `CI PASS` when unexecuted | `CI_LIVE_RUN_STATUS: NOT_EXECUTED (STATICALLY_VALIDATED_ONLY)` |
| **Workflow Token Permissions** | Read-only repository access | Top-level `permissions: contents: read`; zero `write` scopes |
| **Automatic GitHub Releases** | Strictly forbidden without human gate | Zero release steps (`softprops/action-gh-release`, `gh release`, etc. absent) |
| **Trusted Pinned Official Actions** | Official actions only; no untrusted binaries | `actions/checkout@v4`, `actions/setup-node@v4`, `dtolnay/rust-toolchain@stable`, `actions/cache@v4`, `EmbarkStudios/cargo-deny-action@v2`, `actions/upload-artifact@v4`, `actions/download-artifact@v4` |
| **Secret Caching Prevention** | Never cache secrets or signing certs |Zero `${{ secrets.* }}` references anywhere in the workflow |
| **Zero VNG Binary Policy** | Never download or bundle `Zalo.exe` / `Zalo.app` | Enforced in `dependency-audit.mjs`, `build-and-package.mjs`, and `artifact-verification` |
| **Signing Status Honesty** | Unsigned builds are never called signed/production | Enforced as `UNSIGNED_DEVELOPMENT_ARTIFACT` across all jobs and manifests |

---

## 7. Operator Runbook for Future Authorized Remote Execution

When the repository owner explicitly authorizes pushing to `origin` and spending private-repo Actions minutes:

1. **Grant `workflow` (and optional `user`) OAuth scopes locally:**
   ```powershell
   & "C:\Program Files\GitHub CLI\gh.exe" auth refresh -h github.com -s workflow,user
   ```
2. **Verify remaining monthly private-repo Actions minutes before running:**
   ```powershell
   & "C:\Program Files\GitHub CLI\gh.exe" api users/manhquydev/settings/billing/actions
   ```
3. **Push branch after human review (never auto-pushed by agents):**
   ```powershell
   git -C "C:/Users/manhquy/Downloads/zalo-cross-platform-build" push -u origin build/cross-platform-distribution
   ```
4. **Manually dispatch Windows + Ubuntu validation (low-cost default, macOS off):**
   ```powershell
   & "C:\Program Files\GitHub CLI\gh.exe" workflow run cross-platform-validation.yml `
     --ref build/cross-platform-distribution `
     -f enable_windows=true `
     -f enable_ubuntu=true `
     -f enable_macos_arm64=false `
     -f enable_macos_intel=false `
     -f bundles=all
   ```
5. **Opt into macOS `arm64` / `x86_64` validation only when 10x minute budget is approved:**
   ```powershell
   & "C:\Program Files\GitHub CLI\gh.exe" workflow run cross-platform-validation.yml `
     --ref build/cross-platform-distribution `
     -f enable_windows=true `
     -f enable_ubuntu=true `
     -f enable_macos_arm64=true `
     -f enable_macos_intel=true `
     -f bundles=all
   ```
