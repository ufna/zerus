#!/usr/bin/env python3
"""Automated keyboard and focus check of the client (zerus-5wmh.27) in a private Xvfb.

    env -u DISPLAY -u DBUS_SESSION_BUS_ADDRESS WAYLAND_DISPLAY=zerus-none \
        xvfb-run -a -s "-screen 0 1920x1080x24" VENV/bin/python keycheck.py ENGINE BINARY OUTDIR

Needs python-xlib (a private venv) and setxkbmap. Starts the client (built against a copy of
client/dist with keylog.js added before the app's script, through TAURI_CONFIG
'{"build":{"frontendDist":"COPY"}}'; keylog.js reports keydown, focus and
page state through perf_log on stderr), emulates what a window manager does (no WM runs:
XSetInputFocus plus WM_TAKE_FOCUS and _NET_ACTIVE_WINDOW on activation, a second X window
to switch to) and types through XTEST. It replays the owner's eight steps and checks what
reached the page. Never run against the desktop: it refuses unless DISPLAY is a private
X server and WAYLAND_DISPLAY is the nonexistent socket.
"""
import json
import os
import subprocess
import sys
import time

from Xlib import X, XK, Xatom, display as xdisplay, protocol
from Xlib.ext import xtest

ENGINE, BINARY, OUT = sys.argv[1], sys.argv[2], sys.argv[3]
if os.environ.get("WAYLAND_DISPLAY") != "zerus-none" or not os.environ.get("DISPLAY", "").startswith(":") \
        or os.environ.get("DISPLAY") in (":0", ":1"):
    sys.exit("refusing: run inside a private Xvfb with WAYLAND_DISPLAY=zerus-none")
os.makedirs(OUT, exist_ok=True)
FIXTURE = os.environ.get("ZERUS_KEYCHECK_FIXTURE") or os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "fixtures", "replay.ndjson")
print(f"keycheck: {ENGINE} on DISPLAY={os.environ['DISPLAY']} (Xvfb, no window manager)", flush=True)

d = xdisplay.Display()
root = d.screen().root
A = {n: d.intern_atom(n) for n in ("WM_PROTOCOLS", "WM_TAKE_FOCUS", "_NET_ACTIVE_WINDOW", "_NET_WM_NAME", "UTF8_STRING")}

env = dict(os.environ, GDK_BACKEND="x11", XDG_DATA_HOME=f"{OUT}/data", XDG_CACHE_HOME=f"{OUT}/cache",
           ZERUS_CLIENT_REPLAY=FIXTURE, ZERUS_SPIKE_CACHE_DIR=f"{OUT}/cef", LANG="en_US.UTF-8")
env.pop("DBUS_SESSION_BUS_ADDRESS", None)
log_path = f"{OUT}/app.log"
open(log_path, "w").close()
app = None
events, offset = [], 0
layout = {}


def pump():
    """Reads new ZKL lines from the app's log."""
    global offset
    with open(log_path, encoding="utf-8", errors="replace") as handle:
        handle.seek(offset)
        chunk = handle.read()
        offset = handle.tell()
    for line in chunk.splitlines():
        at = line.find("ZKL ")
        if at < 0 or "zerus-client perf:" not in line:
            continue
        try:
            event = json.loads(line[at + 4:])
        except ValueError:
            continue
        if event["kind"] == "layout":
            layout.update(event)
        event["wall"] = round(time.monotonic(), 3)
        events.append(event)


def wait(seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        pump()
        time.sleep(0.05)
    pump()


def name(win):
    try:
        prop = win.get_full_property(A["_NET_WM_NAME"], A["UTF8_STRING"])
        if prop:
            return prop.value.decode("utf-8", "replace")
        return win.get_wm_name() or ""
    except Exception:
        return ""


def find_toplevel(title, timeout=60):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        for win in root.query_tree().children:
            try:
                if name(win) == title and win.get_attributes().map_state == X.IsViewable:
                    return win
            except Exception:
                pass
        wait(0.2)
    return None


def tree(win, depth=0):
    out = []
    for child in win.query_tree().children:
        try:
            g = child.get_geometry()
            a = child.get_attributes()
            out.append((depth, child, g, a.map_state))
            out.extend(tree(child, depth + 1))
        except Exception:
            pass
    return out


def activate(win, how):
    """What a window manager does when the user activates a window (click or Alt+Tab)."""
    root.change_property(A["_NET_ACTIVE_WINDOW"], Xatom.WINDOW, 32, [win.id])
    win.configure(stack_mode=X.Above)
    win.set_input_focus(X.RevertToParent, X.CurrentTime)
    protocols = win.get_wm_protocols() or []
    if A["WM_TAKE_FOCUS"] in protocols:
        msg = protocol.event.ClientMessage(window=win, client_type=A["WM_PROTOCOLS"],
                                           data=(32, [A["WM_TAKE_FOCUS"], X.CurrentTime, 0, 0, 0]))
        win.send_event(msg)
    d.sync()
    print(f"keycheck: activate {how} (WM_TAKE_FOCUS {'yes' if A['WM_TAKE_FOCUS'] in protocols else 'no'})", flush=True)


def keycode(keysym_name):
    return d.keysym_to_keycode(XK.string_to_keysym(keysym_name))


def key(keysym_name, ctrl=False, shift=False):
    code = keycode(keysym_name)
    if not code:
        raise RuntimeError(f"no keycode for {keysym_name}")
    mods = ([keycode("Control_L")] if ctrl else []) + ([keycode("Shift_L")] if shift else [])
    for m in mods:
        xtest.fake_input(d, X.KeyPress, m)
    xtest.fake_input(d, X.KeyPress, code)
    xtest.fake_input(d, X.KeyRelease, code)
    for m in reversed(mods):
        xtest.fake_input(d, X.KeyRelease, m)
    d.sync()
    wait(0.12)


def raw(code):
    xtest.fake_input(d, X.KeyPress, code)
    xtest.fake_input(d, X.KeyRelease, code)
    d.sync()
    wait(0.12)


CHARS = {" ": "space", "-": "minus", "=": "equal", ",": "comma"}


def type_text(text):
    for ch in text:
        key(CHARS.get(ch, ch))


def click(x, y):
    xtest.fake_input(d, X.MotionNotify, x=int(x), y=int(y))
    d.sync()
    wait(0.05)
    xtest.fake_input(d, X.ButtonPress, 1)
    xtest.fake_input(d, X.ButtonRelease, 1)
    d.sync()
    wait(0.3)


results = []


def step(title, sent, check):
    """Runs a check over the events since the step's mark."""
    wait(0.6)
    ok, detail = check()
    results.append({"step": title, "sent": sent, "ok": ok, "detail": detail})
    print(f"keycheck: {'PASS' if ok else 'FAIL'} {title}: {detail}", flush=True)


def since(mark):
    return [e for e in events if e["n"] > mark]


def last_n():
    return events[-1]["n"] if events else 0


MODIFIERS = {"Control", "Shift", "Alt", "Meta"}


def keys_seen(evs):
    return [("C-" if e.get("ctrl") else "") + e["key"] for e in evs if e["kind"] == "keydown" and e["key"] not in MODIFIERS]


def state():
    """The page as of the newest record: "after" records follow each handled key by 80 ms."""
    return events[-1] if events else {}


try:
    # A second window, focused before the client starts (the terminal or launcher the user
    # starts it from), and the one to switch to later, as Alt+Tab or a click elsewhere would.
    decoy = root.create_window(1500, 40, 360, 220, 0, d.screen().root_depth, X.InputOutput, X.CopyFromParent,
                               background_pixel=d.screen().white_pixel, event_mask=X.KeyPressMask | X.FocusChangeMask)
    decoy.set_wm_name("keycheck decoy")
    decoy.set_wm_protocols([A["WM_TAKE_FOCUS"]])
    decoy.map()
    d.sync()
    xtest.fake_input(d, X.MotionNotify, x=1680, y=150)
    d.sync()
    wait(0.3)
    activate(decoy, "decoy before start")
    app = subprocess.Popen([BINARY], env=env, stdout=open(log_path, "w"), stderr=subprocess.STDOUT, start_new_session=True)
    client = find_toplevel("Zerus Next")
    if client is None:
        raise RuntimeError("the client window did not appear")
    g = client.get_geometry()
    print(f"keycheck: client toplevel 0x{client.id:x} {g.width}x{g.height}+{g.x}+{g.y}", flush=True)

    # Step 1: input right after start. A window manager focuses a newly mapped window.
    end = time.monotonic() + 60
    while time.monotonic() < end and not any(e["kind"] == "armed" for e in events):
        wait(0.2)
    activate(client, "new window")
    wait(1.0)
    mark = last_n()
    key("2", ctrl=True); wait(0.4)
    page_after_2 = state().get("page")
    key("1", ctrl=True)
    step("1 startup Ctrl+2, Ctrl+1", ["C-2", "C-1"],
         lambda: (keys_seen(since(mark)) == ["C-2", "C-1"] and (state().get("page") or "").startswith("Home"),
                  f"keys {keys_seen(since(mark))}, page after Ctrl+2 {page_after_2!r}, now {state().get('page')!r}"))

    # Step 2: settings and back.
    mark = last_n()
    key("comma", ctrl=True); wait(0.4)
    page_settings = state().get("page")
    key("2", ctrl=True)
    step("2 Ctrl+, then Ctrl+2", ["C-,", "C-2"],
         lambda: (keys_seen(since(mark)) == ["C-,", "C-2"] and (page_settings or "").startswith("Settings")
                  and (state().get("page") or "").startswith("Sessions"),
                  f"keys {keys_seen(since(mark))}, pages {page_settings!r} then {state().get('page')!r}"))

    # Step 3: zoom keys (Chrome binds them too).
    mark = last_n()
    toasts = []
    for k in ("equal", "equal", "minus", "0"):
        key(k, ctrl=True); wait(0.3)
        toasts.append(state().get("toast"))
    step("3 Ctrl+= x2, Ctrl+-, Ctrl+0", ["C-=", "C-=", "C--", "C-0"],
         lambda: (keys_seen(since(mark)) == ["C-=", "C-=", "C--", "C-0"],
                  f"keys {keys_seen(since(mark))}, toasts {toasts}"))

    # Step 4: click into search, type, switch the layout, type, Escape.
    wait(1.2)
    lay = dict(layout)
    sx, sy, sw, sh = lay.get("searchRect") or (0, 0, 0, 0)
    wg = client.get_geometry()
    origin = client.translate_coords(root, 0, 0)
    ox, oy = -origin.x, -origin.y
    # Without decorations the page fills the toplevel; any difference is at the top (a header bar).
    oy += max(0, wg.height - lay.get("inner", [0, wg.height])[1])
    mark = last_n()
    click(ox + sx + sw / 2, oy + sy + sh / 2)
    downs = [e for e in since(mark) if e["kind"] == "pointerdown"]
    type_text("test 123")
    physical = [keycode(k) for k in ("f", "d", "l")]
    subprocess.run(["setxkbmap", "-layout", "ru"], check=False)
    wait(0.5)
    for code in physical:
        raw(code)
    subprocess.run(["setxkbmap", "-layout", "us"], check=False)
    wait(0.5)
    typed = state().get("search")
    key("Escape")
    step("4 click search, type 'test 123', ru layout f d l, Escape", ["click", "test 123", "f d l (ru)", "Escape"],
         lambda: (bool(downs) and typed == "test 123авд" and state().get("search") == "",
                  f"pointerdown {[(e['x'], e['y']) for e in downs]} (search rect {lay.get('searchRect')}), "
                  f"search before Escape {typed!r}, after {state().get('search')!r}, keys {keys_seen(since(mark))}"))

    # Step 5: switch to another window and back without clicking, then type.
    mark = last_n()
    activate(decoy, "decoy (Alt+Tab away)")
    wait(0.6)
    activate(client, "client (Alt+Tab back)")
    wait(0.6)
    type_text("xyz")
    wait(0.3)
    after_xyz = state().get("search")
    key("3", ctrl=True)
    step("5 away and back, type 'xyz', Ctrl+3", ["focus away", "focus back", "xyz", "C-3"],
         lambda: (keys_seen(since(mark)) == ["x", "y", "z", "C-3"] and (state().get("page") or "").startswith("Projects"),
                  f"keys {keys_seen(since(mark))}, search {after_xyz!r}, page {state().get('page')!r}, "
                  f"focus events {[e['kind'] for e in since(mark) if e['kind'] not in ('keydown', 'after', 'layout')]}"))

    # Step 6: click the other window, then click back on the client (as on its title bar).
    mark = last_n()
    click(1500 + 180, 40 + 110)
    activate(decoy, "decoy (click)")
    wait(0.6)
    activate(client, "client (title bar click)")
    wait(0.6)
    type_text("qwe")
    key("2", ctrl=True)
    step("6 click away, click back, type 'qwe', Ctrl+2", ["click away", "click back", "qwe", "C-2"],
         lambda: (keys_seen(since(mark)) == ["q", "w", "e", "C-2"] and (state().get("page") or "").startswith("Sessions"),
                  f"keys {keys_seen(since(mark))}, page {state().get('page')!r}"))

    # Step 7: click into the list and use the arrows.
    wait(1.2)
    lay = dict(layout)
    lx, ly, lw, lh = lay.get("listRect") or (0, 0, 0, 0)
    mark = last_n()
    click(ox + lx + lw / 2, oy + ly + min(lh / 2, 200))
    for k in ("Down", "Down", "Down", "Up", "Up"):
        key(k)
    step("7 click list, Down x3, Up x2", ["click", "Down x3", "Up x2"],
         lambda: (keys_seen(since(mark)) == ["ArrowDown"] * 3 + ["ArrowUp"] * 2,
                  f"keys {keys_seen(since(mark))}, active {state().get('active')!r}"))
finally:
    wait(0.5)
    if app is not None:
        try:
            os.killpg(app.pid, 15)
        except ProcessLookupError:
            pass
        app.wait(timeout=20)
    with open(f"{OUT}/events.jsonl", "w", encoding="utf-8") as handle:
        for e in events:
            handle.write(json.dumps(e, ensure_ascii=False) + "\n")
    with open(f"{OUT}/results.json", "w", encoding="utf-8") as handle:
        json.dump({"engine": ENGINE, "results": results, "layout": layout}, handle, ensure_ascii=False, indent=2)
    passed = sum(r["ok"] for r in results)
    print(f"keycheck: {ENGINE}: {passed}/{len(results)} steps passed", flush=True)
