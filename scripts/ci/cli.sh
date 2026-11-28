#!/usr/bin/env bash
# Run credential-free CLI checks. Every terminal fixture owns a private socket.
set -euo pipefail
cd "$(dirname "$0")/../.."
phase=${1:-all}
case "$phase" in all|debug|remaining) ;; *) echo "Unknown CLI phase" >&2; exit 2 ;; esac
mkdir -p artifacts/test-results
# Apple's TMPDIR has a /var alias and can exceed native Unix socket limits.
# Allocate one short private canonical fixture root. Never remove retained
# fixture data on exit: only an empty root is retired; VM disposal owns the rest.
if [ "$(uname -s)" = Darwin ]; then
    cli_fixture_tmp="$(python3 -c 'from pathlib import Path; import tempfile; print(Path(tempfile.mkdtemp(prefix="zerus-cli-", dir="/tmp")).resolve(strict=True))')"
    trap 'rmdir "$cli_fixture_tmp" 2>/dev/null || true' EXIT
    export TMPDIR="$cli_fixture_tmp"
fi
# Tests launched from an agent must not inherit its account/run selection.
while IFS= read -r variable; do
    case "$variable" in HGS_*) unset "$variable" ;; esac
done < <(compgen -e)
unset HGS_CLAUDE_TEST_BIN HGS_NATIVE_RECOVERY HGS_TEST_LEGACY_STATE HGS_TEST_LEGACY_BIN
python3 - <<'PY'
import re, subprocess
version = subprocess.check_output(['tmux', '-V'], text=True).strip()
match = re.match(r'tmux (\d+)\.(\d+)', version)
if not match or tuple(map(int, match.groups())) < (3, 7):
    raise SystemExit('Full CLI integration checks require tmux 3.7+ for verified input-mode inspection: ' + version)
print('Terminal fixture toolchain: ' + version)
PY
if [ "$phase" != remaining ]; then
if [ "$(uname -s)" = Linux ]; then
    # zerus-5sl7r: the session resume runtime runs its real resume and send
    # children from its pinned image against a private tmux and the native
    # stand-in agent. It runs before the full suite, so an unrelated failure
    # there never hides it.
    cargo build --locked
    cargo build --locked --example fake_agent
    HGS_TEST_BIN="$PWD/target/debug/hgs" cargo test --locked --bin hgs \
        desktop::session_resume::tests::actor::real_cli::real_cli_resume_from_the_pinned_image_starts_its_supervisor_and_sends \
        -- --ignored --exact --test-threads=1 2>&1 | tee artifacts/test-results/session-resume-real.log
fi
cargo test --locked 2>&1 | tee artifacts/test-results/rust.log
cargo build --locked
# These exact, one-shot tests use the separately built CLI for metadata
# reconciliation. Never run the libtest image as an hgs helper or run the
# private crash leaf on its own.
for native_case in \
    real_cli_recover_only_reconciles_the_original_receipt_and_never_falls_back_to_terminal_input \
    real_cli_original_child_kill_and_revoke_never_release_input_or_replay_after_reopen; do
    HGS_TEST_BIN="$PWD/target/debug/hgs" cargo test --locked --bin hgs \
        "desktop::task_start_reconcile::tests::$native_case" \
        -- --ignored --exact --test-threads=1
done
if [ "$(uname -s)" = Linux ]; then
    HGS_TEST_BIN="$PWD/target/debug/hgs" cargo test --locked --bin hgs \
        desktop::task_start_reconcile::tests::real_cli_actor_sigkill_keeps_original_custody_and_restart_never_reconstructs_consent \
        -- --ignored --exact --test-threads=1
fi
# The native stand-in agent of tests/test_fake_agent.py, tests/test_backend_contract.py
# and later terminal suites.
cargo build --locked --example fake_agent
fi
if [ "$phase" != debug ]; then
export HGS_TEST_BIN="$PWD/target/debug/hgs"
bash tests/test_hgs.sh 2>&1 | tee artifacts/test-results/cli-smoke.log
# Cold fixture compilation belongs to preparation, not a parallel test worker's
# 240 s setup budget. Its own target directory keeps the verified debug CLI intact.
cargo +1.99.1 test --locked --test voice_recovery_fixture --no-run \
    --target-dir target/speech-recovery --message-format=json \
    > artifacts/test-results/speech-recovery-build.jsonl
HGS_TEST_SPEECH_RECOVERY_BIN=$(python3 scripts/ci/speech-recovery-executable.py \
    artifacts/test-results/speech-recovery-build.jsonl)
export HGS_TEST_SPEECH_RECOVERY_BIN
python3 scripts/ci/python-tests.py 2>&1 | tee artifacts/test-results/python.log
cargo build --release --locked
if [ "$phase" = all ]; then
    # zerus-63rru.182: the Teams contracts' person fixture is test-only. The debug and
    # release hgs never hold it; the unit-test executable does (the positive control).
    cargo test --locked --no-run --bin hgs --message-format=json \
        > artifacts/test-results/unit-tests-build.jsonl
    python3 scripts/ci/check-no-person-fixture.py --absent target/debug/hgs target/release/hgs \
        --cargo-messages artifacts/test-results/unit-tests-build.jsonl \
        | tee artifacts/test-results/person-fixture-check.json
fi
printf 'CLI checks and release build passed.\n'
fi
