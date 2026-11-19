"""Black-box heartbeat privacy: what of a native listing reaches the relay.

The native listing producers' top-level keys are classified here after review.
A heartbeat carries only the forwarded ones plus the connector's projections.
"""
import os
import re
import unittest
import uuid

from connector_harness import BlackBoxCase, ROOT, default_rules, listing, session_row

LISTING_KEYS = {
    "host": "forwarded", "ok": "forwarded", "hgs_version": "forwarded", "projects": "forwarded",
    "archive_revision": "forwarded", "archive_paging": "forwarded", "sessions": "forwarded", "metrics": "forwarded", "preferences": "forwarded",
    "local_sessions": "forwarded", "local_host": "forwarded", "archive_page": "forwarded",
    "peers": "withheld",            # SSH aliases and the peer-of-peer inventory stay local.
    "ended_launches": "withheld",   # The last screen lines of failed starts.
    "desktop_presence": "beside",   # The heartbeat's `presence`, outside the snapshot.
    "listing_page": "explicit-page",  # Only `ls --paged`, which the connector never requests.
}
CONNECTOR_KEYS = {"mobile_archive", "mobile_accounts", "mobile_projects", "mobile_capabilities"}
FORWARDED = {key for key, use in LISTING_KEYS.items() if use == "forwarded"} | CONNECTOR_KEYS
PEER_ALIAS = "bb-private-peer-alias"
SCREEN_TEXT = "BB-SCREEN-SENTINEL last terminal line"
PRESENCE = {"idle_seconds": 4.0, "locked": False, "locked_seconds": None}


def producer_keys(source: str, function: str, *outputs: str) -> set[str]:
    """Read the listing object literal and writes to the named output objects."""
    start = source.index(f"fn {function}(")
    body = re.split(r"\n(?:pub(?:\([^\n]*\))? )?fn ", source[start:], maxsplit=1)[0]
    key = r'"((?:[^"\\]|\\.)+)"'
    keys = set(re.findall(rf'\b(?:{"|".join(outputs)})\[{key}\]\s*=(?!=)', body))
    for literal in re.findall(r"let mut raw = json!\(\{(.*?)\}\);", body, re.DOTALL):
        keys.update(re.findall(key + r"\s*:", literal))
    return keys


class HeartbeatPrivacyTests(BlackBoxCase):
    def full_listing(self) -> dict:
        busy = session_row(process_work={"count": 1, "started_at": 100.0, "command": "build --wait", "sampled_at": 200.0})
        return listing(archive_revision="0"*64,archive_paging=True,archive_page={"version": 1, "total": 0},
                       sessions=[busy], peers=[PEER_ALIAS + ".example.invalid"], local_sessions=True,
                       local_host={"state": "running"}, desktop_presence=PRESENCE,
                       ended_launches=[{"launch_id": str(uuid.uuid4()), "name": "claude/example/failed",
                                        "code": 1, "output": SCREEN_TEXT}])

    async def test_heartbeat_withholds_private_keys_and_carries_only_reviewed_ones(self):
        full = self.full_listing()
        self.assertEqual(set(full), {key for key, use in LISTING_KEYS.items() if use != "explicit-page"})
        self.hgs.script(default_rules(full))
        process = await self.launch()
        await self.until(lambda: len(self.relay.heartbeats()) >= 2, what="two heartbeats", process=process)
        for exchange in self.relay.heartbeats():
            body = exchange.body
            self.assertEqual(set(body), {"snapshot", "presence"})
            self.assertEqual(body["presence"], PRESENCE)
            snapshot = body["snapshot"]
            unreviewed = set(snapshot) - FORWARDED
            self.assertFalse(unreviewed, f"unreviewed heartbeat snapshot keys: {sorted(unreviewed)}")
            self.assertEqual(set(snapshot), FORWARDED, "a reviewed listing key went missing")
            for key in ("host", "hgs_version", "projects", "metrics", "preferences", "local_sessions", "local_host", "archive_page"):
                self.assertEqual(snapshot[key], full[key], key)
            for text in (PEER_ALIAS.encode(), SCREEN_TEXT.encode(), b"ended_launches", b"desktop_presence"):
                self.assertNotIn(text, exchange.raw)
            # Observation clocks would defeat the relay's unchanged-snapshot shortcut.
            self.assertEqual(snapshot["sessions"][0]["process_work"],
                             {"count": 1, "started_at": 100.0, "command": "build --wait"})

    def test_reviewed_listing_keys_match_the_hgs_source(self):
        producers = [
            ("src/cli.rs", "local_json_archives_attempt", ("raw", "merged")),
            ("src/state/mod.rs", "merge_listing_archives", ("box_value",)),
            ("src/state/listing_metadata/listing.rs", "merge", ("value",)),
        ]
        if not all((ROOT / filename).is_file() for filename, _, _ in producers):
            if os.environ.get("CI") == "true":
                self.fail("the native listing sources are required in CI")
            self.skipTest("needs the hgs source tree")
        emitted = set()
        for filename, function, outputs in producers:
            keys = producer_keys((ROOT / filename).read_text(), function, *outputs)
            self.assertTrue(keys, f"the listing scan found no keys in {filename}:{function}")
            emitted.update(keys)
        self.assertEqual(emitted - set(LISTING_KEYS), set(),
                         "review these new `hgs ls --json` keys for heartbeats and classify them here")
        self.assertEqual(set(LISTING_KEYS) - emitted, set(), "remove listing keys hgs no longer emits")

    async def test_presence_falls_back_for_a_relay_without_the_field(self):
        self.hgs.script(default_rules(listing(desktop_presence=PRESENCE)))
        self.relay.rule("POST", "/v1/node/heartbeat", 400, times=1, when=lambda exchange: "presence" in exchange.body)
        process = await self.launch()
        await self.until(lambda: len(self.relay.heartbeats()) >= 3, what="three heartbeats", process=process)
        attempts = [exchange for exchange in self.relay.exchanges if exchange.path == "/v1/node/heartbeat"]
        self.assertEqual([("presence" in exchange.body, exchange.status) for exchange in attempts[:4]],
                         [(True, 400), (False, 200), (False, 200), (False, 200)])


if __name__ == "__main__":
    unittest.main()
