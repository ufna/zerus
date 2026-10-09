"""Whole-row public-history limits preserve every control disclosure."""
import copy
import json
import tempfile
import unittest
from unittest.mock import AsyncMock, patch
import uuid

from zerus_mobile.connector import Connector
from zerus_mobile.history import bound_inspection, completed_size


class HistoryTests(unittest.TestCase):
    def test_large_unicode_history_keeps_full_question_identity_and_newest_user(self):
        metadata = {"name": "codex/example", "run_id": "exact-run", "conversation_id": "exact-conversation",
                    "pending_questions": [{"question_id": "exact-question", "question_hash": "exact-hash", "text": "🦀" * 20000}],
                    "input_queue": {"id": "exact-queue", "text": "Literal full queue"},
                    "session_usage": {"context": {"used": 2000, "limit": 100000}}, "compact_context_supported": True}
        own = {"seq": 1, "type": "UserPromptSubmit", "agent_id": "", "at": 1, "detail": "Newest own message"}
        raw = {**metadata, "events": [own], "message_events": [own],
               "provider_messages": [{"message_id": str(index), "at": index + 2, "detail": "🦀" * 32000} for index in range(8)]}
        original = copy.deepcopy(raw)
        result = bound_inspection(raw, 1024 * 1024 - 1024)
        self.assertLessEqual(completed_size(result), 1024 * 1024 - 1024)
        for key, value in metadata.items():
            self.assertEqual(result[key], value)
        self.assertEqual(raw, original)
        self.assertTrue(result["mobile_history_truncated"])
        self.assertEqual(result["message_events"], [own])
        for stream, count in result["mobile_history_omitted"].items():
            self.assertEqual(count, len(raw.get(stream, [])) - len(result.get(stream, [])))

    def test_mandatory_metadata_too_large_fails_without_cropping(self):
        raw = {"pending_questions": [{"text": "Full disclosure" * 1000}], "events": [{"at": 1, "detail": "Synthetic"}]}
        original = copy.deepcopy(raw)
        with self.assertRaisesRegex(ValueError, "required disclosures were not cropped"):
            bound_inspection(raw, 100)
        self.assertEqual(raw, original)

    def test_small_results_unchanged_and_no_event_is_partially_cropped(self):
        raw = {"events": [{"seq": 1, "at": 1, "detail": "Literal 🦀"}]}
        self.assertIs(bound_inspection(raw, 1000), raw)
        large = {"events": [{"seq": index, "at": index, "detail": "literal " * 100} for index in range(30)]}
        result = bound_inspection(large, 2000)
        self.assertLessEqual(completed_size(result), 2000)
        self.assertEqual(result["events"], large["events"][-len(result["events"]):])


class HistoryConnectorTests(unittest.IsolatedAsyncioTestCase):
    async def test_connector_read_returns_bounded_history_or_clear_metadata_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            connector = Connector({"server_url": "https://relay.example.invalid", "node_token": "fixture-token", "state_dir": directory})
            connector.sessions = {"codex/example"}
            try:
                def request():
                    return {"request_id": str(uuid.uuid4()), "operation": "inspect", "session": "codex/example", "payload": {}}
                raw = {"name": "codex/example", "provider_messages": [{"at": index, "detail": "🦀" * 32000} for index in range(8)],
                       "pending_questions": [{"text": "Uncropped disclosure"}]}
                with patch.object(connector, "native", AsyncMock(return_value=raw)):
                    response = await connector.execute(request())
                    self.assertEqual(response["state"], "completed")
                    self.assertTrue(response["result"]["mobile_history_truncated"])
                    self.assertLess(len(json.dumps({"request_id": str(uuid.uuid4()), **response}).encode()), 1024 * 1024)
                raw = {"name": "codex/example", "pending_questions": [{"text": "🦀" * 100000}]}
                with patch.object(connector, "native", AsyncMock(return_value=raw)):
                    response = await connector.execute(request())
                    self.assertEqual(response["state"], "failed")
                    self.assertIn("required disclosures", response["error"])
            finally:
                connector.journal.close()
