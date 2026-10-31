"""Native mechanics (modelled identity boundary); never Apple qualification."""
import contextlib
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
QUALIFICATION = "mechanics (modelled identity boundary)"


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@unittest.skipUnless(sys.platform == "darwin" and os.environ.get("ZERUS_TEST_PRIVATE_MACOS_SIGNING") == "1",
                     "mechanics (modelled identity boundary): reviewed private-keychain opt-in only")
class PrivateSigningMechanics(unittest.TestCase):
    def test_private_pair_mechanics_and_unchanged_owner_references(self):
        helper = load("private_identity", ROOT / "tests/support/private_macos_signing.py")
        prepared = load("private_preparation", ROOT / "tray/prepare-local-macos.py")
        sign = prepared.signer
        original = {key: getattr(sign, key) for key in ("run", "identities", "genuine_signature", "details")}
        source_hashes = {
            "tray/sign-local-macos.py": "bda2d17c6f827947a7e215a9603847a300b3e4fb2d523d87412dce7154db98c5",
            "tray/prepare-local-macos.py": "8d00182b2a2e22c0873ab29840c77ed80cdfcd96a1fc76aa6566f8cffd1258af",
        }
        def source_unchanged():
            for name, expected in source_hashes.items():
                self.assertEqual(hashlib.sha256((ROOT / name).read_bytes()).hexdigest(), expected)
        source_unchanged()
        home = Path.home().resolve()
        helper.require_private_home(home)
        root = Path(tempfile.mkdtemp(prefix="zerus-mechanics-modelled-boundary-", dir=home)).resolve()
        identity_root = root / "identity"; identity_root.mkdir(mode=0o700)
        fixture = helper.PrivateIdentity(identity_root, home, ROOT / "tests/support/private_macos_keychain.c")
        helper.event("scratch", root=str(root))
        native_calls = []
        counters = {"discovery_refusals": 0, "apple_anchor_refusals": 0, "generations": 0, "tamper_refusals": 0}
        def files(app):
            return {str(p.relative_to(app)): sign.sha256(p) for p in sorted(app.rglob("*")) if p.is_file()}
        try:
            sources = []
            for generation in (1, 2):
                raw = root / f"raw{generation}/hgs-tray.app"
                (raw / "Contents/MacOS").mkdir(parents=True)
                import plistlib
                (raw / "Contents/Info.plist").write_bytes(plistlib.dumps({
                    "CFBundleIdentifier": sign.LOCAL_BUNDLE_ID, "CFBundleExecutable": "hgs-tray",
                    "CFBundlePackageType": "APPL", "CFBundleVersion": "1"}))
                cli = root / f"cli{generation}"
                for output, number in ((raw / "Contents/MacOS/hgs-tray", generation), (cli, generation + 10)):
                    code = root / "probe.c"; code.write_text(f"int main(void) {{ return {number}; }}\n")
                    fixture.public_command(["/usr/bin/xcrun", "clang", "-g0", str(code), "-o", str(output)])
                sources.append((raw, cli, files(raw), sign.sha256(cli)))
            fixture.create()
            def wire(*args):
                before = tuple(args)
                if before[0] == "codesign":
                    self.assertTrue("--sign" in before or "--verify" in before or "-d" in before)
                    after = (*before[:-1], "--keychain", str(fixture.keychain), before[-1])
                    self.assertEqual(after[:-3] + after[-1:], before)
                else:
                    self.assertEqual(before, ("security", "find-identity", "-v", "-p", "codesigning"))
                    after = (*before, str(fixture.keychain))
                    self.assertEqual(after[:-1], before)
                native_calls.append([str(x) for x in after])
                try:
                    result = original["run"](*after)
                except subprocess.CalledProcessError as error:
                    helper.event("codesign_or_scoped_discovery", argv=[str(x) for x in after],
                                 exit=error.returncode, stdout=error.stdout, stderr=error.stderr)
                    raise
                helper.event("codesign_or_scoped_discovery", argv=[str(x) for x in after],
                             exit=result.returncode, stdout=result.stdout, stderr=result.stderr)
                return result
            def expected_identifier(path):
                if path.name == "hgs": return "hgs"
                if path.name == "hgs-tray.app": return sign.BUNDLE_ID
                raise AssertionError("identity view outside fixed fixture code")
            def own_requirement(path, identifier):
                self.assertEqual(identifier, expected_identifier(path))
                self.assertIn(root, path.parents)
                requirement = f'identifier "{identifier}" and certificate leaf = H"{fixture.sha1}"'
                sign.run("codesign", "--verify", "--strict", "--all-architectures", "-R", "=" + requirement, path)
            def team_view(path):
                # Never supply a marker for any foreign certificate or ID.
                own_requirement(path, expected_identifier(path))
                actual = original["details"](path)
                if actual.get("TeamIdentifier", "") in ("", "not set"):
                    actual["TeamIdentifier"] = "MECHANICS-NO-TEAM"
                return actual
            def modelled_identity(path, identifier):
                own_requirement(path, identifier)
                actual = team_view(path)
                self.assertNotEqual(actual.get("Signature"), "adhoc")
                self.assertEqual(actual.get("Identifier"), identifier)
                self.assertTrue(actual.get("CDHash"))
                self.assertNotIn("cdhash", sign.requirement(path).lower())
                return actual
            with contextlib.ExitStack() as stack:
                stack.enter_context(mock.patch.object(sign, "run", side_effect=wire))
                # D1: unchanged production discovery, scoped only by the wire.
                discovered = original["identities"]()
                expected_identity = (fixture.sha1, helper.CN)
                self.assertTrue(not discovered or discovered == [expected_identity])
                if not discovered:
                    counters["discovery_refusals"] += 1
                    helper.event("N0_production_discovery_refuses_selfsigned", identities=0)
                    # D2: actual unfiltered private matching identities, not a
                    # fabricated CLI result or an owner-keychain fallback.
                    result = fixture.public_command(["/usr/bin/security", "find-identity", "-p", "codesigning", fixture.keychain])
                    matching = result.decode().split("Matching identities", 1)[1].split("Valid identities only", 1)[0]
                    rows = re.findall(r'^\s*\d+\)\s+([0-9A-Fa-f]{40})\s+"([^"\n]+)"(?:\s+\([^\n]*\))?\s*$', matching, re.M)
                    self.assertEqual([(digest.upper(), name) for digest, name in rows], [expected_identity])
                    stack.enter_context(mock.patch.object(sign, "identities", return_value=[expected_identity]))
                stack.enter_context(mock.patch.dict(os.environ, HGS_MACOS_SIGN_IDENTITY=fixture.sha1))
                # D3/M1: no Apple/team model yet. The real verifier must reject
                # the actual self-signed code at its Apple-anchor command.
                raw, cli, before_app, before_cli = sources[0]
                negative = root / "negative"; negative.mkdir(mode=0o700)
                with self.assertRaises(subprocess.CalledProcessError) as refused:
                    prepared.prepare(raw, cli, negative / "hgs-tray.app", root / "absent.app",
                                     os.environ["HGS_MACOS_SIGN_IDENTITY"])
                self.assertIn("=anchor apple generic", refused.exception.cmd)
                self.assertIn("--verify", refused.exception.cmd)
                self.assertIs(sign.genuine_signature, original["genuine_signature"])
                self.assertIs(sign.details, original["details"])
                self.assertFalse((negative / "hgs-tray.app").exists())
                counters["apple_anchor_refusals"] += 1
                helper.event("N1_unchanged_Apple_verifier_refuses_selfsigned", exit=refused.exception.returncode)
                fixture.unchanged_references("after_negative_codesign")
                # D4: only this identity boundary is modelled; every remaining
                # production step, check, data read and cleanup stays unchanged.
                stack.enter_context(mock.patch.object(sign, "genuine_signature", side_effect=modelled_identity))
                stack.enter_context(mock.patch.object(sign, "details", side_effect=team_view))
                previous = root / "absent.app"
                records = []
                for generation, (raw, cli, before_app, before_cli) in enumerate(sources, 1):
                    prefix = root / f"pair{generation}"; prefix.mkdir(mode=0o700)
                    output = prefix / "hgs-tray.app"
                    record = prepared.prepare(raw, cli, output, previous, os.environ["HGS_MACOS_SIGN_IDENTITY"])
                    self.assertEqual(files(raw), before_app)
                    self.assertEqual(sign.sha256(cli), before_cli)
                    self.assertEqual(json.loads((output / sign.PAIR_RECORD).read_text()), record)
                    records.append(record)
                    counters["generations"] += 1
                    helper.event("mechanics_pair", generation=generation, output=str(output),
                                 input_app=str(raw), input_files=before_app, input_cli=str(cli),
                                 input_cli_sha256=before_cli, output_files=files(output), pair=record)
                    previous = output
                self.assertNotEqual(records[0]["signed_sha256"], records[1]["signed_sha256"])
                service = previous / "Contents/MacOS/hgs"
                data = bytearray(service.read_bytes()); data[0] ^= 1; service.write_bytes(data)
                with self.assertRaises(subprocess.CalledProcessError):
                    sign.run("codesign", "--verify", "--deep", "--strict", "--all-architectures", previous)
                counters["tamper_refusals"] += 1
                helper.event("tamper_refused", service=str(service), sha256=sign.sha256(service))
            fixture.after_signing()
            helper.event("mechanics_cases", **counters, tool_calls=len(native_calls))
        finally:
            immutable = False
            try:
                for key, value in original.items(): self.assertIs(getattr(sign, key), value)
                source_unchanged()
                immutable = True
            finally:
                fixture.cleanup()
                shutil.rmtree(root)
                helper.event("all_owned_artifacts_removed", exists=root.exists(), production_source_unchanged=immutable,
                             module_attributes_restored=immutable)
            source_unchanged()


if __name__ == "__main__":
    unittest.main()
