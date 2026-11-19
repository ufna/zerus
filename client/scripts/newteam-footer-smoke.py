#!/usr/bin/python3
"""zerus-26ltc: New team's footer actions and long refusals in real WebKitGTK.

Loads the Vite build of the sibling newteam-footer-smoke.html once per case,
each in a fresh window of the case's size with the case's page zoom (150 percent
text is zoom 1.5), in English and Russian, with no, a short and a long refusal.
The page measures the production form and posts its checks; this driver saves
one snapshot and the results.

Requires Python GI, GTK3/WebKit2 4.1 and Xvfb. Run it under a private display
(for example `xvfb-run -a -s "-screen 0 1600x1000x24"`) with DISPLAY and
DBUS_SESSION_BUS_ADDRESS cleared and GDK_BACKEND=x11; never use the person's
display. --build and --output are caller-owned directories. No native IPC or
product callback runs.
"""
import argparse
import functools
import http.server
import json
import pathlib
import sys
import threading

import gi
gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import GLib, Gtk, WebKit2

# (name, width, height, zoom): the bead's narrow window at 150 percent text,
# the app's smallest window (900 x 560) at 100, 150 and 200 percent, a short
# wide window, an ordinary desktop window, and a narrow short one at 150.
SIZES = [
    ("narrow-150", 400, 700, 1.5),
    ("minimum-100", 900, 560, 1.0),
    ("minimum-150", 900, 560, 1.5),
    ("short-wide", 1280, 420, 1.0),
    ("desktop", 1280, 800, 1.0),
    ("minimum-200", 900, 560, 2.0),
    ("narrow-short-150", 400, 420, 1.5),
]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--build", type=pathlib.Path, required=True, help="Vite build containing scripts/newteam-footer-smoke.html")
parser.add_argument("--output", type=pathlib.Path, required=True, help="Private evidence directory")
parser.add_argument("--label", default="webkit", help="Evidence prefix (avoids GTK's reserved --name option)")
parser.add_argument("--only", default="", help="Comma-separated case names to run")
args = parser.parse_args()
root = args.output.resolve()
root.mkdir(parents=True, exist_ok=True)


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_args):
        pass


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Quiet, directory=str(args.build.resolve())))
threading.Thread(target=server.serve_forever, daemon=True).start()
cases = [(f"{lang}-{refusal}-{size}", lang, refusal, width, height, zoom)
         for lang in ("en", "ru") for refusal in ("none", "short", "long") for size, width, height, zoom in SIZES]
if args.only:
    wanted = set(args.only.split(","))
    cases = [case for case in cases if case[0] in wanted]
results = []


def run(case):
    name, lang, refusal, width, height, zoom = case
    outcome = {"ok": False, "name": name, "failures": ["Browser timed out before result"]}
    manager = WebKit2.UserContentManager()
    manager.register_script_message_handler("proof")
    view = WebKit2.WebView.new_with_user_content_manager(manager)
    view.set_zoom_level(zoom)
    window = Gtk.Window()
    window.set_default_size(width, height)
    window.add(view)

    def message(_manager, result):
        nonlocal outcome
        value = json.loads(result.get_js_value().to_string())
        if value.get("type") != "result":
            return
        outcome = value

        def saved(source, result, _data):
            source.get_snapshot_finish(result).write_to_png(str(root / f"{args.label}-{name}.png"))
            Gtk.main_quit()
        view.get_snapshot(WebKit2.SnapshotRegion.VISIBLE, WebKit2.SnapshotOptions.NONE, None, saved, None)
    manager.connect("script-message-received::proof", message)
    # Below the app's smallest window (900 x 560 at any zoom) the form may
    # scroll as a whole, as a last resort; the page records whether it did.
    below = "1" if width < 900 or height < 560 else "0"
    view.load_uri(f"http://127.0.0.1:{server.server_port}/scripts/newteam-footer-smoke.html?case={name}&lang={lang}&refusal={refusal}&below={below}")
    window.show_all()
    timer = GLib.timeout_add_seconds(60, lambda: (Gtk.main_quit(), False)[1])
    Gtk.main()
    GLib.source_remove(timer) if GLib.main_context_default().find_source_by_id(timer) else None
    window.destroy()
    outcome["window"] = [width, height, zoom]
    return outcome


try:
    for case in cases:
        result = run(case)
        results.append(result)
        print(json.dumps({"case": case[0], "ok": result.get("ok"), "failures": result.get("failures", [])}), flush=True)
finally:
    server.shutdown()
ok = bool(results) and all(result.get("ok") for result in results)
(root / f"{args.label}-result.json").write_text(json.dumps({"ok": ok, "cases": results}, indent=2) + "\n")
print(json.dumps({"ok": ok, "cases": len(results), "failed": [r["name"] for r in results if not r.get("ok")]}))
sys.exit(0 if ok else 1)
