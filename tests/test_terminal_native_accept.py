"""Exact busy Desktop offers against the private real terminal producers."""
import base64
import hashlib
import json
import subprocess
import threading
import unittest

import test_busy_input_native as busy
import test_claude_outbox_native as claude
import test_kimi_native as kimi


FILE = b'The private fixture keeps this exact file.\n'
FOLLOWUP = 'Please read the attached [File #1] after the earlier message.'
PRIOR = 'This already belongs to the native queue.'
DRAFT = 'Preserve this unsent terminal draft.'


class Original:
    def call(self, *argv, raw=b'', success=True, drop=False):
        result = subprocess.run([str(self.cli_image), *argv], env=self.env, input=raw,
                                stdout=subprocess.DEVNULL if drop else subprocess.PIPE,
                                stderr=subprocess.PIPE, timeout=30)
        if success:
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            return None if drop else json.loads(result.stdout)
        self.assertNotEqual(result.returncode, 0, result.stdout)

    def original(self, name, raw, success=True, **changed):
        request = json.loads(raw)
        query = {key: request[key] for key in ('request_id', 'expected_run_id', 'expected_conversation_id')}
        query['input_sha256'] = hashlib.sha256(raw).hexdigest()
        query.update(changed)
        return self.call('delivery', 'outbox-status', name, '--json', raw=json.dumps(query).encode(), success=success)

    def check_file(self, name, raw):
        request = json.loads(raw)
        result = self.call('attachment', name, '--request', request['request_id'], '--index', '0',
                           '--conversation', request['expected_conversation_id'], '--json')
        self.assertEqual(base64.b64decode(result['data_base64']), FILE)

    def offer(self, name, raw, **options):
        return self.call('send', name, '--when-native-accepts', '--json', raw=raw, **options)

    def request(self, value):
        return json.dumps(dict(value, only_when_idle=True, attachments=[dict(
            name='changes.txt', mime='text/plain', reference='[File #1]', data_base64=base64.b64encode(FILE).decode())])).encode()


class NativeClaudeNext(Original, claude.NativeClaudeOutbox):
    cli_image = claude.native.fixtures.HGS

    def test_busy_native_queue_keeps_draft_and_recovers_the_original_file(self):
        release = self.root / 'tool-release'
        self.addCleanup(release.touch)
        self.busy_ready = True
        busy.terminal_send(self, busy.PROMPT)
        self.wait_for((self.root / 'tool-started').exists)
        self.record.update(activity='busy', phase='tool', active_tools={'toolu_busy': {'tool_name': 'Bash'}})
        self.write_record()
        busy.terminal_send(self, PRIOR)
        self.wait_for(lambda: len(self.enqueued(PRIOR)) == 1)
        self.tmux('send-keys', '-t', self.pane, '-l', DRAFT)
        self.wait_for(lambda: DRAFT in self.screen())
        raw = self.request(self.payload(text=FOLLOWUP))
        wait = self.offer(self.name, raw)
        self.assertEqual(wait['status'], 'waiting_for_idle', wait)
        self.assertEqual(self.original(self.name, raw), wait)
        self.assertIn(DRAFT, self.screen())
        self.assertEqual(self.enqueued(FOLLOWUP), [])
        self.tmux('send-keys', '-t', self.pane, 'C-u')
        self.wait_for(lambda: DRAFT not in self.screen())

        self.offer(self.name, raw, drop=True)
        receipt = self.original(self.name, raw)
        self.assertEqual((receipt['status'], receipt['native']), ('submitted', 'queue'), receipt)
        self.original(self.name, raw, success=False, input_sha256='0' * 64)
        self.check_file(self.name, raw)
        duplicate = self.offer(self.name, raw)
        submitted = duplicate['submitted_text']
        self.assertEqual(duplicate['request_id'], receipt['request_id'])
        def attachment_banner():
            detail = self.call('inspect', self.name)
            return next((event for event in detail.get('attachment_messages', [])
                         if event.get('message_id') == receipt['request_id']), None)
        projected = self.wait_for(attachment_banner)
        self.assertTrue(projected['claude_queued_outbox'])
        self.assertEqual((projected['run_id'], projected['conversation_id']), (self.run_id, self.conversation_id))
        self.assertNotIn('claude_prompt_id', projected)
        self.assertEqual(len(self.enqueued(submitted)), 1)
        self.assertFalse(release.exists(), 'receipt arrived only after the tool was released')
        release.touch()
        self.wait_for(lambda: self.queued_messages() == [PRIOR, submitted])
        self.assertEqual(len(self.enqueued(submitted)), 1)


@unittest.skipUnless(kimi.KIMI, 'Set the official private Kimi fixture pin')
class NativeKimiNext(Original, kimi.NativeKimi):
    cli_image = kimi.lifecycle.HGS

    def test_busy_native_input_keeps_draft_and_recovers_the_original_file(self):
        name = self.launch()
        hold = threading.Event()
        self.addCleanup(hold.set)
        self.model_server.hold = hold
        quoted_hint = '↑ to edit · ctrl-s to steer immediately'
        self.send(name, self.payload(name, f'Start the held native Kimi turn.\n{quoted_hint}\nKeep working.'))
        self.until(lambda: self.inspect(name).get('activity') == 'busy' and bool(self.requests), name)
        pane = '=' + name + ':'
        self.t('send-keys', '-t', pane, '-l', DRAFT)
        self.until(lambda: DRAFT in self.screen(name), name)
        raw = self.request(self.payload(name, FOLLOWUP))
        wait = self.offer(name, raw)
        self.assertEqual(wait['status'], 'waiting_for_idle', wait)
        self.assertEqual(self.original(name, raw), wait)
        self.assertIn(DRAFT, self.screen(name))
        self.assertEqual(self.held(FOLLOWUP), 0)
        self.t('send-keys', '-t', pane, 'C-u')
        self.until(lambda: DRAFT not in self.screen(name), name)
        self.assertIn(quoted_hint, [row.strip() for row in self.screen(name).splitlines()])

        self.offer(name, raw, drop=True)
        receipt = self.original(name, raw)
        self.assertEqual((receipt['status'], receipt['proof']), ('submitted', 'native_kimi_terminal_receipt'), receipt)
        self.original(name, raw, success=False, input_sha256='0' * 64)
        self.check_file(name, raw)
        duplicate = self.offer(name, raw)
        self.assertEqual(duplicate['request_id'], receipt['request_id'])
        self.assertFalse(hold.is_set(), 'receipt arrived only after the native turn was released')
        self.assertTrue(self.inspect(name)['previous_message_pending'])
        for callback in ('UserPromptQueued', 'UserPromptSubmit'):
            # Deliver a delayed callback through the actual tracked hook
            # ingress while the new original is still in native custody.
            old = self.send_event(name, callback, prompt='An unrelated earlier native callback.')
            self.assertEqual(old.returncode, 0, old.stderr)
            self.assertTrue(self.inspect(name)['previous_message_pending'], callback)
        hold.set()
        submitted = duplicate['submitted_text']
        self.until(lambda: self.held(submitted) == 1 and self.settled(name, duplicate)
                   and self.inspect(name)['previous_message_pending'] is False, name)
        self.assertEqual(self.held(submitted), 1)
        self.assertEqual(self.held(DRAFT), 0)



    def test_equal_native_backlog_reoffers_same_request_during_next_working_turn(self):
        first_release = threading.Event()
        second_started = threading.Event()
        second_release = threading.Event()
        self.addCleanup(first_release.set)
        self.addCleanup(second_release.set)

        class HoldSecond(kimi.Model):
            def do_POST(model):
                if not model.path.startswith(kimi.PLATFORM + '/') and len(model.server.requests) == 1:
                    second_started.set()
                    second_release.wait(20)
                super().do_POST()

        self.model_server.hold = first_release
        self.model_server.RequestHandlerClass = HoldSecond
        name = self.launch()
        self.send(name, self.payload(name, 'Hold the first native turn.'))
        self.until(lambda: self.inspect(name).get('activity') == 'busy' and bool(self.requests), name)
        pane = '=' + name + ':'
        text = 'Repeated equal private native prompt.'
        self.t('send-keys', '-t', pane, '-l', text)
        self.until(lambda: text in self.screen(name), name)
        self.t('send-keys', '-t', pane, 'Enter')
        earlier = self.until(lambda: self.inspect(name).get('input_queue'), name)
        self.assertIn(text, earlier['text'])
        raw = json.dumps(dict(self.payload(name, text), only_when_idle=True)).encode()
        result = self.offer(name, raw)
        self.assertEqual(result['status'], 'waiting_for_idle', result)
        self.assertEqual(self.original(name, raw), result)
        self.assertEqual(self.inspect(name)['input_queue']['id'], earlier['id'])
        self.assertEqual(self.held(text), 0)

        first_release.set()
        self.assertTrue(second_started.wait(20), 'The earlier native message did not start')
        def working_without_backlog():
            detail = self.inspect(name)
            return (detail['activity'] == 'busy' and not detail.get('input_queue')
                    and detail['previous_message_pending'] is False)
        self.until(working_without_backlog, name, seconds=10)
        result = self.offer(name, raw)
        self.assertEqual(result['status'], 'submitted', result)
        self.assertEqual(self.original(name, raw)['request_id'], result['request_id'])
        self.assertFalse(second_release.is_set())
        self.assertEqual(self.inspect(name)['activity'], 'busy')
        self.assertTrue(self.inspect(name)['previous_message_pending'])

        second_release.set()
        self.until(lambda: self.held(text) == 2 and self.settled(name, result)
                   and self.inspect(name)['previous_message_pending'] is False, name)
        self.assertEqual(self.offer(name, raw), result)
        self.assertEqual(self.held(text), 2)

# The native fixtures have independent compatibility jobs. This module owns
# only its new exact guarded-route cases.
for _child, _parent in ((NativeClaudeNext, claude.NativeClaudeOutbox), (NativeKimiNext, kimi.NativeKimi)):
    for _case in unittest.defaultTestLoader.getTestCaseNames(_parent):
        setattr(_child, _case, None)
