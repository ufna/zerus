"""Synthetic migration never contacts either managed relay or invokes native agents."""
import hashlib
from pathlib import Path
import tempfile
import unittest
import uuid

from zerus_mobile.connector import Connector, ConnectorError
from zerus_mobile.managed_relay import CURRENT, LEGACY, identity_url, transport


class ManagedRelayTests(unittest.TestCase):
    def test_exact_origin_only(self):
        for value in (LEGACY, LEGACY + '/', 'https://ZERUS.DEV.GUTHUB.DEV:443/'):
            self.assertEqual(CURRENT, transport(value))
        for value in ('http://zerus.dev.guthub.dev', LEGACY + ':444', LEGACY + '/path',
                      LEGACY + '?x=1', LEGACY + '#fragment', 'https://synthetic-user' + '@' + 'zerus.dev.guthub.dev',
                      LEGACY + '.example', 'https://custom.example'):
            self.assertEqual(value, transport(value))

    def test_identity_override_only_preserves_managed_forward_migration(self):
        self.assertEqual(LEGACY, identity_url(CURRENT, LEGACY))
        self.assertEqual('https://custom.example', identity_url('https://custom.example', 'https://custom.example'))
        for server, identity in ((LEGACY, CURRENT), (CURRENT, 'https://custom.example'),
                                 ('https://custom.example', LEGACY), (CURRENT + '/path', LEGACY),
                                 (CURRENT, LEGACY + ':444'), (CURRENT, LEGACY + '?x=1')):
            with self.assertRaises(ValueError):
                identity_url(server, identity)

    def test_original_binding_receipts_and_project_fallback_survive_config_change(self):
        with tempfile.TemporaryDirectory() as directory:
            config = dict(server_url=LEGACY, node_token='synthetic-node-token', state_dir=directory)
            old = Connector(config)
            expected = hashlib.sha256((LEGACY+'\0synthetic-node-token').encode()).hexdigest()
            self.assertEqual(CURRENT, old.server_url)
            self.assertEqual(expected, old.project_fallback_id)
            request = str(uuid.uuid4())
            self.assertTrue(old.journal.claim(request, 'send', 'synthetic-body-hash'))
            old.journal.finish(request, dict(state='completed', result={'request_id':request,'status':'submitted'},error=None))
            old.journal.close()
            migrated = Connector({**config, 'server_url':CURRENT, 'identity_url':LEGACY})
            try:
                self.assertEqual(expected, migrated.project_fallback_id)
                self.assertFalse(migrated.journal.claim(request, 'send', 'synthetic-body-hash'))
                self.assertEqual(request, migrated.journal.replay(request)['result']['request_id'])
            finally:
                migrated.journal.close()
            with self.assertRaises(ConnectorError):
                Connector({**config,'server_url':CURRENT})

    def test_invalid_override_fails_before_opening_journal(self):
        with tempfile.TemporaryDirectory() as directory:
            state=Path(directory)/'not-created'
            with self.assertRaises(ConnectorError):
                Connector(dict(server_url=CURRENT,identity_url='https://foreign.example',node_token='synthetic',state_dir=str(state)))
            self.assertFalse(state.exists())
