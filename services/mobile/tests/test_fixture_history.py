"""Synthetic presentation fixtures must not mutate any native session."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("fixture_history", Path(__file__).with_name("fixture_hgs.py"))
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)


class FixtureHistoryTests(unittest.TestCase):
    def test_default_history_is_unchanged(self):
        original = fixture.initial()
        inspected = fixture.inspection(original, {})
        self.assertEqual(original, {key: inspected[key] for key in original})
        self.assertEqual(original["events"], inspected["message_events"])

    def test_delayed_large_history_is_bounded_stable_and_does_not_change_record(self):
        original = fixture.initial()
        before = fixture.initial()
        with patch.object(fixture.time, "sleep") as sleep:
            first = fixture.inspection(original, {"inspect_delay": 8, "history_count": 120})
            sleep.assert_called_once_with(8)
        self.assertEqual(original, before)
        self.assertEqual(120, len(first["events"]) + len(first["provider_messages"]))
        grown = fixture.inspection(original, {"history_count": 150})
        self.assertTrue(all(row in grown["events"] for row in first["events"]))
        self.assertTrue(all(row in grown["provider_messages"] for row in first["provider_messages"]))
        capped = fixture.inspection(original, {"history_count": 10_000})
        self.assertEqual(500, len(capped["events"]) + len(capped["provider_messages"]))


if __name__ == "__main__":
    unittest.main()
