"""Reviewed consumers and read-only inventory of the selected-record snapshot."""
import importlib.util
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("portable_source", ROOT / "scripts/ci/check-source.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class SelectedRecordSnapshotSource(unittest.TestCase):
    def test_snapshot_has_only_the_saved_fresh_consumer(self):
        references = {
            str(path.relative_to(ROOT))
            for path in (ROOT / "src").rglob("*.rs")
            if re.search(r"\bselected_source\b", CHECK.code_only(path.read_text()))
        }
        self.assertEqual(references, {
            "src/state/storage.rs",
            "src/state/storage/selected_source_tests.rs",
            "src/state/storage/selected_source_tests/cases.rs",
            "src/state/lifecycle.rs",
        })
        storage = CHECK.code_only((ROOT / "src/state/storage.rs").read_text())
        self.assertEqual(len(re.findall(r"\bselected_source\b", storage)), 1,
                         "no production re-export or additional wiring in the declaring module")
        self.assertIn("pub(super) mod selected_source;", storage)
        lifecycle = CHECK.code_only((ROOT / "src/state/lifecycle.rs").read_text())
        fresh = lifecycle.split("pub(super) fn fresh(", 1)[1].split("fn verify_requested(", 1)[0]
        self.assertEqual(len(re.findall(r"\bselected_source\b", lifecycle)), 1)
        self.assertIn("let record = source.record();", fresh)
        self.assertIn("let current = read(name).map_err(|_| changed())?;", fresh)
        self.assertEqual(fresh.count("record_path(name)"), 1, "resolve once before capture")
        self.assertLess(fresh.index("lock(None)?"), fresh.index("source.recheck()"))
        self.assertLess(fresh.index("source.recheck()"), fresh.index("backend::current().start"))

    def test_snapshot_uses_only_the_resolved_metadata_reader(self):
        code = CHECK.code_only((ROOT / "src/state/storage/selected_source.rs").read_text())
        self.assertNotRegex(code, r"\b(?:read|read_metadata|record_path|label_record_path|records|"
                                 r"write|atomic|private_dir|lock|reconcile|Command|backend)\s*(?:\(|::)")
        self.assertNotRegex(code, r"\b(?:Serialize|Deserialize|Drop)\b")
        self.assertIn("read_metadata_at(slot, name)", code)
        self.assertIn("read_metadata_at(&self.slot, &self.name)", code)


if __name__ == "__main__":
    unittest.main()
