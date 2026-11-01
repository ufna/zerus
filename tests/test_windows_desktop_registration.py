"""Disposable Windows CI: actual own-user task, never start/stop a service."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


HGS = Path(sys.argv[sys.argv.index('--hgs') + 1]).resolve() if '--hgs' in sys.argv else None
if '--hgs' in sys.argv:
    index = sys.argv.index('--hgs')
    del sys.argv[index:index + 2]


@unittest.skipUnless(os.name == 'nt' and HGS is not None, 'explicit disposable Windows CLI required')
class WindowsDesktopRegistration(unittest.TestCase):
    def test_own_interactive_logon_task_without_starting_service_or_enabling_sessions(self):
        root = Path(tempfile.mkdtemp(prefix='zerus-desktop-logon-'))
        env = {key: os.environ[key] for key in ('SystemRoot', 'WINDIR', 'COMSPEC') if key in os.environ}
        env.update(HOME=str(root), USERPROFILE=str(root),
                   APPDATA=str(root / 'roaming'), LOCALAPPDATA=str(root / 'local'),
                   TEMP=str(root / 'tmp'), TMP=str(root / 'tmp'),
                   HGS_CONFIG_DIR=str(root / 'config'), HGS_STATE_DIR=str(root / 'state'),
                   HGS_SELF='windows-registration-fixture', HGS_PEERS='')
        for name in ('roaming', 'local', 'tmp', 'config'):
            (root / name).mkdir()

        def cli(*args):
            result = subprocess.run([str(HGS), *args], env=env, capture_output=True,
                                    text=True, encoding='utf-8', timeout=75)
            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            self.assertLessEqual(len(result.stdout.encode('utf-8')), 65536)
            return json.loads(result.stdout)

        def digest(path):
            self.assertLessEqual(path.stat().st_size, 256 * 1024 * 1024)
            with path.open('rb') as stream:
                return hashlib.file_digest(stream, 'sha256').digest()

        absent = False
        installed = False
        try:
            status = cli('desktop', 'registration', 'status', '--json')
            self.assertFalse(status['registered'], 'Keep pre-existing task; this runner is not disposable')
            absent = True
            installed = True
            absent = False
            first = cli('desktop', 'registration', 'install', '--json')
            self.assertTrue(first['registered'])
            self.assertTrue(first['changed'])
            self.assertFalse(first['started'])
            self.assertFalse(first['stopped'])
            self.assertEqual(first['task_state'], 3)  # TASK_STATE_READY, not queued/running.
            staged = root / '.local' / 'bin' / 'hgs.exe'
            self.assertEqual(digest(staged), digest(HGS))
            self.assertFalse((root / 'config' / 'local-sessions').exists())
            self.assertFalse(cli('desktop', 'status', '--json')['running'])
            again = cli('desktop', 'registration', 'install', '--json')
            self.assertFalse(again['changed'])
            self.assertFalse(again['cli']['changed'])
            self.assertEqual(again['task_state'], 3)
            removed = cli('desktop', 'registration', 'remove', '--json')
            self.assertTrue(removed['changed'])
            self.assertFalse(removed['registered'])
            self.assertFalse(removed['stopped'])
            self.assertFalse(cli('desktop', 'registration', 'status', '--json')['registered'])
            installed = False
            absent = True
            self.assertFalse(cli('desktop', 'registration', 'remove', '--json')['changed'])
            self.assertTrue(staged.exists())
        finally:
            # Only our exact native-validated task can be removed. Uncertainty
            # retains its source paths rather than leaving a dangling task.
            if installed:
                try:
                    current = cli('desktop', 'registration', 'status', '--json')
                    absent = not current['registered']
                    if not absent:
                        absent = not cli('desktop', 'registration', 'remove', '--json')['registered']
                except Exception:
                    absent = False
            if absent:
                shutil.rmtree(root)
            else:
                print('Registration cleanup uncertain; private root retained:', root, file=sys.stderr)


if __name__ == '__main__':
    unittest.main()
