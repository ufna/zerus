"""Two-process PostgreSQL scale probe using a newly created synthetic database.

Requires a private JSON admin config with database_url pointing at a loopback
PostgreSQL test instance and CREATE DATABASE permission. Never uses a live relay
URL or existing database contents. Child workers and the new database are removed.
"""
from __future__ import annotations
import argparse
import asyncio
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlsplit, urlunsplit
import uuid

import aiohttp
import asyncpg


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[int((len(ordered) - 1) * fraction)] if ordered else None


def rss(pid):
    data = dict(line.split(':', 1) for line in Path(f'/proc/{pid}/status').read_text().splitlines() if ':' in line)
    return int(data['VmHWM'].split()[0]) / 1024


async def fixture(store, count, history):
    identities = []
    for _ in range(count):
        workspace = await store.workspace('Synthetic scale workspace')
        node = await store.node(workspace, 'Synthetic scale computer')
        phone = await store.pair((await store.invite(workspace))['pair_code'], 'Synthetic scale phone')
        identities.append((workspace, node, phone))
    columns = ('id','workspace_id','device_id','node_id','operation','body_hash','body_bytes','body','state','created','updated')
    now = time.time()
    async with store.pool.acquire() as conn, conn.transaction():
        for start in range(0, history, 1000):
            records = []
            for index in range(start, min(history, start + 1000)):
                workspace, node, phone = identities[index % count]
                records.append((str(uuid.uuid4()), workspace, phone['device_id'], node['node_id'], 'send', '0' * 64, 0, '', 'completed', now - 864000, now - 864000))
            await conn.copy_records_to_table('requests', columns=columns, records=records)
        await conn.execute("UPDATE workspace_usage SET mutations=(SELECT count(*) FROM requests r WHERE r.workspace_id=workspace_usage.workspace_id)")
        for start in range(0, history, 1000):
            records = []
            for index in range(start, min(history, start + 1000)):
                workspace, node, _ = identities[index % count]
                records.append((index + 1, workspace, node['node_id'], 'synthetic', 'completed', now - 3600))
            await conn.copy_records_to_table('events', columns=('id','workspace_id','node_id','session','kind','created'), records=records)
        if history:
            await conn.execute("SELECT setval(pg_get_serial_sequence('events','id'),$1,true)", history)
        await conn.execute("UPDATE workspace_usage SET events=(SELECT count(*) FROM events e WHERE e.workspace_id=workspace_usage.workspace_id)")
        await conn.execute("UPDATE global_usage SET events=$1 WHERE id=1", history)
        await conn.execute('ANALYZE requests')
        await conn.execute('ANALYZE events')
    return identities


async def probe(args):
    from zerus_mobile.postgres import PostgresStore
    configuration = Path(args.admin_config)
    if configuration.stat().st_mode & 0o077:
        raise ValueError('admin config must be private (mode 0600)')
    original = json.loads(configuration.read_text())['database_url']
    parts = urlsplit(original)
    if parts.hostname not in ('127.0.0.1', '::1', 'localhost'):
        raise ValueError('only a loopback disposable PostgreSQL fixture is allowed')
    name = 'zerus_scale_' + uuid.uuid4().hex
    target = urlunsplit(parts._replace(path='/' + name))
    admin = await asyncpg.connect(original)
    workers = []
    store = None
    await admin.execute('CREATE DATABASE ' + name)
    try:
        store = await PostgresStore.open(target, pool_min=1, pool_max=4)
        identities = await fixture(store, args.workspaces, args.history)
        result = {'workers': 2, 'workspaces': args.workspaces, 'history_requests': args.history, 'history_events': args.history, 'duration_seconds': args.duration, 'poll_attempts': 2 * args.workspaces, 'poll_cap_per_worker': args.poll_cap}
        plans = {}
        async with store.pool.acquire() as conn:
            node = identities[0][1]['node_id']
            plans['empty_queue'] = [row[0] for row in await conn.fetch("EXPLAIN (ANALYZE,BUFFERS) SELECT id FROM requests WHERE node_id=$1 AND state='queued' AND created>$2 ORDER BY created LIMIT 1",node,time.time()-120)]
            plans['events_after_head'] = [row[0] for row in await conn.fetch("EXPLAIN (ANALYZE,BUFFERS) SELECT id,node_id,session,kind,created FROM events WHERE workspace_id=$1 AND id>$2 ORDER BY id LIMIT 100",identities[0][0],args.history)]
            plans['events_mid_history'] = [row[0] for row in await conn.fetch("EXPLAIN (ANALYZE,BUFFERS) SELECT id,node_id,session,kind,created FROM events WHERE workspace_id=$1 AND id>$2 ORDER BY id LIMIT 100",identities[0][0],args.history//2)]
        result['query_plans'] = plans
        with tempfile.TemporaryDirectory(prefix='zerus-scale-') as directory:
            root = Path(directory)
            settings = root / 'worker.json'
            settings.write_text(json.dumps({'database_url': target, 'poll_cap': args.poll_cap}))
            settings.chmod(0o600)
            origins = []
            logs = []
            for index in range(2):
                listener = socket.socket()
                listener.bind(('127.0.0.1', 0))
                listener.listen(256)
                origins.append('http://127.0.0.1:' + str(listener.getsockname()[1]))
                log = (root / f'worker-{index}.log').open('wb')
                logs.append(log)
                workers.append(subprocess.Popen([sys.executable, str(Path(__file__).resolve()), '--worker', '--config', str(settings), '--socket-fd', str(listener.fileno())], pass_fds=(listener.fileno(),), stdout=log, stderr=log))
                listener.close()
            async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=40), connector=aiohttp.TCPConnector(limit=4 * args.workspaces + 64)) as client:
                for origin in origins:
                    for _ in range(400):
                        try:
                            async with client.get(origin + '/readyz') as response:
                                await response.read()
                                if response.status == 200: break
                        except aiohttp.ClientError: pass
                        await asyncio.sleep(.025)
                    else: raise RuntimeError('synthetic worker readiness failed')
                health, metadata, metadata_burst, heartbeats, poll_results = [], [], [], [], []
                wake_times = {}
                semaphore = asyncio.Semaphore(32)
                async def heartbeat(index, phase='idle'):
                    _, node, _ = identities[index]
                    async with semaphore:
                        started = time.perf_counter()
                        if phase == 'input': wake_times[('phone',index)] = started
                        async with client.post(origins[index % 2] + '/v1/node/heartbeat', headers={'Authorization':'Bearer '+node['node_token']}, json={'snapshot':{'sessions':[{'name':'synthetic','run_id':'synthetic-run','conversation_id':'synthetic-conversation','phase':phase}]}}) as response:
                            await response.read()
                            heartbeats.append((response.status,time.perf_counter()-started))
                await asyncio.gather(*(heartbeat(index) for index in range(args.workspaces)))
                async def poll(index, role):
                    _, node, phone = identities[index]
                    route = '/v1/events?wait=25&after=' + str(args.history) if role == 'phone' else '/v1/node/requests?wait=25'
                    credential = phone['device_token'] if role == 'phone' else node['node_token']
                    start = time.perf_counter()
                    async with client.get(origins[index % 2]+route, headers={'Authorization':'Bearer '+credential}) as response:
                        data = await response.json()
                        poll_results.append({'role':role,'index':index,'returned':time.perf_counter(),'status':response.status,'seconds':time.perf_counter()-start,'rows':len(data.get('events', data.get('requests', [])))})
                polls = []
                for start in range(0, args.workspaces, 16):
                    for index in range(start, min(start+16,args.workspaces)):
                        polls += [asyncio.create_task(poll(index, role)) for role in ('phone','node')]
                    await asyncio.sleep(.05)
                await asyncio.sleep(.25)
                result['active_poll_leases'] = await store.pool.fetchval('SELECT count(*) FROM poll_leases')
                async def observe():
                    for _ in range(max(1, int(args.duration / .05))):
                        for origin in origins:
                            started = time.perf_counter()
                            async with client.get(origin+'/healthz') as response:
                                await response.read()
                                health.append((response.status,time.perf_counter()-started))
                        await asyncio.sleep(.05)
                observer = asyncio.create_task(observe())
                async def catalog(index, limiter, destination):
                    async with limiter:
                        started = time.perf_counter()
                        async with client.get(origins[index % 2]+'/v1/computers',headers={'Authorization':'Bearer '+identities[index][2]['device_token']}) as response:
                            await response.read()
                            destination.append((response.status,time.perf_counter()-started))
                # A controlled overload burst exercises fail-fast response
                # capacity; the admitted profile uses four reads per worker.
                await asyncio.gather(*(catalog(index,semaphore,metadata_burst) for index in range(args.workspaces)))
                admitted_reads = asyncio.Semaphore(8)
                started_profile = time.perf_counter()
                for _ in range(max(1, int(args.duration / 5))):
                    await asyncio.gather(*(heartbeat(index) for index in range(args.workspaces)))
                    await asyncio.gather(*(catalog(index,admitted_reads,metadata) for index in range(args.workspaces)))
                    await asyncio.sleep(5)
                result['heartbeat_profile_wall_seconds'] = time.perf_counter()-started_profile
                wake_start = time.perf_counter()
                await asyncio.gather(*(heartbeat(index,'input') for index in range(min(32,args.workspaces))))
                await asyncio.sleep(.5)
                result['event_wakes_before_half_second'] = sum(row['role']=='phone' and row['rows']>0 for row in poll_results)
                # One queued read per worker measures durable notification wake
                # without turning a large synchronous burst into a claim limit.
                submission_times = []
                for index in range(min(2,args.workspaces)):
                    _, node, phone = identities[index]
                    identity = str(uuid.uuid4())
                    started = time.perf_counter()
                    wake_times[('node',index)] = started
                    async with client.post(origins[(index+1)%2]+'/v1/requests', headers={'Authorization':'Bearer '+phone['device_token']},json={'request_id':identity,'computer_id':node['node_id'],'session':'synthetic','operation':'inspect','payload':{}}) as response:
                        await response.read()
                        submission_times.append((response.status,time.perf_counter()-started))
                await asyncio.sleep(.5)
                result['node_wakes_before_half_second'] = sum(row['role']=='node' and row['rows']>0 for row in poll_results)
                result['wake_observation_seconds'] = time.perf_counter()-wake_start
                for role in ('phone','node'):
                    latencies = [row['returned']-wake_times[(role,row['index'])] for row in poll_results if row['role']==role and row['rows']>0 and (role,row['index']) in wake_times]
                    result[role+'_wake'] = {'count':len(latencies),'p95_seconds':percentile(latencies,.95),'max_seconds':max(latencies,default=None)}
                for task in polls: task.cancel()
                await asyncio.gather(*polls, return_exceptions=True)
                await observer
                await client.close()
                disconnect_start = time.perf_counter()
                remaining = 0
                while time.perf_counter()-disconnect_start < 5:
                    remaining = await store.pool.fetchval('SELECT count(*) FROM poll_leases')
                    if not remaining: break
                    await asyncio.sleep(.05)
                result['lease_disconnect_drain_seconds'] = time.perf_counter()-disconnect_start
                result['remaining_leases_after_disconnect'] = remaining
                for label, rows in [('health',health),('metadata',metadata),('metadata_burst',metadata_burst),('heartbeat',heartbeats),('submission',submission_times)]:
                    result[label] = {'requests':len(rows),'statuses':{str(status):sum(s==status for s,_ in rows) for status in sorted({s for s,_ in rows})},'p95_seconds':percentile([seconds for _,seconds in rows],.95),'p99_seconds':percentile([seconds for _,seconds in rows],.99),'max_seconds':max((seconds for _,seconds in rows),default=None)}
                result['poll_responses'] = {str(status):sum(row['status']==status for row in poll_results) for status in sorted({row['status'] for row in poll_results})}
                result['worker_peak_rss_mib'] = [rss(worker.pid) for worker in workers]
            for worker in workers:
                worker.terminate()
                worker.wait(timeout=10)
            workers.clear()
            for log in logs: log.close()
        return result
    finally:
        for worker in workers:
            worker.terminate()
            worker.wait(timeout=10)
        if store: await store.close()
        await admin.execute('DROP DATABASE '+name+' WITH (FORCE)')
        await admin.close()


def worker(args):
    from aiohttp import web
    from zerus_mobile.postgres import PostgresStore
    from zerus_mobile.server import Config,create_app
    async def application():
        settings=json.loads(Path(args.config).read_text())
        store=await PostgresStore.open(settings['database_url'],pool_min=2,pool_max=10)
        return create_app(store,Config(background=False,max_polls_global=settings.get('poll_cap',256)))
    web.run_app(application(),sock=socket.socket(fileno=args.socket_fd),print=None,access_log=None,handler_cancellation=True)


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--admin-config')
    parser.add_argument('--workspaces',type=int,default=200)
    parser.add_argument('--history',type=int,default=100000)
    parser.add_argument('--duration',type=float,default=10)
    parser.add_argument('--poll-cap',type=int,default=256,help='Explicit experimental local poll cap per worker; default256')
    parser.add_argument('--worker',action='store_true',help=argparse.SUPPRESS)
    parser.add_argument('--config',help=argparse.SUPPRESS)
    parser.add_argument('--socket-fd',type=int,help=argparse.SUPPRESS)
    args=parser.parse_args()
    if args.worker: worker(args)
    else:
        if not args.admin_config or not 2 <= args.workspaces <= 1000 or not 0 <= args.history <= 1000000 or not 1 <= args.duration <= 60 or not 1 <= args.poll_cap <= 2000:
            parser.error('private admin config, 2..1000 workspaces, 0..1000000 history, 1..60 seconds required')
        print(json.dumps(asyncio.run(probe(args)),indent=2))
