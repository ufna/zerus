#!/usr/bin/env python3
"""Fetch one successful trusted manual candidate; verify run, digest and contents."""
import argparse
import json
from pathlib import Path
import re
import stat
import subprocess
import tempfile
import zipfile

from release_common import REPOSITORY, api, require, sha256, validate_candidate


def validate_run(run, nightly=False):
    if nightly:
        require(run.get("path") == ".github/workflows/nightly.yml" and
                run.get("event") in {"schedule", "workflow_dispatch"}, "Not a trusted nightly workflow.")
    else:
        require(run.get("path") == ".github/workflows/release.yml" and run.get("event") == "workflow_dispatch", "Not a manual release-candidate workflow.")
    require(run.get("head_branch") == "main" and run.get("head_repository", {}).get("full_name") == REPOSITORY, "Candidate must come from upstream main.")
    require(run.get("status") == "completed" and run.get("conclusion") == "success", "Candidate checks have not succeeded.")
    require(re.fullmatch(r"[0-9a-f]{40}", run.get("head_sha", "")), "Invalid candidate commit.")
    return run["head_sha"]


def unpack(path, output):
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        require(sum(entry.file_size for entry in entries) < 500_000_000, "Candidate is unexpectedly large.")
        seen = set()
        for entry in entries:
            name = Path(entry.filename)
            require(not name.is_absolute() and ".." not in name.parts and entry.filename not in seen, "Unsafe/duplicate artifact entry.")
            require(not stat.S_ISLNK(entry.external_attr >> 16), "Artifact symlinks are unsupported.")
            seen.add(entry.filename)
        archive.extractall(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", type=int, required=True)
    parser.add_argument("--version")
    parser.add_argument("--nightly", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    require(args.run_id > 0, "Invalid candidate run ID.")
    require(args.nightly or re.fullmatch(r"\d+\.\d+\.\d+", args.version or ""), "Invalid version.")
    require(not args.output.exists(), "Use a fresh candidate output directory.")
    run = api(f"actions/runs/{args.run_id}")
    commit = validate_run(run, nightly=args.nightly)
    artifacts = api(f"actions/runs/{args.run_id}/artifacts?per_page=100")["artifacts"]
    name = f"zerus-nightly-{args.run_id}-candidate" if args.nightly else f"zerus-{args.version}-candidate"
    matches = [item for item in artifacts if item["name"] == name]
    require(len(matches) == 1 and not matches[0]["expired"], "One unexpired candidate artifact is required; rebuild deliberately if expired.")
    artifact = matches[0]
    require(artifact.get("workflow_run", {}).get("head_sha") == commit, "Artifact provenance mismatch.")
    require(re.fullmatch(r"sha256:[0-9a-f]{64}", artifact.get("digest", "")), "Missing GitHub artifact digest.")
    with tempfile.TemporaryDirectory(prefix="zerus-download-") as temporary:
        path = Path(temporary) / "candidate.zip"
        with path.open("wb") as stream:
            subprocess.run(["gh", "api", f"repos/{REPOSITORY}/actions/artifacts/{artifact['id']}/zip"], stdout=stream, check=True)
        require("sha256:" + sha256(path) == artifact["digest"], "GitHub artifact digest mismatch.")
        unpack(path, args.output)
    if args.nightly:
        from nightly_common import validate_nightly
        info, _ = validate_nightly(args.output, commit, args.run_id, run["run_number"])
    else:
        info, _ = validate_candidate(args.output, args.version, commit)
    print(json.dumps({"version": info["version"], "source_commit": commit, "candidate_run": args.run_id}))


if __name__ == "__main__":
    main()
