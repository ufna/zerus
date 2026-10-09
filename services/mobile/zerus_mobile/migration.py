"""Offline, lossless SQLite-to-PostgreSQL migration.

The operator must stop the SQLite relay before invoking this command. A read-only
SQLite transaction holds one consistent snapshot throughout the destination's
single transaction. Failed imports leave the destination empty and source intact.
"""

from __future__ import annotations

import asyncio
import hashlib
from pathlib import Path
import sqlite3
import time

from .context import READ_OPERATIONS
from .attachments import MAX_QUEUE_BYTES
from .store import canonical
from .postgres import PostgresStore, RESULT_RESERVE

TABLES = (
    "workspaces",
    "nodes",
    "devices",
    "invitations",
    "requests",
    "events",
    "pushes",
    "push_jobs",
)
COLUMNS = {
    "workspaces": ("id", "name"),
    "nodes": (
        "id",
        "workspace_id",
        "name",
        "token_hash",
        "revoked",
        "last_seen",
        "snapshot",
    ),
    "devices": ("id", "workspace_id", "name", "token_hash", "revoked"),
    "invitations": ("code_hash", "workspace_id", "expires"),
    "requests": (
        "id",
        "workspace_id",
        "device_id",
        "node_id",
        "operation",
        "body_hash",
        "body_bytes",
        "body",
        "state",
        "result",
        "error",
        "created",
        "claimed",
        "updated",
        "expires_at",
        "result_read",
        "result_bytes",
        "reserved_bytes",
    ),
    "events": ("id", "workspace_id", "node_id", "session", "kind", "created"),
    "pushes": ("device_id", "provider", "target"),
    "push_jobs": (
        "id",
        "device_id",
        "event_id",
        "payload",
        "attempts",
        "next_at",
        "created",
    ),
}


def _source(path):
    path = Path(path).resolve(strict=True)
    connection = sqlite3.connect(
        path.as_uri() + "?mode=ro", uri=True, check_same_thread=False
    )
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA query_only=ON")
    connection.execute("BEGIN")
    tables = {
        r[0]
        for r in connection.execute("SELECT name FROM sqlite_master WHERE type='table'")
    }
    if not set(TABLES) <= tables:
        connection.close()
        raise ValueError("source is not a supported SQLite relay database")
    # Reading every schema here establishes the read snapshot before destination
    # initialization, without modifying or running SQLite startup migrations.
    for table in TABLES:
        available = {r[1] for r in connection.execute(f"PRAGMA table_info({table})")}
        required = set(COLUMNS[table]) - (
            {"result_bytes", "reserved_bytes"} if table == "requests" else set()
        )
        if not required <= available:
            connection.close()
            raise ValueError(
                "source requires an up-to-date SQLite schema before offline migration"
            )
    if connection.execute("PRAGMA quick_check").fetchone()[0] != "ok":
        connection.close()
        raise ValueError("source SQLite integrity check failed")
    return connection


async def migrate_sqlite(
    path,
    destination_dsn,
    *,
    global_max_bytes=32 * 1024**3,
    quota_shards=64,
    batch_size=1,
):
    source = await asyncio.to_thread(_source, path)
    store = None
    counts = {}
    digests = {}
    try:
        store = await PostgresStore.open(
            destination_dsn,
            global_max_bytes=global_max_bytes,
            quota_shards=quota_shards,
            _migration_reallocate_empty=True,
        )
        async with store.pool.acquire() as conn, conn.transaction():
            await conn.execute("SELECT pg_advisory_xact_lock(735628109)")
            # ACCESS EXCLUSIVE rejects concurrent provisioning/serving until the
            # atomic import finishes. Guards include all identity/state tables.
            await conn.execute(
                "LOCK TABLE " + ",".join(TABLES) + " IN ACCESS EXCLUSIVE MODE"
            )
            for table in TABLES:
                if await conn.fetchval(f"SELECT EXISTS(SELECT 1 FROM {table})"):
                    raise ValueError("migration destination must be empty")
            if await conn.fetchval(
                "SELECT EXISTS(SELECT 1 FROM poll_leases)"
            ) or await conn.fetchval("SELECT EXISTS(SELECT 1 FROM rate_limits)"):
                raise ValueError("migration destination has active relay state")
            for table in TABLES:
                order = (
                    "code_hash"
                    if table == "invitations"
                    else "device_id" if table == "pushes" else "id"
                )
                cursor = await asyncio.to_thread(
                    source.execute, f"SELECT * FROM {table} ORDER BY {order}"
                )
                content_hash = hashlib.sha256()
                original_columns = [
                    c
                    for c in COLUMNS[table]
                    if c not in ("result_bytes", "reserved_bytes")
                ]
                counts[table] = 0
                while True:
                    rows = await asyncio.to_thread(cursor.fetchmany, min(batch_size, 1))
                    if not rows:
                        break
                    records = []
                    for original in rows:
                        row = dict(original)
                        encoded = await asyncio.to_thread(
                            canonical, [row[c] for c in original_columns]
                        )
                        content_hash.update(encoded.encode() + b"\n")
                        if table == "requests":
                            row["result_bytes"] = len(
                                (row["result"] or "").encode()
                            ) + len((row["error"] or "").encode())
                            row["reserved_bytes"] = (
                                RESULT_RESERVE
                                if row["state"] in ("queued", "claimed")
                                else 0
                            )
                            # Imported claimed commands are never requeued. Leave
                            # their durable claim timestamps for ordinary timeout
                            # uncertainty; no worker-start mass transition occurs.
                        records.append(tuple(row[c] for c in COLUMNS[table]))
                    await conn.copy_records_to_table(
                        table, records=records, columns=COLUMNS[table]
                    )
                    counts[table] += len(records)
                destination_hash = hashlib.sha256()
                query = (
                    f"SELECT {','.join(original_columns)} FROM {table} ORDER BY {order}"
                )
                async for row in conn.cursor(query, prefetch=1):
                    encoded = await asyncio.to_thread(canonical, list(row))
                    destination_hash.update(encoded.encode() + b"\n")
                if content_hash.digest() != destination_hash.digest():
                    raise ValueError("migration content verification failed")
                digests[table] = content_hash.hexdigest()
            await conn.execute(
                "INSERT INTO workspace_usage(workspace_id) SELECT id FROM workspaces ON CONFLICT DO NOTHING"
            )
            reads = list(READ_OPERATIONS)
            await conn.execute(
                "UPDATE workspace_usage u SET payload_bytes=(SELECT COALESCE(sum(body_bytes+result_bytes+reserved_bytes),0) FROM requests r WHERE r.workspace_id=u.workspace_id),active=(SELECT count(*) FROM requests r WHERE r.workspace_id=u.workspace_id AND state IN ('queued','claimed')),reads=(SELECT count(*) FROM requests r WHERE r.workspace_id=u.workspace_id AND operation=ANY($1::text[])),mutations=(SELECT count(*) FROM requests r WHERE r.workspace_id=u.workspace_id AND NOT(operation=ANY($1::text[]))),events=(SELECT count(*) FROM events e WHERE e.workspace_id=u.workspace_id),push_jobs=(SELECT count(*) FROM push_jobs j JOIN devices d ON d.id=j.device_id WHERE d.workspace_id=u.workspace_id)",
                reads,
            )
            usage = await conn.fetch(
                "SELECT workspace_id,payload_bytes FROM workspace_usage"
            )
            for row in usage:
                await conn.execute(
                    "UPDATE payload_shards SET payload_bytes=payload_bytes+$2 WHERE id=$1",
                    store.shard(row["workspace_id"]),
                    row["payload_bytes"],
                )
            if await conn.fetchval(
                "SELECT EXISTS(SELECT 1 FROM payload_shards WHERE payload_bytes>$1)",
                store.shard_max_bytes,
            ):
                raise ValueError(
                    "source payloads exceed destination shard budget; stop destination workers and retry this empty import with a larger allocation"
                )
            await conn.execute(
                "UPDATE global_usage SET events=(SELECT count(*) FROM events),push_jobs=(SELECT count(*) FROM push_jobs) WHERE id=1"
            )
            for table in ("events", "push_jobs"):
                high_water = await asyncio.to_thread(
                    lambda: source.execute(
                        "SELECT seq FROM sqlite_sequence WHERE name=?", (table,)
                    ).fetchone()
                )
                surviving = await conn.fetchval(
                    f"SELECT COALESCE(max(id),0) FROM {table}"
                )
                high = max(high_water[0] if high_water else 0, surviving)
                await conn.execute(
                    f"SELECT setval(pg_get_serial_sequence('{table}','id'),$1,$2)",
                    max(high, 1),
                    high > 0,
                )
            for table in TABLES:
                if (
                    await conn.fetchval(f"SELECT count(*) FROM {table}")
                    != counts[table]
                ):
                    raise ValueError("migration verification count mismatch")
        overages = [
            {"workspace_id": r["workspace_id"], "payload_bytes": r["payload_bytes"]}
            for r in usage
            if r["payload_bytes"] > MAX_QUEUE_BYTES + RESULT_RESERVE
        ]
        return {**counts, "digests": digests, "workspace_budget_overages": overages}
    finally:
        await asyncio.to_thread(source.close)
        if store:
            await store.close()
