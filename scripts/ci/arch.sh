#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
git rev-parse --verify HEAD > /dev/null
bash scripts/ci/gui.sh
bash scripts/build-arch-package.sh
