"""Bounded authenticated v1 HTTP API for the trusted relay."""
from __future__ import annotations

import asyncio
from collections import OrderedDict, deque
from dataclasses import dataclass
import json
import time
import uuid

from aiohttp import web

from .attachments import LIMITS as ATTACHMENT_LIMITS, MAX_REQUEST_BYTES, MAX_QUEUE_BYTES, validate_send
from .recovery import validate_recovery
from .history import validate_history
from .context import CONTEXT_COMMANDS, LIFECYCLE_OPERATIONS, OPERATIONS, FEATURES, validate_context, validate_core, validate_inspect, validate_lifecycle
from .launch import OPERATIONS as LAUNCH_OPERATIONS, validate as validate_launch
from .store import Store, digest
from .terminal import OPERATIONS as TERMINAL_OPERATIONS, validate as validate_terminal


@dataclass
class Config:
    queue_ttl: int = 120
    claim_ttl: int = 90
    retention: int = 7 * 86400
    max_queue: int = 200
    max_queue_bytes: int = MAX_QUEUE_BYTES
    online_timeout: int = 45
    push_hosts: tuple[str, ...] = ()
    fcm_credentials: str | None = None
    background: bool = True


STORE = web.AppKey("store", Store)
CONFIG = web.AppKey("config", Config)
LIMITS = web.AppKey("limits", object)
PUSH_WORKER = web.AppKey("push_worker", object)
UPLOADS = web.AppKey("uploads", object)


AUTHENTICATED_REQUEST_LIMIT = 2400
IP_REQUEST_LIMIT = 2400
GLOBAL_REQUEST_LIMIT = 6000


class RateLimits:
    def __init__(self):
        self.entries = OrderedDict()

    def check(self, key, limit=120, seconds=60):
        now = time.monotonic()
        values = self.entries.setdefault(key, deque())
        self.entries.move_to_end(key)
        while values and values[0] < now - seconds:
            values.popleft()
        if len(values) >= limit:
            raise web.HTTPTooManyRequests(text='{"error":"rate limit exceeded"}', content_type="application/json", headers={"Retry-After": "60"})
        values.append(now)
        while len(self.entries) > 10000:
            self.entries.popitem(last=False)


@web.middleware
async def protect(request, handler):
    try:
        request.app[LIMITS].check(("global",), limit=GLOBAL_REQUEST_LIMIT)
        request.app[LIMITS].check(("ip", request.remote), limit=IP_REQUEST_LIMIT)
        if request.headers.get("Content-Encoding", "identity") != "identity":
            raise web.HTTPUnsupportedMediaType()
        deadline = 75 if request.method == "POST" and request.path == "/v1/requests" else 40
        response = await asyncio.wait_for(handler(request), timeout=deadline)
    except web.HTTPException as exc:
        if exc.content_type == "application/json":
            response = web.Response(body=exc.body, status=exc.status, headers=exc.headers)
        else:
            response = web.json_response({"error": exc.reason}, status=exc.status, headers={k: v for k, v in exc.headers.items() if k.lower() not in ("content-type", "content-length")})
    except (ValueError, TypeError, KeyError, RecursionError):
        response = web.json_response({"error": "invalid JSON request"}, status=400)
    except asyncio.TimeoutError:
        response = web.json_response({"error": "request timed out"}, status=408)
    response.headers["Cache-Control"] = "no-store"
    response.headers["X-Content-Type-Options"] = "nosniff"
    return response


def auth(request, role):
    authorization = request.headers.get("Authorization", "")
    if not authorization.startswith("Bearer ") or len(authorization) > 256:
        request.app[LIMITS].check(("login", request.remote), limit=20)
        raise web.HTTPUnauthorized()
    secret = authorization[7:]
    row = request.app[STORE].authenticate(secret, role)
    if row is None:
        request.app[LIMITS].check(("login", request.remote), limit=20)
        raise web.HTTPUnauthorized()
    request.app[LIMITS].check((role, digest(secret)), limit=AUTHENTICATED_REQUEST_LIMIT)
    return row


def still_authorized(request, row, role):
    live = request.app[STORE].db.execute(f"SELECT 1 FROM {role} WHERE id=? AND revoked=0", (row["id"],)).fetchone()
    if not live:
        raise web.HTTPUnauthorized()


async def body(request, required, optional=(), maximum=1024 * 1024, timeout=10):
    if request.content_type != "application/json":
        raise web.HTTPUnsupportedMediaType()
    if request.content_length is not None and request.content_length > maximum:
        raise web.HTTPRequestEntityTooLarge(max_size=maximum, actual_size=request.content_length)
    raw = bytearray()
    async with asyncio.timeout(timeout):
        async for chunk in request.content.iter_chunked(65536):
            if len(raw) + len(chunk) > maximum:
                raise web.HTTPRequestEntityTooLarge(max_size=maximum, actual_size=len(raw) + len(chunk))
            raw.extend(chunk)
    parse = lambda: json.loads(raw, parse_constant=lambda _: (_ for _ in ()).throw(ValueError("nonfinite JSON")))
    value = await asyncio.to_thread(parse) if len(raw) > 1024 * 1024 else parse()
    if not isinstance(value, dict) or set(value) - set(required) - set(optional) or not set(required) <= set(value):
        raise web.HTTPBadRequest()
    return value


def text(value, maximum=1024, empty=False):
    if not isinstance(value, str) or len(value) > maximum or (not empty and not value) or any(ord(c) < 32 for c in value):
        raise web.HTTPBadRequest()
    return value


def maintain(request):
    cfg = request.app[CONFIG]
    request.app[STORE].maintain(queue_ttl=cfg.queue_ttl, claim_ttl=cfg.claim_ttl, retention=cfg.retention)


def wait_seconds(request):
    value = float(request.query.get("wait", "0"))
    if not 0 <= value <= 25:
        raise web.HTTPBadRequest()
    return value


async def health(request):
    return web.json_response({"ok": True, "protocol_version": 1})


async def pair(request):
    request.app[LIMITS].check(("pair", request.remote), limit=10)
    request.app[LIMITS].check(("pair-global",), limit=100)
    value = await body(request, ("code", "device_name"))
    result = request.app[STORE].pair(text(value["code"], 128), text(value["device_name"], 128))
    if result is None:
        raise web.HTTPUnauthorized()
    return web.json_response(result)


async def capabilities(request):
    auth(request, "devices")
    cfg = request.app[CONFIG]
    providers = []
    if cfg.push_hosts:
        providers.append("unifiedpush")
    if request.app[PUSH_WORKER].fcm is not None:
        providers.append("fcm")
    return web.json_response({"protocol_version": 1, "push_providers": providers, "attachment_limits": ATTACHMENT_LIMITS, "operations": sorted(OPERATIONS), "features": sorted(FEATURES)})


async def computers(request):
    device = auth(request, "devices")
    now = time.time()
    rows = request.app[STORE].db.execute("SELECT * FROM nodes WHERE workspace_id=? AND revoked=0 ORDER BY name,id", (device["workspace_id"],)).fetchall()
    return web.json_response({"computers": [{"id": row["id"], "name": row["name"], "online": row["last_seen"] is not None and now - row["last_seen"] < request.app[CONFIG].online_timeout, "last_seen_at": row["last_seen"], "snapshot": json.loads(row["snapshot"]) if row["snapshot"] else None} for row in rows]})


def validate_request(value):
    request_id = text(value["request_id"], 36)
    if str(uuid.UUID(request_id)) != request_id:
        raise web.HTTPBadRequest()
    text(value["computer_id"], 36)
    text(value["session"], 1024, empty=value["operation"] in LAUNCH_OPERATIONS)
    op, payload = value["operation"], value["payload"]
    if not isinstance(op, str) or op not in OPERATIONS or not isinstance(payload, dict):
        raise web.HTTPBadRequest()
    if op in LAUNCH_OPERATIONS:
        if value["session"] != "": raise ValueError("catalog/launch use no existing session target")
        validate_launch(op,payload,request_id)
        return
    if op == "inspect":
        validate_inspect(payload)
        if "archive_id" in payload:
            archive_id = text(payload["archive_id"], 36)
            if str(uuid.UUID(archive_id)) != archive_id:
                raise web.HTTPBadRequest()
        return
    if op == "recovery_action":
        validate_recovery(payload,request_id)
        return
    if op == "history":
        validate_history(payload,request_id)
        return
    if op in TERMINAL_OPERATIONS:
        validate_terminal(op,payload,request_id)
        return
    if op in LIFECYCLE_OPERATIONS:
        validate_lifecycle(op, payload, request_id)
        return
    if op in CONTEXT_COMMANDS:
        validate_context(payload, request_id)
        return
    if op in {"send_now", "settings", "process_output", "process_stop"}:
        validate_core(op, payload, request_id)
        return
    if payload.get("request_id") != request_id:
        raise web.HTTPBadRequest()
    text(payload.get("expected_run_id"), 128)
    text(payload.get("expected_conversation_id"), 256, empty=True)
    base = {"request_id", "expected_run_id", "expected_conversation_id"}
    extras = {"send": {"text", "attachments", "expected_compaction_id", "agent_id"}, "answer": {"question_id", "expected_question_hash", "answers"}, "interrupt": {"expected_turn_started"}}[op]
    if set(payload) - base - extras:
        raise web.HTTPBadRequest()
    required = {"send": {"text"}, "answer": {"question_id", "expected_question_hash", "answers"}, "interrupt": {"expected_turn_started"}}[op]
    if not required <= set(payload):
        raise web.HTTPBadRequest()


async def submit(request):
    device = auth(request, "devices")
    large = request.content_length is None or request.content_length > 1024 * 1024
    slots = request.app[UPLOADS]
    if large:
        if slots.locked():
            raise web.HTTPTooManyRequests(text='{"error":"relay upload capacity is busy"}', content_type="application/json")
        await slots.acquire()
    try:
        value = await body(request, ("request_id", "computer_id", "operation", "session", "payload"), maximum=MAX_REQUEST_BYTES, timeout=60)
        validate_request(value)
        if value["operation"] == "send":
            await asyncio.to_thread(validate_send, value["payload"])
        elif len(json.dumps(value).encode()) > 1024 * 1024:
            raise web.HTTPRequestEntityTooLarge(max_size=1024 * 1024, actual_size=len(json.dumps(value).encode()))
        still_authorized(request, device, "devices")
        maintain(request)
        status, result = request.app[STORE].submit(device, value, request.app[CONFIG].max_queue, request.app[CONFIG].max_queue_bytes)
        return web.json_response(result, status=status)
    finally:
        if large:
            slots.release()


async def get_request(request):
    device = auth(request, "devices")
    maintain(request)
    row = request.app[STORE].db.execute("SELECT * FROM requests WHERE id=? AND device_id=?", (request.match_info["id"], device["id"])).fetchone()
    if row is None:
        raise web.HTTPNotFound()
    if row["operation"] == "terminal_snapshot" and row["state"] not in {"queued", "claimed"}:
        with request.app[STORE].db:
            request.app[STORE].db.execute("UPDATE requests SET result_read=? WHERE id=?", (time.time(),row["id"]))
    return web.json_response(Store.envelope(row))


async def heartbeat(request):
    node = auth(request, "nodes")
    value = await body(request, ("snapshot",))
    snap = value["snapshot"]
    if not isinstance(snap, dict) or not isinstance(snap.get("sessions"), list) or len(snap["sessions"]) > 5000:
        raise web.HTTPBadRequest()
    still_authorized(request, node, "nodes")
    request.app[STORE].heartbeat(node, snap)
    return web.json_response({"ok": True})


async def node_requests(request):
    node = auth(request, "nodes")
    deadline = time.monotonic() + wait_seconds(request)
    while True:
        still_authorized(request, node, "nodes")
        maintain(request)
        rows = request.app[STORE].claim(node)
        if rows or time.monotonic() >= deadline:
            return web.json_response({"requests": rows})
        await asyncio.sleep(min(0.25, max(0, deadline - time.monotonic())))


async def node_result(request):
    node = auth(request, "nodes")
    value = await body(request, ("state", "result", "error"))
    if value["state"] not in ("completed", "failed", "uncertain"):
        raise web.HTTPBadRequest()
    if value["error"] is not None:
        # Errors are data, never placed in logs.
        if not isinstance(value["error"], str) or len(value["error"]) > 4096:
            raise web.HTTPBadRequest()
    still_authorized(request, node, "nodes")
    maintain(request)
    status = request.app[STORE].result(node, request.match_info["id"], value)
    return web.json_response({"ok": status == 200}, status=status)


async def events(request):
    device = auth(request, "devices")
    after = int(request.query.get("after", "0"))
    if after < 0:
        raise web.HTTPBadRequest()
    deadline = time.monotonic() + wait_seconds(request)
    while True:
        still_authorized(request, device, "devices")
        rows = request.app[STORE].db.execute("SELECT * FROM events WHERE workspace_id=? AND id>? ORDER BY id LIMIT 100", (device["workspace_id"], after)).fetchall()
        if rows or time.monotonic() >= deadline:
            return web.json_response({"events": [{"id": row["id"], "computer_id": row["node_id"], "session": row["session"], "kind": row["kind"], "created_at": row["created"]} for row in rows], "cursor": rows[-1]["id"] if rows else after})
        await asyncio.sleep(min(0.25, max(0, deadline - time.monotonic())))


async def push_register(request):
    device = auth(request, "devices")
    value = await body(request, ("provider",), ("endpoint", "token"))
    worker = request.app[PUSH_WORKER]
    if value["provider"] == "unifiedpush":
        if set(value) != {"provider", "endpoint"}:
            raise web.HTTPBadRequest()
        target = text(value["endpoint"], 4096)
        try:
            await worker.validate_endpoint(target)
        except ValueError:
            raise web.HTTPBadRequest(text='{"error":"push endpoint is not allowed"}', content_type="application/json")
    elif value["provider"] == "fcm":
        if set(value) != {"provider", "token"}:
            raise web.HTTPBadRequest()
        if worker.fcm is None:
            raise web.HTTPServiceUnavailable(text='{"error":"FCM is not configured"}', content_type="application/json")
        target = text(value["token"], 4096)
    else:
        raise web.HTTPBadRequest()
    still_authorized(request, device, "devices")
    with request.app[STORE].db:
        request.app[STORE].db.execute("INSERT INTO pushes VALUES(?,?,?) ON CONFLICT(device_id) DO UPDATE SET provider=excluded.provider,target=excluded.target", (device["id"], value["provider"], target))
        request.app[STORE].db.execute("DELETE FROM push_jobs WHERE device_id=?", (device["id"],))
    return web.json_response({"ok": True})


async def push_delete(request):
    device = auth(request, "devices")
    with request.app[STORE].db:
        request.app[STORE].db.execute("DELETE FROM pushes WHERE device_id=?", (device["id"],))
        request.app[STORE].db.execute("DELETE FROM push_jobs WHERE device_id=?", (device["id"],))
    return web.json_response({"ok": True})


async def device_delete(request):
    device = auth(request, "devices")
    request.app[STORE].revoke("devices", device["id"])
    return web.json_response({"ok": True})


async def background(app):
    try:
        while True:
            cfg = app[CONFIG]
            app[STORE].maintain(queue_ttl=cfg.queue_ttl, claim_ttl=cfg.claim_ttl, retention=cfg.retention)
            await app[PUSH_WORKER].once()
            await asyncio.sleep(1)
    except asyncio.CancelledError:
        pass


async def lifecycle(app):
    from .push import PushWorker
    app[PUSH_WORKER] = PushWorker(app[STORE], app[CONFIG])
    app[STORE].maintain(restart=True, queue_ttl=app[CONFIG].queue_ttl, claim_ttl=app[CONFIG].claim_ttl, retention=app[CONFIG].retention)
    task = asyncio.create_task(background(app)) if app[CONFIG].background else None
    yield
    if task:
        task.cancel()
        await task
    await app[PUSH_WORKER].close()


def create_app(store: Store, config: Config | None = None) -> web.Application:
    app = web.Application(client_max_size=1024 * 1024, middlewares=[protect])
    app[STORE], app[CONFIG], app[LIMITS] = store, config or Config(), RateLimits()
    app[UPLOADS] = asyncio.Semaphore(2)
    app.cleanup_ctx.append(lifecycle)
    app.add_routes([
        web.get("/healthz", health), web.post("/v1/pair", pair),
        web.get("/v1/capabilities", capabilities), web.get("/v1/computers", computers),
        web.post("/v1/requests", submit), web.get("/v1/requests/{id}", get_request),
        web.post("/v1/node/heartbeat", heartbeat), web.get("/v1/node/requests", node_requests),
        web.post("/v1/node/requests/{id}/result", node_result), web.get("/v1/events", events),
        web.post("/v1/push", push_register), web.delete("/v1/push", push_delete),
        web.delete("/v1/device", device_delete),
    ])
    return app
