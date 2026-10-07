#!/usr/bin/env python3
"""Install the same-user desktop security-session bridge (macOS only)."""
import argparse
import os
from pathlib import Path
import plistlib
import subprocess
import sys
import tempfile

LABEL = 'com.hgdev.hgs.user-session'


def definition(binary):
    return {
        'Label': LABEL,
        'ProgramArguments': [str(binary), '--macos-session-service'],
        'MachServices': {LABEL: True},
        'LimitLoadToSessionType': 'Aqua',
        'ProcessType': 'Background',
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('--restart', action='store_true')
    args = parser.parse_args()
    if sys.platform != 'darwin':
        return
    binary = args.binary.resolve(strict=True)
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise SystemExit('HGS session service requires an executable CLI.')
    directory = Path.home() / 'Library/LaunchAgents'
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / (LABEL + '.plist')
    payload = plistlib.dumps(definition(binary))
    changed = path.is_symlink() or not path.exists() or path.read_bytes() != payload
    if changed:
        with tempfile.NamedTemporaryFile(dir=directory, delete=False) as stage:
            temporary = Path(stage.name)
            try:
                stage.write(payload)
                stage.flush()
                os.fsync(stage.fileno())
                os.replace(temporary, path)
            finally:
                temporary.unlink(missing_ok=True)
    domain = f'gui/{os.getuid()}'

    def run(*arguments):
        return subprocess.run(['launchctl', *arguments], capture_output=True, text=True, timeout=15)

    if run('print', domain).returncode:
        print('Saved HGS desktop session service (available after macOS login).')
        return
    loaded = run('print', domain + '/' + LABEL).returncode == 0
    if loaded and (changed or args.restart):
        result = run('bootout', domain + '/' + LABEL)
        if result.returncode:
            raise SystemExit('Cannot reload HGS desktop session service: ' + result.stderr.strip())
        loaded = False
    if not loaded:
        result = run('bootstrap', domain, str(path))
        if result.returncode:
            raise SystemExit('Cannot load HGS desktop session service: ' + result.stderr.strip())
        print('Installed HGS desktop session service.')


if __name__ == '__main__':
    main()
