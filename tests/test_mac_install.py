"""Installer configuration checks with fake transports and an isolated home."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[1]


class MacInstall(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='zerus-install-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        self.home = self.root / 'home'
        (self.home / '.config/hgs').mkdir(parents=True)
        (self.home / '.local/bin').mkdir(parents=True)
        self.script(self.home / '.local/bin/hgs', '#!/bin/sh\necho fixture\n')
        self.script(self.bin / 'ssh', '''#!/usr/bin/env python3
import os, pathlib, shlex, subprocess, sys
assert sys.argv[1:3] == ['-o', 'BatchMode=yes'], sys.argv
data = sys.stdin.buffer.read()
with open(os.environ['TRANSPORT_LOG'], 'a') as log:
    log.write('ssh\\n')
command = sys.argv[-1]
if command.startswith('bash -s -- '):
    sys.exit(subprocess.run(shlex.split(command), input=data).returncode)
''')
        self.script(self.bin / 'scp', '#!/bin/sh\n[ "$1" = -o ] && [ "$2" = BatchMode=yes ]\n')
        self.script(self.bin / 'tar', '#!/bin/sh\nexit 0\n')
        self.script(self.bin / 'tmux', '#!/bin/sh\necho fixture\n')
        self.script(self.bin / 'hostname', '#!/bin/sh\necho fixture-mac\n')
        self.env = {k: v for k, v in os.environ.items() if not k.startswith('HGS_MAC_')}
        self.env.update(HOME=str(self.home), PATH=str(self.bin) + os.pathsep + os.environ['PATH'],
                        TRANSPORT_LOG=str(self.root / 'transport.log'))

    def script(self, path, text):
        path.write_text(text)
        path.chmod(0o755)

    def run_install(self, *args, **settings):
        return subprocess.run(['bash', str(REPO / 'mac-install.sh'), *args],
                              env=dict(self.env, **settings), input='', text=True,
                              capture_output=True, timeout=15)

    def config(self):
        return (self.home / '.config/hgs/config').read_text()

    def test_destination_required_before_any_connection(self):
        result = self.run_install()
        self.assertEqual(result.returncode, 2)
        self.assertFalse((self.root / 'transport.log').exists())

    def test_default_does_not_create_ssh_or_reverse_peer(self):
        result = self.run_install('fixture-mac', '--no-tray')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('HGS_PEERS=""', self.config())
        self.assertIn('HGS_TAB_COLORS=""', self.config())
        self.assertFalse((self.home / '.ssh').exists())

    def test_existing_alias_does_not_rewrite_ssh(self):
        ssh = self.home / '.ssh/config'
        ssh.parent.mkdir()
        original = 'Host workstation\n  HostName workstation.example\n'
        ssh.write_text(original)
        result = self.run_install('fixture-mac', '--no-tray', HGS_MAC_PEER='workstation')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(ssh.read_text(), original)
        self.assertIn('HGS_PEERS="workstation"', self.config())

    def test_explicit_new_peer_is_idempotent(self):
        settings = dict(HGS_MAC_PEER='workstation', HGS_MAC_PEER_HOST='workstation.example',
                        HGS_MAC_PEER_USER='example', HGS_MAC_PEER_KEY='~/.ssh/example_key')
        for _ in range(2):
            result = self.run_install('fixture-mac', '--no-tray', **settings)
            self.assertEqual(result.returncode, 0, result.stderr)
        ssh = self.home / '.ssh/config'
        self.assertEqual(ssh.read_text().count('Host workstation\n'), 1)
        self.assertIn('  IdentityFile ~/.ssh/example_key\n', ssh.read_text())
        self.assertEqual(ssh.stat().st_mode & 0o777, 0o600)

    def test_existing_application_config_is_preserved(self):
        original = 'HGS_SELF="custom"\nHGS_PEERS="existing"\nHGS_TAB=0\nHGS_TAB_COLORS="custom=#123456"\n'
        (self.home / '.config/hgs/config').write_text(original)
        result = self.run_install('fixture-mac', '--no-tray')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.config(), original)

    def test_incomplete_or_injected_peer_config_never_connects(self):
        for settings in [dict(HGS_MAC_PEER_HOST='workstation.example'),
                         dict(HGS_MAC_PEER='workstation', HGS_MAC_PEER_USER='example'),
                         dict(HGS_MAC_PEER='workstation\nHost injected')]:
            with self.subTest(settings=settings):
                self.assertEqual(self.run_install('fixture-mac', '--no-tray', **settings).returncode, 2)
        self.assertFalse((self.root / 'transport.log').exists())


if __name__ == '__main__':
    unittest.main()
