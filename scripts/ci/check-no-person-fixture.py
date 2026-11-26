#!/usr/bin/env python3
"""The Teams contracts' person fixture never ships (zerus-63rru.182).

The hosted real-agent Teams contracts perform the person's create, stop and task
intake by the ignored contract entry of the hgs unit-test executable, which runs the
native Person action with a modeled window proof. That entry and the models it uses
are test-only (`#[cfg(all(test, unix))]`). This check fails unless:

- every --absent executable (the debug and the release hgs) holds neither the
  entry's marker nor its name, and, where it keeps a symbol table, no symbol of the
  test-only models; and
- the --present executable (the hgs unit-test executable) holds the marker and the
  entry, so the absence above cannot pass by looking for the wrong thing.

The marker is read from the Rust source, so renaming it there cannot make the
check pass vacuously. Release executables are stripped: the marker and the
entry's name, both strings, are their check; the symbols are the debug build's.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'src/desktop/teams/person_actions/contract.rs'
ENTRY = 'person_actions::contract::contract_person_step'
# Symbols of the test-only fixtures, as `nm -C` prints them.
SYMBOLS = ('person_actions::contract::', 'person_actions::models::', 'ui_authority::Validated::fixture',
           'kernel::Permit::fixture', 'original::Admission::fixture')
CHUNK = 1 << 20


def marker(source=SOURCE):
    """The entry's marker, as the Rust source declares it."""
    found = re.findall(r'pub\(crate\) const MARKER: &str = "([^"]+)";', source.read_text())
    if len(found) != 1:
        raise ValueError(f'{source} does not declare exactly one MARKER')
    return found[0].encode()


def holds(path, needles):
    """The needles that occur in the file at `path`, read in bounded chunks."""
    found, tail = set(), b''
    overlap = max(len(n) for n in needles) - 1
    with open(path, 'rb') as stream:
        while chunk := stream.read(CHUNK):
            window = tail + chunk
            found.update(n for n in needles if n in window)
            tail = window[-overlap:] if overlap else b''
    return found


def symbols(path, nm):
    """The demangled symbol table, or None when the file keeps none."""
    try:
        run = subprocess.run([nm, '-C', str(path)], capture_output=True, text=True, timeout=600)
    except (OSError, subprocess.TimeoutExpired):
        return None
    if run.returncode != 0 or not run.stdout.strip():
        return None
    return run.stdout.splitlines()


def check(absent, present, nm='nm', source=SOURCE):
    mark = marker(source)
    entry = ENTRY.encode()
    report, failures = dict(marker=mark.decode(), absent={}, present={}), []
    for path in absent:
        found = holds(path, (mark, entry))
        table = symbols(path, nm)
        named = sorted({s for line in table or () for s in SYMBOLS if s in line})
        report['absent'][str(path)] = dict(strings=sorted(f.decode() for f in found),
                                           symbols_checked=table is not None, symbols=named)
        if found or named:
            failures.append(f'{path} holds the test-only person fixture: {sorted(f.decode() for f in found) + named}')
    found = holds(present, (mark, entry))
    table = symbols(present, nm)
    report['present'] = {str(present): dict(strings=sorted(f.decode() for f in found), symbols_checked=table is not None,
                                            entry_symbol=table is not None and any(ENTRY in line for line in table))}
    if found != {mark, entry}:
        failures.append(f'{present} lacks the marker or the entry; the absence check would prove nothing')
    if table is not None and not any(ENTRY in line for line in table):
        failures.append(f'{present} keeps symbols but not the entry; the symbol check would prove nothing')
    report['failures'] = failures
    return report


def unit_tests(messages, root=ROOT):
    """The one hgs unit-test executable in cargo's JSON messages (`cargo test --no-run
    --bin hgs --message-format=json`)."""
    raw = messages.read_bytes()
    if len(raw) > 16 << 20:
        raise ValueError('cargo messages exceed their bound')
    source = (root / 'src/main.rs').resolve()
    found = set()
    for line in raw.splitlines():
        try:
            item = json.loads(line)
        except ValueError:
            continue
        if (item.get('reason') == 'compiler-artifact' and item.get('executable')
                and item.get('target', {}).get('name') == 'hgs' and item.get('profile', {}).get('test')
                and Path(item['target'].get('src_path', '')).resolve() == source):
            found.add(Path(item['executable']))
    if len(found) != 1:
        raise ValueError(f'expected one hgs unit-test executable, found {len(found)}')
    return found.pop()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--absent', type=Path, nargs='+', help='shipped-kind executables (debug, release hgs)')
    present = parser.add_mutually_exclusive_group(required=True)
    present.add_argument('--present', type=Path, help='the hgs unit-test executable')
    present.add_argument('--cargo-messages', type=Path, help="cargo's JSON messages naming the hgs unit-test executable")
    parser.add_argument('--print-unit-tests', action='store_true',
                        help="print the unit-test executable that --cargo-messages names, and check nothing")
    parser.add_argument('--nm', default='nm')
    args = parser.parse_args(argv)
    if args.print_unit_tests:
        if not args.cargo_messages or args.absent:
            parser.error('--print-unit-tests takes only --cargo-messages')
        print(unit_tests(args.cargo_messages))
        return 0
    if not args.absent:
        parser.error('--absent names the executables that must not hold the fixture')
    report = check(args.absent, args.present or unit_tests(args.cargo_messages), args.nm)
    print(json.dumps(report, indent=2))
    return 1 if report['failures'] else 0


if __name__ == '__main__':
    sys.exit(main())
