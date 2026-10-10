"""Run installed connector contracts against either relay implementation.

ZERUS_RELAY_BINARY selects the Rust server; no fixture command touches a native
agent, deployed database, host configuration or non-fixture service.
"""
import asyncio
from dataclasses import asdict
import json
import os
from pathlib import Path
import socket
import tempfile

from aiohttp import ClientSession, web
from zerus_mobile.server import create_app


async def start_relay(store, config):
    binary = os.environ.get('ZERUS_RELAY_BINARY')
    if not binary:
        runner = web.AppRunner(create_app(store, config))
        await runner.setup()
        site = web.TCPSite(runner, '127.0.0.1', 0)
        await site.start()
        return runner, 'http://127.0.0.1:' + str(site._server.sockets[0].getsockname()[1])
    path = store.db.execute('PRAGMA database_list').fetchone()[2]
    with socket.socket() as reservation:
        reservation.bind(('127.0.0.1', 0))
        port = reservation.getsockname()[1]
    temporary = tempfile.TemporaryDirectory()
    configuration = Path(temporary.name) / 'relay.json'
    configuration.write_text(json.dumps(asdict(config)))
    configuration.chmod(0o600)
    process = await asyncio.create_subprocess_exec(
        str(Path(binary).resolve()), '--database', path, '--config', str(configuration),
        'serve', '--port', str(port), stdout=asyncio.subprocess.DEVNULL,
        stderr=asyncio.subprocess.PIPE,
    )
    runner = RustRunner(process, temporary)
    url = f'http://127.0.0.1:{port}'
    try:
        async with ClientSession() as client:
            for _ in range(200):
                if process.returncode is not None:
                    raise RuntimeError('Rust relay exited during fixture startup')
                try:
                    async with client.get(url + '/readyz') as response:
                        if response.status == 200:
                            return runner, url
                except OSError:
                    pass
                await asyncio.sleep(0.025)
        raise RuntimeError('Rust relay fixture did not become ready')
    except BaseException:
        await runner.cleanup()
        raise


class RustRunner:
    def __init__(self, process, temporary):
        self.process, self.temporary = process, temporary

    async def cleanup(self):
        if self.process.returncode is None:
            self.process.terminate()
            try:
                await asyncio.wait_for(self.process.wait(), 45)
            except asyncio.TimeoutError:
                self.process.kill()
                await self.process.wait()
        self.temporary.cleanup()
