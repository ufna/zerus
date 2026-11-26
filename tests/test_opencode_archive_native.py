"""OpenCode Archive and Restore against a private official OpenCode service."""

import json
import os
import sqlite3
import subprocess

import test_opencode_context_native as fixture
import test_opencode_native as native


class OpenCodeArchive(fixture.OpenCodeContext):
    def stop_and_check(self):
        # The ordinary listing starts a resident product worker; use its
        # public switch on this private root before checking process custody.
        self.hgs(['listing-index', 'disable', '--json'])
        super().stop_and_check()

    def test_archive_idle_after_interrupt_and_restore_original(self):
        first = self.start()
        name = first['name']
        peer = self.create()
        self.turn(first, 'Original conversation before archive')
        self.turn(peer, 'Separate shared-server conversation')
        self.hgs(['send', name, '--json'], self.payload(first, text='HOLD_NATIVE'))
        self.assertTrue(self.model.holding.wait(15))
        active = self.wait(name, lambda row: row['interrupt_supported'])
        self.assertNotIn('archive', active['allowed_actions'])
        self.action(active, 'archive', status='failed')
        self.hgs(['interrupt', name, '--json'],
                 self.payload(active, expected_turn_started=active['turn_started']))
        self.model.release.set()
        ready = self.wait(name, lambda row: row['phase'] == 'idle')
        self.assertIn('archive', ready['allowed_actions'], ready['action_reasons'])
        self.assertIn('forget', ready['allowed_actions'])
        registration = self.registration_path().read_bytes()
        peer_messages = self.user_messages(peer['conversation_id'])
        original = self.user_messages(ready['conversation_id'])

        archive_request = self.payload(ready, action='archive')
        # The original command runs while its reply is lost to the caller.
        cmd = subprocess.run([str(native.HGS), 'session-action', name, '--json'],
                             env=self.env, cwd=self.workspace, input=json.dumps(archive_request),
                             text=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=15)
        self.assertEqual(cmd.returncode, 0, cmd.stderr)
        saved = self.hgs(['session-action-status', name, '--json'], archive_request)
        self.assertEqual(saved['status'], 'completed', saved)
        archive_id = saved['result_target']['archive_id']
        self.assertEqual(self.hgs(['session-action', name, '--json'], archive_request)['result_target'],
                         saved['result_target'])
        self.assertNotIn(name, self.listing())
        self.assertIn(peer['name'], self.listing())
        archived = self.hgs(['inspect', name, '--archive', archive_id])
        self.assertEqual(archived['state'], 'archived')
        self.assertEqual(archived['conversation_id'], ready['conversation_id'])
        self.assertEqual(archived['session_id'], ready['session_id'])
        self.assertIn('restore', archived['allowed_actions'])
        self.assertTrue(any(event['type'] == 'UserPromptSubmit' and
                            event['detail'] == 'Original conversation before archive'
                            for event in archived['events']))
        rows = self.hgs(['ls', '--local', '--json'])['sessions']
        self.assertEqual([(row['state'], row['archive_id']) for row in rows
                          if row['name'] == name], [('archived', archive_id)])

        restore_request = self.payload(archived, action='restore', archive_id=archive_id)
        restored = self.hgs(['session-action', name, '--json'], restore_request)
        self.assertEqual(restored['status'], 'completed', restored)
        self.assertEqual(restored, self.hgs(['session-action', name, '--json'], restore_request))
        row = self.hgs(['inspect', name])
        self.assertNotEqual(row['run_id'], ready['run_id'])
        self.assertEqual(row['conversation_id'], ready['conversation_id'])
        self.assertEqual(row['session_id'], ready['session_id'])
        self.assertEqual(self.user_messages(row['conversation_id']), original)
        self.assertEqual(self.user_messages(peer['conversation_id']), peer_messages)
        self.assertEqual(self.registration_path().read_bytes(), registration)
        self.assertEqual(self.hgs(['inspect', peer['name']])['conversation_id'], peer['conversation_id'])
        self.hgs(['inspect', name, '--archive', archive_id], success=False)
        self.assertFalse((self.state / 'archives' / (archive_id + '.json')).exists())
        self.hgs(['send', name, '--json'], self.payload(ready, text='Stale archived run'), success=False)
        self.turn(row, 'Exact original is writable after restore')

    def test_root_archive_refuses_exact_native_child_work(self):
        root = self.start()
        other = self.create()
        status, reply = self.api('POST', '/api/session', {
            'parentID': root['conversation_id'], 'title': 'Private native child',
            'location': {'directory': str(self.workspace)},
            'model': {'providerID': 'fixture', 'id': 'local'},
        })
        self.assertIn(status, (200, 201), reply)
        child = reply['data']['id']
        self.assertEqual(self.session(child)['parentID'], root['conversation_id'])
        status, reply = self.api('POST', f'/api/session/{child}/prompt', {
            'text': 'HOLD_NATIVE', 'delivery': 'queue',
        })
        self.assertIn(status, (200, 202), reply)
        self.assertTrue(self.model.holding.wait(15))
        status, active = self.api('GET', '/api/session/active')
        self.assertEqual(status, 200, active)
        self.assertIn(child, active['data'])
        self.assertNotIn(root['conversation_id'], active['data'])

        blocked = self.hgs(['inspect', root['name']])
        self.assertEqual(blocked['phase'], 'idle')
        self.assertIn('running child session', blocked['action_reasons']['archive'])
        self.assertNotIn('archive', blocked['allowed_actions'])
        self.assertNotIn('archive', self.listing()[root['name']]['allowed_actions'])
        self.action(root, 'archive', status='failed')
        self.assertEqual(self.hgs(['inspect', root['name']])['run_id'], root['run_id'])
        self.assertIn(child, self.api('GET', '/api/session/active')[1]['data'])
        self.action(other, 'archive')

        status, result = self.api('POST', f'/api/session/{child}/interrupt', {})
        self.assertIn(status, (200, 204), result)
        self.model.release.set()
        self.wait(root['name'], lambda row: 'archive' in row['allowed_actions'])
        self.action(root, 'archive')

    def test_partial_restore_keeps_one_original_and_recovers_from_archive(self):
        row = self.start()
        name = row['name']
        archive_id = self.action(row, 'archive')['result_target']['archive_id']
        source = self.hgs(['inspect', name, '--archive', archive_id])
        db_path = self.state / 'opencode/archives-v1.sqlite3'
        pending = self.payload(source, action='restore', archive_id=archive_id)
        with sqlite3.connect(db_path) as db:
            db.execute(f"""CREATE TRIGGER fail_exact_removal BEFORE DELETE ON archive_rows
                            WHEN OLD.id='{archive_id}'
                            BEGIN SELECT RAISE(ABORT, 'injected exact archive removal failure'); END""")
        try:
            result = self.hgs(['session-action', name, '--json'], pending)
            self.assertEqual(result['status'], 'uncertain', result)
            current = self.hgs(['inspect', name])
            self.assertNotEqual(current['run_id'], source['run_id'])
            self.assertEqual(current['session_id'], source['session_id'])
            self.assertEqual(current['conversation_id'], source['conversation_id'])
            self.assertNotIn('archive', current['allowed_actions'])
            self.action(current, 'rename', new_name='opencode/workspace/temporary-rename', status='failed')
            self.action(current, 'forget', status='failed')
            self.action(source, 'forget', archive_id=archive_id, status='failed')
            page = self.hgs(['ls', '--local', '--json'])
            archived = next(entry for entry in page['sessions'] if entry.get('archive_id') == archive_id)
            self.assertTrue(archived['restore_pending'], archived)
            self.assertIn('restore', archived['allowed_actions'])
            self.assertNotIn('forget', archived['allowed_actions'])
            self.assertEqual(self.hgs(['session-action-status', name, '--json'], pending)['status'],
                             'uncertain')
            self.assertEqual(self.hgs(['session-action', name, '--json'], pending), result)
        finally:
            with sqlite3.connect(db_path) as db:
                db.execute('DROP TRIGGER fail_exact_removal')

        finished = self.action(source, 'restore', archive_id=archive_id)
        self.assertEqual(finished['result_target']['run_id'], current['run_id'])
        self.hgs(['inspect', name, '--archive', archive_id], success=False)
        self.assertEqual(self.hgs(['inspect', name])['run_id'], current['run_id'])
