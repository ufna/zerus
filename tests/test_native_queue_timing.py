"""Guarded sends and native queue inspection in an isolated scripted terminal."""
from concurrent.futures import ThreadPoolExecutor
import json
import os
import subprocess
import sys
import time
import unittest

import test_input as transport


@unittest.skipIf(os.name == "nt", "POSIX tmux process-group fixture")
class NativeQueueTiming(unittest.TestCase):
    setUp = transport.InputTransport.setUp
    tearDown = transport.InputTransport.tearDown
    write_record = transport.InputTransport.write_record
    script = transport.InputTransport.script
    tmux = transport.InputTransport.tmux
    wait_for = transport.InputTransport.wait_for
    received = transport.InputTransport.received
    screen = transport.InputTransport.screen
    payload = transport.InputTransport.payload
    send = transport.InputTransport.send
    inspect = transport.InputTransport.inspect

    def queue(self, preview=None):
        self.record.update(run_identity_version=1, activity="busy", phase="working",
                           supervisor=dict(pid=self.record["pid"], start=self.record["process_start"]))
        self.write_record()
        self.draw(preview)

    def draw(self, preview=None):
        rows = ["› "] if preview is None else [
            "• Messages to be submitted after next tool call",
            "  (press esc to interrupt and send immediately)",
            *preview.splitlines(), "", "› ",
        ]
        self.screen("\x1b[2J\x1b[H" + "\r\n".join(rows), f"2:{len(rows) - 1}")

    def terminal_send(self, text):
        self.tmux("send-keys", "-t", self.pane, "-l", text)
        self.tmux("send-keys", "-t", self.pane, "Enter")
        self.wait_for(lambda: self.received().count(b"\r") == 2)

    def test_removed_and_reinserted_equal_preview_never_inherits_a_time(self):
        self.queue()
        text = "Private queue input with an identical replacement"
        request = self.payload(text=text)
        receipt = self.send(request)
        self.wait_for(lambda: self.received().endswith(b"\r"))
        self.draw("  ↳ " + text)
        before = self.inspect()["input_queue"]
        self.assertNotIn("entry_times", before)
        self.assertNotIn("native_queue", receipt)
        self.draw()
        self.assertIsNone(self.inspect()["input_queue"])
        self.terminal_send(text)
        self.draw("  ↳ " + text)
        after = self.inspect()["input_queue"]
        self.assertEqual(after["id"], before["id"], "the Send now snapshot may legitimately recur")
        self.assertNotIn("entry_times", after)
        self.assertEqual(self.send(request), receipt)
        self.assertEqual(self.received().count(b"\r"), 2)

    def test_clipped_collision_and_delayed_insertion_are_unavailable(self):
        self.queue()
        prefix = "Same visible part of different private messages"
        request = self.payload(text=prefix + "\nFirst full payload")
        receipt = self.send(request)
        self.wait_for(lambda: self.received().endswith(b"\r"))
        self.assertIsNone(self.inspect()["input_queue"])
        # Only the different second text appears in the mock provider's queue.
        self.terminal_send(prefix + "\nSecond full payload")
        self.draw("  ↳ " + prefix + "\n    …")
        queue = self.inspect()["input_queue"]
        self.assertIn(prefix, queue["text"])
        self.assertNotIn("entry_times", queue)
        self.assertNotIn("native_queue", receipt)

    def test_optional_capture_has_strict_limits_and_cannot_delay_settlement(self):
        self.queue()
        mode = self.root / "capture-mode"
        capture = self.root / "capture-pids"
        after_enter = self.root / "unexpected-send-capture"
        self.script("tmux", f"""#!{sys.executable}
import os
from pathlib import Path
import subprocess
import sys
import time
root = Path({str(self.root)!r})
if sys.argv[1] == "capture-pane":
    mode = root / "capture-mode"
    received = root / "received"
    if not mode.exists() and received.exists() and received.read_bytes().endswith(b"\\r"):
        (root / "unexpected-send-capture").touch()
        time.sleep(2)
    if mode.exists() and os.getpid() == os.getpgrp():
        kind = mode.read_text()
        if kind != "styled" or "-e" in sys.argv:
            helper = subprocess.Popen(["sleep", "10"])
            (root / "capture-pids").write_text(f"{{os.getpid()}} {{helper.pid}}")
            if kind == "oversize":
                if len(os.write(1, b"x" * (512 * 1024 + 4096))) == 0:
                    raise RuntimeError("fixture did not write")
            time.sleep(10)
os.execv({transport.TMUX!r}, [{transport.TMUX!r}, "-S", {str(self.socket)!r}, "-f", "/dev/null", *sys.argv[1:]])
""")
        request = self.payload(text="Private capture fixture")
        receipt = self.send(request)
        receipt_file = self.state / "input_receipts" / f"{request['request_id']}.json"
        self.assertEqual(receipt["status"], "submitted")
        self.assertEqual(json.loads(receipt_file.read_text()), receipt)
        self.assertFalse(after_enter.exists(), "delivery never waits for a queue capture")
        self.wait_for(lambda: self.received().endswith(b"\r"))
        self.assertEqual(self.received().count(b"\r"), 1)
        # Arm read-only inspection separately, after the successful receipt.
        mode.write_text("stall")
        self.draw("  ↳ Private capture fixture")
        for case in ("stall", "styled", "oversize"):
            with self.subTest(case=case):
                capture.unlink(missing_ok=True)
                mode.write_text(case)
                started = time.monotonic()
                with ThreadPoolExecutor(max_workers=1) as worker:
                    inspection = worker.submit(self.inspect)
                    self.wait_for(capture.exists)
                    self.assertEqual(self.send(request), receipt)
                    snapshot = inspection.result(timeout=4)["input_queue"]
                self.assertLess(time.monotonic() - started, 4)
                if case == "styled":
                    self.assertIs(snapshot["can_send_now"], False)
                else:
                    self.assertIsNone(snapshot)
                pids = capture.read_text().split()
                state = subprocess.run(["ps", "-o", "stat=", "-p", ",".join(pids)],
                                       capture_output=True, text=True).stdout.split()
                self.assertTrue(all(value.startswith(("Z", "X")) for value in state), state)
        self.assertEqual(self.received().count(b"\r"), 1)


if __name__ == "__main__":
    unittest.main()
