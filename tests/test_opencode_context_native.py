"""Opt-in OpenCode context and guarded compaction with keyless loopback data."""
from contextlib import contextmanager
import http.server
from io import BytesIO
import json
import os
import shlex
import shutil
import threading
import time
import unittest
import urllib.request
import uuid
from urllib.parse import urlencode

import test_opencode_native as fixture


class UsageModel(fixture.ContextModel):
    def do_POST(self):
        # Keep the existing real-tool and summary handlers. Append one native
        # OpenAI-compatible usage frame after the finish frame on each call.
        if self.path == '/context-gate':
            return super().do_POST()
        output, input_stream = self.wfile, self.rfile
        payload = input_stream.read(int(self.headers['Content-Length']))
        request = json.loads(payload)
        messages = request.get('messages', [])
        last_user = next((str(m.get('content', '')) for m in reversed(messages) if m.get('role') == 'user'), '')
        summary = last_user.startswith('You MUST summarize the conversation above')
        server = self.server

        class UsageStream:
            def write(self, data):
                if data == b'data: [DONE]\n\n':
                    if summary and server.hold_compact:
                        server.compacting.set()
                        server.compact_release.wait(15)
                    prompt = 1800 if summary else 700 if request['model'] == 'alternate' else 900
                    usage = {'prompt_tokens': prompt, 'completion_tokens': 40, 'total_tokens': prompt + 40,
                             'prompt_tokens_details': {'cached_tokens': 210},
                             'completion_tokens_details': {'reasoning_tokens': 12}}
                    chunk = {'id': 'chatcmpl-fixture', 'object': 'chat.completion.chunk',
                             'model': request['model'], 'choices': [], 'usage': usage}
                    output.write(('data: ' + json.dumps(chunk) + '\n\n').encode())
                return output.write(data)

            def flush(self):
                return output.flush()

        self.wfile, self.rfile = UsageStream(), BytesIO(payload)
        try:
            super().do_POST()
        finally:
            self.wfile, self.rfile = output, input_stream


@unittest.skipUnless(os.environ.get('ZERUS_TEST_OPENCODE_NATIVE') == '1', 'explicit private native qualification required')
class OpenCodeContext(fixture.OpenCodeNative):
    model_handler = UsageModel

    @classmethod
    def prepare(cls):
        super().prepare()
        cls.model.compacting = threading.Event()
        cls.model.compact_release = threading.Event()
        cls.model.hold_compact = False
        tool = cls.root / 'private-bin'
        tool.mkdir()
        tmux = tool / 'tmux'
        executable = shutil.which('tmux')
        if not executable:
            raise RuntimeError('The isolated OpenCode fixture requires tmux')
        tmux.write_text(f'#!/bin/sh\nexec {shlex.quote(executable)} -S {shlex.quote(str(cls.root / "tmux/socket"))} -f /dev/null "$@"\n')
        tmux.chmod(0o700)
        cls.env.update(PATH=str(tool) + os.pathsep + cls.env['PATH'],
                       HTTPS_PROXY='http://127.0.0.1:9', HTTP_PROXY='http://127.0.0.1:9',
                       NO_PROXY='127.0.0.1', ALL_PROXY='http://127.0.0.1:9',
                       ZERUS_LOCAL_FIXTURE_KEY=fixture.FIXTURE_KEY)
        config_path = cls.root / 'xdg-config/opencode/opencode.json'
        previous = json.loads(config_path.read_text())
        provider = previous['provider']['fixture']
        config = {'$schema': 'https://opencode.ai/config.json', 'model': 'fixture/local', 'update': 'disable',
                  'plugins': ['-opencode.provider.ollama', '-opencode.provider.lmstudio', '-opencode.provider.vllm'],
                  'providers': {'fixture': {'name': 'Private keyless fixture',
                      'package': '@opencode/ai/providers/openai-compatible',
                      'settings': provider['options'], 'env': ['ZERUS_LOCAL_FIXTURE_KEY'],
                      'compatibility': {'streamUsage': True}, 'models': provider['models']}}}
        for model in provider['models'].values():
            model['capabilities'] = {'tools': True, 'input': ['text'], 'output': ['text']}
        provider['models']['alternate']['limit'] = {'context': 65536, 'output': 4096}
        workspace = cls.workspace / 'opencode.json'
        config_path.write_text(json.dumps(config))
        workspace.write_text(json.dumps(config))
        standard = cls.root / 'home/.config/opencode'
        standard.mkdir(parents=True)
        (standard / 'opencode.json').write_text(json.dumps(config))
        cls.env['OPENCODE_CONFIG'] = str(workspace)

    def start(self):
        self.model.hold_compact = False
        self.model.compacting.clear()
        self.model.compact_release.clear()
        self.addCleanup(self.model.compact_release.set)
        # Use the official private service directly. No launch-time overlay is
        # needed to exercise usage or the native compaction operation.
        self.opencode('service', 'start')
        query = urlencode({'location[directory]': str(self.workspace)})
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            status, answer = self.api('GET', '/api/model?' + query)
            if status == 200 and any(model.get('providerID') == 'fixture' and model.get('id') == 'local'
                                     for model in answer['data']):
                return self.create()
            self.api('GET', '/api/provider?' + query)
            time.sleep(.1)
        self.fail('The disposable OpenCode location did not load the synthetic model')

    def turn(self, session, text):
        self.hgs(['send', session['name'], '--json'], self.payload(session, text=text))
        return self.wait(session['name'], lambda row: row['phase'] == 'idle'
                         and any(e['type'] == 'UserPromptSubmit' and e['detail'] == text for e in row['events'])
                         and row['session_usage']['context'].get('used') is not None)

    def compact(self, session, payload=None, success=True):
        return self.hgs(['compact-context', session['name'], '--json'],
                        self.payload(session) if payload is None else payload, success=success)

    def assert_settled_receipt(self, session, request, observed):
        terminal = self.compact(session, request)
        self.assertEqual(terminal['status'], 'completed')
        self.assertEqual(terminal['request_id'], request['request_id'])
        self.assertEqual(terminal['native_message_id'], observed['native_message_id'])
        saved = self.state / 'opencode/compact-receipts' / (request['request_id'] + '.json')
        self.assertEqual(json.loads(saved.read_text()), terminal)

    def assert_native_total(self, row, expected):
        usage = row['session_usage']
        native = self.session(row['conversation_id'])['tokens']
        self.assertEqual(usage['totals']['uncached_input'], native['input'])
        self.assertEqual(usage['totals']['cache_read'], native['cache']['read'])
        self.assertEqual(usage['totals']['cache_write'], native['cache']['write'])
        self.assertEqual(usage['totals']['output'], native['output'] + native['reasoning'])
        self.assertEqual(usage['totals']['total'], expected)

    def test_model_limits_and_footprint_across_compaction_and_switch(self):
        first = self.start()
        self.assertEqual(first['session_usage']['context']['limit'], 32768)
        self.assertEqual(first['session_usage']['context']['output_limit'], 2048)
        self.assertIsNone(first['session_usage']['context']['used'])
        self.assertIsNone(first['session_usage']['totals'])
        self.assertFalse(first['compact_context_supported'])
        self.assertFalse(first['clear_context_supported'])
        ready = self.turn(first, 'A completed native model step')
        self.assertEqual(ready['session_usage']['context']['used'], 940)
        self.assertEqual(ready['session_usage']['prompt_cache']['status'], 'unknown')
        self.assert_native_total(ready, 940)
        self.assertTrue(ready['compact_context_supported'])
        for wrong in (self.payload(ready, expected_run_id=str(uuid.uuid4())),
                      self.payload(ready, expected_conversation_id='ses_foreign'),
                      self.payload(ready, request_id='00000000-0000-0000-0000-000000000000')):
            self.compact(ready, wrong, success=False)

        request = self.payload(ready)
        receipt = self.compact(ready, request)
        self.assertEqual(receipt['status'], 'submitted')
        after = self.wait(ready['name'], lambda row: row.get('compact_context_request', {}).get('status') == 'completed')
        self.assertEqual(after['conversation_id'], ready['conversation_id'])
        self.assertEqual(after['compact_context_request']['request_id'], request['request_id'])
        self.assert_settled_receipt(ready, request, receipt)
        self.assertIsNone(after['session_usage']['context']['used'])
        self.assertIsNone(after['session_usage']['prompt_cache'])
        self.assert_native_total(after, 2780)
        path = f"/api/session/{after['conversation_id']}"
        status, page = self.api('GET', path + '/message?limit=25&order=desc')
        self.assertEqual(status, 200)
        self.assertEqual([m['id'] for m in page['data'] if m['type'] == 'compaction'],
                         [receipt['native_message_id']])

        status, _ = self.api('POST', path + '/model', {'model': {'providerID': 'fixture', 'id': 'alternate'}})
        self.assertEqual(status, 204)
        switched = self.hgs(['inspect', after['name']])
        self.assertEqual(switched['settings_model'], 'fixture/alternate')
        self.assertEqual(switched['session_usage']['context']['limit'], 65536)
        self.assertEqual(switched['session_usage']['context']['output_limit'], 4096)
        self.assertIsNone(switched['session_usage']['context']['used'])
        self.assert_native_total(switched, 2780)
        refreshed = self.turn(switched, 'First measured step after compaction and model switch')
        self.assertEqual(refreshed['session_usage']['context']['used'], 740)
        self.assert_native_total(refreshed, 3520)

    @contextmanager
    def proxy(self, *, compact_response=None, without_compact=False, steer_only=False, reported_version=None):
        registration = self.registration()
        original_bytes = self.registration_path().read_bytes()
        origin = registration['url'].rstrip('/')
        requests = []
        receipts = self.state / 'opencode/compact-receipts'
        saved_receipts = receipts.with_name('private-fault-compact-receipts')

        class Proxy(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def forward(self):
                size = int(self.headers.get('Content-Length', '0'))
                if size > 8192:
                    self.send_error(413)
                    return
                body = self.rfile.read(size) if size else None
                compact = self.command == 'POST' and self.path.endswith('/compact')
                requests.append((self.command, self.path))
                headers = {name: self.headers[name] for name in ('Content-Type', 'Authorization') if name in self.headers}
                request = urllib.request.Request(origin + self.path, data=body, headers=headers, method=self.command)
                try:
                    response = urllib.request.urlopen(request, timeout=5)
                except urllib.error.HTTPError as error:
                    response = error
                with response:
                    status, raw = response.status, response.read(32 * 1024 * 1024 + 1)
                if compact and compact_response == 'lost':
                    self.close_connection = True
                    return
                if compact and compact_response == 'unbound-400':
                    status, raw = 400, b'{"message":"unbound refusal after admission"}'
                if compact and compact_response == 'receipt-write-failed' and status == 200:
                    receipts.rename(saved_receipts)
                    receipts.touch(exist_ok=False)
                if (without_compact or steer_only) and self.path == '/openapi.json':
                    description = json.loads(raw)
                    if without_compact:
                        description['paths'].pop('/api/session/{sessionID}/compact', None)
                    else:
                        operation = description['paths']['/api/session/{sessionID}/compact']['post']
                        properties = operation['requestBody']['content']['application/json']['schema']['properties']
                        properties['delivery'] = {'type': 'string', 'enum': ['steer']}
                    raw = json.dumps(description).encode()
                if reported_version and self.path == '/api/info':
                    info = json.loads(raw)
                    info['version'] = reported_version
                    raw = json.dumps(info).encode()
                self.send_response(status)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(raw)))
                self.end_headers()
                self.wfile.write(raw)

            do_GET = do_POST = forward

        with http.server.ThreadingHTTPServer(('127.0.0.1', 0), Proxy) as server:
            serving = threading.Thread(target=server.serve_forever, daemon=True)
            serving.start()
            self.write_registration({**registration, 'url': f'http://127.0.0.1:{server.server_port}'})
            try:
                yield requests
            finally:
                self.registration_path().write_bytes(original_bytes)
                server.shutdown()
                serving.join(2)
                if saved_receipts.is_dir():
                    receipts.unlink()
                    saved_receipts.rename(receipts)

    def test_unknown_compact_is_never_replayed_and_reconciles_exact_id(self):
        first = self.turn(self.start(), 'Measure before the lost compact response')
        self.model.hold_compact = True
        with self.proxy(compact_response='lost') as requests:
            attempt = self.payload(first)
            unknown = self.compact(first, attempt)
            self.assertEqual(unknown['status'], 'uncertain')
            self.assertTrue(self.model.compacting.wait(5), 'OpenCode did not begin the exact compaction')
            self.assertEqual(self.compact(first, attempt), unknown)
            self.compact(first, success=False)
            pending = self.hgs(['inspect', first['name']])
            self.assertFalse(pending['compact_context_supported'])
            self.model.compact_release.set()
            done = self.wait(first['name'], lambda row: row.get('compact_context_request', {}).get('status') == 'completed')
            self.assertEqual(done['compact_context_request']['native_message_id'], unknown['native_message_id'])
            self.assert_settled_receipt(first, attempt, unknown)
            self.assertEqual(sum(method == 'POST' and route.endswith('/compact') for method, route in requests), 1)

    def test_admitted_compact_with_unbound_400_stays_uncertain(self):
        first = self.turn(self.start(), 'Measure before an unbound compact response')
        self.model.hold_compact = True
        with self.proxy(compact_response='unbound-400') as requests:
            attempt = self.payload(first)
            uncertain = self.compact(first, attempt)
            self.assertTrue(self.model.compacting.wait(5), 'The forwarded native compact was admitted')
            self.assertEqual(uncertain['status'], 'uncertain')
            self.assertEqual(self.compact(first, attempt), uncertain)
            self.compact(first, success=False)
            self.model.compact_release.set()
            done = self.wait(first['name'], lambda row: row.get('compact_context_request', {}).get('status') == 'completed')
            self.assertEqual(done['compact_context_request']['native_message_id'], uncertain['native_message_id'])
            self.assert_settled_receipt(first, attempt, uncertain)
            self.assertEqual(sum(method == 'POST' and path.endswith('/compact') for method, path in requests), 1)

    def test_receipt_write_failure_after_offer_returns_typed_uncertainty(self):
        first = self.turn(self.start(), 'Measure before the private receipt storage fault')
        self.model.hold_compact = True
        attempt = self.payload(first)
        with self.proxy(compact_response='receipt-write-failed') as requests:
            uncertain = self.compact(first, attempt)
            self.assertEqual(uncertain['status'], 'uncertain')
            self.assertEqual(uncertain['request_id'], attempt['request_id'])
            self.assertTrue(self.model.compacting.wait(5), 'The exact native compact was admitted')
            self.assertEqual(sum(method == 'POST' and path.endswith('/compact') for method, path in requests), 1)
        self.assertEqual(self.compact(first, attempt), uncertain)
        self.compact(first, success=False)
        self.model.compact_release.set()
        self.wait(first['name'], lambda row: row.get('compact_context_request', {}).get('status') == 'completed')
        self.assert_settled_receipt(first, attempt, uncertain)

    def test_steer_only_operation_cannot_offer_compact(self):
        first = self.turn(self.start(), 'Measure where the compact request cannot queue')
        with self.proxy(steer_only=True) as requests:
            observed = self.hgs(['inspect', first['name']])
            self.assertFalse(observed['compact_context_supported'])
            self.compact(first, success=False)
            self.assertFalse(any(method == 'POST' for method, _ in requests))

    def test_missing_native_compact_capability_keeps_usage(self):
        first = self.turn(self.start(), 'Measure on a service without the optional route')
        with self.proxy(without_compact=True) as requests:
            row = self.hgs(['inspect', first['name']])
            self.assertEqual(row['session_usage']['context']['used'], 940)
            self.assertFalse(row['compact_context_supported'])
            self.assertIn('unavailable', self.compact(row, success=False).stderr)
            self.assertFalse(any(method == 'POST' for method, _ in requests))
        with self.proxy(reported_version='2.0.31') as requests:
            older = self.hgs(['inspect', first['name']])
            self.assertTrue(older['inspection_unavailable'])
            self.assertFalse(older['compact_context_supported'])
            self.assertIn('2.0.31 is not supported', self.compact(first, success=False).stderr)
            self.assertFalse(any(method == 'POST' for method, _ in requests))


def load_tests(_loader, _suite, _pattern):
    # Reuse the private lifecycle without running the inherited suite twice.
    return unittest.TestSuite(OpenCodeContext(name) for name in vars(OpenCodeContext) if name.startswith('test_'))


if __name__ == '__main__':
    unittest.main()
