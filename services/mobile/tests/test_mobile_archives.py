"""Archived inspections bind immutable UUIDs, never reusable session names."""
import copy
import json
import os
from pathlib import Path
import tempfile
import time
import unittest
from unittest.mock import AsyncMock, patch
import uuid

from aiohttp.test_utils import TestClient, TestServer

from zerus_mobile.connector import Connector
from zerus_mobile.projects import apply_memberships
from zerus_mobile.server import Config, create_app
from zerus_mobile.store import Store

ARCHIVE = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
OTHER_ARCHIVE = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
NAME = "codex/project/nested/tag"


def command(operation="inspect", archive=ARCHIVE):
    request_id = str(uuid.uuid4())
    payload = {} if archive is None else {"archive_id": archive}
    if operation != "inspect":
        payload.update(request_id=request_id, expected_run_id="saved-run", expected_conversation_id="saved-conversation", text="Hello")
    return {"request_id": request_id, "operation": operation, "session": NAME, "payload": payload}


class ArchiveConnectorTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.connector = Connector({"server_url": "https://relay.example.invalid", "node_token": "example-token", "state_dir": self.temp.name})
        self.rows = [{"name": NAME, "run_id": "live-run", "state": "running"},
                     {"name": NAME, "run_id": "saved-run", "state": "archived", "archive_id": ARCHIVE}]
        with patch.object(self.connector, "native", AsyncMock(return_value={"host": "Example", "sessions": self.rows})):
            await self.connector.snapshot()

    async def asyncTearDown(self):
        self.connector.journal.close()
        self.temp.cleanup()

    async def test_live_and_archive_same_name_use_distinct_native_arguments(self):
        async def inspect(argv, payload=None):
            self.assertIsNone(payload)
            return {"name": NAME, "archive_id": ARCHIVE, "run_id": "saved-run", "state": "archived"} if "--archive" in argv else {"name": NAME, "run_id": "live-run"}
        with patch.object(self.connector, "native", AsyncMock(side_effect=inspect)) as native:
            archived = await self.connector.execute(command())
            live = await self.connector.execute(command(archive=None))
            self.assertEqual(archived["state"], "completed")
            self.assertEqual(archived["result"]["archive_id"], ARCHIVE)
            self.assertEqual(archived["result"]["run_id"], "saved-run")
            self.assertEqual(live["result"]["run_id"], "live-run")
            self.assertEqual(native.call_args_list[0].args[0], ["inspect", NAME, "--archive", ARCHIVE])
            self.assertEqual(native.call_args_list[1].args[0], ["inspect", NAME])

    async def test_invalid_unknown_and_wrong_name_archives_never_spawn(self):
        with patch.object(self.connector, "native", AsyncMock()) as native:
            for archive in ("", "../../outside", ARCHIVE.upper(), OTHER_ARCHIVE, 7, None):
                request = command(archive=archive)
                if archive is None:
                    request["payload"] = {"archive_id": None}
                self.assertEqual((await self.connector.execute(request))["state"], "failed")
            wrong = command()
            wrong["session"] = "codex/other/name"
            self.assertEqual((await self.connector.execute(wrong))["state"], "failed")
            native.assert_not_awaited()

    async def test_archived_only_name_has_no_legacy_or_mutation_fallback(self):
        with patch.object(self.connector, "native", AsyncMock(return_value={"host": "Example", "sessions": [self.rows[1]]})):
            await self.connector.snapshot()
        with patch.object(self.connector, "native", AsyncMock()) as native:
            self.assertEqual((await self.connector.execute(command(archive=None)))["state"], "failed")
            for operation in ("send", "answer", "interrupt"):
                self.assertEqual((await self.connector.execute(command(operation)))["state"], "failed")
                self.assertEqual((await self.connector.execute(command(operation, archive=None)))["state"], "failed")
            native.assert_not_awaited()

    async def test_native_missing_or_wrong_archive_identity_fails_without_fallback(self):
        for result in ({"name": NAME, "run_id": "live-run"}, {"name": NAME, "archive_id": OTHER_ARCHIVE}, {"name": "codex/other/name", "archive_id": ARCHIVE}):
            with patch.object(self.connector, "native", AsyncMock(return_value=result)) as native:
                response = await self.connector.execute(command())
                self.assertEqual(response["state"], "failed")
                self.assertEqual(native.await_count, 1)
                self.assertEqual(native.call_args.args[0], ["inspect", NAME, "--archive", ARCHIVE])

    async def test_expired_archive_snapshot_is_not_used_to_spawn_native_inspection(self):
        self.connector.snapshot_seen_at = time.monotonic() - 46
        with patch.object(self.connector, "native", AsyncMock()) as native:
            response = await self.connector.execute(command())
            self.assertEqual(response["state"], "failed")
            self.assertIn("stale", response["error"])
            native.assert_not_awaited()

    async def test_archive_and_live_same_name_have_distinct_project_assignments(self):
        catalog = {"available": True, "default_project": "general", "projects": [
            {"id": "live-project", "sessions": [NAME], "archives": []},
            {"id": "saved-project", "sessions": [], "archives": [ARCHIVE]}]}
        snapshot = {"sessions": copy.deepcopy(self.rows)}
        apply_memberships(snapshot, catalog)
        self.assertEqual([s["mobile_project_id"] for s in snapshot["sessions"]], ["live-project", "saved-project"])

    async def test_archive_id_without_state_still_requires_exact_archive_selection(self):
        row = {"name": NAME, "archive_id": ARCHIVE}
        with patch.object(self.connector, "native", AsyncMock(return_value={"host": "Example", "sessions": [row]})):
            await self.connector.snapshot()
        self.assertNotIn(NAME, self.connector.sessions)
        self.assertIn((NAME, ARCHIVE), self.connector.archives)
        catalog = {"available": True, "default_project": "general", "projects": [
            {"id": "live-project", "sessions": [NAME], "archives": []},
            {"id": "saved-project", "sessions": [], "archives": [ARCHIVE]}]}
        snapshot = {"sessions": [row]}
        apply_memberships(snapshot, catalog)
        self.assertEqual(row["mobile_project_id"], "saved-project")
        with patch.object(self.connector, "native", AsyncMock()) as native:
            self.assertEqual((await self.connector.execute(command(archive=None)))["state"], "failed")
            native.assert_not_awaited()

    async def test_ordinary_inspect_rejects_raced_archive_or_different_name_response(self):
        for response in ({"name": NAME, "archive_id": ARCHIVE}, {"name": NAME, "state": "archived"}, {"name": "codex/other/name"}):
            with patch.object(self.connector, "native", AsyncMock(return_value=response)) as native:
                result = await self.connector.execute(command(archive=None))
                self.assertEqual(result["state"], "failed")
                self.assertEqual(native.await_count, 1)
                self.assertEqual(native.call_args.args[0], ["inspect", NAME])


class ArchiveRelayTests(unittest.IsolatedAsyncioTestCase):
    async def test_relay_preserves_archive_id_and_rejects_malformed_or_mutation_payloads(self):
        with tempfile.TemporaryDirectory() as root:
            store = Store(Path(root) / "relay" / "state.sqlite3")
            workspace = store.workspace("Example")
            node = store.node(workspace, "Example computer")
            phone = store.pair(store.invite(workspace)["pair_code"], "Example phone")
            client = TestClient(TestServer(create_app(store, Config(background=False))))
            await client.start_server()
            headers = {"Authorization": "Bearer " + phone["device_token"]}
            try:
                request = {**command(), "computer_id": node["node_id"]}
                async with client.post("/v1/requests", headers=headers, json=request) as response:
                    self.assertEqual(response.status, 202)
                async with client.get("/v1/node/requests", headers={"Authorization": "Bearer " + node["node_token"]}) as response:
                    self.assertEqual((await response.json())["requests"][0]["payload"], {"archive_id": ARCHIVE})
                for archive in ("", "../../outside", ARCHIVE.upper(), 7, None):
                    invalid = {**command(), "computer_id": node["node_id"]}
                    invalid["payload"] = {"archive_id": archive}
                    async with client.post("/v1/requests", headers=headers, json=invalid) as response:
                        self.assertEqual(response.status, 400)
                for operation in ("send", "answer", "interrupt"):
                    invalid = {**command(operation), "computer_id": node["node_id"]}
                    async with client.post("/v1/requests", headers=headers, json=invalid) as response:
                        self.assertEqual(response.status, 400)
                async with client.post("/v1/requests", headers=headers, json={**command(archive=None), "computer_id": node["node_id"]}) as response:
                    self.assertEqual(response.status, 202)
            finally:
                await client.close()
                store.close()

    async def test_archived_name_collision_does_not_change_live_notification_baseline(self):
        with tempfile.TemporaryDirectory() as root:
            store = Store(Path(root) / "relay" / "state.sqlite3")
            workspace = store.workspace("Example")
            node_token = store.node(workspace, "Example computer")["node_token"]
            node = store.authenticate(node_token, "nodes")
            live = {"name": NAME, "run_id": "live-run", "conversation_id": "live-conversation", "phase": "working", "activity": "busy"}
            archive = {"name": NAME, "archive_id": ARCHIVE, "state": "archived", "run_id": "saved-run", "conversation_id": "saved-conversation", "phase": "idle", "activity": "idle"}
            try:
                store.heartbeat(node, {"sessions": [live, archive]})
                question = {**live, "phase": "input", "attention_id": "question"}
                store.heartbeat(node, {"sessions": [question, archive]})
                store.heartbeat(node, {"sessions": [question, archive]})
                store.heartbeat(node, {"sessions": [{**live, "phase": "idle", "activity": "idle"}, archive]})
                self.assertEqual([row[0] for row in store.db.execute("SELECT kind FROM events ORDER BY id")], ["attention", "completed"])
            finally:
                store.close()


class ArchiveFixtureTests(unittest.IsolatedAsyncioTestCase):
    async def test_optional_synthetic_states_and_archive_pass_through(self):
        with tempfile.TemporaryDirectory() as root:
            fixture = Path(__file__).with_name("fixture_hgs.py")
            fixture.chmod(0o755)
            Path(root, "fixture-config.json").write_text(json.dumps({"all_states": True}))
            connector = Connector({"server_url": "https://relay.example.invalid", "node_token": "example-token", "hgs_path": str(fixture), "state_dir": str(Path(root) / "journal")})
            try:
                with patch.dict(os.environ, {"ZERUS_MOBILE_FIXTURE_DIR": root}):
                    snapshot = await connector.snapshot()
                    self.assertEqual(len(snapshot["sessions"]), 7)
                    archived = next(row for row in snapshot["sessions"] if row.get("state") == "archived")
                    live = next(row for row in snapshot["sessions"] if row["name"] == archived["name"] and row.get("state") != "archived")
                    self.assertNotEqual(archived["mobile_project_id"], live["mobile_project_id"])
                    request = command(archive=archived["archive_id"])
                    request["session"] = archived["name"]
                    inspected = await connector.execute(request)
                    self.assertEqual(inspected["state"], "completed")
                    self.assertEqual(inspected["result"]["archive_id"], archived["archive_id"])
                    self.assertEqual(inspected["result"]["conversation_id"], "fixture-archive-conversation")
                    second = next(row for row in snapshot["sessions"] if row.get("state") == "archived" and row["archive_id"] != archived["archive_id"])
                    request = command(archive=second["archive_id"])
                    request["session"] = second["name"]
                    inspected = await connector.execute(request)
                    self.assertEqual(inspected["result"]["conversation_id"], "fixture-second-archive-conversation")
                    self.assertIn("Second synthetic", inspected["result"]["provider_messages"][0]["detail"])
                    request = command(archive=None)
                    request["session"] = live["name"]
                    inspected = await connector.execute(request)
                    self.assertEqual(inspected["result"]["conversation_id"], "fixture-conversation")
                    saved = next(row for row in snapshot["sessions"] if row.get("state") == "paused")
                    request = command(archive=None)
                    request["session"] = saved["name"]
                    inspected = await connector.execute(request)
                    self.assertEqual(inspected["result"]["state"], "paused")
                    self.assertFalse(Path(root, "mutations.jsonl").exists())
            finally:
                connector.journal.close()


if __name__ == "__main__":
    unittest.main()
