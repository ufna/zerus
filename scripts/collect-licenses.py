#!/usr/bin/env python3
"""Collect exact host-target Cargo dependency notices for binary distribution."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    target = next(line[6:] for line in subprocess.check_output(["rustc", "-vV"], text=True).splitlines()
                  if line.startswith("host: "))
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--offline", "--format-version", "1", "--filter-platform", target], text=True))
    resolved = {node["id"] for node in metadata["resolve"]["nodes"]}
    destination = args.destination.resolve()
    destination.mkdir(parents=True, exist_ok=True)
    inventory = []
    for package in sorted(metadata["packages"], key=lambda item: (item["name"], item["version"])):
        if not package.get("source") or package["id"] not in resolved:
            continue
        source = Path(package["manifest_path"]).parent
        notices = [path for path in source.rglob("*") if path.is_file()
                   and path.name.upper().startswith(("LICENSE", "COPYING", "NOTICE", "UNLICENSE"))]
        if package.get("license_file"):
            notices.append(source / package["license_file"])
        notices = sorted(set(notices))
        if not notices:
            raise SystemExit(f"No license text found for {package['name']} {package['version']}; review before distribution.")
        folder = destination / f"{package['name']}-{package['version']}"
        for notice in notices:
            relative = notice.relative_to(source)
            output = folder / relative
            output.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(notice, output)
        if package["name"] == "libsqlite3-sys":
            # This crate's own MIT notice does not describe bundled SQLite.
            # Preserve the exact upstream public-domain dedication too.
            header = (source / "sqlite3/sqlite3.h").read_text().split("*************************************************************************", 1)[0]
            if "The author disclaims copyright" not in header:
                raise SystemExit("Review the bundled SQLite dedication before distribution.")
            (folder / "SQLite-public-domain.txt").write_text(header + "*/\nhttps://www.sqlite.org/copyright.html\n")
        inventory.append({key: package.get(key) for key in ("name", "version", "license", "repository")})
    (destination / "inventory.json").write_text(json.dumps({"target": target, "packages": inventory}, indent=2) + "\n")
    print(f"Collected notices for {len(inventory)} host-target dependencies.")


if __name__ == "__main__":
    main()
