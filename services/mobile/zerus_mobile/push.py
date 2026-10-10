"""Generic FCM wake pushes. Payloads carry event IDs only, never content."""
from __future__ import annotations

import asyncio
from datetime import timedelta
import json


class PushWorker:
    def __init__(self, store, config):
        from .async_store import adapt_store
        self.store, self.config = adapt_store(store), config
        self._owns_adapter = self.store is not store
        self.fcm = None
        self.messaging = None
        if config.fcm_credentials:
            import firebase_admin
            from firebase_admin import credentials, messaging
            self.messaging = messaging
            self.fcm = firebase_admin.initialize_app(credentials.Certificate(config.fcm_credentials), options={"httpTimeout": 10}, name="zerus-relay-" + str(id(self)))

    async def deliver(self, provider, target, payload):
        if provider == "fcm" and self.fcm:
            message = self.messaging.Message(token=target, data={key: str(value) for key, value in payload.items()}, android=self.messaging.AndroidConfig(priority="high", ttl=timedelta(seconds=300)))
            try:
                await asyncio.to_thread(self.messaging.send, message, app=self.fcm)
                return True, False
            except self.messaging.UnregisteredError:
                return False, True
        # Registrations from retired providers such as UnifiedPush can never deliver.
        return False, provider != "fcm"

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
