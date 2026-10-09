"""Connector retention preserves pending uploads and mutation identities."""
import json
from pathlib import Path
import sqlite3
import tempfile
import time
import unittest
from unittest.mock import AsyncMock, patch
import uuid

from zerus_mobile.connector import Connector, ConnectorError, Journal


class JournalRetentionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.directory = Path(self.temp.name) / "journal"
        self.journal = Journal(self.directory, "example-binding")

    def tearDown(self):
        self.journal.close()
        self.temp.cleanup()

    def record(self, operation="send", delivered=True):
        request_id = str(uuid.uuid4())
        self.assertTrue(self.journal.claim(request_id, operation, "a" * 64))
        response = {"state": "completed", "result": {"text": "private native content"}, "error": None}
        self.journal.finish(request_id, response)
        if delivered:
            self.journal.delivered(request_id)
        with self.journal.db:
            self.journal.db.execute("UPDATE requests SET created_at=1,updated_at=1,delivered_at=1 WHERE request_id=?", (request_id,))
        return request_id, response

    def test_delivered_inspect_expires_and_is_safe_to_read_again(self):
        request_id, _ = self.record("inspect")
        self.journal.prune()
        self.assertIsNone(self.journal.db.execute("SELECT 1 FROM requests WHERE request_id=?", (request_id,)).fetchone())
        self.assertTrue(self.journal.claim(request_id, "inspect", "a" * 64))

    def test_mutation_content_scrubs_without_losing_durable_identity(self):
        request_id, _ = self.record()
        self.assertIn(b"private native content", (self.directory / "journal.sqlite3").read_bytes())
        self.journal.prune()
        self.assertNotIn(b"private native content", (self.directory / "journal.sqlite3").read_bytes())
        self.assertFalse(self.journal.claim(request_id, "send", "a" * 64))
        response = self.journal.replay(request_id)
        self.assertEqual(response["state"], "completed")
        self.assertEqual(response["result"], {"status": "history_expired"})
        self.assertIn("will not execute again", response["error"])
        self.assertNotIn("private native content", json.dumps(response))
        self.assertEqual(self.journal.pending(), [(request_id, response)])
        with self.assertRaises(ConnectorError):
            self.journal.claim(request_id, "send", "b" * 64)
        self.journal.close()
        self.journal = Journal(self.directory, "example-binding")
        self.assertFalse(self.journal.claim(request_id, "send", "a" * 64))
        self.assertEqual(self.journal.replay(request_id), response)

    def test_pending_results_never_expire(self):
        expected = []
        for operation in ("inspect", "send"):
            expected.append(self.record(operation, delivered=False))
        self.journal.prune()
        self.assertCountEqual(self.journal.pending(), expected)
        self.journal.close()
        self.journal = Journal(self.directory, "example-binding")
        self.assertCountEqual(self.journal.pending(), expected)

    def test_legacy_migration_is_conservative_and_preserves_outbox(self):
        self.journal.close()
        old = sqlite3.connect(self.directory / "journal.sqlite3")
        old.execute("DROP TABLE requests")
        old.execute("CREATE TABLE requests(request_id TEXT PRIMARY KEY,operation TEXT NOT NULL,state TEXT NOT NULL,response TEXT,delivered INTEGER NOT NULL DEFAULT 0)")
        read_id, write_id = str(uuid.uuid4()), str(uuid.uuid4())
        response = {"state": "completed", "result": {"text": "legacy private content"}, "error": None}
        old.execute("INSERT INTO requests VALUES(?,?,?, ?,1)", (read_id, "inspect", "completed", json.dumps(response)))
        old.execute("INSERT INTO requests VALUES(?,?,?,NULL,0)", (write_id, "send", "running"))
        old.commit()
        old.close()
        before = time.time()
        self.journal = Journal(self.directory, "example-binding")
        row = self.journal.db.execute("SELECT created_at,delivered_at FROM requests WHERE request_id=?", (read_id,)).fetchone()
        self.assertGreaterEqual(row[0], before)
        self.assertGreaterEqual(row[1], before)
        self.journal.prune()
        self.assertFalse(self.journal.claim(read_id, "inspect"))  # Old direct API remains safe.
        with self.assertRaises(ConnectorError):
            self.journal.claim(write_id, "send", "a" * 64)  # Unknown old hash cannot imply a match.
        pending = self.journal.pending()
        self.assertEqual(pending[0][0], write_id)
        self.assertEqual(pending[0][1]["state"], "uncertain")

    def test_retained_read_limit_does_not_consume_mutation_capacity(self):
        now = time.time()
        with self.journal.db:
            self.journal.db.executemany("INSERT INTO requests(request_id,operation,state,created_at,updated_at) VALUES(?,'inspect','running',?,?)", ((str(uuid.uuid4()), now, now) for _ in range(5000)))
        with self.assertRaises(ConnectorError):
            self.journal.claim(str(uuid.uuid4()), "inspect", "a" * 64)
        self.assertTrue(self.journal.claim(str(uuid.uuid4()), "send", "a" * 64))


class ConnectorRetentionTests(unittest.IsolatedAsyncioTestCase):
    async def test_changed_payload_never_executes_and_expired_result_is_explicit(self):
        with tempfile.TemporaryDirectory() as root:
            connector = Connector({"server_url": "https://relay.example.invalid", "node_token": "example-token", "state_dir": root})
            connector.sessions = {"codex/example"}
            request_id = str(uuid.uuid4())
            request = {"request_id": request_id, "operation": "send", "session": "codex/example", "payload": {"request_id": request_id, "expected_run_id": "example-run", "expected_conversation_id": "example-conversation", "text": "original"}}
            try:
                with patch.object(connector, "native", AsyncMock(return_value={"status": "sent"})) as native:
                    await connector.execute(request)
                    connector.journal.delivered(request_id)
                    with connector.journal.db:
                        connector.journal.db.execute("UPDATE requests SET delivered_at=1 WHERE request_id=?", (request_id,))
                    result = await connector.execute(request)
                    self.assertEqual(result["result"]["status"], "history_expired")
                    self.assertEqual(native.await_count, 1)
                    changed = {**request, "payload": {**request["payload"], "text": "changed"}}
                    with self.assertRaises(ConnectorError):
                        await connector.execute(changed)
                    self.assertEqual(native.await_count, 1)
            finally:
                connector.journal.close()


if __name__ == "__main__":
    unittest.main()
