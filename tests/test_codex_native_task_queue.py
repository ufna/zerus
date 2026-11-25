"""Private Codex queue fixtures and keyless native MCP hook coverage.

The mocked MCP tool returns a synthetic id and creates no task or agent.
"""
import http.server
import json
import os
from pathlib import Path
import shlex
import threading
import time
import unittest
import uuid

import test_clear_context as clear
import test_clear_context_native as native
import test_codex_async_questions_native as wire
import test_codex_input_native as input_native
import test_input


QUEUE_COMMANDS = r'''
    if (argc >= 2 && (!strcmp(argv[1], "--version") || !strcmp(argv[1], "queue"))) {
        const char *home = getenv("CODEX_HOME");
        const char *expected = getenv("FIXTURE_QUEUE_HOME");
        const char *receipt = getenv("FIXTURE_QUEUE_ID");
        const char *log_path = getenv("FIXTURE_QUEUE_LOG");
        FILE *log = log_path ? fopen(log_path, "a") : NULL;
        if (!log) return 3;
        fprintf(log, "%s%s %s\n", argv[1], argc == 3 ? " --help" : "", home ? home : "");
        fclose(log);
        if (!home || !expected || strcmp(home, expected)) return 4;
        if (argc == 2 && !strcmp(argv[1], "--version")) {
            puts("codex-cli 0.169.0");
            return 0;
        }
        if (argc == 3 && !strcmp(argv[2], "--help")) {
            puts("--thread --message --resume");
            return 0;
        }
        if (argc != 6 || strcmp(argv[2], "--thread") || strcmp(argv[4], "--message")
                || strcmp(argv[5], "Exact message for the original private task") || !receipt)
            return 5;
        printf("Queued message %s for thread %s.\n", receipt, argv[3]);
        return 0;
    }
'''


class FakeNativeTaskQueue(unittest.TestCase):
    setUp = test_input.InputTransport.setUp
    tearDown = test_input.InputTransport.tearDown
    script = test_input.InputTransport.script
    write_record = test_input.InputTransport.write_record
    tmux = test_input.InputTransport.tmux
    wait_for = test_input.InputTransport.wait_for
    received = test_input.InputTransport.received
    payload = test_input.InputTransport.payload
    send = test_input.InputTransport.send
    assert_refusal = test_input.InputTransport.assert_refusal

    def prepare_queue(self, *, explicit=False, outside=False, missing=False, linked=False):
        root = self.root.resolve()
        configured = root / ('explicit-home' if explicit else '.codex')
        account = root / 'codex-data' if linked else configured
        history_home = root / 'other-home' if outside or missing else account
        directory = history_home / 'sessions'
        directory.mkdir(parents=True)
        if not missing:
            account.mkdir(exist_ok=True)
        if linked:
            configured.symlink_to(account, target_is_directory=True)
        self.queue_log = self.root / 'queue-invocations'
        self.queue_id = str(uuid.uuid4())
        self.env.update(CODEX_HOME=str(self.root / 'inherited-wrong-home'),
                        FIXTURE_QUEUE_HOME=str(account), FIXTURE_QUEUE_LOG=str(self.queue_log),
                        FIXTURE_QUEUE_ID=self.queue_id)
        source = clear.RESUMED_CODEX.replace('#include <stdio.h>',
                                             '#include <stdio.h>\n#include <stdlib.h>')
        marker = 'int main(int argc, char **argv) {'
        source = source.replace(marker, marker + QUEUE_COMMANDS)
        rollout = directory / f'rollout-{self.conversation_id}.jsonl'
        test_input.InputTransport.resumed_unconfirmed(self, native=source, rollout=rollout)
        header = json.loads(rollout.read_text())
        header['payload']['cli_version'] = '0.169.0'
        rollout.write_text(json.dumps(header) + '\n')
        self.record.pop('expected_id')
        self.record.update(
            account_id='test-explicit' if explicit else 'native-codex',
            agent_home=str(configured) if explicit else None,
            cwd=str(self.root), launch_dir=str(self.root), conversation_state='active',
            activity='idle', phase='idle', main_done=True,
            codex_native_tasks=dict(parent=self.conversation_id, run=self.run_id,
                                    threads={str(uuid.uuid4()): True}, pending={}),
        )
        if linked:
            self.record['transcript'] = str(configured / 'sessions' / rollout.name)
        self.write_record()
        return account

    def assert_queued(self, account):
        request = self.payload(text='Exact message for the original private task')
        receipt = self.send(request)
        self.assertEqual(receipt['status'], 'submitted')
        self.assertEqual(receipt['transport'], 'codex_queue')
        self.assertEqual(receipt['conversation_id'], self.conversation_id)
        self.assertEqual(receipt['native_queued_message_id'], self.queue_id)
        self.assertEqual(self.send(request), receipt)
        self.assertEqual(self.queue_log.read_text().splitlines(),
                         [f'{command} {account}' for command in ('--version', 'queue --help', 'queue')])
        self.assertEqual(self.received(), b'')

    def assert_queue_refused(self, reason):
        request = self.payload(text='Exact message for the original private task')
        self.assertIn(reason, self.send(request, success=False))
        self.assert_refusal(request, reason)
        self.assertFalse(self.queue_log.exists(), 'refusal must precede the first Codex invocation')
        self.assertEqual(self.received(), b'')

    def test_native_account_uses_default_for_every_queue_command(self):
        self.assert_queued(self.prepare_queue())

    def test_explicit_account_uses_recorded_home(self):
        self.assert_queued(self.prepare_queue(explicit=True))

    def test_native_account_uses_the_reported_path_through_its_home_symlink(self):
        self.assert_queued(self.prepare_queue(linked=True))

    def test_explicit_relative_home_resolves_from_the_launch_directory(self):
        account = self.prepare_queue(explicit=True)
        self.record['agent_home'] = account.name
        self.write_record()
        self.assert_queued(account)

    def test_native_account_refuses_a_held_transcript_from_another_home(self):
        self.prepare_queue(outside=True)
        self.assert_queue_refused(
            'Codex native task history path does not match the account sessions; no message was queued')

    def test_native_account_refuses_a_transcript_redirected_to_another_home(self):
        account = self.prepare_queue(outside=True)
        (account / 'sessions').symlink_to(self.rollout.parent, target_is_directory=True)
        self.record['transcript'] = str(account / 'sessions' / self.rollout.name)
        self.write_record()
        self.assert_queue_refused(
            'Codex native task history path does not match the account sessions; no message was queued')

    def test_native_account_reports_missing_default_without_invoking_codex(self):
        self.prepare_queue(missing=True)
        self.assert_queue_refused('Codex account home is unavailable; no message was queued')


class Provider(wire.Provider):
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers.get('Content-Length','0'))))
  outputs=[x for x in body.get('input',[]) if x.get('type')=='function_call_output' and x.get('call_id')=='synthetic_mcp']
  self.server.results.extend(outputs)
  self.server.offered.append(sorted(wire.offered(body.get('tools',[])+[t for x in body.get('input',[]) if x.get('type')=='additional_tools' for t in x.get('tools',[])])))
  if outputs:item=dict(type='message',id='fixture_done',role='assistant',status='completed',content=[dict(type='output_text',text='Fixture complete.',annotations=[])])
  else:item=dict(type='function_call',id='fixture_mcp',call_id='synthetic_mcp',namespace='mcp__codex_tui',name='create_thread',arguments='{}',status='completed')
  response=dict(id='resp_fixture',object='response',status='completed',output=[item],usage=dict(input_tokens=1,output_tokens=1,total_tokens=2))
  self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
  for k,v in [('response.created',dict(response=dict(response,status='in_progress',output=[]))),('response.output_item.added',dict(output_index=0,item=dict(item,status='in_progress'))),('response.output_item.done',dict(output_index=0,item=item)),('response.completed',dict(response=response))]:self.emit(k,**v)

@unittest.skipUnless(os.environ.get('HGS_CODEX_TEST_BIN'), 'Requires isolated native Codex')
class NativeTaskQueue(unittest.TestCase):
 native_features='hooks=true\n'
 script=native.NativeCodexClear.script
 write_record=native.NativeCodexClear.write_record
 tmux=native.NativeCodexClear.tmux
 wait_for=native.NativeCodexClear.wait_for
 tearDown=native.NativeCodexClear.tearDown
 fail=native.NativeCodexClear.fail
 screen=input_native.NativeCodexInput.screen
 payload=test_input.InputTransport.payload
 send=test_input.InputTransport.send
 def setUp(self):
  self.server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider)
  self.server.results=[];self.server.offered=[]
  self.addCleanup(self.server.server_close);self.addCleanup(self.server.shutdown)
  threading.Thread(target=self.server.serve_forever,daemon=True).start()
  self.provider_url=f'http://127.0.0.1:{self.server.server_port}/v1'
  native.NativeCodexClear.setUp(self)
 def prepare_native_hooks(self):
  mock=self.root/'mcp.py'
  mock.write_text("import sys,json\nfor line in sys.stdin:\n r=json.loads(line)\n if 'id' not in r:continue\n m=r['method']\n if m=='initialize':v=dict(protocolVersion='2024-11-05',capabilities=dict(tools={}),serverInfo=dict(name='Synthetic native-MCP schema fixture',version='1'))\n elif m=='tools/list':v=dict(tools=[dict(name='create_thread',description='Return a synthetic id; creates no task and runs no agent.',inputSchema=dict(type='object',properties={},additionalProperties=False))])\n elif m=='tools/call':v=dict(content=[dict(type='text',text=json.dumps(dict(threadId='11111111-1111-4111-8111-111111111111')))],isError=False)\n else:v={}\n print(json.dumps(dict(jsonrpc='2.0',id=r['id'],result=v)),flush=True)\n")
  config=self.native_home/'config.toml'
  with config.open('a') as f:f.write('\n[mcp_servers.codex_tui]\ncommand="/usr/bin/python3"\nargs='+json.dumps([str(mock)])+'\n')
  env=dict(HGS_SESSION=self.name,HGS_RUN_ID=self.run_id,HGS_STATE_DIR=str(self.state),HGS_CONFIG_DIR=str(self.root/'config'),HGS_AGENT='codex',PATH=self.env['PATH'])
  command='env '+' '.join(shlex.quote(k+'='+v) for k,v in env.items())+' '+shlex.quote(str(test_input.HGS))+' __state hook'
  (self.native_home/'hooks.json').write_text(json.dumps(dict(hooks={k:[dict(hooks=[dict(type='command',command=command)])] for k in ['SessionStart','PreToolUse','PostToolUse','UserPromptSubmit','Stop']})))

 def until(self,predicate):
  end=time.monotonic()+20
  while time.monotonic()<end:
   value=predicate()
   if value:return value
   time.sleep(.05)
  self.fail('Native task fixture timed out')
 def test_native_mcp_result_keeps_parent_and_send_uses_exact_queue_once(self):
  self.tmux('send-keys','-t',self.pane,'-l','Run the synthetic MCP schema fixture.')
  time.sleep(.5);self.tmux('send-keys','-t',self.pane,'Enter')
  def created():
   r=json.loads(self.record_path.read_text())
   return r if r.get('codex_native_tasks',{}).get('threads',{}).get('11111111-1111-4111-8111-111111111111') and r.get('phase')=='idle' else None
  record=self.until(created)
  self.assertEqual(record['conversation_id'],self.conversation_id)
  self.assertEqual(record['codex_native_tasks']['pending'],{})
  request=self.payload(text='Exact selected task queue fixture')
  receipt=self.send(request)
  self.assertEqual(receipt['transport'],'codex_queue')
  self.assertEqual(receipt['conversation_id'],self.conversation_id)
  self.assertTrue(receipt['native_queued_message_id'])
  self.assertEqual(self.send(request),receipt,'original request UUID never writes the native queue again')
  self.until(lambda:json.loads(self.record_path.read_text()).get('input_seen_at',0)>receipt['submitted_at'])
  self.assertEqual(json.loads(self.record_path.read_text())['conversation_id'],self.conversation_id)
  self.assertNotIn('Exact selected task queue fixture',self.screen().split('Ask Codex to do anything')[-1])
