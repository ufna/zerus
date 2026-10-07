#!/usr/bin/env bash
# Build and run all Qt suites sequentially to keep desktop fixtures independent.
set -euo pipefail
cd "$(dirname "$0")/../.."
mkdir -p artifacts/test-results
gui_build="${ZERUS_GUI_BUILD_DIR:-$PWD/.ci-build/gui}"
cmake_args=(-S tray -B "$gui_build" -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON)
if [ "$(uname -s)" = Darwin ]; then
    cmake_args+=("-DCMAKE_PREFIX_PATH=$(brew --prefix qt)")
fi
if command -v ccache >/dev/null 2>&1; then
    cmake_args+=(-DCMAKE_C_COMPILER_LAUNCHER=ccache -DCMAKE_CXX_COMPILER_LAUNCHER=ccache)
fi
cmake "${cmake_args[@]}"
cmake --build "$gui_build" --parallel "${ZERUS_BUILD_JOBS:-2}"
export QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software
export QTWEBENGINE_CHROMIUM_FLAGS=--disable-gpu
ctest --test-dir "$gui_build" --parallel 1 --timeout 240 --output-on-failure \
    --output-junit "$PWD/artifacts/test-results/qt.xml" 2>&1 | tee artifacts/test-results/qt.log
