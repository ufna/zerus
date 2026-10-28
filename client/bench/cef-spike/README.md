# Tauri on CEF (Chromium) on Linux: the spike

Beads `zerus-5wmh.27`, 2026-10-28. This branch keeps the spike that checks whether the
client can run on Chromium (CEF) instead of WebKitGTK on Linux, to pace at the display
rate. It is not meant to merge. The research runs beside feature work (owner,
2026-10-28): features and releases do not wait for it, and the Linux client stays on
WebKitGTK meanwhile, as does the WebKitGTK patch work (`zerus-5wmh.28`).

| Path | What it is |
|---|---|
| `app/` | the smallest Tauri 3 app on the CEF runtime: loads `ZERUS_SPIKE_URL` (the pacing harness) or the client `dist` through the asset protocol, with DevTools on 127.0.0.1 |
| `patches/zerus-client-cef-port.diff` | our `client/src-tauri` ported to the CEF runtime (applies to main at `1f388605`) |
| `patches/tauri-16189-idle-spin.diff` | the fix its reporter proposed in tauri-apps/tauri#16189 (the runtime's main thread spins at 100% while idle on Linux), copied from the issue, against `crates/tauri-runtime-cef` |
| `../run-chromium.mjs`, `../run-chromium.sh` | the client pacing scenarios and idle windows in Chrome, `cefsimple` or this app, over the DevTools protocol |
| `../run-webkitgtk-idle.py` | the same idle windows of the client frontend in WebKitGTK |
| `../results/arch-kde-cef-2026-10-28/` | the real-display results on arch |

## Where Tauri's CEF work lives

Checked with the GitHub API on 2026-10-28:

- Branch `v3` (`78f49ba`, 2026-10-21): `tauri 3.0.0-alpha.5`, `tauri-runtime-cef
  3.0.0-alpha.6` (released 2026-10-14), pinned to `cef =154.5.0`, which is CEF
  154.0.34 with Chromium 154.0.8037.98. All active Linux CEF pull requests target it:
  #16294 (window dragging and resizing), #16352 (focus), #16210 (switches for child
  processes), #16242 (window class), #16338 (bundler); OSR (#16225) merged on 2026-10-28.
- Branch `feat/cef` (`9d1a5b1`, 2026-10-07) is the older line: `tauri 2.11.5` with
  `tauri-runtime-cef 0.1.0` pinned to `cef =151.8.1`. `v3` contains all of it except
  one macOS fix (#16048).
- `dev` (2.12.x) has no CEF.

CEF itself builds only even Chromium milestones; M156 (CEF 156.0.14) is current, and
CEF no longer updates M154. `cef-rs` already publishes 156.2.0.

## What the runtime does on Linux

- It forces X11: `--ozone-platform=x11` and `GDK_BACKEND=x11`
  (`crates/tauri-runtime-cef/src/runtime.rs`, lines 3326 and 3336 at `78f49ba`). On a
  Wayland session the app runs through XWayland. CEF upstream has no native Wayland
  embedding (chromiumembedded/cef#2804).
- Pages render in windowed CEF child X11 windows, under an X11 host window placed in a
  GTK 4 window. Off-screen rendering is opt-in.
- `invoke` is a `fetch` to `http://ipc.localhost/<command>`, answered by a CEF scheme
  handler on the main thread. Raw `tauri::ipc::Response` bodies arrive as `ArrayBuffer`,
  and every response header passes, so COOP and COEP give `crossOriginIsolated` (timer
  step 5 µs) on `http://tauri.localhost/`.
- Open Linux issues that matter to the client: #16189 (idle main thread at 100%; the
  reporter's patch, copied here, fixes it), #16251 and #16351 (keyboard input lost at start or after a
  focus change), #16249 (browser accelerators swallow keys), #16205 (`visible(false)`
  ignored), #16131 (drag and drop). Fractional scales are rounded to integers.
- A Chrome-style profile picks up system-wide Chromium external extensions: on KDE it
  installed Plasma Browser Integration from `/usr/share/chromium/extensions`, started
  `plasma-browser-integration-host` and made outbound HTTPS requests, even with
  `--disable-extensions`. A production profile needs hardening.

## The client port

`patches/zerus-client-cef-port.diff` changes four things and the manifest:

1. `main.rs`: `#[tauri_runtime_cef::cef_entry_point]` (the executable is also
   Chromium's renderer, GPU and utility process).
2. `lib.rs`: `.runtime(cef_runtime())`, with the DevTools port and profile from
   `ZERUS_SPIKE_DEBUG_PORT` and `ZERUS_SPIKE_CACHE_DIR`; `ZERUS_SPIKE_URL` loads an
   http page instead of the app.
3. `display.rs`: the GTK 4 API (`surface()`, `monitor_at_surface`).
4. `Cargo.toml`: `tauri`, `tauri-build` and `tauri-runtime-cef` from `v3`, and `gtk4`
   instead of `gtk 0.18`. `tauri.conf.json` gets its own identifier.

Everything else compiled unchanged: commands, actions, `ipc::Response`, `State`, metrics,
`zerus-client-core` and `@tauri-apps/api` 2.12.4. Build with
`--features tauri/custom-protocol`, or `cargo build` loads the dev server URL. Still
missing for a real port: Chromium process roles in `metrics.rs`, profile hardening, the
Tauri CLI and npm CLI on v3.

Keeping macOS on WKWebView means one of:

- the whole client on Tauri 3 alpha, with macOS on `tauri-runtime-wry` (still
  WKWebView; `with_webview_configuration` moved to a wry extension trait in v3), every
  plugin and the CLI on v3 alpha as well; or
- two Tauri majors in one crate behind `cfg`, which Cargo allows only through renamed
  dependencies and duplicated glue.

## Results on the arch display (2026-10-28)

`../results/arch-kde-cef-2026-10-28/`, two 144 Hz outputs at scale 1, RTX 3090 Ti with
nvidia 615.78.08 (`vblank=N`), T = 6.94 ms, worst of three 10 s runs:

| Engine and path | Pacing scenarios passing criterion 1 | Means |
|---|---|---|
| WebKitGTK 2.54.1, Wayland (reference, `arch-kde-webkitgtk-harness-2026-10-28.json`) | 0 of 5 | 12.7-16.1 ms |
| Tauri v3 + CEF 154 spike, XWayland | 5 of 5 | 6.95-6.96 ms |
| cefsimple CEF 156, native X11 parent, XWayland | 5 of 5 | 6.95-6.98 ms |
| cefsimple CEF 156, Views, native Wayland | 4 of 4 run | 6.94-6.95 ms |
| Chrome 155, native Wayland | 4 of 5 | 6.94-7.05 ms |
| our client on CEF, live data | 3 of 5 (two slow-frame tails) | 6.95-7.39 ms |
| our client on WebKitGTK, live data | 0 of 5 | 16.07-17.73 ms |

Idle of the two clients with live data, back to back (`*-client-perf.json`, CPU of the
process tree, one core = 100%): as displayed WebKitGTK 50.4% (13 endless animations) and
CEF 22.2% (15); Animate working sessions off 7.9% and 10.7%; Reduce motion 10.2% and
11.5%. `hgs` takes 6-7% of each. On CEF hiding the window left the page visible
(tauri#16205). The Chrome XWayland pacing run and the CEF and Chrome replay-page idle runs
were cut off when the session locked.

## Keyboard and focus (automated, Xvfb)

`keycheck/keycheck.py` replays eight steps through XTEST in a private Xvfb with no window
manager: Ctrl+2 and Ctrl+1 right after start, Ctrl+, and Ctrl+2, the zoom keys, a click
into the session search with typing and a layout switch (`setxkbmap ru`), switching to
another window and back without a click, clicking away and back, the list arrows, and the
zoom keys and Ctrl+1 with the focus inside the page.
Window activation is emulated the way an ICCCM window manager does it (`XSetInputFocus`,
`WM_TAKE_FOCUS`, `_NET_ACTIVE_WINDOW`); a decoy window holds the focus before the client
starts. `keycheck/keylog.js`, added to a copy of `client/dist`, reports keys, focus and the
visible page through the client's `perf_log`, so both builds are observed the same way.

Results on 2026-10-28 (replay data):

| Build | Steps passed: four runs of seven steps, then one of eight |
|---|---|
| Tauri 2, WebKitGTK 2.54.1 | 7, 7, 7, 7, then 8 of 8 |
| Tauri v3, CEF 154 (alpha.6 with the #16189 fix) | 2, 2, 2, then 3 of 8 |
| the same plus tauri PR #16352 (`patches/tauri-pr-16352-focus.diff`) | 4, 3, 0, 4, 4, then 5 of 8 |

On CEF, keys reach the page only after a click inside it; after any activation of the
window (at start and when switching back) every key is lost (tauri#16251, #16351). PR
#16352 brings back input after switching windows in most runs, but not right after start,
and one run lost all input even after clicks. With the focus inside the page, Ctrl+=,
Ctrl+-, Ctrl+0 and Ctrl+digit reach the client, so #16249 did not affect it here.
Limits: no KWin, no real Alt+Tab, Xvfb instead of XWayland; the owner's desktop needs a
short manual check once a fix lands.

## Upstream

Posted from the owner's account on 2026-10-28: the key-check data on PR #16352, #16251
and #16351, and a confirmation of the reporter's patch on #16189 asking them to open a PR.
Tauri's AI Tool Policy (CONTRIBUTING) requires the submitter to review and test any
LLM-generated PR content personally and not to answer review comments with AI, and asks
for signed commits; a PR from this spike needs the owner, not an agent.

## Building

```sh
# the client frontend, then the spike app (12 jobs at most on shared machines)
(cd client && npm ci && npm run build)
CARGO_BUILD_JOBS=12 cargo build --release --manifest-path client/bench/cef-spike/app/Cargo.toml
```

`cef-dll-sys` downloads the CEF "minimal" archive for its exact version from
`cef-builds.spotifycdn.com` and checks its SHA-1 against `index.json` from the same host.
To use an archive verified beforehand, set `CEF_PATH` to a directory holding it (see the
crate's `build.rs`). For the idle fix, clone tauri at the same commit, apply
`patches/tauri-16189-idle-spin.diff` in `crates/tauri-runtime-cef`, and add

```toml
[patch."https://github.com/tauri-apps/tauri"]
tauri-runtime-cef = { path = "<clone>/crates/tauri-runtime-cef" }
```

The client port: `git apply client/bench/cef-spike/patches/zerus-client-cef-port.diff`,
then `cargo build --release --features tauri/custom-protocol` in `client/src-tauri`.

## Measuring

```sh
# real display, only with the coordinator told; the scripts hold the arch display lock
ZERUS_BENCH_DISPLAY=xwayland client/bench/run-chromium.sh --engine spike \
    --binary client/bench/cef-spike/app/target/release/zerus-cef-spike \
    --mode harness --dist client/bench/build/harness --hz 144 --out FILE.json
ZERUS_BENCH_DISPLAY=wayland client/bench/run-chromium.sh --engine chrome --mode harness ...
ZERUS_BENCH_DISPLAY=wayland client/bench/run-chromium.sh --engine chrome --mode client-idle --dist client/dist ...
ZERUS_BENCH_DISPLAY=wayland python3 client/bench/run-webkitgtk-idle.py --dist client/dist --out FILE.json
```

`--engine cefsimple --binary DIR/Release/cefsimple` runs the official CEF sample
(`cef_binary_<version>_linux64_client.tar.bz2`; verify its SHA-1 against `index.json`
and the `.sha1` file). `--arg=--use-native` gives it a native X11 parent window, the
mode Tauri uses. The ported client runs its own measurement with
`ZERUS_CLIENT_PERF=auto` (see `client/README.md`).
