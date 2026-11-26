"""Run the real Antigravity CLI against a disposable, loopback model endpoint."""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
from queue import Queue
import shutil
import subprocess
import sys
from tempfile import TemporaryDirectory
from threading import Event, Thread
import time
import unittest
from urllib.parse import urlsplit
import uuid

AGY = os.environ.get("AGY_TEST_BIN")
HGS = os.environ.get("HGS_TEST_BIN")
TMUX = shutil.which("tmux")


class Model(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        size = int(self.headers.get("Content-Length", "0"))
        if size > 1 << 20:
            self.send_error(413)
            return
        payload = self.rfile.read(size)
        self.server.offers.append(payload)
        if b"HOLD-NATIVE-TEST" in payload:
            self.server.entered.put(True)
            self.server.release.wait(12)
        response = {"candidates": [{"content": {"role": "model", "parts": [{"text": "Disposable response"}]},
                                    "index": 0, "finishReason": "STOP"}],
                    "usageMetadata": {"promptTokenCount": 12, "candidatesTokenCount": 4, "totalTokenCount": 16}}
        method = urlsplit(self.path).path
        stream = method.endswith(":streamGenerateContent")
        if method.endswith(":countTokens"):
            response = {"totalTokens": 12}
        body = json.dumps(response).encode()
        if stream:
            body = b"data: " + body + b"\n\n"
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream" if stream else "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass


@unittest.skipUnless(AGY and HGS and TMUX, "set AGY_TEST_BIN and HGS_TEST_BIN on a native host with tmux")
class AntigravityNative(unittest.TestCase):
    def setUp(self):
        temporary = TemporaryDirectory(prefix="agy-t-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.home = self.root / "home"
        self.project = self.root / "project"
        self.tmux = self.root / "tmux"
        runtime = self.root / "runtime"
        self.bin = self.home / ".local/bin"
        self.bin.mkdir(parents=True)
        for folder in (self.project, self.tmux, runtime):
            folder.mkdir(mode=0o700)
        (self.bin / "agy").symlink_to(Path(AGY).resolve())
        wrapper = self.bin / "tmux"
        import shlex
        wrapper.write_text(f"#!/bin/sh\nexec {shlex.quote(TMUX)} -S {shlex.quote(str(self.tmux / 'socket'))} -f /dev/null \"$@\"\n")
        wrapper.chmod(0o700)
        self.env = {"HOME": str(self.home), "PATH": f"{self.bin}:/usr/bin:/bin:/usr/local/bin",
                    "HGS_CONFIG_DIR": str(self.root / "config"), "HGS_STATE_DIR": str(self.root / "state"),
                    "HGS_PEERS": "", "HGS_SELF": "agy-test", "HGS_TAB": "0", "TMUX_TMPDIR": str(self.tmux),
                    "TMPDIR": temporary.name, "LANG": "C.UTF-8", "BROWSER": "/usr/bin/false",
                    "XDG_RUNTIME_DIR": str(runtime), "XDG_CONFIG_HOME": str(self.home / "config"),
                    "XDG_CACHE_HOME": str(self.home / "cache"), "XDG_DATA_HOME": str(self.home / "data"),
                    "XDG_STATE_HOME": str(self.home / "state"),
                    "DBUS_SESSION_BUS_ADDRESS": f"unix:path={runtime / 'unavailable'}",
                    "HTTPS_PROXY": "http://127.0.0.1:9", "HTTP_PROXY": "http://127.0.0.1:9",
                    "NO_PROXY": "127.0.0.1"}
        self.addCleanup(self.close_bound_sessions)

    def cli(self, *argv, payload=None, check=True):
        cmd = subprocess.run([HGS, *argv], input=json.dumps(payload) if payload is not None else "",
                             text=True, capture_output=True, timeout=15, cwd=self.project, env=self.env)
        if check:
            self.assertEqual(cmd.returncode, 0, cmd.stderr + cmd.stdout)
        return cmd

    def launch(self, tag):
        launch_id = str(uuid.uuid4())
        result = self.cli("agy", str(self.project), "--new", "-n", tag, "--launch-id", launch_id,
                          "--account", "native-agy")
        name = f"agy/project/{tag}"
        row = self.inspect(name)
        self.assertEqual(row["name"], name, result.stdout)
        self.assertEqual(row["launch_id"], launch_id)
        return row

    def inspect(self, name):
        return json.loads(self.cli("inspect", name).stdout)

    def identity_request(self, row, **fields):
        return {"request_id": str(uuid.uuid4()), "expected_run_id": row["run_id"],
                "expected_conversation_id": row["conversation_id"], **fields}

    def action(self, row, action, check=True):
        return self.cli("session-action", row["name"], "--json",
                        payload=self.identity_request(row, action=action), check=check)

    def wait_for(self, predicate, seconds=8):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            observed = predicate()
            if observed:
                return observed
            time.sleep(0.05)
        self.fail("Timed out waiting for the disposable Antigravity session")

    @contextmanager
    def model(self):
        with ThreadingHTTPServer(("127.0.0.1", 0), Model) as server:
            server.entered = Queue()
            server.release = Event()
            server.offers = []
            worker = Thread(target=server.serve_forever, daemon=True)
            worker.start()
            settings = self.home / ".gemini/antigravity-cli/settings.json"
            settings.parent.mkdir(parents=True)
            settings.write_text('{"modelProvider":"gemini"}')
            self.env.update(GOOGLE_GEMINI_BASE_URL=f"http://127.0.0.1:{server.server_port}",
                            GEMINI_API_KEY="loopback-test-not-a-google-key")
            try:
                yield server
            finally:
                server.release.set()
                server.shutdown()
                worker.join(2)

    def close_bound_sessions(self):
        for path in (self.root / "state").glob("*.json"):
            record = json.loads(path.read_bytes())
            if record.get("agent") != "agy":
                continue
            row = self.inspect(record["name"])
            if row["phase"] == "working":
                self.cli("interrupt", row["name"], "--json", check=False,
                         payload=self.identity_request(row, expected_turn_started=row["turn_started"]))
                row = self.inspect(record["name"])
            if "pause" in row["allowed_actions"]:
                self.action(row, "pause")

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux fresh keyring-isolated auth control")
    def test_missing_auth_refuses_send(self):
        row = self.launch("no-account")
        self.assertTrue(row["auth_required"])
        self.assertFalse(row["send_available"])
        self.assertFalse(row["conversation_id"])
        self.assertFalse(row["resumable"])

    def test_exact_send_resume_cancel_stop_and_no_replay(self):
        profiles = json.loads(self.cli("account", "ls", "--provider", "agy", "--json").stdout)["profiles"]
        account, = profiles
        self.assertEqual((account["id"], account["auth_status"]), ("native-agy", "unknown"))
        self.assertTrue(account["launch_supported"])
        with self.model() as model:
            first = self.launch("one")
            second = self.launch("two")
            self.assertNotEqual(first["conversation_id"], second["conversation_id"])
            self.assertTrue(first["activity_supported"])
            self.assertTrue(first["send_available"])
            wrong = self.identity_request(first, text="Wrong session must refuse")
            self.assertNotEqual(self.cli("send", second["name"], "--json", payload=wrong, check=False).returncode, 0)
            request = self.identity_request(first, text="First exact disposable prompt")
            sent = self.cli("send", first["name"], "--json", payload=request)
            self.assertEqual(json.loads(sent.stdout)["status"], "submitted")
            self.assertEqual(json.loads(self.cli("send", first["name"], "--json", payload=request).stdout), json.loads(sent.stdout))
            settled = self.wait_for(lambda: (row if (row := self.inspect(first["name"]))["send_available"] and
                                              any(e["type"] == "AgentMessage" for e in row["events"]) else None))
            window = json.loads(self.cli("inspect", first["name"], "--after", "0", "--skip-processes").stdout)
            self.assertTrue(window["replace_events"])
            self.assertEqual(window["events"], settled["events"])
            self.assertEqual(json.loads(self.action(settled, "pause").stdout)["status"], "completed")
            stopped = self.inspect(first["name"])
            self.assertEqual(stopped["state"], "paused")
            self.assertFalse(stopped["send_available"])
            resumed_result = json.loads(self.action(stopped, "resume").stdout)
            self.assertEqual(resumed_result["status"], "completed")
            resumed = self.inspect(first["name"])
            self.assertEqual(resumed_result["result_target"], {
                "name": first["name"], "run_id": resumed["run_id"], "conversation_id": first["conversation_id"],
            })
            self.assertNotEqual(resumed["run_id"], first["run_id"])
            self.assertEqual(resumed["conversation_id"], first["conversation_id"])
            self.assertNotEqual(self.cli("send", first["name"], "--json", payload=request, check=False).returncode, 0)
            waiting = self.identity_request(resumed, text="HOLD-NATIVE-TEST")
            original = self.cli("send", resumed["name"], "--json", payload=waiting, check=False)
            self.assertEqual(json.loads(original.stdout)["status"], "uncertain")
            self.assertTrue(model.entered.get(timeout=2))
            duplicate = self.cli("send", resumed["name"], "--json", payload=waiting, check=False)
            self.assertEqual(json.loads(duplicate.stdout)["status"], "uncertain")
            working = self.inspect(resumed["name"])
            self.assertTrue(working["interrupt_supported"])
            wrong_turn = self.identity_request(working, expected_turn_started=working["turn_started"] + 1)
            self.assertNotEqual(self.cli("interrupt", resumed["name"], "--json", payload=wrong_turn, check=False).returncode, 0)
            cancel = self.identity_request(working, expected_turn_started=working["turn_started"])
            self.assertTrue(json.loads(self.cli("interrupt", resumed["name"], "--json", payload=cancel).stdout)["confirmed"])
            model.release.set()
            self.assertEqual(sum(b"HOLD-NATIVE-TEST" in body for body in model.offers), 1)
            self.assertFalse(any(b"Wrong session must refuse" in body for body in model.offers))
            idle = self.wait_for(lambda: (row if (row := self.inspect(resumed["name"]))["send_available"] else None))
            self.action(idle, "pause")
            self.action(second, "pause")


if __name__ == "__main__":
    unittest.main()
