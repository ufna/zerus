#!/usr/bin/env python3
"""Repackage one exact verified Arch binary into an immutable nightly candidate."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from nightly_common import PACKAGE, ROOT, nightly_version, render_recipe
from release_common import require, sha256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--run-id", type=int, required=True)
    parser.add_argument("--run-number", type=int, required=True)
    args = parser.parse_args()
    require(args.run_id > 0, "Invalid nightly run ID.")
    product = (ROOT / "VERSION").read_text().strip()
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    count = int(subprocess.check_output(["git", "rev-list", "--count", "HEAD"], cwd=ROOT, text=True))
    version = nightly_version(product, count, commit, args.run_number)
    args.output.mkdir(parents=True, exist_ok=True)
    require(not any(args.output.iterdir()), "Use an empty nightly output directory.")
    with tempfile.TemporaryDirectory(prefix="zerus-nightly-") as temporary:
        verified = Path(temporary) / "verified"
        # Existing assembly validates package paths, embedded source/version and
        # .BUILDINFO library floors. It never compiles or publishes anything.
        subprocess.run([sys.executable, str(ROOT / "scripts/prepare-release.py"),
                        "--version", product, "--package", str(args.package.resolve()),
                        "--output", str(verified)], cwd=ROOT, check=True)
        info = json.loads((verified / "release-info.json").read_text())
        info.update(version=version, product_version=product, channel="nightly",
                    revision_count=count, workflow_run_id=args.run_id,
                    workflow_run_number=args.run_number, release_tag=f"nightly-{args.run_id}")
        binary = args.output / f"zerus-{version}-arch-x86_64.tar.gz"
        shutil.copyfile(verified / f"zerus-{product}-arch-x86_64.tar.gz", binary)
    recipe = args.output / "aur" / PACKAGE
    recipe.mkdir(parents=True)
    (recipe / "PKGBUILD").write_text(render_recipe(info, sha256(binary)))
    (args.output / "release-info.json").write_text(json.dumps(info, indent=2) + "\n")
    print(f"Prepared exact nightly {version}; validate and seal before publication.")


if __name__ == "__main__":
    main()
