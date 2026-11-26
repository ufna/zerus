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

    def exact_events(self, binding, request):
        return self.wait(binding['name'], lambda row: any(
            e.get('request_id') == request['request_id'] for e in row['events']))

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
                    schema = json.loads(raw)['paths']
                    operation = schema['/api/session/{sessionID}/prompt']['post']
                    fields = operation['requestBody']['content']['application/json']['schema']['properties']
                    fields['delivery'] = {'type': 'string', 'enum': ['steer']}
                    schema = json.loads(raw) | {'paths': schema}
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
        request = self.attempt(target, 'Run the synthetic regression once')
        with self.prompt_proxy(target, 'unbound') as offered:
            result = self.guarded(target, request, success=False)
            self.assertIn('uncertain', result.stderr.lower())
            self.assertEqual(json.loads(self.receipt_path(request).read_text())['status'], 'pending')
            self.guarded(target, request, success=False)
            self.assertEqual(len(offered), 1, offered)
        body, status = offered[0]
        self.assertEqual(status, 200)
        observed = self.exact_events(target, request)
        self.assertIn(body['id'], [e['message_id'] for e in observed['events']
                                  if e.get('request_id') == request['request_id']])
        self.assertEqual(self.guarded(target, request)['native_message_id'], body['id'])
        self.assertEqual(self.user_messages(target['conversation_id']), [(body['id'], request['text'])])

    def test_native_prompt_schema_must_accept_queue_on_the_actual_operation(self):
        target = self.start()
        request = self.attempt(target, 'This must stay local')
        with self.prompt_proxy(target, 'steer_only') as offered:
            details = self.hgs(['inspect', target['name']])
            self.assertFalse(details.get('guarded_idle_send'))
            self.assertFalse(details['attachments_supported'])
            self.guarded(target, request, success=False)
            self.assertEqual(offered, [])
        self.assertFalse(self.receipt_path(request).exists())
        self.assertEqual(self.user_messages(target['conversation_id']), [])
