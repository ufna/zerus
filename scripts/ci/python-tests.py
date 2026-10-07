#!/usr/bin/env python3
"""Run each integration module in a fresh process; fixture globals stay local."""
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def main():
    results = ROOT / "artifacts/test-results/python"
    results.mkdir(parents=True, exist_ok=True)
    failed = []
    for path in sorted((ROOT / "tests").glob("test_*.py")):
        print(f"\nRunning {path.name}", flush=True)
        command = [sys.executable, "-m", "unittest", "discover", "-s", "tests", "-p", path.name, "-v"]
        try:
            result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=600)
            output = result.stdout + result.stderr
            (results / f"{path.stem}.log").write_text(output)
            print(output, end="", flush=True)
            if result.returncode:
                failed.append(path.name)
        except subprocess.TimeoutExpired:
            failed.append(path.name)
            print(f"FAIL: {path.name} exceeded 10 minutes.", flush=True)
    print(f"Integration modules: {len(list((ROOT / 'tests').glob('test_*.py')))}, failed: {len(failed)}")
    if failed:
        print("Failed modules: " + ", ".join(failed))
    return bool(failed)


if __name__ == "__main__":
    sys.exit(main())
