# CI Baseline, Runner Architecture & GitHub Blocker / Billing Audit Report

**Mission:** AGENT MISSION E / E2 — Multi-Zalo Cross-Platform Build & Distribution  
**Worktree:** `<WORKTREE_ROOT>/zalo-cross-platform-build` (`build/cross-platform-distribution`)  
**Baseline `main` Commit:** `a5feb4a520798064c1508c26fe4c4ac17a3a03cf` (`a5feb4a`)  
**Remote `origin/main` Commit:** `8cf47035f0f06bd97cc99aff80d81fb10b103af8` (`8cf4703`, local `main` is `ahead 1`)  
**Workflow File:** [`.github/workflows/cross-platform-validation.yml`](../../../../.github/workflows/cross-platform-validation.yml)  
**Workflow URL (once registered on `origin/main`):** `https://github.com/manhquydev/zalo/actions/workflows/cross-platform-validation.yml`  
**Audit Date:** `2026-11-18`  
**Live GitHub Actions Execution Status:** `BLOCKED (REMOTE_WORKFLOW_DISPATCH_UNREGISTERED_AND_MISSING_WORKFLOW_SCOPE)` — `CI_LIVE_RUN_STATUS: NOT_EXECUTED (STATICALLY_VALIDATED_ONLY)`  
**Static Workflow Validation Status:** `PASS` (`6/6` core invariants in `validate-workflow.mjs`, `exit 0`)

> [!IMPORTANT]
> **Strict Honesty Disclosure:** Remote GitHub Actions runs have **NOT** been executed (`0` workflow runs on `manhquydev/zalo`; never claim `CI PASS`). Remote execution is concretely **BLOCKED** by three empirically verified facts: (1) local HTTPS Git pushes use `gh.exe auth git-credential` whose OAuth token scopes (`gist, read:org, repo`) lack `workflow` scope; (2) `origin/main` is at `8cf4703` with `0` registered root workflows, and GitHub requires `workflow_dispatch` YAML files to exist on the default branch (`main`) before manual dispatch works; and (3) `manhquydev/zalo` is a private repository subject to metered per-minute Actions billing where unauthorized pushes/dispatches are prohibited.

---

## 1. Verified GitHub-Hosted Runner Labels & P0 macOS Architecture Remediation

Runner labels and hardware specifications were verified against official GitHub Actions documentation (`docs.github.com/en/actions/using-github-hosted-runners/using-github-hosted-runners/about-github-hosted-runners`):

| Priority | Job ID | Verified GitHub Runner Label | Native Hardware & vCPU / RAM | Expected `uname -m` / Host Arch | Expected `rustc -vV` `host:` Triple | Lifecycle & Plan Availability Notes |
| :---: | :--- | :--- | :--- | :--- | :--- | :--- |
| **1** | `ubuntu-2404-build` | `ubuntu-24.04` | 2-core x86_64, 7 GB RAM, 14 GB SSD | `x86_64` | `x86_64-unknown-linux-gnu` | GA standard runner on Free/Pro/Team/Enterprise |
| **2** | `windows-msvc-build` | `windows-2022` | 4-core x86_64, 16 GB RAM, 14 GB SSD (VS 2022 MSVC, WiX 3.14, NSIS 3.x) | `AMD64` (`x86_64`) | `x86_64-pc-windows-msvc` | GA standard runner on Free/Pro/Team/Enterprise |
| **3** | `macos-arm64-build` | `macos-15` | 3-core Apple M1 (`arm64`), 7 GB RAM, 14 GB SSD | `arm64` | `aarch64-apple-darwin` | GA standard Apple Silicon runner (`macos-latest` family); opt-in `false` (`$0.062/min`, `10x`) |
| **4** | `macos-intel-build` | `macos-15-intel` | 4-core Intel (`x86_64`), 14 GB RAM, 14 GB SSD | `x86_64` | `x86_64-apple-darwin` | Official standard Intel x64 runner replacing **retired `macos-13`** (Intel macOS runners sunset Fall 2027); opt-in `false` (`$0.062/min`, `10x`) |

### P0 Remediation: Removal of `macos-14` Cross-Compile for Intel macOS
- **Defect Fixed from E1:** E1 ran `macos-intel-build` on `macos-14` (an Apple Silicon `arm64` runner) and cross-compiled `--target x86_64-apple-darwin`. Cross-compilation on `arm64` cannot execute native Intel x86_64 tests/fixtures without binary translation and must **never** be conflated with native Intel hardware verification.
- **Mission E2 Fix:** `macos-intel-build` now targets `runs-on: macos-15-intel` exclusively and executes a fail-closed hardware gate (`[ "$(uname -m)" = "x86_64" ]` and `rustc -vV | grep '^host: x86_64-apple-darwin$'`).
- **Fail-Closed `BLOCKED` Rule:** If `macos-15-intel` is unavailable on the repository owner's account tier or experiences Intel capacity sunset/queue rejection, `macos-intel-build` must be recorded honestly as **`BLOCKED`** — never silently switched back to an `arm64` cross-compile.

### Native 7-Stage Pipeline Enforced Across Every OS Build Job
Every OS build job (`ubuntu-2404-build`, `windows-msvc-build`, `macos-arm64-build`, `macos-intel-build`) executes all 7 native stages with `--locked` Cargo reproducibility, pinned `@tauri-apps/cli@2.12.5` (verified via `npm.cmd view @tauri-apps/cli version`, never floating `@latest`), and explicit job `timeout-minutes` (`10–30 min`):
1. **Compile (`--locked`):** Workspace release binaries (`dim-cli`, `dim-test-fixture`) + Tauri desktop shell (`dim-desktop`).
2. **Test (`--locked`):** Native workspace unit/integration tests + Tauri crate tests on real target CPU.
3. **Clippy (`--locked -- -D warnings`):** Workspace `--all-targets` + `apps/desktop/src-tauri/Cargo.toml` lint enforcement.
4. **Native Runtime Fixture:** Executes `node desktop-instance-manager/fixtures/test-desktop-app/fixture-app.mjs --stay-ms=100` + native `dim-cli status --json` smoke test (plus headless `xvfb-run -a` GUI dynamic-linker/WebKitGTK startup verification on `ubuntu-24.04`).
5. **Tauri Build:** Pinned `@tauri-apps/cli@2.12.5` native build.
6. **OS Bundle Packaging:** Produces `.deb`/`.AppImage` (Ubuntu 24.04), `.msi`/NSIS `-setup.exe` (Windows MSVC), and `.app`/`.dmg` (macOS arm64 & Intel) using `scripts/ci-cross-platform/tauri-ci-bundle-overlay.json` (`--config`) so shared `tauri.conf.json` remains untouched.
7. **Artifact Verification & Section 7 Manifest:** Runs `inspect-artifact.mjs` + `generate-artifact-manifest.mjs`, consolidated by downstream `artifact-verification` (`if: always()`).

---

## 2. Existing CI Baseline Audit (`.github/workflows/ci.yml`) & Trigger Non-Conflict

Existing root workflow [`.github/workflows/ci.yml`](../../../../.github/workflows/ci.yml) (`name: MultiZaloDesktop CI`, untouched) was verified for zero trigger, permission, or cache collision:

| Dimension | Existing `.github/workflows/ci.yml` | `.github/workflows/cross-platform-validation.yml` | Non-Conflict Verdict |
| :--- | :--- | :--- | :--- |
| **Triggers (`on:`)** | `push` (`main`, `master`), `pull_request` (`main`, `master`), bare `workflow_dispatch` | **`workflow_dispatch` ONLY** (5 explicit inputs; **zero** `push`, `pull_request`, or `schedule`) | Zero duplicate push/PR runs; zero cron drain |
| **Permissions** | Default repository token | Explicit `permissions: contents: read` (zero `write` scopes, zero releases, zero secrets) | Least-privilege read-only |
| **Supply-Chain Scan** | `EmbarkStudios/cargo-deny-action@v2` (`check`) | `EmbarkStudios/cargo-deny-action@v2` (`arguments: --config <sanitized-deny.toml> --all-features --locked`, `command-arguments: -W wildcard advisories bans licenses sources` on both lockfiles, empirically verified against `cargo-deny 0.21.0` + `rustsec/advisory-db @ 233155b4`) + 7-pillar `dependency-audit.mjs` | Full RustSec advisory coverage (`0` vulnerabilities across `439` crates) |
| **Cache Keys** | `${{ runner.os }}-cargo-<hash>` | `${{ runner.os }}-{xplat-check,ubuntu2404-gnu,msvc,aarch64,intel-x86_64}-<dual-lockfile-hash>` | Zero cache key collision |

---

## 3. Empirical Proof of GitHub `workflow_dispatch` & OAuth Credential Blockers

All checks below were executed strictly read-only on `2026-11-18` (zero pushes, zero force-pushes, zero permission mutations, zero PATs created):

### 3.1 Remote Ref State (`git ls-remote origin`)
```text
8cf47035f0f06bd97cc99aff80d81fb10b103af8	HEAD
0f873024f4ee16810b9d35cf206803f793fc55cb	refs/heads/feat/ui-vi-en-quiet-workspace
8cf47035f0f06bd97cc99aff80d81fb10b103af8	refs/heads/main
```
- **Proof:** `origin/main` is still at initial commit `8cf4703` (where `ci.yml` sat inside nested `desktop-instance-manager/.github/workflows/ci.yml`, which GitHub Actions ignores). Local `main` (`a5feb4a`) and local branch `build/cross-platform-distribution` have not been pushed to `origin`.
- `gh api repos/manhquydev/zalo/actions/workflows` -> `{"total_count": 0, "workflows": []}` (`0` registered workflows, `0` historical runs).

### 3.2 Git HTTPS Credential Helper Chain (`git config --show-origin --get-regexp credential`)
```text
file:<USERPROFILE>/AppData/Local/Microsoft/WinGet/Packages/Git.MinGit_Microsoft.Winget.Source_8wekyb3d8bbwe/etc/gitconfig	credential.helper manager
file:<USERPROFILE>/.gitconfig	credential.https://github.com.helper 
file:<USERPROFILE>/.gitconfig	credential.https://github.com.helper !'C:\Program Files\GitHub CLI\gh.exe' auth git-credential
```
- **Proof:** User-level `~/.gitconfig` resets `credential.https://github.com.helper` and routes all `https://github.com` Git pushes exclusively through `C:\Program Files\GitHub CLI\gh.exe auth git-credential`.

### 3.3 Live OAuth Token Scopes (`gh auth status` & `gh api -i user`)
```http
HTTP/2.0 200 OK
X-Oauth-Client-Id: 178c6fc778ccc68e1d6a
X-Oauth-Scopes: gist, read:org, repo
```
- **Repository Metadata (`gh api repos/manhquydev/zalo`):** `{"default_branch":"main","fork":false,"full_name":"manhquydev/zalo","owner_type":"User","permissions":{"admin":true,"maintain":true,"pull":true,"push":true,"triage":true},"private":true,"visibility":"private"}`.
- **Actions Permissions (`gh api repos/manhquydev/zalo/actions/permissions`):** `{"enabled":true,"allowed_actions":"all","sha_pinning_required":false}`.
- **Blocker #1 — Missing `workflow` OAuth Scope (`AWAITING_USER_AUTHORIZATION`):** Because `X-Oauth-Scopes` is `gist, read:org, repo` (missing `workflow`), GitHub's remote pre-receive hook rejects any HTTPS push adding or modifying `.github/workflows/*.yml` (`! [remote rejected] ... refusing to allow an OAuth App to create or update workflow ... without workflow scope`).
- **Blocker #2 — Private-Repository Billing API Visibility:** Billing REST endpoints are restricted under current token scopes (`HTTP 404 Not Found`); verify remaining monthly Actions compute credit directly in the GitHub Web UI (`Settings -> Billing & licensing`) rather than expanding CLI OAuth scopes beyond `workflow`.
- **Blocker #3 — GitHub `workflow_dispatch` Default-Branch Registration Rule (`NOT_REGISTERED_ON_DEFAULT_BRANCH`):** Per official GitHub Docs (`Events that trigger workflows -> workflow_dispatch`), GitHub Actions **only discovers and exposes `workflow_dispatch` triggers for workflow files present on the repository's default branch (`main`)**. Pushing `.github/workflows/cross-platform-validation.yml` solely to `build/cross-platform-distribution` will **not** register the workflow or enable `gh workflow run cross-platform-validation.yml --ref build/cross-platform-distribution` until the workflow file is first landed on `origin/main`.

---

## 4. Grounded GitHub Actions Private-Repository Billing & Quota Calculation

Pricing and multiplier rules were verified against official GitHub Actions billing documentation (`docs.github.com/en/billing/managing-billing-for-your-products/managing-billing-for-github-actions/about-billing-for-github-actions`):

- **Repository Billing Class:** `manhquydev/zalo` is `private: true` (`owner_type: User`). Standard GitHub-hosted runners are free only on public repositories; private repositories consume monthly plan inclusions (GitHub Free: **`$12.00 USD/month`** included compute credit; GitHub Pro: `$18.00/month` credit) and bill overages per minute (with **each job rounded up to the next whole minute**).
- **Verified Standard Runner Rates & Multipliers:**
  - **Linux 2-core x64 (`ubuntu-24.04`, SKU `actions_linux`):** **`$0.006 / min`** (`1x` minute multiplier)
  - **Windows x64 (`windows-2022`, SKU `actions_windows`):** **`$0.010 / min`** (`~1.67x` USD rate; `2x` classic minute multiplier)
  - **macOS 3-core arm64 / 4-core Intel (`macos-15` & `macos-15-intel`, SKU `actions_macos`):** **`$0.062 / min`** (`~10.33x` USD rate; `10x` classic minute multiplier)

### Per-Job Cost & Compute Credit Estimate (`ESTIMATED_PRE_RUN` — Actual Remote Spend: `$0.00 USD / NOT_MEASURED_YET`)

| Job ID | Runner Label | Billing SKU | Est. Rounded Min (`ESTIMATED_PRE_RUN`) | Per-Min Rate (USD) | Est. USD / Run (`ESTIMATED_PRE_RUN`) | Share of `$12.00` Free Credit | Default Dispatch Enabled? |
| :--- | :--- | :--- | :---: | :---: | :---: | :---: | :---: |
| `rust-workspace-validation` | `ubuntu-24.04` | `actions_linux` | `6 min` | `$0.006` | `$0.036` | `0.30%` | Always |
| `dependency-audit` | `ubuntu-24.04` | `actions_linux` | `3 min` | `$0.006` | `$0.018` | `0.15%` | Always |
| `ubuntu-2404-build` | `ubuntu-24.04` | `actions_linux` | `10 min` | `$0.006` | `$0.060` | `0.50%` | **`true`** |
| `windows-msvc-build` | `windows-2022` | `actions_windows` | `12 min` | `$0.010` | `$0.120` | `1.00%` | **`true`** |
| `macos-arm64-build` | `macos-15` | `actions_macos` | `14 min` | `$0.062` | `$0.868` | `7.23%` | **`false`** *(opt-in)* |
| `macos-intel-build` | `macos-15-intel` | `actions_macos` | `15 min` | `$0.062` | `$0.930` | `7.75%` | **`false`** *(opt-in)* |
| `artifact-verification` | `ubuntu-24.04` | `actions_linux` | `2 min` | `$0.006` | `$0.012` | `0.10%` | Always (`if: always()`) |
| **Default Run Total (Ubuntu + Windows)** | — | — | **`33 min`** | — | **`$0.246`** | **`2.05%` of `$12.00`** | Default |
| **Full 4-OS Run Total (+ macOS arm64 & Intel)** | — | — | **`62 min`** | — | **`$2.044`** | **`17.03%` of `$12.00`** | Explicit opt-in only |
| **Actual Remote Consumption To Date** | — | — | **`0 min`** | — | **`$0.00`** | **`NOT_MEASURED_YET` (`0` runs)** | — |

Keeping `enable_macos_arm64=false` and `enable_macos_intel=false` by default reduces routine manual dispatch cost by **88.0%** (`$2.044` -> `$0.246` per run) and prevents accidental exhaustion of private-repo Actions quota.

---

## 5. Safe 6-Step Default-Branch Rollout & `workflow_dispatch` Execution Plan

Because interactive browser/device OAuth consent is required to add `workflow` scope and human approval is required before pushing to `origin` or spending private-repo Actions budget, execute the following safe rollout sequence (never force-push, never create plaintext PAT scripts):

1. **Interactive OAuth Scope Upgrade (Human Terminal Action):**
   Run GitHub CLI interactive web flow to grant `workflow` scope to the existing keyring token:
   ```powershell
   & "C:\Program Files\GitHub CLI\gh.exe" auth refresh -h github.com -s workflow
   & "C:\Program Files\GitHub CLI\gh.exe" api -i user | Select-String "X-Oauth-Scopes"
   ```
2. **Pre-Flight Private-Repo Compute Budget Check:**
   Verify remaining monthly GitHub Free `$12.00 USD/month` Actions compute credit in the GitHub Web UI (`Settings -> Billing & licensing`) before triggering builds.
3. **Sync Verified Baseline `main` (`a5feb4a`) to `origin/main` (Fast-Forward Only):**
   ```powershell
   git -C "<REPO_ROOT>" push origin main
   ```
4. **Register `cross-platform-validation.yml` on Default Branch (`main`) via Reviewed PR:**
   Push `build/cross-platform-distribution`, open a reviewed PR into `main` (after applying approved atomic `ir-patches/0001..0007` if desired), and merge cleanly without `--force`:
   ```powershell
   git -C "<WORKTREE_ROOT>/zalo-cross-platform-build" push -u origin build/cross-platform-distribution
   & "C:\Program Files\GitHub CLI\gh.exe" pr create --base main --head build/cross-platform-distribution --title "ci(packaging): add native cross-platform validation workflow and packaging docs"
   ```
5. **Confirm Remote Workflow Registration on `main`:**
   Once merged to `main`, confirm GitHub indexed `cross-platform-validation.yml`:
   ```powershell
   & "C:\Program Files\GitHub CLI\gh.exe" workflow list --repo manhquydev/zalo
   ```
6. **Phased Manual `workflow_dispatch` Execution (Priority 1 -> 4):**
   - **Phase A (Low-cost Linux + Windows baseline, `$0.246` est.):**
     ```powershell
     & "C:\Program Files\GitHub CLI\gh.exe" workflow run cross-platform-validation.yml --repo manhquydev/zalo --ref main `
       -f enable_ubuntu=true -f enable_windows=true -f enable_macos_arm64=false -f enable_macos_intel=false -f bundles=all
     ```
   - **Phase B (Opt-in native macOS `macos-15` arm64 & `macos-15-intel` x86_64 once Phase A is green and `$1.80` macOS compute is approved):**
     ```powershell
     & "C:\Program Files\GitHub CLI\gh.exe" workflow run cross-platform-validation.yml --repo manhquydev/zalo --ref main `
       -f enable_ubuntu=false -f enable_windows=false -f enable_macos_arm64=true -f enable_macos_intel=true -f bundles=all
     ```
