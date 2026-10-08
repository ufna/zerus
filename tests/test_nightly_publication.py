"""Contracts for trusted nightly provenance, immutable assets and AUR upgrades."""
import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import nightly_common as nightly
import release_common as common


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


fetch = load("nightly_fetch", "fetch-release-candidate.py")
publish = load("nightly_publish", "publish-release.py")
COMMIT = "a" * 40


class NightlyTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zerus-nightly-contract-")
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.info = {"schema": 1, "channel": "nightly", "publication": "candidate",
                     "product_version": "0.37.0", "revision_count": 60, "source_commit": COMMIT,
                     "workflow_run_id": 123, "workflow_run_number": 7, "release_tag": "nightly-123",
                     "version": nightly.nightly_version("0.37.0", 60, COMMIT, 7),
                     "architecture": "x86_64", "distribution": "Arch Linux", "native_agents_bundled": False,
                     "binary_dependencies": {name: "1.2.3" for name in common.BINARY_LIBRARIES}}

    def candidate(self):
        directory = self.directory / "candidate"
        directory.mkdir()
        version = self.info["version"]
        binary = directory / f"zerus-{version}-arch-x86_64.tar.gz"
        binary.write_bytes(b"verified binary")
        (directory / f"{nightly.PACKAGE}-{version}-1-x86_64.pkg.tar.zst").write_bytes(b"checked package")
        (directory / "release-info.json").write_text(json.dumps(self.info))
        recipe = directory / "aur" / nightly.PACKAGE
        recipe.mkdir(parents=True)
        digest = common.sha256(binary)
        (recipe / "PKGBUILD").write_text(nightly.render_recipe(self.info, digest))
        (recipe / ".SRCINFO").write_text(
            f"pkgbase = {nightly.PACKAGE}\n\tpkgver = {version}\n\tpkgrel = 1\n\tarch = x86_64\n"
            f"\tsource = https://github.com/ufna/zerus/releases/download/nightly-123/{binary.name}\n"
            f"\tsha256sums = {digest}\n" +
            "".join(f"\tdepends = {name}>=1.2.3\n" for name in common.BINARY_LIBRARIES) +
            "".join(f"\tdepends = {name}\n" for name in ("tmux>=3.7", "openssh", "python", "curl", "procps-ng", "hicolor-icon-theme", "bash", "tar")) +
            "".join(f"\tconflicts = {name}\n" for name in ("zerus", "zerus-git", "zerus-ade-bin")) +
            f"\tprovides = zerus={version}\n\tprovides = hgs\n\tprovides = hgs-tray\npkgname = {nightly.PACKAGE}\n")
        common.finish_candidate(directory, nightly.PACKAGES)
        return directory

    def test_only_successful_upstream_scheduled_or_manual_runs_are_trusted(self):
        run = {"path": ".github/workflows/nightly.yml", "event": "schedule", "head_branch": "main",
               "head_repository": {"full_name": "ufna/zerus"}, "status": "completed",
               "conclusion": "success", "head_sha": COMMIT}
        self.assertEqual(fetch.validate_run(run, nightly=True), COMMIT)
        for updates in ({"event": "pull_request"}, {"path": ".github/workflows/ci.yml"},
                        {"head_branch": "feature"}, {"conclusion": "failure"},
                        {"status": "in_progress"}, {"head_repository": {"full_name": "example/zerus"}}):
            with self.subTest(updates=updates), self.assertRaises(ValueError):
                fetch.validate_run({**run, **updates}, nightly=True)

    def test_exact_commit_run_and_build_number_are_required(self):
        directory = self.candidate()
        info, hashes = nightly.validate_nightly(directory, COMMIT, 123, 7)
        self.assertEqual(info["release_tag"], "nightly-123")
        self.assertEqual(len(hashes), 4)
        for commit, run, number in (("b" * 40, 123, 7), (COMMIT, 124, 7), (COMMIT, 123, 8)):
            with self.assertRaises(ValueError):
                nightly.validate_nightly(directory, commit, run, number)

    def test_asset_and_recipe_tampering_fail_before_publication(self):
        directory = self.candidate()
        path = directory / "aur" / nightly.PACKAGE / "PKGBUILD"
        path.write_text(path.read_text() + "\nmalicious_command\n")
        with self.assertRaisesRegex(ValueError, "Unsealed nightly recipe"):
            nightly.validate_nightly(directory)
        common.finish_candidate(directory, nightly.PACKAGES)
        with self.assertRaisesRegex(ValueError, "trusted binary template"):
            nightly.validate_nightly(directory)

    def test_moving_url_missing_floors_and_extra_files_are_rejected(self):
        directory = self.candidate()
        path = directory / "aur" / nightly.PACKAGE / ".SRCINFO"
        original = path.read_text()
        for replacement in (original.replace("nightly-123", "nightly"),
                            original.replace("depends = qt6-base>=1.2.3", "depends = qt6-base")):
            path.write_text(replacement)
            common.finish_candidate(directory, nightly.PACKAGES)
            with self.assertRaises(ValueError):
                nightly.validate_nightly(directory)
        path.write_text(original)
        common.finish_candidate(directory, nightly.PACKAGES)
        (directory / "unsealed.txt").write_text("unexpected")
        with self.assertRaisesRegex(ValueError, "Unsealed/missing"):
            nightly.validate_nightly(directory)

    def test_rebuild_of_same_commit_upgrades_without_changing_stable_version(self):
        first = nightly.nightly_version("0.37.0", 60, COMMIT, 7)
        next_build = nightly.nightly_version("0.37.0", 60, COMMIT, 8)
        def version(value): return common.package_version({"pkgver": [value], "pkgrel": ["1"]})
        self.assertGreater(version(next_build), version(first))
        self.assertGreater(version(nightly.nightly_version("0.37.0", 61, "b" * 40, 9)), version(next_build))
        self.assertGreater(version("0.38.0"), version(next_build))

    def test_nightly_never_becomes_latest_stable_and_retry_changes_nothing(self):
        directory = self.candidate()
        info, sums = nightly.validate_nightly(directory)
        release = {"draft": True, "target_commitish": COMMIT, "tag_name": "nightly-123",
                   "assets": [{"name": name, "size": (directory / name).stat().st_size,
                               "digest": "sha256:" + digest}
                              for name, digest in {**sums, "SHA256SUMS": common.sha256(directory / "SHA256SUMS")}.items()]}
        commands = []
        def gh(*args):
            commands.append(args)
            if args[:2] == ("release", "edit"): release["draft"] = False
        class Download:
            def __init__(self, path): self.stream = path.open("rb")
            def __enter__(self): return self
            def __exit__(self, *args): self.stream.close()
            def geturl(self): return "https://example.com/asset"
            def readable(self): return True
            def read(self, *args): return self.stream.read(*args)
            def readinto(self, buffer): return self.stream.readinto(buffer)
        with patch.object(publish, "release_for_tag", side_effect=lambda _: copy.deepcopy(release)), \
                patch.object(publish, "verify_tag", return_value=True), patch.object(publish, "gh", side_effect=gh), \
                patch.object(publish.urllib.request, "urlopen", side_effect=lambda url, **kw: Download(directory / url.rsplit("/", 1)[1])):
            publish.publish_github(directory, info, sums, nightly=True)
            self.assertEqual(len(commands), 1)
            self.assertIn("--latest=false", commands[0])
            self.assertIn("--prerelease", commands[0])
            commands.clear()
            publish.publish_github(directory, info, sums, nightly=True)
            self.assertEqual(commands, [])


if __name__ == "__main__":
    unittest.main()
