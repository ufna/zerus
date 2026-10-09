"""Child input is deliberately restricted to the native DSH continuable ABI."""
import tempfile
import unittest
from unittest.mock import AsyncMock, patch
import uuid

from zerus_mobile.connector import Connector
from zerus_mobile.attachments import validate_send


class ChildSendTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.connector = Connector({"server_url": "https://relay.example.invalid", "node_token": "fixture", "state_dir": self.temp.name})
        self.connector.sessions = {"dsh/example/session", "codex/example/session"}
        self.parent = {"name": "dsh/example/session", "run_id": "run", "conversation_id": "conversation", "subagents": {"child": {}}}
        self.child = {"name": "dsh/example/session", "run_id": "run", "parent_conversation_id": "conversation", "conversation_id": "conversation/child", "agent_id": "child", "send_supported": True}

    async def asyncTearDown(self):
        self.connector.journal.close(); self.temp.cleanup()

    def request(self, session="dsh/example/session"):
        request = str(uuid.uuid4())
        return {"request_id": request, "operation": "send", "session": session,
                "payload": {"request_id": request, "expected_run_id": "run", "expected_conversation_id": "conversation", "agent_id": "child", "text": "Synthetic child message"}}

    def receipt(self, request):
        return {"request_id": request["request_id"], "name": request["session"], "run_id": "run", "conversation_id": "conversation", "agent_id": "child", "status": "submitted"}

    async def test_exact_parent_and_continuable_child_verified_before_one_native_send(self):
        request = self.request()
        with patch.object(self.connector, "native", AsyncMock(side_effect=[self.parent, self.child, self.receipt(request)])) as native:
            response = await self.connector.execute(request)
            self.assertEqual(response["state"], "completed")
            self.assertEqual(native.call_args.args, (["send", request["session"], "--json"], request["payload"]))
            self.assertEqual(await self.connector.execute(request), response)
            self.assertEqual(native.await_count, 3)

    async def test_other_provider_or_child_readonly_identity_never_mutates(self):
        with patch.object(self.connector, "native", AsyncMock()) as native:
            self.assertEqual((await self.connector.execute(self.request("codex/example/session")))["state"], "failed")
            native.assert_not_awaited()
        for fields in ({"send_supported": False}, {"agent_id": "other"}, {"run_id": "new"}, {"parent_conversation_id": "other"}):
            with patch.object(self.connector, "native", AsyncMock(side_effect=[self.parent, {**self.child, **fields}])) as native:
                self.assertEqual((await self.connector.execute(self.request()))["state"], "failed")
                self.assertEqual(native.await_count, 2)

    async def test_wrong_child_receipt_is_uncertain_and_no_replay(self):
        request = self.request()
        with patch.object(self.connector, "native", AsyncMock(side_effect=[self.parent, self.child, {**self.receipt(request), "agent_id": "other"}])) as native:
            response = await self.connector.execute(request)
            self.assertEqual(response["state"], "uncertain")
            self.assertEqual(await self.connector.execute(request), response)
            self.assertEqual(native.await_count, 3)

    def test_arbitrary_child_paths_and_context_continuations_rejected(self):
        payload = self.request()["payload"]
        for fields in ({"agent_id": "../child"}, {"agent_id": ""}, {"expected_conversation_id": ""}, {"expected_compaction_id": str(uuid.uuid4())}):
            with self.assertRaises(ValueError):
                validate_send({**payload, **fields})
