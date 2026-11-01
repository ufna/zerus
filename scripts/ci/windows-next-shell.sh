#!/usr/bin/env bash
# Disposable Windows runner only. The tests use private pipes and registry keys.
set -euo pipefail
cd "$(dirname "$0")/../.."
export RUSTUP_TOOLCHAIN=1.99.1
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target/client-native}"
for selected in windows_shell::tests instance::tests platform::windows::startup::tests platform::windows::drawing::tests cli::tests lifecycle::tests; do
    cargo test --locked --manifest-path client/src-tauri/Cargo.toml --lib "$selected"
done
npm --prefix client ci --ignore-scripts --no-audit --no-fund
(cd client && npm run tauri -- build --debug --no-bundle)
cargo build --locked --bin hgs
mkdir -p artifacts/zerus-next-windows-shell
cp "$CARGO_TARGET_DIR/debug/hgs-tray.exe" "$CARGO_TARGET_DIR/debug/hgs.exe" artifacts/zerus-next-windows-shell/
