"""One-hop administration and lossless offline registry migration regressions."""
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import shutil
import sys
import tempfile
import unittest
from urllib.parse import urlsplit
import uuid

from zerus_mobile.migration import COLUMNS, ORDER, REGISTRY_TABLES, TABLES, _source, migrate_sqlite
from zerus_mobile.routes import drive_sync
from zerus_mobile.store import Store

DSN = os.environ.get('ZERUS_RELAY_TEST_DSN', '')
LOCAL = '00000000-0000-4000-8000-000000000001'
PEER = '00000000-0000-4000-8000-000000000002'
REVOKED = '00000000-0000-4000-8000-000000000003'
ROUTE = '00000000-0000-4000-8000-000000000004'
REVOKED_ROUTE = '00000000-0000-4000-8000-000000000005'
ALIAS = '00000000-0000-4000-8000-000000000006'
SNAPSHOT = {'sessions': [{'name': 'codex/example'}], 'mobile_capabilities': {
    'protocol_version': 1, 'operations': ['inspect', 'send'], 'features': ['gateway_one_hop']}}


def fixture(path, *, legacy=False):
    source = Store(path)
    workspace = source.workspace('Synthetic migration workspace')
    gateway = source.node(workspace, 'Arch')
    phone = source.pair(source.invite(workspace)['pair_code'], 'Synthetic phone')
    node = source.authenticate(gateway['node_token'], 'nodes')
    device = source.authenticate(phone['device_token'], 'devices')
    if not legacy:
        source.heartbeat(node, SNAPSHOT, machine_id=LOCAL, peers=[
            {'route_id': ROUTE, 'machine_id': PEER, 'name': 'Mac', 'online': True},
            {'route_id': REVOKED_ROUTE, 'machine_id': REVOKED, 'name': 'Other', 'online': True}])
        source.peer_heartbeat(node, ROUTE, PEER, SNAPSHOT)
        source.peer_heartbeat(node, REVOKED_ROUTE, REVOKED, SNAPSHOT)
        target = source.db.execute('SELECT computer_id FROM native_computers WHERE machine_id=?', (PEER,)).fetchone()[0]
        revoked_target = source.db.execute('SELECT computer_id FROM native_computers WHERE machine_id=?', (REVOKED,)).fetchone()[0]
        with source.db:
            drive_sync(source.db, source.registry.alias(workspace, ALIAS, target))
    else:
        source.heartbeat(node, SNAPSHOT)
        target = gateway['node_id']
    commands = {}
    for label in ('completed', 'claimed', 'queued'):
        identity = str(uuid.uuid4())
        body = {'request_id': identity, 'computer_id': target, 'operation': 'send', 'session': 'codex/example',
                'payload': {'request_id': identity, 'expected_run_id': 'fixture-run',
                            'expected_conversation_id': 'fixture-conversation', 'text': label}}
        commands[label] = body
        assert source.submit(device, body, 200)[0] == 202
        if label != 'queued':
            assert source.claim(node, allow_gateway=True)[0]['request_id'] == identity
        if label == 'completed':
            assert source.result(node, identity, {'state': 'completed', 'result': {'fixture': True}, 'error': None}) == 200
    if not legacy:
        other = source.node(workspace, 'Mac direct')
        source.heartbeat(source.authenticate(other['node_token'], 'nodes'), SNAPSHOT, machine_id=PEER, peers=[])
        source.revoke_computer(revoked_target)
    source.close()
    if legacy:
        connection = sqlite3.connect(path)
        try:
            for table in ('computer_routes', 'computer_aliases', 'native_computers', 'computers'):
                connection.execute(f'DROP TABLE {table}')
            for table, columns in (('nodes', ('computer_id', 'machine_id', 'manifest_hash')),
                ('requests', ('target_computer_id', 'target_machine_id', 'route_id'))):
                for column in columns:
                    connection.execute(f'ALTER TABLE {table} DROP COLUMN {column}')
            connection.commit()
        finally:
            connection.close()
    return workspace, gateway, phone, commands


class ComputerRevocationCliTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which('zerus-relay'), 'Rust executable required for compatibility CLI')
    def test_offline_revoke_computer_accepts_alias_and_revokes_all_target_routes(self):
        with tempfile.TemporaryDirectory(prefix='zerus-computer-cli-') as directory:
            path = Path(directory) / 'relay.sqlite3'
            fixture(path)
            completed = subprocess.run([sys.executable, '-m', 'zerus_mobile', '--database', str(path),
                'revoke-computer', '--id', ALIAS], text=True, capture_output=True, check=True)
            self.assertEqual(json.loads(completed.stdout), {'revoked': True})
            source = Store(path)
            try:
                target = source.db.execute('SELECT computer_id FROM computer_aliases WHERE alias=?', (ALIAS,)).fetchone()[0]
                self.assertEqual(source.db.execute('SELECT revoked FROM computers WHERE id=?', (target,)).fetchone()[0], 1)
                self.assertEqual(source.db.execute('SELECT count(*) FROM computer_routes WHERE computer_id=? AND active=1', (target,)).fetchone()[0], 0)
                self.assertEqual(source.db.execute('SELECT count(*) FROM nodes WHERE revoked=1').fetchone()[0], 0)
            finally:
                source.close()
            missing = subprocess.run([sys.executable, '-m', 'zerus_mobile', '--database', str(path),
                'revoke-computer', '--id', str(uuid.uuid4())], text=True, capture_output=True)
            self.assertEqual(missing.returncode, 2)
            self.assertIn('computer not found', missing.stderr)

    def test_partial_registry_source_rejected_before_destination_open(self):
        with tempfile.TemporaryDirectory(prefix='zerus-incomplete-registry-') as directory:
            path = Path(directory) / 'relay.sqlite3'
            fixture(path)
            connection = sqlite3.connect(path)
            connection.execute('DROP TABLE computer_routes'); connection.commit(); connection.close()
            with self.assertRaisesRegex(ValueError, 'incomplete'):
                _source(path)

    def test_missing_registry_with_populated_bindings_is_not_silently_downgraded(self):
        with tempfile.TemporaryDirectory(prefix='zerus-missing-registry-') as directory:
            path = Path(directory) / 'relay.sqlite3'
            fixture(path)
            connection = sqlite3.connect(path)
            for table in ('computer_routes', 'computer_aliases', 'native_computers', 'computers'):
                connection.execute(f'DROP TABLE {table}')
            connection.commit(); connection.close()
            with self.assertRaisesRegex(ValueError, 'bindings'):
                _source(path)


@unittest.skipUnless(DSN, 'requires explicitly disposable ZERUS_RELAY_TEST_DSN')
class PeerMigrationTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        parsed = urlsplit(DSN)
        if parsed.hostname not in {'127.0.0.1', 'localhost', '::1'} or not parsed.path.lstrip('/').startswith('zerus_test_'):
            raise ValueError('peer migration test DSN must name a disposable loopback zerus_test_* database')
        import asyncpg
        connection = await asyncpg.connect(DSN)
        try:
            await connection.execute('DROP SCHEMA public CASCADE; CREATE SCHEMA public')
        finally:
            await connection.close()
        self.destination = None

    async def asyncTearDown(self):
        if self.destination:
            await self.destination.close()

    async def open(self):
        from zerus_mobile.postgres import PostgresStore
        self.destination = await PostgresStore.open(DSN, global_max_bytes=16 * 1024 * 1024, quota_shards=1)
        return self.destination

    async def test_populated_registry_and_route_sidecars_preserved_exactly(self):
        with tempfile.TemporaryDirectory(prefix='zerus-peer-migration-') as directory:
            path = Path(directory) / 'relay.sqlite3'
            workspace, gateway, phone, commands = fixture(path)
            before = hashlib.sha256(path.read_bytes()).digest()
            original = sqlite3.connect(path); original.row_factory = sqlite3.Row
            expected = {table: [tuple(row[c] for c in COLUMNS[table]) for row in original.execute(
                f"SELECT * FROM {table} ORDER BY {ORDER.get(table, 'id')}")] for table in TABLES}
            original.close()
            report = await migrate_sqlite(path, DSN, global_max_bytes=16 * 1024 * 1024, quota_shards=1)
            self.assertEqual(hashlib.sha256(path.read_bytes()).digest(), before)
            self.assertEqual(set(report['digests']), set(TABLES))
            destination = await self.open()
            for table in TABLES:
                actual = await destination.pool.fetch(f"SELECT {','.join(COLUMNS[table])} FROM {table} ORDER BY {ORDER.get(table, 'id')}")
                self.assertEqual([tuple(row) for row in actual], expected[table], table)
            node = await destination.authenticate(gateway['node_token'], 'nodes')
            device = await destination.authenticate(phone['device_token'], 'devices')
            self.assertEqual((await destination.submit(device, commands['completed'], 200))[1]['state'], 'completed')
            claimed = await destination.claim(node, allow_gateway=True)
            self.assertEqual([r['request_id'] for r in claimed], [commands['queued']['request_id']])
            self.assertEqual(claimed[0]['gateway_route']['route_id'], ROUTE)
            self.assertEqual(await destination.claim(node, allow_gateway=True), [])
            self.assertEqual(await destination.pool.fetchval('SELECT state FROM requests WHERE id=$1', commands['claimed']['request_id']), 'claimed')
            with self.assertRaisesRegex(ValueError, 'empty'):
                await migrate_sqlite(path, DSN, global_max_bytes=16 * 1024 * 1024, quota_shards=1)

    async def test_legacy_eight_table_import_creates_only_unbound_direct_routes(self):
        with tempfile.TemporaryDirectory(prefix='zerus-legacy-peer-migration-') as directory:
            path = Path(directory) / 'relay.sqlite3'
            _, gateway, phone, commands = fixture(path, legacy=True)
            report = await migrate_sqlite(path, DSN, global_max_bytes=16 * 1024 * 1024, quota_shards=1)
            self.assertEqual(set(report['digests']), set(TABLES) - set(REGISTRY_TABLES))
            destination = await self.open()
            self.assertEqual(await destination.pool.fetchval('SELECT count(*) FROM computers'), 1)
            self.assertEqual(await destination.pool.fetchval('SELECT count(*) FROM native_computers'), 0)
            route = await destination.pool.fetchrow('SELECT * FROM computer_routes')
            self.assertEqual(route['computer_id'], gateway['node_id'])
            self.assertIsNone(route['machine_id'])
            node = await destination.authenticate(gateway['node_token'], 'nodes')
            queued = await destination.claim(node)
            self.assertEqual([r['request_id'] for r in queued], [commands['queued']['request_id']])
            self.assertNotIn('gateway_route', queued[0])
            self.assertEqual(await destination.claim(node), [])
            self.assertEqual(await destination.pool.fetchval('SELECT state FROM requests WHERE id=$1', commands['claimed']['request_id']), 'claimed')
