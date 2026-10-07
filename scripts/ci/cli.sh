#!/usr/bin/env bash
# Run credential-free CLI checks. Every terminal fixture owns a private socket.
set -euo pipefail
cd "$(dirname "$0")/../.."
mkdir -p artifacts/test-results
# Tests launched from an agent must not inherit its account/run selection.
while IFS= read -r variable; do
    case "$variable" in HGS_*) unset "$variable" ;; esac
done < <(compgen -e)
unset HGS_CLAUDE_TEST_BIN HGS_NATIVE_RECOVERY HGS_TEST_LEGACY_STATE HGS_TEST_LEGACY_BIN
cargo test --locked 2>&1 | tee artifacts/test-results/rust.log
cargo build --locked
export HGS_TEST_BIN="$PWD/target/debug/hgs"
bash tests/test_hgs.sh 2>&1 | tee artifacts/test-results/cli-smoke.log
python3 scripts/ci/python-tests.py 2>&1 | tee artifacts/test-results/python.log
cargo build --release --locked
printf 'CLI checks and release build passed.\n'
