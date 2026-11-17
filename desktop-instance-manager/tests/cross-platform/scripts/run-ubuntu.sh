#!/usr/bin/env bash
# Ubuntu 22.04 / 24.04 LTS Cross-Platform Reliability & Contract Runner
set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "ERROR: run-ubuntu.sh must be executed on a Linux / Ubuntu host (observed: $(uname -s))." >&2
  exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CROSS_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
DIM_ROOT="$(cd "${CROSS_DIR}/../.." && pwd)"

CYCLES="${DIM_SOAK_CYCLES:-100}"
METRICS_RUNS="${DIM_METRICS_RUNS:-10}"
SEED="${DIM_TEST_SEED:-0x20261117}"
SUITE="${DIM_SUITE:-all}"

cd "${DIM_ROOT}"

echo "=== [Ubuntu Host] Preflight Toolchain & Namespace Sandbox Check ==="
rustc --version
cargo --version
node --version
if command -v bwrap >/dev/null 2>&1; then
  echo "[INFO] bwrap detected: $(bwrap --version)"
else
  echo "[INFO] bwrap not installed; testing XDG/HOME fallback mode"
fi
if [[ -r /proc/sys/kernel/apparmor_restrict_unprivileged_userns ]]; then
  echo "[INFO] kernel.apparmor_restrict_unprivileged_userns = $(cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns)"
fi

echo "=== [Ubuntu Host] 1/4 Building synthetic desktop fixture (xf-desktop-fixture) ==="
cargo build --manifest-path fixtures/cross-platform/Cargo.toml --release

echo "=== [Ubuntu Host] 2/4 Building native dim-cli (--locked) ==="
cargo build -p dim-test-fixture --bin dim-cli --release --locked

echo "=== [Ubuntu Host] 3/4 Building cross-platform reliability runner ==="
cargo build --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner

REPORTS_DIR="${DIM_ROOT}/plans/reports/reliability"
mkdir -p "${REPORTS_DIR}"

echo "=== [Ubuntu Host] 4/4 Executing Cross-Platform Reliability & Contract Suites ==="
"${CROSS_DIR}/target/release/xf-test-runner" \
  --suite "${SUITE}" \
  --cycles "${CYCLES}" \
  --metrics-runs "${METRICS_RUNS}" \
  --seed "${SEED}" \
  --json-out "${REPORTS_DIR}/ubuntu-host-execution-evidence.json" \
  --metrics-out "${REPORTS_DIR}/metrics-benchmark-ubuntu.json" \
  --defects-out "${REPORTS_DIR}/defect-reproductions-ubuntu.json" \
  "$@"
