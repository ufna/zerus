"""Distributable frontend notices come from the exact installed lock entries."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
spec = importlib.util.spec_from_file_location("frontend_licenses", ROOT / "scripts/collect-frontend-licenses.py")
collector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(collector)


class FrontendLicenses(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="zerus-frontend-notices-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.client = self.root / "client"
        self.client.mkdir()
        self.output = self.root / "notices"
        self.package = self.client / "node_modules/@example/runtime"
        self.package.mkdir(parents=True)
        (self.package / "package.json").write_text(json.dumps({"name": "@example/runtime", "version": "1.2.3", "license": "MIT"}))
        (self.package / "LICENSE").write_bytes(b"exact license text\n")
        (self.client / "package-lock.json").write_text(json.dumps({"packages": {
            "": {"version": "1.0.0"}, "node_modules/@example/runtime": {"version": "1.2.3"},
            "node_modules/dev-tool": {"version": "4.0.0", "dev": True}}}))

    def test_exact_production_notices_are_copied_and_dev_tools_are_excluded(self):
        result = collector.collect(self.client, self.output)
        self.assertEqual([row["name"] for row in result], ["@example/runtime"])
        self.assertEqual((self.output / "node_modules/@example/runtime/LICENSE").read_bytes(), b"exact license text\n")
        self.assertEqual(json.loads((self.output / "inventory.json").read_text()), result)
        self.assertFalse((self.output / "node_modules/dev-tool").exists())

    def test_stale_installed_package_and_missing_notices_refuse_before_output(self):
        metadata = self.package / "package.json"
        original = metadata.read_bytes()
        metadata.write_text(json.dumps({"name": "@example/runtime", "version": "1.2.2"}))
        with self.assertRaisesRegex(ValueError, "differs from its lockfile"):
            collector.collect(self.client, self.output)
        self.assertFalse(self.output.exists())
        metadata.write_bytes(original)
        (self.package / "LICENSE").unlink()
        with self.assertRaisesRegex(ValueError, "No license text"):
            collector.collect(self.client, self.output)
        self.assertFalse(self.output.exists())

    def test_existing_destination_and_oversized_notice_are_preserved(self):
        self.output.mkdir()
        kept = self.output / "kept.txt"
        kept.write_text("keep")
        with self.assertRaisesRegex(ValueError, "must be new"):
            collector.collect(self.client, self.output)
        self.assertEqual(kept.read_text(), "keep")
        (self.package / "LICENSE").write_bytes(b"x" * (collector.LIMIT + 1))
        with self.assertRaisesRegex(ValueError, "oversized"):
            collector.collect(self.client, self.root / "other")
        self.assertFalse((self.root / "other").exists())

    @unittest.skipIf(sys.platform == "win32", "symlink creation needs an explicitly privileged Windows fixture")
    def test_linked_notice_cannot_copy_an_unrelated_file(self):
        outside = self.root / "outside"
        outside.write_text("unrelated")
        (self.package / "LICENSE").unlink()
        (self.package / "LICENSE").symlink_to(outside)
        with self.assertRaises(ValueError):
            collector.collect(self.client, self.output)
        self.assertFalse(self.output.exists())
        self.assertEqual(outside.read_text(), "unrelated")


if __name__ == "__main__":
    unittest.main()
