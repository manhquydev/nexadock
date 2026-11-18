#!/usr/bin/env bash
# macOS (Apple Silicon arm64 & Intel x86_64) Cross-Platform Reliability & Contract Runner
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "ERROR: run-macos.sh must be executed on an Apple macOS host (observed: $(uname -s))." >&2
  exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CROSS_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
DIM_ROOT="$(cd "${CROSS_DIR}/../.." && pwd)"

MODE="${DIM_MODE:-baseline-audit}"
PRIORITY="${DIM_PRIORITY:-all}"
CYCLES="${DIM_SOAK_CYCLES:-100}"
METRICS_RUNS="${DIM_METRICS_RUNS:-10}"
SEED="${DIM_TEST_SEED:-0x20261117}"
SUITE="${DIM_SUITE:-all}"

cd "${DIM_ROOT}"

echo "=== [macOS Host] Preflight Toolchain Check ==="
rustc --version
cargo --version
node --version

echo "=== [macOS Host] 1/4 Building synthetic desktop fixture (xf-desktop-fixture) ==="
cargo build --manifest-path fixtures/cross-platform/Cargo.toml --release

echo "=== [macOS Host] 2/4 Building native dim-cli (--locked) ==="
cargo build -p dim-test-fixture --bin dim-cli --release --locked

echo "=== [macOS Host] 3/4 Building cross-platform reliability runner ==="
cargo build --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner

BASELINE_ARGS=()
if [[ "${DIM_OVERWRITE_BASELINE_REPORTS:-0}" == "1" ]]; then
  REPORTS_DIR="${DIM_ROOT}/plans/reports/reliability"
  mkdir -p "${REPORTS_DIR}"
  BASELINE_ARGS=(
    --overwrite-baseline-reports
    --json-out "${REPORTS_DIR}/macos-host-execution-evidence.json"
    --metrics-out "${REPORTS_DIR}/metrics-benchmark-macos.json"
    --defects-out "${REPORTS_DIR}/defect-reproductions-macos.json"
  )
fi

echo "=== [macOS Host] 4/4 Executing Cross-Platform Reliability & Contract Suites ==="
"${CROSS_DIR}/target/release/xf-test-runner" \
  --mode "${MODE}" \
  --suite "${SUITE}" \
  --priority "${PRIORITY}" \
  --cycles "${CYCLES}" \
  --metrics-runs "${METRICS_RUNS}" \
  --seed "${SEED}" \
  "${BASELINE_ARGS[@]}" \
  "$@"
