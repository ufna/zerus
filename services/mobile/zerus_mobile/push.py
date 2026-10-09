"""Generic wake pushes. Targets are administrator-allowed and DNS is pinned."""
from __future__ import annotations

import asyncio
from datetime import timedelta
import ipaddress
import json
import socket
import time
from urllib.parse import urlsplit

import aiohttp
from aiohttp.abc import AbstractResolver


class PinnedResolver(AbstractResolver):
    def __init__(self, host: str, addresses: list[str]):
        self.host, self.addresses = host, addresses

    async def resolve(self, host, port=0, family=socket.AF_INET):
        if host != self.host:
            raise OSError("unexpected push hostname")
        return [{"hostname": host, "host": address, "port": port, "family": socket.AF_INET6 if ":" in address else socket.AF_INET, "proto": 0, "flags": socket.AI_NUMERICHOST} for address in self.addresses]

    async def close(self):
        pass


class PushWorker:
    def __init__(self, store, config):
        from .async_store import adapt_store
        self.store, self.config = adapt_store(store), config
        self._owns_adapter = self.store is not store
        self.allowed = {h.encode("idna").decode("ascii").lower() for h in config.push_hosts}
        self.fcm = None
        self.messaging = None
        if config.fcm_credentials:
            import firebase_admin
            from firebase_admin import credentials, messaging
            self.messaging = messaging
            self.fcm = firebase_admin.initialize_app(credentials.Certificate(config.fcm_credentials), options={"httpTimeout": 10}, name="zerus-relay-" + str(id(self)))

    @staticmethod
    def public_address(address):
        ip = ipaddress.ip_address(address)
        if ip.version == 6 and ip.ipv4_mapped:
            ip = ip.ipv4_mapped
        return ip.is_global and not ip.is_multicast and not ip.is_unspecified

    async def validate_endpoint(self, endpoint: str):
        try:
            parsed = urlsplit(endpoint)
            if parsed.scheme != "https" or parsed.username or parsed.password or parsed.fragment or parsed.port not in (None, 443) or not parsed.hostname:
                raise ValueError("invalid endpoint")
            host = parsed.hostname.encode("idna").decode("ascii").lower()
            if host not in self.allowed or parsed.hostname.endswith("."):
                raise ValueError("host is not allowed")
            loop = asyncio.get_running_loop()
            addresses = sorted({row[4][0] for row in await asyncio.wait_for(loop.getaddrinfo(host, 443, type=socket.SOCK_STREAM), timeout=3)})
            if not addresses or any(not self.public_address(address) for address in addresses):
                raise ValueError("nonpublic push target")
            return host, addresses
        except (OSError, UnicodeError, asyncio.TimeoutError) as exc:
            raise ValueError("invalid push target") from exc

    async def deliver(self, provider, target, payload):
        if provider == "unifiedpush":
            host, addresses = await self.validate_endpoint(target)
            connector = aiohttp.TCPConnector(resolver=PinnedResolver(host, addresses), use_dns_cache=False)
            async with aiohttp.ClientSession(connector=connector, timeout=aiohttp.ClientTimeout(total=10), trust_env=False) as session:
                async with session.post(target, json=payload, allow_redirects=False) as response:
                    # Never read/log a provider response containing an endpoint token.
                    return 200 <= response.status < 300, response.status in (404, 410)
        if provider == "fcm" and self.fcm:
            message = self.messaging.Message(token=target, data={key: str(value) for key, value in payload.items()}, android=self.messaging.AndroidConfig(priority="high", ttl=timedelta(seconds=300)))
            try:
                await asyncio.to_thread(self.messaging.send, message, app=self.fcm)
                return True, False
            except self.messaging.UnregisteredError:
                return False, True
        return False, False

    async def one(self, job):
        # Refresh registration and revocation immediately before network delivery.
        registration = await self.store.push_registration(job["device_id"])
        if not registration:
            await self.store.finish_push(job, False, False, None)
            return
        try:
            delivered, invalid = await self.deliver(registration["provider"], registration["target"], json.loads(job["payload"]))
        except Exception:
            # Transient failures stay generic. No exception/URL/token is logged.
            delivered, invalid = False, False
        await self.store.finish_push(job, delivered, invalid, registration)

    async def once(self):
        jobs = await self.store.claim_push_jobs(limit=4, lease_ttl=60)
        await asyncio.gather(*(self.one(job) for job in jobs))

    async def close(self):
        if self._owns_adapter:
            await self.store.close()
        if self.fcm:
            import firebase_admin
            firebase_admin.delete_app(self.fcm)
