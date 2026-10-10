#!/usr/bin/env python3
"""Disposable Docker/cgroup qualification. No live URL or credential input.

Build services/mobile, then run with a Python environment containing aiohttp.
Database and response spools live in new disk-backed Docker volumes; fixtures
are streamed from the host and are never charged to the relay's cgroup.
"""
import argparse
import asyncio
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import uuid

import aiohttp

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'tests'))
from relay_load import write_fixture, memory_kib


def docker(*args):
    return subprocess.check_output(['docker', *args], text=True).strip()


async def profile(image, kind, repeats, concurrency, memory_limit=256, backend='sqlite'):
    identity = 'zerus-rust-load-' + uuid.uuid4().hex[:12]
    data, spool = identity + '-data', identity + '-spool'
    docker('volume', 'create', data)
    docker('volume', 'create', spool)
    database, network, started = None, None, False
    try:
        config_args = []
        if backend == 'postgres':
            network, database = identity + '-network', identity + '-postgres'
            docker('network', 'create', network)
            docker('run', '--detach', '--name', database, '--network', network,
                '--network-alias', 'database', '--memory', '256m', '--memory-swap', '256m',
                '--tmpfs', '/var/lib/postgresql/data:size=256m',
                '-e', 'POSTGRES_HOST_AUTH_METHOD=trust',
                'postgres:17.11-alpine@sha256:b0f9560a2de083e2cc7382e75f808c7381a32852a7ec49117deedb300e552b24')
            for _ in range(200):
                ready = subprocess.run(['docker', 'exec', database, 'pg_isready', '-U', 'postgres'],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                if ready.returncode == 0:
                    break
                await asyncio.sleep(.05)
            else:
                raise AssertionError('disposable PostgreSQL failed readiness')
            subprocess.run(['docker', 'run', '--rm', '--network', 'none', '-i',
                '-v', data + ':/data', '--entrypoint', '/bin/sh', image,
                '-c', 'umask 077; cat > /data/config.json'],
                input=json.dumps({'database_url': 'postgresql://postgres@database/postgres'}),
                text=True, check=True)
            config_args = ['--config', '/data/config.json']
        credentials = json.loads(docker('run', '--rm', '--network', network or 'none',
            '-v', data + ':/data', image, *config_args, 'provision', '--name', 'Synthetic load',
            '--computer-name', 'Synthetic computer'))
        docker('run', '--detach', '--name', identity, '--memory', f'{memory_limit}m', '--memory-swap', f'{memory_limit}m',
            '--pids-limit', '64', '--read-only', '--tmpfs', '/tmp:size=4m',
            '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges',
            '-v', data + ':/data', '-v', spool + ':/spool', '-p', '127.0.0.1::8787',
            '--network', network or 'bridge', image, *config_args, 'serve', '--host', '0.0.0.0')
        started = True
        port = docker('port', identity, '8787').rsplit(':', 1)[1]
        pid = int(docker('inspect', '--format', '{{.State.Pid}}', identity))
        url = f'http://127.0.0.1:{port}'
        health, statuses, admitted, memory_stages = [], [], [], []
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=90)) as client:
            for _ in range(200):
                try:
                    async with client.get(url + '/readyz') as r:
                        if r.status == 200:
                            break
                except aiohttp.ClientError:
                    pass
                await asyncio.sleep(.025)
            else:
                raise AssertionError('isolated container failed readiness')
            async with client.post(url + '/v1/pair', json={
                'code': credentials['pair_code'], 'device_name': 'Synthetic phone'}) as r:
                assert r.status == 200
                phone = await r.json()
            done = asyncio.Event()
            async def observe():
                while not done.is_set():
                    start = time.perf_counter()
                    async with client.get(url + '/healthz') as r:
                        await r.read()
                        assert r.status == 200
                        health.append((time.perf_counter() - start) * 1000)
                    await asyncio.sleep(.01)
            task = asyncio.create_task(observe())
            try:
                with tempfile.TemporaryDirectory(prefix=identity) as directory:
                    async def upload(index):
                        path = Path(directory) / f'{index}.json'
                        request_id = await asyncio.to_thread(write_fixture, path, credentials['node_id'],
                            'attachments_unicode' if kind == 'late-malformed' else kind)
                        if kind == 'late-malformed':
                            with path.open('ab') as stream:
                                stream.write(b' garbage')
                        async def chunks():
                            with path.open('rb') as stream:
                                while chunk := stream.read(65536):
                                    yield chunk
                                    await asyncio.sleep(0)
                        async with client.post(url + '/v1/requests', data=chunks(), headers={
                            'Authorization': 'Bearer ' + phone['device_token'],
                            'Content-Type': 'application/json', 'Content-Length': str(path.stat().st_size)}) as r:
                            payload = await r.json()
                            statuses.append(r.status)
                            memory_stages.append(('upload', *memory_kib(pid)))
                            assert r.status in (202, 400, 413, 429), (r.status, payload)
                            if r.status == 202:
                                assert kind == 'attachments_unicode'
                                admitted.append(request_id)
                    for _ in range(repeats):
                        await asyncio.gather(*(upload(i) for i in range(concurrency)))
                    if kind == 'attachments_unicode':
                        assert admitted, 'the overload check must also exercise successful delivery'
                    if backend == 'postgres':
                        # Suspend only our owned disposable database. HTTP health
                        # stays responsive; readiness must fail and then recover.
                        docker('pause', database)
                        try:
                            await asyncio.sleep(1.1)
                            async with client.get(url + '/healthz') as r:
                                assert r.status == 200
                            async with client.get(url + '/readyz') as r:
                                assert r.status == 503
                        finally:
                            docker('unpause', database)
                        for _ in range(100):
                            async with client.get(url + '/readyz') as r:
                                if r.status == 200:
                                    break
                            await asyncio.sleep(.1)
                        else:
                            raise AssertionError('readiness failed to recover after database outage')
                    claimed = []
                    for _ in admitted:
                        async with client.get(url + '/v1/node/requests', headers={
                            'Authorization': 'Bearer ' + credentials['node_token']}) as r:
                            assert r.status == 200
                            raw = await r.read()
                        memory_stages.append(('claim', *memory_kib(pid)))
                        reply = await asyncio.to_thread(json.loads, raw)
                        del raw
                        command = reply['requests'][0]
                        claimed.append(command['request_id'])
                        assert len(command['payload']['attachments']) == 2
                        del command, reply
                        async with client.post(url + '/v1/node/requests/' + claimed[-1] + '/result',
                            headers={'Authorization': 'Bearer ' + credentials['node_token']},
                            json={'state': 'completed', 'result': {'ok': True}, 'error': None}) as r:
                            assert r.status == 200
                    assert sorted(admitted) == sorted(claimed)
                    async with client.get(url + '/v1/node/requests', headers={
                        'Authorization': 'Bearer ' + credentials['node_token']}) as r:
                        assert (await r.json())['requests'] == []
                    for request_id in admitted:
                        async with client.get(url + '/v1/requests/' + request_id,
                            headers={'Authorization': 'Bearer ' + phone['device_token']}) as r:
                            assert (await r.json())['state'] == 'completed'
            finally:
                done.set()
                await task
        events = docker('exec', identity, 'cat', '/sys/fs/cgroup/memory.events')
        peak = int(docker('exec', identity, 'cat', '/sys/fs/cgroup/memory.peak'))
        assert dict(line.split() for line in events.splitlines())['oom_kill'] == '0'
        await asyncio.sleep(.05)
        resting, hwm = memory_kib(pid)
        assert resting < 64 * 1024, 'large allocations must not accumulate in idle pooled connections'
        health.sort()
        if backend == 'postgres':
            docker('pause', database)
            try:
                await asyncio.sleep(1.1)
                docker('stop', '--time', '10', identity)
                assert docker('inspect', '--format', '{{.State.ExitCode}}', identity) == '0', \
                    'shutdown must finish even when the database cannot acknowledge cleanup'
            finally:
                docker('unpause', database)
        return dict(backend=backend, kind=kind, repeats=repeats, concurrency=concurrency,
            statuses=statuses, claims=len(admitted), cgroup_peak_mib=round(peak / 2**20, 2),
            rss_high_water_mib=round(hwm / 1024, 2), resting_rss_mib=round(resting / 1024, 2), health_samples=len(health),
            health_p99_ms=round(health[min(len(health)-1, int(len(health)*.99))], 2),
            oom_kills=0, memory_stages_kib=memory_stages)
    except BaseException:
        if started:
            print(docker('inspect', '--format', '{{.State.OOMKilled}} {{.State.ExitCode}}', identity), file=sys.stderr)
        raise
    finally:
        subprocess.run(['docker', 'rm', '--force', identity], stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL, check=False)
        subprocess.run(['docker', 'volume', 'rm', data, spool], stdout=subprocess.DEVNULL, check=True)
        if database:
            subprocess.run(['docker', 'rm', '--force', database], stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL, check=False)
        if network:
            subprocess.run(['docker', 'network', 'rm', network], stdout=subprocess.DEVNULL, check=False)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', default='zerus-relay-contract')
    parser.add_argument('--kind', choices=['attachments_unicode', 'late-malformed', 'dense',
        'malformed', 'unicode', 'unicode_escaped', 'unicode_mixed', 'unicode_all'], default='attachments_unicode')
    parser.add_argument('--repeats', type=int, choices=range(1, 11), default=5)
    parser.add_argument('--concurrency', type=int, choices=range(1, 5), default=1)
    parser.add_argument('--memory-limit', type=int, choices=[256, 512], default=256)
    parser.add_argument('--backend', choices=['sqlite', 'postgres'], default='sqlite')
    args = parser.parse_args()
    print(json.dumps(asyncio.run(profile(args.image, args.kind, args.repeats, args.concurrency,
        args.memory_limit, args.backend)), indent=2))
