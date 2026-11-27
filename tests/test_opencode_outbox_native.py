"""Guarded Desktop Send against a private OpenCode service and a keyless model."""
from contextlib import contextmanager
import hashlib
import http.server
import json
import threading
import urllib.error
import urllib.request

import test_opencode_context_native as fixture


class OpenCodeOutbox(fixture.OpenCodeContext):
    def guarded(self, binding, request, success=True):
        return self.hgs(['send', binding['name'], '--when-idle', '--json'], request, success=success)

    def attempt(self, binding, text, **fields):
        return self.payload(binding, text=text, only_when_idle=True, **fields)

    def status(self, binding, request, success=True, **overrides):
        query = {
            'request_id': request['request_id'], 'expected_run_id': request['expected_run_id'],
            'expected_conversation_id': request['expected_conversation_id'],
            'input_sha256': hashlib.sha256(json.dumps(request).encode()).hexdigest(),
        } | overrides
        return self.hgs(['delivery', 'outbox-status', binding['name'], '--json'], query, success=success)

    def exact_events(self, binding, request):
        return self.wait(binding['name'], lambda row: any(
            e.get('request_id') == request['request_id'] for e in row['events']))

    def test_busy_desktop_offer_appends_exact_files_and_recovers_lost_ack(self):
        target, other = self.start(), self.create()
        conversation = target['conversation_id']

        def queued(request, success=True):
            return self.hgs(['delivery', 'outbox-offer', target['name'], '--session-id', target['session_id'],
                             '--opencode-queue', '--json'], request, success=success)

        running = self.hgs(['send', target['name'], '--json'], self.payload(target, text='HOLD_NATIVE'))
        self.assertTrue(self.model.holding.wait(15))
        busy = self.wait(target['name'], lambda row: row['phase'] == 'working')
        self.assertTrue(busy['opencode_busy_outbox_supported'])
        earlier = self.hgs(['send', target['name'], '--json'], self.payload(target, text='Earlier native entry'))
        data = 'Zml4dHVyZSBmaWxlIGJvZHk='
        first = self.attempt(target, 'Read [File #1]', attachments=[{
            'name': 'fixture.txt', 'mime': 'text/plain', 'reference': '[File #1]', 'data_base64': data,
        }])
        second = self.attempt(target, 'Second quietly queued desktop input')
        queued(dict(second, expected_conversation_id=other['conversation_id']), success=False)
        self.hgs(['delivery', 'outbox-offer', target['name'], '--session-id', other['session_id'],
                  '--opencode-queue', '--json'], second, success=False)
        self.hgs(['__state', 'delivery', 'outbox-offer', target['name'], '--opencode-queue', '--json'],
                 second, success=False)
        with self.prompt_proxy(target, 'unbound') as offers:
            queued(first, success=False)
            self.status(target, first, success=False)
            self.assertEqual(len(offers), 1)
            native_first = offers[0][0]['id']
        recovered = self.status(target, first)
        self.assertEqual((recovered['status'], recovered['native_message_id']), ('submitted', native_first))
        self.assertEqual(queued(first)['native_message_id'], native_first)
        later = queued(second)
        self.assertEqual((later['status'], later['queued']), ('submitted', True))
        ids = [earlier['native_message_id'], native_first, later['native_message_id']]
        self.assertEqual([item['id'] for item in self.inbox(conversation)], ids)
        self.assertEqual(self.user_messages(conversation), [(running['native_message_id'], 'HOLD_NATIVE')])
        self.assertEqual(self.user_messages(other['conversation_id']), [])
        self.assertFalse(self.model.release.is_set())
        self.model.release.set()
        self.wait(target['name'], lambda row: row['phase'] == 'idle'
                  and any(event.get('message_id') == ids[-1] for event in row['events']))
        self.assertEqual([item[0] for item in reversed(self.user_messages(conversation))],
                         [running['native_message_id'], *ids])
        status, message = self.api('GET', f'/api/session/{conversation}/message/{native_first}')
        self.assertEqual(status, 200)
        self.assertEqual(message['data']['files'], [{'data': data, 'mime': 'text/plain',
                         'source': {'type': 'inline'}, 'name': 'fixture.txt'}])

    def test_busy_admission_preserves_native_queue_and_exact_attachment_history(self):
        target, peer = self.start(), self.create()
        conversation = target['conversation_id']
        held = self.hgs(['send', target['name'], '--json'], self.payload(target, text='HOLD_NATIVE'))
        self.assertTrue(self.model.holding.wait(15))
        busy = self.wait(target['name'], lambda row: row['phase'] == 'working')
        # Verify that an ordinary unchanged native CLI queue still works while busy.
        native = self.hgs(['send', target['name'], '--json'],
                          self.payload(target, text='Older native queue entry'))
        self.assertTrue(native['queued'])
        self.assertTrue(busy['guarded_idle_send'])
        self.assertTrue(busy['attachments_supported'])
        self.assertTrue(busy['previous_message_pending'])
        requests = [self.attempt(target, 'Same text') for _ in range(2)]
        data = 'Zml4dHVyZSBmaWxlIGJvZHk='
        requests.append(self.attempt(target, 'Inspect [File #1]', attachments=[{
            'name': 'fixture.txt', 'mime': 'text/plain', 'reference': '[File #1]', 'data_base64': data,
        }]))
        for attempt in requests:
            wait = self.guarded(target, attempt)
            self.assertEqual(wait, {
                'schema': 1, 'status': 'waiting_for_idle', 'proof': 'native_opencode_queue',
                'request_id': attempt['request_id'], 'name': target['name'], 'run_id': target['run_id'],
                'conversation_id': conversation,
                'input_sha256': hashlib.sha256(json.dumps(attempt).encode()).hexdigest(),
            })
            self.assertFalse(self.receipt_path(attempt).exists())
        changed = dict(requests[0], expected_conversation_id=peer['conversation_id'])
        self.guarded(target, changed, success=False)
        self.assertEqual([item['id'] for item in self.inbox(conversation)], [native['native_message_id']])
        self.model.release.set()

        offered = []
        for attempt in requests:
            self.wait(target['name'], lambda row: row['phase'] == 'idle')
            receipt = self.guarded(target, attempt)
            self.assertEqual(receipt['status'], 'submitted', receipt)
            self.assertEqual(receipt['request_id'], attempt['request_id'])
            self.assertEqual(receipt['conversation_id'], conversation)
            self.assertEqual(receipt['run_id'], target['run_id'])
            self.assertEqual(receipt['transport'], 'native_opencode')
            self.assertEqual(receipt, self.guarded(target, attempt))
            offered.append(receipt['native_message_id'])
        latest = self.wait(target['name'], lambda row: row['phase'] == 'idle'
                           and any(e.get('message_id') == offered[-1] for e in row['events']))
        ids = [held['native_message_id'], native['native_message_id'], *offered]
        self.assertEqual(list(reversed(self.user_messages(conversation))), list(zip(
            ids, ['HOLD_NATIVE', 'Older native queue entry', 'Same text', 'Same text', 'Inspect [File #1]'])))
        self.assertEqual(len(set(offered)), 3)
        self.assertEqual([e['request_id'] for e in latest['events'] if e.get('message_id') in offered],
                         [a['request_id'] for a in requests])
        self.assertNotIn(data, json.dumps(latest))
        self.assertEqual(self.user_messages(peer['conversation_id']), [])
        status, message = self.api('GET', f"/api/session/{conversation}/message/{offered[-1]}")
        self.assertEqual(status, 200, message)
        self.assertEqual(message['data']['files'], [{'data': data, 'mime': 'text/plain',
                         'source': {'type': 'inline'}, 'name': 'fixture.txt'}])

    @contextmanager
    def prompt_proxy(self, target, mode):
        registration = self.registration()
        original = self.registration_path().read_bytes()
        origin = registration['url'].rstrip('/')
        path = f"/api/session/{target['conversation_id']}/prompt"
        offered = []

        class Proxy(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def forward(self):
                length = int(self.headers.get('Content-Length', 0))
                if length > 30000000:
                    self.send_error(413)
                    return
                body = self.rfile.read(length) if length else None
                headers = {key: self.headers[key] for key in ('Authorization', 'Content-Type')
                           if key in self.headers}
                request = urllib.request.Request(origin + self.path, data=body, headers=headers, method=self.command)
                try:
                    response = urllib.request.urlopen(request, timeout=5)
                except urllib.error.HTTPError as error:
                    response = error
                with response:
                    status, raw = response.status, response.read(32 * 1024 * 1024 + 1)
                if self.path == '/openapi.json' and mode == 'steer_only':
                    schema = json.loads(raw)
                    operation = schema['paths']['/api/session/{sessionID}/prompt']['post']
                    fields = operation['requestBody']['content']['application/json']['schema']['properties']
                    fields['delivery'] = {'type': 'string', 'enum': ['steer']}
                    raw = json.dumps(schema).encode()
                if self.command == 'POST' and self.path == path:
                    offered.append((json.loads(body), status))
                    if mode == 'unbound':
                        status, raw = 400, b'{"message":"response substituted after native admission"}'
                if mode == 'unbound' and (self.path.endswith('/inbox') or '/message/' in self.path):
                    status, raw = 503, b'{}'
                self.send_response(status)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(raw)))
                self.end_headers()
                self.wfile.write(raw)

            do_POST = do_GET = forward

        with http.server.ThreadingHTTPServer(('127.0.0.1', 0), Proxy) as proxy:
            thread = threading.Thread(target=proxy.serve_forever, daemon=True)
            thread.start()
            switched = dict(registration, url=f'http://127.0.0.1:{proxy.server_port}')
            self.registration_path().write_text(json.dumps(switched))
            try:
                yield offered
            finally:
                self.registration_path().write_bytes(original)
                proxy.shutdown()
                thread.join(5)

    def test_unbound_4xx_after_native_admission_remains_pending_and_never_reposts(self):
        target = self.start()
        data = 'Zml4dHVyZSBmaWxlIGJvZHk='
        request = self.attempt(target, 'Run the synthetic regression once [File #1]', attachments=[{
            'name': 'fixture.txt', 'mime': 'text/plain', 'reference': '[File #1]', 'data_base64': data,
        }])
        with self.prompt_proxy(target, 'unbound') as offered:
            result = self.guarded(target, request, success=False)
            self.assertIn('uncertain', result.stderr.lower())
            self.assertEqual(json.loads(self.receipt_path(request).read_text())['status'], 'pending')
            self.status(target, request, success=False)
            self.assertEqual(len(offered), 1, offered)
        body, status = offered[0]
        self.assertEqual(status, 200)
        self.status(target, request, success=False, input_sha256='0' * 64)
        self.status(target, request, success=False, expected_run_id='changed')
        self.assertEqual(json.loads(self.receipt_path(request).read_text())['status'], 'pending')
        receipt = self.status(target, request)
        self.assertEqual(receipt['proof'], 'native_opencode_receipt')
        self.assertEqual(receipt['native_message_id'], body['id'])
        self.assertEqual(json.loads(self.receipt_path(request).read_text())['status'], 'submitted')
        observed = self.exact_events(target, request)
        [event] = [e for e in observed['events'] if e.get('request_id') == request['request_id']]
        self.assertEqual(event['message_id'], body['id'])
        self.assertEqual(event['attachments'][0]['name'], 'fixture.txt')
        attachment = self.hgs(['attachment', target['name'], '--request', request['request_id'],
                               '--index', '0', '--conversation', target['conversation_id'], '--json'])
        self.assertEqual(attachment['data_base64'], data)
        self.assertEqual(self.user_messages(target['conversation_id']), [(body['id'], request['text'])])

    def test_native_prompt_schema_must_accept_queue_on_the_actual_operation(self):
        target = self.start()
        request = self.attempt(target, 'This must stay local')
        with self.prompt_proxy(target, 'steer_only') as offered:
            details = self.hgs(['inspect', target['name']])
            self.assertFalse(details.get('guarded_idle_send'))
            self.assertFalse(details.get('opencode_busy_outbox_supported'))
            self.assertFalse(details['attachments_supported'])
            self.guarded(target, request, success=False)
            self.hgs(['delivery', 'outbox-offer', target['name'], '--opencode-queue', '--json'], request, success=False)
            self.assertEqual(offered, [])
        self.assertFalse(self.receipt_path(request).exists())
        self.assertEqual(self.user_messages(target['conversation_id']), [])

    def test_status_uses_the_original_provider_path_after_label_reuse(self):
        target, shadow = self.start(), self.create()
        name = target['name']
        label_path = self.binding_path(name)
        shadow_path = self.binding_path(shadow['name'])
        original, original_shadow = label_path.read_bytes(), shadow_path.read_bytes()
        id_record = dict(json.loads(original), layout='id')
        id_path = self.state / 'opencode/sessions' / (id_record['session_id'] + '.json')
        id_path.parent.mkdir(exist_ok=True)
        try:
            # Fault fixture: both native bindings came from ordinary launch.
            # Put the original in the supported ID layout, then introduce the
            # other real conversation as a later holder of the original label.
            id_path.write_text(json.dumps(id_record))
            id_path.chmod(0o600)
            label_path.unlink()
            request = self.attempt(target, 'Keep the original conversation')
            sent = self.hgs(['send', name, '--session-id', id_record['session_id'],
                             '--when-idle', '--json'], request)
            self.assertEqual(sent['status'], 'submitted')
            shadow_path.unlink()
            label_path.write_text(json.dumps(dict(json.loads(original_shadow), name=name)))
            label_path.chmod(0o600)
            status = self.status(target, request)
            self.assertEqual(status['native_message_id'], sent['native_message_id'])
            self.status(target, request, success=False, expected_conversation_id=shadow['conversation_id'])
            id_path.unlink()
            self.status(target, request, success=False)
            self.assertEqual(self.user_messages(shadow['conversation_id']), [])
            self.assertEqual(self.user_messages(target['conversation_id']),
                             [(sent['native_message_id'], request['text'])])
        finally:
            id_path.unlink(missing_ok=True)
            label_path.write_bytes(original)
            label_path.chmod(0o600)
            shadow_path.write_bytes(original_shadow)
            shadow_path.chmod(0o600)
