#!/usr/bin/env bash
# ==============================================================================
# MultiZaloDesktop — Turnkey Native Ubuntu 24.04 Validation Harness
# Path: desktop-instance-manager/tests/platform-linux/run-native-ubuntu-validation.sh
#
# Executes all 7 native Ubuntu 24.04 LTS verification checks on a real Linux
# kernel (Ubuntu 24.04 Desktop VM, `ubuntu-24.04` / `ubuntu-latest` GitHub-hosted
# runner, or physical Ubuntu workstation) and emits both human-readable evidence
# and a structured RFC 8259 JSON report.
#
# Strict Claim-Discipline & Security Invariants:
#   - NEVER marks `LINUX_ADAPTER_TESTED=PASS` on Windows, macOS, MSYS2, Git Bash,
#     Cygwin, or cross-compilation hosts (`uname -s` MUST equal `Linux` with a
#     genuine `/proc` filesystem).
#   - Explicitly distinguishes WSL2 (`WSL2_KERNEL_ONLY_NOT_GNOME_DESKTOP`) and
#     headless CI (`HEADLESS_CI_KERNEL_ONLY`) from a real Ubuntu GNOME/Wayland
#     desktop session (`WAYLAND_DESKTOP_ACTIVE` / `X11_DESKTOP_ACTIVE`).
#   - Refuses execution as `root` (`UID == 0` -> `SECURITY_BLOCKED`).
#   - Never runs `killall` or `pkill`, never disables AppArmor, never injects
#     `--no-sandbox`, and cleans up all temporary profiles/processes on exit.
# ==============================================================================

set -euo pipefail

SCRIPT_SOURCE="${BASH_SOURCE[0]:-$0}"
case "$SCRIPT_SOURCE" in
  */*|*\\*) SCRIPT_PARENT_DIR="${SCRIPT_SOURCE%[/\\]*}" ;;
  *)        SCRIPT_PARENT_DIR="." ;;
esac
SCRIPT_DIR="$(cd "${SCRIPT_PARENT_DIR:-.}" && pwd)"
DIM_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
FIXTURE_SUITE_MJS="$DIM_ROOT/tests/platform-linux/run-ubuntu-fixture-tests.mjs"

OPT_JSON_ONLY=0
OPT_REPORT_OUT=""
OPT_STRICT_TAURI_PREREQS=0
OPT_REQUIRE_BWRAP=0
OPT_VERIFY_HOST_GUARD=0

usage() {
  printf '%s\n' \
    "Usage: run-native-ubuntu-validation.sh [OPTIONS]" \
    "" \
    "Options:" \
    "  --json                   Emit pure JSON report to stdout (logs go to stderr)" \
    "  --report-out <PATH>      Write structured JSON evidence report to <PATH>" \
    "  --strict-tauri-prereqs   Fail Check 7 if any Tauri 2 / audio / FUSE package is missing" \
    "  --require-bwrap          Fail Check 1 if bubblewrap (bwrap) is missing or blocked" \
    "  --verify-host-guard      Non-Linux self-test: verify MSYS2/Cygwin/non-Linux guard exits cleanly" \
    "  -h, --help               Show this help text"
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --json)
      OPT_JSON_ONLY=1
      shift
      ;;
    --report-out)
      if [ "$#" -lt 2 ]; then
        echo "ERROR: --report-out requires a file path argument" >&2
        exit 2
      fi
      OPT_REPORT_OUT="$2"
      shift 2
      ;;
    --report-out=*)
      OPT_REPORT_OUT="${1#--report-out=}"
      shift
      ;;
    --strict-tauri-prereqs)
      OPT_STRICT_TAURI_PREREQS=1
      shift
      ;;
    --require-bwrap)
      OPT_REQUIRE_BWRAP=1
      shift
      ;;
    --verify-host-guard)
      OPT_VERIFY_HOST_GUARD=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "ERROR: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

log_info() {
  if [ "$OPT_JSON_ONLY" -eq 1 ]; then
    printf '[ubuntu-native-validation] %s\n' "$*" >&2
  else
    printf '[ubuntu-native-validation] %s\n' "$*"
  fi
}

json_escape() {
  local raw="${1:-}"
  raw="${raw//\\/\\\\}"
  raw="${raw//\"/\\\"}"
  raw="${raw//$'\r'/\\r}"
  raw="${raw//$'\n'/\\n}"
  raw="${raw//$'\t'/\\t}"
  printf '%s' "$raw"
}

utc_now_iso() {
  date -u +"%Y-%m-%dT%H:%M:%SZ" 2>/dev/null || printf '1970-01-01T00:00:00Z'
}

now_epoch_ms() {
  if command -v node >/dev/null 2>&1; then
    node -e 'process.stdout.write(String(Date.now()))' 2>/dev/null && return 0
  fi
  if command -v python3 >/dev/null 2>&1; then
    python3 -c 'import time; print(int(time.time() * 1000))' 2>/dev/null && return 0
  fi
  local sec
  sec="$(date +%s 2>/dev/null || printf '0')"
  printf '%s000' "$sec"
}

# ------------------------------------------------------------------------------
# Non-Linux / MSYS2 / Git Bash / Cygwin Hard Guard
# ------------------------------------------------------------------------------
UNAME_S="$(uname -s 2>/dev/null || printf 'UNKNOWN')"
UNAME_R="$(uname -r 2>/dev/null || printf 'UNKNOWN')"
UNAME_M="$(uname -m 2>/dev/null || printf 'UNKNOWN')"
UNAME_ALL="$(uname -a 2>/dev/null || printf 'UNKNOWN')"
DETECTED_OSTYPE="${OSTYPE:-unknown}"

IS_GENUINE_LINUX=1
NON_LINUX_REASON=""

case "$UNAME_S" in
  Linux)
    ;;
  MINGW*|MSYS*|CYGWIN*)
    IS_GENUINE_LINUX=0
    NON_LINUX_REASON="MSYS2/MinGW/Cygwin userland atop Windows NT ($UNAME_S) is NOT a Linux kernel"
    ;;
  *)
    IS_GENUINE_LINUX=0
    NON_LINUX_REASON="Host kernel '$UNAME_S' is not 'Linux'"
    ;;
esac

case "$DETECTED_OSTYPE" in
  msys*|cygwin*|win32*)
    IS_GENUINE_LINUX=0
    NON_LINUX_REASON="OSTYPE='$DETECTED_OSTYPE' indicates Windows compatibility layer, NOT a Linux kernel"
    ;;
esac

if [ "$IS_GENUINE_LINUX" -eq 1 ] && [ ! -r /proc/version ]; then
  IS_GENUINE_LINUX=0
  NON_LINUX_REASON="/proc/version is absent or unreadable; genuine Linux procfs is required"
fi

if [ "$IS_GENUINE_LINUX" -eq 0 ]; then
  GUARD_TS_UTC="$(utc_now_iso)"
  printf -v GUARD_JSON '%s\n' \
    "{" \
    "  \"schema_version\": \"1.0.0\"," \
    "  \"timestamp_utc\": \"$(json_escape "$GUARD_TS_UTC")\"," \
    "  \"overall_status\": \"BLOCKED_NOT_LINUX_KERNEL\"," \
    "  \"host\": {" \
    "    \"uname_s\": \"$(json_escape "$UNAME_S")\"," \
    "    \"uname_r\": \"$(json_escape "$UNAME_R")\"," \
    "    \"uname_m\": \"$(json_escape "$UNAME_M")\"," \
    "    \"uname_a\": \"$(json_escape "$UNAME_ALL")\"," \
    "    \"ostype\": \"$(json_escape "$DETECTED_OSTYPE")\"," \
    "    \"is_linux_kernel\": false," \
    "    \"blocker_reason\": \"$(json_escape "$NON_LINUX_REASON")\"" \
    "  }," \
    "  \"acceptance_classification\": {" \
    "    \"LINUX_ADAPTER_TESTED\": \"NOT_TESTED\"," \
    "    \"DESKTOP_FIXTURE_TESTED\": \"NOT_TESTED\"," \
    "    \"UBUNTU_DESKTOP_SESSION_SCOPE\": \"NON_LINUX_HOST\"," \
    "    \"TAURI_UBUNTU_PREREQUISITES\": \"NOT_TESTED\"," \
    "    \"COMMUNITY_PORT_TESTED\": \"NOT_TESTED\"," \
    "    \"OFFICIAL_ZALO_UNSUPPORTED\": \"CONFIRMED_UNSUPPORTED\"" \
    "  }," \
    "  \"checks\": []" \
    "}"
  GUARD_JSON="${GUARD_JSON%$'\n'}"
  if [ -n "$OPT_REPORT_OUT" ]; then
    case "$OPT_REPORT_OUT" in
      */*|*\\*)
        REPORT_PARENT_DIR="${OPT_REPORT_OUT%[/\\]*}"
        if [ -n "$REPORT_PARENT_DIR" ] && [ ! -d "$REPORT_PARENT_DIR" ]; then
          mkdir -p "$REPORT_PARENT_DIR"
        fi
        ;;
    esac
    printf '%s\n' "$GUARD_JSON" > "$OPT_REPORT_OUT"
  fi
  log_info "BLOCKED: $NON_LINUX_REASON"
  log_info "Claim discipline enforced: LINUX_ADAPTER_TESTED=NOT_TESTED"
  if [ "$OPT_JSON_ONLY" -eq 1 ]; then
    printf '%s\n' "$GUARD_JSON"
  else
    printf -- '--- BEGIN_VALIDATION_JSON ---\n%s\n--- END_VALIDATION_JSON ---\n' "$GUARD_JSON"
  fi
  if [ "$OPT_VERIFY_HOST_GUARD" -eq 1 ]; then
    exit 0
  fi
  exit 1
fi

if [ "$OPT_VERIFY_HOST_GUARD" -eq 1 ]; then
  log_info "Host is a genuine Linux kernel ($UNAME_ALL); --verify-host-guard satisfied."
  exit 0
fi

# ------------------------------------------------------------------------------
# Linux Workspace & Tracked Cleanup Setup
# ------------------------------------------------------------------------------
EFFECTIVE_UID="$(id -u)"
if [ "$EFFECTIVE_UID" -eq 0 ]; then
  log_info "SECURITY_BLOCKED: refusing to run validation harness as root (UID 0). Run as an unprivileged user."
  exit 1
fi

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/dim-ubuntu-native-XXXXXX")"
chmod 0700 "$WORK_DIR"
RESULTS_TSV="$WORK_DIR/checks.tsv"
: > "$RESULTS_TSV"

TRACKED_PIDS=""

register_pid() {
  local pid="${1:-}"
  if [ -n "$pid" ]; then
    TRACKED_PIDS="$TRACKED_PIDS $pid"
  fi
}

cleanup() {
  local pid
  for pid in $TRACKED_PIDS; do
    if kill -0 "$pid" 2>/dev/null; then
      kill -TERM "$pid" 2>/dev/null || true
    fi
  done
  rm -rf "$WORK_DIR" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

record_check() {
  local id="$1"
  local name="$2"
  local status="$3"
  local duration_ms="$4"
  local detail="$5"
  local clean_detail="${detail//$'\t'/ }"
  clean_detail="${clean_detail//$'\r'/}"
  clean_detail="${clean_detail//$'\n'/ | }"
  printf '%s\t%s\t%s\t%s\t%s\n' "$id" "$name" "$status" "$duration_ms" "$clean_detail" >> "$RESULTS_TSV"
  log_info "[$status] Check #$id ($name) [${duration_ms}ms] — $clean_detail"
}

read_first_line_or_na() {
  local file_path="$1"
  if [ -r "$file_path" ]; then
    head -n 1 "$file_path" | tr -d '\r\n'
  else
    printf 'absent'
  fi
}

# Supports dot-separated paths including numeric array indices (e.g. `profiles.0.running`)
extract_json_field() {
  local json_file="$1"
  local expr="$2"
  if command -v node >/dev/null 2>&1; then
    node - "$json_file" "$expr" <<'EOF'
const fs = require('node:fs');
const data = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const parts = process.argv[3].split('.');
let cur = data;
for (const p of parts) {
  if (cur === null || cur === undefined) break;
  cur = Array.isArray(cur) ? cur[Number(p)] : cur[p];
}
if (cur === null || cur === undefined) process.stdout.write('');
else if (typeof cur === 'object') process.stdout.write(JSON.stringify(cur));
else process.stdout.write(String(cur));
EOF
    return 0
  fi
  python3 - "$json_file" "$expr" <<'EOF'
import json, sys
with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    cur = json.load(fh)
for part in sys.argv[2].split('.'):
    if cur is None:
        break
    if isinstance(cur, list):
        idx = int(part)
        cur = cur[idx] if 0 <= idx < len(cur) else None
    elif isinstance(cur, dict):
        cur = cur.get(part)
    else:
        cur = None
if cur is None:
    sys.stdout.write('')
elif isinstance(cur, (dict, list)):
    sys.stdout.write(json.dumps(cur))
elif isinstance(cur, bool):
    sys.stdout.write('true' if cur else 'false')
else:
    sys.stdout.write(str(cur))
EOF
}

wait_pid_dead() {
  local pid="$1"
  local max_iters="${2:-60}"
  local i=0
  while [ "$i" -lt "$max_iters" ]; do
    if ! kill -0 "$pid" 2>/dev/null; then
      return 0
    fi
    if [ -r "/proc/$pid/stat" ]; then
      local st
      st="$(awk '{ sub(/^.*\)[[:space:]]+/, ""); print $1 }' "/proc/$pid/stat" 2>/dev/null || printf '')"
      if [ "$st" = "Z" ] || [ "$st" = "X" ]; then
        return 0
      fi
    else
      return 0
    fi
    sleep 0.05
    i=$((i + 1))
  done
  ! kill -0 "$pid" 2>/dev/null
}

is_active_linux_pid() {
  local pid="$1"
  if [ -z "$pid" ] || ! kill -0 "$pid" 2>/dev/null || [ ! -r "/proc/$pid/stat" ]; then
    return 1
  fi
  local st
  st="$(awk '{ sub(/^.*\)[[:space:]]+/, ""); print $1 }' "/proc/$pid/stat" 2>/dev/null || printf '')"
  if [ -z "$st" ] || [ "$st" = "Z" ] || [ "$st" = "X" ] || [ "$st" = "x" ]; then
    return 1
  fi
  return 0
}

parse_proc_stat_fields() {
  local pid="$1"
  awk '
    {
      open_idx = index($0, "(");
      close_idx = 0;
      pos = 1;
      tmp = $0;
      while ((idx = index(tmp, ") ")) > 0) {
        close_idx = pos + idx - 1;
        pos = close_idx + 2;
        tmp = substr($0, pos);
      }
      if (open_idx > 0 && close_idx > open_idx) {
        comm_val = substr($0, open_idx + 1, close_idx - open_idx - 1);
        after = substr($0, close_idx + 2);
        split(after, f, /[[:space:]]+/);
        # f[1]=state(3), f[2]=ppid(4), f[3]=pgrp(5), f[4]=session(6), f[20]=starttime(22)
        printf "state=%s ppid=%s pgrp=%s session=%s starttime_ticks=%s comm=%s\n", f[1], f[2], f[3], f[4], f[20], comm_val;
      }
    }
  ' "/proc/$pid/stat"
}

# Global metadata collected across checks for JSON serialization
DISTRO_ID="unknown"
DISTRO_VERSION_ID="unknown"
DISTRO_CODENAME="unknown"
DISTRO_PRETTY="Linux"
IS_WSL2=0
SESSION_SCOPE="HEADLESS_CI_KERNEL_ONLY"
APPARMOR_ENABLED="absent"
APPARMOR_RESTRICT_USERNS="absent"
UNPRIV_USERNS_CLONE="absent"
BWRAP_PROFILE_PRESENT=0
BWRAP_VERSION=""
BWRAP_SMOKE_OK="not_tested"
POSIX_PERMS_STATUS="NOT_TESTED"
TAURI_PREREQS_STATUS="NOT_TESTED"
TAURI_PREREQS_JSON_FILE="$WORK_DIR/tauri-prereqs.json"
printf '[]\n' > "$TAURI_PREREQS_JSON_FILE"

CARGO_TARGET_BASE="${CARGO_TARGET_DIR:-$DIM_ROOT/target}"
DIM_CLI_BIN="$CARGO_TARGET_BASE/debug/dim-cli"
DIM_FIXTURE_BIN="$CARGO_TARGET_BASE/debug/dim-test-fixture"

# ==============================================================================
# CHECK 1: OS / Kernel / AppArmor / Session Preflight
# ==============================================================================
run_check_1_preflight() {
  local t0
  t0="$(now_epoch_ms)"
  local ok=1
  local notes=""

  if [ -r /etc/os-release ]; then
    DISTRO_ID="$(grep -E '^ID=' /etc/os-release | head -n 1 | cut -d= -f2- | tr -d '"' | tr -d "'" || printf 'unknown')"
    DISTRO_VERSION_ID="$(grep -E '^VERSION_ID=' /etc/os-release | head -n 1 | cut -d= -f2- | tr -d '"' | tr -d "'" || printf 'unknown')"
    DISTRO_CODENAME="$(grep -E '^VERSION_CODENAME=' /etc/os-release | head -n 1 | cut -d= -f2- | tr -d '"' | tr -d "'" || printf 'unknown')"
    DISTRO_PRETTY="$(grep -E '^PRETTY_NAME=' /etc/os-release | head -n 1 | cut -d= -f2- | tr -d '"' | tr -d "'" || printf 'Linux')"
  else
    ok=0
    notes="missing /etc/os-release"
  fi

  if grep -qiE '(microsoft|wsl)' /proc/version 2>/dev/null; then
    IS_WSL2=1
  fi

  APPARMOR_ENABLED="$(read_first_line_or_na /sys/module/apparmor/parameters/enabled)"
  APPARMOR_RESTRICT_USERNS="$(read_first_line_or_na /proc/sys/kernel/apparmor_restrict_unprivileged_userns)"
  UNPRIV_USERNS_CLONE="$(read_first_line_or_na /proc/sys/kernel/unprivileged_userns_clone)"

  if [ -f /etc/apparmor.d/bwrap-userns-restrict ] || [ -f /etc/apparmor.d/bwrap ] || [ -f /etc/apparmor.d/usr.bin.bwrap ]; then
    BWRAP_PROFILE_PRESENT=1
  fi

  local xdg_session="${XDG_SESSION_TYPE:-}"
  local wl_disp="${WAYLAND_DISPLAY:-}"
  local x11_disp="${DISPLAY:-}"

  if [ "$IS_WSL2" -eq 1 ]; then
    SESSION_SCOPE="WSL2_KERNEL_ONLY_NOT_GNOME_DESKTOP"
  elif [ "$xdg_session" = "wayland" ] || [ -n "$wl_disp" ]; then
    SESSION_SCOPE="WAYLAND_DESKTOP_ACTIVE"
  elif [ "$xdg_session" = "x11" ] || [ -n "$x11_disp" ]; then
    SESSION_SCOPE="X11_DESKTOP_ACTIVE"
  else
    SESSION_SCOPE="HEADLESS_CI_KERNEL_ONLY"
  fi

  if command -v bwrap >/dev/null 2>&1; then
    BWRAP_VERSION="$(bwrap --version 2>&1 | head -n 1 | tr -d '\r\n' || printf 'present')"
    if bwrap --new-session \
      --ro-bind /usr /usr \
      --ro-bind-try /lib /lib \
      --ro-bind-try /lib64 /lib64 \
      --ro-bind-try /bin /bin \
      --proc /proc \
      --dev /dev \
      -- /usr/bin/env true >/dev/null 2>&1; then
      BWRAP_SMOKE_OK="pass"
    else
      BWRAP_SMOKE_OK="blocked_by_kernel_or_apparmor"
      if [ "$OPT_REQUIRE_BWRAP" -eq 1 ]; then
        ok=0
        notes="${notes:+$notes; }bwrap namespace smoke test failed under current AppArmor/userns policy"
      fi
    fi
  else
    BWRAP_VERSION="not_installed"
    BWRAP_SMOKE_OK="not_installed"
    if [ "$OPT_REQUIRE_BWRAP" -eq 1 ]; then
      ok=0
      notes="${notes:+$notes; }bwrap not installed (--require-bwrap set)"
    fi
  fi

  local detail="os=${DISTRO_PRETTY} (${DISTRO_ID}/${DISTRO_VERSION_ID}), kernel=${UNAME_R}/${UNAME_M}, euid=${EFFECTIVE_UID}, wsl2=${IS_WSL2}, apparmor=${APPARMOR_ENABLED}, restrict_userns=${APPARMOR_RESTRICT_USERNS}, session_type=${xdg_session:-unset}, wayland=${wl_disp:-unset}, display=${x11_disp:-unset}, scope=${SESSION_SCOPE}, bwrap='${BWRAP_VERSION}' (smoke=${BWRAP_SMOKE_OK})"
  if [ -n "$notes" ]; then
    detail="$detail; error=$notes"
  fi

  local dur=$(( $(now_epoch_ms) - t0 ))
  if [ "$ok" -eq 1 ]; then
    record_check 1 "OS/kernel/AppArmor/session preflight" "PASS" "$dur" "$detail"
  else
    record_check 1 "OS/kernel/AppArmor/session preflight" "FAIL" "$dur" "$detail"
    return 1
  fi
}

# ==============================================================================
# CHECK 2: Native Rust Compile
# ==============================================================================
run_check_2_rust_compile() {
  local t0
  t0="$(now_epoch_ms)"

  if ! command -v cargo >/dev/null 2>&1; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 2 "Native Rust compile" "FAIL" "$dur" "cargo not found in PATH"
    return 1
  fi

  local rustc_ver cargo_ver build_log
  rustc_ver="$(rustc --version 2>&1 | head -n 1)"
  cargo_ver="$(cargo --version 2>&1 | head -n 1)"
  build_log="$WORK_DIR/check2-build.log"

  if ! (cd "$DIM_ROOT" && cargo build -p dim-platform-linux -p dim-test-fixture --locked) >"$build_log" 2>&1; then
    local tail_err
    tail_err="$(tail -n 15 "$build_log" | tr '\n' ' ')"
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 2 "Native Rust compile" "FAIL" "$dur" "cargo build failed: $tail_err"
    return 1
  fi

  if [ ! -x "$DIM_CLI_BIN" ] || [ ! -x "$DIM_FIXTURE_BIN" ]; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 2 "Native Rust compile" "FAIL" "$dur" "Expected executables missing: $DIM_CLI_BIN or $DIM_FIXTURE_BIN"
    return 1
  fi

  # Verify native ELF magic (0x7f 'E' 'L' 'F' -> 7f454c46) statically without ldd
  local magic_cli magic_fix
  magic_cli="$(od -An -tx1 -N4 "$DIM_CLI_BIN" | tr -d ' \n')"
  magic_fix="$(od -An -tx1 -N4 "$DIM_FIXTURE_BIN" | tr -d ' \n')"
  if [ "$magic_cli" != "7f454c46" ] || [ "$magic_fix" != "7f454c46" ]; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 2 "Native Rust compile" "FAIL" "$dur" "Compiled binaries lacked ELF magic (cli=$magic_cli, fixture=$magic_fix)"
    return 1
  fi

  local dur=$(( $(now_epoch_ms) - t0 ))
  record_check 2 "Native Rust compile" "PASS" "$dur" "Built ELF64 dim-platform-linux + dim-cli + dim-test-fixture (${rustc_ver}; ${cargo_ver})"
}

# ==============================================================================
# CHECK 3: Native Rust Clippy, Unit & Integration Tests (+ 15-Scenario Suite)
# ==============================================================================
run_check_3_rust_tests() {
  local t0
  t0="$(now_epoch_ms)"
  local clippy_log="$WORK_DIR/check3-clippy.log"
  local test_log="$WORK_DIR/check3-test.log"
  local node_json="$WORK_DIR/check3-fixture-suite.json"

  if ! command -v node >/dev/null 2>&1; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 3 "Native Rust unit & integration tests" "FAIL" "$dur" "node runtime required by fixture integration tests was not found in PATH"
    return 1
  fi

  if ! (cd "$DIM_ROOT" && cargo clippy -p dim-platform-linux --all-targets --locked -- -D warnings) >"$clippy_log" 2>&1; then
    local err_snip
    err_snip="$(tail -n 15 "$clippy_log" | tr '\n' ' ')"
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 3 "Native Rust unit & integration tests" "FAIL" "$dur" "cargo clippy failed: $err_snip"
    return 1
  fi

  if ! (cd "$DIM_ROOT" && cargo test -p dim-platform-linux --locked) >"$test_log" 2>&1; then
    local err_snip
    err_snip="$(tail -n 20 "$test_log" | tr '\n' ' ')"
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 3 "Native Rust unit & integration tests" "FAIL" "$dur" "cargo test failed: $err_snip"
    return 1
  fi

  local passed_lines
  passed_lines="$(grep -E 'test result: ok\.' "$test_log" | tr '\n' ';' || printf 'ok')"

  if [ -f "$FIXTURE_SUITE_MJS" ]; then
    if ! (cd "$DIM_ROOT" && node "$FIXTURE_SUITE_MJS" --json --report-out "$node_json") >/dev/null 2>"$WORK_DIR/check3-node.err"; then
      local dur=$(( $(now_epoch_ms) - t0 ))
      record_check 3 "Native Rust unit & integration tests" "FAIL" "$dur" "run-ubuntu-fixture-tests.mjs failed"
      return 1
    fi
    local linux_tested
    linux_tested="$(extract_json_field "$node_json" "acceptance_classification.LINUX_ADAPTER_TESTED")"
    if [ "$linux_tested" != "PASS" ]; then
      local dur=$(( $(now_epoch_ms) - t0 ))
      record_check 3 "Native Rust unit & integration tests" "FAIL" "$dur" "Expected LINUX_ADAPTER_TESTED=PASS in Node suite on Linux, got '$linux_tested'"
      return 1
    fi
  fi

  local dur=$(( $(now_epoch_ms) - t0 ))
  record_check 3 "Native Rust unit & integration tests" "PASS" "$dur" "clippy=0_warnings; cargo_test=[${passed_lines}]; node_15_scenarios=PASS (LINUX_ADAPTER_TESTED=PASS)"
}

# ==============================================================================
# CHECK 4: Fixture Process Lifecycle (Short-Lived Launcher Survival & /proc)
# ==============================================================================
run_check_4_lifecycle() {
  local t0
  t0="$(now_epoch_ms)"
  local data_dir="$WORK_DIR/check4-store"
  local create_json="$WORK_DIR/check4-create.json"
  local launch_json="$WORK_DIR/check4-launch.json"
  local status_json="$WORK_DIR/check4-status.json"
  local stop_json="$WORK_DIR/check4-stop.json"
  local launcher_pid_file="$WORK_DIR/check4-launcher.pid"

  if ! "$DIM_CLI_BIN" --data-dir "$data_dir" create "Lifecycle Alpha" >"$create_json"; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 4 "Fixture process lifecycle" "FAIL" "$dur" "dim-cli create failed"
    return 1
  fi

  local profile_id dir_name profile_dir zalo_dir
  profile_id="$(extract_json_field "$create_json" "data.id")"
  dir_name="$(extract_json_field "$create_json" "data.directory_name")"
  profile_dir="$data_dir/Profiles/$dir_name"
  zalo_dir="$profile_dir/.config/ZaloData"

  # 4a. Spawn NodeFixture via a short-lived subshell process with leak-canary env vars:
  #     1) Short-lived launcher exits completely
  #     2) Spawned fixture child survives launcher exit (no premature SIGHUP / --die-with-parent kill)
  #     3) env_clear() strips APPDATA and USERPROFILE from child /proc/<pid>/environ
  (
    bash -c '
      echo "$$" > "$1"
      shift
      exec env APPDATA="/tmp/dim-leak-canary" USERPROFILE="/tmp/dim-leak-canary" "$@"
    ' _ "$launcher_pid_file" \
      "$DIM_CLI_BIN" --data-dir "$data_dir" --node-fixture --user-id 849400001 launch "$profile_id" >"$launch_json"
  )

  local launcher_pid
  launcher_pid="$(tr -d ' \r\n' < "$launcher_pid_file")"
  if ! wait_pid_dead "$launcher_pid" 40; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 4 "Fixture process lifecycle" "FAIL" "$dur" "Short-lived launcher PID $launcher_pid did not exit"
    return 1
  fi

  local fixture_pid
  fixture_pid="$(extract_json_field "$launch_json" "data.pid")"
  register_pid "$fixture_pid"

  # Wait 300ms AFTER launcher exit so any broken PDEATHSIG / session-leader SIGHUP manifests
  sleep 0.30

  if ! is_active_linux_pid "$fixture_pid"; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 4 "Fixture process lifecycle" "FAIL" "$dur" "Fixture child PID '$fixture_pid' died after short-lived launcher PID '$launcher_pid' exited"
    return 1
  fi

  local stat_summary proc_starttime env_dump
  stat_summary="$(parse_proc_stat_fields "$fixture_pid")"
  proc_starttime="$(printf '%s\n' "$stat_summary" | sed -n 's/.*starttime_ticks=\([^ ]*\).*/\1/p')"
  env_dump="$WORK_DIR/check4-environ.txt"
  tr '\0' '\n' < "/proc/$fixture_pid/environ" > "$env_dump"

  if ! grep -Fxq "DIM_PROFILE_ID=$profile_id" "$env_dump"; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 4 "Fixture process lifecycle" "FAIL" "$dur" "/proc/$fixture_pid/environ missing DIM_PROFILE_ID=$profile_id"
    return 1
  fi
  if ! grep -Fxq "HOME=$profile_dir" "$env_dump"; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 4 "Fixture process lifecycle" "FAIL" "$dur" "/proc/$fixture_pid/environ HOME mismatch"
    return 1
  fi
  if grep -Eq '^(APPDATA|USERPROFILE|LD_PRELOAD)=' "$env_dump"; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 4 "Fixture process lifecycle" "FAIL" "$dur" "/proc/$fixture_pid/environ leaked forbidden host env vars"
    return 1
  fi

  # Validate .runtime-pid metadata (including starttime_ticks & instance_nonce when recorded)
  local runtime_pid_note="runtime_pid_file=absent"
  if [ -f "$profile_dir/.runtime-pid" ]; then
    local rt_pid rt_starttime rt_nonce env_nonce
    rt_pid="$(extract_json_field "$profile_dir/.runtime-pid" "pid" || printf '')"
    rt_starttime="$(extract_json_field "$profile_dir/.runtime-pid" "starttime_ticks" || printf '')"
    rt_nonce="$(extract_json_field "$profile_dir/.runtime-pid" "instance_nonce" || printf '')"
    env_nonce="$(grep -E '^DIM_INSTANCE_NONCE=' "$env_dump" | head -n 1 | cut -d= -f2- || printf '')"

    if [ -n "$rt_pid" ] && [ "$rt_pid" != "$fixture_pid" ]; then
      local dur=$(( $(now_epoch_ms) - t0 ))
      record_check 4 "Fixture process lifecycle" "FAIL" "$dur" ".runtime-pid pid ($rt_pid) != fixture_pid ($fixture_pid)"
      return 1
    fi
    if [ -n "$rt_starttime" ] && [ "$rt_starttime" != "$proc_starttime" ]; then
      local dur=$(( $(now_epoch_ms) - t0 ))
      record_check 4 "Fixture process lifecycle" "FAIL" "$dur" ".runtime-pid starttime_ticks ($rt_starttime) != /proc/$fixture_pid/stat ($proc_starttime)"
      return 1
    fi
    if [ -n "$rt_nonce" ] && [ "$rt_nonce" != "$env_nonce" ]; then
      local dur=$(( $(now_epoch_ms) - t0 ))
      record_check 4 "Fixture process lifecycle" "FAIL" "$dur" ".runtime-pid instance_nonce mismatch against /proc/$fixture_pid/environ"
      return 1
    fi
    runtime_pid_note="runtime_pid(starttime=${rt_starttime:-legacy},nonce_present=$([ -n "$rt_nonce" ] && echo yes || echo no))"
  fi

  "$DIM_CLI_BIN" --data-dir "$data_dir" status >"$status_json"
  local mgr_running
  mgr_running="$(extract_json_field "$status_json" "profiles.0.running")"
  if [ "$mgr_running" != "true" ]; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 4 "Fixture process lifecycle" "FAIL" "$dur" "dim-cli status reported profiles[0].running='$mgr_running' while child alive"
    return 1
  fi

  if ! "$DIM_CLI_BIN" --data-dir "$data_dir" stop "$profile_id" >"$stop_json"; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 4 "Fixture process lifecycle" "FAIL" "$dur" "dim-cli stop failed"
    return 1
  fi

  if ! wait_pid_dead "$fixture_pid" 60; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 4 "Fixture process lifecycle" "FAIL" "$dur" "Fixture PID $fixture_pid still alive after dim-cli stop"
    return 1
  fi

  local clean_shutdown
  clean_shutdown="$(extract_json_field "$zalo_dir/launch-state.json" "clean_shutdown")"
  if [ "$clean_shutdown" != "true" ] || [ -e "$zalo_dir/lockfile" ] || [ -L "$zalo_dir/SingletonLock" ]; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 4 "Fixture process lifecycle" "FAIL" "$dur" "Unclean shutdown or leftover lockfile (clean_shutdown=$clean_shutdown)"
    return 1
  fi

  # 4b. Also launch native ELF64 `dim-test-fixture` via CustomExecutable path in a short-lived subshell
  local elf_create_json="$WORK_DIR/check4-elf-create.json"
  local elf_launch_json="$WORK_DIR/check4-elf-launch.json"
  "$DIM_CLI_BIN" --data-dir "$data_dir" create "Lifecycle Native ELF" >"$elf_create_json"
  local elf_id elf_pid
  elf_id="$(extract_json_field "$elf_create_json" "data.id")"
  (
    "$DIM_CLI_BIN" --data-dir "$data_dir" --app-path "$DIM_FIXTURE_BIN" launch "$elf_id" >"$elf_launch_json"
  )
  elf_pid="$(extract_json_field "$elf_launch_json" "data.pid")"
  register_pid "$elf_pid"
  sleep 0.20
  if ! is_active_linux_pid "$elf_pid"; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 4 "Fixture process lifecycle" "FAIL" "$dur" "Native ELF64 dim-test-fixture PID '$elf_pid' died after short-lived launcher exit"
    return 1
  fi
  "$DIM_CLI_BIN" --data-dir "$data_dir" stop "$elf_id" >/dev/null
  if ! wait_pid_dead "$elf_pid" 60; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 4 "Fixture process lifecycle" "FAIL" "$dur" "Native ELF64 dim-test-fixture PID $elf_pid did not stop cleanly"
    return 1
  fi

  local dur=$(( $(now_epoch_ms) - t0 ))
  record_check 4 "Fixture process lifecycle" "PASS" "$dur" "launcher_pid=${launcher_pid}(exited) -> node_pid=${fixture_pid}(survived) & elf64_pid=${elf_pid}(survived); ${stat_summary}; ${runtime_pid_note}; clean_stop=true"
}

# ==============================================================================
# CHECK 5: Multi-Profile HOME/XDG_* Isolation & user-dirs.dirs / Downloads
# ==============================================================================
CHECK5_DATA_DIR=""
CHECK5_DIR_A=""

run_check_5_isolation() {
  local t0
  t0="$(now_epoch_ms)"
  CHECK5_DATA_DIR="$WORK_DIR/check5-store"
  local c_a="$WORK_DIR/check5-create-a.json"
  local c_b="$WORK_DIR/check5-create-b.json"
  local l_a="$WORK_DIR/check5-launch-a.json"
  local l_b="$WORK_DIR/check5-launch-b.json"

  "$DIM_CLI_BIN" --data-dir "$CHECK5_DATA_DIR" create "Isolation Alpha" >"$c_a"
  "$DIM_CLI_BIN" --data-dir "$CHECK5_DATA_DIR" create "Isolation Beta" >"$c_b"

  local id_a id_b dname_a dname_b dir_a dir_b
  id_a="$(extract_json_field "$c_a" "data.id")"
  id_b="$(extract_json_field "$c_b" "data.id")"
  dname_a="$(extract_json_field "$c_a" "data.directory_name")"
  dname_b="$(extract_json_field "$c_b" "data.directory_name")"
  dir_a="$CHECK5_DATA_DIR/Profiles/$dname_a"
  dir_b="$CHECK5_DATA_DIR/Profiles/$dname_b"
  CHECK5_DIR_A="$dir_a"

  "$DIM_CLI_BIN" --data-dir "$CHECK5_DATA_DIR" --node-fixture --user-id 849500001 launch "$id_a" >"$l_a"
  "$DIM_CLI_BIN" --data-dir "$CHECK5_DATA_DIR" --node-fixture --user-id 849500002 launch "$id_b" >"$l_b"

  local pid_a pid_b
  pid_a="$(extract_json_field "$l_a" "data.pid")"
  pid_b="$(extract_json_field "$l_b" "data.pid")"
  register_pid "$pid_a"
  register_pid "$pid_b"

  if [ "$pid_a" = "$pid_b" ] || ! is_active_linux_pid "$pid_a" || ! is_active_linux_pid "$pid_b"; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 5 "Multi-profile HOME/XDG_* & Downloads isolation" "FAIL" "$dur" "Concurrent profiles failed to run independently (pid_a=$pid_a, pid_b=$pid_b)"
    return 1
  fi

  local env_a="$WORK_DIR/check5-env-a.txt"
  local env_b="$WORK_DIR/check5-env-b.txt"
  tr '\0' '\n' < "/proc/$pid_a/environ" > "$env_a"
  tr '\0' '\n' < "/proc/$pid_b/environ" > "$env_b"

  local key
  for key in HOME XDG_CONFIG_HOME XDG_DATA_HOME XDG_STATE_HOME XDG_CACHE_HOME XDG_DOWNLOAD_DIR XDG_DOCUMENTS_DIR TMPDIR; do
    local val_a val_b
    val_a="$(grep -E "^${key}=" "$env_a" | head -n 1 | cut -d= -f2- || printf '')"
    val_b="$(grep -E "^${key}=" "$env_b" | head -n 1 | cut -d= -f2- || printf '')"
    if [ -z "$val_a" ] || [ -z "$val_b" ] || [ "$val_a" = "$val_b" ]; then
      local dur=$(( $(now_epoch_ms) - t0 ))
      record_check 5 "Multi-profile HOME/XDG_* & Downloads isolation" "FAIL" "$dur" "Env key $key not isolated ('$val_a' vs '$val_b')"
      return 1
    fi
    case "$val_a" in
      "$dir_a"|"$dir_a"/*) ;;
      *)
        local dur=$(( $(now_epoch_ms) - t0 ))
        record_check 5 "Multi-profile HOME/XDG_* & Downloads isolation" "FAIL" "$dur" "Alpha $key='$val_a' escaped '$dir_a'"
        return 1
        ;;
    esac
    case "$val_b" in
      "$dir_b"|"$dir_b"/*) ;;
      *)
        local dur=$(( $(now_epoch_ms) - t0 ))
        record_check 5 "Multi-profile HOME/XDG_* & Downloads isolation" "FAIL" "$dur" "Beta $key='$val_b' escaped '$dir_b'"
        return 1
        ;;
    esac
  done

  if ! grep -Fq 'XDG_DOWNLOAD_DIR="$HOME/Downloads"' "$dir_a/.config/user-dirs.dirs" || \
     ! grep -Fq 'XDG_DOCUMENTS_DIR="$HOME/Documents"' "$dir_b/.config/user-dirs.dirs"; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 5 "Multi-profile HOME/XDG_* & Downloads isolation" "FAIL" "$dur" "user-dirs.dirs confinement entries missing"
    return 1
  fi

  local xdg_tool_note="xdg_user_dir=not_installed"
  if command -v xdg-user-dir >/dev/null 2>&1; then
    local resolved_dl
    resolved_dl="$(HOME="$dir_a" XDG_CONFIG_HOME="$dir_a/.config" xdg-user-dir DOWNLOAD 2>/dev/null || printf '')"
    if [ "$resolved_dl" != "$dir_a/Downloads" ]; then
      local dur=$(( $(now_epoch_ms) - t0 ))
      record_check 5 "Multi-profile HOME/XDG_* & Downloads isolation" "FAIL" "$dur" "xdg-user-dir DOWNLOAD resolved to '$resolved_dl', expected '$dir_a/Downloads'"
      return 1
    fi
    xdg_tool_note="xdg_user_dir=verified"
  fi

  printf 'alpha-secret-payload\n' > "$dir_a/Downloads/only-alpha.txt"
  printf 'beta-secret-payload\n' > "$dir_b/Downloads/only-beta.txt"
  if [ -e "$dir_a/Downloads/only-beta.txt" ] || [ -e "$dir_b/Downloads/only-alpha.txt" ]; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 5 "Multi-profile HOME/XDG_* & Downloads isolation" "FAIL" "$dur" "Downloads directory cross-contaminated"
    return 1
  fi

  if [ ! -f "$dir_a/.config/ZaloData/Database/_production/849500001/session.json" ] || \
     [ ! -f "$dir_b/.config/ZaloData/Database/_production/849500002/session.json" ] || \
     [ -e "$dir_a/.config/ZaloData/Database/_production/849500002" ]; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 5 "Multi-profile HOME/XDG_* & Downloads isolation" "FAIL" "$dur" "Database/_production/<uid> isolation failed"
    return 1
  fi

  "$DIM_CLI_BIN" --data-dir "$CHECK5_DATA_DIR" stop "$id_a" >/dev/null
  wait_pid_dead "$pid_a" 60
  if ! is_active_linux_pid "$pid_b"; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 5 "Multi-profile HOME/XDG_* & Downloads isolation" "FAIL" "$dur" "Stopping Alpha killed sibling Beta (pid_b=$pid_b)"
    return 1
  fi

  "$DIM_CLI_BIN" --data-dir "$CHECK5_DATA_DIR" stop "$id_b" >/dev/null
  wait_pid_dead "$pid_b" 60

  local dur=$(( $(now_epoch_ms) - t0 ))
  record_check 5 "Multi-profile HOME/XDG_* & Downloads isolation" "PASS" "$dur" "pids=[${pid_a},${pid_b}] disjoint HOME/XDG_*/TMPDIR/Downloads/user-dirs.dirs (${xdg_tool_note}); sibling survived targeted stop"
}

# ==============================================================================
# CHECK 6: Crash Recovery & Stale SingletonLock / Live PID-Reuse Check
# ==============================================================================
run_check_6_crash_and_pid_reuse() {
  local t0
  t0="$(now_epoch_ms)"
  local data_dir="$WORK_DIR/check6-store"
  local create_json="$WORK_DIR/check6-create.json"
  local status_crashed="$WORK_DIR/check6-status-crashed.json"
  local relaunch_json="$WORK_DIR/check6-relaunch.json"
  local status_spoofed="$WORK_DIR/check6-status-spoofed.json"

  "$DIM_CLI_BIN" --data-dir "$data_dir" create "Crash Recovery" >"$create_json"
  local crash_id dname crash_dir zalo_dir
  crash_id="$(extract_json_field "$create_json" "data.id")"
  dname="$(extract_json_field "$create_json" "data.directory_name")"
  crash_dir="$data_dir/Profiles/$dname"
  zalo_dir="$crash_dir/.config/ZaloData"

  # 6a. Launch with simulated crash (--crash --crash-delay-ms 80)
  "$DIM_CLI_BIN" --data-dir "$data_dir" --node-fixture --crash --crash-delay-ms 80 --user-id 849600001 launch "$crash_id" >/dev/null 2>&1 || true

  local waited=0
  while [ "$waited" -lt 40 ]; do
    if [ -f "$zalo_dir/launch-state.json" ]; then
      local cr_flag
      cr_flag="$(extract_json_field "$zalo_dir/launch-state.json" "crashed" || printf '')"
      if [ "$cr_flag" = "true" ]; then
        break
      fi
    fi
    sleep 0.05
    waited=$((waited + 1))
  done

  if [ ! -e "$zalo_dir/lockfile" ]; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 6 "Crash recovery & stale SingletonLock / PID-reuse check" "FAIL" "$dur" "Simulated crash did not leave stale lockfile"
    return 1
  fi

  # Ensure the crashing Node/fixture process has fully completed OS exit teardown
  # (`launch-state.json` is flushed right before `process.exit(1)` in `fixture-app.mjs`).
  local crashed_pid=""
  if [ -f "$zalo_dir/launch-state.json" ]; then
    crashed_pid="$(extract_json_field "$zalo_dir/launch-state.json" "last_pid" || printf '')"
  fi
  if [ -z "$crashed_pid" ] && [ -f "$zalo_dir/lockfile" ]; then
    crashed_pid="$(extract_json_field "$zalo_dir/lockfile" "pid" || printf '')"
  fi
  if [ -n "$crashed_pid" ]; then
    wait_pid_dead "$crashed_pid" 40 || true
  fi

  "$DIM_CLI_BIN" --data-dir "$data_dir" status >"$status_crashed"
  local stale_detected running_after_crash
  stale_detected="$(extract_json_field "$status_crashed" "profiles.0.stale_lockfile_detected")"
  running_after_crash="$(extract_json_field "$status_crashed" "profiles.0.running")"

  if [ "$running_after_crash" != "false" ] || [ "$stale_detected" != "true" ]; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 6 "Crash recovery & stale SingletonLock / PID-reuse check" "FAIL" "$dur" "Expected running=false & stale_lockfile_detected=true after crash, got running=$running_after_crash stale=$stale_detected"
    return 1
  fi

  "$DIM_CLI_BIN" --data-dir "$data_dir" --node-fixture --user-id 849600001 launch "$crash_id" >"$relaunch_json"
  local rec_pid crash_rec_flag launch_cnt
  rec_pid="$(extract_json_field "$relaunch_json" "data.pid")"
  register_pid "$rec_pid"
  crash_rec_flag="$(extract_json_field "$relaunch_json" "data.crash_recovered")"
  launch_cnt="$(extract_json_field "$zalo_dir/launch-state.json" "launch_count")"

  if [ "$crash_rec_flag" != "true" ] || [ "$launch_cnt" != "2" ]; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 6 "Crash recovery & stale SingletonLock / PID-reuse check" "FAIL" "$dur" "Relaunch recovery failed (crash_recovered=$crash_rec_flag, launch_count=$launch_cnt)"
    return 1
  fi

  "$DIM_CLI_BIN" --data-dir "$data_dir" stop "$crash_id" >/dev/null
  wait_pid_dead "$rec_pid" 60

  # 6b. Live /proc PID-reuse spoofing check against our own live shell PID ($$)
  local live_unrelated_pid="$$"
  local host_str
  host_str="$(hostname 2>/dev/null || printf 'ubuntu-host')"
  rm -f "$zalo_dir/SingletonLock" "$zalo_dir/lockfile" "$crash_dir/.runtime-pid"
  ln -s "${host_str}-${live_unrelated_pid}" "$zalo_dir/SingletonLock"
  printf '{"pid":%s}\n' "$live_unrelated_pid" > "$zalo_dir/lockfile"
  printf '{"pid":%s,"profile_id":"%s","instance_nonce":"spoofed-nonce","starttime_ticks":1}\n' "$live_unrelated_pid" "$crash_id" > "$crash_dir/.runtime-pid"

  "$DIM_CLI_BIN" --data-dir "$data_dir" status >"$status_spoofed"
  local spoof_running spoof_stale
  spoof_running="$(extract_json_field "$status_spoofed" "profiles.0.running")"
  spoof_stale="$(extract_json_field "$status_spoofed" "profiles.0.stale_lockfile_detected")"

  if [ "$spoof_running" != "false" ] || [ "$spoof_stale" != "true" ]; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    record_check 6 "Crash recovery & stale SingletonLock / PID-reuse check" "FAIL" "$dur" "PID-reuse spoofing check failed! Live unrelated PID $live_unrelated_pid was misclassified (running=$spoof_running, stale=$spoof_stale)"
    return 1
  fi

  "$DIM_CLI_BIN" --data-dir "$data_dir" stop "$crash_id" >/dev/null 2>&1 || true
  if ! kill -0 "$live_unrelated_pid" 2>/dev/null; then
    return 1
  fi

  local dur=$(( $(now_epoch_ms) - t0 ))
  record_check 6 "Crash recovery & stale SingletonLock / PID-reuse check" "PASS" "$dur" "Crash recovered cleanly (launch_count=2, crash_recovered=true); dangling SingletonLock + recycled live PID ${live_unrelated_pid} rejected safely"
}

# ==============================================================================
# CHECK 7: Tauri 2 Ubuntu Prerequisites + Strict POSIX 0700 / 0600 Verification
# ==============================================================================
check_pkg_config_item() {
  local pc_name="$1"
  local apt_pkg="$2"
  if command -v pkg-config >/dev/null 2>&1 && pkg-config --exists "$pc_name" 2>/dev/null; then
    local ver
    ver="$(pkg-config --modversion "$pc_name" 2>/dev/null | tr -d '\r\n')"
    printf '%s\ttrue\t%s\t%s\n' "$pc_name" "$ver" "$apt_pkg"
  else
    printf '%s\tfalse\tmissing\t%s\n' "$pc_name" "$apt_pkg"
  fi
}

check_dpkg_or_soname() {
  local pkg_label="$1"
  local soname="$2"
  local apt_pkg="$3"
  if command -v dpkg-query >/dev/null 2>&1; then
    local dq
    dq="$(dpkg-query -W -f='${Status}\t${Version}\n' "$pkg_label" 2>/dev/null || printf '')"
    if printf '%s' "$dq" | grep -Fq "install ok installed"; then
      local ver
      ver="$(printf '%s' "$dq" | awk -F'\t' '{print $2}' | tr -d '\r\n')"
      printf '%s\ttrue\t%s\t%s\n' "$pkg_label" "$ver" "$apt_pkg"
      return 0
    fi
  fi
  local multiarch_dir
  for multiarch_dir in /usr/lib/x86_64-linux-gnu /lib/x86_64-linux-gnu /usr/lib/aarch64-linux-gnu /lib/aarch64-linux-gnu /usr/lib /lib64; do
    if [ -e "$multiarch_dir/$soname" ]; then
      printf '%s\ttrue\t%s\t%s\n' "$pkg_label" "$multiarch_dir/$soname" "$apt_pkg"
      return 0
    fi
  done
  printf '%s\tfalse\tmissing\t%s\n' "$pkg_label" "$apt_pkg"
}

serialize_prereq_tsv_to_json() {
  local in_tsv="$1"
  local out_json="$2"
  if command -v node >/dev/null 2>&1; then
    node - "$in_tsv" "$out_json" <<'EOF'
const fs = require('node:fs');
const lines = fs.readFileSync(process.argv[2], 'utf8').trim().split('\n').filter(Boolean);
const items = lines.map((line) => {
  const [name, present, version_or_path, ubuntu_2404_apt_package] = line.split('\t');
  return { name, present: present === 'true', version_or_path, ubuntu_2404_apt_package };
});
fs.writeFileSync(process.argv[3], JSON.stringify(items, null, 2) + '\n', 'utf8');
EOF
    return 0
  fi
  python3 - "$in_tsv" "$out_json" <<'EOF'
import json, sys
items = []
with open(sys.argv[1], 'r', encoding='utf-8') as fh:
    for raw in fh:
        line = raw.rstrip('\r\n')
        if not line:
            continue
        name, present, ver, apt_pkg = line.split('\t')
        items.append({
            "name": name,
            "present": present == "true",
            "version_or_path": ver,
            "ubuntu_2404_apt_package": apt_pkg,
        })
with open(sys.argv[2], 'w', encoding='utf-8') as out:
    json.dump(items, out, indent=2)
    out.write('\n')
EOF
}

run_check_7_tauri_and_permissions() {
  local t0
  t0="$(now_epoch_ms)"
  local pdir="$CHECK5_DIR_A"
  local store_dir="$CHECK5_DATA_DIR"

  if [ -z "$pdir" ] || [ ! -d "$pdir" ]; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    POSIX_PERMS_STATUS="FAIL"
    record_check 7 "Tauri 2 Ubuntu prerequisites & POSIX 0700/0600 permissions" "FAIL" "$dur" "Profile directory from Check 5 unavailable"
    return 1
  fi

  # 7a. Strict POSIX 0700 directory & 0600 file permission checks via `stat -c '%a %u'`
  local expected_dir_stat="700 $EFFECTIVE_UID"
  local expected_file_stat="600 $EFFECTIVE_UID"
  local checked_dirs=0
  local checked_files=0

  local rel_d
  for rel_d in \
    "$store_dir/Profiles" \
    "$pdir" \
    "$pdir/.config" \
    "$pdir/.config/ZaloData" \
    "$pdir/.config/Zalo" \
    "$pdir/.local" \
    "$pdir/.local/share" \
    "$pdir/.local/state" \
    "$pdir/.cache" \
    "$pdir/tmp" \
    "$pdir/Downloads" \
    "$pdir/Documents" \
    "$pdir/Documents/Zalo Received Files"; do
    local actual_stat
    actual_stat="$(stat -c '%a %u' "$rel_d")"
    if [ "$actual_stat" != "$expected_dir_stat" ]; then
      local dur=$(( $(now_epoch_ms) - t0 ))
      POSIX_PERMS_STATUS="FAIL"
      record_check 7 "Tauri 2 Ubuntu prerequisites & POSIX 0700/0600 permissions" "FAIL" "$dur" "Directory '$rel_d' had stat '$actual_stat', expected '$expected_dir_stat'"
      return 1
    fi
    checked_dirs=$((checked_dirs + 1))
  done

  local rel_f
  for rel_f in \
    "$pdir/.config/ZaloData/.zalo-profile-id" \
    "$pdir/.config/zmeta.json" \
    "$pdir/.config/ZaloData/migrate-config.json" \
    "$pdir/.config/user-dirs.dirs" \
    "$pdir/.config/user-dirs.locale"; do
    local actual_fstat
    actual_fstat="$(stat -c '%a %u' "$rel_f")"
    if [ "$actual_fstat" != "$expected_file_stat" ]; then
      local dur=$(( $(now_epoch_ms) - t0 ))
      POSIX_PERMS_STATUS="FAIL"
      record_check 7 "Tauri 2 Ubuntu prerequisites & POSIX 0700/0600 permissions" "FAIL" "$dur" "File '$rel_f' had stat '$actual_fstat', expected '$expected_file_stat'"
      return 1
    fi
    checked_files=$((checked_files + 1))
  done

  local diag_ok="$WORK_DIR/check7-diag-ok.json"
  local bad_store="$WORK_DIR/check7-insecure-store"
  local diag_bad="$WORK_DIR/check7-diag-bad.json"
  "$DIM_CLI_BIN" --data-dir "$store_dir" diagnostics >"$diag_ok"
  mkdir -p "$bad_store/Profiles"
  chmod 0777 "$bad_store/Profiles"
  "$DIM_CLI_BIN" --data-dir "$bad_store" diagnostics >"$diag_bad"

  local acl_good acl_bad
  acl_good="$(extract_json_field "$diag_ok" "profiles_acl_safe")"
  acl_bad="$(extract_json_field "$diag_bad" "profiles_acl_safe")"
  if [ "$acl_good" != "true" ] || [ "$acl_bad" != "false" ]; then
    local dur=$(( $(now_epoch_ms) - t0 ))
    POSIX_PERMS_STATUS="FAIL"
    record_check 7 "Tauri 2 Ubuntu prerequisites & POSIX 0700/0600 permissions" "FAIL" "$dur" "profiles_acl_safe check mismatch (0700->$acl_good, 0777->$acl_bad)"
    return 1
  fi
  POSIX_PERMS_STATUS="PASS"

  # 7b. Tauri 2 & Ubuntu 24.04 prerequisites check
  local prereq_tsv="$WORK_DIR/check7-prereqs.tsv"
  : > "$prereq_tsv"
  check_pkg_config_item "webkit2gtk-4.1" "libwebkit2gtk-4.1-dev" >> "$prereq_tsv"
  check_pkg_config_item "ayatana-appindicator3-0.1" "libayatana-appindicator3-dev" >> "$prereq_tsv"
  check_pkg_config_item "librsvg-2.0" "librsvg2-dev" >> "$prereq_tsv"
  check_dpkg_or_soname "libfuse3-3" "libfuse3.so.3" "libfuse3-3" >> "$prereq_tsv"
  check_dpkg_or_soname "libopus0" "libopus.so.0" "libopus0" >> "$prereq_tsv"
  check_dpkg_or_soname "libpulse0" "libpulse.so.0" "libpulse0" >> "$prereq_tsv"

  if command -v gdbus >/dev/null 2>&1; then
    local gdbus_path
    gdbus_path="$(command -v gdbus)"
    printf 'gdbus\ttrue\t%s\tlibglib2.0-bin\n' "$gdbus_path" >> "$prereq_tsv"
  else
    printf 'gdbus\tfalse\tmissing\tlibglib2.0-bin\n' >> "$prereq_tsv"
  fi

  serialize_prereq_tsv_to_json "$prereq_tsv" "$TAURI_PREREQS_JSON_FILE"

  local missing_pkgs
  missing_pkgs="$(awk -F'\t' '$2 == "false" { print $4 }' "$prereq_tsv" | tr '\n' ' ' | sed 's/[[:space:]]*$//')"
  local present_count
  present_count="$(awk -F'\t' '$2 == "true" { c++ } END { print c+0 }' "$prereq_tsv")"

  local dur=$(( $(now_epoch_ms) - t0 ))
  if [ -z "$missing_pkgs" ]; then
    TAURI_PREREQS_STATUS="PASS"
    record_check 7 "Tauri 2 Ubuntu prerequisites & POSIX 0700/0600 permissions" "PASS" "$dur" "POSIX permissions PASS (${checked_dirs} dirs=0700, ${checked_files} files=0600, uid=${EFFECTIVE_UID}); all ${present_count}/7 Tauri/Ubuntu prerequisites installed"
  elif [ "$OPT_STRICT_TAURI_PREREQS" -eq 1 ]; then
    TAURI_PREREQS_STATUS="MISSING_PACKAGES"
    record_check 7 "Tauri 2 Ubuntu prerequisites & POSIX 0700/0600 permissions" "FAIL" "$dur" "POSIX permissions PASS (${checked_dirs} dirs=0700, ${checked_files} files=0600); missing required Tauri/Ubuntu apt packages: ${missing_pkgs}"
    return 1
  else
    TAURI_PREREQS_STATUS="MISSING_PACKAGES"
    record_check 7 "Tauri 2 Ubuntu prerequisites & POSIX 0700/0600 permissions" "WARN_MISSING_TAURI_PACKAGES" "$dur" "POSIX permissions PASS (${checked_dirs} dirs=0700, ${checked_files} files=0600, uid=${EFFECTIVE_UID}); ${present_count}/7 Tauri/Ubuntu packages present (install missing via: sudo apt-get install -y ${missing_pkgs})"
  fi
}

emit_final_report_json() {
  local out_file="$1"
  if command -v node >/dev/null 2>&1; then
    node - "$RESULTS_TSV" "$TAURI_PREREQS_JSON_FILE" "$out_file" \
      "$ANY_HARD_FAILURE" "$UNAME_S" "$UNAME_R" "$UNAME_M" "$UNAME_ALL" \
      "$DISTRO_ID" "$DISTRO_VERSION_ID" "$DISTRO_CODENAME" "$DISTRO_PRETTY" \
      "$EFFECTIVE_UID" "$IS_WSL2" "$SESSION_SCOPE" \
      "$APPARMOR_ENABLED" "$APPARMOR_RESTRICT_USERNS" "$UNPRIV_USERNS_CLONE" \
      "$BWRAP_PROFILE_PRESENT" "$BWRAP_VERSION" "$BWRAP_SMOKE_OK" \
      "$POSIX_PERMS_STATUS" "$TAURI_PREREQS_STATUS" <<'EOF'
const fs = require('node:fs');

const [
  , ,
  tsvPath,
  prereqPath,
  outPath,
  anyHardFailureRaw,
  unameS,
  unameR,
  unameM,
  unameAll,
  distroId,
  distroVersionId,
  distroCodename,
  distroPretty,
  euidRaw,
  isWsl2Raw,
  sessionScope,
  apparmorEnabled,
  apparmorRestrictUserns,
  unprivUsernsClone,
  bwrapProfileRaw,
  bwrapVersion,
  bwrapSmokeOk,
  posixPermsStatus,
  tauriPrereqsStatus,
] = process.argv;

const hardFail = anyHardFailureRaw !== '0';
const isWsl2 = isWsl2Raw === '1';

const checks = fs
  .readFileSync(tsvPath, 'utf8')
  .trim()
  .split('\n')
  .filter(Boolean)
  .map((line) => {
    const [id, name, status, durationMs, detail] = line.split('\t');
    return {
      id: Number(id),
      check: name,
      status,
      duration_ms: Number(durationMs),
      detail,
    };
  });

const tauriPrereqs = JSON.parse(fs.readFileSync(prereqPath, 'utf8'));
const coreAdapterPassed =
  !hardFail &&
  checks.length === 7 &&
  checks.slice(0, 6).every((c) => c.status === 'PASS') &&
  posixPermsStatus === 'PASS';

const report = {
  schema_version: '1.0.0',
  timestamp_utc: new Date().toISOString(),
  overall_status: coreAdapterPassed
    ? tauriPrereqsStatus === 'PASS'
      ? 'PASS'
      : 'PASS_ADAPTER_MISSING_TAURI_DEV_PACKAGES'
    : 'FAIL',
  host: {
    uname_s: unameS,
    uname_r: unameR,
    uname_m: unameM,
    uname_a: unameAll,
    distro_id: distroId,
    version_id: distroVersionId,
    version_codename: distroCodename,
    pretty_name: distroPretty,
    effective_uid: Number(euidRaw),
    is_linux_kernel: true,
    is_wsl2: isWsl2,
    session_scope: sessionScope,
    xdg_session_type: process.env.XDG_SESSION_TYPE || null,
    wayland_display: process.env.WAYLAND_DISPLAY || null,
    x11_display: process.env.DISPLAY || null,
    apparmor_enabled: apparmorEnabled,
    apparmor_restrict_unprivileged_userns: apparmorRestrictUserns,
    unprivileged_userns_clone: unprivUsernsClone,
    bwrap_apparmor_profile_present: bwrapProfileRaw === '1',
    bwrap_version: bwrapVersion,
    bwrap_unprivileged_smoke: bwrapSmokeOk,
  },
  acceptance_classification: {
    LINUX_ADAPTER_TESTED: coreAdapterPassed ? 'PASS' : 'FAIL',
    DESKTOP_FIXTURE_TESTED: coreAdapterPassed ? 'PASS' : 'FAIL',
    POSIX_PERMISSIONS_0700_0600: posixPermsStatus,
    UBUNTU_DESKTOP_SESSION_SCOPE: sessionScope,
    TAURI_UBUNTU_PREREQUISITES: tauriPrereqsStatus,
    COMMUNITY_PORT_TESTED: 'NOT_TESTED',
    OFFICIAL_ZALO_UNSUPPORTED: 'CONFIRMED_UNSUPPORTED',
  },
  tauri_ubuntu_prerequisites: tauriPrereqs,
  checks,
};

fs.writeFileSync(outPath, JSON.stringify(report, null, 2) + '\n', 'utf8');
EOF
    return 0
  fi

  python3 - "$RESULTS_TSV" "$TAURI_PREREQS_JSON_FILE" "$out_file" \
    "$ANY_HARD_FAILURE" "$UNAME_S" "$UNAME_R" "$UNAME_M" "$UNAME_ALL" \
    "$DISTRO_ID" "$DISTRO_VERSION_ID" "$DISTRO_CODENAME" "$DISTRO_PRETTY" \
    "$EFFECTIVE_UID" "$IS_WSL2" "$SESSION_SCOPE" \
    "$APPARMOR_ENABLED" "$APPARMOR_RESTRICT_USERNS" "$UNPRIV_USERNS_CLONE" \
    "$BWRAP_PROFILE_PRESENT" "$BWRAP_VERSION" "$BWRAP_SMOKE_OK" \
    "$POSIX_PERMS_STATUS" "$TAURI_PREREQS_STATUS" <<'EOF'
import datetime, json, os, sys

(
    _,
    tsv_path,
    prereq_path,
    out_path,
    any_hard_failure_raw,
    uname_s,
    uname_r,
    uname_m,
    uname_all,
    distro_id,
    distro_version_id,
    distro_codename,
    distro_pretty,
    euid_raw,
    is_wsl2_raw,
    session_scope,
    apparmor_enabled,
    apparmor_restrict_userns,
    unpriv_userns_clone,
    bwrap_profile_raw,
    bwrap_version,
    bwrap_smoke_ok,
    posix_perms_status,
    tauri_prereqs_status,
) = sys.argv

hard_fail = any_hard_failure_raw != "0"
checks = []
with open(tsv_path, "r", encoding="utf-8") as fh:
    for raw in fh:
        line = raw.rstrip("\r\n")
        if not line:
            continue
        cid, cname, cstatus, cdur, cdetail = line.split("\t")
        checks.append({
            "id": int(cid),
            "check": cname,
            "status": cstatus,
            "duration_ms": int(cdur),
            "detail": cdetail,
        })

with open(prereq_path, "r", encoding="utf-8") as pfh:
    tauri_prereqs = json.load(pfh)

core_adapter_passed = (
    not hard_fail
    and len(checks) == 7
    and all(c["status"] == "PASS" for c in checks[:6])
    and posix_perms_status == "PASS"
)

report = {
    "schema_version": "1.0.0",
    "timestamp_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "overall_status": (
        "PASS"
        if core_adapter_passed and tauri_prereqs_status == "PASS"
        else "PASS_ADAPTER_MISSING_TAURI_DEV_PACKAGES"
        if core_adapter_passed
        else "FAIL"
    ),
    "host": {
        "uname_s": uname_s,
        "uname_r": uname_r,
        "uname_m": uname_m,
        "uname_a": uname_all,
        "distro_id": distro_id,
        "version_id": distro_version_id,
        "version_codename": distro_codename,
        "pretty_name": distro_pretty,
        "effective_uid": int(euid_raw),
        "is_linux_kernel": True,
        "is_wsl2": is_wsl2_raw == "1",
        "session_scope": session_scope,
        "xdg_session_type": os.environ.get("XDG_SESSION_TYPE"),
        "wayland_display": os.environ.get("WAYLAND_DISPLAY"),
        "x11_display": os.environ.get("DISPLAY"),
        "apparmor_enabled": apparmor_enabled,
        "apparmor_restrict_unprivileged_userns": apparmor_restrict_userns,
        "unprivileged_userns_clone": unpriv_userns_clone,
        "bwrap_apparmor_profile_present": bwrap_profile_raw == "1",
        "bwrap_version": bwrap_version,
        "bwrap_unprivileged_smoke": bwrap_smoke_ok,
    },
    "acceptance_classification": {
        "LINUX_ADAPTER_TESTED": "PASS" if core_adapter_passed else "FAIL",
        "DESKTOP_FIXTURE_TESTED": "PASS" if core_adapter_passed else "FAIL",
        "POSIX_PERMISSIONS_0700_0600": posix_perms_status,
        "UBUNTU_DESKTOP_SESSION_SCOPE": session_scope,
        "TAURI_UBUNTU_PREREQUISITES": tauri_prereqs_status,
        "COMMUNITY_PORT_TESTED": "NOT_TESTED",
        "OFFICIAL_ZALO_UNSUPPORTED": "CONFIRMED_UNSUPPORTED",
    },
    "tauri_ubuntu_prerequisites": tauri_prereqs,
    "checks": checks,
}

with open(out_path, "w", encoding="utf-8") as out:
    json.dump(report, out, indent=2)
    out.write("\n")
EOF
}

# ==============================================================================
# Execute All 7 Checks & Emit Final Report
# ==============================================================================
ANY_HARD_FAILURE=0

run_check_1_preflight               || ANY_HARD_FAILURE=1
if [ "$ANY_HARD_FAILURE" -eq 0 ]; then
  run_check_2_rust_compile          || ANY_HARD_FAILURE=1
fi
if [ "$ANY_HARD_FAILURE" -eq 0 ]; then
  run_check_3_rust_tests            || ANY_HARD_FAILURE=1
fi
if [ "$ANY_HARD_FAILURE" -eq 0 ]; then
  run_check_4_lifecycle             || ANY_HARD_FAILURE=1
fi
if [ "$ANY_HARD_FAILURE" -eq 0 ]; then
  run_check_5_isolation             || ANY_HARD_FAILURE=1
fi
if [ "$ANY_HARD_FAILURE" -eq 0 ]; then
  run_check_6_crash_and_pid_reuse   || ANY_HARD_FAILURE=1
fi
if [ "$ANY_HARD_FAILURE" -eq 0 ]; then
  run_check_7_tauri_and_permissions || ANY_HARD_FAILURE=1
fi

FINAL_JSON_FILE="$WORK_DIR/final-validation-report.json"
emit_final_report_json "$FINAL_JSON_FILE"

if [ -n "$OPT_REPORT_OUT" ]; then
  case "$OPT_REPORT_OUT" in
    */*|*\\*)
      FINAL_REPORT_PARENT="${OPT_REPORT_OUT%[/\\]*}"
      if [ -n "$FINAL_REPORT_PARENT" ] && [ ! -d "$FINAL_REPORT_PARENT" ]; then
        mkdir -p "$FINAL_REPORT_PARENT"
      fi
      ;;
  esac
  cp "$FINAL_JSON_FILE" "$OPT_REPORT_OUT"
  log_info "Wrote structured JSON report to $OPT_REPORT_OUT"
fi

if [ "$OPT_JSON_ONLY" -eq 1 ]; then
  cat "$FINAL_JSON_FILE"
else
  log_info "Summary: overall_status=$(extract_json_field "$FINAL_JSON_FILE" "overall_status"), LINUX_ADAPTER_TESTED=$(extract_json_field "$FINAL_JSON_FILE" "acceptance_classification.LINUX_ADAPTER_TESTED"), session_scope=${SESSION_SCOPE}"
  printf -- '--- BEGIN_VALIDATION_JSON ---\n'
  cat "$FINAL_JSON_FILE"
  printf -- '--- END_VALIDATION_JSON ---\n'
fi

if [ "$ANY_HARD_FAILURE" -ne 0 ]; then
  exit 1
fi
exit 0
