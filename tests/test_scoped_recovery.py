"""Waiting-job edits on isolated files, without workers or native agents."""
import hashlib
import json
from pathlib import Path
import subprocess
import time
import uuid
import unittest
import test_public_history_contract as fixtures

class ScopedRecovery(fixtures.PublicHistoryContract):
    def setUp(self):
        super().setUp()
        self.write_binding(model='fixture-model')
        inspected=subprocess.run([str(fixtures.HGS),'inspect',self.name,'--skip-processes'],capture_output=True,text=True,env=self.env,timeout=10)
        self.assertEqual(inspected.returncode,0,inspected.stderr)
        native=json.loads(inspected.stdout)
        self.job_id=str(uuid.uuid4())
        self.job={'id':self.job_id,'name':self.name,'state':'waiting','identity':[native.get(field) for field in ('run_id','conversation_id','model','account_id','host_generation')],
                  'due_at':time.time()+1000,'not_before':time.time()+100,'attempt':1,'history':[]}
        directory=self.state/'recovery/jobs';directory.mkdir(parents=True,mode=0o700)
        self.job_path=directory/(hashlib.sha256(self.name.encode()).hexdigest()+'.json')
        self.job_path.write_text(json.dumps(self.job));self.job_path.chmod(0o600)
    def action(self,payload):
        result=subprocess.run([str(fixtures.HGS),'recovery','action','--scoped-json'],input=json.dumps(payload),text=True,capture_output=True,env=self.env,timeout=10)
        self.assertEqual(result.returncode,0,result.stderr);return json.loads(result.stdout)
    def payload(self,action='now'):
        return {'request_id':str(uuid.uuid4()),'name':self.name,'expected_run_id':self.run,'expected_conversation_id':self.conversation,'job_id':self.job_id,'action':action}
    def test_schedule_obeys_not_before_and_uuid_replay_cannot_edit_again(self):
        request=self.payload();result=self.action(request);self.assertEqual(result['status'],'scheduled');self.assertGreaterEqual(result['recovery']['due_at'],self.job['not_before'])
        self.assertEqual(self.action(request),result)
    def test_cancel_and_full_five_field_identity_reject_replacement_model(self):
        request=self.payload('cancel');self.write_binding(model='changed')
        result=self.action(request);self.assertEqual(result['status'],'failed');self.assertEqual(json.loads(self.job_path.read_text())['state'],'waiting')
        self.write_binding(model='fixture-model')
        result=self.action(self.payload('cancel'));self.assertEqual(result['status'],'cancelled');self.assertEqual(result['recovery']['state'],'cancelled')
    def test_wrong_run_and_finished_job_rejected_without_job_change(self):
        request=self.payload();request['expected_run_id']='replacement'
        self.assertEqual(self.action(request)['status'],'failed');self.assertEqual(json.loads(self.job_path.read_text()),self.job)
        self.job['state']='dispatching';self.job_path.write_text(json.dumps(self.job))
        self.assertEqual(self.action(self.payload())['status'],'failed');self.assertEqual(json.loads(self.job_path.read_text()),self.job)
for _name in vars(fixtures.PublicHistoryContract):
    if _name.startswith('test_'):setattr(ScopedRecovery,_name,None)

import test_dsh_session_actions as dsh_fixtures

class DeepSeekScopedRecovery(dsh_fixtures.DeepSeekScopedActions):
    def setUp(self):
        super().setUp()
        self.replace_binding=False
        self.job={'id':str(uuid.uuid4()),'name':self.name,'state':'waiting',
                  'identity':['run','session-synthetic','native-model','native-dsh','fixture-generation'],
                  'attempt':1,'due_at':time.time()+500,'not_before':time.time()+100,'history':[]}
        directory=self.root/'state/recovery/jobs';directory.mkdir(parents=True,mode=0o700)
        self.job_path=directory/(hashlib.sha256(self.name.encode()).hexdigest()+'.json')
        self.job_path.write_text(json.dumps(self.job));self.job_path.chmod(0o600)
    def serve(self):
        import socket
        while not self.stop.is_set():
            try:client,_=self.server.accept()
            except socket.timeout:continue
            with client:
                request=json.loads(client.makefile('rb').readline());self.calls.append(request)
                if request['method']=='inspect':
                    value={'generation':'fixture-generation','live':True,'selection':{'provider':'synthetic','model':'native-model'},'page':{'records':[],'hasMore':False}}
                    if self.replace_binding:self.binding.write_text(json.dumps({**self.record,'run_id':'replacement'}))
                elif request['method']=='list':value={'items':[{'sessionId':'session-synthetic','agentAvailable':True,'running':False}]}
                elif request['method']=='ping':value={'generation':'fixture-generation','sessionActions':True}
                else:value={}
                client.sendall((json.dumps({'ok':True,'value':value})+'\n').encode())
    def recovery(self):
        payload={'request_id':str(uuid.uuid4()),'name':self.name,'expected_run_id':'run','expected_conversation_id':'session-synthetic','job_id':self.job['id'],'action':'now'}
        result=subprocess.run([str(dsh_fixtures.HGS),'recovery','action','--scoped-json'],input=json.dumps(payload),text=True,capture_output=True,env=self.env,timeout=10)
        self.assertEqual(result.returncode,0,result.stderr);return json.loads(result.stdout)
    def test_native_model_account_and_generation_missing_from_binding_are_verified(self):
        self.assertNotIn('model',self.record);self.assertNotIn('host_generation',self.record)
        result=self.recovery();self.assertEqual(result['status'],'scheduled',result)
        self.assertGreaterEqual(result['recovery']['due_at'],self.job['not_before'])
        self.assertEqual(result['recovery']['identity'],self.job['identity'])
        self.assertTrue(set(call['method'] for call in self.calls)<={'inspect','list','ping','catalog'})
    def test_binding_replacement_during_normalized_inspection_rejects_before_edit(self):
        self.replace_binding=True
        self.assertEqual(self.recovery()['status'],'failed')
        self.assertEqual(json.loads(self.job_path.read_text()),self.job)
for _name in vars(dsh_fixtures.DeepSeekScopedActions):
    if _name.startswith('test_'):setattr(DeepSeekScopedRecovery,_name,None)

if __name__=='__main__':unittest.main()
