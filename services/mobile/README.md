# Zerus mobile relay

A self-hosted Python 3.11+ relay connects the Android client to computers running
`hgs`. Computers make outbound HTTPS connections; phones never receive SSH keys.
The relay is trusted with snapshots, requests and conversation results. TLS protects
transport. Version 1 does **not** provide end-to-end encryption.

See [the architecture and wire contract](../../docs/mobile-architecture.md) and
[deployment instructions](../../docs/mobile-deployment.md). The relay exposes no
signup, workspace creation or administrator HTTP endpoint. Provisioning is a local
CLI operation. A phone can revoke its own credential with `DELETE /v1/device`.

## Local installation

```sh
cd services/mobile
python3 -m venv .venv
.venv/bin/pip install .
# Install .[fcm] instead if this deployment needs Firebase Cloud Messaging.
.venv/bin/zerus-mobile --database ./data/relay.sqlite3 provision \
  --name 'Example workspace' --computer-name 'Example computer'
.venv/bin/zerus-mobile --database ./data/relay.sqlite3 serve
```

Provisioning deliberately prints the node token and a single-use phone pairing
code. Transfer them privately; never paste them into logs or repository files.
The invitation expires after ten minutes. The state directory must be private
(mode 0700); the relay creates its database with mode 0600 and refuses database
symlinks. It never changes permissions on an existing parent directory.

Add another computer, invite another phone, or revoke a credential locally:

```sh
zerus-mobile --database ./data/relay.sqlite3 node \
  --workspace WORKSPACE_UUID --name 'Another computer'
zerus-mobile --database ./data/relay.sqlite3 invite --workspace WORKSPACE_UUID
zerus-mobile --database ./data/relay.sqlite3 revoke-device --id DEVICE_UUID
zerus-mobile --database ./data/relay.sqlite3 revoke-node --id NODE_UUID
```

The default listener is `127.0.0.1:8787`. Place it behind a TLS proxy before using
it on another machine. Keep access/body/header logging disabled at the proxy.
Only one relay process should serve a database; restarting marks existing claimed
requests uncertain. Back up the entire private state directory while stopped or
use SQLite's online backup API. A backup contains session data and push endpoints.

## Docker and optional Caddy TLS

```sh
cd services/mobile
docker compose build
docker compose run --rm relay provision \
  --name 'Example workspace' --computer-name 'Example computer'
docker compose up -d relay
```

Compose publishes the relay only on loopback. For Caddy TLS, edit `Caddyfile` to
replace the reserved `relay.example.com` domain, point that domain at the server,
and run `docker compose --profile tls up -d`. Caddy needs ports 80 and 443; do not
publish port 8787 on a public interface. If using an existing TLS proxy, route it
to the loopback port instead. Health: `GET /healthz` reports protocol version 1.

## Push configuration

Push is disabled by default. `GET /v1/capabilities` requires phone authentication
and lists the configured `push_providers`. Create a private JSON file with mode
0600 and pass `serve --config /private/path/config.json`:

```json
{
  "push_hosts": ["push.example.com"],
  "fcm_credentials": "/private/path/firebase-service-account.json"
}
```

Omit either setting when unused. The example domain is reserved; replace it with
an explicitly trusted public UnifiedPush distributor hostname. Wildcards are not
allowed. UnifiedPush requires HTTPS on port 443, no user information/fragments,
public-only DNS answers and no redirects. Every delivery resolves again, rejects
private targets and pins the validated addresses during the TLS request. Proxy
environment variables are ignored. Allowlisting a hostname does not permit private
addresses. The default allowlist is empty.

FCM uses the optional pinned `firebase-admin` package and its HTTP v1 transport.
Keep the Google service account JSON outside the repository, readable only by the
relay user (mode 0600), and restrict its Firebase permissions to sending messages.
For Docker, mount private configuration and credentials at explicit paths and use
a Compose override to add `serve --host 0.0.0.0 --config /private/config.json`.
Credential paths and mounts belong in private deployment configuration.

Push payloads contain only `event_id` and `kind: "wake"`; they contain no session
names, messages or questions. The phone fetches details through its authenticated
API. Delivery runs outside heartbeat handling, retries at most five attempts and
expires after 24 hours. Unregistered provider targets are removed. Revoking a
phone deletes its push registration and pending delivery jobs.

## Delivery and resource limits

Requests require a UUID and one of `inspect`, `send`, `answer`, `interrupt`,
`compact_context` or `clear_context`.
Mutation payloads carry the same UUID plus exact run/conversation expectations.
An identical repeated mutation gets the existing envelope. Changed content gets
409. The relay commits a durable claim before delivery and never requeues a
claimed command. An expired queue entry fails; a stale claim becomes uncertain.
Uncertain input needs explicit user review and a refreshed native identity.

Defaults: 1 MiB HTTP body for ordinary endpoints, 29 MiB for native send request
envelopes, 10-second ordinary body timeout, 60-second send body timeout,
25-second long poll, 40-second ordinary handler deadline and 75 seconds for
request submission, 120-second queued expiry, 90-second claim timeout, and
200 pending requests per workspace. Retained request bodies across all states
have a 100 MiB workspace budget plus a reserved 1 MiB allowance for small text
and control requests. Identical UUID retries still return their receipt when
the budget is full; uncertain requests are never evicted to make room.
Payload/result/event retention is seven days. Completed
read-only inspections expire after one hour; repeating an expired inspection is
safe. Mutation UUID tombstones remain indefinitely, with a 100,000-mutation
workspace ceiling. Retained inspections have a separate 5,000-entry ceiling.
These limits never evict a mutation tombstone to permit accidental redelivery.

The computer connector uses the same retention policy for delivered results:
inspections expire after one hour; mutation responses become an explicit
`history_expired` receipt after seven days. Pending result uploads are retained.
The connector keeps mutation UUIDs and canonical request hashes indefinitely,
so changing a replayed request cannot execute a new command. Legacy journal
records receive the migration time as their age; an unknown legacy request hash
never implies that a newly received command matches it.

Invitations are single-use, use 256 bits of randomness and expire after ten
minutes. Node/device tokens use the same entropy and only SHA-256 token hashes
are stored. Push registrations and session data remain readable in the trusted
relay database. Pairing and failed authentication are rate limited. Events and
push jobs have additional hard global ceilings of 100,000 and 10,000 rows.

Events come from snapshot transitions. The first snapshot is silent. A new
attention identity, a new async question fingerprint, busy-to-idle completion or
a new error emits a generic event. Async question fingerprints come from the
connector's bounded round-robin inspection, so notifications can lag by a scan
cycle when many sessions are running.


## Native projects and attachments

The legacy `snapshot.projects` object contains CLI directory aliases and does not
represent desktop logical groups. The connector reads `hgs swarm get` every
15 seconds with a three-second timeout and attaches `snapshot.mobile_projects`.
It includes all native group headers (`id`, `name`, `color`, `accessible`) plus
this computer's folders, exact session names and archive IDs. It excludes peer
configuration, catalog conflicts and history. Each session receives
`mobile_project_id` from its exact native assignment or the verified default.
Custom worktree names never determine the logical project. Shared project keys
include `swarm_id`; an uninitialized/unavailable catalog has an explicit stable
local fallback identity and no invented project list. Transient failures retain
the previous catalog with `stale: true`.

Phone attachments use the existing native send payload:

```json
{
  "attachments": [{
    "name": "example.txt",
    "mime": "text/plain",
    "data_base64": "ZXhhbXBsZQ==",
    "reference": "[File #1]"
  }]
}
```

This is part of the identity-checked `send` payload, alongside its request UUID,
text and expected run/conversation IDs. Limits match native input: eight files,
10 MiB per file, 20 MiB total decoded bytes and 64 KiB UTF-8 text. The relay and
connector validate independently. Names must be plain filenames; filesystem
paths, URLs and unexpected fields are rejected. References are optional native
`[Image #N]` or `[File #N]` labels and must be unique. Base64 is standard and
canonical. The phone supplies bytes, never a path on the computer.

The native `hgs send` implementation creates immutable private copies using its
existing attachment storage and durable input receipts. The connector passes the
validated native ABI without introducing a shell or path-based upload command.
Its permanent request fingerprint includes the attachment bytes, so uncertain
or duplicate sends cannot repeat delivery. Request polling claims at most one
command per batch and permits bounded 29 MiB node responses; native inspection
and result limits remain unchanged. At most two large uploads are read
concurrently; small authenticated requests retain separate capacity.

`GET /v1/capabilities` exposes these limits in `attachment_limits`. A TLS proxy
must allow 29 MiB only on `POST /v1/requests` with at least 75-second upstream
read/write deadlines; keep the 1 MiB limit on other endpoints. The supplied
Caddy configuration applies this route distinction.

## Archived conversations

Current, paused and stopped conversations keep the ordinary `inspect` payload
`{}`. Archived browsing supplies `{"archive_id":"CANONICAL_UUID"}` instead.
The connector requires the exact session name and immutable archive UUID in its
native snapshot, no older than 45 seconds, and invokes only
`hgs inspect SESSION --archive UUID`. The returned name and archive ID must
match. An ordinary inspection cannot silently return an archive if a live name
disappears during the request. Any nonblank archive ID identifies an archive,
even if a compatibility snapshot omits its `state` field.

Archives are read-only through mobile. All mutations reject archive IDs;
archived-only names cannot fall back to a current session. Project membership
uses the archive UUID, so a saved conversation and its same-name live session
can remain in different logical projects. Archive rows do not affect live
notification transitions.

For synthetic five-filter QA, set `ZERUS_MOBILE_FIXTURE_CATALOG_MODE=filters`
when running the test connector, or put `{"all_states":true,"send_delay":8}`
in `fixture-config.json` inside `ZERUS_MOBILE_FIXTURE_DIR`. The next fixture CLI
call picks it up without restarting any agent. It exposes seven rows: current,
working, attention, paused, stopped and two distinct archives sharing the current
session name. The General group is empty, current/saved rows share the work
project and both archives belong to a separate history project. Native-shaped
archive inspections return distinct synthetic conversation IDs and reply text.
The same private configuration accepts `inspect_delay` in seconds (maximum 30)
and `history_count` (maximum 500 journal/provider rows) for cache and rendering
checks. These options alter only inspection output; generated messages have
stable native IDs and do not modify the saved fixture session.

## Context usage and controls

Inspect results preserve native `session_usage.context` (`used`, `limit` and
optional estimate metadata), `session_usage.prompt_cache`, `cache_hint`,
`compact_context_request` and the two native context support flags. Missing
measurements remain unknown. A native saving suggestion is distinct from a
reported cold cache; provider cache deadlines are estimates supplied by native
usage, separate from the phone's encrypted history cache.

The authenticated relay capabilities include `operations`. A current connector
adds `snapshot.mobile_capabilities` with `protocol_version: 1` and its supported
transport operations. Both declarations are required before enabling new context
controls; an older node with no declaration receives HTTP 409 for these operations.
Ordinary version 1 sends retain their behavior. The connector additionally reads a
fresh inspect result, checks exact name/run/conversation identity and requires the
corresponding native support flag before attempting a control. An older native
binary or unsupported agent therefore cannot trigger a terminal command.

`compact_context` and `clear_context` accept only this payload:

```json
{
  "request_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  "expected_run_id": "example-run",
  "expected_conversation_id": "example-conversation"
}
```

The UUID must match the outer request; both native IDs must be nonempty. The only
commands are fixed `hgs compact-context SESSION --json` and
`hgs clear-context SESSION --json`, using JSON stdin. Archives, extra fields and
arbitrary provider commands are denied. Native acknowledgements must match the
request UUID, session name and original run/conversation. Compaction acknowledges
`submitted`; clear acknowledges `submitted` or `confirmed`. A completed relay
envelope means that this acknowledgement arrived, not that compaction finished.
Once a mutation is invoked, an error or unverified acknowledgement is uncertain
and its UUID cannot execute again.

Compact-before-send uses a separate send UUID and optional
`expected_compaction_id`. Before delivery the connector verifies a fresh native
`compact_context_request` with that exact UUID, current run/conversation,
`status: completed`, and idle activity/phase. Native input performs its own final
check. The relay never automatically chains a send. The client must preserve the
draft and stop a continuation if the draft, target or navigation changes, or
compaction fails, is cancelled, is unchanged or is uncertain. A confirmed clear
acknowledges the original identity; the client then refreshes the same run to find
the new conversation. A submitted clear is ambiguous: native inspect does not
export a request-correlated clear completion, so it must not authorize automatic
draft migration.

Synthetic context QA can use `fixture-config.json`:

```json
{
  "all_states": true,
  "compact_delay": 8,
  "compact_status": "completed",
  "clear_status": "confirmed",
  "session_usage": {
    "status": "ok", "source": "claude", "scope": "conversation",
    "context": {"used": 60000, "limit": 200000},
    "prompt_cache": {"status": "unknown", "source": "native_usage"}
  },
  "cache_hint": {"status": "cold", "source": "native_footer", "tokens": 60000}
}
```

Telemetry configuration applies to a fresh synthetic conversation. Compaction
progresses from submitted to compacting, then after `compact_delay` seconds
(default 2, maximum 60) to `compact_status`: `completed`, `failed`, `cancelled`,
`unchanged` or `uncertain`. Terminal completion shrinks reported context and warms
the synthetic cache. `clear_status` is `confirmed` by default, generating a new
conversation ID and empty history, or `submitted`, leaving the identity unchanged.
`context_supported: false` disables both controls. For uncertain receipt handling,
`context_failure_after_submit: true` records one synthetic mutation and exits
without acknowledging it. These controls never invoke real hgs or tmux.

## Local validation

```sh
PYTHONPATH=services/mobile .deps/mobile-venv/bin/python \
  -m unittest discover -s services/mobile/tests -v
```

Run from the repository root with an environment containing the pinned package.
Tests use temporary databases, synthetic sessions and loopback HTTP. No live
agent, tmux server or provider credentials are required.

Scoped lifecycle and Terminal operations require native help advertisement.
Terminal input uses an exact pane/process binding and a relay-issued five-second
deadline; its `submitted` receipt confirms bytes delivered, not command completion.
The connector never replays a claimed input UUID. Literal text is staged through
private tmux stdin buffers, and multiline input requires verified bracketed paste.
Terminal screen reads retain at most 128 KiB of screen text with explicit truncation;
finished relay reads expire after 120 seconds even if the phone abandoned its poll.
Connector read receipts expire 120 seconds after relay delivery; undelivered results
remain in the durable outbox. Input mutation tombstones keep the normal no-replay
retention. Terminal never resizes the desktop pane, and DeepSeek/archived/child
conversations report that raw Terminal is unavailable.

Synthetic fixture Terminal is available through the same fixed ABI and never
executes the supplied text. The fixture help advertises Terminal alongside scoped
lifecycle; `fixture_terminal_text` records only the synthetic screen content.

Terminal screen polling is capped at four frames per second, with slower idle
polling. Authenticated credential and source-IP budgets are 2,400 requests per
minute, and the relay-wide budget remains 6,000. Pairing and failed-login limits
retain their stricter independent budgets. These transport limits include receipt
polling and node result uploads; they do not permit replaying delayed input.

### Public history and session recovery

`history` is a scoped read operation with the original request UUID, expected run
and conversation, optional explicit archive/child identity, and a page limit up
to 100. Choose one opaque `before`, `after` or `around` cursor, or
`around_incoming_seq` together with the exact complete `history_epoch`.
Public journal/provider/attachment rows preserve their native identities and
canonical order. Equal timestamps remain distinct. History epochs survive a
confirmed same-conversation resume; source replacement, historical backfill or
an unavoidable canonical sequence change explicitly resets the epoch. Unknown
incoming totals remain null while indexing or when a source is partial.

The native derived SQLite index contains public projections only, uses private
files, and bounds each call to two MiB of provider input plus bounded journal,
receipt and reconciliation work. It caps each index at 100,000 rows/512 MiB;
exhaustion is disclosed as a source gap. Individual message text is bounded to
32 KiB with `detail_truncated`; whole pages fit the ordinary one-MiB transport
limit and their cursors advance only across returned rows. A valid final
unterminated source row is visible with partial status. An older resident DSH
adapter that cannot page its retained history reports partial availability,
unknown totals and `indexing:false`; no host reload is performed.

For a newest-head request the connector advances at most 32 bounded native reads
within two seconds, stopping at completion, permanent partial status or no
progress. Cursor reads execute once. History results use the existing one-hour
read retention and do not create permanent mutation tombstones.

`recovery_action` accepts only `job_id` and `action:now|cancel` in addition to
exact request/run/conversation identities. The native writer lock verifies the
current waiting job and its full five-field identity before changing it. Retry
now preserves `not_before`; `scheduled` means the job was rescheduled, not that
input was delivered. Durable UUID receipts prevent replay. Global recovery
policies, archived targets and child recovery controls are excluded.

Synthetic paging QA uses `history_count` up to 5,000 for history pages while
ordinary inspect remains capped at 500 fixture rows. Set `history_indexing:true`
for a stable incomplete head, `history_gap:true` for permanent partial source
status, or `history_epoch` to exercise epoch replacement. Configure `recovery`
as a native waiting job with `id`, `state`, `due_at`, `not_before`, `attempt`, and
`identity:[run_id,conversation_id,model,account_id,host_generation]`.

### Managed relay transport migration

Only the exact official HTTPS root origin `https://zerus.dev.guthub.dev`
(empty or `/` path and default/443 port, no credentials/query/fragment) transports
requests through `https://relay.zerus.dev`. Custom servers remain unchanged.
The connector hashes its original validated URL and node token for its private
receipt journal and fallback project identity. Neither is rekeyed by transport
migration, and claimed commands are never replayed.

To change an existing managed connector's stored `server_url` to the canonical
URL, explicitly retain its previous validated URL in `identity_url`. This override
is accepted only for the exact old-official-to-new-official pair (or an unchanged
identical URL). Preserve the node token and `state_dir`; retain the precise original
URL spelling if it included a default port. An arbitrary identity override is
rejected before opening the journal. A genuinely different relay requires a new
credential and separate private state directory, preserving the old receipts.
Saving configuration alone does not restart the connector or any native agents.
