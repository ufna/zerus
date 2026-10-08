#!/usr/bin/env python3
"""Publish an immutable verified GitHub release, then synchronize its AUR recipes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import urllib.error
import urllib.parse
import urllib.request

from release_common import (PACKAGES, REPOSITORY, api, gh, metadata, package_version,
                            recipe_changed, require, sha256, validate_candidate)

HOST_FINGERPRINT = "SHA256:RFzBCUItH9LZS0cKB5UE6ceAYhBD5C8GeOBip8Z11+4"


def git(directory, *arguments, env=None):
    return subprocess.check_output(["git", "-C", str(directory), *arguments], env=env, text=True).strip()


def ssh_environment(identity, hosts):
    fingerprint = subprocess.check_output(["ssh-keygen", "-lf", str(hosts), "-E", "sha256"], text=True)
    require(len(fingerprint.splitlines()) == 1 and fingerprint.split()[1] == HOST_FINGERPRINT, "Unverified AUR host key; consult the official AUR homepage.")
    require(identity.is_file() and identity.stat().st_mode & 0o077 == 0, "AUR private key must be a private file (0600).")
    command = ["ssh", "-F", "/dev/null", "-i", str(identity.resolve()), "-o", "IdentitiesOnly=yes",
               "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes", "-o", "HostKeyAlgorithms=ssh-ed25519",
               "-o", f"UserKnownHostsFile={hosts.resolve()}", "-o", "ConnectTimeout=10"]
    env = os.environ.copy()
    env["GIT_SSH_COMMAND"] = shlex.join(command)
    return command, env


def prepare_aur(candidate, workspace, ssh, env):
    # AUR authentication/ownership failures must precede any public GitHub write.
    owned = {name.lstrip("*") for name in subprocess.check_output([*ssh, "aur@aur.archlinux.org", "list-repos"], text=True).split()}
    query = urllib.parse.urlencode([("arg[]", package) for package in PACKAGES])
    with urllib.request.urlopen("https://aur.archlinux.org/rpc/v5/info?" + query, timeout=30) as response:
        existing = json.load(response)["results"]
    require(all(item["PackageBase"] in owned for item in existing), "An AUR name belongs to another account; do not overwrite it.")
    plans = []
    for package in PACKAGES:
        directory = workspace / package
        subprocess.run(["git", "-c", "init.defaultBranch=master", "clone", "--quiet",
                        f"ssh://aur@aur.archlinux.org/{package}.git", str(directory)], env=env, check=True)
        tracked = git(directory, "ls-files").splitlines()
        require(set(tracked) <= {"PKGBUILD", ".SRCINFO"}, f"Unexpected existing AUR files: {package}; inspect manually.")
        source = candidate / "aur" / package
        changed = False
        for name in ("PKGBUILD", ".SRCINFO"):
            old = (directory / name).read_text() if (directory / name).exists() else ""
            new = (source / name).read_text()
            changed |= recipe_changed(old, new, vcs=package == "zerus-git")
        if not changed:
            print(f"{package}: recipe already synchronized.")
            continue
        if (directory / ".SRCINFO").exists():
            old = metadata((directory / ".SRCINFO").read_text())
            new = metadata((source / ".SRCINFO").read_text())
            require(old.get("pkgbase") == [package] and old.get("pkgname") == [package], f"Unexpected existing package: {package}")
            require(package_version(new) >= package_version(old), f"Refusing an AUR downgrade: {package}")
            if package != "zerus-git":
                require(package_version(new) > package_version(old), f"Changed stable recipe needs a pkgrel/version bump: {package}")
        for name in ("PKGBUILD", ".SRCINFO"):
            (directory / name).write_bytes((source / name).read_bytes())
        git(directory, "add", "PKGBUILD", ".SRCINFO")
        git(directory, "diff", "--cached", "--check")
        git(directory, "-c", "user.name=Vladimir Alyamkin", "-c", "user.email=ufna@ufna.dev",
            "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "Update reviewed Zerus release recipe")
        # Opening receive-pack also checks AUR email verification/write access.
        # --dry-run changes no refs and runs before any GitHub release write.
        git(directory, "push", "--dry-run", "origin", "HEAD:master", env=env)
        plans.append((package, directory))
    return plans


def release_for_tag(tag):
    # Inspect the actual HTTP status instead of treating every API error as absence.
    result = subprocess.run(["gh", "api", f"repos/{REPOSITORY}/releases/tags/{tag}"], capture_output=True, text=True)
    if result.returncode == 0:
        return json.loads(result.stdout)
    require("HTTP 404" in result.stderr, "Cannot inspect GitHub release: " + result.stderr.strip())
    return None


def verify_tag(tag, commit):
    result = subprocess.run(["gh", "api", f"repos/{REPOSITORY}/git/ref/tags/{tag}"], capture_output=True, text=True)
    if result.returncode:
        require("HTTP 404" in result.stderr, "Cannot inspect release tag: " + result.stderr.strip())
        return False
    target = json.loads(result.stdout)["object"]
    for _ in range(5):
        if target["type"] != "tag":
            break
        target = api(f"git/tags/{target['sha']}")["object"]
    require(target["type"] == "commit" and target["sha"] == commit, "Existing release tag points at a different commit.")
    return True


def verify_assets(release, candidate, hashes, allow_missing=False):
    assets = {asset["name"]: asset for asset in release["assets"]}
    require(len(assets) == len(release["assets"]) and set(assets) <= set(hashes), "Unexpected/duplicate existing release assets.")
    if not allow_missing:
        require(set(assets) == set(hashes), "Published release has missing assets; never repair it by replacing content.")
    for name, asset in assets.items():
        digest = asset.get("digest")
        if digest:
            require(digest == "sha256:" + hashes[name], f"Existing immutable asset differs: {name}")
        else:
            with tempfile.TemporaryDirectory(prefix="zerus-asset-") as temporary:
                gh("release", "download", release["tag_name"], "--repo", REPOSITORY, "--pattern", name, "--dir", temporary)
                require(sha256(Path(temporary) / name) == hashes[name], f"Existing immutable asset differs: {name}")
        require(asset["size"] == (candidate / name).stat().st_size, f"Existing asset size differs: {name}")
    return set(hashes) - set(assets)


def publish_github(candidate, info, sums):
    version, commit = info["version"], info["source_commit"]
    tag = "v" + version
    hashes = {**sums, "SHA256SUMS": sha256(candidate / "SHA256SUMS")}
    verify_tag(tag, commit)
    release = release_for_tag(tag)
    if release is None:
        notes = (f"Arch Linux x86_64 ADE bundle.\n\n"
                 f"Install a stable binary with `yay -S zerus-ade-bin`, stable sources with `yay -S zerus`, "
                 f"or upstream main with `yay -S zerus-git`. Run `zerus-setup` once as your normal user. "
                 f"AUR updates follow GitHub publication; consult the publication workflow if an update is pending.\n\n"
                 f"Native agents are installed separately. Services and autostart are opt-in.\n\n"
                 f"Source commit: `{commit}`. Verify downloads using `SHA256SUMS`.\n")
        with tempfile.TemporaryDirectory(prefix="zerus-notes-") as temporary:
            path = Path(temporary) / "notes.md"
            path.write_text(notes)
            gh("release", "create", tag, "--repo", REPOSITORY, "--draft", "--target", commit,
               "--title", f"Zerus {version}", "--notes-file", str(path))
        release = release_for_tag(tag)
    if release["draft"]:
        require(release["target_commitish"] == commit, "Existing draft targets another commit or a moving branch.")
    else:
        require(verify_tag(tag, commit), "Published release tag is missing.")
    missing = verify_assets(release, candidate, hashes, allow_missing=release["draft"])
    for name in sorted(missing):
        gh("release", "upload", tag, str(candidate / name), "--repo", REPOSITORY)
    if release["draft"]:
        release = release_for_tag(tag)
        verify_assets(release, candidate, hashes)
        verify_tag(tag, commit)  # Existing draft/tag must still name the verified commit.
        gh("release", "edit", tag, "--repo", REPOSITORY, "--draft=false", "--latest")
    require(verify_tag(tag, commit), "Published release tag is missing.")
    # AUR recipes must resolve without a GitHub login or token.
    for name, expected in hashes.items():
        url = f"https://github.com/{REPOSITORY}/releases/download/{tag}/{name}"
        with urllib.request.urlopen(url, timeout=60) as response:
            require(response.geturl().startswith("https://"), "Release download redirected outside HTTPS.")
            digest = hashlib.file_digest(response, "sha256").hexdigest()
        require(digest == expected, f"Anonymous release checksum mismatch: {name}")
    print(f"GitHub {tag}: all immutable assets verified anonymously.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--ssh-key", type=Path, required=True)
    parser.add_argument("--known-hosts", type=Path, default=Path("packaging/aur/known_hosts"))
    args = parser.parse_args()
    info, sums = validate_candidate(args.candidate, args.version)
    ssh, env = ssh_environment(args.ssh_key, args.known_hosts)
    with tempfile.TemporaryDirectory(prefix="zerus-aur-") as temporary:
        plans = prepare_aur(args.candidate, Path(temporary), ssh, env)
        publish_github(args.candidate, info, sums)
        for package, directory in plans:
            try:
                git(directory, "push", "origin", "HEAD:master", env=env)
            except subprocess.CalledProcessError:
                raise SystemExit(f"GitHub assets remain published; {package} AUR update failed. Rerun publication with this same candidate; do not rebuild or replace assets.")
            print(f"{package}: AUR update published.")
    print(f"Zerus {args.version}: GitHub and all AUR recipes synchronized.")


if __name__ == "__main__":
    main()
