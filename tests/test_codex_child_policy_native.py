"""Private tracked Codex sessions with a terminal policy result from localhost."""
import datetime
import hashlib
import io
import json
import os
from pathlib import Path
import shlex
import signal
import sqlite3
import subprocess
import sys
import threading
import time
import unittest
import uuid

import test_pause
from test_codex_async_questions_native import Provider as CompletionProvider
from test_codex_input_native import StartupFixture
from test_watch import Stream

if os.name == 'posix':
    import fcntl

LAUNCH = 'Synthetic supervisor launches a local child.'
REFUSE = 'Synthetic child request for a policy fixture.'
NON_POLICY = 'Synthetic unrelated provider failure.'
POLICY_SOURCE = 'zerus_child_policy'
PRIVATE = 'Synthetic private refusal detail.'
CALL_ID = 'call_launch_fixture_child'
CHILD = 'codex/p/child'


class PolicyProvider(CompletionProvider):
    def do_POST(self):
        raw = self.rfile.read(int(self.headers['Content-Length']))
        request = json.loads(raw)
        items = request.get('input', [])
        texts = [part.get('text') for item in items if item.get('role') == 'user'
                 for part in item.get('content', []) if isinstance(part.get('text'), str)]
        if texts[-1:] in ([REFUSE], [NON_POLICY]):
            self.server.refusal_offers += 1
            self.server.refusal_ready.set()
            if not self.server.release_refusal.wait(10):
                self.send_error(500, 'Private fixture did not release the refusal')
                return
            self.begin_response()
            code = 'misalignment_policy_violation' if texts[-1] == REFUSE else 'invalid_prompt'
            self.emit('response.failed', response={
                'id': 'resp_policy_fixture', 'object': 'response', 'status': 'failed',
                'error': {'code': code, 'message': PRIVATE}})
            return
        if texts[-1:] != [LAUNCH] or any(item.get('call_id') == CALL_ID for item in items):
            self.rfile = io.BytesIO(raw)
            return super().do_POST()
        tools = request.get('tools', []) + [tool for item in items if item.get('type') == 'additional_tools'
                                           for tool in item.get('tools', [])]
        exec_tool = next((tool for namespace in tools if namespace.get('name') == 'functions'
                          for tool in namespace.get('tools', [])
                          if tool.get('type') == 'custom' and tool.get('name') == 'exec'), None)
        if exec_tool is None:
            self.send_error(400, 'The private Codex fixture needs its native shell tool')
            return
        code = 'text(await tools.exec_command({cmd:' + json.dumps(self.server.child_command) + '}));'
        item = dict(type='custom_tool_call', id='fc_launch_fixture_child', call_id=CALL_ID,
                    name='exec', namespace='functions', input=code, status='completed')
        result = dict(id='resp_spawn_fixture', object='response', status='completed', output=[item],
                      usage=dict(input_tokens=1, output_tokens=1, total_tokens=2))
        self.begin_response()
        self.emit('response.created', response=dict(result, status='in_progress', output=[]))
        self.emit('response.output_item.added', output_index=0, item=dict(item, status='in_progress'))
        self.emit('response.output_item.done', output_index=0, item=item)
        self.emit('response.completed', response=result)

    def begin_response(self):
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()


@unittest.skipUnless(os.environ.get('HGS_CODEX_TEST_BIN') and sys.platform in ('linux', 'darwin'),
                     'Use the isolated native Codex binary on a terminal host')
class NativeCodexChildPolicy(StartupFixture):
    def setUp(self):
        super().setUp()
        self.env['TMUX_TMPDIR'] = str(self.root)
        self.server.RequestHandlerClass = PolicyProvider
        self.server.refusal_offers = 0
        self.server.refusal_ready = threading.Event()
        self.server.release_refusal = threading.Event()
        self.server.release_refusal.set()
        self.addCleanup(self.server.release_refusal.set)
        self.server.child_command = shlex.join([
            str(test_pause.HGS), 'codex', 'p', '--new', '-n', 'child', '-d', '--',
            '--dangerously-bypass-approvals-and-sandbox', '--dangerously-bypass-hook-trust'])

    def send(self, name, text, uncertain=False):
        record = self.binding(name)
        request = dict(request_id=str(uuid.uuid4()), expected_run_id=record['run_id'],
                       expected_conversation_id=record.get('conversation_id') or '', text=text)
        if uncertain:
            result = subprocess.run([str(test_pause.HGS), 'send', name, '--json'], env=self.env,
                                    input=json.dumps(request), capture_output=True, text=True, timeout=20)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('delivery uncertain', result.stderr)
            return json.loads((self.root / 'state/input_receipts' / (request['request_id'] + '.json')).read_text())
        receipt = self.call_json('send', name, payload=request)
        self.assertEqual(receipt['status'], 'submitted')
        return receipt

    def call_json(self, *args, payload):
        result = subprocess.run([str(test_pause.HGS), *args, '--json'], env=self.env,
                                input=json.dumps(payload), capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def info(self, name):
        return json.loads(self.hgs('inspect', name, '--skip-processes'))

    def start_pair(self):
        self.hgs('swarm', 'hello')
        self.launch()
        self.until('The ordinary startup composer must be fully visible', lambda:
                   self.inspect().get('first_message_can_send') and 'Ask Codex to do anything' in self.screen())
        time.sleep(.25)
        self.deliver_first()
        self.until('Parent should accept an ordinary message',
                   lambda: self.info(self.name).get('guarded_idle_send'))
        self.send(self.name, LAUNCH)
        linked = self.until('The real parent shell must launch a child with ancestry proof',
                            lambda: self.binding(CHILD).get('parent'))
        parent = self.binding(self.name)
        self.assertEqual(linked['source'], 'launch')
        keys = ('name', 'session_id', 'run_id', 'conversation_id')
        self.assertEqual(tuple(linked[k] for k in keys), tuple(parent[k] for k in keys))
        self.until('Child should reach the ordinary composer',
                   lambda: self.info(CHILD).get('first_message_can_send'))
        # Link and creation metadata have whole-second precision.
        time.sleep(max(0, max(linked['at'], self.binding(CHILD)['created']) + 1.05 - time.time()))
        return parent

    def refusal(self, uncertain=False):
        receipt = self.send(CHILD, REFUSE, uncertain)
        row = self.until('The child must record a typed native completion', self.child_refusal)
        child = self.binding(CHILD)
        self.assertEqual(child['run_id'], receipt['run_id'])
        self.assertTrue(child['conversation_id'])
        print(json.dumps(dict(child_run=child['run_id'], child_conversation=child['conversation_id'],
                              native_kind=row['payload']['error']['codex_error_info'],
                              turn_id=row['payload']['turn_id'], event_at=row['timestamp'],
                              request=receipt['request_id'], refusal_offers=self.server.refusal_offers)), flush=True)
        return child, row

    def child_refusal(self, require_policy=True):
        path = self.binding(CHILD).get('transcript')
        if not path or not Path(path).is_file():
            return None
        with Path(path).open() as transcript:
            for line in transcript:
                if not line.endswith('\n'):
                    continue
                row = json.loads(line)
                event = row.get('payload', {})
                error = event.get('error') or {}
                if event.get('type') == 'task_complete' and error and (
                        not require_policy or error.get('codex_error_info') == 'misalignment_policy_violation'):
                    return row

    def restart_parent(self, parent):
        self.until('The parent must finish its original turn', lambda: self.info(self.name).get('guarded_idle_send'))
        self.pause_settled_original()
        self.hgs('resume', self.name, '-d')
        self.until('The new parent run must be ready',
                   lambda: self.binding(self.name).get('run_id') != parent['run_id']
                   and self.inspect().get('resume_message_can_send'))
        self.assertEqual(self.binding(self.name)['conversation_id'], parent['conversation_id'])
        text = 'Synthetic parent readiness after restart.'
        self.send(self.name, text)
        self.until('The replacement parent must finish its own input', lambda: self.turn_completed(text))


    def declare_waits(self, parent):
        prompt = 'Synthetic independent child readiness.'
        self.send(CHILD, prompt)
        self.until('Child must confirm the previous independent turn', lambda:
                   self.binding(CHILD).get('conversation_id') and self.info(CHILD).get('guarded_idle_send'))
        child_ref = json.loads(self.hgs('reference', CHILD, '--json'))['uri']
        parent_ref = json.loads(self.hgs('reference', self.name, '--json'))['uri']

        def declare(label):
            return self.call_json('wait', 'begin', payload=dict(
                request_id=str(uuid.uuid4()), waiter_reference=parent_ref,
                producer_reference=child_ref, label=label))['reply_ticket']

        previous = declare('Earlier independent result')
        prior_text = 'Synthetic result for the earlier wait.'
        receipt = self.send(self.name, prior_text)
        settled = self.call_json('wait', 'finish', payload=dict(
            previous, request_id=str(uuid.uuid4()), result_request_id=receipt['request_id']))
        self.assertEqual(settled['state'], 'completed')
        self.until('The parent must finish the previous result turn', lambda: self.turn_completed(prior_text))
        ticket = declare('Unsettled independent result')
        pending = self.call_json('wait', 'ack', payload=dict(ticket, request_id=str(uuid.uuid4())))
        self.assertEqual(pending['state'], 'awaiting_result')
        return self.wait_page(parent), ticket


    def wait_page(self, parent):
        return self.call_json('wait', 'list', payload=dict(
            expected_run_id=parent['run_id'], expected_conversation_id=parent['conversation_id']))


    def lose_enter(self, pane):
        command = shlex.join([test_pause.TMUX, '-S', self.socket, '-f', '/dev/null'])
        self.script('tmux', '\n'.join([
            '#!/bin/sh',
            "if [ \"$*\" = " + shlex.quote(f'send-keys -t {pane} Enter') + " ]; then",
            f'    {command} "$@"',
            '    exit 1',
            'fi',
            f'exec {command} "$@"',
        ]))


    def subscribe(self, record, after=None, executable=None):
        stream = Stream(self, executable=executable or test_pause.HGS)
        stream.next(lambda frame: frame['t'] == 'hello')
        stream.next(lambda frame: frame['t'] == 'snapshot' and frame['topic'] == 'catalog' and frame['last'])
        session = {key: record[key] for key in ('name', 'session_id', 'run_id', 'conversation_id')}
        request = dict(op='subscribe', topic='activity', session=session)
        if after is not None:
            request['after'] = after
        stream.send(**request)
        return stream

    @staticmethod
    def notices(frame):
        return [event for event in frame.get('events', []) if event.get('source') == POLICY_SOURCE]

    def test_exact_parent_receives_one_native_refusal_in_activity(self):
        parent = self.start_pair()
        waits, _ = self.declare_waits(parent)
        before_requests = len(self.server.requests)
        stream = self.subscribe(parent)
        stream.next(lambda frame: frame['t'] == 'events')
        child, row = self.refusal()
        event = self.notices(stream.next(lambda frame: self.notices(frame), timeout=15))[0]
        self.assertEqual(event['run_id'], parent['run_id'])
        advisory = event['child_policy_advisory']
        self.assertEqual(advisory['scope'], 'session')
        keys = ('session_id', 'run_id', 'conversation_id')
        self.assertEqual(tuple(advisory['child'][key] for key in keys), tuple(child[key] for key in keys))
        self.assertEqual(advisory['event']['turn_id'], row['payload']['turn_id'])
        self.assertNotIn(REFUSE, json.dumps(event))
        self.assertNotIn(PRIVATE, json.dumps(event))
        self.assertNotIn('request_id', event)
        after = datetime.datetime.fromisoformat(row['timestamp']).timestamp()
        self.assertGreaterEqual(event['at'], after)
        self.assertLess(event['at'] - after, 10)
        print(json.dumps(dict(event_id=event['message_id'], sequence=event['seq'],
                              latency_ms=round((event['at'] - after) * 1_000))), flush=True)
        self.restart_observer(child)
        stream.quiet(self.notices, 3)
        self.assertEqual(self.wait_page(parent), waits)
        self.assertEqual(len(self.server.requests), before_requests, 'the notice never enters native input')
        self.assertEqual(stream.close(), 0)
        reconnected = self.subscribe(parent)
        history = reconnected.next(lambda frame: frame['t'] == 'events')['events']
        self.assertEqual([item for item in history if item.get('source') == POLICY_SOURCE], [event])
        reconnected.quiet(self.notices, 3)
        self.assertEqual(self.server.refusal_offers, 1)
        self.assertEqual(reconnected.close(), 0)

        if older := os.environ.get('HGS_POLICY_OLD_BIN'):
            old_reader = self.subscribe(parent, executable=Path(older))
            old_events = old_reader.next(lambda frame: frame['t'] == 'events')['events']
            self.assertEqual([item for item in old_events if item.get('source') == POLICY_SOURCE], [event])
            self.assertEqual(old_reader.close(), 0)
            print('The older main binary read the same typed event.', flush=True)
        # Resuming the parent retains this conversation's original history.
        self.restart_parent(parent)
        events = [item for item in self.info(self.name)['events'] if item.get('source') == POLICY_SOURCE]
        self.assertEqual(events, [event])

    def restart_observer(self, child):
        binary = os.environ.get('HGS_POLICY_WATCH_TEST_BIN')
        if not binary:
            return
        result = subprocess.run(
            [binary, '--exact', 'state::parent_link::advisory::activity::tests::reobserve_live_native_child',
             '--ignored', '--test-threads=4'],
            env=dict(self.env, ZERUS_CHILD_POLICY_TEST_ROOT=str(self.root),
                     ZERUS_CHILD_POLICY_TEST_CHILD=child['session_id']),
            capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('1 passed', result.stdout)
        print('A fresh observer reprocessed the original native refusal.', flush=True)

    def test_new_parent_run_is_not_a_recipient_of_the_old_run_refusal(self):
        parent = self.start_pair()
        self.restart_parent(parent)
        replacement = self.binding(self.name)
        stream = self.subscribe(replacement)
        stream.next(lambda frame: frame['t'] == 'events')
        self.refusal()
        stream.quiet(self.notices, 4)
        self.assertEqual(self.server.refusal_offers, 1)
        self.assertEqual(stream.close(), 0)

    def test_native_refusal_after_uncertain_delivery_is_visible_without_reoffer(self):
        parent = self.start_pair()
        stream = self.subscribe(parent)
        stream.next(lambda frame: frame['t'] == 'events')
        self.lose_enter(self.binding(CHILD)['pane'])
        child, row = self.refusal(uncertain=True)
        [event] = self.notices(stream.next(lambda frame: self.notices(frame)))
        self.assertEqual(event['child_policy_advisory']['event']['turn_id'], row['payload']['turn_id'])
        self.assertEqual(event['child_policy_advisory']['child']['run_id'], child['run_id'])
        stream.quiet(self.notices, 3)
        self.assertEqual(self.server.refusal_offers, 1)
        self.assertEqual(stream.close(), 0)

    def test_non_policy_native_failure_does_not_publish_a_notice(self):
        parent = self.start_pair()
        stream = self.subscribe(parent)
        stream.next(lambda frame: frame['t'] == 'events')
        self.send(CHILD, NON_POLICY)
        self.until('The child must record the other native failure', lambda: self.child_refusal(False))
        stream.quiet(self.notices, 4)
        self.assertEqual(self.server.refusal_offers, 1)
        self.assertEqual(stream.close(), 0)

    def test_exited_child_cannot_publish_after_a_busy_activity_writer(self):
        parent = self.start_pair()
        stream = self.subscribe(parent)
        stream.next(lambda frame: frame['t'] == 'events')
        self.server.release_refusal.clear()
        self.send(CHILD, REFUSE)
        self.assertTrue(self.server.refusal_ready.wait(10))
        writer = sqlite3.connect(self.root / 'state/events.sqlite3')
        try:
            writer.execute('BEGIN IMMEDIATE')
            self.server.release_refusal.set()
            self.until('The child must record the policy failure', self.child_refusal)
            child = self.binding(CHILD)
            # Let at least one observation reach the contended journal.
            stream.quiet(self.notices, 2.3)
            self.assertEqual(self.binding(CHILD)['pid'], child['pid'])
            self.assertEqual(self.binding(CHILD)['process_start'], child['process_start'])
            os.kill(child['pid'], signal.SIGKILL)
            self.until('The exact child supervisor must settle while Activity is locked',
                       lambda: self.binding(CHILD).get('exited_at'))
        finally:
            writer.rollback()
            writer.close()
        stream.quiet(self.notices, 3)
        self.assertEqual(self.server.refusal_offers, 1)
        self.assertEqual(stream.close(), 0)

    def test_unchanged_native_refusal_is_retried_after_temporary_process_stop(self):
        parent = self.start_pair()
        stream = self.subscribe(parent)
        stream.next(lambda frame: frame['t'] == 'events')
        self.server.release_refusal.clear()
        self.send(CHILD, REFUSE)
        self.assertTrue(self.server.refusal_ready.wait(10))
        writer = sqlite3.connect(self.root / 'state/events.sqlite3')
        paused = False
        try:
            writer.execute('BEGIN IMMEDIATE')
            self.server.release_refusal.set()
            self.until('The child must record the policy failure', self.child_refusal)
            child = self.binding(CHILD)
            os.kill(child['pid'], signal.SIGSTOP)
            paused = True
            writer.rollback()
            stream.quiet(self.notices, 3)
        finally:
            writer.close()
            if paused:
                self.assertEqual(self.binding(CHILD)['pid'], child['pid'])
                self.assertEqual(self.binding(CHILD)['process_start'], child['process_start'])
                os.kill(child['pid'], signal.SIGCONT)
        [event] = self.notices(stream.next(lambda frame: self.notices(frame)))
        self.assertEqual(event['child_policy_advisory']['child']['run_id'], child['run_id'])
        self.assertEqual(self.server.refusal_offers, 1)
        self.assertEqual(stream.close(), 0)

    def test_parent_death_during_journal_wait_prevents_stale_publication(self):
        parent = self.start_pair()
        self.server.release_refusal.clear()
        self.send(CHILD, REFUSE)
        self.assertTrue(self.server.refusal_ready.wait(10))
        writer = sqlite3.connect(self.root / 'state/events.sqlite3')
        try:
            writer.execute('BEGIN IMMEDIATE')
            self.server.release_refusal.set()
            self.until('The child must record the policy failure', self.child_refusal)
            self.assertEqual(self.binding(self.name)['pid'], parent['pid'])
            self.assertEqual(self.binding(self.name)['process_start'], parent['process_start'])
            # The observer has to take Root before waiting on this fixture's
            # journal writer. Catch it there, kill the exact disposable
            # parent, and release SQLite inside the acquisition window.
            with (self.root / 'state/.lock').open('rb') as root:
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    try:
                        fcntl.flock(root, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    except BlockingIOError:
                        time.sleep(.025)
                        os.kill(parent['pid'], signal.SIGKILL)
                        exited = subprocess.run(['ps', '-p', str(parent['pid']), '-o', 'stat='],
                                                env=self.env, capture_output=True, text=True, timeout=1)
                        self.assertTrue(exited.returncode != 0 or exited.stdout.lstrip().startswith('Z'))
                        writer.rollback()
                        break
                    fcntl.flock(root, fcntl.LOCK_UN)
                    time.sleep(.002)
                else:
                    self.fail('The Activity observer never waited for the held journal')
            self.until('Original parent exit settles', lambda: self.binding(self.name).get('exited_at'))
            self.assertEqual(self.notices(self.info(self.name)), [])
            self.assertEqual(self.server.refusal_offers, 1)
        finally:
            writer.close()

    def test_the_actual_parent_wins_when_another_record_owns_its_label(self):
        parent = self.start_pair()
        sid, moved = self.move_to_id_layout(self.name)
        self.assertEqual(moved()['session_id'], parent['session_id'])
        self.assertEqual(sid, parent['session_id'])
        decoy_name = 'codex/p/decoy'
        self.hgs('codex', 'p', '--new', '-n', 'decoy', '-d', '--',
                 '--dangerously-bypass-approvals-and-sandbox', '--dangerously-bypass-hook-trust')
        self.until('Second native session starts', lambda: self.info(decoy_name).get('first_message_can_send'))
        self.send(decoy_name, 'Synthetic independent decoy prompt.')
        self.until('Decoy has its own native conversation', lambda: self.binding(decoy_name).get('conversation_id'))
        # Shipping launches still prevent duplicate labels. Move the two real
        # producer records into the supported reader layout for this control.
        state = self.root / 'state'
        saved = lambda name: state / (hashlib.sha256(name.encode()).hexdigest() + '.json')
        decoy = self.binding(decoy_name)
        decoy['name'] = parent['name']
        decoy['journal_name'] = decoy_name
        saved(parent['name']).write_text(json.dumps(decoy))
        saved(decoy_name).unlink()
        (state / 'ids' / (decoy['session_id'] + '.json')).write_text(
            json.dumps(dict(version=1, name=parent['name'], kind='record')))
        def events_for(record):
            details = json.loads(self.hgs('inspect', '--session-id', record['session_id'], '--skip-processes'))
            self.assertEqual(details['run_id'], record['run_id'])
            return [event for event in details['events'] if event.get('source') == POLICY_SOURCE]

        self.assertEqual(events_for(parent), [])
        self.assertEqual(events_for(decoy), [])
        self.refusal()
        [event] = self.until('The exact parent must receive Activity despite the duplicate label',
                             lambda: events_for(parent))
        self.assertEqual(event['run_id'], parent['run_id'])
        self.assertEqual(events_for(decoy), [])



if __name__ == '__main__':
    unittest.main()
