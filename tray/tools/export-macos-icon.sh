#!/usr/bin/env bash
set -euo pipefail

# Regenerate the native app icon with Xcode 26+ / Icon Composer support.
# Normal application builds use the checked-in results, including on older SDKs.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ICON_ROOT="${SCRIPT_DIR}/../resources/icons"
ICON_BUILD="$(mktemp -d "${TMPDIR:-/tmp}/hgs-macos-icon.XXXXXX")"
trap 'rm -rf "${ICON_BUILD}"' EXIT

xcrun actool "${ICON_ROOT}/hgs-zerus-swarm.icon" \
    --compile "${ICON_BUILD}" \
    --platform macosx --minimum-deployment-target 12.0 \
    --app-icon hgs-zerus-swarm \
    --output-partial-info-plist "${ICON_BUILD}/Info.plist" \
    --output-format human-readable-text --notices --warnings

mkdir -p "${ICON_ROOT}/macos"
cp -f "${ICON_BUILD}/hgs-zerus-swarm.icns" "${ICON_ROOT}/hgs-zerus-swarm.icns"
cp -f "${ICON_BUILD}/Assets.car" "${ICON_ROOT}/macos/Assets.car"
