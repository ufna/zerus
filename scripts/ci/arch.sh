#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
bash scripts/ci/gui.sh
bash scripts/build-arch-package.sh
