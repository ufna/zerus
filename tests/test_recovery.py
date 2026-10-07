"""Durable worker and terminal delivery contracts, with private fake agents only."""
import json
import os
import subprocess
import time
import unittest
from pathlib import Path
import test_input as fixtures

class Recovery(unittest.TestCase):
    setUp=fixtures.InputTransport.setUp
    tearDown=fixtures.InputTransport.tearDown
    script=fixtures.InputTransport.script
    tmux=fixtures.InputTransport.tmux
    wait_for=fixtures.InputTransport.wait_for
    write_record=fixtures.InputTransport.write_record
    screen=fixtures.InputTransport.screen
    received=fixtures.InputTransport.received

    def command(self,*args,payload=None,ok=True):
        result=subprocess.run([str(fixtures.HGS),*args],env=self.env,text=True,capture_output=True,
                              input=json.dumps(payload) if payload is not None else None,timeout=15)
        if ok:self.assertEqual(result.returncode,0,result.stderr)
        else:self.assertNotEqual(result.returncode,0)
        return json.loads(result.stdout) if result.stdout.strip() else result.stderr

    def prepare(self,agent='codex'):
        self.record.update(agent=agent,phase='error',activity='attention',recovery_user_at=1,
            provider_error=dict(type='StopFailure',source='provider_hook',message_id='failure',at=time.time(),detail='503 Service unavailable',error_kind='provider'))
        self.write_record()
        if agent=='claude':self.screen('\x1b[2J\x1b[H❯ ','2:0')
        if agent=='kimi':self.screen('\x1b[2J\x1b[H ╭──────────────╮\r\n │ >          │\r\n ╰──────────────╯\x1b[2;6H','5:1')
        directory=self.state/'recovery';directory.mkdir(exist_ok=True)
        self.policy=directory/'policy.json'
        self.policy.write_text(json.dumps(dict(version=1,revision=1,enabled=True,enabled_at=1,service=True,rate_limit=True,network=True,delays=[15,30,60,300])))

    def inspect(self):return self.command('inspect',self.name)
    def tick(self):self.command('recovery','tick')
    def hook(self,kind,**values):
        result=subprocess.run([str(fixtures.HGS),'__state','hook'],env=dict(self.env,HGS_SESSION=self.name,HGS_RUN_ID=self.run_id),
            input=json.dumps(dict(hook_event_name=kind,session_id=self.conversation_id,**values)),text=True,capture_output=True,timeout=10)
        self.assertEqual(result.returncode,0,result.stderr)

    def exercise(self,agent):
        self.prepare(agent);self.tick();job=self.inspect()['recovery'];self.assertEqual(job['state'],'waiting')
        self.assertEqual(self.received(),b'')
        self.command('recovery','action',payload=dict(name=self.name,id=job['id'],action='now'))
        self.tick();job=self.inspect()['recovery'];self.assertEqual(job['state'],'retrying',job)
        self.assertEqual(job['attempt'],1)
        self.wait_for(lambda:self.received().endswith(b'\r'))
        raw=self.received();self.assertEqual(raw.count(b'\r'),1)
        message=job['receipt']['text'];self.assertIn('[HGS automatic recovery]',message)
        self.hook('UserPromptSubmit',prompt=message);self.tick()
        self.assertTrue(self.inspect()['recovery']['acknowledged'])
        if agent=='codex':
            import sqlite3
            home=self.root/'.codex';home.mkdir(exist_ok=True);at=time.time()
            with sqlite3.connect(home/'logs_2.sqlite') as db:
                db.execute('CREATE TABLE logs(id INTEGER, ts INTEGER, ts_nanos INTEGER, thread_id TEXT, target TEXT, feedback_log_body TEXT)')
                db.execute('INSERT INTO logs VALUES(1,?,?,?,?,?)',(int(at),int((at%1)*1e9),self.conversation_id,'codex_core::session::turn','run_turn: Turn error: 529 Overloaded'))
        else:self.hook('StopFailure',error_message='529 Overloaded',error_type='overloaded_error')
        self.tick()
        following=self.inspect()['recovery'];self.assertEqual(following['state'],'waiting');self.assertEqual(following['attempt'],1)
        self.assertEqual(self.received(),raw)
        self.command('recovery','action',payload=dict(name=self.name,id=job['id'],action='cancel'))
        self.tick();self.assertEqual(self.inspect()['recovery']['state'],'cancelled');self.assertEqual(self.received(),raw)

    def test_codex_delivery_acknowledgement_and_cancel(self):self.exercise('codex')
    def test_claude_delivery_acknowledgement_and_cancel(self):self.exercise('claude')
    def test_kimi_delivery_acknowledgement_and_cancel(self):self.exercise('kimi')

    def test_draft_and_changed_identity_never_receive_recovery(self):
        self.prepare();self.tick();job=self.inspect()['recovery']
        self.screen('\x1b[2J\x1b[H› my unfinished draft',str(len('› my unfinished draft'))+':0')
        self.command('recovery','action',payload=dict(name=self.name,id=job['id'],action='now'))
        self.tick();self.assertEqual(self.inspect()['recovery']['state'],'blocked');self.assertEqual(self.inspect()['recovery']['attempt'],0);self.assertEqual(self.received(),b'')
        self.tick();self.assertEqual(self.received(),b'')

    def test_new_input_cancels_and_stale_action_is_rejected(self):
        self.prepare();self.tick();job=self.inspect()['recovery']
        self.hook('UserPromptSubmit',prompt='User changed the task');self.tick()
        self.assertEqual(self.inspect()['recovery']['state'],'cancelled')
        self.command('recovery','action',payload=dict(name=self.name,id=job['id'],action='now'),ok=False)
        self.assertEqual(self.received(),b'')

    def test_policy_is_off_by_default_and_compare_and_swap_rejects_stale_settings(self):
        policy=self.command('recovery','get')['policy'];self.assertFalse(policy['enabled'])
        result=self.command('recovery','set',payload=policy);self.assertEqual(result['policy']['revision'],1)
        self.assertIn('changed',self.command('recovery','set',payload=policy,ok=False))
        self.assertEqual(self.received(),b'')

    def test_shared_sync_converges_and_stale_writes_cannot_overwrite(self):
        first=self.command('recovery','get')['policy']
        first['schedules']={'service':[3,7,-1],'network':[5,20,0],'rate_limit':[-1]}
        saved=self.command('recovery','set',payload=first)['policy']
        self.assertEqual(saved['version'],2)
        incoming=dict(saved,revision=saved['revision']+1,writer='remote-writer',service=False)
        result=self.command('recovery','sync',payload=incoming)
        self.assertEqual(result['policy']['writer'],'remote-writer')
        self.assertFalse(result['policy']['service'])
        stale=self.command('recovery','sync',payload=saved)['policy']
        self.assertEqual(stale['writer'],'remote-writer')
        self.command('recovery','set',payload=saved,ok=False)
        peer=dict(incoming,writer='zz-peer')
        self.assertEqual(self.command('recovery','sync',payload=peer)['policy']['writer'],'zz-peer')
        self.assertEqual(self.command('recovery','sync',payload=incoming)['policy']['writer'],'zz-peer')
        for values in ([0],[1,0,2],[1,-1,3],[-2],[86401]):
            invalid=dict(peer,schedules={'network':values})
            self.command('recovery','set',payload=invalid,ok=False)
        self.assertEqual(self.received(),b'')

    def test_rename_preserves_budget_deadline_and_episode(self):
        self.prepare();self.tick();original=self.inspect()['recovery']
        subprocess.run([str(fixtures.HGS),'rename',self.name,'codex/input-test/renamed'],env=self.env,text=True,capture_output=True,check=True)
        self.name='codex/input-test/renamed';self.tick();renamed=self.inspect()['recovery']
        self.assertEqual(renamed['name'],self.name)
        for key in ('id','due_at','attempt','delays','identity'):self.assertEqual(renamed[key],original[key])
        self.assertEqual(self.received(),b'')

if __name__=='__main__':unittest.main()
