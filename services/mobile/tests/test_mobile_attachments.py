"""Inline files never name a computer path and are bounded independently."""
import asyncio
import base64
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import AsyncMock, patch
import uuid

from aiohttp import ClientSession
from aiohttp.test_utils import TestClient, TestServer

from zerus_mobile.attachments import MAX_FILE_BYTES, MAX_TOTAL_BYTES, validate_send
from zerus_mobile.connector import Connector
from zerus_mobile.server import Config, create_app
from zerus_mobile.store import Store


def file(raw=b"example bytes", **change):
    return {"name": "example.txt", "mime": "text/plain", "data_base64": base64.b64encode(raw).decode(), **change}


def payload(files):
    return {"request_id": str(uuid.uuid4()), "text": "Read these files", "expected_run_id": "example-run", "expected_conversation_id": "example-conversation", "attachments": files}


class AttachmentValidationTests(unittest.TestCase):
    def test_paths_urls_controls_and_unknown_fields_are_rejected(self):
        for invalid in (file(name="../../outside"), file(name="C:\\outside"), file(name="."), file(name="line\nname"), file(path="/etc/passwd"), file(url="https://example.com/file"), file(data_uri="file:///etc/passwd"), file(data_base64="not base64"), file(data_base64=""), file(data_base64="eA====")):
            with self.subTest(invalid=list(invalid)):
                with self.assertRaises(ValueError):
                    validate_send(payload([invalid]))
        with self.assertRaises(ValueError):
            validate_send({**payload([file()]), "path": "/etc/passwd"})
        with self.assertRaises(ValueError):
            validate_send(payload([file(reference="[File #1]"), file(reference="[File #1]")]))
        with self.assertRaises(ValueError):
            validate_send(payload([file()] * 9))

    def test_native_file_and_total_limits_and_utf8_text(self):
        maximum = file(b"a" * MAX_FILE_BYTES)
        validate_send(payload([maximum, maximum]))
        with self.assertRaises(ValueError):
            validate_send(payload([maximum, maximum, file(b"x")]))
        with self.assertRaises(ValueError):
            validate_send(payload([file(b"a" * (MAX_FILE_BYTES + 1))]))
        with self.assertRaises(ValueError):
            validate_send({**payload([]), "text": "😀" * (65536 // 4 + 1)})
        validate_send({**payload([file(reference="[Image #1]")]), "text": ""})


class AttachmentRelayTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.store = Store(Path(self.temp.name) / "relay" / "state.sqlite3")
        self.workspace = self.store.workspace("Example")
        self.node = self.store.node(self.workspace, "Example computer")
        self.phone = self.store.pair(self.store.invite(self.workspace)["pair_code"], "Example phone")
        self.config = Config(background=False)
        self.client = TestClient(TestServer(create_app(self.store, self.config)))
        await self.client.start_server()
        self.headers = {"Authorization": "Bearer " + self.phone["device_token"]}
        self.node_headers = {"Authorization": "Bearer " + self.node["node_token"]}

    async def asyncTearDown(self):
        await self.client.close()
        self.store.close()
        self.temp.cleanup()

    def request(self, files):
        value = payload(files)
        return {"request_id": value["request_id"], "computer_id": self.node["node_id"], "operation": "send", "session": "codex/example/nested/tag", "payload": value}

    async def test_larger_than_one_mebibyte_upload_and_node_delivery(self):
        request = self.request([file(b"x" * (2 * 1024 * 1024))])
        async with self.client.post("/v1/requests", headers=self.headers, json=request) as response:
            self.assertEqual(response.status, 202, await response.text())
        async with self.client.get("/v1/node/requests", headers=self.node_headers) as response:
            batch = await response.json()
        self.assertEqual(len(batch["requests"]), 1)
        self.assertEqual(batch["requests"][0]["payload"], request["payload"])
        async with self.client.post("/v1/requests", headers=self.headers, json=request) as response:
            self.assertEqual(response.status, 202)
            self.assertEqual((await response.json())["state"], "claimed")
        async with self.client.get("/v1/node/requests", headers=self.node_headers) as response:
            self.assertEqual((await response.json())["requests"], [])

    async def test_ordinary_endpoint_cap_invalid_metadata_and_capabilities(self):
        async with self.client.post("/v1/node/heartbeat", headers=self.node_headers, json={"snapshot": {"sessions": [], "padding": "x" * (1024 * 1024)}}) as response:
            self.assertEqual(response.status, 413)
        async with self.client.post("/v1/requests", headers=self.headers, json=self.request([file(path="/etc/passwd")])) as response:
            self.assertEqual(response.status, 400)
        async with self.client.get("/v1/capabilities", headers=self.headers) as response:
            limits = (await response.json())["attachment_limits"]
        self.assertEqual(limits["max_total_bytes"], MAX_TOTAL_BYTES)
        self.assertEqual(limits["max_file_bytes"], MAX_FILE_BYTES)

    async def test_retained_completed_bodies_budget_and_reserved_control_allowance(self):
        request = self.request([file(b"x" * 3000)])
        self.config.max_queue_bytes = len(json.dumps(request).encode()) + 100
        async with self.client.post("/v1/requests", headers=self.headers, json=request) as response:
            self.assertEqual(response.status, 202)
        with self.store.db:
            self.store.db.execute("UPDATE requests SET state='completed' WHERE id=?", (request["request_id"],))
        async with self.client.post("/v1/requests", headers=self.headers, json=self.request([file(b"x" * 3000)])) as response:
            self.assertEqual(response.status, 429)
            self.assertIn("retained payload", (await response.json())["error"])
        async with self.client.post("/v1/requests", headers=self.headers, json=request) as response:
            self.assertEqual(response.status, 202)  # Exact retry works when full.
        async with self.client.post("/v1/requests", headers=self.headers, json=self.request([])) as response:
            self.assertEqual(response.status, 202)  # Reserved small/control space.

    async def test_connector_accepts_large_request_response_independently(self):
        request = self.request([file(b"x" * (2 * 1024 * 1024))])
        async with self.client.post("/v1/requests", headers=self.headers, json=request) as response:
            self.assertEqual(response.status, 202)
        with tempfile.TemporaryDirectory() as state:
            connector = Connector({"server_url": str(self.client.make_url("/")).rstrip("/"), "node_token": self.node["node_token"], "state_dir": state}, allow_insecure_localhost=True)
            connector.sessions = {request["session"]}
            try:
                async with ClientSession() as client:
                    batch = await connector.http(client, "GET", "/v1/node/requests", max_response_bytes=29 * 1024 * 1024 + 4096)
                with patch.object(connector, "native", AsyncMock(return_value={"status": "submitted"})) as native:
                    result = await connector.execute(batch["requests"][0])
                    self.assertEqual(result["state"], "completed")
                    self.assertEqual(native.call_args.args[1]["attachments"], request["payload"]["attachments"])
                    invalid = copy.deepcopy(batch["requests"][0])
                    invalid["request_id"] = str(uuid.uuid4())
                    invalid["payload"]["request_id"] = invalid["request_id"]
                    invalid["payload"]["attachments"][0]["path"] = "/etc/passwd"
                    self.assertEqual((await connector.execute(invalid))["state"], "failed")
                    self.assertEqual(native.await_count, 1)
            finally:
                connector.journal.close()


if __name__ == "__main__":
    unittest.main()
