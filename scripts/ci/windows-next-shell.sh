#!/usr/bin/env bash
# Disposable Windows runner only. The tests use private pipes and registry keys.
set -euo pipefail
cd "$(dirname "$0")/../.."
export RUSTUP_TOOLCHAIN=1.99.1
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target/client-native}"
on_exit() {
    status=$?
    if [ "$status" -ne 0 ]; then
        python scripts/ci/windows-shell-loader.py --target "$CARGO_TARGET_DIR" \
            --output artifacts/windows-shell-loader || true
    fi
    exit "$status"
}
trap on_exit EXIT
python scripts/ci/windows-desktop-registration-tests.py
# The service/relay binary and native tests come from this exact checkout.
cargo build --locked --bin hgs
cargo test --locked desktop::ui_authority
for selected in windows_shell::tests instance::tests platform::windows::startup::tests platform::windows::drawing::tests cli::tests lifecycle::tests window_layer::tests; do
    cargo test --locked --manifest-path client/src-tauri/Cargo.toml --lib "$selected"
done
cargo test --locked --manifest-path client/src-tauri/Cargo.toml --lib --no-run --message-format=json > "$CARGO_TARGET_DIR/windows-native-tests.jsonl"
export HGS_TEST_BIN
HGS_TEST_BIN=$(cygpath -w "$CARGO_TARGET_DIR/debug/hgs.exe")
export HGS_WINDOWS_FRONTEND_FIXTURE
HGS_WINDOWS_FRONTEND_FIXTURE=$(python - "$CARGO_TARGET_DIR/windows-native-tests.jsonl" <<'PYTHON'
import json, sys
from pathlib import Path
executables = []
for line in Path(sys.argv[1]).read_text(encoding='utf-8').splitlines():
    value = json.loads(line)
    if value.get('reason') == 'compiler-artifact' and value['target']['name'] == 'zerus_client_lib' and value['profile']['test'] and value.get('executable'):
        executables.append(value['executable'])
if len(executables) != 1:
    raise SystemExit('one exact native frontend test executable required')
print(executables[0])
PYTHON
)
python -m unittest discover -s tests -p windows_frontend_auth.py -v
npm --prefix client ci --ignore-scripts --no-audit --no-fund
(cd client && npm run tauri -- build --debug --no-bundle)
cargo build --locked --bin hgs
python tests/test_windows_desktop_registration.py --hgs "$CARGO_TARGET_DIR/debug/hgs.exe"
mkdir -p artifacts/zerus-next-windows-shell
cp "$CARGO_TARGET_DIR/debug/hgs-tray.exe" "$CARGO_TARGET_DIR/debug/hgs.exe" artifacts/zerus-next-windows-shell/
# Keep exact native fixture bytes/drivers for the same private real-PC checks.
cp "$HGS_WINDOWS_FRONTEND_FIXTURE" artifacts/zerus-next-windows-shell/windows-frontend-tests.exe
mkdir -p artifacts/zerus-next-windows-shell/native-tests
cp tests/windows_frontend_auth.py tests/test_desktop.py artifacts/zerus-next-windows-shell/native-tests/
