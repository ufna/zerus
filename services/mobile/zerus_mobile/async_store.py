"""Async storage boundary and bounded compatibility worker for SQLite.

SQLite is deliberately a single-process development backend. Production workers
share PostgreSQL; no database connection is held by notification waiters.
"""

from __future__ import annotations

import asyncio
from collections import OrderedDict
from concurrent.futures import ThreadPoolExecutor
import functools
import time
import uuid

from .store import Store


class StoreBusy(RuntimeError):
    """The bounded database worker is at capacity."""


class Notifications:
    def __init__(self):
        self._topics = OrderedDict()
        self._serial = 0

    def generation(self, topic):
        entry = self._topics.get(topic)
        if entry is None:
            entry = (0, asyncio.Event(), 0)
            self._topics[topic] = entry
        self._topics.move_to_end(topic)
        self._prune_topics()
        return entry[0]

    def _prune_topics(self):
        if len(self._topics) <= 10000:
            return
        while len(self._topics) > 10000:
            key = next(
                (key for key, value in self._topics.items() if value[2] == 0), None
            )
            if key is None:
                break
            del self._topics[key]

    def notify(self, topic):
        self._serial += 1
        entry = self._topics.get(topic)
        if entry:
            self._topics[topic] = (entry[0] + 1, asyncio.Event(), entry[2])
            entry[1].set()

    async def wait(self, topic, after, timeout):
        # Bounded topic generations survive the query-to-wait handoff; unrelated
        # tenants cannot cause immediate retry loops.
        entry = self._topics.get(topic)
        if entry is None:
            entry = (0, asyncio.Event(), 0)
        generation, event, count = entry
        self._topics[topic] = (generation, event, count + 1)
        try:
            if generation != after:
                return
            await asyncio.wait_for(event.wait(), max(0.001, min(timeout, 5)))
        except asyncio.TimeoutError:
            pass
        finally:
            current = self._topics[topic]
            if current[2] == 1:
                self._topics[topic] = (current[0], current[1], 0)
            else:
                self._topics[topic] = (current[0], current[1], current[2] - 1)


class SyncStoreAdapter(Notifications):
    def __init__(self, store, *, max_pending=64):
        super().__init__()
        self.store = store
        self.executor = ThreadPoolExecutor(
            max_workers=1, thread_name_prefix="relay-sqlite"
        )
        self.slots = asyncio.Semaphore(max_pending)
        self._leases = {}
        self._rates = {1: OrderedDict(), 2: OrderedDict()}
        self._closed = False

    async def _call(self, method, *args, **kwargs):
        if self._closed:
            raise RuntimeError("store is closed")
        # asyncio cancellation cannot stop SQLite in the worker. Keep the slot
        # until the actual operation finishes; this prevents unbounded queuing.
        if self.slots.locked():
            raise StoreBusy("database worker capacity exhausted")
        await self.slots.acquire()
        try:
            future = asyncio.get_running_loop().run_in_executor(
                self.executor,
                functools.partial(getattr(self.store, method), *args, **kwargs),
            )
        except BaseException:
            self.slots.release()
            raise
        future.add_done_callback(lambda _: self.slots.release())
        try:
            try:
                value = await asyncio.shield(future)
            except asyncio.CancelledError:
                while not future.done():
                    try:
                        await asyncio.shield(future)
                    except asyncio.CancelledError:
                        pass
                    except Exception:
                        break
                if future.done() and not future.cancelled():
                    future.exception()
                raise
            if hasattr(value, "keys") and not isinstance(value, dict):
                return dict(value)
            return value
        finally:
            # Executor exceptions keep their traceback frames. Break the
            # future -> traceback -> this frame -> future cycle immediately.
            future = None
            args = ()
            kwargs = {}

    async def authenticate(self, secret, role):
        return await self._call("authenticate", secret, role)

    async def authorized(self, value, role):
        return await self._call("authorized", value, role)

    async def pair(self, code, name):
        return await self._call("pair", code, name)

    async def computers(self, workspace, max_bytes=1024 * 1024):
        return await self._call("computers", workspace, max_bytes=max_bytes)

    async def ready(self):
        return await self._call("ready")

    async def submit(self, device, body, max_queue, max_bytes):
        value = await self._call("submit", device, body, max_queue, max_bytes)
        if value[0] == 202:
            self.notify("node:" + body["computer_id"])
        return value

    async def get_request(self, device, value, queue_ttl=120, claim_ttl=90):
        return await self._call(
            "get_request", device, value, queue_ttl=queue_ttl, claim_ttl=claim_ttl
        )

    async def has_pending(self, node, queue_ttl=120):
        return await self._call("has_pending", node, queue_ttl=queue_ttl)

    async def claim(self, node, queue_ttl=120, claim_ttl=90):
        return await self._call("claim", node, queue_ttl=queue_ttl, claim_ttl=claim_ttl)

    async def result(self, node, value, body, claim_ttl=90):
        return await self._call("result", node, value, body, claim_ttl=claim_ttl)

    async def heartbeat(self, node, snapshot):
        changed = await self._call("heartbeat", node, snapshot)
        if changed is not False:
            self.notify("workspace:" + node["workspace_id"])

    async def events(self, workspace, after, max_bytes=1024 * 1024):
        return await self._call("events", workspace, after, max_bytes=max_bytes)

    async def register_push(self, device, provider, target):
        return await self._call("register_push", device, provider, target)

    async def delete_push(self, device):
        return await self._call("delete_push", device)

    async def revoke(self, role, value):
        found = await self._call("revoke", role, value)
        self.notify("node:" + value)
        return found

    async def maintain(self, **kwargs):
        return await self._call("maintain", **kwargs)

    async def claim_push_jobs(self, limit=4, lease_ttl=60):
        return await self._call("claim_push_jobs", limit, lease_ttl)

    async def push_registration(self, device):
        return await self._call("push_registration", device)

    async def finish_push(self, job, delivered, invalid, registration):
        return await self._call("finish_push", job, delivered, invalid, registration)

    async def acquire_poll(
        self, role, credential_id, workspace_id, ttl, max_credential, max_workspace
    ):
        now = time.monotonic()
        self._leases = {k: v for k, v in self._leases.items() if v[3] > now}
        if (
            sum(v[0:2] == (role, credential_id) for v in self._leases.values())
            >= max_credential
            or sum(v[2] == workspace_id for v in self._leases.values()) >= max_workspace
        ):
            return None
        lease = str(uuid.uuid4())
        self._leases[lease] = (role, credential_id, workspace_id, now + ttl)
        return lease

    async def release_poll(self, lease):
        self._leases.pop(lease, None)

    async def rate_limit(self, key, limit, seconds, max_entries=100000):
        bucket = (
            2
            if isinstance(key, (tuple, list)) and key and key[0] in ("devices", "nodes")
            else 1
        )
        entries = self._rates[bucket]
        key = repr(key)
        now = time.monotonic()
        if key not in entries and len(entries) >= max_entries:
            for old in list(entries):
                if entries[old][1] <= now:
                    del entries[old]
            if len(entries) >= max_entries:
                return False
        count, expires = entries.get(key, (0, now + seconds))
        if expires <= now:
            count, expires = 0, now + seconds
        entries[key] = (count + 1, expires)
        entries.move_to_end(key)
        return count < limit

    async def close(self):
        if self._closed:
            return
        # App cleanup drains the worker but the caller retains ownership of Store.
        self._closed = True
        await asyncio.to_thread(self.executor.shutdown, wait=True)


def adapt_store(store):
    return SyncStoreAdapter(store) if isinstance(store, Store) else store
