#!/usr/bin/env bash
# Check the public nightly recipe against local immutable files, without secrets.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ "$(id -u)" = 0 ]; then echo 'makepkg must run as an unprivileged user.' >&2; exit 1; fi
nightly_dir=$(realpath "${1:-dist}")
repo_root="$PWD"
recipe_dir="$nightly_dir/aur/zerus-ade-nightly-bin"
work="$repo_root/.ci-build/nightly-package"
mkdir -p "$work" artifacts/test-results
(cd "$recipe_dir"; makepkg --printsrcinfo > .SRCINFO)
cp "$recipe_dir/PKGBUILD" "$work/PKGBUILD"
python3 - "$work/PKGBUILD" "$nightly_dir" <<'PY'
from pathlib import Path
import json, sys
recipe, directory = Path(sys.argv[1]), Path(sys.argv[2])
version = json.loads((directory / "release-info.json").read_text())["version"]
uri = (directory / f"zerus-{version}-arch-x86_64.tar.gz").as_uri()
assert "'" not in uri
text = recipe.read_text()
start, end = text.index("source=("), text.index("\n", text.index("source=("))
recipe.write_text(text[:start] + "source=('" + uri + "')" + text[end:])
PY
(
    cd "$work"
    makepkg --cleanbuild --force --noconfirm 2>&1 | tee "$repo_root/artifacts/test-results/nightly-makepkg.log"
    namcap "$recipe_dir/PKGBUILD" ./*.pkg.tar.zst 2>&1 | tee "$repo_root/artifacts/test-results/nightly-namcap.log"
    if grep -q ' E: ' "$repo_root/artifacts/test-results/nightly-namcap.log"; then exit 1; fi
    cp ./*.pkg.tar.zst "$nightly_dir/"
)
python3 scripts/check-package.py --root "$work/pkg/zerus-ade-nightly-bin" --package-name zerus-ade-nightly-bin
python3 - "$nightly_dir" <<'PY'
from pathlib import Path
import sys
sys.path.insert(0, "scripts")
from nightly_common import PACKAGES, validate_nightly
from release_common import finish_candidate
directory = Path(sys.argv[1])
finish_candidate(directory, packages=PACKAGES)
validate_nightly(directory)
PY
printf 'Nightly binary package, library floors and sealed AUR metadata validated.\n'
