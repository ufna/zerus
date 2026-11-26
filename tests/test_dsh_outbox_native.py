"""Guarded Desktop outbox against the sealed official DeepSeek harness."""
import base64
import hashlib
import json
from pathlib import Path
import uuid

import test_dsh_native as native


class NativeDeepSeekOutbox(native.NativeHarness):
    def guarded(self, binding, request):
        return self.hgs('send', binding['name'], '--when-idle', '--json', request=request)

    def test_busy_guarded_send_leaves_no_offer(self):
        binding = self.create()
        running = self.payload(binding, text='INTERRUPT_WAIT_FIXTURE')
        self.cli('send', binding, running)
        self.wait(lambda: self.cli('inspect', binding)['activity'] == 'busy')
        waiting = self.payload(binding, text='Wait to send the next fixture message', only_when_idle=True)
        result = self.guarded(binding, waiting)
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(result.stdout)
        self.assertEqual(receipt['status'], 'waiting_for_idle')
        self.assertEqual(receipt['request_id'], waiting['request_id'])
        self.assertEqual(receipt['name'], binding['name'])
        self.assertFalse((self.state / 'dsh/receipts' / f"{waiting['request_id']}.json").exists())
        self.assertNotIn(waiting['request_id'], json.dumps(self.rpc('inspect', {'sessionId': binding['conversation_id']})['page']))

    def outbox_status(self, binding, request, **override):
        query = dict(request_id=request['request_id'], expected_run_id=binding['run_id'],
                     expected_conversation_id=binding['conversation_id'],
                     input_sha256=hashlib.sha256(json.dumps(request).encode()).hexdigest())
        query.update(override)
        return self.hgs('delivery', 'outbox-status', binding['name'], '--json', request=query)

    def native_messages(self, binding, request_id):
        records = self.rpc('inspect', {'sessionId': binding['conversation_id']})['page']['records']
        return [row['event'] for row in records if row.get('event', {}).get('type') == 'user/message'
                and row['event']['data']['source'].get('rpcId') == request_id]

    def test_lost_bridge_ack_recovers_durable_native_file_on_reconnect(self):
        binding = self.create()
        data = b'One private, fixture-owned file.\n'
        request = self.payload(binding, text='Read [File #1] after the marker', only_when_idle=True,
                               attachments=[{'name': 'changes.txt', 'mime': 'text/plain', 'reference': '[File #1]',
                                             'data_base64': base64.b64encode(data).decode()}])
        receipt_path = self.state / 'dsh/receipts' / f"{request['request_id']}.json"
        offers = []

        def intercept(message, forward):
            if message['method'] == 'outbox-send':
                self.assertEqual(json.loads(receipt_path.read_text())['status'], 'pending')
                self.assertEqual(message['params']['mode'], 'queue')
                answer = forward(message)
                self.assertTrue(answer['ok'], answer)
                self.assertTrue(answer['value']['accepted'], answer)
                offers.append(message['params']['requestId'])
                return None
            if message['method'] == 'outbox-proof':
                return None
            return forward(message)

        with self.archive_rpc_proxy(intercept):
            first = self.guarded(binding, request)
            self.assertEqual(first.returncode, 1, first.stdout)
            self.assertIn('delivery uncertain', first.stderr.lower())
            self.assertEqual(self.guarded(binding, request).returncode, 1)
            self.assertEqual(self.outbox_status(binding, request).returncode, 1)
            self.assertEqual(json.loads(receipt_path.read_text())['status'], 'pending')

        address = Path(self.sock)
        parked = address.with_name('parked.sock')
        address.rename(parked)
        try:
            self.assertEqual(self.outbox_status(binding, request).returncode, 1)
            self.assertEqual(self.guarded(binding, request).returncode, 1)
            self.assertEqual(json.loads(receipt_path.read_text())['status'], 'pending')
        finally:
            parked.rename(address)

        event, = self.wait(lambda: self.native_messages(binding, request['request_id']))
        self.assertEqual(offers, [request['request_id']])
        self.assertEqual(event['data']['content'], [
            {'type': 'text', 'text': 'Read [File #1]'},
            {'type': 'file', 'attachment': {'attachmentId': 'sha256:' + hashlib.sha256(data).hexdigest(),
                                            'name': 'changes.txt', 'bytes': len(data)}},
            {'type': 'text', 'text': ' after the marker'},
        ])
        self.assertEqual(self.outbox_status(binding, request, input_sha256='0' * 64).returncode, 1)
        self.assertEqual(self.outbox_status(binding, request, expected_run_id=str(uuid.uuid4())).returncode, 1)
        self.assertEqual(json.loads(receipt_path.read_text())['status'], 'pending')

        def wrong_attachment_proof(message, forward):
            answer = forward(message)
            if message['method'] == 'outbox-proof':
                for row in answer['value']['events']:
                    if row.get('rpcId') == request['request_id'] and row['type'] == 'user/message':
                        row['attachmentIds'] = ['sha256:' + '0' * 64]
            return answer

        with self.archive_rpc_proxy(wrong_attachment_proof):
            self.assertEqual(self.outbox_status(binding, request).returncode, 1)
            self.assertEqual(json.loads(receipt_path.read_text())['status'], 'pending')
        status = self.outbox_status(binding, request)
        self.assertEqual(status.returncode, 0, status.stderr)
        proof = json.loads(status.stdout)
        self.assertEqual(proof['proof'], 'native_dsh_receipt')
        self.assertEqual(proof['native_message_id'], event['data']['id'])
        self.assertEqual(proof['request_id'], request['request_id'])
        self.assertEqual(json.loads(receipt_path.read_text())['status'], 'submitted')
        self.assertEqual(self.guarded(binding, request).stdout, status.stdout)
        self.assertEqual(self.native_messages(binding, request['request_id']), [event])

        detail = self.cli('inspect', binding)
        echo, = [row for row in detail['events'] if row.get('dsh_request_id') == request['request_id']]
        self.assertEqual(echo['dsh_message_id'], proof['native_message_id'])
        attachment, = [row for row in detail['attachment_messages'] if row['message_id'] == request['request_id']]
        self.assertTrue(attachment['dsh_outbox'])
        self.assertEqual(attachment['dsh_message_id'], echo['dsh_message_id'])
        path = self.hgs('attachment', binding['name'], '--request', request['request_id'], '--index', '0',
                        '--conversation', binding['conversation_id'])
        self.assertEqual(path.returncode, 0, path.stderr)
        self.assertEqual(base64.b64decode(json.loads(path.stdout)['data_base64']), data)

        address.rename(parked)
        try:
            self.assertEqual(self.outbox_status(binding, request).stdout, status.stdout)
        finally:
            parked.rename(address)

    def test_final_native_gate_waits_behind_an_earlier_native_queue(self):
        binding = self.create()
        conversation = binding['conversation_id']
        first_native = str(uuid.uuid4())
        wait = self.payload(binding, text='Two distinct equal outbox messages', only_when_idle=True)
        response_seen = []

        def intercept(message, forward):
            answer = forward(message)
            if message['method'] == 'outbox-state' and answer.get('value', {}).get('ready') and not response_seen:
                for request, mode, text in [
                    (str(uuid.uuid4()), 'steer', 'INTERRUPT_WAIT_FIXTURE'),
                    (first_native, 'queue', wait['text']),
                ]:
                    offered = forward({'method': 'send', 'params': {'sessionId': conversation, 'requestId': request,
                                                                     'mode': mode, 'content': [{'type': 'text', 'text': text}]}})
                    self.assertTrue(offered['ok'], offered)
                response_seen.append(first_native)
            return answer

        with self.archive_rpc_proxy(intercept):
            refused = self.guarded(binding, wait)
            self.assertEqual(refused.returncode, 0, refused.stderr)
            receipt = json.loads(refused.stdout)
            self.assertEqual(receipt['status'], 'waiting_for_idle')
            self.assertEqual(receipt['input_sha256'], hashlib.sha256(json.dumps(wait).encode()).hexdigest())
            self.assertEqual(response_seen, [first_native])
        self.assertEqual(self.native_messages(binding, wait['request_id']), [])
        prior, = self.wait(lambda: self.native_messages(binding, first_native), timeout=20)
        self.wait(lambda: self.cli('inspect', binding)['dsh_outbox_ready'])
        second = self.payload(binding, text=wait['text'], only_when_idle=True)
        native_ids = []
        prior_sequence = prior['seq']
        for request in [wait, second]:
            result = self.guarded(binding, request)
            if result.returncode:
                result = self.wait(lambda: status if (status := self.outbox_status(binding, request)).returncode == 0 else None)
            proof = json.loads(result.stdout)
            self.assertEqual(proof['status'], 'submitted')
            native, = self.native_messages(binding, request['request_id'])
            self.assertEqual(proof['native_message_id'], native['data']['id'])
            self.assertGreater(native['seq'], prior_sequence)
            prior_sequence = native['seq']
            native_ids.append(native['data']['id'])
            self.wait(lambda: self.cli('inspect', binding)['dsh_outbox_ready'])
        self.assertEqual(len(set(native_ids + [prior['data']['id']])), 3)

    def test_guarded_inline_image_binds_exact_native_hash(self):
        binding = self.create()
        encoded = 'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAADklEQVR4nGP4DwYMEAoAU7oL9ZisIGcAAAAASUVORK5CYII='
        request = self.payload(binding, text='Inspect [Image #1]', only_when_idle=True, attachments=[
            {'name': 'fixture.png', 'mime': 'image/png', 'reference': '[Image #1]', 'data_base64': encoded},
        ])
        attempt = self.guarded(binding, request)
        if attempt.returncode != 0:
            attempt = self.wait(lambda: result if (result := self.outbox_status(binding, request)).returncode == 0 else None)
        proof = json.loads(attempt.stdout)
        event, = self.native_messages(binding, request['request_id'])
        self.assertEqual(proof['native_message_id'], event['data']['id'])
        self.assertEqual(proof['status'], 'submitted')
        self.assertEqual(event['data']['content'][1]['attachment']['attachmentId'],
                         'sha256:' + hashlib.sha256(base64.b64decode(encoded)).hexdigest())
