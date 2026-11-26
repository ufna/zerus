"""scripts/ci/check-no-person-fixture.py (zerus-63rru.182): a shipped-kind executable
holding the test-only person fixture fails the job, and a check that looks for the
wrong thing cannot pass. Executables are synthetic files; `nm` is a stand-in that
prints a given symbol table."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('check_no_person_fixture', ROOT / 'scripts/ci/check-no-person-fixture.py')
check = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(check)


class CheckNoPersonFixture(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='hgs-person-check-')
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.marker = check.marker()
        self.tables = {}
        self.nm = self.base / 'nm'
        self.nm.write_text('#!/bin/sh\ntable="$1.nm"\n[ "$1" = -C ] && table="$2.nm"\n'
                           '[ -f "$table" ] || exit 1\ncat "$table"\n')
        self.nm.chmod(0o755)

    def binary(self, name, content=b'', table=None):
        path = self.base / name
        # A large file puts the needle across a chunk boundary.
        path.write_bytes(b'\0' * (check.CHUNK - 7) + content + b'\0' * 64)
        if table is not None:
            Path(str(path) + '.nm').write_text('\n'.join(table) + '\n')
        return path

    def run_check(self, absent, present):
        return check.check(absent, present, nm=str(self.nm))['failures']

    def present(self):
        return self.binary('unit-tests', self.marker + b' ' + check.ENTRY.encode(),
                           ['0000 T hgs::desktop::teams::' + check.ENTRY])

    def test_the_marker_is_the_rust_sources_own(self):
        source = (ROOT / 'src/desktop/teams/person_actions/contract.rs').read_text()
        self.assertIn(f'pub(crate) const MARKER: &str = "{self.marker.decode()}";', source)
        renamed = self.base / 'contract.rs'
        renamed.write_text(source.replace('pub(crate) const MARKER', 'pub(crate) const OTHER'))
        with self.assertRaises(ValueError):
            check.marker(renamed)

    def test_clean_shipped_executables_pass(self):
        debug = self.binary('hgs', b'ordinary', ['0000 T hgs::main'])
        release = self.binary('hgs-release', b'ordinary')
        self.assertEqual(self.run_check([debug, release], self.present()), [])

    def test_a_shipped_executable_with_the_fixture_fails(self):
        for what, content, table in (
                ('the marker', self.marker, None),
                ('the entry name', check.ENTRY.encode(), None),
                ('a model symbol', b'', ['0000 T hgs::desktop::teams::person_actions::models::team_action']),
                ('the window proof model', b'', ['0000 T hgs::desktop::ui_authority::Validated::fixture']),
                ('the permit model', b'', ['0000 T hgs::desktop::teams::person_actions::kernel::Permit::fixture'])):
            shipped = self.binary('hgs-' + what.replace(' ', '-'), content, table)
            self.assertTrue(self.run_check([shipped], self.present()), what)

    def test_a_check_that_would_find_nothing_fails(self):
        clean = self.binary('hgs', b'ordinary')
        for what, content, table in (
                ('no marker', check.ENTRY.encode(), None),
                ('no entry', self.marker, None),
                ('symbols without the entry', self.marker + b' ' + check.ENTRY.encode(), ['0000 T hgs::main'])):
            present = self.binary('unit-' + what.replace(' ', '-'), content, table)
            self.assertTrue(self.run_check([clean], present), what)

    def test_the_unit_test_executable_is_the_one_cargo_names(self):
        main = str(ROOT / 'src/main.rs')
        def message(name, test, src=main, executable='/x/hgs-1'):
            return json.dumps(dict(reason='compiler-artifact', executable=executable, profile=dict(test=test),
                                   target=dict(name=name, src_path=src)))
        messages = self.base / 'messages.jsonl'
        messages.write_text('\n'.join(['not json', message('hgs', False, executable='/x/hgs'),
                                       message('hgs', True), message('fake_ssh', True, executable='/x/fake')]) + '\n')
        self.assertEqual(check.unit_tests(messages), Path('/x/hgs-1'))
        for lines in ([message('hgs', False)], [message('hgs', True), message('hgs', True, executable='/x/hgs-2')],
                      [message('hgs', True, src='/elsewhere/main.rs')]):
            messages.write_text('\n'.join(lines) + '\n')
            with self.assertRaises(ValueError):
                check.unit_tests(messages)

    def test_the_command_fails_the_job(self):
        shipped = self.binary('hgs', self.marker)
        code = check.main(['--absent', str(shipped), '--present', str(self.present()), '--nm', str(self.nm)])
        self.assertEqual(code, 1)
        clean = self.binary('hgs-clean', b'ordinary')
        self.assertEqual(check.main(['--absent', str(clean), '--present', str(self.present()), '--nm', str(self.nm)]), 0)


if __name__ == '__main__':
    unittest.main()
