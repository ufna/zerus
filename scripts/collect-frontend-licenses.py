#!/usr/bin/env python3
"""Collect the locked production frontend packages' notices, without running npm."""
import argparse
import json
from pathlib import Path

from release_common import require

ROOT = Path(__file__).resolve().parents[1]
LIMIT = 1024 * 1024


def read(path):
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= LIMIT,
            f"Missing or oversized frontend notice input: {path}")
    with path.open("rb") as stream:
        data = stream.read(LIMIT + 1)
    require(len(data) <= LIMIT, f"Frontend notice input exceeds its limit: {path}")
    return data


def collect(client, destination):
    modules = (client / "node_modules").resolve()
    lock = json.loads(read(client / "package-lock.json"))
    require(isinstance(lock, dict), "Unsupported frontend lockfile.")
    packages = lock.get("packages")
    require(isinstance(packages, dict) and len(packages) <= 4096, "Unsupported frontend lockfile.")
    inventory, planned, total = [], [], 0
    for location, item in sorted(packages.items()):
        require(isinstance(item, dict), "Invalid frontend lockfile entry.")
        if not location or item.get("dev") is True:
            continue
        require(isinstance(item.get("version"), str) and item["version"], "Missing locked frontend version.")
        relative = Path(location)
        require(location.startswith("node_modules/") and not relative.is_absolute()
                and ".." not in relative.parts, "Invalid locked frontend package path.")
        source = client / relative
        require(not source.is_symlink() and source.resolve().is_relative_to(modules),
                "Frontend notices must come from this installed package tree.")
        metadata = json.loads(read(source / "package.json"))
        require(isinstance(metadata, dict) and isinstance(metadata.get("name"), str)
                and metadata.get("version") == item.get("version"),
                f"Frontend package differs from its lockfile: {location}")
        names = []
        # Notices in these packages live at the package root. No unbounded
        # recursive walk into node_modules, examples, generated maps or caches.
        for count, path in enumerate(source.iterdir()):
            require(count < 4096, "Frontend package directory exceeds its entry limit.")
            if path.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE")):
                data = read(path)
                total += len(data)
                require(total <= 16 * LIMIT and len(planned) < 4096, "Frontend notices exceed their byte/file limits.")
                names.append(path.name)
                planned.append((relative / path.name, data))
        require(names, f"No license text found for frontend package {metadata['name']}.")
        inventory.append({"name": metadata["name"], "version": metadata["version"],
                          "license": metadata.get("license"), "files": sorted(names)})
    require(inventory and len(planned) <= 4096, "Frontend notice inventory is empty or too large.")
    require(not destination.exists(), "Frontend notice destination must be new.")
    destination.mkdir(parents=True)
    for relative, data in planned:
        output = destination / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(data)
    (destination / "inventory.json").write_text(json.dumps(inventory, indent=2) + "\n", encoding="utf-8")
    return inventory


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--client", type=Path, default=ROOT / "client")
    args = parser.parse_args()
    inventory = collect(args.client, args.destination)
    print(f"Collected notices for {len(inventory)} locked production frontend packages.")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError) as error:
        raise SystemExit(f"collect-frontend-licenses: {error}") from None
