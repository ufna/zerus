#!/usr/bin/env bash
# Runs the client's pacing scenarios (or the client's idle windows) in a Chromium
# engine through run-chromium.mjs. Build the page first:
#   (cd client && npx vite build --config bench/harness/vite.config.ts)   # --mode harness
#   (cd client && npm run build)                                          # --mode client, client-idle
#   client/bench/run-chromium.sh --engine chrome --hz 144 --out FILE.json [--mode ...]
#   client/bench/run-chromium.sh --engine cefsimple --binary DIR/Release/cefsimple ...
#
# ZERUS_BENCH_DISPLAY selects where the window goes:
#   xvfb      (default) a private Xvfb, never the desktop; no vsync, so Chromium
#             paces at its own 60 Hz
#   wayland   the desktop's compositor, --ozone-platform=wayland
#   xwayland  the desktop through XWayland, --ozone-platform=x11
# The real-display modes only with the coordinator told first: they refuse while
# the session is locked or a display sleeps, hold the arch display lock (two
# measurements at once invalidate each other) and add the display modes to --out.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
lock="${ZERUS_BENCH_DISPLAY_LOCK:-/tmp/zerus-arch-display.lock}"

out=""
args=("$@")
for ((i = 0; i < ${#args[@]}; i++)); do
    [[ "${args[i]}" == --out ]] && out="${args[i + 1]:-}"
done

case "${ZERUS_BENCH_DISPLAY:-xvfb}" in
xvfb)
    exec env -u DISPLAY -u WAYLAND_SOCKET -u DBUS_SESSION_BUS_ADDRESS WAYLAND_DISPLAY=zerus-none GDK_BACKEND=x11 \
        QT_QPA_PLATFORM=offscreen ZERUS_BENCH_XVFB=1 \
        xvfb-run -a -s "-screen 0 1440x900x24" \
        node "$here/run-chromium.mjs" --display xvfb "$@"
    ;;
wayland | xwayland)
    mode="${ZERUS_BENCH_DISPLAY}"
    [[ -n "${WAYLAND_DISPLAY:-}" && -n "${DISPLAY:-}" ]] || { echo "the desktop's WAYLAND_DISPLAY and DISPLAY are needed" >&2; exit 2; }
    python3 "$here/display-info.py" --require-visible
    if [[ "$mode" == wayland ]]; then
        flock -w 1800 "$lock" env -u ZERUS_BENCH_XVFB node "$here/run-chromium.mjs" --display wayland "$@"
    else
        flock -w 1800 "$lock" env -u ZERUS_BENCH_XVFB GDK_BACKEND=x11 node "$here/run-chromium.mjs" --display xwayland "$@"
    fi
    if [[ -n "$out" ]]; then python3 "$here/display-info.py" --attach "$out"; fi
    ;;
*)
    echo "ZERUS_BENCH_DISPLAY must be xvfb, wayland or xwayland" >&2
    exit 2
    ;;
esac
