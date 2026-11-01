#!/usr/bin/env python3
"""Register the selected hgs desktop service in this user's Aqua login session."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import re
import selectors
import stat
import subprocess
import sys
import tempfile
import time

LABEL = "com.hgdev.hgs-desktop"
ABI = "desktop-lifecycle-v1"
MARKER = "Zerus desktop registration v1"
LIMIT = 128 * 1024


class Refusal(RuntimeError):
    pass


def require(condition, message):
    if not condition:
        raise Refusal(message)


def run(argv, env=None, timeout=10):
    """Bound both pipes and the lifetime of this one owned command."""
    with subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, env=env, close_fds=True) as child:
        output = {child.stdout: bytearray(), child.stderr: bytearray()}
        try:
            with selectors.DefaultSelector() as selector:
                for stream in output:
                    os.set_blocking(stream.fileno(), False)
                    selector.register(stream, selectors.EVENT_READ)
                deadline = time.monotonic() + timeout
                while selector.get_map():
                    remaining = deadline - time.monotonic()
                    require(remaining > 0, "Desktop service command timed out; inspect its status before retrying.")
                    for key, _ in selector.select(min(remaining, 0.2)):
                        stream = key.fileobj
                        chunk = os.read(stream.fileno(), 8192)
                        if not chunk:
                            selector.unregister(stream)
                        else:
                            output[stream].extend(chunk)
                            require(len(output[stream]) <= LIMIT, "Desktop service command output exceeds its bound.")
                child.wait(timeout=max(0.01, deadline - time.monotonic()))
            return subprocess.CompletedProcess(argv, child.returncode,
                output[child.stdout].decode("utf-8", "strict"), output[child.stderr].decode("utf-8", "replace"))
        finally:
            if child.poll() is None:
                child.kill()  # Only this Popen child; never a process group or native agent.
                child.wait()


def environment(binary, home, source):
    values = {"HOME": str(home), "PATH": source.get("PATH", "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"),
              "HGS_EXECUTABLE": str(binary)}
    for name in ("HGS_CONFIG_DIR", "HGS_STATE_DIR", "XDG_CONFIG_HOME", "XDG_DATA_HOME",
                 "XDG_STATE_HOME", "XDG_CACHE_HOME", "XDG_RUNTIME_DIR", "TMPDIR", "TMUX_TMPDIR"):
        if source.get(name):
            values[name] = source[name]
    return values


def ordinary(path, directory=False):
    info = path.lstat()
    kind = stat.S_ISDIR if directory else stat.S_ISREG
    require(kind(info.st_mode) and info.st_uid == os.getuid() and not info.st_mode & 0o022,
            "Desktop service path is not an ordinary user-owned file/directory.")
    if not directory:
        require(info.st_nlink == 1, "Desktop service definition has another hard link.")
    return info


def read_definition(path):
    try:
        before = ordinary(path)
    except FileNotFoundError:
        return None
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        current = os.fstat(descriptor)
        require((before.st_dev, before.st_ino) == (current.st_dev, current.st_ino)
                and stat.S_ISREG(current.st_mode), "Desktop service definition changed while opening it.")
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            content = stream.read(LIMIT + 1)
        require(len(content) <= LIMIT, "Desktop service definition exceeds its bound.")
        return content, current.st_dev, current.st_ino
    finally:
        os.close(descriptor)


def registration(binary, home, env, command=run):
    result = command([str(binary), "desktop", "registration", "--json"], env=env)
    require(result.returncode == 0, "Cannot obtain desktop service registration from the selected hgs CLI.")
    value = json.loads(result.stdout)
    require(isinstance(value, dict) and value.get("schema") == 1 and value.get("operation") == "preview"
            and value.get("label") == LABEL, "Unsupported desktop registration definition.")
    destination = home / "Library/LaunchAgents" / (LABEL + ".plist")
    require(value.get("destination") == str(destination), "Desktop registration is outside this user's LaunchAgents.")
    config = value.get("configuration", "")
    require(isinstance(config, str) and re.fullmatch(r"[0-9a-f]{64}", config), "Invalid desktop configuration identity.")
    argv = value.get("argv")
    require(isinstance(argv, list) and len(argv) == 8 and argv[:3] == [str(binary), "desktop", "serve"]
            and argv[3] == "--config-dir" and argv[5] == "--state-dir" and argv[7] == "--launchd"
            and all(isinstance(part, str) and part and not any(ord(c) < 32 for c in part) for part in argv)
            and Path(argv[4]).is_absolute() and Path(argv[6]).is_absolute(), "Unexpected desktop service command.")
    expected = {"Label": LABEL, "ProgramArguments": argv, "RunAtLoad": True,
                "KeepAlive": {"SuccessfulExit": False}, "ThrottleInterval": 10,
                "AbandonProcessGroup": True, "LimitLoadToSessionType": "Aqua"}
    require(isinstance(value.get("definition"), str), "Missing desktop LaunchAgent definition.")
    content = value["definition"].encode("utf-8")
    require(len(content) <= LIMIT and f"{MARKER}; configuration={config}".encode() in content
            and plistlib.loads(content) == expected, "Unexpected Aqua LaunchAgent definition.")
    probe = value.get("readiness")
    require(probe == {"abi": ABI, "argv": [str(binary), "desktop", "status", "--json"],
        "environment": {"HGS_CONFIG_DIR": argv[4], "HGS_STATE_DIR": argv[6]},
        "expected_configuration": config, "configuration_field": "lifecycle.configuration",
        "ready_field": "lifecycle.ipc_ready", "epoch_field": "lifecycle.epoch",
        "generation_field": "lifecycle.generation"}, "Unsupported desktop readiness interface.")
    require(value.get("session") == {"kind": "aqua", "notification_bundle": "com.hgdev.hgs-tray"},
            "Desktop service requires the Aqua session and the existing notification identity.")
    return value, destination, content


def owned(content, value):
    if content is None:
        return None
    definition = plistlib.loads(content)
    markers = re.findall(rb"<!--\s*" + re.escape(MARKER.encode())
                         + rb"; configuration=([0-9a-f]{64})\s*-->", content)
    require(markers == [value["configuration"].encode()] and isinstance(definition, dict)
            and definition.get("Label") == LABEL,
            "Existing desktop LaunchAgent is custom or belongs to another configuration; it was kept.")
    argv = definition.get("ProgramArguments")
    require(isinstance(argv, list) and len(argv) >= 3 and argv[1:3] == ["desktop", "serve"]
            and all(isinstance(part, str) and part and not any(ord(c) < 32 for c in part) for part in argv)
            and Path(argv[0]).is_absolute(), "Owned desktop definition has invalid arguments; it was kept.")
    return definition


def loaded_identity(printed, destination, argv):
    def field(name):
        values = re.findall(r"(?m)^\t" + name + r" = (.+)$", printed)
        return values[0] if len(values) == 1 else None
    blocks = re.findall(r"(?ms)^\targuments = \{\n(.*?)^\t\}", printed)
    arguments = [line[2:] for line in blocks[0].splitlines()] if len(blocks) == 1 else None
    require(field("path") == str(destination) and field("program") == argv[0] and arguments == argv,
            "Loaded desktop LaunchAgent does not match the owned definition; it was kept.")
    pid = field("pid")
    return int(pid) if pid and pid.isdecimal() else None


def ready(value, status, pid):
    require(isinstance(status, dict) and isinstance(status.get("lifecycle"), dict),
            "Missing desktop lifecycle status.")
    lifecycle = status.get("lifecycle", {})
    require(lifecycle.get("abi") == ABI and lifecycle.get("configuration") == value["configuration"],
            "Desktop lifecycle ABI/configuration differs; reconcile the selected service before retrying.")
    if lifecycle.get("ipc_ready") is not True:
        return False
    require(type(lifecycle.get("pid")) is int and lifecycle["pid"] > 0
            and lifecycle["pid"] == pid and isinstance(lifecycle.get("epoch"), str) and lifecycle["epoch"]
            and type(lifecycle.get("generation")) is int and lifecycle["generation"] >= 0,
            "Desktop readiness does not belong to the loaded service actor.")
    return True


def install(binary, home, source, restart=False, check_only=False, remove=False,
            command=run, clock=time.monotonic, sleep=time.sleep):
    require(home.is_absolute(), "Desktop service HOME must be absolute.")
    binary = Path(os.path.abspath(binary))  # Retain the maintained CLI path, including a cask symlink.
    require(binary.is_file() and os.access(binary, os.X_OK), "Desktop service requires an executable hgs CLI.")
    ordinary(home, directory=True)
    for directory in (home / "Library", home / "Library/LaunchAgents"):
        if os.path.lexists(directory):
            ordinary(directory, directory=True)
    env = environment(binary, home, source)
    value, destination, content = registration(binary, home, env, command)
    previous = read_definition(destination)
    previous_definition = owned(previous[0] if previous is not None else None, value)
    domain = f"gui/{os.getuid()}"
    target = domain + "/" + LABEL
    def launchctl(*args):
        return command(["launchctl", *args], env=env)
    gui = launchctl("print", domain).returncode == 0
    printed = launchctl("print", target) if gui else None
    loaded = printed is not None and printed.returncode == 0
    changed = previous is None or previous[0] != content
    if loaded:
        require(previous is not None, "Desktop label is loaded without an owned definition; it was kept.")
        loaded_identity(printed.stdout, destination, previous_definition["ProgramArguments"])
        if remove or restart or changed:
            require(previous_definition.get("AbandonProcessGroup") is True,
                    "The old desktop definition cannot preserve its process group during unload; it was kept.")
    require(read_definition(destination) == previous, "Desktop definition changed during preflight; it was kept.")
    if check_only:
        return "checked"
    if remove and previous is None:
        return "unchanged"
    if loaded and (remove or restart or changed):
        result = launchctl("bootout", target)
        require(result.returncode == 0, "Cannot unload the owned desktop service; inspect it before retrying.")
        deadline = clock() + 15
        while launchctl("print", target).returncode == 0:
            require(clock() < deadline, "Desktop LaunchAgent is still unloading; no replacement was started.")
            sleep(0.2)
        loaded = False
    require(read_definition(destination) == previous, "Desktop definition changed during installation; it was kept.")
    if remove:
        destination.unlink()
        return "removed"
    if changed:
        for directory in (home / "Library", destination.parent):
            directory.mkdir(mode=0o755, exist_ok=True)
            ordinary(directory, directory=True)
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as stage:
                temporary = Path(stage.name)
                stage.write(content); stage.flush(); os.fsync(stage.fileno())
                os.fchmod(stage.fileno(), 0o644)
            require(read_definition(destination) == previous, "Desktop definition changed before replacement; it was kept.")
            os.replace(temporary, destination)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
    if not gui:
        return "deferred"
    if not loaded:
        result = launchctl("bootstrap", domain, str(destination))
        require(result.returncode == 0, "Cannot load desktop service in Aqua; registration was kept for inspection.")
    probe_env = dict(env, **value["readiness"]["environment"])
    deadline = clock() + 15
    while True:
        printed = launchctl("print", target)
        if printed.returncode == 0:
            pid = loaded_identity(printed.stdout, destination, value["argv"])
            result = command(value["readiness"]["argv"], env=probe_env)
            require(result.returncode == 0, "Desktop readiness command failed; inspect hgs desktop status --json.")
            if ready(value, json.loads(result.stdout), pid):
                return "ready"
        require(clock() < deadline, "Desktop IPC did not become ready; inspect hgs desktop status --json.")
        sleep(0.2)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--check-only", action="store_true")
    group.add_argument("--remove", action="store_true")
    parser.add_argument("--restart", action="store_true", help="reload only this owned desktop service after a backend upgrade")
    args = parser.parse_args()
    if sys.platform != "darwin":
        return 0
    if os.geteuid() == 0:
        parser.error("run the desktop installer as the login user, without sudo")
    try:
        state = install(args.binary, Path.home(), os.environ, args.restart, args.check_only, args.remove)
        if state == "deferred":
            print("Desktop service registered for the next Aqua login; IPC readiness is unverified.")
        elif state == "ready":
            print("Desktop service IPC is ready. Background workers remain controlled by the service.")
        else:
            print(f"Desktop service registration: {state}.")
        return 0
    except (Refusal, OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError) as error:
        print(f"hgs desktop installer: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
