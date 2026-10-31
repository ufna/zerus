"""Paired local preparation controls; real signature evidence is separately opt in."""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import plistlib
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("prepare_mac_pair", ROOT / "tray/prepare-local-macos.py")
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)
sign = prepare.signer
MAGIC = b"\xcf\xfa\xed\xfe"
AUTHORITY = "Developer ID Application: Example (ABCDE12345)"
IDENTITY = ("A" * 40, AUTHORITY)


def raw_app(root, name="source"):
    app = root / name / "hgs-tray.app"
    code = app / "Contents/MacOS/hgs-tray"
    code.parent.mkdir(parents=True)
    code.write_bytes(MAGIC + b"synthetic app")
    code.chmod(0o755)
    (app / "Contents/Info.plist").write_bytes(plistlib.dumps({
        "CFBundleIdentifier": sign.LOCAL_BUNDLE_ID, "CFBundleExecutable": "hgs-tray",
        "CFBundlePackageType": "APPL", "CFBundleVersion": "1"}))
    return app


def metadata(path):
    service = path.name == "hgs"
    return {"Authority": AUTHORITY, "TeamIdentifier": "ABCDE12345",
            "Identifier": "hgs" if service else sign.BUNDLE_ID,
            "CDHash": "1" * 40 if service else "2" * 40}


@unittest.skipIf(os.name == "nt", "local Mac preparation requires Unix filesystem semantics")
class PairedControls(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zerus-pair-controls-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.app = raw_app(self.root)
        self.hgs = self.root / "build-hgs"
        self.hgs.write_bytes(MAGIC + b"synthetic CLI")
        self.hgs.chmod(0o755)
        self.output = self.root / "hgs-tray.app"
        self.installed = self.root / "previous.app"
        self.calls = []
        stack = contextlib.ExitStack()
        self.addCleanup(stack.close)
        self.enter = stack.enter_context
        self.enter(contextlib.redirect_stdout(io.StringIO()))
        self.run = self.enter(mock.patch.object(sign, "run", side_effect=self.command))
        self.enter(mock.patch.object(sign, "installed_identity", return_value=None))
        self.enter(mock.patch.object(sign, "identities", return_value=[IDENTITY]))
        self.details = self.enter(mock.patch.object(sign, "details", side_effect=metadata))
        self.enter(mock.patch.object(sign, "requirement", return_value='identifier "synthetic"'))

    def command(self, *args):
        # Models only the mutating sign operation, never native verification.
        self.assertEqual(args[0], "codesign")
        self.calls.append(args)
        if "--sign" in args and Path(args[-1]).name == "hgs":
            with Path(args[-1]).open("ab") as stream:
                stream.write(b" modeled certificate signature")
        return subprocess.CompletedProcess(args, 0, "", "")

    def prepare(self):
        return prepare.prepare(self.app, self.hgs, self.output, self.installed)

    def test_copy_sign_order_and_sealed_hash_record_preserve_both_inputs(self):
        original_app = (self.app / "Contents/Info.plist").read_bytes()
        original_cli = self.hgs.read_bytes()
        record = self.prepare()
        self.assertEqual((self.app / "Contents/Info.plist").read_bytes(), original_app)
        self.assertEqual(self.hgs.read_bytes(), original_cli)
        self.assertFalse((self.app / "Contents/MacOS/hgs").exists())
        self.assertEqual(record["build_sha256"], hashlib.sha256(original_cli).hexdigest())
        self.assertEqual(record["signed_sha256"], sign.sha256(self.output / "Contents/MacOS/hgs"))
        self.assertNotEqual(record["build_sha256"], record["signed_sha256"])
        self.assertEqual(json.loads((self.output / sign.PAIR_RECORD).read_text()), record)
        signing = [c for c in self.calls if "--sign" in c]
        self.assertEqual([Path(c[-1]) for c in signing], [self.output / "Contents/MacOS/hgs", self.output])
        self.assertTrue(all("--deep" not in c for c in signing))
        self.assertIn("--identifier", signing[0])
        self.assertEqual(self.calls[-1][1:-1], ("--verify", "--deep", "--strict", "--all-architectures"))
        self.assertFalse(self.installed.exists())
        self.assertEqual((self.output.stat().st_mode & 0o777), 0o700)

    def test_paired_mode_is_explicit_and_legacy_mode_rejects_the_cli(self):
        (self.app / "Contents/MacOS/hgs").write_bytes(MAGIC)
        with self.assertRaisesRegex(ValueError, "nested code"):
            sign.sign(self.app, self.installed)
        self.run.assert_not_called()

    def test_missing_text_link_and_hardlinked_paired_code_refuse_before_tools(self):
        cli = self.app / "Contents/MacOS/hgs"
        for case in ("missing", "text", "link", "hardlink"):
            with self.subTest(case=case):
                cli.unlink(missing_ok=True)
                if case == "text":
                    cli.write_bytes(b"not Mach-O"); cli.chmod(0o755)
                elif case == "link":
                    cli.symlink_to(self.hgs)
                elif case == "hardlink":
                    os.link(self.hgs, cli)
                with self.assertRaises(ValueError):
                    sign.sign(self.app, self.installed, paired=True)
                self.run.assert_not_called()

    def test_third_code_link_or_special_file_refuse_before_tools(self):
        extra = self.app / "Contents/unexpected"
        cases = ["code", "link"] + (["fifo"] if hasattr(os, "mkfifo") else [])
        for case in cases:
            with self.subTest(case=case):
                if case == "code": extra.write_bytes(MAGIC)
                elif case == "link": extra.symlink_to(self.hgs)
                else: os.mkfifo(extra)
                with self.assertRaises(ValueError): self.prepare()
                self.assertFalse(self.output.exists())
                self.run.assert_not_called()
                extra.unlink()

    def test_existing_and_overlapping_outputs_are_never_removed(self):
        self.output.mkdir(); marker = self.output / "keep"; marker.write_text("existing")
        with self.assertRaisesRegex(ValueError, "already exists"): self.prepare()
        self.assertEqual(marker.read_text(), "existing")
        with self.assertRaises(ValueError):
            prepare.prepare(self.app, self.hgs, self.app, self.installed)
        self.assertTrue((self.app / "Contents/Info.plist").is_file())
        self.run.assert_not_called()

    def test_nonprivate_prefix_refuses_without_creating_output(self):
        prefix = self.root / "public"; prefix.mkdir(mode=0o755)
        with self.assertRaisesRegex(ValueError, "private"):
            prepare.prepare(self.app, self.hgs, prefix / "hgs-tray.app", self.installed)
        self.assertEqual(list(prefix.iterdir()), [])
        self.run.assert_not_called()

    def test_signing_failure_cleans_only_its_copy_and_preserves_previous_app(self):
        self.installed.mkdir(); marker = self.installed / "keep"; marker.write_text("old")
        original_cli = self.hgs.read_bytes()
        for target in ("hgs", "hgs-tray.app"):
            def fail(*args):
                if "--sign" in args and Path(args[-1]).name == target:
                    raise subprocess.CalledProcessError(1, args, stderr="synthetic locked key")
                return self.command(*args)
            with self.subTest(target=target), mock.patch.object(sign, "run", side_effect=fail):
                with self.assertRaisesRegex(ValueError, "private key"): self.prepare()
                self.assertFalse(self.output.exists())
                self.assertEqual(marker.read_text(), "old")
                self.assertEqual(self.hgs.read_bytes(), original_cli)

    def test_empty_wrong_team_adhoc_identifier_and_same_image_fail_closed(self):
        for change in ({"TeamIdentifier": ""}, {"TeamIdentifier": "not set"},
                       {"TeamIdentifier": "FGHIJ67890"}, {"Signature": "adhoc"},
                       {"Authority": "unsupported"}, {"Identifier": "other"}, {"CDHash": "2" * 40}):
            def changed(path):
                result = metadata(path)
                if path.name == "hgs": result.update(change)
                return result
            with self.subTest(change=change), mock.patch.object(sign, "details", side_effect=changed):
                with self.assertRaises(ValueError): self.prepare()
                self.assertFalse(self.output.exists())

    def test_apple_anchor_rejection_is_not_replaced_by_matching_labels(self):
        def refuse(*args):
            if "-R" in args: raise subprocess.CalledProcessError(1, args, stderr="synthetic untrusted anchor")
            return self.command(*args)
        with mock.patch.object(sign, "run", side_effect=refuse):
            with self.assertRaises(subprocess.CalledProcessError): self.prepare()
        self.assertFalse(self.output.exists())

    def test_cli_mutation_during_app_signing_is_not_published(self):
        def tamper(*args):
            if "--sign" in args and Path(args[-1]) == self.output:
                (self.output / "Contents/MacOS/hgs").write_bytes(b"changed after leaf signing")
            return self.command(*args)
        with mock.patch.object(sign, "run", side_effect=tamper):
            with self.assertRaisesRegex(ValueError, "changed during preparation"): self.prepare()
        self.assertFalse(self.output.exists())

    def test_previous_paired_cli_requires_its_own_mutual_continuity(self):
        previous = raw_app(self.root, "previous")
        leaf = previous / "Contents/MacOS/hgs"; leaf.write_bytes(MAGIC)
        self.installed = previous
        with mock.patch.object(sign, "installed_identity", return_value=metadata(previous)), \
                mock.patch.object(sign, "verify_continuity") as continuity:
            self.prepare()
            self.assertEqual(continuity.call_args_list, [mock.call(self.output, previous),
                             mock.call(self.output / "Contents/MacOS/hgs", leaf)])

    def test_either_continuity_failure_preserves_previous_and_cleans_output(self):
        previous = raw_app(self.root, "previous")
        leaf = previous / "Contents/MacOS/hgs"; leaf.write_bytes(MAGIC)
        self.installed = previous
        original = leaf.read_bytes()
        for service_failure in (False, True):
            def continuity(candidate, old):
                if (candidate.name == "hgs") == service_failure:
                    raise ValueError("synthetic incompatible requirement")
            with self.subTest(service=service_failure), \
                    mock.patch.object(sign, "installed_identity", return_value=metadata(previous)), \
                    mock.patch.object(sign, "verify_continuity", side_effect=continuity):
                with self.assertRaisesRegex(ValueError, "incompatible"): self.prepare()
                self.assertFalse(self.output.exists())
                self.assertEqual(leaf.read_bytes(), original)

    def test_hash_pinned_requirement_is_not_stable_pair_identity(self):
        with mock.patch.object(sign, "requirement", return_value='cdhash H"synthetic"'):
            with self.assertRaisesRegex(ValueError, "stable Apple"): self.prepare()
        self.assertFalse(self.output.exists())

    def test_replaced_failed_output_is_preserved_for_manual_review(self):
        retained = self.root / "owned-before-replacement"
        def replaced(*args, **kwargs):
            self.output.rename(retained)
            self.output.mkdir(); (self.output / "foreign").write_text("retain")
            raise ValueError("synthetic replaced output")
        with mock.patch.object(sign, "sign", side_effect=replaced):
            with self.assertRaisesRegex(ValueError, "replaced output"): self.prepare()
        self.assertEqual((self.output / "foreign").read_text(), "retain")
        self.assertTrue(retained.is_dir())


@unittest.skipUnless(sys.platform == "darwin" and os.environ.get("ZERUS_TEST_LOCAL_MACOS_SIGNING") == "1",
                     "opt-in native pair signing requires the maintained existing-key flow and owner window")
class NativePairSigning(unittest.TestCase):
    def test_real_pair_rebuild_continuity_and_tamper_refusal(self):
        with tempfile.TemporaryDirectory(prefix="zerus-native-pair-") as temporary:
            root = Path(temporary).resolve()
            requested = os.environ.get("HGS_MACOS_SIGN_IDENTITY", "")
            previous = root / "absent.app"
            records = []
            for generation in (1, 2):
                app = raw_app(root, f"raw{generation}")
                cli = root / f"cli{generation}"
                source = root / "probe.c"
                for target, value in ((app / "Contents/MacOS/hgs-tray", generation), (cli, generation + 10)):
                    source.write_text(f"int main(void) {{ return {value}; }}\n")
                    subprocess.run(["xcrun", "clang", str(source), "-o", str(target)],
                                   check=True, capture_output=True, timeout=60)
                prefix = root / f"prepared{generation}"; prefix.mkdir(mode=0o700)
                output = prefix / "hgs-tray.app"
                records.append(prepare.prepare(app, cli, output, previous, requested))
                sign.run("codesign", "--verify", "--deep", "--strict", output)
                previous = output
            self.assertNotEqual(records[0]["signed_sha256"], records[1]["signed_sha256"])
            service = previous / "Contents/MacOS/hgs"
            contents = bytearray(service.read_bytes()); contents[0] ^= 1; service.write_bytes(contents)
            with self.assertRaises(subprocess.CalledProcessError):
                sign.run("codesign", "--verify", "--deep", "--strict", previous)


if __name__ == "__main__":
    unittest.main()
