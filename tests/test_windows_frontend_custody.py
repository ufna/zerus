"""Portable controls for original-child custody in the Windows native fixture."""
import subprocess
import tempfile
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import windows_frontend_auth as native_fixture


class WindowsFrontendCustody(unittest.TestCase):
    def child(self, *, exited=False, timeout=False):
        events = []
        process = SimpleNamespace(returncode=78 if exited else None)
        handle = SimpleNamespace(closed=False)

        def poll():
            return process.returncode

        def terminate():
            events.append('original_terminate')

        def communicate(*, timeout):
            self.assertEqual(timeout, 8)
            events.append('original_pipe_join')
            if must_timeout:
                raise subprocess.TimeoutExpired('original fixture child', timeout)
            process.returncode = 78 if exited else 0
            return '', ''

        def close():
            self.assertIsNotNone(process.returncode)
            events.append('original_handle_close')
            handle.closed = True

        must_timeout = timeout
        process.poll, process.terminate, process.communicate = poll, terminate, communicate
        handle.Close = close
        process._handle = handle
        return process, events

    def test_already_exited_original_still_drains_pipes_and_closes_its_handle(self):
        process, events = self.child(exited=True)
        native_fixture.WindowsFrontend.stop(self, process)
        self.assertEqual(events, ['original_pipe_join', 'original_handle_close'])
        self.assertTrue(process._handle.closed)

    def test_live_original_is_joined_before_its_handle_closes(self):
        process, events = self.child()
        native_fixture.WindowsFrontend.stop(self, process)
        self.assertEqual(events, ['original_terminate', 'original_pipe_join', 'original_handle_close'])

    def test_uncertain_original_pipe_join_retains_the_original_handle(self):
        process, events = self.child(timeout=True)
        with self.assertRaises(subprocess.TimeoutExpired):
            native_fixture.WindowsFrontend.stop(self, process)
        self.assertEqual(events, ['original_terminate', 'original_pipe_join'])
        self.assertFalse(process._handle.closed)
        self.assertFalse(getattr(process, '_fixture_settled', False))

    def test_repeated_cleanup_does_not_signal_or_join_a_settled_original_again(self):
        process, events = self.child()
        native_fixture.WindowsFrontend.stop(self, process)
        before = events.copy()
        native_fixture.WindowsFrontend.stop(self, process)
        self.assertEqual(events, before)


class WindowsTaskModeCleanup(unittest.TestCase):
    def fixture(self, *, registered=True, state=3, instances=(), exited=True):
        fixture = native_fixture.WindowsTaskMode('test_explicit_mode_round_trip_uses_the_own_task_and_preserves_unrelated_processes')
        root = tempfile.TemporaryDirectory(prefix='task-cleanup-control-')
        self.addCleanup(root.cleanup)
        fixture.root = Path(root.name)
        fixture.task, fixture.installed = 'private-fixture-task', registered
        fixture.held_originals = []
        calls = []

        def cli(*args):
            calls.append(args)
            if args == ('desktop', 'registration', 'status', '--json'):
                return dict(registered=registered, task_name=fixture.task)
            if args == ('desktop', 'status', '--json'):
                return dict(running=False)
            self.assertEqual(args, ('desktop', 'registration', 'remove', '--json'))
            return dict(registered=False)

        fixture.cli = cli
        fixture.observe = lambda: dict(state=state, instances=list(instances))
        fixture.held_originals = [(SimpleNamespace(CloseHandle=lambda _: calls.append('close')),
                                   object(), dict(pid=123, birth=456))]
        self.addCleanup(setattr, native_fixture.WindowsTaskMode, 'unsettled_fixture', None)
        metadata = dict(pid=123, birth=456, exited=exited)
        return fixture, calls, metadata

    def test_failed_row_can_remove_only_a_proven_idle_owned_registration(self):
        fixture, calls, metadata = self.fixture()
        with patch.object(native_fixture, 'windows_process_metadata', return_value=metadata):
            fixture.cleanup_registration()
        self.assertIn(('desktop', 'registration', 'remove', '--json'), calls)
        self.assertEqual(calls[-1], 'close')
        self.assertFalse(fixture.root.exists())
        self.assertIsNone(native_fixture.WindowsTaskMode.unsettled_fixture)

    def test_live_unknown_or_replaced_instances_leave_task_and_root_for_review(self):
        for state, instances, exited in [(4, (), True), (0, (), True), (2, (), True),
                                         (3, ('replacement',), True), (3, (), False)]:
            with self.subTest(state=state, instances=instances, exited=exited):
                fixture, calls, metadata = self.fixture(state=state, instances=instances, exited=exited)
                with patch.object(native_fixture, 'windows_process_metadata', return_value=metadata):
                    with self.assertRaises(AssertionError):
                        fixture.cleanup_registration()
                self.assertNotIn(('desktop', 'registration', 'remove', '--json'), calls)
                self.assertTrue(fixture.root.exists())
                # The next row must not attempt a different private profile
                # against the retained machine-level task.
                later = native_fixture.WindowsTaskMode('test_original_instance_refresh_keeps_its_grant_mode_and_original_ids')
                with patch.object(native_fixture.tempfile, 'mkdtemp', side_effect=AssertionError('no new root')):
                    with self.assertRaisesRegex(unittest.SkipTest, 'NOT RUN: a preceding task fixture'):
                        later.setUp()
                native_fixture.WindowsTaskMode.unsettled_fixture = None

    def test_refused_definition_read_never_falls_back_to_task_name_deletion(self):
        fixture, calls, _ = self.fixture()
        fixture.cli = lambda *args: (_ for _ in ()).throw(AssertionError('owned definition changed'))
        with self.assertRaisesRegex(AssertionError, 'owned definition changed'):
            fixture.cleanup_registration()
        self.assertEqual(calls, [])
        self.assertTrue(fixture.root.exists())

    def test_removed_registration_does_not_prove_a_held_process_exited(self):
        fixture, calls, metadata = self.fixture(registered=False, exited=False)
        with patch.object(native_fixture, 'windows_process_metadata', return_value=metadata):
            with self.assertRaises(AssertionError):
                fixture.cleanup_registration()
        self.assertTrue(fixture.root.exists())
        self.assertNotIn('close', calls)

    def test_changed_kernel_identity_refuses_before_removal(self):
        fixture, calls, metadata = self.fixture()
        with patch.object(native_fixture, 'windows_process_metadata', return_value=dict(metadata, birth=457)):
            with self.assertRaises(AssertionError):
                fixture.cleanup_registration()
        self.assertNotIn(('desktop', 'registration', 'remove', '--json'), calls)
        self.assertTrue(fixture.root.exists())

    def test_changed_activity_during_readback_refuses_removal(self):
        fixture, calls, metadata = self.fixture()
        observations = iter([dict(state=3, instances=[]), dict(state=4, instances=[])])
        fixture.observe = lambda: next(observations)
        with patch.object(native_fixture, 'windows_process_metadata', return_value=metadata):
            with self.assertRaises(AssertionError):
                fixture.cleanup_registration()
        self.assertNotIn(('desktop', 'registration', 'remove', '--json'), calls)
        self.assertTrue(fixture.root.exists())


if __name__ == '__main__':
    unittest.main()
