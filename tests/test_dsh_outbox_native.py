"""Guarded Desktop outbox against the sealed official DeepSeek harness."""
import base64
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import time
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

    def test_busy_desktop_offer_appends_exact_files_and_recovers_lost_ack(self):
        binding = self.create()
        conversation = binding['conversation_id']
        release = self.root / 'holds/OUTBOX_HELD_TURN_FIXTURE.release'
        started = release.with_suffix('.started')
        release.unlink(missing_ok=True)
        started.unlink(missing_ok=True)
        self.addCleanup(release.touch)
        self.cli('send', binding, self.payload(binding, text='OUTBOX_HELD_TURN_FIXTURE'))
        self.wait(started.exists)
        busy = self.cli('inspect', binding)
        self.assertEqual(busy['activity'], 'busy')
        self.assertTrue(busy['dsh_busy_outbox_supported'])
        self.assertTrue(busy['dsh_busy_outbox_ready'])
        earlier = str(uuid.uuid4())
        self.rpc('send', {'sessionId': conversation, 'requestId': earlier, 'mode': 'queue',
                          'content': [{'type': 'text', 'text': 'Earlier native next-turn entry'}]})
        data = b'Original synthetic file bytes while DeepSeek is busy.\n'
        first = self.payload(binding, text='Read [File #1]', only_when_idle=True,
                             attachments=[{'name': 'fixture.txt', 'mime': 'text/plain', 'reference': '[File #1]',
                                           'data_base64': base64.b64encode(data).decode()}])
        second = self.payload(binding, text='Second quietly queued desktop input', only_when_idle=True)

        def queued(request):
            return self.hgs('send', binding['name'], '--when-idle', '--deepseek-queue', '--json', request=request)

        def pending(request_id):
            return self.rpc('outbox-proof', {'sessionId': conversation, 'requestId': request_id,
                            'generation': self.rpc('ping')['generation'], 'queue': True, 'waitMs': 0})['queued']

        offers = []

        def intercept(message, forward):
            if message['method'] == 'outbox-queue-send':
                self.assertEqual(message['params']['mode'], 'queue')
                real = forward(message)
                self.assertTrue(real['ok'], real)
                self.assertTrue(real['value']['accepted'], real)
                offers.append(message['params']['requestId'])
                return None
            return None if message['method'] == 'outbox-proof' else forward(message)

        changed = dict(second, expected_run_id=str(uuid.uuid4()))
        self.assertNotEqual(queued(changed).returncode, 0)
        with self.archive_rpc_proxy(intercept):
            unknown = queued(first)
            self.assertEqual(unknown.returncode, 1, unknown.stdout)
            self.assertIn('delivery uncertain', unknown.stderr.lower())
            self.assertEqual(queued(first).returncode, 1)
        self.assertEqual(offers, [first['request_id']])
        recovered = self.outbox_status(binding, first)
        self.assertEqual(recovered.returncode, 0, recovered.stderr)
        one = json.loads(recovered.stdout)
        self.assertTrue(one['queued'])
        self.assertEqual(json.loads(queued(first).stdout), one)
        result = queued(second)
        self.assertEqual(result.returncode, 0, result.stderr)
        two = json.loads(result.stdout)
        self.assertTrue(two['queued'])
        order = [pending(request_id) for request_id in (earlier, first['request_id'], second['request_id'])]
        self.assertEqual([p['seq'] for p in order], sorted({p['seq'] for p in order}))
        self.assertEqual([p['id'] for p in order[1:]], [one['native_message_id'], two['native_message_id']])
        self.assertEqual(order[1]['attachmentIds'], ['sha256:' + hashlib.sha256(data).hexdigest()])
        self.assertEqual(self.native_messages(binding, first['request_id']), [])
        self.assertEqual(self.native_messages(binding, second['request_id']), [])
        self.assertFalse(release.exists())
        release.touch()
        event, = self.wait(lambda: self.native_messages(binding, first['request_id']))
        self.assertEqual(event['data']['id'], one['native_message_id'])
        self.wait(lambda: self.native_messages(binding, second['request_id']))
        attachment = self.hgs('attachment', binding['name'], '--request', first['request_id'], '--index', '0',
                              '--conversation', conversation)
        self.assertEqual(attachment.returncode, 0, attachment.stderr)
        self.assertEqual(base64.b64decode(json.loads(attachment.stdout)['data_base64']), data)

    def test_busy_desktop_offer_preserves_the_original_native_recovery(self):
        binding = self.create()
        conversation = binding['conversation_id']
        recovery = self.state / 'recovery'
        recovery.mkdir(exist_ok=True)
        policy, heartbeat = recovery / 'policy.json', recovery / 'heartbeat.json'
        policy.write_text(json.dumps({'enabled': True}))
        heartbeat.write_text(json.dumps({'enabled': True, 'at': time.time()}))
        self.addCleanup(policy.unlink, missing_ok=True)
        self.addCleanup(heartbeat.unlink, missing_ok=True)
        self.cli('send', binding, self.payload(binding, text='PROVIDER_CAPACITY_FIXTURE'))
        pending = self.wait(lambda: self.rpc('inspect', {'sessionId': conversation})['nativeRecovery'])
        try:
            detail = self.cli('inspect', binding)
            self.assertTrue(detail['dsh_busy_outbox_supported'])
            self.assertFalse(detail['dsh_busy_outbox_ready'])
            request = self.payload(binding, text='Another next-turn input', only_when_idle=True)
            result = self.hgs('send', binding['name'], '--when-idle', '--deepseek-queue', '--json', request=request)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)['status'], 'waiting_for_idle')
            self.assertEqual(self.rpc('inspect', {'sessionId': conversation})['nativeRecovery']['id'], pending['id'])
            self.assertEqual(self.native_messages(binding, request['request_id']), [])
        finally:
            self.rpc('cancel-recovery', {'sessionId': conversation, 'id': pending['id']})

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

    def test_offer_still_waits_when_work_starts_inside_native_image_admission(self):
        binding = self.create()
        conversation = binding['conversation_id']
        gate = self.root / 'holds' / 'OUTBOX_IMAGE_ADMISSION_HOLD'
        gate.touch()
        encoded = 'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAADklEQVR4nGP4DwYMEAoAU7oL9ZisIGcAAAAASUVORK5CYII='
        guarded = self.payload(binding, text='Inspect [Image #1]', only_when_idle=True, attachments=[
            {'name': 'fixture.png', 'mime': 'image/png', 'reference': '[Image #1]', 'data_base64': encoded},
        ])
        host_work = str(uuid.uuid4())
        earlier = str(uuid.uuid4())
        with ThreadPoolExecutor(max_workers=1) as pool:
            sent = pool.submit(self.guarded, binding, guarded)
            try:
                self.wait(lambda: gate.with_suffix('.resolving').exists())
                for request, mode, text in [(host_work, 'steer', 'OUTBOX_HELD_TURN_FIXTURE'),
                                            (earlier, 'queue', 'Earlier native fixture activity')]:
                    result = self.rpc('send', {'sessionId': conversation, 'requestId': request,
                                               'mode': mode, 'content': [{'type': 'text', 'text': text}]})
                    self.assertTrue(result['accepted'])
                self.assertFalse(self.rpc('outbox-state', {'sessionId': conversation})['ready'])
                gate.unlink()
                result = sent.result(timeout=20)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(result.stdout)['status'], 'waiting_for_idle')
                self.assertEqual(self.native_messages(binding, guarded['request_id']), [])
            finally:
                gate.unlink(missing_ok=True)
                (self.root / 'holds/OUTBOX_HELD_TURN_FIXTURE.release').touch()
        self.wait(lambda: self.native_messages(binding, earlier))
        self.wait(lambda: self.rpc('outbox-state', {'sessionId': conversation})['ready'])
        self.assertEqual(self.native_messages(binding, guarded['request_id']), [])

    def test_lost_file_preparation_reply_is_a_bound_pre_offer_wait(self):
        binding = self.create()
        guarded = self.payload(binding, text='Read [File #1]', only_when_idle=True,
                               attachments=[{'name': 'fixture.txt', 'mime': 'text/plain', 'reference': '[File #1]',
                                             'data_base64': 'dGVzdA=='}])
        calls = []

        def intercept(request, forward):
            calls.append(request['method'])
            if request['method'] == 'prepare-files':
                result = forward(request)
                self.assertTrue(result['ok'], result)
                return None
            return forward(request)

        with self.archive_rpc_proxy(intercept):
            wait = self.guarded(binding, guarded)
            self.assertEqual(wait.returncode, 0, wait.stderr)
            receipt = json.loads(wait.stdout)
            self.assertEqual(receipt['status'], 'waiting_for_idle')
            self.assertEqual(receipt['input_sha256'], hashlib.sha256(json.dumps(guarded).encode()).hexdigest())
        self.assertEqual(calls, ['outbox-state', 'prepare-files'])
        self.assertFalse((self.state / 'dsh/receipts' / f"{guarded['request_id']}.json").exists())
        self.assertEqual(self.native_messages(binding, guarded['request_id']), [])
        forwarded = self.guarded(binding, guarded)
        if forwarded.returncode:
            forwarded = self.wait(lambda: proof if (proof := self.outbox_status(binding, guarded)).returncode == 0 else None)
        self.assertEqual(json.loads(forwarded.stdout)['status'], 'submitted')
        self.assertEqual(len(self.native_messages(binding, guarded['request_id'])), 1)

        invalid = self.payload(binding, text='Invalid [File #1]', only_when_idle=True,
                               attachments=[{'name': 'invalid.txt', 'mime': 'text/plain', 'reference': '[File #1]',
                                             'data_base64': 'this is not base64!'}])
        self.wait(lambda: self.rpc('outbox-state', {'sessionId': binding['conversation_id']})['ready'])
        rejected = self.guarded(binding, invalid)
        self.assertEqual(rejected.returncode, 0, rejected.stderr)
        receipt = json.loads(rejected.stdout)
        self.assertEqual(receipt['status'], 'not_offered')
        self.assertIn('attachment data', receipt['reason'].lower())
        self.assertEqual(receipt['request_id'], invalid['request_id'])
        self.assertFalse((self.state / 'dsh/receipts' / f"{invalid['request_id']}.json").exists())


# Share the official native fixture while leaving its original tests in their
# own module. Both unittest and the CI job splitter discover inherited methods.
for _test in vars(native.NativeHarness):
    if _test.startswith('test_'):
        setattr(NativeDeepSeekOutbox, _test, None)
