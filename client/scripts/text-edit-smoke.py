#!/usr/bin/env python3
"""Rendered text undo regression in a PRIVATE Xvfb/D-Bus session.
Build bench/text-edit first, then pass its output directory. Synthetic text only;
never point this at an installed GUI. Requires GTK3/WebKitGTK4.1 and libXtst.
"""
import ctypes
import functools
import http.server
import json
import os
from pathlib import Path
import sys
import threading

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("GdkX11", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import Gdk, GdkX11, GLib, Gtk, WebKit2  # noqa: E402

assert os.environ.get("GDK_BACKEND") == "x11"
assert os.environ.get("WAYLAND_DISPLAY") == "zerus-none"
assert os.environ.get("DBUS_SESSION_BUS_ADDRESS")
root = Path(sys.argv[1]).resolve(strict=True)

class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass

server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Handler, directory=str(root)))
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
window = Gtk.Window()
window.set_default_size(800, 600)
web = WebKit2.WebView.new_with_context(WebKit2.WebContext.new_ephemeral())
window.add(web)
window.show_all()
manager = web.get_user_content_manager()
manager.register_script_message_handler("result")
rows = []
finished = False
list_cases = iter(
    (kind, case, before, after, start, end)
    for kind in ("composer", "task")
    for case, before, after, start, end in (
        ("continues an ordered list over a selection", "9. item selected tail", "9. item\n10.  tail", 7, 16),
        ("leaves nested fenced code literal", "1. outer\n   - inner\n     ```js\n     - literal",
         "1. outer\n   - inner\n     ```js\n     - literal\n", None, None),
        ("ends an empty list", "- ", "", None, None),
        ("plain Enter submits", "1. submit me", None, None, None),
    )
)
list_case = None

# Actual X11 events go only to this fixture's own XID on its private display.
x11 = ctypes.CDLL("libX11.so.6")
xtst = ctypes.CDLL("libXtst.so.6")
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
x11.XSetInputFocus.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
x11.XFlush.argtypes = [ctypes.c_void_p]
x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
xtst.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
display = x11.XOpenDisplay(None)
assert display

def keys(symbols):
    x11.XSetInputFocus(display, GdkX11.X11Window.get_xid(window.get_window()), 1, 0)
    codes = [x11.XKeysymToKeycode(display, s) for s in symbols]
    for c in codes:
        xtst.XTestFakeKeyEvent(display, c, True, 0)
    for c in reversed(codes):
        xtst.XTestFakeKeyEvent(display, c, False, 0)
    x11.XFlush(display)


def js(code):
    web.evaluate_javascript(code, -1, None, None, None, None, None)


def next_list_case():
    global list_case, finished
    list_case = next(list_cases, None)
    if list_case is None:
        finished = True
        Gtk.main_quit()
        return
    kind, case, before, _, start, end = list_case
    js("textEditFixture.prepareList(..." + json.dumps([
        kind, before, len(before) if start is None else start, len(before) if end is None else end,
        case.startswith("continues"),
    ]) + ")")


def list_snapshot(stage):
    js("textEditFixture.listSnapshot(..." + json.dumps([list_case[0], stage]) + ")")
    return False


def receive(_manager, message):
    global finished
    data = json.loads(message.get_js_value().to_string())
    if data.get("error"):
        rows.append({"name": "rendered fixture", "pass": False, "detail": data})
        finished = True
        Gtk.main_quit()
    elif data.get("stage") == "ready":
        rows.extend(data["results"])
        js('''const f=window.textEditFixture;const e=f.editor();e.focus();e.setSelectionRange(e.value.length,e.value.length);document.execCommand('insertText',false,' keyboard');''')
        GLib.timeout_add(150, lambda: (keys([0xffe3, ord('z')]), GLib.timeout_add(150, inspect_undo), False)[-1])
    elif data.get("stage") == "undo":
        rows.append({"name": "actual Ctrl+Z reaches Composer", "pass": data["value"] == "another saved draft" and data["draft"] == data["value"], "detail": data})
        keys([0xffe3, 0xffe1, ord('z')])
        GLib.timeout_add(150, lambda: (js("window.webkit.messageHandlers.result.postMessage(JSON.stringify({stage:'redo',value:textEditFixture.editor().value,draft:textEditFixture.model.text()}))"), False)[-1])
    elif data.get("stage") == "redo":
        rows.append({"name": "actual Ctrl+Shift+Z reaches Composer", "pass": data["value"] == "another saved draft keyboard" and data["draft"] == data["value"], "detail": data})
        js("textEditFixture.editor().select()")
        GLib.timeout_add(100, lambda: (keys([0xffe3, ord('x')]), GLib.timeout_add(150, lambda: snapshot("cut")), False)[-1])
    elif data.get("stage") == "cut":
        rows.append({"name": "actual Ctrl+X cuts and updates draft", "pass": data["value"] == "" and data["draft"] == "", "detail": data})
        keys([0xffe3, ord('z')])
        GLib.timeout_add(150, lambda: snapshot("cut-undo"))
    elif data.get("stage") == "cut-undo":
        rows.append({"name": "actual cut undo", "pass": data["value"] == "another saved draft keyboard" and data["draft"] == data["value"], "detail": data})
        Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).set_text("paste fixture", -1)
        js("textEditFixture.editor().select()")
        GLib.timeout_add(100, lambda: (keys([0xffe3, ord('v')]), GLib.timeout_add(200, lambda: snapshot("paste")), False)[-1])
    elif data.get("stage") == "paste":
        rows.append({"name": "actual Ctrl+V pastes and updates draft", "pass": data["value"] == "paste fixture" and data["draft"] == data["value"], "detail": data})
        keys([0xffe3, ord('z')])
        GLib.timeout_add(150, lambda: snapshot("paste-undo"))
    elif data.get("stage") == "paste-undo":
        rows.append({"name": "actual paste undo", "pass": data["value"] == "another saved draft keyboard" and data["draft"] == data["value"], "detail": data})
        keys([0xffe3, ord('y')])
        GLib.timeout_add(150, lambda: snapshot("paste-redo"))
    elif data.get("stage") == "paste-redo":
        rows.append({"name": "actual Ctrl+Y redo", "pass": data["value"] == "paste fixture" and data["draft"] == data["value"], "detail": data})
        next_list_case()
    elif data.get("stage") == "list-ready":
        kind, case, _, after, _, _ = list_case
        if case.startswith("continues"):
            rows.append({"name": kind + " preserves composition-final Enter", "pass": True})
        keys([0xff0d] if after is None else [0xffe1, 0xff0d])
        GLib.timeout_add(180, lambda: list_snapshot("list-edited"))
    elif data.get("stage") == "list-edited":
        kind, case, before, after, start, _ = list_case
        if after is None:
            rows.append({"name": kind + " " + case, "pass": data["submissions"] == [before], "detail": data})
            next_list_case()
        else:
            caret = len(after) if start is None else start + len("\n10. ")
            matched = data["value"] == after and data["draft"] == after and not data["submissions"]
            rows.append({"name": kind + " " + case, "pass": matched and data["start"] == data["end"] == caret,
                         "detail": data})
            keys([0xffe3, ord("z")])
            GLib.timeout_add(150, lambda: list_snapshot("list-undone"))
    elif data.get("stage") == "list-undone":
        kind, case, before, _, start, end = list_case
        selection = start is None or (data["start"], data["end"]) == (start, end)
        rows.append({"name": kind + " native undo: " + case,
                     "pass": data["value"] == data["draft"] == before and selection,
                     "detail": data})
        next_list_case()


def snapshot(stage):
    js("window.webkit.messageHandlers.result.postMessage(JSON.stringify({stage:"+json.dumps(stage)+",value:textEditFixture.editor().value,draft:textEditFixture.model.text()}))")
    return False


def inspect_undo():
    js("window.webkit.messageHandlers.result.postMessage(JSON.stringify({stage:'undo',value:textEditFixture.editor().value,draft:textEditFixture.model.text()}))")
    return False

manager.connect("script-message-received::result", receive)
web.load_uri(f"http://127.0.0.1:{server.server_port}/bench/text-edit/index.html")
GLib.timeout_add_seconds(25, lambda: (rows.append({"name": "bounded fixture", "pass": False}), Gtk.main_quit(), False)[-1])
try:
    Gtk.main()
finally:
    window.destroy()
    x11.XCloseDisplay(display)
    server.shutdown()
    server.server_close()
    thread.join(2)
print(json.dumps({"engine": ".".join(str(v()) for v in (WebKit2.get_major_version, WebKit2.get_minor_version, WebKit2.get_micro_version)), "results": rows}, indent=2))
sys.exit(0 if finished and rows and all(r["pass"] for r in rows) else 1)
