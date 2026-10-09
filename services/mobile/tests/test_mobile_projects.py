"""Logical groups use native IDs and exact assignments, including archives."""
import copy
from pathlib import Path
import tempfile
import unittest
from unittest.mock import AsyncMock, patch

from zerus_mobile.connector import Connector, ConnectorError
from zerus_mobile.projects import apply_memberships, normalize


def native_catalog():
    return {"schema": 1, "initialized": True, "node_id": "native-local", "swarm_id": "native-shared",
            "machines": [{"id": "native-local", "connection": "local-native", "local": True}],
            "peers": {"private-peer": {"private": "must not leave"}}, "conflicts": [{"private": "history"}],
            "organization": {"version": 2, "default_project": "ungrouped", "projects": [
                {"id": "logical", "name": "Logical project", "color": "#123456", "accessible": True,
                 "folders": [{"id": "f-local", "name": "Root", "path": "/example/root", "machine_id": "native-local", "machine_name": "Local"},
                             {"id": "f-remote", "name": "Remote", "path": "/private/remote", "machine_id": "native-remote", "machine_name": "Remote"}],
                 "sessions": ["local-native\ncodex/custom-worktree/nested/tag", "local-native\narchive\narchive-id", "remote-native\ncodex/remote/tag"]},
                {"id": "ungrouped", "name": "General", "color": "#8d9baa", "accessible": True, "folders": [], "sessions": []},
                {"id": "empty", "name": "Empty logical project", "color": "#112233", "accessible": False, "folders": [], "sessions": []}]}}


class NativeProjectsTests(unittest.TestCase):
    def test_all_groups_local_folders_and_exact_memberships_only(self):
        snapshot = {"host": "local-native", "projects": {}, "sessions": [
            {"name": "codex/custom-worktree/nested/tag", "project": "custom-worktree"},
            {"name": "codex/custom-worktree/nested/tag", "state": "archived", "archive_id": "archive-id"},
            {"name": "codex/unknown/tag", "project": "Logical project"}]}
        catalog = normalize(native_catalog(), snapshot, "fallback")
        apply_memberships(snapshot, catalog)
        self.assertEqual(len(catalog["projects"]), 3)
        self.assertEqual(catalog["projects"][0]["sessions"], ["codex/custom-worktree/nested/tag"])
        self.assertEqual(catalog["projects"][0]["archives"], ["archive-id"])
        self.assertEqual([f["id"] for f in catalog["projects"][0]["folders"]], ["f-local"])
        self.assertEqual([s["mobile_project_id"] for s in snapshot["sessions"]], ["logical", "logical", "ungrouped"])
        self.assertNotIn("peers", catalog)
        self.assertNotIn("conflicts", catalog)
        self.assertNotIn("/private/remote", str(catalog))

    def test_uninitialized_catalog_is_stable_unavailable_without_alias_invention(self):
        native = {**native_catalog(), "initialized": False}
        snapshot = {"host": "local-native", "projects": {"alias": "/example/alias"}, "sessions": [{"name": "codex/alias/tag"}]}
        first = normalize(native, snapshot, "stable-node-binding")
        second = normalize(native, snapshot, "stable-node-binding")
        self.assertEqual(first, second)
        self.assertFalse(first["available"])
        self.assertEqual(first["projects"], [])
        apply_memberships(snapshot, first)
        self.assertNotIn("mobile_project_id", snapshot["sessions"][0])


class ProjectCacheTests(unittest.IsolatedAsyncioTestCase):
    async def test_cache_reuses_fifteen_seconds_and_keeps_stale_memberships(self):
        with tempfile.TemporaryDirectory() as root:
            connector = Connector({"server_url": "https://relay.example.invalid", "node_token": "example-token", "state_dir": root})
            snapshot = {"host": "local-native", "sessions": [{"name": "codex/custom-worktree/nested/tag"}]}
            try:
                with patch.object(connector, "native", AsyncMock(return_value=native_catalog())) as native:
                    await connector.enrich_projects(snapshot)
                    await connector.enrich_projects(copy.deepcopy(snapshot))
                    self.assertEqual(native.await_count, 1)
                    self.assertEqual(native.call_args.args[0], ["swarm", "get"])
                    self.assertEqual(native.call_args.kwargs["timeout"], 3)
                    self.assertEqual(snapshot["sessions"][0]["mobile_project_id"], "logical")
                connector.project_next_poll = 0
                with patch.object(connector, "native", AsyncMock(side_effect=ConnectorError("unavailable"))):
                    stale = {"host": "local-native", "sessions": [{"name": "codex/custom-worktree/nested/tag"}]}
                    await connector.enrich_projects(stale)
                self.assertTrue(stale["mobile_projects"]["stale"])
                self.assertTrue(stale["mobile_projects"]["available"])
                self.assertEqual(stale["mobile_projects"]["swarm_id"], "native-shared")
                self.assertEqual(stale["sessions"][0]["mobile_project_id"], "logical")
            finally:
                connector.journal.close()


if __name__ == "__main__":
    unittest.main()
