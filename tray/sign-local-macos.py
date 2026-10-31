#!/usr/bin/env python3
"""Sign a Homebrew-linked source bundle without changing its privacy identity.

This is a local development build, not a notarized distribution. It deliberately
does not enable the hardened runtime: its Qt libraries live outside the bundle.
Release bundles use scripts/sign-macos.py and its hardened signing policy.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import subprocess

BUNDLE_ID = "com.hgdev.hgs-tray"
LOCAL_BUNDLE_ID = BUNDLE_ID + ".local"
KINDS = ("Developer ID Application", "Apple Development")
IDENTITY = re.compile(r'^\s*\d+\)\s+([0-9A-Fa-f]{40})\s+"([^"]+)"\s*$', re.M)
MACH_O = {b"\xcf\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xce",
          b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca", b"\xca\xfe\xba\xbf", b"\xbf\xba\xfe\xca"}
PAIR_RECORD = "Contents/Resources/zerus-local-pair.json"


def run(*args):
    return subprocess.run(list(map(str, args)), check=True, capture_output=True, text=True, timeout=60)


def identities():
    result = run("security", "find-identity", "-v", "-p", "codesigning")
    return list(dict.fromkeys((digest.upper(), name) for digest, name in IDENTITY.findall(result.stdout)
                             if any(name.startswith(kind + ":") for kind in KINDS)))


def details(app):
    output = run("codesign", "-d", "--verbose=4", app)
    result = {}
    for line in (output.stdout + output.stderr).splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            result.setdefault(key, value)  # Keep the leaf authority, not its issuer.
    return result


def requirement(app):
    result = run("codesign", "-d", "-r-", app)
    for line in (result.stdout + result.stderr).splitlines():
        if line.startswith("designated => "):
            return line[len("designated => "):]
    raise ValueError("The bundle has no designated signing requirement.")


def bundle_info(app, *, source=False):
    info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    allowed = (BUNDLE_ID, LOCAL_BUNDLE_ID) if source else (BUNDLE_ID,)
    if info.get("CFBundleIdentifier") not in allowed:
        raise ValueError(f"Expected the compatible {BUNDLE_ID} bundle identifier.")
    if info.get("CFBundleExecutable") != "hgs-tray":
        raise ValueError("Expected the hgs-tray bundle executable.")
    return info


def installed_identity(app):
    if not app.exists():
        if app.is_symlink():
            raise ValueError("The installed bundle is a dangling link; preserve it for manual review.")
        return None
    bundle_info(app)
    run("codesign", "--verify", "--deep", "--strict", app)
    current = details(app)
    if current.get("Signature") == "adhoc":
        print("Previous local build is ad hoc; moving to certificate signing may require fresh consent once.")
        return None
    if not any(current["Authority"].startswith(kind + ":") for kind in KINDS):
        raise ValueError("The installed bundle has an unsupported signing identity; preserve it for manual review.")
    if "cdhash" in requirement(app).lower():
        raise ValueError("The installed certificate signature pins a code hash; preserve it for manual review.")
    if not current.get("TeamIdentifier") or current["TeamIdentifier"] == "not set":
        raise ValueError("The installed certificate signature has no team identifier.")
    return current


def select_identity(available, installed, requested=""):
    if requested:
        selected = [item for item in available if requested.upper() == item[0] or requested == item[1]]
    elif installed:
        kind = next(kind for kind in KINDS if installed["Authority"].startswith(kind + ":"))
        selected = [item for item in available if item[1] == installed["Authority"]]
        # Developer ID names include the team. Apple Development names can end
        # in a personal identifier instead, so prefer the exact leaf authority.
        if not selected:
            selected = [item for item in available if item[1].startswith(kind + ":")
                        and item[1].endswith("(" + installed["TeamIdentifier"] + ")")]
    else:
        selected = []
        for kind in KINDS:
            selected = [item for item in available if item[1].startswith(kind + ":")]
            if selected:
                break
    if len(selected) != 1:
        raise ValueError("Need one compatible signing identity with a private key in a searchable keychain. "
                         "Use security find-identity -v -p codesigning and set HGS_MACOS_SIGN_IDENTITY "
                         "to its exact name or SHA-1 fingerprint. Ad hoc signing is not an installation fallback.")
    return selected[0][0]


def verify_continuity(app, installed):
    # Apple TN3127: privacy access is shared only with mutually compatible DRs.
    # Comparing team names or bundle identifiers alone cannot establish this.
    for candidate, previous in ((app, installed), (installed, app)):
        try:
            run("codesign", "--verify", "--strict", "-R", "=" + requirement(previous), candidate)
        except subprocess.CalledProcessError as error:
            raise ValueError("The new and installed signing requirements are incompatible. "
                             "The installed app is unchanged; select its signing identity.\n" + error.stderr.strip()) from error


def source_layout(app, *, paired=False):
    """Validate before running signing tools; never follow nested links or special files."""
    if app.is_symlink():
        raise ValueError("The source bundle contains a link; refusing to sign external code.")
    info = bundle_info(app, source=True)
    executable = app / "Contents/MacOS" / info["CFBundleExecutable"]
    if not executable.is_file() or executable.is_symlink():
        raise ValueError("The source bundle executable is missing or is a link.")
    expected = {executable}
    if paired:
        expected.add(app / "Contents/MacOS/hgs")
        if (app / PAIR_RECORD).exists() or (app / PAIR_RECORD).is_symlink():
            raise ValueError("The source already has pair metadata; prepare a fresh unsigned copy.")
        for path in expected:
            if (not path.is_file() or path.is_symlink() or path.stat().st_nlink != 1
                    or not os.access(path, os.X_OK)):
                raise ValueError("Paired code must be independent regular executable files.")
            with path.open("rb") as stream:
                if stream.read(4) not in MACH_O:
                    raise ValueError("Paired code must contain exactly the two expected Mach-O executables.")
    # Single-code mode stays the default. Paired mode adds only the fixed hgs
    # leaf, never general deep signing or links into Homebrew installations.
    for path in app.rglob("*"):
        if path.is_symlink():
            raise ValueError("The source bundle contains a link; refusing to sign external code.")
        if not path.is_file() and not path.is_dir():
            raise ValueError("The source bundle contains a special file.")
        if path.is_file() and path not in expected:
            with path.open("rb") as stream:
                if stream.read(4) in MACH_O:
                    raise ValueError("Unexpected nested code in the source bundle; use the distribution signer.")
    return info


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(256 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def genuine_signature(path, identifier):
    run("codesign", "--verify", "--strict", "--all-architectures", "-R", "=anchor apple generic", path)
    signed = details(path)
    if (signed.get("Signature") == "adhoc"
            or not any(signed.get("Authority", "").startswith(kind + ":") for kind in KINDS)
            or signed.get("TeamIdentifier", "") in ("", "not set")
            or signed.get("Identifier") != identifier
            or not signed.get("CDHash") or "cdhash" in requirement(path).lower()):
        raise ValueError("Paired code requires genuine, stable Apple application signatures and exact identifiers.")
    return signed


def sign_code(path, identity, identifier=None):
    command = ["codesign", "--force", "--sign", identity, "--options", "0", "--timestamp=none"]
    if identifier:
        command += ["--identifier", identifier]
    try:
        run(*command, path)
    except subprocess.CalledProcessError as error:
        raise ValueError("codesign could not use the selected private key. Test signing from Terminal on the Mac; "
                         "unlock its keychain and respond to any key-access dialog there. "
                         "The installer does not unlock keys or change their access controls. "
                         "See docs/macos-distribution.md#local-source-builds.\n" + error.stderr.strip()) from error


def sign(app, installed, requested="", *, paired=False):
    if app.resolve() == installed.resolve() or app.is_symlink():
        raise ValueError("Signing requires a separate source bundle; the installed app must stay untouched.")
    info = source_layout(app, paired=paired)
    current = installed_identity(installed)
    service = app / "Contents/MacOS/hgs"
    previous_service = installed / "Contents/MacOS/hgs"
    paired_previous = False
    if paired and current:
        genuine_signature(installed, BUNDLE_ID)
        if previous_service.exists() or previous_service.is_symlink():
            if previous_service.is_symlink() or not previous_service.is_file():
                raise ValueError("The installed paired CLI is not a regular file; preserve it for review.")
            previous = genuine_signature(previous_service, "hgs")
            if previous["TeamIdentifier"] != current["TeamIdentifier"]:
                raise ValueError("The installed app and CLI have different signing teams.")
            paired_previous = True
    identity = select_identity(identities(), current, requested)
    # The installer supplies a private copy. Promote only after a compatible key
    # has been selected; failed preparation must retain the local identifier.
    plist = app / "Contents/Info.plist"
    original = plist.read_bytes()
    promoted = info["CFBundleIdentifier"] == LOCAL_BUNDLE_ID
    complete = False
    try:
        if promoted:
            info["CFBundleIdentifier"] = BUNDLE_ID
            plist.write_bytes(plistlib.dumps(info))
        if paired:
            build_hash = sha256(service)
            sign_code(service, identity, "hgs")
            genuine_signature(service, "hgs")
            record = {"schema": 1, "build_sha256": build_hash, "signed_sha256": sha256(service)}
            record_path = app / PAIR_RECORD
            record_path.parent.mkdir(parents=True, exist_ok=True)
            record_path.write_text(json.dumps(record, sort_keys=True) + "\n")
        sign_code(app, identity)
        run("codesign", "--verify", "--deep", "--strict", app)
        signed = details(app)
        if (signed.get("Signature") == "adhoc" or not any(signed.get("Authority", "").startswith(kind + ":")
                                                        for kind in KINDS)
                or "cdhash" in requirement(app).lower()):
            raise ValueError("The source bundle did not receive a stable certificate signature.")
        if current:
            verify_continuity(app, installed)
        if paired:
            application = genuine_signature(app, BUNDLE_ID)
            cli = genuine_signature(service, "hgs")
            if (application["TeamIdentifier"] != cli["TeamIdentifier"]
                    or application["CDHash"] == cli["CDHash"]):
                raise ValueError("The paired app and distinct CLI must have the same signing team.")
            if paired_previous:
                verify_continuity(service, previous_service)
            if (json.loads((app / PAIR_RECORD).read_text()) != record
                    or sha256(service) != record["signed_sha256"]):
                raise ValueError("The signed CLI or sealed pair metadata changed during preparation.")
            # Last check covers both nested code and the new resource seal.
            run("codesign", "--verify", "--deep", "--strict", "--all-architectures", app)
        if current:
            print("Local certificate signature verified; privacy identity matches the installed application.")
        else:
            print("Local certificate signature verified; subsequent compatible rebuilds retain this identity.")
        complete = True
        return record if paired else None
    finally:
        if promoted and not complete:
            plist.write_bytes(original)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=Path)
    parser.add_argument("installed", type=Path)
    parser.add_argument("--paired", action="store_true", help="sign only the fixed app plus Contents/MacOS/hgs pair")
    args = parser.parse_args()
    try:
        sign(args.app, args.installed, os.environ.get("HGS_MACOS_SIGN_IDENTITY", ""), paired=args.paired)
    except (ValueError, OSError, KeyError, subprocess.SubprocessError) as error:
        message = error.stderr.strip() if isinstance(error, subprocess.CalledProcessError) else str(error)
        parser.exit(1, f"Local macOS signing failed: {message}\n")


if __name__ == "__main__":
    main()
