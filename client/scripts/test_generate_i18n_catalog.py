import importlib.util
import json
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("generate_i18n_catalog", HERE / "generate-i18n-catalog.py")
generator = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(generator)


class GeneratedCatalog(unittest.TestCase):
    def test_checked_in_catalog_is_current(self):
        self.assertEqual(generator.DESTINATION.read_text(encoding="utf-8"), generator.generate())

    def test_disambiguated_sources_keep_their_own_translation(self):
        catalog = json.loads(generator.generate())
        self.assertEqual(catalog["Pause"], "Приостановить")
        self.assertEqual(catalog["media playback\u0004Pause"], "Пауза")
        self.assertEqual(catalog["media playback\u0004Play"], "Воспроизвести")
        self.assertFalse(any("\u0004" in key and not key.split("\u0004")[0] for key in catalog))


if __name__ == "__main__":
    unittest.main()
