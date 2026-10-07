#!/usr/bin/env python3
"""Real Claude TUI against a localhost-only API fixture, never a user's account.
Run with HGS_CLAUDE_TEST_BIN=/absolute/path/to/claude (otherwise skipped).
"""
import hashlib,json,os,shlex,shutil,subprocess,sys,tempfile,threading,time,unittest,uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from pathlib import Path
import test_input as fixtures
import test_questions as questions

CLAUDE=os.environ.get('HGS_CLAUDE_TEST_BIN')
ITEMS=[dict(question='Which release should we prepare?',header='Release',options=[dict(label='Stable',description='Use stable build'),dict(label='Preview',description='Use preview build')]),
 dict(question='Which checks should we run?',header='Checks',multiSelect=True,options=[dict(label='Unit',description='Unit checks'),dict(label='Integration',description='Integration checks'),dict(label='UI',description='UI checks')]),
 dict(question='What else should we know?',header='Notes',options=[dict(label='Nothing',description='No extra notes'),dict(label='Later',description='Discuss later')]),
 dict(question='Which extra checks should we include?',header='Extras',multiSelect=True,options=[dict(label='Mac',description='Check macOS'),dict(label='Linux',description='Check Linux')])]

@unittest.skipUnless(CLAUDE,'Set HGS_CLAUDE_TEST_BIN to test the installed native Claude against a local fake API')
class NativeClaudeQuestions(unittest.TestCase):
 script=fixtures.InputTransport.script
 tmux=fixtures.InputTransport.tmux
 write_record=fixtures.InputTransport.write_record
 inspect=questions.Questions.inspect
 def answer(self,*args,**kwargs):
  try:return questions.Questions.answer(self,*args,**kwargs)
  except AssertionError as error:raise AssertionError(str(error)+"\n"+self.screen()) from error
 hook=questions.Questions.hook
 def wait_for(self,callback):
  for _ in range(250):
   result=callback()
   if result:return result
   time.sleep(.04)
  self.fail('Native fixture timed out:\n'+self.tmux('capture-pane','-p','-t',self.pane,check=False))
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory(prefix='hgs-claude-native-');self.addCleanup(self.temp.cleanup)
  self.root=Path(self.temp.name).resolve();self.bin=self.root/'.local/bin';self.bin.mkdir(parents=True);self.state=self.root/'state';self.state.mkdir()
  work=self.root/'work';work.mkdir();(self.root/'.claude').mkdir()
  self.tool_name=getattr(self,'tool_name','AskUserQuestion');self.tool_input=getattr(self,'tool_input',None);self.approval_fixture=self.tool_name!='AskUserQuestion'
  self.items=ITEMS[:1] if self._testMethodName=='test_single_question_direct_submission' else ITEMS
  (self.root/'.claude.json').write_text(json.dumps(dict(hasCompletedOnboarding=True,bypassPermissionsModeAccepted=True,theme='dark',customApiKeyResponses=dict(approved=['fixture-key'],rejected=[]),projects={str(work):dict(hasTrustDialogAccepted=True)})))
  (self.root/'.claude/settings.json').write_text(json.dumps({'permissions':{'defaultMode':'default' if self.approval_fixture else 'bypassPermissions'}}))
  fixture=self
  class Server(BaseHTTPRequestHandler):
   def log_message(self,*args):pass
   def do_POST(self):
    body=json.loads(self.rfile.read(int(self.headers.get('Content-Length',0))) or '{}')
    asks=any(t.get('name')==fixture.tool_name for t in body.get('tools',[]))
    answered=any(isinstance(m.get('content'),list) and any(b.get('type')=='tool_result' for b in m['content']) for m in body.get('messages',[]))
    blocks=[dict(type='tool_use',id='toolu_native_fixture',name=fixture.tool_name,input=fixture.tool_input or dict(questions=fixture.items))] if asks and not answered else [dict(type='text',text='Fixture complete.')]
    message=dict(id='msg_fixture',type='message',role='assistant',model=body.get('model','claude-sonnet-4-6'),content=blocks,stop_reason='tool_use' if asks and not answered else 'end_turn',stop_sequence=None,usage=dict(input_tokens=100,output_tokens=30))
    self.send_response(200)
    if body.get('stream'):
     self.send_header('Content-Type','text/event-stream');self.end_headers()
     def emit(kind,**data):self.wfile.write(('event: '+kind+'\ndata: '+json.dumps(dict(type=kind,**data))+'\n\n').encode());self.wfile.flush()
     emit('message_start',message=dict(message,content=[],stop_reason=None))
     for i,b in enumerate(blocks):
      emit('content_block_start',index=i,content_block=dict(b,input={}) if b['type']=='tool_use' else dict(b,text=''))
      delta=dict(type='input_json_delta',partial_json=json.dumps(b['input'])) if b['type']=='tool_use' else dict(type='text_delta',text=b['text'])
      emit('content_block_delta',index=i,delta=delta);emit('content_block_stop',index=i)
     emit('message_delta',delta=dict(stop_reason=message['stop_reason'],stop_sequence=None),usage=dict(output_tokens=30));emit('message_stop')
    else:self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(json.dumps(message).encode())
  server=ThreadingHTTPServer(('127.0.0.1',0),Server);self.addCleanup(server.server_close);self.addCleanup(server.shutdown)
  threading.Thread(target=server.serve_forever,daemon=True).start()
  self.socket=self.root/'socket';self.script('tmux','#!/bin/sh\nexec '+shlex.quote(fixtures.TMUX)+' -S '+shlex.quote(str(self.socket))+' -f /dev/null "$@"\n')
  self.env=dict(os.environ,HOME=str(self.root),HGS_STATE_DIR=str(self.state),HGS_CONFIG_DIR=str(self.root/'config'),PATH=str(self.bin)+':'+os.environ['PATH'],HGS_SELF='test')
  for v in ('TMUX','TMUX_PANE','HGS_RUN_ID','HGS_SESSION','HGS_EXECUTABLE'):self.env.pop(v,None)
  self.name='claude/question-test';self.run_id=str(uuid.uuid4())
  self.addCleanup(lambda:subprocess.run([fixtures.TMUX,'-S',str(self.socket),'kill-server'],capture_output=True))
  self.tmux('new-session','-d','-s',self.name,'-x','120','-y','45','-c',str(work),'env','-i','HOME='+str(self.root),'PATH='+os.environ['PATH'],'SHELL=/bin/bash','TERM=xterm-256color','LANG=C.UTF-8','CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1','ANTHROPIC_API_KEY=fixture-key','ANTHROPIC_BASE_URL=http://127.0.0.1:'+str(server.server_port),CLAUDE,*([] if self.approval_fixture else ['--dangerously-skip-permissions']),'--model','claude-sonnet-4-6','Ask questions with the tool now.')
  self.pane=self.tmux('display-message','-p','-t','='+self.name+':','#{pane_id}').strip()
  self.wait_for(lambda:('Do you want to proceed?' if self.approval_fixture else 'Which release should we prepare?') in self.screen())
  transcript=next((self.root/'.claude/projects').glob('*/*.jsonl'));self.conversation_id=transcript.stem
  pid=int(self.tmux('display-message','-p','-t',self.pane,'#{pane_pid}'))
  start=subprocess.check_output(['ps','-p',str(pid),'-o','lstart='],env=dict(self.env,LC_ALL='C',TZ='UTC'),text=True).strip()
  self.tmux('set-option','-t','='+self.name+':','@hgs_run',self.run_id)
  self.record_path=self.state/(hashlib.sha256(self.name.encode()).hexdigest()+'.json')
  self.record=dict(version=1,name=self.name,agent='claude',run_id=self.run_id,conversation_id=self.conversation_id,pane=self.pane,pid=pid,process_start=start,activity='busy',phase='input',active_tools={},subagents={},last_event_at=time.time(),created=time.time(),transcript=str(transcript))
  self.write_record();self.hook(dict(hook_event_name='PermissionRequest' if self.approval_fixture else 'PreToolUse',tool_name=self.tool_name,tool_use_id='toolu_native_fixture',tool_input=self.tool_input or dict(questions=self.items)))
  if self.approval_fixture:
   self.card=self.inspect()['pending_questions'][0]
   return
  inspection=self.inspect()
  if not inspection.get('pending_questions'):
   diagnostics={'inspection':inspection,'record':json.loads(self.record_path.read_text()),'live':self.tmux('list-panes','-a','-F','#{session_name} #{pane_id} #{pane_dead} #{@hgs_run} #{pane_pid}'),'process':subprocess.run(['ps','-p',str(pid),'-o','pid=,lstart=,command='],env=dict(self.env,LC_ALL='C',TZ='UTC'),capture_output=True,text=True).stdout,'cli':str(fixtures.HGS)}
   self.fail(json.dumps(diagnostics,ensure_ascii=False))
  self.card=inspection['pending_questions'][0]
 def screen(self):return self.tmux('capture-pane','-p','-t',self.pane,'-J')
 def key(self,key):self.tmux('send-keys','-t',self.pane,key);time.sleep(.15)
 def enable_gutter_capture(self):
  # Reproduce the dim gutter observed in the user's Claude terminal
  # on captures only; all input, transcript acknowledgements and UI state still
  # come from the real CLI against our isolated localhost API.
  self.script('tmux','#!'+sys.executable+'\n'+f'TMUX={fixtures.TMUX!r}\nSOCKET={str(self.socket)!r}\nROOT={str(self.root)!r}\n'+r'''
import json,pathlib,re,subprocess,sys
args=sys.argv[1:]
if args and args[0]=='send-keys':
 with (pathlib.Path(ROOT)/'sent-keys').open('a') as log:log.write(json.dumps(args)+'\n')
result=subprocess.run([TMUX,'-S',SOCKET,'-f','/dev/null',*args],stdout=subprocess.PIPE)
if args and args[0]=='capture-pane' and result.returncode==0:
 rows=re.sub(r'\x1b\[[0-9;:]*m','',result.stdout.decode()).splitlines()
 starts=[i for i,r in enumerate(rows) if r.strip().startswith('←') and '✔ Submit' in r]
 if starts:
  start=starts[-1]+1; reviewing=any(r.strip()=='Review your answers' for r in rows[start:])
  question=False
  for i in range(start,len(rows)):
   row=rows[i].strip()
   if reviewing:
    if row=='Ready to submit your answers?':break
    if row.startswith('● '):question=True
    elif row.startswith('→ '):question=False
    if row and question:rows[i]='\x1b[2m│\x1b[0m '+rows[i]
   else:
    if re.match(r'^(?:❯\s*)?\d+\. ',row):break
    if row:rows[i]='\x1b[2m│\x1b[0m '+rows[i]
  with (pathlib.Path(ROOT)/'gutter-observed').open('a') as log:log.write(('review' if reviewing else 'question')+'\n')
  if reviewing and (pathlib.Path(ROOT)/'changed-review').exists():
   rows=[r.replace('Which release should we prepare?','A different question?') for r in rows]
 sys.stdout.write('\n'.join(rows)+'\n')
else:sys.stdout.buffer.write(result.stdout)
sys.exit(result.returncode)
''')
 def payload(self,**changes):
  answers=[dict(question_id='q_0',selected_option_ids=['opt_0_1'],text=''),dict(question_id='q_1',selected_option_ids=['opt_1_0','opt_1_2'],text=''),dict(question_id='q_2',selected_option_ids=[],text='Привет from Zerus'),dict(question_id='q_3',selected_option_ids=['opt_3_1'],text='Browser check')][:len(self.items)]
  value=dict(request_id=str(uuid.uuid4()),expected_run_id=self.run_id,expected_conversation_id=self.conversation_id,question_id=self.card['question_id'],expected_question_hash=self.card['question_hash'],answers=answers);value.update(changes);return value
 def test_tabs_multi_other_and_exact_native_receipt(self):
  self.assertTrue(self.card['can_answer'],self.card['answer_unavailable_reason'])
  payload=self.payload();receipt=self.answer(payload);self.assertEqual(receipt['status'],'answered')
  self.assertEqual(self.answer(payload),receipt)
  self.assertIn('Fixture complete.',self.wait_for(lambda:self.screen() if 'Fixture complete.' in self.screen() else ''))
 def test_gutter_on_questions_and_review_confirms_exact_native_receipt(self):
  self.enable_gutter_capture()
  self.assertTrue(self.inspect()['pending_questions'][0]['can_answer'])
  payload=self.payload();receipt=self.answer(payload);self.assertEqual(receipt['status'],'answered')
  self.assertEqual(self.answer(payload),receipt)
  observed=(self.root/'gutter-observed').read_text().splitlines()
  self.assertIn('question',observed);self.assertIn('review',observed)
 def test_retry_from_gutter_review_keeps_exact_choices_and_confirms_once(self):
  self.enable_gutter_capture()
  (self.root/'changed-review').touch();first=self.payload()
  self.assertIn('delivery uncertain',self.answer(first,success=False))
  (self.root/'changed-review').unlink()
  before=(self.root/'sent-keys').read_text().splitlines()
  self.assertIn('may already be in Terminal',self.answer(first,success=False))
  self.assertEqual(before,(self.root/'sent-keys').read_text().splitlines())
  retry=dict(first,request_id=str(uuid.uuid4()))
  receipt=self.answer(retry);self.assertEqual(receipt['status'],'answered')
  keys=(self.root/'sent-keys').read_text().splitlines()[len(before):]
  self.assertEqual([json.loads(row)[-1] for row in keys],['Enter'])
  self.assertEqual(self.answer(retry),receipt)
  self.assertEqual(keys,(self.root/'sent-keys').read_text().splitlines()[len(before):])
 def test_gutter_review_can_change_preselected_single_choices(self):
  self.enable_gutter_capture()
  self.key('2');self.key('1');self.key('3');self.key('Tab')
  self.key('1');self.key('2');self.key('Tab')
  self.wait_for(lambda:'Review your answers' in self.screen())
  payload=self.payload();payload['answers'][0]['selected_option_ids']=['opt_0_0']
  payload['answers'][2]=dict(question_id='q_2',selected_option_ids=['opt_2_1'],text='')
  payload['answers'][3]=dict(question_id='q_3',selected_option_ids=['opt_3_1'],text='')
  receipt=self.answer(payload);self.assertEqual(receipt['status'],'answered')
  self.assertEqual(self.answer(payload),receipt)
 def test_single_question_direct_submission(self):
  self.assertTrue(self.card['can_answer'],self.card['answer_unavailable_reason']);self.assertEqual(self.answer()['status'],'answered')
 def test_stale_request_sends_no_input(self):
  before=self.screen();self.answer(self.payload(expected_question_hash='0'*64),success=False);self.assertEqual(before.partition("←")[2],self.screen().partition("←")[2])
 def test_preselected_options_are_reconciled_in_order(self):
  self.key('Tab');self.key('3');self.key('1');self.key('2');self.key('Left')
  self.assertEqual(self.answer()['status'],'answered')
 def test_foreign_other_draft_is_preserved(self):
  self.key('3');self.key('x');before=self.screen();card=self.inspect()['pending_questions'][0]
  self.assertFalse(card['can_answer']);self.answer(success=False);self.assertEqual(self.screen().partition("←")[2],before.partition("←")[2])
 def test_missing_native_receipt_is_uncertain_and_never_retried(self):
  other=self.root/'unconfirmed';other.mkdir();path=other/(self.conversation_id+'.jsonl');path.write_text('')
  self.record=json.loads(self.record_path.read_text())
  self.record['transcript']=str(path);self.write_record();payload=self.payload()
  self.assertIn('delivery uncertain',self.answer(payload,success=False))
  self.assertIn('may already be in Terminal',self.answer(payload,success=False))
  self.assertNotIn('Review your answers',self.screen())

if __name__=='__main__':unittest.main()
