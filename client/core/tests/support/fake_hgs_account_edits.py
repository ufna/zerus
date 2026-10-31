#!/usr/bin/env python3
"""Private account-edit fixture. Never reads hgs state or credentials."""
import json
import sys
from pathlib import Path

root = Path(__file__).parent
args = sys.argv[1:]
with (root / "calls.jsonl").open("a") as log:
    log.write(json.dumps(args) + "\n")
if args[0].startswith("@"):
    assert args.pop(0) == "@studio"
assert args[:2] == ["--expect-machine", "11111111-1111-4111-8111-111111111111"]
assert args[2] == "account"
catalog = json.loads((root / "catalog.json").read_text())
if args[3] == "ls":
    print(json.dumps(catalog))
    sys.exit()
profile = next(p for p in catalog["profiles"] if p["id"] == args[4])
assert args[args.index("--revision") + 1] == catalog["revision"]
if args[3] == "rename":
    profile["label"] = args[args.index("--label") + 1]
elif args[3] == "default":
    profile["is_default"] = True
elif args[3] == "rm":
    catalog.setdefault("removed_profiles", []).append(profile)
    catalog["profiles"].remove(profile)
else:
    raise AssertionError(args)
catalog["revision"] = "r" + str(int(catalog["revision"][1:]) + 1)
(root / "catalog.json").write_text(json.dumps(catalog))
if (root / "uncertain").exists():
    print("Synthetic uncertain result", file=sys.stderr)
    sys.exit(7)
print(json.dumps(catalog))
