"""Mac installer contract with private files and a modelled launchd/CLI only."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("mac_desktop_installer", ROOT / "scripts/install-macos-desktop-service.py")
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class Commands:
    def __init__(self, home, binary):
        self.calls = []
        self.gui, self.loaded, self.pid = True, False, 4321
        self.config = "a" * 64
        self.destination = home / "Library/LaunchAgents" / (installer.LABEL + ".plist")
        self.argv = [str(binary), "desktop", "serve", "--config-dir", str(home / "config $literal"),
                     "--state-dir", str(home / "state %literal"), "--launchd"]
        definition = {"Label": installer.LABEL, "ProgramArguments": self.argv, "RunAtLoad": True,
                      "KeepAlive": {"SuccessfulExit": False}, "ThrottleInterval": 10,
                      "AbandonProcessGroup": True, "LimitLoadToSessionType": "Aqua"}
        content = plistlib.dumps(definition).decode().replace("?>", "?>\n<!-- " + installer.MARKER
                    + "; configuration=" + self.config + " -->", 1)
        self.plan = {"schema": 1, "operation": "preview", "configuration": self.config,
                     "label": installer.LABEL, "destination": str(self.destination), "argv": self.argv,
                     "definition": content, "session": {"kind": "aqua", "notification_bundle": "com.hgdev.hgs-tray"},
                     "readiness": {"abi": installer.ABI, "argv": [str(binary), "desktop", "status", "--json"],
                     "environment": {"HGS_CONFIG_DIR": self.argv[4], "HGS_STATE_DIR": self.argv[6]},
                     "expected_configuration": self.config, "configuration_field": "lifecycle.configuration",
                     "ready_field": "lifecycle.ipc_ready", "epoch_field": "lifecycle.epoch",
                     "generation_field": "lifecycle.generation"}}
        self.status = {"lifecycle": {"abi": installer.ABI, "configuration": self.config, "ipc_ready": True,
                        "pid": self.pid, "epoch": "fixture-epoch", "generation": 1,
                        "background": {"attention": {"ready": False, "code": "not_started"}}}}
        self.printed_path, self.printed_argv = str(self.destination), list(self.argv)

    def __call__(self, argv, env=None, **kwargs):
        self.calls.append((list(argv), dict(env or {})))
        code, output = 0, ""
        if argv[1:3] == ["desktop", "registration"]:
            output = json.dumps(self.plan)
        elif argv[1:3] == ["desktop", "status"]:
            output = json.dumps(self.status)
        elif argv[0] == "launchctl":
            if argv[1] == "print":
                if argv[2].count("/") == 1:
                    code = 0 if self.gui else 113
                else:
                    code = 0 if self.loaded else 113
                    output = "\tpath = " + self.printed_path + "\n\tprogram = " + self.printed_argv[0]
                    output += "\n\targuments = {\n" + "".join("\t\t" + arg + "\n" for arg in self.printed_argv)
                    output += "\t}\n\tpid = " + str(self.pid) + "\n"
            elif argv[1] == "bootout":
                self.loaded = False
            elif argv[1] == "bootstrap":
                self.printed_path = argv[3]
                self.printed_argv = plistlib.loads(Path(argv[3]).read_bytes())["ProgramArguments"]
                self.loaded = True
        else:
            raise AssertionError(argv)
        return subprocess.CompletedProcess(argv, code, output, "")

    def mutations(self):
        return [argv for argv, _ in self.calls if argv[0] == "launchctl" and argv[1] != "print"]


class MacDesktopService(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="zerus-mac-desktop-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.home = self.root / "home"; self.home.mkdir()
        self.binary = self.root / "hgs"; self.binary.write_text("synthetic CLI, never executed")
        self.binary.chmod(0o755)
        self.command = Commands(self.home, self.binary)

    def install(self, **kwargs):
        return installer.install(self.binary, self.home, {"HGS_EXECUTABLE": "/unselected/hgs"},
                                 command=self.command, **kwargs)

    def existing(self, content=None):
        path = self.command.destination
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(self.command.plan["definition"] if content is None else content)
        return path

    def test_aqua_registration_and_readiness_preserve_the_selected_config_and_cli(self):
        self.assertEqual(self.install(), "ready")
        definition = plistlib.loads(self.command.destination.read_bytes())
        self.assertEqual(definition["ProgramArguments"], self.command.argv)
        self.assertEqual(definition["LimitLoadToSessionType"], "Aqua")
        self.assertIs(definition["AbandonProcessGroup"], True)
        status_env = next(env for argv, env in self.command.calls if argv[1:3] == ["desktop", "status"])
        self.assertEqual(status_env["HGS_EXECUTABLE"], str(self.binary))
        self.assertEqual(status_env["HGS_STATE_DIR"], self.command.argv[6])
        self.assertEqual(status_env["HGS_CONFIG_DIR"], self.command.argv[4])
        self.assertFalse(self.command.status["lifecycle"]["background"]["attention"]["ready"])

    def test_gui_only_install_retains_an_unchanged_active_backend(self):
        self.existing(); self.command.loaded = True
        self.assertEqual(self.install(), "ready")
        self.assertEqual(self.command.mutations(), [])

    def test_backend_upgrade_reloads_only_the_owned_desktop_label(self):
        self.existing(); self.command.loaded = True
        self.assertEqual(self.install(restart=True), "ready")
        domain = f"gui/{os.getuid()}"
        self.assertEqual(self.command.mutations(), [
            ["launchctl", "bootout", domain + "/" + installer.LABEL],
            ["launchctl", "bootstrap", domain, str(self.command.destination)]])

    def test_custom_definition_refuses_before_launchd_and_keeps_all_bytes(self):
        original = self.command.plan["definition"]
        for content in (original.replace(installer.MARKER, "Unmanaged definition"),
                        original.replace(self.command.config, "b" * 64),
                        original.replace(self.command.config, self.command.config + "b"),
                        original.replace("<string>" + installer.LABEL, "<string>com.example.foreign")):
            path = self.existing(content)
            with self.subTest(content=content), self.assertRaisesRegex(installer.Refusal, "custom"):
                self.install(restart=True)
            self.assertEqual(path.read_text(), content)
        self.assertFalse(any(argv[0] == "launchctl" for argv, _ in self.command.calls))

    def test_owned_old_cli_path_is_preflighted_and_replaced_using_its_loaded_argv(self):
        old_argv = list(self.command.argv)
        old_argv[0] = str(self.root / "source-install/hgs")  # The old binary need not still exist.
        old = self.command.plan["definition"].replace(self.command.argv[0], old_argv[0])
        path = self.existing(old)
        self.command.loaded = True; self.command.printed_argv = old_argv
        self.assertEqual(self.install(check_only=True), "checked")
        self.assertEqual(path.read_text(), old)
        self.assertEqual(self.command.mutations(), [])
        inode = path.stat().st_ino
        self.assertEqual(self.install(), "ready")
        self.assertEqual(path.read_text(), self.command.plan["definition"])
        self.assertNotEqual(path.stat().st_ino, inode)
        self.assertEqual(self.command.printed_argv, self.command.argv)
        self.assertEqual([args[1] for args in self.command.mutations()], ["bootout", "bootstrap"])

    def test_owned_definition_drift_can_be_upgraded_or_removed(self):
        original = self.command.plan["definition"]
        old = original.replace("<integer>10</integer>", "<integer>30</integer>")
        prefix, suffix = old.rsplit("</dict>", 1)
        old = prefix + "<key>ProcessType</key><string>Background</string></dict>" + suffix
        for remove in (False, True):
            with self.subTest(remove=remove):
                path = self.existing(old); self.command.loaded = True
                self.assertEqual(self.install(remove=remove), "removed" if remove else "ready")
                self.assertEqual(path.exists(), not remove)
                if not remove:
                    self.assertEqual(path.read_text(), original)

    def test_owned_old_cli_path_can_be_removed_after_the_binary_has_moved(self):
        old_argv = list(self.command.argv); old_argv[0] = str(self.root / "removed-source/hgs")
        path = self.existing(self.command.plan["definition"].replace(self.command.argv[0], old_argv[0]))
        self.command.loaded = True; self.command.printed_argv = old_argv
        self.assertEqual(self.install(remove=True), "removed")
        self.assertFalse(path.exists())
        self.assertFalse(Path(old_argv[0]).exists())
        self.assertEqual([args[1] for args in self.command.mutations()], ["bootout"])

    def test_owned_marker_does_not_make_malformed_or_nonservice_arguments_safe(self):
        original = self.command.plan["definition"]
        for argv in ([], ["relative/hgs", "desktop", "serve"], ["/private/example/agent", "run", "session"]):
            definition = plistlib.loads(original)
            definition["ProgramArguments"] = argv
            content = plistlib.dumps(definition).decode().replace("?>", "?>\n<!-- " + installer.MARKER
                        + "; configuration=" + self.command.config + " -->", 1)
            path = self.existing(content)
            with self.subTest(argv=argv), self.assertRaisesRegex(installer.Refusal, "invalid arguments"):
                self.install(restart=True)
            self.assertEqual(path.read_text(), content)
        self.assertFalse(any(args[0] == "launchctl" for args, _ in self.command.calls))

    def test_loaded_old_definition_must_match_old_not_new_argv(self):
        old = self.command.plan["definition"].replace(self.command.argv[0], str(self.root / "old/hgs"))
        self.existing(old); self.command.loaded = True
        # The loaded argv matches the new CLI but not the owned definition on disk.
        with self.assertRaisesRegex(installer.Refusal, "Loaded"):
            self.install()
        self.assertEqual(self.command.mutations(), [])

    def test_owned_reload_keeps_unknown_process_group_protection(self):
        old = self.command.plan["definition"].replace("<key>AbandonProcessGroup</key>\n\t<true/>",
                                                                  "<key>AbandonProcessGroup</key>\n\t<false/>")
        self.assertNotEqual(old, self.command.plan["definition"])
        path = self.existing(old); self.command.loaded = True
        with self.assertRaisesRegex(installer.Refusal, "process group"):
            self.install(restart=True)
        self.assertEqual(path.read_text(), old)
        self.assertEqual(self.command.mutations(), [])

    def test_symlink_and_actual_fifo_definition_refuse_before_open(self):
        path = self.command.destination; path.parent.mkdir(parents=True)
        foreign = self.root / "foreign"; foreign.write_text("keep")
        path.symlink_to(foreign)
        with self.assertRaises(installer.Refusal): self.install()
        path.unlink(); os.mkfifo(path)
        with self.assertRaises(installer.Refusal): self.install()
        self.assertEqual(foreign.read_text(), "keep")
        self.assertEqual(self.command.mutations(), [])

    def test_linked_directory_spine_refuses_before_cli_or_foreign_reads(self):
        foreign = self.root / "foreign-directory"; foreign.mkdir()
        (self.home / "Library").symlink_to(foreign, target_is_directory=True)
        with self.assertRaises(installer.Refusal): self.install()
        self.assertEqual(self.command.calls, [])
        self.assertEqual(list(foreign.iterdir()), [])

    def test_loaded_foreign_path_or_arguments_are_never_unloaded(self):
        self.existing(); self.command.loaded = True
        for path, argv in [("/private/example/custom.plist", self.command.argv),
                           (str(self.command.destination), [*self.command.argv[:-1], "--custom"])]:
            self.command.printed_path, self.command.printed_argv = path, argv
            with self.assertRaisesRegex(installer.Refusal, "Loaded"):
                self.install(restart=True)
        self.assertEqual(self.command.mutations(), [])

    def test_check_only_creates_no_directories_and_loads_nothing(self):
        self.assertEqual(self.install(check_only=True), "checked")
        self.assertFalse((self.home / "Library").exists())
        self.assertEqual(self.command.mutations(), [])

    def test_replaced_definition_is_kept_even_when_its_bytes_match(self):
        path = self.existing()
        inode = path.stat().st_ino
        def replace(argv, **kwargs):
            result = self.command(argv, **kwargs)
            if argv == ["launchctl", "print", f"gui/{os.getuid()}"]:
                temporary = path.with_suffix(".replacement")
                temporary.write_bytes(path.read_bytes()); os.replace(temporary, path)
            return result
        with self.assertRaisesRegex(installer.Refusal, "changed during preflight"):
            installer.install(self.binary, self.home, {}, command=replace, restart=True)
        self.assertNotEqual(path.stat().st_ino, inode)
        self.assertEqual(self.command.mutations(), [])

    def test_missing_aqua_session_is_deferred_without_a_readiness_claim(self):
        self.command.gui = False
        self.assertEqual(self.install(), "deferred")
        self.assertTrue(self.command.destination.exists())
        self.assertEqual(self.command.mutations(), [])
        self.assertFalse(any(argv[1:3] == ["desktop", "status"] for argv, _ in self.command.calls))

    def test_mismatched_abi_config_or_actor_refuses_readiness(self):
        for field, value in [("abi", "unknown"), ("configuration", "b" * 64), ("pid", 9999)]:
            self.command.status["lifecycle"][field] = value
            with self.subTest(field=field), self.assertRaises(installer.Refusal): self.install()
            self.command.status["lifecycle"][field] = {"abi": installer.ABI, "configuration": self.command.config, "pid": self.command.pid}[field]

    def test_unavailable_ipc_is_bounded_and_never_reported_ready(self):
        self.command.status["lifecycle"]["ipc_ready"] = False
        ticks = iter(range(0, 100, 10))
        with self.assertRaisesRegex(installer.Refusal, "did not become ready"):
            self.install(clock=lambda: next(ticks), sleep=lambda _: None)

    def test_remove_touches_only_its_owned_definition(self):
        self.existing(); self.command.loaded = True
        other = self.command.destination.parent / "com.hgdev.hgs-recovery.plist"; other.write_text("keep")
        self.assertEqual(self.install(remove=True), "removed")
        self.assertFalse(self.command.destination.exists())
        self.assertEqual(other.read_text(), "keep")
        self.assertEqual(len(self.command.mutations()), 1)

    def test_registration_cannot_change_scope_or_probe_command(self):
        original = copy.deepcopy(self.command.plan)
        for field, value in [("destination", "/private/example/foreign.plist"),
                             ("readiness", {"argv": ["unselected", "command"]}),
                             ("session", {"kind": "system"})]:
            self.command.plan = copy.deepcopy(original); self.command.plan[field] = value
            with self.subTest(field=field), self.assertRaises(installer.Refusal): self.install()
        self.assertFalse((self.home / "Library").exists())

    def test_command_output_and_timeout_are_bounded_with_owned_child_cleanup(self):
        env = {"HOME": str(self.home), "PATH": "/usr/bin:/bin"}
        for key in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME",
                    "XDG_RUNTIME_DIR", "TMPDIR", "TMUX_TMPDIR"):
            directory = self.home / key.lower(); directory.mkdir()
            env[key] = str(directory)
        with self.assertRaisesRegex(installer.Refusal, "bound"):
            installer.run([sys.executable, "-c", "import sys; sys.stdout.write('x'*262144)"], env=env)
        with self.assertRaisesRegex(installer.Refusal, "timed out"):
            installer.run([sys.executable, "-c", "import time; time.sleep(10)"], env=env, timeout=0.05)

    def test_source_install_restarts_desktop_only_for_changed_cli_bytes(self):
        source = self.root / "source"; (source / "scripts").mkdir(parents=True)
        (source / "install.sh").write_bytes((ROOT / "install.sh").read_bytes())
        shim = source / "hgs_state.py"; shim.write_text("first shim")
        calls = self.root / "desktop-install.calls"
        workers = self.root / "worker-install.calls"
        (source / "scripts/install-macos-session-service.py").write_text("pass\n")
        agents = self.home / "Library/LaunchAgents"; agents.mkdir(parents=True)
        for label in ("com.hgdev.hgs-recovery", "com.hgdev.hgs-swarm", "com.hgdev.hgs-voice"):
            (agents / (label + ".plist")).write_text("existing worker definition")
        for name in ("recovery", "swarm", "voice"):
            (source / f"scripts/install-{name}-service.py").write_text(
                "import json, sys\nfrom pathlib import Path\nwith Path(" + repr(str(workers))
                + ").open('a') as log: log.write(json.dumps([" + repr(name) + ",sys.argv[2:]])+'\\n')\n")
        (source / "scripts/install-macos-desktop-service.py").write_text(
            "import sys\nfrom pathlib import Path\nwith Path(" + repr(str(calls))
            + ").open('a') as log: log.write(' '.join(sys.argv[2:])+'\\n')\n")
        tools = self.root / "tools"; tools.mkdir()
        for name, text in {
            "uname": "#!/bin/sh\necho Darwin\n",
            "cargo": "#!/bin/sh\nmkdir -p \"$CARGO_TARGET_DIR/release\"\n"
                     "printf '#!/bin/sh\\necho %s\\n' \"$FIXTURE_REVISION\" > \"$CARGO_TARGET_DIR/release/hgs\"\n"
                     "chmod 755 \"$CARGO_TARGET_DIR/release/hgs\"\n",
        }.items():
            path = tools / name; path.write_text(text); path.chmod(0o755)
        env = {"HOME": str(self.home), "PATH": str(tools) + ":/usr/bin:/bin",
               "CARGO_TARGET_DIR": str(self.root / "target"), "HGS_MOBILE_CONNECTOR": "0",
               "PYTHONDONTWRITEBYTECODE": "1"}
        for key in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME",
                    "XDG_RUNTIME_DIR", "TMPDIR", "TMUX_TMPDIR"):
            directory = self.home / key.lower(); directory.mkdir()
            env[key] = str(directory)
        for revision, content in [("one", "first shim"), ("one", "first shim"),
                                  ("one", "second shim"), ("two", "second shim")]:
            shim.write_text(content)
            result = subprocess.run(["bash", str(source / "install.sh")],
                env=dict(env, FIXTURE_REVISION=revision), capture_output=True, text=True, timeout=20)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(calls.read_text().splitlines(), ["--restart", "", "", "--restart"])
        self.assertEqual([json.loads(line) for line in workers.read_text().splitlines()],
                         [[name, args] for args in (["--restart"], [], ["--restart"], ["--restart"])
                          for name in ("recovery", "swarm", "voice")])
        self.assertTrue(all(path.read_text() == "existing worker definition" for path in agents.iterdir()))


if __name__ == "__main__":
    unittest.main()
