"""Exact native repository authority, bounded creation and no replay."""
import asyncio
import copy
import json
from pathlib import Path
import tempfile
import unittest
import uuid
from unittest.mock import AsyncMock, patch
from zerus_mobile import worktrees
from zerus_mobile.connector import Connector

class Worktrees(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.node=Connector({'server_url':'https://relay.example.invalid','node_token':'fixture','state_dir':self.tmp.name})
        self.node.launch_supported=self.node.worktree_supported=True
        self.id=str(uuid.uuid4())
        self.payload={'request_id':self.id,'path':'/repo','common_dir':'/repo/.git','destination':'/alias/new','branch':'feature','base':'HEAD'}
        self.request={'request_id':self.id,'operation':'worktree_create','session':'','payload':self.payload}
        self.catalog={'state':'ok','stale':False,'path':'/repo','common_dir':'/repo/.git','worktrees':[{'path':'/repo','kind':'main','available':True},{'path':'/linked','kind':'linked','available':True}]}
    async def asyncTearDown(self):
        await self.node.accounts.close();self.node.journal.close();self.tmp.cleanup()
    async def test_confirmed_canonical_destination_and_duplicate_do_not_create_again(self):
        created={'request_id':self.id,'status':'created','path':'/canonical/new','common_dir':'/repo/.git','branch':'feature'}
        with patch.object(self.node,'native',AsyncMock(side_effect=[{'path':'/canonical'},self.catalog,created])) as native:
            result=await self.node.execute(self.request)
            self.assertEqual(result['state'],'completed',result)
            self.assertEqual(native.call_args_list[-1].kwargs['timeout'],35)
            self.assertIn('--mobile',native.call_args_list[-1].args[0])
            self.assertEqual(await self.node.execute(self.request),result)
            self.assertEqual(native.await_count,3)
    async def test_foreign_result_is_uncertain_and_never_replayed(self):
        created={'request_id':self.id,'status':'created','path':'/other/new','common_dir':'/repo/.git','branch':'feature'}
        with patch.object(self.node,'native',AsyncMock(side_effect=[{'path':'/canonical'},self.catalog,created])) as native:
            result=await self.node.execute(self.request)
            self.assertEqual(result['state'],'uncertain',result)
            self.assertEqual(await self.node.execute(self.request),result);self.assertEqual(native.await_count,3)
    async def test_changed_repository_fails_before_mutation(self):
        changed={**self.catalog,'common_dir':'/other/.git'}
        with patch.object(self.node,'native',AsyncMock(side_effect=[{'path':'/canonical'},changed])) as native:
            self.assertEqual((await self.node.execute(self.request))['state'],'failed');self.assertEqual(native.await_count,2)
    async def test_legacy_local_downgrade_uses_old_safe_namespace(self):
        with patch.object(self.node,'_native',AsyncMock(return_value=self.catalog)) as native:
            await self.node.native(['worktrees','--path','/repo','--refresh','--json'])
            self.assertEqual(native.call_args.args[0],['__state','worktrees','--path','/repo','--refresh','--json'])
    def test_nested_independent_repository_and_ordinary_subdir_not_members(self):
        self.assertTrue(worktrees.contains(self.catalog,'/linked'))
        self.assertFalse(worktrees.contains(self.catalog,'/repo/unrelated'))
        self.assertFalse(worktrees.contains(self.catalog,'/linked/subdir'))
        self.assertFalse(worktrees.contains({**self.catalog,'stale':True},'/linked'))
    def test_strict_fields_and_result_identity(self):
        for payload in ({**self.payload,'shell':'bad'},{**self.payload,'branch':'-bad'}):
            with self.assertRaises(ValueError): worktrees.validate('worktree_create',payload,self.id)
        bad={'request_id':self.id,'status':'created','path':'/canonical/new','common_dir':'/repo/.git','branch':'wrong'}
        with self.assertRaises(ValueError): worktrees.created(bad,self.payload,self.id,'/canonical/new')
    async def test_repeated_cancellation_drains_only_owned_bounded_create(self):
        helper=Path(self.tmp.name)/'helper.py'
        helper.write_text('#!/usr/bin/env python3\nimport time,json,pathlib\ntime.sleep(.4)\npathlib.Path('+repr(str(Path(self.tmp.name)/'finished'))+').write_text("done")\nprint(json.dumps({"done":True}))\n');helper.chmod(0o700)
        self.node.hgs=str(helper)
        task=asyncio.create_task(self.node._native(['__state','worktrees','create','--mobile'],timeout=2))
        await asyncio.sleep(.1);task.cancel();await asyncio.sleep(.1);task.cancel()
        with self.assertRaises(asyncio.CancelledError): await task
        self.assertEqual((Path(self.tmp.name)/'finished').read_text(),'done')
        # Successful drain leaves no connector-owned read/write/wait task running.
        self.assertFalse(any(t is not asyncio.current_task() and not t.done() and 'Connector._native' in repr(t.get_coro()) for t in asyncio.all_tasks()))
    async def test_cancel_while_spawn_returns_retains_one_owned_process(self):
        helper=Path(self.tmp.name)/'helper.py'
        helper.write_text('#!/usr/bin/env python3\nimport time,json\ntime.sleep(.4)\nprint(json.dumps({"done":True}))\n');helper.chmod(0o700)
        self.node.hgs=str(helper)
        real_spawn=asyncio.create_subprocess_exec;started=asyncio.Event();spawns=[]
        async def delayed(*args,**kwargs):
            child=await real_spawn(*args,**kwargs);spawns.append(child);started.set();await asyncio.sleep(.2);return child
        with patch('zerus_mobile.connector.asyncio.create_subprocess_exec',side_effect=delayed):
            task=asyncio.create_task(self.node._native(['__state','worktrees','create','--mobile'],timeout=2))
            await started.wait();task.cancel();await asyncio.sleep(.05);task.cancel()
            with self.assertRaises(asyncio.CancelledError): await task
        self.assertEqual(len(spawns),1);self.assertEqual(spawns[0].returncode,0)
    def test_related_project_uses_exact_anchor_without_adding_folder(self):
        from zerus_mobile.launch import project_choice
        project={'id':'project','folders':[{'id':'anchor','path':'/repo'}]}
        catalog={'swarm_id':'swarm','projects':[project]}
        payload={'swarm_id':'swarm','project_id':'project','add_folder':False,'worktree_folder_id':'anchor','worktree_common_dir':'/repo/.git'}
        self.assertEqual(project_choice(catalog,payload,'/linked',self.catalog),project)
        with self.assertRaises(ValueError): project_choice(catalog,payload,'/repo/nested-unrelated',self.catalog)
        with self.assertRaises(ValueError): project_choice(catalog,{**payload,'worktree_folder_id':'other'},'/linked',self.catalog)

class PeerWorktrees(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        import test_connector_peers as fixture
        self.fixture=fixture
        self.calls=fixture.PeerConnectorTests.calls.__get__(self)
        self.write_inventory=fixture.PeerConnectorTests.write_inventory.__get__(self)
        await fixture.PeerConnectorTests.asyncSetUp(self)
    async def asyncTearDown(self):await self.fixture.PeerConnectorTests.asyncTearDown(self)
    def request(self,peer=True):
        run='peer-run' if peer else 'local-run';identity=str(uuid.uuid4())
        return {'request_id':identity,'operation':'worktree_create','session':'','payload':{'request_id':identity,'path':'/example/'+run,'common_dir':'/example/'+run+'/.git','destination':'/example/'+run+'/new','branch':'feature','base':'HEAD'},'gateway_route':{'schema':1,'route_id':self.identity if peer else None,'computer_id':self.fixture.COMPUTER,'machine_id':self.fixture.PEER if peer else self.fixture.LOCAL}}
    async def test_worktree_create_keeps_selected_native_context_and_exact_guard(self):
        for peer,machine in ((True,self.fixture.PEER),(False,self.fixture.LOCAL)):
            request=self.request(peer);result=await self.connector.execute(request)
            self.assertEqual(result['state'],'completed',result)
            record=json.loads((self.root/('worktree-'+machine+'.json')).read_text())
            self.assertEqual(record['request_id'],request['request_id'])
            self.assertEqual(record['common_dir'],request['payload']['common_dir'])
        wrappers=[row for row in self.calls() if row['argv']==['swarm','mobile-peer','--json'] and row['payload']['argv'][:2]==['worktrees','create']]
        self.assertEqual(wrappers[0]['payload']['target_machine_id'],self.fixture.PEER)
        self.assertIn('--mobile',wrappers[0]['payload']['argv'])
    async def test_rebound_peer_cannot_create_on_local_or_new_identity(self):
        self.inventory['peers'][0]['machine_id']=self.fixture.OTHER;self.write_inventory()
        result=await self.connector.execute(self.request())
        self.assertEqual(result['state'],'failed',result)
        self.assertFalse(list(self.root.glob('worktree-*.json')))

class WorktreeRestart(unittest.IsolatedAsyncioTestCase):
    async def test_restart_of_claimed_creation_never_calls_native_again(self):
        from zerus_mobile.connector import Journal
        with tempfile.TemporaryDirectory() as directory:
            config={'server_url':'https://relay.example.invalid','node_token':'fixture','state_dir':directory}
            node=Connector(config);identity=str(uuid.uuid4())
            import hashlib
            request={'request_id':identity,'operation':'worktree_create','session':'','payload':{}}
            fingerprint=hashlib.sha256(json.dumps(request,sort_keys=True,separators=(',',':'),allow_nan=False).encode()).hexdigest()
            self.assertTrue(node.journal.claim(identity,'worktree_create',fingerprint))
            node.journal.close();await node.accounts.close()
            resumed=Connector(config)
            try:
                request={'request_id':identity,'operation':'worktree_create','session':'','payload':{}}
                with patch.object(resumed,'native',AsyncMock()) as native:
                    result=await resumed.execute(request)
                    self.assertEqual(result['state'],'uncertain');native.assert_not_awaited()
            finally:await resumed.accounts.close();resumed.journal.close()
