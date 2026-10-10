"""Safe fixed-argument creation, exact launch UUID and minimal account projection."""
import os
from pathlib import Path
import tempfile
import unittest
import uuid
from unittest.mock import AsyncMock, patch
from zerus_mobile.connector import Connector
from zerus_mobile.server import validate_request
from zerus_mobile.launch import catalog


class LaunchTransport(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.temp=tempfile.TemporaryDirectory()
        self.connector=Connector({'server_url':'https://relay.example.invalid','node_token':'fixture','state_dir':self.temp.name})
        self.connector.launch_supported=True

    async def asyncTearDown(self):
        self.connector.journal.close();self.temp.cleanup()

    def request(self, operation='launch', **fields):
        identity=str(uuid.uuid4())
        payload={'request_id':identity}
        if operation=='launch':payload.update(agent='codex',directory='/example/mobile',tag='new-session')
        payload.update(fields)
        return {'request_id':identity,'computer_id':str(uuid.uuid4()),'operation':operation,'session':'','payload':payload}

    async def test_exact_marker_required_and_duplicate_cannot_launch_again(self):
        request=self.request(account_id='native-codex')
        catalog={'profiles':[{'id':'native-codex','provider':'codex','label':'Synthetic','installed':True,'secret':'must not escape'}]}
        target={'name':'codex/example/new-session','run_id':'new-run','conversation_id':'new-conversation','launch_id':request['request_id']}
        with patch.object(self.connector,'native',AsyncMock(side_effect=[catalog,{'path':'/example/mobile'},'Created',{'sessions':[target]}])) as native:
            result=await self.connector.execute(request)
            self.assertEqual(result['state'],'completed');self.assertEqual(result['result']['status'],'created')
            self.assertEqual(await self.connector.execute(request),result);self.assertEqual(native.await_count,4)
            self.assertEqual(native.call_args_list[2].args[0],['codex','/example/mobile','--new','-n','new-session','-d','--launch-id',request['request_id'],'--account','native-codex'])
            self.assertNotIn('launch_id',result['result']['result_target'])

    async def test_preflight_old_cli_invalid_fields_and_wrong_provider_do_not_launch(self):
        self.connector.launch_supported=False
        with patch.object(self.connector,'native',AsyncMock()) as native:
            self.assertEqual((await self.connector.execute(self.request()))['state'],'failed');native.assert_not_awaited()
        self.connector.launch_supported=True
        for fields in ({'directory':'../relative'},{'tag':'x/evil'},{'tag':'x.y'},{'agent':'shell'},{'command':'rm'},{'account_id':'../secret'}):
            request=self.request(**fields)
            with self.assertRaises(Exception):validate_request(request)
            with patch.object(self.connector,'native',AsyncMock()) as native:
                self.assertEqual((await self.connector.execute(request))['state'],'failed');native.assert_not_awaited()
        with patch.object(self.connector,'native',AsyncMock(return_value={'profiles':[{'id':'native-claude','provider':'claude','label':'Synthetic','installed':True}]})) as native:
            self.assertEqual((await self.connector.execute(self.request()))['state'],'failed');self.assertEqual(native.await_count,1)

    async def test_native_handoff_failure_is_uncertain_and_never_replayed(self):
        from zerus_mobile.connector import ConnectorError
        request=self.request()
        accounts={'profiles':[{'id':'native-codex','provider':'codex','label':'Synthetic','installed':True}]}
        with patch.object(self.connector,'native',AsyncMock(side_effect=[accounts,{'path':'/example/mobile'},ConnectorError('native command timed out')])) as native:
            result=await self.connector.execute(request);self.assertEqual(result['state'],'uncertain')
            self.assertEqual(await self.connector.execute(request),result);self.assertEqual(native.await_count,3)

    async def test_fixture_catalog_launch_then_inspect_and_send_new_exact_target(self):
        with patch.dict(os.environ,{'ZERUS_MOBILE_FIXTURE_DIR':self.temp.name}):
            self.connector.hgs=str(Path(__file__).with_name('fixture_hgs.py').resolve())
            catalog=await self.connector.execute(self.request('catalog'))
            self.assertEqual(catalog['state'],'completed')
            self.assertEqual(set(catalog['result']['accounts'][0]),{'id','provider','label','native','installed','is_default','auth_status'})
            result=await self.connector.execute(self.request())
            self.assertEqual(result['state'],'completed',result)
            target=result['result']['result_target']
            snapshot=await self.connector.snapshot()
            self.assertIn(target['name'],self.connector.sessions)
            request=self.request('inspect');request.update(session=target['name'],payload={})
            detail=await self.connector.execute(request)
            self.assertEqual(detail['result']['conversation_id'],target['conversation_id'])
            identity=str(uuid.uuid4())
            sent=await self.connector.execute({'request_id':identity,'operation':'send','session':target['name'],'payload':{'request_id':identity,'expected_run_id':target['run_id'],'expected_conversation_id':target['conversation_id'],'text':'Synthetic new-session message'}})
            self.assertEqual(sent['state'],'completed',sent)


class CatalogPrivacy(unittest.TestCase):
    def test_only_selected_public_fields_are_forwarded(self):
        value=catalog({'profiles':[{'id':'native-codex','provider':'codex','label':'Synthetic','installed':True,'home':'/private','credentials':{'secret':'private'}}],'defaults':{'private':'secret'}})
        self.assertEqual(value,{'accounts':[{'id':'native-codex','provider':'codex','label':'Synthetic','native':False,'installed':True,'is_default':False,'auth_status':'unknown'}],'agents':['codex']})
