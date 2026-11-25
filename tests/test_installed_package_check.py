"""Installed-package fixtures must not inherit another profile's XDG roots."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('installed_package_check', ROOT / 'scripts/check-installed-package.py')
PACKAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKAGE)


class PrivateProfile(unittest.TestCase):
    def test_inherited_xdg_and_fake_manager_stay_in_the_same_private_home(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); home = root / 'home'; home.mkdir(mode=0o700)
            foreign = root / 'foreign'; foreign.mkdir(mode=0o700)
            marker = foreign / 'keep'; marker.write_bytes(b'unchanged')
            sockets = home / 'tmux'; sockets.mkdir(mode=0o700)
            inherited = dict(os.environ, XDG_CONFIG_HOME=str(foreign), XDG_STATE_HOME=str(foreign),
                             HGS_STATE_DIR='/owner', ZERUS_CLIENT_DESKTOP_SERVICE='1')
            env = PACKAGE.isolated_environment(home, sockets, inherited)
            env.update(FIXTURE_VENV=str(home/'venv'), FIXTURE_CONFIG=str(home/'connector.json'))
            systemctl = home / 'fake-systemctl.py'; systemctl.write_text(PACKAGE.SYSTEMCTL)
            selected = Path(env['XDG_CONFIG_HOME']) / 'systemd/user/zerus-mobile-connector.service.d'
            selected.mkdir(parents=True)
            (selected/'50-zerus-killmode.conf').write_text('[Service]\nKillMode=process\n')
            subprocess.run([sys.executable, str(systemctl), '--user', 'daemon-reload'], env=env, check=True)
            self.assertEqual(json.loads((home/'loaded.json').read_text())['KillMode'], 'process')
            self.assertEqual(list(foreign.iterdir()), [marker])
            self.assertEqual(marker.read_bytes(), b'unchanged')
            self.assertNotIn('ZERUS_CLIENT_DESKTOP_SERVICE', env)
            self.assertEqual(env['HGS_STATE_DIR'], str(home/'state'))
            for key in ('XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_STATE_HOME', 'XDG_CACHE_HOME', 'XDG_RUNTIME_DIR'):
                self.assertTrue(Path(env[key]).is_relative_to(home))
                self.assertEqual(Path(env[key]).stat().st_mode & 0o777, 0o700)


class ListingBootstrap(unittest.TestCase):
    def test_publication_refusals_wait_for_the_complete_body(self):
        rows = [dict(name='sh/project/package-fixture', cwd='/srv/example')]
        messages = (
            'hgs: Missing private file',
            'hgs: Session listing is building, stale or unavailable; no complete body returned',
            'hgs: Session listing changed before publication; no complete body returned',
            'hgs: The session listing index is still being built. Try again shortly.',
            'hgs: Listing metadata is still rebuilding',
            'hgs: The session listing is busy. Try again shortly.',
        )
        replies = [subprocess.CompletedProcess([], 1, '', message + '\n') for message in messages]
        replies.append(subprocess.CompletedProcess([], 0, json.dumps(dict(sessions=rows)), ''))
        with patch.object(PACKAGE.subprocess, 'run', side_effect=replies) as run, \
             patch.object(PACKAGE.time, 'sleep'):
            self.assertEqual(PACKAGE.listed_sessions('/private/hgs', {'HOME': '/private'}), rows)
        self.assertEqual(run.call_count, len(replies))
        self.assertTrue(all(call.args[0] == ['/private/hgs', 'ls', '--local', '--json']
                            for call in run.call_args_list))

    def test_unknown_refusal_partial_body_and_different_exit_are_not_retried(self):
        for code, body, error in [(1, '', 'hgs: corrupt record'),
                                  (1, '', 'hgs: The session listing index cannot be read. Rebuild it and try again.'),
                                  (1, '{"sessions":', 'hgs: The session listing is busy. Try again shortly.'),
                                  (2, '', 'hgs: Missing private file')]:
            with self.subTest(code=code, body=body, error=error), \
                 patch.object(PACKAGE.subprocess, 'run', return_value=subprocess.CompletedProcess([], code, body, error)) as run:
                with self.assertRaises(subprocess.CalledProcessError):
                    PACKAGE.listed_sessions('/private/hgs', {})
                self.assertEqual(run.call_count, 1)

    def test_each_read_uses_only_the_original_remaining_deadline(self):
        clock = [0.0]
        limits = []
        def execute(*args, **kwargs):
            limits.append(kwargs['timeout'])
            clock[0] += .6
            return subprocess.CompletedProcess([], 1, '', 'hgs: Missing private file')
        with patch.object(PACKAGE.time, 'monotonic', side_effect=lambda: clock[0]), \
             patch.object(PACKAGE.time, 'sleep', side_effect=lambda n: clock.__setitem__(0, clock[0] + n)), \
             patch.object(PACKAGE.subprocess, 'run', side_effect=execute):
            with self.assertRaises(TimeoutError):
                PACKAGE.listed_sessions('/private/hgs', {}, timeout=1)
        self.assertEqual(len(limits), 2)
        self.assertAlmostEqual(limits[0], 1)
        self.assertAlmostEqual(limits[1], .3)

    def test_even_success_after_the_deadline_is_refused(self):
        with patch.object(PACKAGE.time, 'monotonic', side_effect=[0, 0, 1]), \
             patch.object(PACKAGE.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, '{"sessions": []}', '')):
            with self.assertRaises(TimeoutError):
                PACKAGE.listed_sessions('/private/hgs', {}, timeout=1)


if __name__ == '__main__':
    unittest.main()
