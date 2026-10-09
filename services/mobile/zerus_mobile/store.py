"""SQLite state. No network call or await happens inside a transaction."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import secrets
import sqlite3
import stat
import time
import uuid

from .attachments import MAX_QUEUE_BYTES
from .context import CAPABILITY_OPERATIONS, READ_OPERATIONS, READ_SQL, inspect_features, supports
from .projects import is_archived


def token() -> str:
    return secrets.token_urlsafe(32)


def digest(value: str) -> str:
    return hashlib.sha256(value.encode()).hexdigest()


def canonical(value) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)


class Store:
    def __init__(self, path: str | Path):
        path = Path(path)
        path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        # Do not chmod a caller's home, cwd or shared directory.
        parent_info = path.parent.stat()
        if parent_info.st_uid != os.geteuid() or stat.S_IMODE(parent_info.st_mode) & 0o077:
            raise ValueError("database requires a dedicated private directory (mode 0700)")
        fd = os.open(path, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
        try:
            info = os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid():
                raise ValueError("database must be a regular file owned by this user")
            os.fchmod(fd, 0o600)
        finally:
            os.close(fd)
        self.db = sqlite3.connect(path, timeout=5)
        self.db.row_factory = sqlite3.Row
        self.db.execute("PRAGMA foreign_keys=ON")
        self.db.execute("PRAGMA journal_mode=DELETE")
        self.db.execute("PRAGMA synchronous=FULL")
        self.db.execute("PRAGMA secure_delete=ON")
        self.db.executescript("""
        CREATE TABLE IF NOT EXISTS workspaces(id TEXT PRIMARY KEY,name TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS nodes(id TEXT PRIMARY KEY,workspace_id TEXT NOT NULL REFERENCES workspaces(id),name TEXT NOT NULL,token_hash TEXT UNIQUE NOT NULL,revoked INTEGER NOT NULL DEFAULT 0,last_seen REAL,snapshot TEXT);
        CREATE TABLE IF NOT EXISTS devices(id TEXT PRIMARY KEY,workspace_id TEXT NOT NULL REFERENCES workspaces(id),name TEXT NOT NULL,token_hash TEXT UNIQUE NOT NULL,revoked INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE IF NOT EXISTS invitations(code_hash TEXT PRIMARY KEY,workspace_id TEXT NOT NULL REFERENCES workspaces(id),expires REAL NOT NULL);
        CREATE TABLE IF NOT EXISTS requests(id TEXT PRIMARY KEY,workspace_id TEXT NOT NULL,device_id TEXT NOT NULL REFERENCES devices(id),node_id TEXT NOT NULL REFERENCES nodes(id),operation TEXT NOT NULL,body_hash TEXT NOT NULL,body_bytes INTEGER NOT NULL,body TEXT NOT NULL,state TEXT NOT NULL,result TEXT,error TEXT,created REAL NOT NULL,claimed REAL,updated REAL NOT NULL);
        CREATE INDEX IF NOT EXISTS request_queue ON requests(node_id,state,created);
        CREATE INDEX IF NOT EXISTS request_expiry ON requests(state,created,claimed);
        CREATE INDEX IF NOT EXISTS request_history ON requests(updated) WHERE body!='' AND state IN ('completed','failed','uncertain');
        CREATE TABLE IF NOT EXISTS events(id INTEGER PRIMARY KEY AUTOINCREMENT,workspace_id TEXT NOT NULL,node_id TEXT NOT NULL,session TEXT NOT NULL,kind TEXT NOT NULL,created REAL NOT NULL);
        CREATE INDEX IF NOT EXISTS event_workspace ON events(workspace_id,id);
        CREATE TABLE IF NOT EXISTS pushes(device_id TEXT PRIMARY KEY REFERENCES devices(id),provider TEXT NOT NULL,target TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS push_jobs(id INTEGER PRIMARY KEY AUTOINCREMENT,device_id TEXT NOT NULL REFERENCES devices(id),event_id INTEGER NOT NULL,payload TEXT NOT NULL,attempts INTEGER NOT NULL DEFAULT 0,next_at REAL NOT NULL,created REAL NOT NULL);
        """)
        if "operation" not in {r[1] for r in self.db.execute("PRAGMA table_info(requests)")}:
            with self.db:
                self.db.execute("ALTER TABLE requests ADD COLUMN operation TEXT NOT NULL DEFAULT 'unknown'")
                self.db.execute("UPDATE requests SET operation=json_extract(body,'$.operation') WHERE body!=''")
        if "body_hash" not in {r[1] for r in self.db.execute("PRAGMA table_info(requests)")}:
            with self.db:
                self.db.execute("ALTER TABLE requests ADD COLUMN body_hash TEXT NOT NULL DEFAULT ''")
                for row in self.db.execute("SELECT id,body FROM requests WHERE body!=''").fetchall():
                    self.db.execute("UPDATE requests SET body_hash=? WHERE id=?", (digest(row["body"]), row["id"]))
        if "body_bytes" not in {r[1] for r in self.db.execute("PRAGMA table_info(requests)")}:
            with self.db:
                self.db.execute("ALTER TABLE requests ADD COLUMN body_bytes INTEGER NOT NULL DEFAULT 0")
                self.db.execute("UPDATE requests SET body_bytes=length(CAST(body AS BLOB))")
        columns={r[1] for r in self.db.execute("PRAGMA table_info(requests)")}
        with self.db:
            for column in ("expires_at","result_read"):
                if column not in columns:
                    self.db.execute(f"ALTER TABLE requests ADD COLUMN {column} REAL")
        self.db.execute("CREATE INDEX IF NOT EXISTS terminal_history_v2 ON requests(updated) WHERE operation='terminal_snapshot' AND state IN ('completed','failed','uncertain')")
        self.db.execute("CREATE INDEX IF NOT EXISTS terminal_expiry ON requests(expires_at) WHERE state='queued' AND expires_at IS NOT NULL")
        self.db.execute("CREATE INDEX IF NOT EXISTS request_workspace ON requests(workspace_id,operation,state)")

    def close(self):
        self.db.close()

    def workspace(self, name: str) -> str:
        value = str(uuid.uuid4())
        with self.db:
            self.db.execute("INSERT INTO workspaces VALUES(?,?)", (value, name))
        return value

    def node(self, workspace: str, name: str) -> dict:
        value, secret = str(uuid.uuid4()), token()
        with self.db:
            self.db.execute("INSERT INTO nodes(id,workspace_id,name,token_hash) VALUES(?,?,?,?)", (value, workspace, name, digest(secret)))
        return {"node_id": value, "node_token": secret}

    def invite(self, workspace: str) -> dict:
        code, expires = token(), time.time() + 600
        with self.db:
            self.db.execute("DELETE FROM invitations WHERE expires<=?", (time.time(),))
            self.db.execute("INSERT INTO invitations VALUES(?,?,?)", (digest(code), workspace, expires))
        return {"pair_code": code, "expires_at": expires}

    def pair(self, code: str, name: str) -> dict | None:
        with self.db:
            self.db.execute("BEGIN IMMEDIATE")
            row = self.db.execute("SELECT invitations.*,workspaces.name AS workspace_name FROM invitations JOIN workspaces ON workspaces.id=invitations.workspace_id WHERE code_hash=? AND expires>?", (digest(code), time.time())).fetchone()
            if not row:
                return None
            value, secret = str(uuid.uuid4()), token()
            self.db.execute("DELETE FROM invitations WHERE code_hash=?", (digest(code),))
            self.db.execute("INSERT INTO devices(id,workspace_id,name,token_hash) VALUES(?,?,?,?)", (value, row["workspace_id"], name, digest(secret)))
        return {"device_id": value, "device_token": secret, "workspace_id": row["workspace_id"], "workspace_name": row["workspace_name"]}

    def authenticate(self, secret: str, role: str):
        assert role in ("nodes", "devices")
        return self.db.execute(f"SELECT * FROM {role} WHERE token_hash=? AND revoked=0", (digest(secret),)).fetchone()

    def revoke(self, role: str, value: str) -> bool:
        assert role in ("nodes", "devices")
        now = time.time()
        with self.db:
            found = self.db.execute(f"UPDATE {role} SET revoked=1 WHERE id=?", (value,)).rowcount
            field = "node_id" if role == "nodes" else "device_id"
            self.db.execute(f"UPDATE requests SET state='failed',error='credential revoked before delivery',updated=? WHERE {field}=? AND state='queued'", (now, value))
            self.db.execute(f"UPDATE requests SET state='uncertain',error='credential revoked after claim',updated=? WHERE {field}=? AND state='claimed'", (now, value))
            if role == "devices":
                self.db.execute("DELETE FROM pushes WHERE device_id=?", (value,))
                self.db.execute("DELETE FROM push_jobs WHERE device_id=?", (value,))
        return bool(found)

    @staticmethod
    def envelope(row) -> dict:
        return {"request_id": row["id"], "state": row["state"], "result": json.loads(row["result"]) if row["result"] is not None else None, "error": row["error"]}

    def maintain(self, *, restart=False, queue_ttl=120, claim_ttl=90, retention=7 * 86400):
        now = time.time()
        with self.db:
            self.db.execute("UPDATE requests SET state='failed',error='terminal input expired before delivery',updated=? WHERE state='queued' AND expires_at<=?", (now,now))
            self.db.execute("DELETE FROM requests WHERE operation='terminal_snapshot' AND state IN ('completed','failed','uncertain') AND updated<?", (now-120,))
            self.db.execute("UPDATE requests SET state='failed',error='request expired before delivery',updated=? WHERE state='queued' AND created<=?", (now, now - queue_ttl))
            if restart:
                self.db.execute("UPDATE requests SET state='uncertain',error='relay restarted after claim',updated=? WHERE state='claimed'", (now,))
            else:
                self.db.execute("UPDATE requests SET state='uncertain',error='node result timeout after claim',updated=? WHERE state='claimed' AND claimed<=?", (now, now - claim_ttl))
            # Read-only inspections can expire; mutations keep indefinite UUID
            # tombstones so an old command can never execute a second time.
            self.db.execute(f"DELETE FROM requests WHERE operation IN {READ_SQL} AND operation!='terminal_snapshot' AND state IN ('completed','failed','uncertain') AND updated<?", (now - min(retention, 3600),))
            self.db.execute("UPDATE requests SET body='',body_bytes=0,result=NULL,error='request history expired; delivery must not be retried' WHERE state IN ('completed','failed','uncertain') AND updated<? AND body!=''", (now - retention,))
            self.db.execute("DELETE FROM events WHERE created<?", (now - retention,))
            self.db.execute("DELETE FROM push_jobs WHERE created<? OR attempts>=5", (now - 86400,))
            self.db.execute("DELETE FROM invitations WHERE expires<=?", (now,))

    def submit(self, device, body: dict, max_queue: int, max_bytes: int = MAX_QUEUE_BYTES):
        now, encoded = time.time(), canonical(body)
        with self.db:
            self.db.execute("BEGIN IMMEDIATE")
            existing = self.db.execute("SELECT * FROM requests WHERE id=?", (body["request_id"],)).fetchone()
            if existing:
                if existing["device_id"] != device["id"] or existing["body_hash"] != digest(encoded):
                    return 409, {"error": "request_id already used with different content"}
                return 202, self.envelope(existing)
            node = self.db.execute("SELECT id,snapshot,last_seen FROM nodes WHERE id=? AND workspace_id=? AND revoked=0", (body["computer_id"], device["workspace_id"])).fetchone()
            if not node:
                return 404, {"error": "computer not found"}
            snapshot = json.loads(node["snapshot"]) if node["snapshot"] else None
            if body["operation"] in CAPABILITY_OPERATIONS and not supports(snapshot, body["operation"]):
                return 409, {"error": "computer does not advertise this operation; update its connector"}
            if body["operation"] == "inspect" and any(not supports(snapshot, feature, "features") for feature in inspect_features(body["payload"])):
                return 409, {"error": "computer does not advertise this inspection feature; update its connector"}
            if body["operation"] == "send" and "agent_id" in body["payload"] and not supports(snapshot, "send_agent", "features"):
                return 409, {"error": "computer does not advertise child message transport; update its connector"}
            if body["operation"] == "terminal_input" and (node["last_seen"] is None or now-node["last_seen"]>45):
                return 409, {"error": "computer is offline; terminal input was not queued"}
            count = self.db.execute("SELECT count(*) FROM requests WHERE workspace_id=? AND state IN ('queued','claimed')", (device["workspace_id"],)).fetchone()[0]
            if count >= max_queue:
                return 429, {"error": "workspace queue is full"}
            reads = body["operation"] in READ_OPERATIONS
            total = self.db.execute(f"SELECT count(*) FROM requests WHERE workspace_id=? AND (operation IN {READ_SQL})=?", (device["workspace_id"], int(reads))).fetchone()[0]
            if total >= (5000 if reads else 100000):
                return 429, {"error": "workspace retained request limit reached"}
            byte_count = len(encoded.encode())
            retained = self.db.execute("SELECT COALESCE(sum(body_bytes),0) FROM requests WHERE workspace_id=?", (device["workspace_id"],)).fetchone()[0]
            # Preserve a small control/text allowance when attachment traffic
            # occupies the main body budget. No uncertain request is evicted.
            small = byte_count <= 64 * 1024 and not body.get("payload", {}).get("attachments")
            if retained + byte_count > max_bytes + (1024 * 1024 if small else 0):
                return 429, {"error": "workspace retained payload budget is full"}
            self.db.execute("INSERT INTO requests(id,workspace_id,device_id,node_id,operation,body_hash,body_bytes,body,state,created,updated) VALUES(?,?,?,?,?,?,?,?,'queued',?,?)", (body["request_id"], device["workspace_id"], device["id"], body["computer_id"], body["operation"], digest(encoded), byte_count, encoded, now, now))
            if body["operation"] == "terminal_input":
                self.db.execute("UPDATE requests SET expires_at=? WHERE id=?", (now+5,body["request_id"]))
            row = self.db.execute("SELECT * FROM requests WHERE id=?", (body["request_id"],)).fetchone()
        return 202, self.envelope(row)

    def claim(self, node) -> list:
        now = time.time()
        with self.db:
            self.db.execute("BEGIN IMMEDIATE")
            live = self.db.execute("SELECT 1 FROM nodes WHERE id=? AND revoked=0", (node["id"],)).fetchone()
            if not live:
                return []
            row = self.db.execute("SELECT * FROM requests WHERE node_id=? AND state='queued' AND (expires_at IS NULL OR expires_at>?) ORDER BY created LIMIT 1", (node["id"],now)).fetchone()
            if not row:
                return []
            self.db.execute("UPDATE requests SET state='claimed',claimed=?,updated=? WHERE id=? AND state='queued'", (now, now, row["id"]))
            body = json.loads(row["body"])
        command={key: body[key] for key in ("request_id", "operation", "session", "payload")}
        if row["expires_at"] is not None:
            command["expires_at"]=row["expires_at"]
        return [command]

    def result(self, node, request_id: str, body: dict) -> int:
        with self.db:
            row = self.db.execute("SELECT * FROM requests WHERE id=? AND node_id=?", (request_id, node["id"])).fetchone()
            if not row:
                return 404
            encoded = canonical(body["result"]) if body["result"] is not None else None
            if row["state"] != "claimed":
                # A late success cannot silently resolve an uncertain delivery.
                return 200 if (row["state"], row["result"], row["error"]) == (body["state"], encoded, body["error"]) else 409
            self.db.execute("UPDATE requests SET state=?,result=?,error=?,updated=? WHERE id=?", (body["state"], encoded, body["error"], time.time(), request_id))
        return 200

    def event(self, node, session: str, kind: str):
        now = time.time()
        cur = self.db.execute("INSERT INTO events(workspace_id,node_id,session,kind,created) VALUES(?,?,?,?,?)", (node["workspace_id"], node["id"], session, kind, now))
        payload = canonical({"event_id": cur.lastrowid, "kind": "wake"})
        self.db.execute("INSERT INTO push_jobs(device_id,event_id,payload,next_at,created) SELECT p.device_id,?,?,?,? FROM pushes p JOIN devices d ON d.id=p.device_id WHERE d.workspace_id=? AND d.revoked=0 AND (SELECT count(*) FROM push_jobs)<10000", (cur.lastrowid, payload, now, now, node["workspace_id"]))

    def heartbeat(self, node, snapshot: dict):
        now = time.time()
        with self.db:
            previous = self.db.execute("SELECT snapshot FROM nodes WHERE id=?", (node["id"],)).fetchone()[0]
            if previous is not None:
                old = {s["name"]: s for s in json.loads(previous).get("sessions", []) if isinstance(s, dict) and isinstance(s.get("name"), str) and not is_archived(s)}
                for session in snapshot.get("sessions", []):
                    if not isinstance(session, dict) or not isinstance(session.get("name"), str) or is_archived(session):
                        continue
                    before = old.get(session["name"])
                    if before is None:
                        before = {}
                    identity = lambda s: (s.get("run_id"), s.get("conversation_id"))
                    question = lambda s: canonical([s.get("attention_id"), s.get("question_request", s.get("question_requests", None))])
                    phase = session.get("phase")
                    pending = lambda s: {canonical(q) for q in s.get("mobile_attention", []) if isinstance(q, dict)} if isinstance(s.get("mobile_attention", []), list) else set()
                    new_async_question = bool(pending(session) - pending(before)) or (bool(pending(session)) and identity(before) != identity(session))
                    if new_async_question or (phase in ("approval", "input") and (before.get("phase") != phase or identity(before) != identity(session) or question(before) != question(session))):
                        self.event(node, session["name"], "attention")
                    elif phase == "error" and (before.get("phase") != "error" or identity(before) != identity(session) or question(before) != question(session)):
                        self.event(node, session["name"], "error")
                    elif session.get("activity") == "idle" and before.get("activity") == "busy" and identity(before) == identity(session):
                        self.event(node, session["name"], "completed")
            self.db.execute("UPDATE nodes SET last_seen=?,snapshot=? WHERE id=?", (now, canonical(snapshot), node["id"]))
            # Bound event and push storage even for a noisy computer.
            self.db.execute("DELETE FROM events WHERE id NOT IN (SELECT id FROM events ORDER BY id DESC LIMIT 100000)")
            self.db.execute("DELETE FROM push_jobs WHERE id NOT IN (SELECT id FROM push_jobs ORDER BY id DESC LIMIT 10000)")
