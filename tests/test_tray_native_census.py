"""Updater preservation evidence for native agents; never signal an owner process."""
import importlib.util
from pathlib import Path
import selectors
import subprocess
import sys
import unittest
from unittest.mock import patch


REPO = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("tray_native_census", REPO / "tray/update-installed.py")
UPDATER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = UPDATER
SPEC.loader.exec_module(UPDATER)


class TrayNativeCensus(unittest.TestCase):
    def test_macos_keeps_antigravity_pid_and_birth(self):
        observed = "202 Thu Nov 26 12:34:56 2026 /Users/example/.local/bin/agy\n"
        with patch.object(UPDATER, "MAC", True), patch.object(UPDATER, "run", return_value=observed) as read:
            self.assertEqual(UPDATER.native().get("202"), {"name": "agy", "start": "Thu Nov 26 12:34:56 2026"})
        read.assert_called_once_with(["ps", "-x", "-o", "pid=,lstart=,comm="])

    def test_macos_keeps_existing_agents_without_matching_lookalikes(self):
        observed = "\n".join(
            f"{pid} Thu Nov 26 12:34:56 2026 /private/bin/{name}"
            for pid, name in [(201, "codex"), (203, "tmux: server"), (204, "node"),
                              (205, "agy-helper"), (206, "not-agy"), (207, "antigravity")]
        )
        with patch.object(UPDATER, "MAC", True), patch.object(UPDATER, "run", return_value=observed):
            records = UPDATER.native()
        self.assertEqual(set(records), {"201", "203", "204"})
        self.assertEqual(records["203"]["name"], "tmux: server")

    def test_macos_unavailable_process_read_does_not_become_an_empty_census(self):
        failure = subprocess.CalledProcessError(1, ["ps"])
        with patch.object(UPDATER, "MAC", True), patch.object(UPDATER, "run", side_effect=failure):
            with self.assertRaises(subprocess.CalledProcessError):
                UPDATER.native()

    @unittest.skipUnless(sys.platform.startswith("linux"), "real Linux procfs process identity")
    def test_linux_keeps_real_antigravity_child_and_kernel_start(self):
        code = (
            "import ctypes,sys; "
            "assert ctypes.CDLL(None).prctl(15,sys.argv[1].encode(),0,0,0)==0; "
            "print('ready',flush=True); sys.stdin.read()"
        )
        children = []
        try:
            for name in ("agy", "agy-helper"):
                child = subprocess.Popen([sys.executable, "-c", code, name],
                                         stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
                children.append(child)
                with selectors.DefaultSelector() as ready:
                    ready.register(child.stdout, selectors.EVENT_READ)
                    self.assertTrue(ready.select(3), "private child did not reach its owned barrier")
                self.assertEqual(child.stdout.readline().strip(), "ready")
            actual, lookalike = children
            start = Path(f"/proc/{actual.pid}/stat").read_text().rsplit(")", 1)[1].split()[19]
            with patch.object(UPDATER, "MAC", False):
                records = UPDATER.native()
            self.assertEqual(records.get(str(actual.pid)), {"name": "agy", "start": start})
            self.assertNotIn(str(lookalike.pid), records)
        finally:
            for child in children:
                child.stdin.close()
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    child.terminate()
                    child.wait(timeout=3)
                child.stdout.close()


if __name__ == "__main__":
    unittest.main()
