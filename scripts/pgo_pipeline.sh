#!/usr/bin/env bash
# ==============================================================================
# Viernes Voice Assistant - Profile-Guided Optimization (PGO) & Fat LTO Pipeline
# Aligned with HU-RUST-19 & migrate-to-rust skill standards.
# ==============================================================================

set -euo pipefail

WORKSPACE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PGO_DATA_DIR="${WORKSPACE_DIR}/pgo-data"
LLVM_PROFDATA="$(which llvm-profdata || find "$(rustc --print sysroot)" -name llvm-profdata 2>/dev/null || true)"

if [[ -z "${LLVM_PROFDATA}" ]]; then
    echo "ERROR: llvm-profdata tool not found in PATH or rust sysroot!"
    exit 1
fi

echo "======================================================================"
echo " Starting PGO + Fat LTO Compilation Pipeline"
echo " Workspace:     ${WORKSPACE_DIR}"
echo " PGO Data:      ${PGO_DATA_DIR}"
echo " LLVM Profdata: ${LLVM_PROFDATA}"
echo "======================================================================"

cd "${WORKSPACE_DIR}"

# 1. Clean previous profile artifacts
echo -e "\n[1/5] Cleaning previous profile artifacts..."
rm -rf "${PGO_DATA_DIR}"
mkdir -p "${PGO_DATA_DIR}"

# 2. Build instrumented binary with -Cprofile-generate
echo -e "\n[2/5] Compiling instrumented release binary (-Cprofile-generate)..."
RUSTFLAGS="-Cprofile-generate=${PGO_DATA_DIR}" cargo build --release -p viernes-cli

# 3. Execute representative profiling workload
echo -e "\n[3/5] Executing representative profiling workload..."
./target/release/viernes --check-config
./target/release/viernes --list-devices
./target/release/viernes --test-voice
./target/release/viernes --benchmark

# Run additional benchmark iterations to record deep hot-loop execution profiles
for i in {1..3}; do
    echo "  -> Training iteration $i/3..."
    ./target/release/viernes --benchmark > /dev/null
done

# 4. Merge collected profiling profiles
echo -e "\n[4/5] Merging profile data using llvm-profdata..."
"${LLVM_PROFDATA}" merge -o "${PGO_DATA_DIR}/merged.profdata" "${PGO_DATA_DIR}"

# 5. Compile final optimized binary with -Cprofile-use and Fat LTO
echo -e "\n[5/5] Compiling final PGO-optimized binary with Fat LTO..."
RUSTFLAGS="-Cprofile-use=${PGO_DATA_DIR}/merged.profdata -Cllvm-args=-pgo-warn-missing-function" cargo build --release -p viernes-cli

echo -e "\n======================================================================"
echo " PGO + Fat LTO Pipeline Completed Successfully!"
ls -lh "${WORKSPACE_DIR}/target/release/viernes"
echo "======================================================================"
