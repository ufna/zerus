"""Native clear commands over a private tmux socket; never real user sessions."""
import json, subprocess, time, unittest, uuid
import test_input

AGENT = r'''
import os, pathlib, sys, termios, tty, json, uuid, signal
root=pathlib.Path(os.environ['INPUT_FIXTURE']); fd=sys.stdin.fileno(); old=termios.tcgetattr(fd);tty.setraw(fd)
text='';agent='codex'
def show(*args):
 global agent
 agent=(root/'provider').read_text() if (root/'provider').exists() else 'codex'
 if (root/'screen').exists():os.write(1,(root/'screen').read_bytes());return
 row=(' │ > '+text+' '*(29-len(text))+'│') if agent=='kimi' else ('❯ ' if agent=='claude' else '› ')+text
 screen='\x1b[2J\x1b[25;1H'+(' ╭─────────────────────────────────╮' if agent=='kimi' else '──────────────────────────────────')+'\x1b[26;1H'+row+'\x1b[27;1H'+(' ╰─────────────────────────────────╯' if agent=='kimi' else '──────────────────────────────────')+'\x1b[28;1H'+(' Ask When Needed  K3 /fixture' if agent=='kimi' else '')+'\x1b[26;'+str((6 if agent=='kimi' else 3)+len(text))+'H'
 if agent=='codex':screen='\x1b[2J\x1b[27;1H\x1b[48;5;234m› '+text+' '*80+'\x1b[28;1H   \x1b[49m\x1b[29;1H  GPT-6-Astra high · /fixture\x1b[30;1H  ? for shortcuts\x1b[27;'+str(3+len(text))+'H'
 os.write(1,screen.encode())
signal.signal(signal.SIGUSR1,show);os.write(1,b'\x1b[?2004h');show();(root/'agent-pid').write_text(str(os.getpid()))
try:
 while True:
  value=os.read(fd,65536)
  with (root/'received').open('ab') as f:f.write(value)
  if (root/'freeze').exists():continue
  for c in value:
   if c==13:
    if text=='/clear':
     p=next((root/'state').glob('*.json'));r=json.loads(p.read_text());r['conversation_id']=str(uuid.uuid4())
     # Match real hooks: readers must see a complete binding during confirmation.
     staged=p.with_suffix('.next');staged.write_text(json.dumps(r));staged.replace(p);text=''
   else:text+=chr(c)
  show()
finally:termios.tcsetattr(fd,termios.TCSANOW,old)
'''
class ClearContext(unittest.TestCase):
 script=test_input.InputTransport.script
 write_record=test_input.InputTransport.write_record
 tmux=test_input.InputTransport.tmux
 wait_for=test_input.InputTransport.wait_for
 received=test_input.InputTransport.received
 tearDown=test_input.InputTransport.tearDown
 def setUp(self):
  original=test_input.FAKE_AGENT;test_input.FAKE_AGENT=AGENT
  try:test_input.InputTransport.setUp(self)
  finally:test_input.FAKE_AGENT=original
 def clear(self,payload=None,success=True,remote=False):
  payload=payload or dict(request_id=str(uuid.uuid4()),expected_run_id=self.run_id,expected_conversation_id=self.conversation_id)
  r=subprocess.run([str(test_input.HGS),*(['@remote'] if remote else []),'clear-context',self.name,'--json'],env=self.env,input=json.dumps(payload),text=True,capture_output=True,timeout=12)
  if success:self.assertEqual(r.returncode,0,r.stderr);return json.loads(r.stdout)
  self.assertNotEqual(r.returncode,0);return r.stderr
 def test_exact_native_command_and_confirmed_identity(self):
  for agent in ['codex','claude','kimi']:
   with self.subTest(agent=agent):
    import signal
    self.record['agent']=agent;self.write_record();(self.root/'provider').write_text(agent);os=__import__('os');os.kill(self.record['pid'],signal.SIGUSR1);time.sleep(.12)
    before=self.received();self.assertEqual(self.clear()['status'],'confirmed');self.assertEqual(self.received()[len(before):],b'/clear\r')
 def test_stale_busy_draft_and_reused_request_do_not_submit(self):
  p=dict(request_id=str(uuid.uuid4()),expected_run_id='old',expected_conversation_id=self.conversation_id)
  self.clear(p,False);self.assertEqual(self.received(),b'')
  self.record['activity']='busy';self.record['phase']='working';self.write_record();self.clear(success=False);self.assertEqual(self.received(),b'')
  self.record['activity']='idle';self.record['phase']='idle';self.write_record()
  test_input.InputTransport.screen(self,'\x1b[2J\x1b[H› unsent draft', '14:0');self.clear(success=False);self.assertEqual(self.received(),b'')
 def test_uncertain_input_does_not_press_enter_or_retry(self):
  (self.root/'freeze').touch();p=dict(request_id=str(uuid.uuid4()),expected_run_id=self.run_id,expected_conversation_id=self.conversation_id)
  self.assertIn('uncertain',self.clear(p,False));self.assertEqual(self.received(),b'/clear')
  self.clear(p,False);self.assertEqual(self.received(),b'/clear')
if __name__=='__main__':unittest.main()
