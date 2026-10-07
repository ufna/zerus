#!/usr/bin/env python3
"""Check tracked/new source files without printing matching secret values.

This is a portable-path guard, not a replacement for Gitleaks or manual review.
Third-party copyright/author attribution is intentionally preserved.
"""
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SYNTHETIC_USERS = {'example', 'user', 'test', 'test user', 'remote', 'alice', 'bob'}
HOME_PATH = re.compile(r'/(?:Users|home)/([^/"\'`\n<>]+)(?:/|["\'`])')
PRIVATE_KEY = re.compile(rb'-----BEGIN (?:[A-Z0-9]+ )?PRIVATE KEY-----')
EMAIL = re.compile(r'(?<![\w.+-])[\w.+-]+@(?:[\w-]+\.)+[A-Za-z]{2,}')


def main():
    paths = subprocess.check_output(
        ['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=ROOT
    ).decode().split('\0')
    problems = []
    checked = 0
    for name in sorted(set(filter(None, paths))):
        path = ROOT / name
        if not path.is_file():
            continue
        checked += 1
        if path.name in {'.env', 'id_rsa', 'id_ed25519', 'credentials.json', 'auth.json'}:
            problems.append((name, 0, 'credential filename'))
        data = path.read_bytes()
        if PRIVATE_KEY.search(data):
            problems.append((name, 0, 'private key material'))
        # Compressed image bytes can accidentally resemble paths/email addresses.
        # Binary assets need a separate metadata/visual audit.
        if b'\0' in data:
            continue
        for line_no, line in enumerate(data.decode('utf-8', errors='replace').splitlines(), 1):
            for match in HOME_PATH.finditer(line):
                if match[1] not in SYNTHETIC_USERS:
                    problems.append((name, line_no, 'non-example absolute home path'))
            if re.search(r'(?<![A-Za-z0-9])/' + 'w/', line):
                problems.append((name, line_no, 'absolute workspace path'))
            if not name.startswith('tray/vendor/'):
                for address in EMAIL.findall(line):
                    domain = address.rsplit('@', 1)[1].lower()
                    if address == 'git@github.com' or domain.endswith(('.png', '.svg', '.icns')):
                        continue
                    if not (domain in {'example.com', 'example.org', 'example.net', 'example.test',
                                       'users.noreply.github.com'} or domain.endswith(('.example', '.test', '.invalid'))):
                        problems.append((name, line_no, 'non-example email address'))
    for name, line, reason in sorted(set(problems)):
        print(f'{name}:{line}: {reason}')
    print(f'Checked {checked} source files; {len(set(problems))} privacy guard findings.')
    return bool(problems)


if __name__ == '__main__':
    sys.exit(main())
