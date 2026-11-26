"""Guarded Desktop FIFO offers against a credential-free, isolated Claude."""
import base64
import hashlib
import json
import subprocess

import test_busy_input_native as busy
import test_claude_input_native as native_input
import test_claude_questions_native as native


class NativeClaudeOutbox(busy.NativeBusyClaude):
    def native_settings(self, _work):
        self.hook_log, command = native_input.identity_recorder(self.root)
        return {'hooks': {'UserPromptSubmit': [{'hooks': [
            {'type': 'command', 'command': command, 'timeout': 5}]}]}}

    def call(self, args, raw, success=True):
        reply = subprocess.run([str(native.fixtures.HGS), *args], env=self.env,
                               input=raw, capture_output=True, timeout=20)
        if success:
            self.assertEqual(reply.returncode, 0, reply.stderr.decode())
            return json.loads(reply.stdout)
        self.assertNotEqual(reply.returncode, 0, reply.stdout)

    def original(self, request, success=True, **changes):
        fields = json.loads(request)
        query = {key: fields[key] for key in ('request_id', 'expected_run_id', 'expected_conversation_id')}
        query.update(input_sha256=hashlib.sha256(request).hexdigest(), **changes)
        return self.call(['delivery', 'outbox-status', self.name, '--json'], json.dumps(query).encode(), success)

    def guarded(self, **fields):
        return json.dumps(self.payload(only_when_idle=True, **fields)).encode()

    def new_rows(self, text):
        return [row for row in self.rows() if row.get('type') == 'user'
                and row.get('message', {}).get('content') == text]

    def test_desktop_offer_waits_for_native_queue_and_keeps_equal_messages_distinct(self):
        release = self.root / 'tool-release'
        self.addCleanup(release.touch)
        self.busy_ready = True
        busy.terminal_send(self, busy.PROMPT)
        self.wait_for((self.root / 'tool-started').exists)
        prompt = 'continue the private test'
        busy.terminal_send(self, prompt)
        self.wait_for(lambda: 'Press up to edit queued messages' in self.screen())
        self.wait_for(lambda: len(self.enqueued(prompt)) == 1)
        # The actual CLI queue still exists. Simulate a Ready record one
        # publication ahead of the native pane to exercise the host recheck.
        self.record.update(activity='idle', phase='idle')
        self.write_record()
        first, second = self.guarded(text=prompt), self.guarded(text=prompt)
        command = ['send', self.name, '--when-idle', '--json']
        wait = self.call(command, first)
        self.assertEqual(wait['status'], 'waiting_for_idle', wait)
        self.assertEqual(wait['input_sha256'], hashlib.sha256(first).hexdigest())
        self.assertEqual(wait['request_id'], json.loads(first)['request_id'])
        self.assertEqual(len(self.enqueued(prompt)), 1)
        self.original(first, success=False)
        release.touch()
        self.wait_for(lambda: self.queued_messages() == [prompt])
        self.wait_for(lambda: 'esc to interrupt' not in self.screen())

        receipts = []
        for offer in (first, second):
            delivered = self.call(command, offer)
            self.assertEqual((delivered['status'], delivered['native']), ('submitted', 'transcript'))
            self.assertTrue(delivered['claude_outbox'])
            echoed = self.original(offer)
            self.assertEqual(echoed['native_input'], delivered['native_input'])
            self.assertEqual(echoed['request_id'], json.loads(offer)['request_id'])
            receipts.append(echoed)
            self.wait_for(lambda: 'esc to interrupt' not in self.screen())
        self.assertNotEqual(receipts[0]['request_id'], receipts[1]['request_id'])
        prompts = [row['native_input']['prompt_id'] for row in receipts]
        self.assertEqual(len(set(prompts)), 2)
        self.assertEqual([row['promptId'] for row in self.new_rows(prompt)], prompts)
        self.assertEqual(self.queued_messages(), [prompt])

    def test_lost_ack_recovers_original_file_and_never_uses_unconfirmed_receipt(self):
        name = 'synthetic.txt'
        body = b'isolated fixture file, with no account data'
        offer = self.guarded(text='Inspect [File #1]', attachments=[{
            'name': name, 'mime': 'text/plain', 'reference': '[File #1]',
            'data_base64': base64.b64encode(body).decode(),
        }])
        # Drop the original CLI's response while retaining its real native effect.
        result = subprocess.run([str(native.fixtures.HGS), 'send', self.name, '--when-idle', '--json'],
                                env=self.env, input=offer, stdout=subprocess.DEVNULL,
                                stderr=subprocess.PIPE, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        key = json.loads(offer)['request_id']
        path = self.state / 'input_receipts' / (key + '.json')
        saved = path.read_bytes()
        receipt = json.loads(saved)
        self.assertIn('Read the attached file at', receipt['submitted_text'])
        self.assertEqual(self.original(offer)['native_input'], receipt['native_input'])
        self.original(offer, success=False, expected_conversation_id=self.run_id)
        bad_digest = offer + b' '
        self.original(bad_digest, success=False)
        # The same original receipt without its final write confers no proof.
        proof = json.loads(saved)
        proof['status'] = 'in_progress'
        try:
            path.write_text(json.dumps(proof))
            self.original(offer, success=False)
        finally:
            path.write_bytes(saved)
        args = ['attachment', self.name, '--request', key, '--index', '0',
                '--conversation', self.conversation_id, '--json']
        recovered = self.call(args, b'')
        self.assertEqual(base64.b64decode(recovered['data_base64']), body)
        self.assertEqual(sum(row.get('uuid') == receipt['native_input']['uuid'] for row in self.rows()), 1)
