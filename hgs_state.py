#!/usr/bin/env python3
"""Compatibility entry point for trusted hooks installed by hgs 1.8–1.10.

All session logic lives in the Rust executable. Keep this path and existing hook
command text stable while sessions started before the migration are still alive.
Fresh installations use `hgs __state hook` directly and do not require Python.
"""
import os
from pathlib import Path
import shutil
import sys


def main():
    args = sys.argv[1:]
    if args == ["hook"] and (not os.environ.get("HGS_SESSION") or not os.environ.get("HGS_RUN_ID")):
        return 0
    candidates = [os.environ.get("HGS_EXECUTABLE"), str(Path(__file__).resolve().with_name("hgs")),
                  str(Path.home() / ".local/bin/hgs"), shutil.which("hgs")]
    for candidate in candidates:
        if not candidate or not os.access(candidate, os.X_OK):
            continue
        try:
            with open(candidate, "rb") as source:
                header = source.read(256)
        except OSError:
            continue
        # Calling an old Bash hgs with __state would be interpreted as an agent
        # launch. Only dispatch to a native executable or our development launcher.
        native = header[:4] in (b"\x7fELF", b"\xcf\xfa\xed\xfe", b"\xfe\xed\xfa\xcf",
                                b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca")
        if native or b"# hgs Rust launcher" in header:
            os.execv(candidate, [candidate, "__state", *args])
    print("hgs: Rust executable not found; install hgs 1.11+ alongside this legacy hook", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
