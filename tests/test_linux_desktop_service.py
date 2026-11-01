"""Installer ownership, configuration and graphical-session ordering."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import sys

ROOT = Path(__file__).resolve().parents[1]
installer = None
if sys.platform.startswith("linux"):
    spec = importlib.util.spec_from_file_location("linux_desktop_installer", ROOT / "scripts/install-linux-desktop-service.py")
    installer = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(installer)


class Manager:
    def __init__(self, test):
        self.test = test
        self.calls = []
        self.active = False
        self.fragment = ""
        self.dropins = ""
        self.state_override = None
        self.lifecycle = {"abi": installer.ABI, "configuration": "a" * 64, "ipc_ready": True,
                          "phase": "serving", "pid": 4321, "epoch": "original-epoch", "generation": 1,
                          "background": {"attention": {"ready": False, "code": "not_started"}}}

    def run(self, args, env, timeout=10):
        self.calls.append((list(args), dict(env)))
        if args[0] == str(self.test.hgs):
            if args[1:] == ["--help"]:
                return "hgs desktop registration --json"
            if args[1:] == ["desktop", "registration", "--json"]:
                return json.dumps(self.test.preview)
            self.test.assertEqual(args[1:], ["desktop", "status", "--json"])
            self.test.assertEqual(env["HGS_CONFIG_DIR"], self.test.scope["HGS_CONFIG_DIR"])
            self.test.assertEqual(env["HGS_STATE_DIR"], self.test.scope["HGS_STATE_DIR"])
            return json.dumps({"lifecycle": self.lifecycle})
        self.test.assertEqual(args[:2], ["systemctl", "--user"])
        action = args[2]
        if action == "show":
            if self.state_override is not None:
                return self.state_override
            fragment = self.fragment or (str(self.test.destination) if self.test.destination.exists() else "")
            return ("LoadState=" + ("loaded" if fragment else "not-found") + "\nActiveState="
                    + ("active" if self.active else "inactive") + "\nMainPID="
                    + ("4321" if self.active else "0") + "\nFragmentPath=" + fragment
                    + "\nDropInPaths=" + self.dropins + "\n")
        if action in ("enable", "start", "restart"):
            self.test.assertEqual(args[3:], [installer.UNIT])
        if action in ("start", "restart"):
            self.active = True
        return ""

    def actions(self):
        return [args[2] for args, _ in self.calls if args[0] == "systemctl"]


@unittest.skipUnless(sys.platform.startswith("linux"), "Linux service registration")
class LinuxDesktopService(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zerus-desktop-install-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.home = self.root / "home"
        self.home.mkdir()
        self.runtime = self.root / "runtime"
        self.runtime.mkdir(mode=0o700)
        self.hgs = self.root / "hgs cli%selected"
        self.hgs.write_bytes(b"fixture executable, not launched\n")
        self.hgs.chmod(0o755)
        self.env = {"PATH": "/usr/bin:/bin", "HOME": str(self.home), "XDG_CONFIG_HOME": str(self.home / "config"),
                    "XDG_RUNTIME_DIR": str(self.runtime), "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(self.runtime / "bus"),
                    "WAYLAND_DISPLAY": "wayland-0", "XDG_SESSION_TYPE": "wayland", "XDG_SESSION_ID": "session-one",
                    "DISPLAY": ":99", "LD_LIBRARY_PATH": "/private/gui-only-stack"}
        self.scope = {"HGS_CONFIG_DIR": str(self.root / "config $literal%"), "HGS_STATE_DIR": str(self.root / "state")}
        self.destination = self.home / "config/systemd/user/hgs-desktop.service"
        args = [str(self.hgs), "desktop", "serve", "--config-dir", self.scope["HGS_CONFIG_DIR"],
                "--state-dir", self.scope["HGS_STATE_DIR"]]
        definition = ("# Zerus desktop registration v1; configuration=" + "a" * 64
                      + "\n[Unit]\nAfter=graphical-session.target\nPartOf=graphical-session.target\n"
                      + "[Service]\nExecStart=" + " ".join(map(installer.unit_quote, args))
                      + "\nKillMode=process\nRestart=on-failure\nRestartPreventExitStatus=78\n"
                      + "[Install]\nWantedBy=graphical-session.target\n")
        self.preview = {"schema": 1, "configuration": "a" * 64, "label": installer.UNIT,
                        "destination": str(self.destination), "argv": args, "definition": definition,
                        "readiness": {"abi": installer.ABI, "argv": [str(self.hgs), "desktop", "status", "--json"],
                                      "environment": self.scope, "expected_configuration": "a" * 64,
                                      "configuration_field": "lifecycle.configuration", "ready_field": "lifecycle.ipc_ready",
                                      "epoch_field": "lifecycle.epoch", "generation_field": "lifecycle.generation"},
                        "session": {"kind": "graphical_user", "import_from": "current_login_session"}}
        self.manager = Manager(self)

    def instance(self):
        return installer.Installer(str(self.hgs), self.env, self.manager.run)

    def test_imports_only_current_session_before_starting_only_desktop(self):
        message = self.instance().install()
        self.assertIn("IPC ready", message)
        self.assertEqual(self.destination.read_text(), self.preview["definition"])
        actions = self.manager.actions()
        self.assertLess(actions.index("import-environment"), actions.index("start"))
        [imported] = [args[3:] for args, _ in self.manager.calls if args[:3] == ["systemctl", "--user", "import-environment"]]
        self.assertEqual(set(imported), {"XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS", "WAYLAND_DISPLAY",
                                         "XDG_SESSION_TYPE", "XDG_SESSION_ID"})
        self.assertNotIn("DISPLAY", imported)
        self.assertNotIn("LD_LIBRARY_PATH", imported)
        self.assertFalse(self.manager.lifecycle["background"]["attention"]["ready"])

    def test_ui_only_update_keeps_unchanged_backend(self):
        self.instance().install()
        before = self.destination.stat().st_mtime_ns
        self.manager.calls.clear()
        self.instance().install()
        self.assertNotIn("start", self.manager.actions())
        self.assertNotIn("restart", self.manager.actions())
        self.assertEqual(self.destination.stat().st_mtime_ns, before)

    def test_backend_binary_upgrade_restarts_only_desktop(self):
        self.instance().install()
        self.hgs.write_bytes(b"changed backend executable\n")
        self.manager.calls.clear()
        self.instance().install()
        actions = self.manager.actions()
        self.assertIn("restart", actions)
        self.assertLess(actions.index("import-environment"), actions.index("restart"))

    def test_refresh_does_not_create_register_enable_or_start_an_absent_service(self):
        self.instance().install(refresh=True)
        self.assertFalse(self.destination.parent.exists())
        self.assertEqual(self.manager.actions(), [])

    def test_refresh_ignores_session_changes_when_backend_unchanged(self):
        self.instance().install()
        self.env.update(WAYLAND_DISPLAY="wayland-1", XDG_SESSION_ID="session-two")
        self.manager.calls.clear()
        self.instance().install(refresh=True)
        self.assertEqual(self.manager.actions(), ["show", "show"])

    def test_refresh_updates_backend_without_enabling_an_inactive_service(self):
        self.instance().install()
        self.manager.active = False
        self.hgs.write_bytes(b"new CLI\n")
        self.manager.calls.clear()
        self.instance().install(refresh=True)
        self.assertIn("daemon-reload", self.manager.actions())
        for action in ("enable", "start", "restart", "import-environment"):
            self.assertNotIn(action, self.manager.actions())

    def test_refresh_restarts_only_changed_active_owned_backend(self):
        self.instance().install()
        self.hgs.write_bytes(b"new CLI\n")
        self.manager.calls.clear()
        self.instance().install(refresh=True)
        actions = self.manager.actions()
        self.assertLess(actions.index("import-environment"), actions.index("restart"))
        self.assertNotIn("enable", actions)

    def test_argv_upgrade_restarts_the_managed_backend(self):
        self.instance().install()
        old = str(self.hgs)
        replacement = self.root / "other hgs"
        replacement.write_bytes(self.hgs.read_bytes())
        replacement.chmod(0o755)
        self.hgs = replacement
        self.preview["argv"][0] = str(replacement)
        self.preview["readiness"]["argv"][0] = str(replacement)
        self.preview["definition"] = self.preview["definition"].replace(installer.unit_quote(old), installer.unit_quote(str(replacement)))
        self.manager.calls.clear()
        self.instance().install(refresh=True)
        self.assertIn("restart", self.manager.actions())

    def test_headless_backend_upgrade_remains_pending_without_restart(self):
        self.instance().install()
        self.hgs.write_bytes(b"new CLI\n")
        self.env.pop("XDG_SESSION_TYPE")
        self.manager.calls.clear()
        with self.assertRaisesRegex(installer.Refusal, "pending"):
            self.instance().install(refresh=True)
        record = json.loads(self.destination.with_name(".hgs-desktop.zerus-registration.json").read_text())
        self.assertTrue(record["pending_restart"])
        self.assertNotIn("restart", self.manager.actions())
        # A later deliberate refresh uses the saved pending state, even though
        # the definition and binary now equal the installed record.
        self.env["XDG_SESSION_TYPE"] = "wayland"
        self.instance().install(refresh=True)
        self.assertIn("restart", self.manager.actions())

    def test_readiness_pid_mismatch_keeps_the_active_service(self):
        self.instance().install()
        self.hgs.write_bytes(b"new CLI\n")
        self.manager.lifecycle["pid"] = 9999
        self.manager.calls.clear()
        with self.assertRaisesRegex(installer.Refusal, "PID/readiness"):
            self.instance().install(refresh=True)
        self.assertNotIn("restart", self.manager.actions())

    def test_transitional_or_masked_manager_state_does_not_change_registration(self):
        for raw in ("LoadState=loaded\nActiveState=activating\n", "LoadState=masked\nActiveState=inactive\n"):
            self.manager.state_override = raw
            with self.assertRaisesRegex(installer.Refusal, "state"):
                self.instance().install()
            self.assertFalse(self.destination.exists())

    def test_unit_directory_writable_by_others_is_kept(self):
        self.destination.parent.mkdir(parents=True)
        self.destination.parent.chmod(0o777)
        with self.assertRaisesRegex(installer.Refusal, "directory"):
            self.instance().install()
        self.assertFalse(self.destination.exists())

    def test_session_replacement_refreshes_the_running_child(self):
        self.instance().install()
        self.env.update(WAYLAND_DISPLAY="wayland-1", XDG_SESSION_ID="session-two")
        self.manager.calls.clear()
        self.instance().install(session=True)
        self.assertIn("restart", self.manager.actions())

    def test_headless_registration_does_not_start_with_stale_manager_data(self):
        self.env.pop("XDG_SESSION_TYPE")
        message = self.instance().install()
        self.assertIn("next graphical login", message)
        self.assertNotIn("start", self.manager.actions())
        self.assertNotIn("import-environment", self.manager.actions())
        self.assertTrue(self.destination.is_file())

    def test_check_is_read_only(self):
        self.instance().install(check=True)
        self.assertFalse(self.destination.parent.exists())
        self.assertEqual(self.manager.actions(), ["show"])

    def test_custom_and_edited_managed_definitions_are_preserved(self):
        self.destination.parent.mkdir(parents=True)
        self.destination.write_text("# custom service\n")
        with self.assertRaisesRegex(installer.Refusal, "custom"):
            self.instance().install()
        self.assertEqual(self.destination.read_text(), "# custom service\n")
        self.destination.unlink()
        self.instance().install()
        self.destination.write_text(self.destination.read_text() + "# local edit\n")
        self.manager.calls.clear()
        with self.assertRaisesRegex(installer.Refusal, "custom"):
            self.instance().install()
        self.assertNotIn("restart", self.manager.actions())
        self.assertTrue(self.destination.read_text().endswith("# local edit\n"))

    def test_symlink_and_global_custom_unit_are_not_adopted(self):
        target = self.root / "custom"
        target.write_text("user data")
        self.destination.parent.mkdir(parents=True)
        self.destination.symlink_to(target)
        with self.assertRaises(OSError):
            self.instance().install()
        self.assertEqual(target.read_text(), "user data")
        self.destination.unlink()
        self.manager.fragment = "/etc/systemd/user/hgs-desktop.service"
        with self.assertRaisesRegex(installer.Refusal, "custom"):
            self.instance().install()
        self.assertFalse(self.destination.exists())

    def test_custom_dropins_are_preserved(self):
        self.manager.dropins = "/etc/systemd/user/hgs-desktop.service.d/custom.conf"
        with self.assertRaisesRegex(installer.Refusal, "drop-ins"):
            self.instance().install()
        self.assertFalse(self.destination.exists())

    def test_another_configuration_and_abi_are_refused(self):
        self.instance().install()
        for change in ({"configuration": "b" * 64}, {"abi": "future"}):
            self.manager.lifecycle.update(change)
            self.manager.calls.clear()
            with self.assertRaisesRegex(installer.Refusal, "mismatch"):
                self.instance().install()
            self.assertNotIn("restart", self.manager.actions())
            self.manager.lifecycle.update(configuration="a" * 64, abi=installer.ABI)

    def test_probe_cannot_substitute_another_command_or_configuration(self):
        for field, value in (("argv", [str(self.hgs), "other-command"]),
                             ("expected_configuration", "b" * 64)):
            original = copy.deepcopy(self.preview)
            self.preview["readiness"][field] = value
            with self.assertRaises(installer.Refusal):
                self.instance()
            self.preview = original

    def test_ready_probe_requires_live_pid_epoch_and_generation(self):
        for field, value in (("pid", True), ("epoch", ""), ("generation", None), ("phase", "unavailable")):
            with self.subTest(field=field):
                before = self.manager.lifecycle[field]
                self.manager.lifecycle[field] = value
                with self.assertRaisesRegex(installer.Refusal, "lifecycle identity"):
                    self.instance().probe()
                self.manager.lifecycle[field] = before

    def test_genuine_x11_clears_wayland_and_imports_its_display(self):
        self.env["XDG_SESSION_TYPE"] = "x11"
        self.instance().install()
        self.assertIn("unset-environment", self.manager.actions())
        [imported] = [args[3:] for args, _ in self.manager.calls if args[:3] == ["systemctl", "--user", "import-environment"]]
        self.assertIn("DISPLAY", imported)
        self.assertNotIn("WAYLAND_DISPLAY", imported)

    def test_incomplete_graphical_environment_never_starts_service(self):
        self.env.pop("DBUS_SESSION_BUS_ADDRESS")
        with self.assertRaisesRegex(installer.Refusal, "incomplete"):
            self.instance().install()
        self.assertNotIn("start", self.manager.actions())

    def test_command_output_and_deadline_are_bounded(self):
        with self.assertRaisesRegex(installer.Refusal, "output exceeded"):
            installer.command([sys.executable, '-c', 'print("x" * 70000)'], self.env)
        with self.assertRaisesRegex(installer.Refusal, "timed out"):
            installer.command([sys.executable, '-c', 'import time; time.sleep(5)'], self.env, timeout=0.05)


@unittest.skipUnless(sys.platform.startswith("linux"), "Linux CLI refresh hook")
class CliRefreshHook(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zerus-cli-refresh-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.repo = self.root / "repo"
        (self.repo / "scripts").mkdir(parents=True)
        shutil.copyfile(ROOT / "install.sh", self.repo / "install.sh")
        (self.repo / "hgs_state.py").write_text("# fixture shim\n")
        (self.repo / "scripts/install-linux-desktop-service.py").write_text(
            'import os, pathlib, sys\npathlib.Path(os.environ["HOME"], "refresh-call").write_text(repr(sys.argv[1:]))\n')
        fake = self.root / "bin"
        fake.mkdir()
        for name, body in {
                "cargo": 'mkdir -p "$CARGO_TARGET_DIR/release"\nprintf \'#!/bin/sh\\necho hgs-fixture\\n\' > "$CARGO_TARGET_DIR/release/hgs"\n',
                "uname": 'echo Linux\n', "id": 'echo "${FIXTURE_UID:-1000}"\n'}.items():
            script = fake / name
            script.write_text("#!/bin/sh\n" + body)
            script.chmod(0o755)
        self.home = self.root / "home"
        self.home.mkdir()
        self.env = {"PATH": str(fake) + ":/usr/bin:/bin", "HOME": str(self.home),
                    "CARGO_TARGET_DIR": str(self.root / "build"), "HGS_MOBILE_CONNECTOR": "0",
                    "HGS_RECOVERY_SERVICE": "0", "HGS_SWARM_SERVICE": "0", "HGS_VOICE_SERVICE": "0"}

    def invoke(self, **extra):
        result = subprocess.run(["bash", str(self.repo / "install.sh")], env=dict(self.env, **extra),
                                stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_deliberate_per_user_cli_upgrade_uses_the_same_refresh_helper(self):
        self.invoke()
        call = (self.home / "refresh-call").read_text()
        self.assertEqual(call, repr(["refresh", "--hgs", str(self.home / ".local/bin/hgs")]))

    def test_staging_root_and_foreign_configurations_never_refresh(self):
        for extra in ({"HGS_INSTALL_PREFIX": str(self.root / "stage")}, {"DESTDIR": str(self.root / "package")},
                      {"HGS_CONFIG_DIR": str(self.root / "foreign-config")},
                      {"HGS_STATE_DIR": str(self.root / "foreign-state")}, {"FIXTURE_UID": "0"}):
            with self.subTest(extra=extra):
                self.invoke(**extra)
                self.assertFalse((self.home / "refresh-call").exists())


if __name__ == "__main__":
    unittest.main()
