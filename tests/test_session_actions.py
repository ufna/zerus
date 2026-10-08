"""Native interrupt and terminal upload contracts on a private raw PTY."""
import base64,json,subprocess,time,unittest,uuid
import test_input as fixtures
HGS=fixtures.HGS

class SessionActions(unittest.TestCase):
    setUp=fixtures.InputTransport.setUp
    tearDown=fixtures.InputTransport.tearDown
    script=fixtures.InputTransport.script
    tmux=fixtures.InputTransport.tmux
    wait_for=fixtures.InputTransport.wait_for
    write_record=fixtures.InputTransport.write_record
    screen=fixtures.InputTransport.screen
    received=fixtures.InputTransport.received
    def request(self,command,data,ok=True):
        args=[str(HGS),command,self.name]
        if command=='attachment':args+=['--stage']
        result=subprocess.run(args+['--json'],input=json.dumps(data),env=self.env,text=True,capture_output=True,timeout=10)
        if ok:self.assertEqual(result.returncode,0,result.stderr)
        else:self.assertNotEqual(result.returncode,0)
        return json.loads(result.stdout) if result.stdout.strip() else result.stderr
    def identity(self):return dict(request_id=str(uuid.uuid4()),expected_run_id=self.run_id,expected_conversation_id=self.conversation_id)
    def working(self,agent):
        self.record.update(agent=agent,run_identity_version=1,supervisor=dict(pid=self.record['pid'],start=self.record['process_start']),phase='working',activity='busy',turn_started=time.time())
        self.write_record()
    def test_interrupt_sends_one_escape_preserves_process_and_draft(self):
        for agent in ('claude','kimi','codex'):
            with self.subTest(agent=agent):
                self.working(agent)
                self.screen('\x1b[2J\x1b[HUnsent terminal draft','21:0')
                before=self.received();data=dict(self.identity(),expected_turn_started=self.record['turn_started'])
                self.request('interrupt',data);self.wait_for(lambda:len(self.received())>len(before))
                self.assertEqual(self.received()[len(before):],b'\x1b')
                self.request('interrupt',data,ok=False);self.assertEqual(self.received()[len(before):],b'\x1b')
                self.assertIn('Unsent terminal draft',self.tmux('capture-pane','-p','-t',self.pane))
    def test_claude_interrupt_is_recorded_once_claude_shows_it(self):
        # Claude emits no hook after Escape; its own transcript line confirms it.
        self.working('claude');self.record.update(active_tools=dict(tool=dict(name='Bash',detail='sleep 30')));self.write_record()
        self.screen('\x1b[2J\x1b[H  \u23bf  Interrupted \u00b7 What should Claude do instead?\r\n','0:1')
        (self.root/'on-escape').write_bytes('\r\n  \u23bf  Interrupted \u00b7 What should Claude do instead?\r\n\u276f '.encode())
        receipt=self.request('interrupt',dict(self.identity(),expected_turn_started=self.record['turn_started']))
        self.assertTrue(receipt['confirmed'])
        state=json.loads(self.record_path.read_text())
        self.assertEqual((state['phase'],state['activity'],state['active_tools']),('interrupted','unknown',{}))
        # An earlier interruption still on screen is not a confirmation.
        (self.root/'on-escape').unlink();self.working('claude')
        receipt=self.request('interrupt',dict(self.identity(),expected_turn_started=self.record['turn_started']))
        self.assertFalse(receipt['confirmed'])
        self.assertEqual(json.loads(self.record_path.read_text())['phase'],'working')
    def test_changed_turn_and_idle_state_receive_nothing(self):
        self.working('codex');data=dict(self.identity(),expected_turn_started=self.record['turn_started']-1)
        self.request('interrupt',data,ok=False)
        data['expected_turn_started']=self.record['turn_started'];self.record.update(activity='idle',phase='idle');self.write_record()
        self.request('interrupt',data,ok=False);self.assertEqual(self.received(),b'')
    def test_upload_pins_run_and_stages_without_pasting_or_submitting(self):
        data=dict(self.identity(),attachments=[dict(name='report \'one.pdf',mime='application/pdf',data_base64=base64.b64encode(b'%PDF fixture').decode())])
        staged=self.request('attachment',data)
        from pathlib import Path
        path=Path(staged['files'][0]['path']);self.assertEqual(path.read_bytes(),b'%PDF fixture')
        self.assertTrue(path.is_relative_to(self.state/'attachments'));self.assertEqual(path.stat().st_mode&0o777,0o600)
        self.assertEqual(self.received(),b'')
        data['expected_run_id']='another';self.request('attachment',data,ok=False);self.assertEqual(self.received(),b'')

if __name__=='__main__':unittest.main()
