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
        self.send(name, self.payload(name, 'Start the held native Kimi turn.'))
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

        self.offer(name, raw, drop=True)
        receipt = self.original(name, raw)
        self.assertEqual((receipt['status'], receipt['proof']), ('submitted', 'native_kimi_terminal_receipt'), receipt)
        self.original(name, raw, success=False, input_sha256='0' * 64)
        self.check_file(name, raw)
        duplicate = self.offer(name, raw)
        self.assertEqual(duplicate['request_id'], receipt['request_id'])
        self.assertFalse(hold.is_set(), 'receipt arrived only after the native turn was released')
        hold.set()
        submitted = duplicate['submitted_text']
        self.until(lambda: self.held(submitted) == 1 and self.settled(name, duplicate), name)
        self.assertEqual(self.held(submitted), 1)
        self.assertEqual(self.held(DRAFT), 0)


# The native fixtures have independent compatibility jobs. This module owns
# only its new exact guarded-route cases.
for _child, _parent in ((NativeClaudeNext, claude.NativeClaudeOutbox), (NativeKimiNext, kimi.NativeKimi)):
    for _case in unittest.defaultTestLoader.getTestCaseNames(_parent):
        setattr(_child, _case, None)
