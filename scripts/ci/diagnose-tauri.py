#!/usr/bin/env python3
"""Disposable .78 hosted diagnosis; never install or run on an owner's Mac."""
import hashlib
import json
from pathlib import Path
import re
import resource
import shutil
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "artifacts/tauri-diagnostic"
DEADLINE_SECONDS = 45


def save(name, value):
    (OUT / name).write_text(json.dumps(value, indent=2) + "\n")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def source():
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


def bundle():
    candidates = []
    with (OUT / "build.jsonl").open() as lines:
        for line in lines:
            event = json.loads(line)
            if (event.get("reason") == "compiler-artifact"
                    and event.get("profile", {}).get("test")
                    and event.get("target", {}).get("name") == "zerus_client_lib"
                    and event.get("executable")):
                candidates.append(Path(event["executable"]))
    if len(candidates) != 1:
        raise RuntimeError(f"Expected one exact library test executable, got {len(candidates)}")
    binary = OUT / "tauri-tests"
    shutil.copyfile(candidates[0], binary)
    save("binary.json", {"source": source(), "sha256": digest(binary), "bytes": binary.stat().st_size})


def verified():
    manifest = json.loads((OUT / "binary.json").read_text())
    binary = OUT / "tauri-tests"
    if manifest["source"] != source() or manifest["sha256"] != digest(binary):
        raise RuntimeError("Source or executable identity mismatch")
    binary.chmod(0o700)  # artifact download does not retain executable permission
    return binary


def child_limits():
    # Only the freshly forked diagnostic test process. No process-group signals.
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def execute(binary, args, label):
    log = OUT / f"{label}.log"
    if sum(p.stat().st_size for p in OUT.glob("*.log")) > 112 * 1024 * 1024:
        raise RuntimeError("128 MiB aggregate diagnostic log budget; no further test starts")
    print(f"BEGIN {label} {json.dumps(args)}", flush=True)
    started = time.monotonic()
    with tempfile.TemporaryDirectory(prefix="zt78-", dir="/tmp") as scratch:
        root = Path(scratch)
        private = {}
        for key, leaf in [("HOME", "home"), ("TMPDIR", "tmp"), ("TMUX_TMPDIR", "tmux"),
                          ("XDG_STATE_HOME", "state"), ("XDG_DATA_HOME", "data"),
                          ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"),
                          ("HGS_STATE_DIR", "hgs-state"), ("HGS_CONFIG_DIR", "hgs-config")]:
            folder = root / leaf
            folder.mkdir(mode=0o700)
            private[key] = str(folder)
        env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "en_US.UTF-8",
               "LC_ALL": "en_US.UTF-8", "TZ": "UTC", **private}
        child = subprocess.Popen([str(binary), *args], cwd=ROOT, env=env,
                                 stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                 stderr=subprocess.STDOUT, preexec_fn=child_limits)
        capture = {"total": 0, "retained": 0, "error": False}

        def drain():
            try:
                with child.stdout, log.open("wb") as output:
                    while chunk := child.stdout.read(65536):
                        capture["total"] += len(chunk)
                        keep = chunk[:max(0, 16 * 1024 * 1024 - capture["retained"])]
                        output.write(keep)
                        capture["retained"] += len(keep)
            except OSError:
                capture["error"] = True

        reader = threading.Thread(target=drain, daemon=True)
        reader.start()
        timed_out = False
        try:
            status = child.wait(timeout=DEADLINE_SECONDS)
        except subprocess.TimeoutExpired:
            timed_out = True
            # This unreaped Child is the test we just spawned; never a PID lookup.
            child.kill()
            status = child.wait(timeout=2)
        reader.join(timeout=2)
        drained = not reader.is_alive() and not capture["error"]
    if not log.exists():
        log.touch()
    result = {"label": label, "args": args, "exit_code": status,
              "process_exit_code": status, "output_drained": drained,
              "output_bytes": capture["total"],
              "discarded_output_bytes": capture["total"] - capture["retained"],
              "timed_out": timed_out, "seconds": round(time.monotonic() - started, 3),
              "sha256": digest(log), "bytes": log.stat().st_size}
    if not drained or result["discarded_output_bytes"]:
        result["exit_code"] = 125  # truncated/incomplete evidence grants no pass
    if "--list" not in args:
        with log.open("rb") as stream:
            stream.seek(max(0, result["bytes"] - 8192))
            tail = stream.read(8192).decode(errors="replace")
        summary = re.search(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", tail)
        result["test_counts"] = list(map(int, summary.groups())) if summary else None
        expected = 1 if "--exact" in args else len(json.loads((OUT / "tests.json").read_text())["tests"])
        if status == 0 and (not summary or sum(result["test_counts"]) != expected):
            result["exit_code"] = 125  # a successful empty harness is not test execution
    save(f"{label}.json", result)
    print("END " + json.dumps(result), flush=True)
    if timed_out or not drained:
        raise RuntimeError(f"Owned test timeout or incomplete drain: {label}; no further test starts")
    return result


def listing():
    result = execute(verified(), ["--list", "--format", "terse"], "list")
    if result["exit_code"]:
        raise RuntimeError("Test executable could not list tests")
    tests = []
    for line in (OUT / "list.log").read_text().splitlines():
        if line.endswith(": test"):
            tests.append(line.removesuffix(": test"))
    if not tests or len(tests) > 10000 or len(tests) != len(set(tests)):
        raise RuntimeError("Missing or duplicate test inventory")
    save("tests.json", {"source": source(), "tests": sorted(tests)})


def plan(batch):
    verified()
    inventory = json.loads((OUT / "tests.json").read_text())
    if inventory["source"] != source() or batch not in range(4):
        raise RuntimeError("Invalid source/batch")
    # Stable contiguous quarters make a dead runner's job name useful for bisection.
    tests = inventory["tests"]
    selected = tests[len(tests) * batch // 4:len(tests) * (batch + 1) // 4]
    value = {"source": source(), "batch": batch, "tests": selected}
    save(f"batch-{batch}.json", value)
    return selected


def run(batch):
    binary = verified()
    tests = plan(batch)
    results = []
    for index, name in enumerate(tests):
        results.append(execute(binary, ["--exact", name, "--test-threads=1", "--nocapture"],
                               f"batch-{batch}-test-{index:04d}"))
    save(f"batch-{batch}-summary.json", {
        "source": source(), "invocations": len(results),
        "passed_tests": sum((r["test_counts"] or [0, 0, 0])[0] for r in results),
        "failed_tests": sum((r["test_counts"] or [0, 0, 0])[1] for r in results),
        "ignored_tests": sum((r["test_counts"] or [0, 0, 0])[2] for r in results),
        "failed_invocations": sum(r["exit_code"] != 0 for r in results)})
    if any(r["exit_code"] != 0 for r in results):
        raise RuntimeError("Test failures; inspect exact per-test logs")


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    mode = sys.argv[1]
    if mode == "bundle":
        bundle()
    elif mode == "list":
        listing()
    elif mode in {"prepare", "run"}:
        (plan if mode == "prepare" else run)(int(sys.argv[2]))
    elif mode == "parallel":
        DEADLINE_SECONDS = 180
        result = execute(verified(), ["--nocapture"], "default-parallel")
        if result["exit_code"]:
            raise SystemExit(result["exit_code"])
    else:
        raise SystemExit("usage: diagnose-tauri.py bundle|list|prepare BATCH|run BATCH|parallel")
