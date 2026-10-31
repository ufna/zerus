#!/usr/bin/env python3
"""Prepare a signed local app/CLI pair in a fresh private output directory.

This headless step never installs, registers or starts an application or service.
The previous app is an explicit read-only input; no owner installation is guessed.
"""
import argparse
import importlib.util
import os
from pathlib import Path
import shutil
import stat
import subprocess

spec = importlib.util.spec_from_file_location("local_sign", Path(__file__).with_name("sign-local-macos.py"))
signer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signer)


def overlaps(a, b):
    return a == b or a in b.parents or b in a.parents


def prepare(app, hgs, output, installed, requested=""):
    """Only a successfully verified private copy is returned to the caller."""
    if app.is_symlink() or hgs.is_symlink() or installed.is_symlink():
        raise ValueError("App, CLI and previous-app inputs must not be symbolic links.")
    app = app.resolve(strict=True)
    hgs = hgs.resolve(strict=True)
    installed = installed.resolve()
    # Resolve the parent, not an existing final leaf. mkdir below reserves that
    # leaf without ever replacing a file, directory or dangling link.
    output = output.parent.resolve(strict=True) / output.name
    parent = output.parent.stat()
    if parent.st_uid != os.getuid() or parent.st_mode & 0o077:
        raise ValueError("The output prefix must be owned by this user and private (mode 0700).")
    if output.name != "hgs-tray.app":
        raise ValueError("The prepared output must keep the hgs-tray.app basename.")
    if output.exists() or output.is_symlink():
        raise ValueError("The output already exists; use a fresh scratch prefix.")
    if any(overlaps(output, source) for source in (app, hgs, installed)):
        raise ValueError("The output must not overlap any input or previous installation.")
    signer.source_layout(app)
    if (app / "Contents/MacOS/hgs").exists():
        raise ValueError("The source app already contains a CLI; use a fresh single-code build.")
    if not hgs.is_file() or not os.access(hgs, os.X_OK):
        raise ValueError("The built CLI must be a regular executable file.")
    with hgs.open("rb") as stream:
        if stream.read(4) not in signer.MACH_O:
            raise ValueError("The built CLI is not Mach-O.")
    output.mkdir(mode=0o700)
    owned = output.stat()
    try:
        # Preserve any raced-in link, so the signer's fresh scan refuses it;
        # never follow a nested source link into external code.
        shutil.copytree(app, output, symlinks=True, dirs_exist_ok=True)
        output.chmod(0o700)
        shutil.copy2(hgs, output / "Contents/MacOS/hgs", follow_symlinks=False)
        return signer.sign(output, installed, requested, paired=True)
    except BaseException:
        # A replaced output is uncertain. Preserve it rather than deleting a
        # path we no longer own. rmtree itself never follows a root symlink.
        try:
            current = output.lstat()
        except FileNotFoundError:
            pass
        else:
            if stat.S_ISDIR(current.st_mode) and (current.st_dev, current.st_ino) == (owned.st_dev, owned.st_ino):
                shutil.rmtree(output)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, required=True, help="built single-executable source app")
    parser.add_argument("--hgs", type=Path, required=True, help="matching root Cargo build artifact")
    parser.add_argument("--output", type=Path, required=True, help="fresh scratch prefix's hgs-tray.app")
    parser.add_argument("--installed", type=Path, required=True, help="previous app, or a nonexistent scratch path")
    args = parser.parse_args()
    try:
        prepare(args.app, args.hgs, args.output, args.installed, os.environ.get("HGS_MACOS_SIGN_IDENTITY", ""))
    except (ValueError, OSError, KeyError, subprocess.SubprocessError) as error:
        message = error.stderr.strip() if isinstance(error, subprocess.CalledProcessError) else str(error)
        parser.exit(1, f"Local macOS pair preparation failed: {message}\n")


if __name__ == "__main__":
    main()
