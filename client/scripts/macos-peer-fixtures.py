#!/usr/bin/env python3
"""Qualify the opt-in peer observer with private signed peers, never a live app.

Run headless on macOS under the project's owned heavy lock. Supply a verified
private rcodesign tool and a built core test executable. Signing uses disposable
PEM files and ad-hoc signatures, with no keychain commands or installed signing
identity. Services, installed applications and Notification Center stay untouched.
"""
import argparse
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rcodesign", type=Path, required=True)
    parser.add_argument("--test-bin", type=Path, required=True)
    parser.add_argument("--scratch", type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("native macOS qualification only")
    tool, test = args.rcodesign.resolve(), args.test_bin.resolve()
    if not tool.is_file() or not test.is_file():
        parser.error("the verified tool and test executable must exist")
    root = args.scratch.resolve()
    if len(str(root)) > 65:
        parser.error("use a short private scratch path for Unix socket limits")
    os.umask(0o077)
    root.mkdir(mode=0o700)
    env = {"PATH": "/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "C.UTF-8"}
    for name, folder in [("HOME", "home"), ("TMPDIR", "tmp"),
                         ("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"),
                         ("XDG_CACHE_HOME", "cache"), ("XDG_STATE_HOME", "state"),
                         ("XDG_RUNTIME_DIR", "runtime"), ("TMUX_TMPDIR", "tmux")]:
        path = root / folder
        path.mkdir(mode=0o700)
        env[name] = str(path)
    env["ZERUS_MAC_PEER_FIXTURES"] = str(root)

    def run(command):
        command = [str(x) for x in command]
        print("EXECUTED", repr(command), flush=True)
        subprocess.run(command, env=env, check=True, timeout=90)

    source = Path(__file__).resolve().parent.parent / "core/tests/support/macos_peer.c"
    keys = root / "keys"
    keys.mkdir(mode=0o700)
    try:
        run(["cc", "-Wall", "-Wextra", "-Werror", source, "-o", root / "peer-base"])
        for name, team in [("a", "FIXTEAM001"), ("b", "FIXTEAM002")]:
            key = keys / name
            run([tool, "generate-self-signed-certificate", "--config-file", "/dev/null",
                 "--person-name", "Synthetic peer " + name, "--team-id", team,
                 "--validity-days", "1", "--pem-filename", key])
            destination = root / ("peer-" + name)
            shutil.copy2(root / "peer-base", destination)
            run([tool, "sign", "--config-file", "/dev/null", "--timestamp-url", "none",
                 "--pem-file", str(key) + ".crt", "--pem-file", str(key) + ".key",
                 "--team-name", team, "--binary-identifier", "com.example.zerus-peer", destination])
            run(["codesign", "--verify", "--strict", destination])
        adhoc = root / "peer-adhoc"
        shutil.copy2(root / "peer-base", adhoc)
        run(["codesign", "--force", "--sign", "-", "--identifier", "com.example.zerus-peer", adhoc])
        test_copy = root / "core-test"
        shutil.copy2(test, test_copy)
        command = [str(test_copy), "macos_notify::peer::tests::", "--ignored", "--test-threads=1", "--nocapture"]
        print("EXECUTED", repr(command), flush=True)
        process = subprocess.Popen(command, env=env, start_new_session=True)
        try:
            code = process.wait(timeout=120)
        except subprocess.TimeoutExpired:
            # Only this fresh test session and its synthetic children.
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            raise
        if code:
            raise subprocess.CalledProcessError(code, command)
    finally:
        # These are freshly generated fixture keys, never owner signing material.
        shutil.rmtree(keys)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
