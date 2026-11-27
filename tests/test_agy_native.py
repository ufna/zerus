"""Run the real Antigravity CLI against a disposable, loopback model endpoint."""
from contextlib import contextmanager
from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import hashlib
import json
import os
from pathlib import Path
from queue import Queue
import shutil
import shlex
import socket
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
LEGACY_HGS = os.environ.get("AGY_LEGACY_HGS_TEST_BIN")
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

    def inspect(self, name, *options):
        return json.loads(self.cli("inspect", name, *options).stdout)

    def identity_request(self, row, **fields):
        return {"request_id": str(uuid.uuid4()), "expected_run_id": row["run_id"],
                "expected_conversation_id": row["conversation_id"], **fields}

    def action(self, row, action, check=True, **fields):
        return self.cli("session-action", row["name"], "--json",
                        payload=self.identity_request(row, action=action, **fields), check=check)

    def settings_request(self, row, **fields):
        return self.identity_request(row, expected_session_id=row["session_id"],
                                     expected_config_revision=row["agy_settings"]["revision"], **fields)

    def settings(self, row, request, check=True):
        result = self.cli("settings", row["name"], "--json", payload=request, check=check)
        if not check:
            self.assertNotEqual(result.returncode, 0, result.stdout)
        answer = json.loads(result.stdout)
        self.assertEqual((answer["request_id"], answer["name"], answer["run_id"],
                          answer["conversation_id"], answer["session_id"]),
                         (request["request_id"], row["name"], request["expected_run_id"],
                          request["expected_conversation_id"], request["expected_session_id"]))
        return answer

    def test_native_settings_use_the_offered_model_and_effort_without_a_prompt(self):
        from test_watch import Stream

        with self.model() as endpoint:
            first = self.launch("native-settings")
            self.assertTrue(first["settings_change_supported"])
            self.assertTrue(first["model_change_supported"])
            self.assertEqual(first["agy_settings"]["status"], "confirmed")
            self.assertEqual(first["agy_quota"]["status"], "unavailable")
            self.assertIsNone(first["session_usage"]["last_turn"])
            stream = Stream(self, executable=HGS)
            stream.next(lambda frame: frame["t"] == "hello")
            stream.next(lambda frame: frame["t"] == "snapshot" and frame["topic"] == "catalog" and frame["last"])
            stream.send(op="subscribe", topic="activity", session={
                key: first[key] for key in ("name", "session_id", "run_id", "conversation_id")
            })
            watched = stream.next(lambda frame: frame["t"] == "state")["value"]
            self.assertEqual(watched["agy_settings"], first["agy_settings"])
            mode = first["agy_settings"]["mode"]
            selected = first["settings_model"]
            next_model = next(option for option in first["model_options"] if option["id"] != selected)
            self.assertNotIn("effort_options", next_model)
            model_id = next_model["id"]
            bad = self.settings_request(first, model="never-advertised-model")
            self.assertEqual(self.settings(first, bad, check=False)["status"], "refused")
            wrong_run = dict(self.settings_request(first, model=model_id), expected_run_id=str(uuid.uuid4()))
            self.assertEqual(self.settings(first, wrong_run, check=False)["status"], "refused")
            both = self.settings_request(first, model=model_id, effort="high")
            self.assertEqual(self.settings(first, both, check=False)["status"], "refused")
            self.assertEqual(self.inspect(first["name"])["settings_model"], selected)

            change = self.settings_request(first, model=model_id)
            receipt = self.settings(first, change)
            self.assertEqual((receipt["status"], receipt["model"]), ("applied", model_id))
            now = self.inspect(first["name"])
            self.assertEqual(now["model"], model_id)
            self.assertEqual(now["agy_settings"]["revision"], receipt["config_revision"])
            self.assertNotEqual(now["agy_settings"]["revision"], first["agy_settings"]["revision"])
            self.assertEqual(self.settings(first, change), receipt)
            stale = self.settings_request(first, model=selected)
            self.assertEqual(self.settings(first, stale, check=False)["status"], "refused")

            offered = next(option for option in now["model_options"] if option["id"] == model_id)
            different = next(effort for effort in offered["effort_options"] if effort != now["effort"])
            effort = self.settings_request(now, model=model_id, effort=different)
            result = self.settings(now, effort)
            self.assertEqual((result["status"], result["model"], result["effort"]),
                             ("applied", model_id, different))
            now = self.inspect(first["name"])
            self.assertEqual(now["agy_settings"]["mode"], mode)
            self.assertEqual(now["effort"], different)
            self.assertEqual(endpoint.offers, [])
            watched = stream.next(lambda frame: frame["t"] == "state"
                                  and frame["value"]["agy_settings"].get("revision") == result["config_revision"])["value"]
            self.assertEqual((watched["model"], watched["effort"]), (model_id, different))

            prompt = self.identity_request(now, text="Disposable context usage fixture")
            self.cli("send", now["name"], "--json", payload=prompt)
            observed = self.wait_for_reply(now["name"], prompt)
            self.assertGreater(observed["session_usage"]["context"]["limit"], 0)
            self.assertGreater(observed["session_usage"]["context"]["used"], 0)
            self.assertGreater(observed["session_usage"]["last_turn"]["total_tokens"], 0)
            self.assertEqual(observed["agy_quota"]["status"], "unavailable")
            self.assertEqual(stream.close(), 0)

    def test_settings_refuse_during_the_original_prompt(self):
        with self.model() as endpoint:
            row = self.launch("busy-settings")
            next_model = next(model["id"] for model in row["model_options"] if model["id"] != row["model"])
            active = self.identity_request(row, text="HOLD-NATIVE-TEST original turn")
            with ThreadPoolExecutor(max_workers=1) as sender:
                pending = sender.submit(self.cli, "send", row["name"], "--json", payload=active)
                try:
                    self.assertTrue(endpoint.entered.get(timeout=5))
                    current = self.inspect(row["name"])
                    self.assertFalse(current["settings_change_supported"])
                    self.assertEqual(current["model"], row["model"])
                    request = self.settings_request(row, model=next_model)
                    self.assertEqual(self.settings(row, request, check=False)["status"], "refused")
                finally:
                    endpoint.release.set()
                pending.result(timeout=6)
            self.assertEqual(self.wait_for_reply(row["name"], active)["model"], row["model"])

    def hold_settings_response(self):
        """Delay one exact reply from the real ACP process, forwarding other frames."""
        wrapper = self.bin / "agy"
        wrapper.unlink()
        wrapper.write_text(
            r"""#!/usr/bin/env python3
import json
from pathlib import Path
import subprocess
import sys
from threading import Lock, Thread
import time

root = Path(sys.argv[0]).parent
provider = subprocess.Popen([str(root / "provider"), *sys.argv[1:]],
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE)
pending = set()
writer = Lock()

def emit(line):
    with writer:
        sys.stdout.buffer.write(line)
        sys.stdout.buffer.flush()

def later(line):
    (root / "held").touch()
    while not (root / "release").exists():
        if provider.poll() is not None:
            return
        time.sleep(.02)
    emit(line)

def read():
    for line in provider.stdout:
        frame = json.loads(line)
        if "method" not in frame and frame.get("id") in pending:
            Thread(target=later, args=(line,), daemon=True).start()
        else:
            emit(line)

reader = Thread(target=read, daemon=True)
reader.start()
for line in sys.stdin.buffer:
    frame = json.loads(line)
    if frame.get("method") == "session/set_config_option":
        pending.add(frame["id"])
        with (root / "settings-offers").open("a") as trace:
            trace.write("one offer\n")
    provider.stdin.write(line)
    provider.stdin.flush()
provider.stdin.close()
result = provider.wait(timeout=5)
reader.join(1)
sys.exit(result)
"""
        )
        wrapper.chmod(0o700)
        (self.bin / "provider").symlink_to(Path(AGY).resolve())
        self.addCleanup((self.bin / "release").touch)

    def test_unknown_settings_never_reoffer_and_late_result_confirms_original(self):
        self.hold_settings_response()
        with self.model() as endpoint:
            first = self.launch("slow-settings")
            next_model = next(model["id"] for model in first["model_options"] if model["id"] != first["model"])
            request = self.settings_request(first, model=next_model)
            try:
                unknown = self.settings(first, request, check=False)
                self.assertEqual(unknown["status"], "uncertain")
                self.assertTrue((self.bin / "held").exists())
                pending = self.inspect(first["name"])
                self.assertEqual(pending["agy_settings"]["reason"], "awaiting_native_result")
                self.assertFalse(pending["settings_change_supported"])
                self.assertEqual(self.settings(first, request, check=False), unknown)
                self.assertEqual((self.bin / "settings-offers").read_text().splitlines(), ["one offer"])
            finally:
                (self.bin / "release").touch()
            updated = self.wait_for(lambda: row if (row := self.inspect(first["name"]))["settings_change_supported"] else None)
            result = self.settings(first, request)
            self.assertEqual(result["config_revision"], updated["agy_settings"]["revision"])
            self.assertEqual(result["model"], next_model)
            no_op = self.settings(updated, self.settings_request(updated, model=next_model))
            self.assertFalse(no_op["changed"])
            self.assertEqual(no_op["config_revision"], result["config_revision"])
            self.assertEqual(endpoint.offers, [])
            self.assertEqual((self.bin / "settings-offers").read_text().splitlines(), ["one offer"])

    def test_resume_keeps_an_earlier_unconfirmed_settings_request_unknown(self):
        self.hold_settings_response()
        with self.model() as endpoint:
            original = self.launch("unknown-settings")
            next_model = next(model["id"] for model in original["model_options"] if model["id"] != original["model"])
            request = self.settings_request(original, model=next_model)
            unknown = self.settings(original, request, check=False)
            self.assertEqual(unknown["status"], "uncertain")
            self.assertTrue((self.bin / "held").exists())
            self.action(original, "pause")
            self.wait_for(lambda: "resume" in self.inspect(original["name"])["allowed_actions"])
            self.action(original, "resume")
            resumed = self.inspect(original["name"])
            self.assertEqual(resumed["conversation_id"], original["conversation_id"])
            self.assertNotEqual(resumed["run_id"], original["run_id"])
            self.assertEqual(resumed["agy_settings"]["status"], "confirmed")
            self.assertNotEqual(resumed["agy_settings"]["revision"], original["agy_settings"]["revision"])
            self.assertEqual(self.settings(original, request, check=False), unknown)
            self.assertEqual((self.bin / "settings-offers").read_text().splitlines(), ["one offer"])
            self.assertEqual(endpoint.offers, [])

    @unittest.skipUnless(LEGACY_HGS, "Optional mixed worker compatibility test")
    def test_older_worker_resumes_without_inheriting_current_model_evidence(self):
        with self.model() as endpoint:
            first = self.launch("mixed-settings")
            next_model = next(model["id"] for model in first["model_options"] if model["id"] != first["model"])
            request = self.settings_request(first, model=next_model)
            applied = self.settings(first, request)
            self.action(first, "pause")
            self.wait_for(lambda: "resume" in self.inspect(first["name"])["allowed_actions"])
            old_resume = self.identity_request(first, action="resume")
            result = subprocess.run([LEGACY_HGS, "session-action", first["name"], "--json"],
                                    input=json.dumps(old_resume), text=True, capture_output=True,
                                    timeout=15, cwd=self.project, env=self.env)
            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            restored = self.inspect(first["name"])
            self.assertNotEqual(restored["run_id"], first["run_id"])
            self.assertEqual(restored["conversation_id"], first["conversation_id"])
            self.assertEqual(restored["agy_settings"], {"status": "unavailable", "reason": "legacy_worker"})
            self.assertFalse(restored["settings_change_supported"])
            self.assertNotIn("model", restored)
            self.assertNotIn("session_usage", restored)
            self.assertEqual(self.settings(first, request), applied)
            self.assertEqual(endpoint.offers, [])

    def wait_for_reply(self, name, request):
        def settled():
            row = self.inspect(name)
            if row["send_available"] and row["reply_id"] == request["request_id"]:
                return row

        return self.wait_for(settled)

    def record(self, name):
        records = (self.root / "state").glob("*.json")
        return next(record for path in records if (record := json.loads(path.read_bytes())).get("name") == name)

    def offer(self, row, request):
        # The exact control frames of two callers that already passed the public
        # receipt lookup, with the second scheduled after the first native offer.
        record = self.record(row["name"])
        control = {key: record[key] for key in ("name", "session_id", "run_id", "conversation_id")}
        control.update(action="send", request_id=request["request_id"], text=request["text"],
                       digest=hashlib.sha256(json.dumps(request).encode()).hexdigest())
        stream = socket.socket(socket.AF_UNIX)
        self.addCleanup(stream.close)
        stream.settimeout(10)
        stream.connect(record["agy_socket"])
        stream.sendall(json.dumps(control).encode())
        stream.shutdown(socket.SHUT_WR)
        return stream

    def read_offer(self, stream):
        with stream.makefile("rb") as reply:
            return json.load(reply)

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
            settings.parent.mkdir(parents=True, exist_ok=True)
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
    def test_missing_auth_disables_send(self):
        row = self.launch("no-account")
        self.assertTrue(row["auth_required"])
        self.assertFalse(row["send_available"])
        self.assertFalse(row["conversation_id"])
        self.assertTrue(row["startup_retry_available"])
        self.assertNotIn("archive", row["allowed_actions"])
        self.assertNotEqual(self.cli("archive", row["name"], check=False).returncode, 0)

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux fresh keyring-isolated auth control")
    def test_guarded_retry_after_proven_sign_in_refusal(self):
        row = self.launch("retry-sign-in")
        self.assertTrue(row["auth_required"])
        self.assertFalse(row["conversation_id"])
        with self.model():
            wrong_run = self.identity_request(row, action="resume", expected_run_id=str(uuid.uuid4()))
            refused = self.cli("session-action", row["name"], "--json", payload=wrong_run)
            self.assertEqual(json.loads(refused.stdout)["status"], "failed")
            self.assertEqual(self.inspect(row["name"])["run_id"], row["run_id"])
            receipt = json.loads(self.action(row, "resume").stdout)
            self.assertEqual(receipt["status"], "completed")
            new = self.inspect(row["name"])
            self.assertEqual(receipt["result_target"]["conversation_id"], new["conversation_id"])
            self.assertNotEqual(row["run_id"], new["run_id"])
            self.assertEqual(row["session_id"], new["session_id"])
            self.assertEqual(row["launch_id"], new["launch_id"])
            self.assertTrue(new["send_available"])

    def test_late_conflicting_send_keeps_original_turn(self):
        with self.model() as model:
            row = self.launch("conflict")
            first = self.identity_request(row, text="HOLD-NATIVE-TEST")
            original = self.offer(row, first)
            self.assertTrue(model.entered.get(timeout=5))
            conflict = self.offer(row, {**first, "text": "Conflicting payload"})
            try:
                refused = self.read_offer(conflict)
                self.assertEqual(refused["status"], "refused")
            finally:
                model.release.set()
            self.assertEqual(self.read_offer(original)["status"], "submitted")
            self.assertTrue(self.wait_for(lambda: self.inspect(row["name"])["send_available"]))
            self.assertFalse(any(b"Conflicting payload" in body for body in model.offers))

    def test_replaced_workspace_refuses_without_exiting(self):
        with self.model() as model:
            row = self.launch("replaced-workspace")
            saved = self.project.with_name("original-project")
            self.project.rename(saved)
            self.project.mkdir()
            try:
                replaced = self.inspect(row["name"])
                refused = self.cli("send", row["name"], "--json",
                                   payload=self.identity_request(row, text="Must never be offered"), check=False)
                self.assertNotEqual(refused.returncode, 0)
                self.assertFalse(replaced["send_available"])
                self.assertEqual(self.inspect(row["name"])["process_state"], "running")
            finally:
                self.project.rmdir()
                saved.rename(self.project)
            self.assertTrue(self.inspect(row["name"])["send_available"])
            self.assertEqual(model.offers, [])

    def test_stopped_archive_restores_only_the_selected_native_conversation(self):
        with self.model() as model:
            first = self.launch("archive")
            name = first["name"]
            peer = self.launch("keep-live")
            peer_turn = self.identity_request(peer, text="Unrelated conversation")
            self.cli("send", peer["name"], "--json", payload=peer_turn)
            peer = self.wait_for_reply(peer["name"], peer_turn)

            self.assertNotIn("archive", first["allowed_actions"])
            self.assertNotEqual(self.cli("archive", name, check=False).returncode, 0)
            request = self.identity_request(first, text="HOLD-NATIVE-TEST original conversation")
            turn = self.offer(first, request)
            try:
                self.assertTrue(model.entered.get(timeout=5))
                working = self.inspect(name)
                self.assertEqual(working["phase"], "working")
                self.assertNotIn("archive", working["allowed_actions"])
                self.assertEqual(json.loads(self.action(working, "archive").stdout)["status"], "failed")
            finally:
                model.release.set()
            self.assertEqual(self.read_offer(turn)["status"], "submitted")
            ready = self.wait_for_reply(name, request)
            self.assertEqual(json.loads(self.action(ready, "pause").stdout)["status"], "completed")
            stopped = self.inspect(name)
            self.assertIn("archive", stopped["allowed_actions"], stopped["action_reasons"])
            archived = json.loads(self.action(stopped, "archive").stdout)
            self.assertEqual(archived["status"], "completed", archived)
            archive_id = archived["result_target"]["archive_id"]

            saved = self.inspect(name, "--archive", archive_id)
            self.assertEqual(saved["state"], "archived")
            self.assertIn("restore", saved["allowed_actions"], saved["action_reasons"])
            self.assertTrue(saved["resumable"])
            self.assertFalse(saved["send_available"])
            self.assertFalse(saved["interrupt_supported"])
            for key in ("name", "session_id", "conversation_id", "run_id"):
                self.assertEqual(saved[key], first[key], key)
            self.assertEqual(saved["events"], ready["events"])
            self.assertTrue(any(event["detail"] == request["text"] for event in saved["events"]))
            self.assertTrue(any(event["type"] == "AgentMessage" for event in saved["events"]))
            before = saved["events"][0]["seq"]
            tail = self.inspect(name, "--archive", archive_id, "--after", str(before), "--skip-processes")
            self.assertEqual(tail["events"], [event for event in saved["events"] if event["seq"] > before])
            self.assertFalse(tail["replace_events"])

            stale = self.action(saved, "restore", archive_id=archive_id,
                                expected_conversation_id=peer["conversation_id"])
            self.assertEqual(json.loads(stale.stdout)["status"], "failed")
            restore = self.identity_request(saved, action="restore", archive_id=archive_id)
            receipt = json.loads(self.cli("session-action", name, "--json", payload=restore).stdout)
            self.assertEqual(receipt["status"], "completed", receipt)
            self.assertEqual(json.loads(self.cli("session-action", name, "--json", payload=restore).stdout), receipt)
            restored = self.inspect(name)
            for key in ("name", "session_id", "conversation_id"):
                self.assertEqual(restored[key], first[key], key)
            self.assertNotEqual(restored["run_id"], first["run_id"])
            self.assertEqual(receipt["result_target"], {key: restored[key] for key in ("name", "run_id", "conversation_id")})
            self.assertEqual(restored["events"], saved["events"])
            self.assertNotEqual(self.cli("inspect", name, "--archive", archive_id, check=False).returncode, 0)

            followup = self.identity_request(restored, text="Continue the restored conversation")
            self.cli("send", name, "--json", payload=followup)
            self.wait_for_reply(name, followup)
            resumed_offers = [body for body in model.offers if followup["text"].encode() in body]
            self.assertTrue(any(request["text"].encode() in body for body in resumed_offers))
            self.assertFalse(any(peer_turn["text"].encode() in body for body in resumed_offers))
            other = self.inspect(peer["name"])
            for key in ("run_id", "conversation_id", "events", "send_available"):
                self.assertEqual(other[key], peer[key], key)

    def test_cli_restore_selects_the_saved_archive_after_name_reuse(self):
        with self.model():
            original = self.launch("reused")
            name = original["name"]
            request = self.identity_request(original, text="Earlier conversation with this name")
            self.cli("send", name, "--json", payload=request)
            self.wait_for_reply(name, request)
            self.action(original, "pause")
            self.cli("archive", name)
            self.cli("__state", "archive-catalog-worker")

            def archive_page_row():
                page = self.cli("archives", "--json", "--bounded", payload={"limit": 2})
                row, = json.loads(page.stdout)["sessions"]
                return row

            archive = archive_page_row()
            self.assertEqual(archive["conversation_id"], original["conversation_id"])
            self.assertIn("restore", archive["allowed_actions"])
            self.assertTrue(archive["resumable"])
            archive_id = archive["archive_id"]

            replacement = self.launch("reused")
            self.assertNotEqual(replacement["conversation_id"], original["conversation_id"])
            for occupied in (self.inspect(name, "--archive", archive_id), archive_page_row()):
                self.assertEqual(occupied["archive_id"], archive_id)
                self.assertNotIn("restore", occupied["allowed_actions"])
                self.assertFalse(occupied["resumable"])
            self.assertNotEqual(self.cli("resume", name, "--archive", archive_id, check=False).returncode, 0)
            self.assertEqual(self.inspect(name)["run_id"], replacement["run_id"])

            self.action(replacement, "pause")
            later = json.loads(self.action(replacement, "archive").stdout)
            later_id = later["result_target"]["archive_id"]
            self.assertNotEqual(later_id, archive_id)
            self.cli("resume", name, "--archive", archive_id)
            current = self.inspect(name)
            self.assertEqual(current["session_id"], original["session_id"])
            self.assertEqual(current["conversation_id"], original["conversation_id"])
            self.assertNotEqual(current["run_id"], original["run_id"])
            other = self.inspect(name, "--archive", later_id)
            self.assertEqual(other["conversation_id"], replacement["conversation_id"])

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux fresh keyring-isolated auth control")
    def test_archive_survives_native_sign_in_refusal_during_restore(self):
        with self.model():
            first = self.launch("restore-sign-in")
            name = first["name"]
            request = self.identity_request(first, text="Keep history through a restore refusal")
            self.cli("send", name, "--json", payload=request)
            ready = self.wait_for_reply(name, request)
            self.action(ready, "pause")
            archived = json.loads(self.action(ready, "archive").stdout)
            archive_id = archived["result_target"]["archive_id"]
            key = self.env.pop("GEMINI_API_KEY")
            try:
                refused = json.loads(self.action(first, "restore", archive_id=archive_id).stdout)
                self.assertEqual(refused["status"], "failed", refused)
                self.assertIn("sign-in", refused["error"].lower())
            finally:
                self.env["GEMINI_API_KEY"] = key
            source = self.inspect(name, "--archive", archive_id)
            self.assertEqual(source["events"], ready["events"])
            self.assertIn("restore", source["allowed_actions"], source["action_reasons"])
            self.cli("resume", name, "--archive", archive_id)
            current = self.inspect(name)
            self.assertEqual(current["conversation_id"], first["conversation_id"])
            self.assertEqual(current["events"], ready["events"])

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux worker admission gate")
    def test_restore_recovers_after_launcher_exits_before_worker_adoption(self):
        gate = self.root / "worker-gate.so"
        source = Path(__file__).parent / "fixtures/agy_worker_start_gate.c"
        subprocess.run(["cc", "-shared", "-fPIC", str(source), "-o", str(gate)], check=True,
                       env=self.env, capture_output=True, timeout=30)
        with self.model():
            first = self.launch("abandoned-restore")
            name = first["name"]
            request = self.identity_request(first, text="Keep the original in the abandoned restore")
            self.cli("send", name, "--json", payload=request)
            self.wait_for_reply(name, request)
            self.action(first, "pause")
            archive_id = json.loads(self.action(first, "archive").stdout)["result_target"]["archive_id"]
            worker_log = self.root / "original-worker.log"
            release = self.root / "release"
            provider_starts = self.root / "provider-starts"
            agy = self.bin / "agy"
            agy.unlink()
            agy.write_text(f"#!/bin/sh\necho started >> {shlex.quote(str(provider_starts))}\n"
                           f"exec {shlex.quote(str(Path(AGY).resolve()))} \"$@\"\n")
            agy.chmod(0o700)

            env = dict(self.env, LD_PRELOAD=str(gate), AGY_TEST_WORKER_LOG=str(worker_log),
                       AGY_TEST_WORKER_RELEASE=str(release))
            with subprocess.Popen([HGS, "resume", name, "--archive", archive_id], cwd=self.project,
                                  env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL) as launcher:
                try:
                    self.wait_for(worker_log.exists)
                    pending = self.inspect(name, "--archive", archive_id)
                    self.assertNotIn("restore", pending["allowed_actions"])
                    self.assertEqual(json.loads(self.action(first, "restore", archive_id=archive_id).stdout)["status"], "failed")
                    old_run = self.record(name)["run_id"]
                    launcher.kill()
                    launcher.wait(timeout=4)
                    self.assertFalse(provider_starts.exists())
                    self.assertIn("restore", self.inspect(name, "--archive", archive_id)["allowed_actions"])
                    receipt = json.loads(self.action(first, "restore", archive_id=archive_id).stdout)
                    self.assertEqual(receipt["status"], "completed", receipt)
                    restored = self.inspect(name)
                    self.assertEqual(restored["conversation_id"], first["conversation_id"])
                    self.assertNotIn(restored["run_id"], (old_run, first["run_id"]))
                finally:
                    launcher.kill()
                    release.touch()
            self.wait_for(lambda: "already selected" in worker_log.read_text())
            self.assertEqual(provider_starts.read_text().splitlines(), ["started"])
            self.assertEqual(self.inspect(name)["run_id"], restored["run_id"])

    def test_interrupt_reservation_has_bounded_writer_admission(self):
        import fcntl

        with self.model() as model:
            row = self.launch("interrupt-writer")
            original = self.offer(row, self.identity_request(row, text="HOLD-NATIVE-TEST"))
            self.assertTrue(model.entered.get(timeout=5))
            working = self.inspect(row["name"])
            request = self.identity_request(working, expected_turn_started=working["turn_started"])
            args = [HGS, "interrupt", row["name"], "--json"]
            with (self.root / "state" / ".lock").open("rb") as writer:
                fcntl.flock(writer, fcntl.LOCK_EX)
                with subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                      stderr=subprocess.PIPE, text=True, cwd=self.project,
                                      env=self.env) as attempt:
                    try:
                        _, error = attempt.communicate(json.dumps(request), timeout=3.5)
                    except subprocess.TimeoutExpired:
                        attempt.kill()
                        attempt.communicate(timeout=2)
                        self.fail("Interrupt did not return while the state writer was held")
                self.assertEqual(attempt.returncode, 1)
                self.assertIn("state is busy", error)
                receipt = self.root / "state" / "interrupt_receipts" / f"{request['request_id']}.json"
                self.assertFalse(receipt.exists())
            exact = self.cli(*args[1:], payload=request)
            self.assertTrue(json.loads(exact.stdout)["confirmed"])
            self.assertEqual(self.read_offer(original)["status"], "submitted")
            model.release.set()

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
