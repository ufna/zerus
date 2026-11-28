"""Exercise the desktop request and native Terminate receipt with private stores."""
import copy
import hashlib
import json
import os
import shlex
import sqlite3
import subprocess
import sys
import time
import unittest
import uuid

from test_lifecycle_receipt_status import ARCHIVE, HGS, MACHINE, REQUEST
import test_lifecycle_receipt_status as lifecycle


def encode(value):
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False)


def digest(text):
    return hashlib.sha256(text.encode()).hexdigest()


class TerminationOutcome(unittest.TestCase):
    def setUp(self):
        self.receipts = lifecycle.LifecycleReceiptStatus()
        self.addCleanup(self.receipts.doCleanups)
        # macOS CI's native TMPDIR can exceed the UNIX socket address limit.
        short_tmp = os.environ.get("ZERUS_TEST_SHORT_TMPDIR", "/tmp") if sys.platform == "darwin" else None
        self.receipts.setUp(temp_dir=short_tmp)
        self.receipts.payload["action"] = "terminate"
        self.receipts.catalog()
        self.receipts.write()
        self.state = self.receipts.state
        self.db = self.state / "desktop/state.sqlite3"
        if os.name != "nt":
            self.configure_peer()
        self.start()

    def configure_peer(self):
        # All peer traffic uses the actual CLI route/marker script. The fake
        # SSH process can execute only in our private HOME and uses no network.
        self.home = self.receipts.root / "home"
        ssh = self.home / "ssh"
        ssh.write_text(
            "#!/usr/bin/env python3\n"
            "import os,sys,pathlib,time\n"
            "home=pathlib.Path(os.environ['HOME'])\n"
            "with (home/'ssh.calls').open('a') as log: log.write('\\n'.join(sys.argv[1:])+'\\n')\n"
            "if (home/'slow').exists(): time.sleep(10)\n"
            "os.execv('/bin/sh',['sh','-c',sys.argv[-1]])\n"
        )
        ssh.chmod(0o700)
        self.receipts.env["HGS_SSH"] = str(ssh)
        peer = self.home / ".local/bin/hgs"
        peer.parent.mkdir(parents=True, mode=0o700)
        peer.write_text(
            "#!/bin/sh\n"
            "if [ \"$1\" = --help ]; then cat \"$HOME/help\"; exit; fi\n"
            "if [ \"$1\" != --expect-machine ]; then exit 99; fi\n"
            "if [ \"$3\" = --client ]; then verb=$5; else verb=$3; fi\n"
            "if [ \"$verb\" != session-action-status ]; then exit 99; fi\n"
            "cat > \"$HOME/forwarded\"\n"
            f"exec {shlex.quote(str(HGS))} \"$@\" < \"$HOME/forwarded\"\n"
        )
        peer.chmod(0o700)
        self.help = self.home / "help"
        self.help.write_text("guarded-create-v1\nlifecycle-receipt-status-v1\n")

    def start(self):
        self.server = subprocess.Popen(
            [str(HGS), "desktop", "serve"], env=self.receipts.env,
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
            text=True,
        )
        self.addCleanup(self.stop, self.server)
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline and self.server.poll() is None:
            if self.db.exists():
                result = self.command("status")
                if result.returncode == 0 and json.loads(result.stdout)["running"]:
                    return
            time.sleep(0.05)
        self.fail("private desktop service did not start")

    def stop(self, process):
        if process.poll() is None:
            process.terminate()
        try:
            _, error = process.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            _, error = process.communicate()
            self.fail(f"private desktop service did not drain: {error}")

    def command(self, verb, data=None):
        return subprocess.run(
            [str(HGS), "desktop", verb, "--json"], input=encode(data), text=True,
            env=self.receipts.env, capture_output=True, timeout=10,
        )

    def request(self, payload):
        result = self.command("request", payload)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def retain(self, *, computer="", machine=MACHINE):
        # Seed only the immutable row that the existing native permit writes.
        # This fixture has no GUI permit, session process or dispatch target.
        with sqlite3.connect(self.db, timeout=5) as db:
            configuration, epoch = db.execute(
                "SELECT configuration,epoch FROM service WHERE id=1"
            ).fetchone()
            capture = {"target": {"computer": computer, "name": lifecycle.NAME,
                                  "run_id": "run-1", "conversation_id": "conversation-1"},
                       "machine": machine, "root": str(self.state / "unused-root"),
                       "common_dir": str(self.state / "unused-common"), "owner_id": "fixture"}
            original = " " + encode(self.receipts.payload) + "\n"
            confirmation = str(uuid.uuid4())
            choice = "remove_after_terminate"
            fingerprint = digest(encode([configuration, confirmation, choice, REQUEST, capture, original]))
            intent = {"scope": {"configuration": configuration, "service_epoch": epoch},
                      "confirmation_id": confirmation, "choice": choice,
                      "request_id": REQUEST, "capture": capture, "original": original}
            data = encode(intent)
            db.execute("INSERT INTO desktop_termination_outcomes VALUES(?,?,?,?,?,?,1)",
                       (REQUEST, confirmation, configuration, fingerprint, digest(data), data))
        status = self.request({"op": "termination_outcome_status", "configuration": configuration,
                               "request_id": str(uuid.uuid4()), "original_request_id": REQUEST})
        self.assertTrue(status["found"], status)
        self.assertEqual(status["status"]["fingerprint"], fingerprint)
        self.assertEqual(status["status"]["request_sha256"], digest(original))
        self.original_bytes = original.encode()
        return {"op": "termination_outcome_observe", "request_id": str(uuid.uuid4()),
                "configuration": configuration, "service_epoch": epoch, "original": status["status"]}

    def test_observe_exact_original(self):
        request = self.retain()
        private = self.receipts.write()
        original = private.read_bytes()
        answer = self.request(request)
        self.assertTrue(answer["accepted"], answer)
        self.assertEqual(answer["status"], "completed", answer)
        self.assertEqual(answer["archive_id"], ARCHIVE, answer)
        self.assertEqual(answer["request_id"], request["request_id"])
        self.assertEqual(answer["original_request_id"], REQUEST)
        self.assertFalse(answer["cleanup_authorized"])
        self.assertEqual(private.read_bytes(), original)
        self.assertEqual(set(private.parent.iterdir()), {private})
        self.assertFalse((self.state / "unused-root").exists())
        self.assertFalse((self.state / "unused-common").exists())

    def test_unavailable_claimed_failed_and_corrupt(self):
        request = self.retain()
        private = self.receipts.write()
        private.unlink()
        self.assertEqual(self.request(request)["status"], "unavailable")
        self.assertFalse(private.exists())
        for stored, expected in (("claimed", "uncertain"), ("failed", "failed")):
            self.receipts.write(stored)
            answer = self.request(request)
            self.assertEqual(answer["status"], expected, answer)
            self.assertNotIn("archive_id", answer)
        private.write_text("{}")
        answer = self.request(request)
        self.assertFalse(answer["accepted"], answer)
        self.assertNotIn("archive_id", answer)

    def test_wrong_tuple_never_returns_archive(self):
        request = self.retain()
        for field in ("original_service_epoch", "confirmation_id", "original_request_id",
                      "fingerprint", "capture_sha256", "request_sha256"):
            with self.subTest(field=field):
                changed = copy.deepcopy(request)
                changed["original"][field] = ("0" * 64 if field.endswith("sha256") or field == "fingerprint"
                                              else str(uuid.uuid4()))
                answer = self.request(changed)
                self.assertFalse(answer["accepted"], answer)
                self.assertNotIn("archive_id", answer)

    def test_original_epoch_survives_service_restart(self):
        request = self.retain()
        self.stop(self.server)
        self.start()
        old = self.request(request)
        self.assertFalse(old["accepted"], old)
        with sqlite3.connect(self.db) as db:
            request["service_epoch"] = db.execute("SELECT epoch FROM service WHERE id=1").fetchone()[0]
        self.assertNotEqual(request["service_epoch"], request["original"]["original_service_epoch"])
        answer = self.request(request)
        self.assertEqual(answer["status"], "completed", answer)
        self.assertEqual(answer["archive_id"], ARCHIVE)


    @unittest.skipIf(os.name == "nt", "POSIX private SSH transport")
    def test_peer_passes_exact_bytes_only_to_guarded_status_verb(self):
        request = self.retain(computer="synthetic-peer")
        self.help.write_text("guarded-create-v1\n")
        answer = self.request(request)
        self.assertFalse(answer["accepted"], answer)
        self.assertFalse((self.home / "forwarded").exists())
        self.help.write_text("guarded-create-v1\nlifecycle-receipt-status-v1\n")
        answer = self.request(request)
        self.assertEqual(answer["archive_id"], ARCHIVE, answer)
        self.assertEqual((self.home / "forwarded").read_bytes(), self.original_bytes)
        calls = (self.home / "ssh.calls").read_text().splitlines()
        self.assertIn("synthetic-peer", calls)
        self.assertIn("BatchMode=yes", calls)
        self.assertNotIn("-t", calls)
        catalog = self.receipts.root / "config/swarm/catalog.json"
        config = json.loads(catalog.read_text())
        catalog.write_text(encode({**config, "node_id": ARCHIVE}))
        answer = self.request(request)
        self.assertFalse(answer["accepted"], answer)
        self.assertNotIn("archive_id", answer)

    @unittest.skipIf(os.name == "nt", "POSIX private SSH transport")
    def test_held_peer_worker_does_not_block_actor_or_admit_another_read(self):
        request = self.retain(computer="synthetic-peer")
        slow = self.home / "slow"
        slow.touch()
        with subprocess.Popen(
            [str(HGS), "desktop", "request", "--json"], env=self.receipts.env,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        ) as pending:
            pending.stdin.write(encode(request))
            pending.stdin.close()
            pending.stdin = None
            deadline = time.monotonic() + 1
            while not (self.home / "ssh.calls").exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertTrue((self.home / "ssh.calls").exists())
            health = self.command("status")
            self.assertEqual(health.returncode, 0, health.stderr)
            self.assertTrue(json.loads(health.stdout)["running"])
            self.assertIsNone(pending.poll(), "status was blocked by the receipt worker")
            answer = self.request({**request, "request_id": str(uuid.uuid4())})
            self.assertFalse(answer["accepted"], answer)
            self.assertIsNone(pending.poll(), "a second read waited for the first")
            stdout, stderr = pending.communicate(timeout=5)
            self.assertEqual(pending.returncode, 0, stderr)
            self.assertFalse(json.loads(stdout)["accepted"])
        slow.unlink()
        answer = self.request(request)
        self.assertEqual(answer["archive_id"], ARCHIVE, answer)

    @unittest.skipUnless(sys.platform == "linux", "private escaped pipe holder and flock")
    def test_shutdown_keeps_serving_owner_until_original_pipe_closes(self):
        import fcntl

        # The command-scoped process only holds an inherited pipe. It exits on
        # its own and neither reads original stdin nor contacts a real peer.
        ssh = self.home / "ssh"
        ssh.write_text(
            "#!/usr/bin/env python3\n"
            "import os,sys,pathlib,time\n"
            "home=pathlib.Path(os.environ['HOME'])\n"
            "if 'session-action-status' in sys.argv[-1]:\n"
            "    if os.fork() == 0:\n"
            "        os.setsid()\n"
            "        (home/'holder').touch()\n"
            "        time.sleep(5)\n"
            "        (home/'holder-ended').touch()\n"
            "        os._exit(0)\n"
            "    os._exit(0)\n"
            "os.execv('/bin/sh',['sh','-c',sys.argv[-1]])\n"
        )
        request = self.retain(computer="synthetic-peer")
        result = self.command("request", request)
        if result.returncode == 0:
            self.assertFalse(json.loads(result.stdout)["accepted"], result.stdout)
        else:
            self.assertTrue(result.stderr.strip())
        self.assertTrue((self.home / "holder").exists())
        self.assertFalse((self.home / "holder-ended").exists())

        self.server.terminate()
        time.sleep(0.2)
        self.assertIsNone(self.server.poll())
        with (self.db.parent / "owner.lock").open("rb") as owner:
            with self.assertRaises(BlockingIOError):
                fcntl.flock(owner, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.server.wait(timeout=8)
            self.assertTrue((self.home / "holder-ended").exists())
            fcntl.flock(owner, fcntl.LOCK_EX | fcntl.LOCK_NB)


if __name__ == "__main__":
    unittest.main()
