#!/usr/bin/env python3
"""Idle cost of the client frontend's endless motion in WebKitGTK (webkit2gtk-4.1).

The WebKitGTK side of run-chromium.mjs --mode client-idle (Beads zerus-5wmh.27):
the same built client frontend (client/dist, built-in replay), the same window size
and the same four 10 s windows: as displayed, with Animate working sessions off,
with Reduce motion (each through the client's own local-storage key and a reload)
and as displayed again. Each window records the process-tree CPU of this host (the
UI process, where WebKitGTK composites) and its WebKit processes, the running
animations and the rAF count over 2 s.

    ZERUS_BENCH_DISPLAY=wayland python3 client/bench/run-webkitgtk-idle.py --out FILE.json

Only through the same display rules as run-webkitgtk.sh: the real display with the
coordinator told and the session unlocked (display-info.py --require-visible), under
the arch display lock; or a private Xvfb with GDK_BACKEND=x11, WAYLAND_DISPLAY=zerus-none
and ZERUS_BENCH_XVFB=1.
"""
import argparse
import functools
import http.server
import json
import os
import sys
import threading
import time

WAYLAND = os.environ.get("WAYLAND_DISPLAY", "")
XVFB = (os.environ.get("ZERUS_BENCH_XVFB") == "1" and os.environ.get("GDK_BACKEND") == "x11"
        and WAYLAND in ("", "zerus-none"))
DISPLAY_MODE = os.environ.get("ZERUS_BENCH_DISPLAY")
REAL = (DISPLAY_MODE == "wayland" and WAYLAND not in ("", "zerus-none")) or (
    DISPLAY_MODE == "xwayland" and os.environ.get("GDK_BACKEND") == "x11" and bool(os.environ.get("DISPLAY"))
    and not os.environ.get("ZERUS_BENCH_XVFB"))
if not (XVFB or REAL):
    sys.exit("set ZERUS_BENCH_DISPLAY=wayland|xwayland for the desktop, or run under a private Xvfb (see the docstring)")

import gi  # noqa: E402

gi.require_version("Gdk", "3.0")
gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import Gdk, GLib, Gtk, WebKit2  # noqa: E402

CLOCK_TICKS = os.sysconf("SC_CLK_TCK")
VARIANTS = [
    ("as displayed", {}),
    ("Animate working sessions off", {"workspace/animateActivity": False}),
    ("Reduce motion", {"workspace/reduceMotion": True}),
    ("as displayed again", {}),
]
RUNNING = """const c = {}; for (const a of document.getAnimations()) { if (a.playState !== "running") continue;
  const n = a.animationName || a.constructor.name; c[n] = (c[n] || 0) + 1; } return c;"""
FRAMES = """return await new Promise((ok) => { let n = 0, run = true; const t = () => { if (!run) return; n++; requestAnimationFrame(t); };
  requestAnimationFrame(t); setTimeout(() => { run = false; ok(n); }, 2000); });"""
FACTS = """return { visibility: document.visibilityState, focus: document.hasFocus(), coi: crossOriginIsolated,
  window: [innerWidth, innerHeight], dpr: devicePixelRatio, ua: navigator.userAgent };"""


class IsolatedHandler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, *args):
        pass


def proc_stat(pid):
    try:
        text = open(f"/proc/{pid}/stat", encoding="utf-8", errors="replace").read()
    except OSError:
        return None
    close = text.rfind(")")
    rest = text[close + 1:].split()
    return {"name": text[text.find("(") + 1:close], "ppid": int(rest[1]), "ticks": int(rest[11]) + int(rest[12])}


def kb(path, key):
    try:
        for line in open(path, encoding="utf-8"):
            if line.startswith(key + ":"):
                return int(line.split()[1]) * 1024
    except OSError:
        pass
    return None


def tree(root):
    parents = {int(p): s["ppid"] for p in os.listdir("/proc") if p.isdigit() and (s := proc_stat(int(p)))}
    members, queue = set(), [root]
    while queue:
        pid = queue.pop()
        if pid not in members:
            members.add(pid)
            queue.extend(c for c, parent in parents.items() if parent == pid and c != pid)
    return {pid: s for pid in members if (s := proc_stat(pid))}


def role(pid, root, name):
    if pid == root:
        return "host"
    lowered = name.lower()
    if "webkitwebproces" in lowered:
        return "web"
    if "network" in lowered:
        return "network"
    if "gpu" in lowered:
        return "gpu"
    return "child"


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    here = os.path.dirname(os.path.abspath(__file__))
    parser.add_argument("--dist", default=os.path.join(here, "..", "dist"))
    parser.add_argument("--out", required=True)
    parser.add_argument("--label", default=None)
    parser.add_argument("--settle-seconds", type=float, default=8.0)
    parser.add_argument("--idle-seconds", type=float, default=10.0)
    parser.add_argument("--width", type=int, default=1440)
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--disable-feature", action="append", default=[], metavar="ID")
    args = parser.parse_args()

    display = Gdk.Display.get_default()
    display_name = display.get_name() if display else None
    display_type = type(display).__gtype__.name if display else None
    if XVFB and not (display_name and display_name.startswith(":") and display_name == os.environ.get("DISPLAY")):
        sys.exit(f"refusing: Xvfb mode but the GDK display is {display_type} {display_name}")
    mode = "xvfb" if XVFB else DISPLAY_MODE
    print(f"run-webkitgtk-idle: {mode}, GDK display {display_type} {display_name}", file=sys.stderr)

    dist = os.path.abspath(args.dist)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(IsolatedHandler, directory=dist))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{server.server_address[1]}/index.html"

    window = Gtk.Window(title="Zerus WebKitGTK idle measurement (closes itself)")
    window.set_default_size(args.width, args.height)
    view = WebKit2.WebView()
    settings = view.get_settings()
    features = WebKit2.Settings.get_all_features()
    known = {features.get(i).get_identifier(): features.get(i) for i in range(features.get_length())}
    for identifier in args.disable_feature:
        if identifier not in known:
            sys.exit(f"unknown WebKit feature {identifier}")
        settings.set_feature_enabled(known[identifier], False)
    window.add(view)
    window.show_all()

    root = os.getpid()
    state = {"windows": [], "error": None, "facts": None, "index": 0}

    def call(body, then):
        def done(webview, result, _data):
            try:
                value = webview.call_async_javascript_function_finish(result)
                then(json.loads(value.to_json(0)) if value and not value.is_undefined() else None)
            except Exception as error:  # the page failed
                state["error"] = str(error)
                Gtk.main_quit()
        view.call_async_javascript_function(body, -1, None, None, None, None, done, None)

    def sample_then(then):
        first, started = tree(root), time.monotonic()

        def second():
            now, elapsed = tree(root), time.monotonic() - started
            processes = []
            for pid, s in sorted(now.items()):
                before = first.get(pid)
                cpu = round((s["ticks"] - before["ticks"]) / CLOCK_TICKS / elapsed * 100, 3) if before else None
                processes.append({"role": role(pid, root, s["name"]), "name": s["name"], "cpu_percent": cpu,
                                  "rss_bytes": kb(f"/proc/{pid}/status", "VmRSS"),
                                  "pss_bytes": kb(f"/proc/{pid}/smaps_rollup", "Pss")})
            by_role = {}
            for p in processes:
                r = by_role.setdefault(p["role"], {"processes": 0, "cpu_percent": 0.0, "rss_bytes": 0, "pss_bytes": 0})
                r["processes"] += 1
                r["cpu_percent"] = round(r["cpu_percent"] + (p["cpu_percent"] or 0), 3)
                r["rss_bytes"] += p["rss_bytes"] or 0
                r["pss_bytes"] += p["pss_bytes"] or 0
            then({"window_ms": round(elapsed * 1000), "total": {
                "processes": len(processes), "cpu_percent": round(sum(p["cpu_percent"] or 0 for p in processes), 3),
                "rss_bytes": sum(p["rss_bytes"] or 0 for p in processes),
                "pss_bytes": sum(p["pss_bytes"] or 0 for p in processes)}, "by_role": by_role, "processes": processes})
            return False

        GLib.timeout_add(int(args.idle_seconds * 1000), second)

    def run_variant():
        if state["index"] >= len(VARIANTS):
            Gtk.main_quit()
            return
        what, values = VARIANTS[state["index"]]
        record = {"what": what, "settings": values}

        def after_sample(sample):
            record["sample"] = sample

            def facts(value):
                record["page"] = {k: v for k, v in (value or {}).items() if k != "ua"}
                state["windows"].append(record)
                roles = ", ".join(f"{r} {v['cpu_percent']:.1f}%" for r, v in sample["by_role"].items())
                print(f"run-webkitgtk-idle: {what}: {sample['total']['cpu_percent']:.1f}% CPU ({roles}), "
                      f"PSS {sample['total']['pss_bytes'] // 1048576} MiB; animations {json.dumps(record['running_animations'])}; "
                      f"rAF {record['raf_frames_2s']} in 2 s; {record['page'].get('visibility')}", file=sys.stderr)
                state["index"] += 1
                set_and_reload()
            call(FACTS, facts)

        def measure():
            def got_animations(value):
                record["running_animations"] = value

                def got_frames(frames):
                    record["raf_frames_2s"] = frames
                    GLib.timeout_add(1000, lambda: (sample_then(after_sample), False)[1])
                call(FRAMES, got_frames)
            call(RUNNING, got_animations)
            return False

        GLib.timeout_add(int(args.settle_seconds * 1000), measure)

    def set_and_reload():
        if state["index"] >= len(VARIANTS):
            Gtk.main_quit()
            return
        _what, values = VARIANTS[state["index"]]
        call(f"localStorage.setItem('zerus-client-settings', {json.dumps(json.dumps(values))}); return true;",
             lambda _v: (state.update(reloading=True), view.reload_bypass_cache()))

    def loaded(webview, event):
        if event != WebKit2.LoadEvent.FINISHED:
            return
        if state["facts"] is None:
            def first_facts(value):
                state["facts"] = value
                set_and_reload()
            call(FACTS, first_facts)
        elif state.pop("reloading", False):
            run_variant()

    view.connect("load-changed", loaded)
    view.load_uri(url)
    GLib.timeout_add(600 * 1000, lambda: (state.update(error="timeout"), Gtk.main_quit()))
    Gtk.main()
    server.shutdown()
    window.destroy()
    if state["error"]:
        sys.exit(f"measurement failed: {state['error']}")
    facts = state["facts"] or {}
    report = {
        "schema": "zerus-idle-v1", "created_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "platform": {"label": args.label or f"{mode}-webkitgtk-client-idle", "representative": not XVFB,
                     "user_agent": facts.get("ua"), "cross_origin_isolated": facts.get("coi"), "dpr": facts.get("dpr"),
                     "window": facts.get("window"),
                     "engine": f"WebKitGTK {WebKit2.get_major_version()}.{WebKit2.get_minor_version()}.{WebKit2.get_micro_version()}",
                     "webkit": {"display": mode, "gdk_backend": os.environ.get("GDK_BACKEND"),
                                "features_disabled": args.disable_feature}},
        "harness": "client frontend (built-in replay) in client/bench/run-webkitgtk-idle.py",
        "windows": state["windows"],
    }
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(report, handle, indent=2)
        handle.write("\n")


if __name__ == "__main__":
    main()
