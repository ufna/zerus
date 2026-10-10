# Relay Presence Alerts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Rust relay holds each derived attention, error and completion event as an alert candidate, delivers it as an additive `alert` event plus one coalesced FCM wake only when the owner's preference, desktop presence and read state allow it, and Android creates notifications only from those `alert` events when the relay advertises `presence_alerts`.

**Architecture:** The connector sends each machine's desktop presence beside its snapshot, so presence changes never rewrite or rehash snapshots. A pure rule module (`src/alerts.rs`) converts remembered presence, the gateway preference and per-session read state into decisions, taking the clock as an argument. The store records candidates in the event-publishing transaction, re-evaluates a workspace after every changed snapshot or new presence and in an elected maintenance pass, and treats "delete the candidate row in the transaction that appends the `alert` event" as the cross-worker single-delivery claim. Android keeps its card machinery and only gates *new* cards on a matching `alert` row.

**Tech Stack:** Rust 1.85 (Tokio, SQLx Any over SQLite and PostgreSQL), Python 3.11 connector and compatibility fixtures (aiohttp, unittest), Kotlin Android app (JUnit unit tests).

**Spec:** `docs/superpowers/specs/2026-10-10-mobile-presence-notifications-design.md` — this plan implements implementation phase 3 ("relay alerts and phone alert display"): "Relay decision rule", "Delivery", "Phone" (alert-only notifications), "Failure handling" and the relay/alert-only parts of "Rollout", with the owner decisions D1–D4 below. Shared read marks, presence sampling, the preference store, `set_mobile_delivery` and `mark_read` belong to phases 1–2.

**Fixed inputs from phases 1–2 (consume, do not redefine):**

- C1. Each `hgs ls --json` session row has `attention_signature` (lowercase hex SHA-256 or `null`) and `read`: `{"conversation_id", "reply_id"|null, "attention_signature"|null, "at"}` or `null`. Session rows already carry `reply_id` and `activity`. Phase 1 computes the signature from the same Codex question index the connector publishes as `mobile_attention`.
- C2. `hgs ls --json --local` top level has `desktop_presence: {"idle_seconds": number, "locked": bool, "locked_seconds": number|null}`, aged to listing time and omitted when older than 120 s.
- C3. The gateway snapshot has top-level `preferences: {"mobile_delivery": "immediate"|"away"|"away_5"|"away_10"|"away_15"|"away_30"}`; absent means `away`.

**Owner decisions (2026-10-10):**

- D1. The connector removes top-level `desktop_presence` from every listing (gateway and each direct peer) before hashing or sending the snapshot and sends it beside it: `{"snapshot", "presence": <object|null>}` and `{"machine_id", "snapshot", "presence"}`. The relay accepts `presence` additively, remembers the latest presence and its receipt time per machine without rewriting an unchanged snapshot, and re-evaluates candidates when presence arrives. `preferences` stays inside the snapshot.
- D2. T = max over all machines with any remembered presence of (`received_at − idle_seconds + 120`, or the lock time when earlier). Stale presence cannot make the owner active but still contributes its last activity. No presence ever received for the workspace means away since forever.
- D3. Rollout order: `hgs` + desktop (including the connector) → Android → relay.
- D4. Every condition Android alerts on produces a relay event (recovery-state errors, `pending_question_count`-only questions). Candidates without `attention_signature` are dropped only by native resolution, never by read marks.

## Global Constraints

- API v1 stays additive: new event kind `alert`, new capability feature `presence_alerts`, optional `presence` heartbeat field, optional snapshot fields. Ordinary `attention`, `error` and `completed` events keep their exact JSON shape and remain for state refresh.
- `alert` event JSON is exactly `{"id","computer_id","session","kind":"alert","alert_kind","created_at"}` with `alert_kind` in `attention | error | completed`; no session content.
- Push payloads stay the generic wake `{"event_id": <id>, "kind": "wake"}`; only delivered alerts create or coalesce wake jobs.
- Rule values: presence is fresh for at most 120 s; the owner is active when a machine with fresh presence is unlocked with `idle_seconds < 120`; away since T as in D2; "`away` delivers as soon as the owner is not active; `away_N` delivers when `now >= T + N minutes`" (spec).
- "When information is missing (machine not reachable by the connector, GUI closed, stale snapshot), the owner counts as away: an extra alert is preferred over a missed one." A connector that never sends `presence` therefore behaves like today.
- Preference values are exactly `immediate | away | away_5 | away_10 | away_15 | away_30`; default `away`. Unknown values count as absent.
- Candidates "expire after 24 hours and are bounded per workspace like events": at most one per computer, session and kind, 1,000 per workspace; waiting candidates are rechecked at least every 15 s.
- `relay_schema` stays version 1. Schema changes are only `CREATE TABLE/INDEX IF NOT EXISTS` and nullable columns (`ADD COLUMN IF NOT EXISTS` on PostgreSQL, `PRAGMA table_info`-guarded `ALTER TABLE` on SQLite). Never bind `Arg::Null` (a text-typed parameter) into a double column on PostgreSQL; presence columns are written only with real values.
- Lock order (relay README): global event accounting, payload shard when needed, workspace, credential, then route/request. Candidate rows are locked only after workspace locks or by themselves, always in ascending `id`.
- Rollout (D3): `hgs` + desktop + connector, then Android, then the relay. Each step tolerates older neighbors: the connector retries without `presence` when a relay rejects it (400/422) and probes again after 300 s; Android alert-only mode starts only when the relay advertises `presence_alerts`. The Python reference relay stays strict and exercises the connector's fallback.
- Rust minimum 1.85. Relay checks from `services/mobile/relay`: `cargo +1.85.0 fmt --check`, `cargo +1.85.0 clippy --locked --all-targets -- -D warnings`, `cargo +1.85.0 test --locked`.
- Android without `presence_alerts` keeps today's behavior; per-type switches, card replacement, privacy `publicVersion` and tap targets stay unchanged.
- Repository text is English; match the surrounding code style and comment density (dense one-line SQL in the relay, compact Python and Kotlin).
- Tests use synthetic fixtures, temporary SQLite files and disposable PostgreSQL only; never touch the production relay, live agents, tmux or DeepSeek hosts.
- Relay contract tests fail on the macOS development machine for an unrelated SQLite fixture reason. Run them, the Python fixtures and Android on Linux host `arch`. Mirror the working tree first (from the repository root), and reinstall the Python package whenever `services/mobile/zerus_mobile` changed (tests import the installed package):

  ```sh
  rsync -a --delete -e 'ssh -o BatchMode=yes' \
    --exclude '.git/' --exclude '.beads/' --exclude 'target/' --exclude '.gradle/' \
    --exclude 'build/' --exclude 'local.properties' --exclude 'google-services.json' \
    ./ arch:build/zerus-presence-alerts/
  ssh -o BatchMode=yes arch 'test -x ~/build/zerus-presence-venv/bin/python || python3 -m venv ~/build/zerus-presence-venv; ~/build/zerus-presence-venv/bin/pip install --force-reinstall --no-deps ~/build/zerus-presence-alerts/services/mobile && ~/build/zerus-presence-venv/bin/pip install ~/build/zerus-presence-alerts/services/mobile'
  ```

  Excluded files on arch are not deleted by `--delete`; place a private `mobile/android/app/google-services.json` there yourself for the Firebase variant, or record that variant as unavailable.
- Disposable PostgreSQL on arch (same image as CI), stopped after use:

  ```sh
  ssh -o BatchMode=yes arch 'docker run --rm -d --name zerus-presence-pg -e POSTGRES_HOST_AUTH_METHOD=trust -p 127.0.0.1:55432:5432 postgres:17.11-alpine@sha256:b0f9560a2de083e2cc7382e75f808c7381a32852a7ec49117deedb300e552b24 && until docker exec zerus-presence-pg pg_isready -U postgres; do sleep 1; done'
  ssh -o BatchMode=yes arch 'docker stop zerus-presence-pg'
  ```
- Android on arch: `JAVA_HOME=$HOME/.cache/zerus-toolchains/jdk-17.0.20.1+1 ANDROID_HOME=$HOME/Android/Sdk ./gradlew testDebugUnitTest lintDebug` from `mobile/android`, and the same with `-PzerusFirebase=true`.
- Track execution in Beads under epic `zerus-rk5b` (`bd create`, `bd update`, `bd close`); run `bd export -o .beads/issues.jsonl` before staging tracker changes.
- Commit steps run only when the owner has authorized commits for this execution; otherwise leave the work uncommitted and report it ready. Commit messages are imperative sentence case (repository style) and end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

- A presence-only change (identical snapshot content, new `presence`): presence is remembered without rewriting the snapshot, and held candidates are re-evaluated so a lock alone delivers. Tests: Task 2 `heartbeats_store_presence_outside_the_snapshot`; Task 3 final step of `presence_holds_read_drops_and_lock_delivers_alert`.
- A machine whose heartbeats stop (lid closed, asleep): it stops counting as present, yet away minutes count from its last activity rather than delivering at once. Tests: Task 1 lid-closed rows; Task 4 `stale_presence_keeps_last_activity_for_away_minutes`.
- A new connector talking to a relay that predates `presence` (rollout D3, or the Python reference): heartbeats must keep flowing without the field. Test: Task 2 `test_desktop_presence_travels_beside_the_snapshot_with_old_relay_fallback`.
- Malformed presence or preference from older or faulty hosts (textual or negative durations, missing `locked`, `away_45`): treated as missing or default, never a heartbeat failure. Tests: Task 1 `malformed_presence_is_missing`, `delivery_names_are_exact_and_default_to_away`; Task 2 `heartbeats_store_presence_outside_the_snapshot`.
- A phone in fallback mode (capability fetch failed or relay rolled back to Python) receiving `alert` rows: input, error and completion cards behave exactly as before. Test: Task 5 `withoutPresenceAlertsAlertRowsNeitherAnnounceNorHideCompletion`.

---

## File Structure

Relay (`services/mobile/relay`):

- Create `src/alerts.rs` — pure rule: preference parsing, presence conversion with the receipt clock, owner away time, delivery decision, native need, candidate capture and resolution. No I/O.
- Create `src/store/candidates.rs` — candidate storage, evaluation plans, alert delivery, workspace evaluation and the elected maintenance pass.
- Modify `src/lib.rs` (export `alerts`), `src/store.rs` (declare `candidates`), `src/registry.rs` (shared `archived`; recovery and counted-question events), `src/protocol.rs` (`presence_alerts`, `presence` validation, `peer_heartbeat`), `src/http.rs` (peer heartbeat validation), `src/store/snapshots.rs` (remembered presence, candidate recording, event/wake split, `alert_kind` serialization), `src/store/background.rs` (call the alert pass), `src/db.rs`, `src/sqlite.sql`, `src/postgres.sql` (schema), `README.md`.
- Create `tests/alerts.rs` (pure tables, runnable on macOS); modify `tests/contracts.rs` (store contracts and PostgreSQL cross-worker test).

Connector and Python fixtures (`services/mobile`):

- Modify `zerus_mobile/connector.py` (strip and remember presence, `post_with_presence`, heartbeat use), `zerus_mobile/fleet.py` (peer heartbeat use).
- Modify `tests/test_connector.py`, `tests/test_connector_peers.py`, `tests/fixture_peer_hgs.py` (presence transport and fallback), `tests/test_relay.py` (two shared assertions ignore additive `alert` rows), `tests/test_rust_relay.py` (Rust-only HTTP contract), `tests/fixture_hgs.py` (forward `desktop_presence` and `preferences`).
- Create `tests/test_presence_end_to_end.py` (connector + Rust relay scenario; matches CI's `test*end_to_end.py`).

Android (`mobile/android/app/src`):

- Modify `main/java/app/zerus/mobile/NotificationPolicy.kt`, `NotificationCatalog.kt`, `Notifications.kt`, `RelayApi.kt`; tests `test/java/app/zerus/mobile/NotificationPolicyTest.kt`, `NotificationCatalogTest.kt`; `mobile/android/README.md`.

Docs: `docs/mobile-architecture.md`.

---

### Task 1: Pure alert rule and alertable transitions

**Files:**
- Create: `services/mobile/relay/src/alerts.rs`
- Modify: `services/mobile/relay/src/lib.rs:1-9`
- Modify: `services/mobile/relay/src/registry.rs:1-9,430-490` (shared `archived`; recovery and counted-question events)
- Test: `services/mobile/relay/tests/alerts.rs`

**Interfaces:**
- Consumes: C1–C3 field names; `serde_json::Value`.
- Produces (all in `zerus_relay::alerts`):
  - `pub const PRESENCE_FRESH: f64 = 120.0; pub const IDLE_AWAY: f64 = 120.0; pub const CANDIDATE_TTL: f64 = 86400.0; pub const RECHECK: f64 = 15.0; pub const WORKSPACE_CANDIDATES: i64 = 1000;`
  - `pub enum Delivery { Immediate, Away(u32) }` with `Delivery::DEFAULT`, `fn parse(&str) -> Option<Delivery>`, `fn from_snapshot(&Value) -> Option<Delivery>`, `fn name(self) -> &'static str`
  - `pub struct Presence { pub received: f64, pub away_since: f64 }` with `fn from_value(presence: &Value, received: f64) -> Option<Presence>`
  - `pub fn away_since(&[Presence]) -> Option<f64>` (D2: every remembered machine; `None` = never received)
  - `pub enum Decision { Deliver, Wait(f64) }`, `pub fn decide(Delivery, Option<f64>, now: f64) -> Decision`
  - `pub fn recovery_failed(&Value) -> bool`, `pub fn native_need(&Value, kind: &str) -> bool`
  - `pub struct Candidate { pub session, pub kind: String, pub run_id, pub conversation_id, pub attention_signature, pub reply_id: Option<String> }` with `fn capture(&Value, session: &str, kind: &str) -> Candidate`, `fn from_row(&Value) -> Candidate`, `fn resolved(&self, Option<&Value>) -> bool`
  - `pub fn live<'a>(&'a Value, session: &str) -> Option<&'a Value>`
  - `zerus_relay::registry::archived(&Value) -> bool`; `registry::event_changes` additionally reports `error` for failed recovery and `attention` for counted questions.

- [ ] **Step 1: Write the failing table tests**

Create `services/mobile/relay/tests/alerts.rs`:

```rust
//! Table-driven presence alert rule: preference × presence × read × time,
//! plus the transitions that must reach the relay as events.
use serde_json::{json, Value};
use zerus_relay::{
    alerts::{away_since, decide, Candidate, Decision, Delivery, Presence, RECHECK},
    json::canonical,
    registry::event_changes,
};

const NOW: f64 = 1_000_000.0;

/// `locked`: `None` unlocked, `Some(None)` locked without a reported duration.
fn machine(age: f64, idle: f64, locked: Option<Option<f64>>) -> Presence {
    let presence = json!({"idle_seconds":idle,"locked":locked.is_some(),"locked_seconds":locked.flatten()});
    Presence::from_value(&presence, NOW - age).expect("valid presence")
}
fn candidate(kind: &str) -> Candidate {
    let row = json!({"name":"s","run_id":"r","conversation_id":"c","attention_signature":"s1","reply_id":"p1","phase":"input"});
    Candidate::capture(&json!({ "sessions": [row] }), "s", kind)
}
fn with(base: Value, changes: Value) -> Value {
    let mut row = base;
    for (key, value) in changes.as_object().unwrap() {
        row[key] = value.clone();
    }
    row
}
fn state(changes: Value) -> Value {
    with(json!({"name":"s","run_id":"r","conversation_id":"c","attention_signature":"s1","reply_id":"p1","phase":"input","activity":"idle","read":null}), changes)
}
/// The store applies the same order: resolution first, then the delivery decision.
fn outcome(c: &Candidate, row: &Value, delivery: Delivery, machines: &[Presence]) -> &'static str {
    if c.resolved(Some(row)) {
        "drop"
    } else if decide(delivery, away_since(machines), NOW) == Decision::Deliver {
        "deliver"
    } else {
        "wait"
    }
}

#[test]
fn delivery_follows_preference_presence_and_time() {
    use Decision::{Deliver, Wait};
    use Delivery::{Away, Immediate};
    let active = machine(5.0, 1.0, None);
    let cases = [
        ("immediate ignores an active desktop", Immediate, vec![active], Deliver),
        ("away waits while a desktop is in use", Away(0), vec![active], Wait(NOW + RECHECK)),
        ("away rechecks at the idle threshold", Away(0), vec![machine(0.0, 110.0, None)], Wait(NOW + 10.0)),
        ("reaching the idle threshold is away", Away(0), vec![machine(0.0, 120.0, None)], Deliver),
        ("a locked desktop is away", Away(0), vec![machine(5.0, 1.0, Some(Some(1.0)))], Deliver),
        ("a lock without duration counts from the last input", Away(5), vec![machine(0.0, 400.0, Some(None))], Deliver),
        ("any active desktop holds alerts", Away(0), vec![machine(0.0, 1000.0, None), active], Wait(NOW + RECHECK)),
        ("presence exactly 120 s old is still present", Away(30), vec![machine(120.0, 0.0, None)], Wait(NOW + RECHECK)),
        ("stale presence cannot keep the owner active", Away(0), vec![machine(121.0, 0.0, None)], Deliver),
        ("stale presence keeps its last activity for away minutes", Away(30), vec![machine(121.0, 0.0, None)], Wait(NOW + RECHECK)),
        ("lid closed and locked 5 min ago: away_10 still waits", Away(10), vec![machine(300.0, 0.0, Some(Some(0.0)))], Wait(NOW + RECHECK)),
        ("lid closed and locked 10 min ago: away_10 delivers", Away(10), vec![machine(600.0, 0.0, Some(Some(0.0)))], Deliver),
        ("asleep unlocked: away counts from the idle threshold", Away(10), vec![machine(700.0, 0.0, None)], Wait(NOW + RECHECK)),
        ("no presence ever received means away since forever", Away(30), vec![], Deliver),
        ("away_5 counts from the end of activity", Away(5), vec![machine(0.0, 410.0, None)], Wait(NOW + 10.0)),
        ("away_5 delivers after five minutes", Away(5), vec![machine(0.0, 420.0, None)], Deliver),
        ("away_10 delivers after ten minutes", Away(10), vec![machine(0.0, 720.0, None)], Deliver),
        ("away_15 still waits after ten minutes", Away(15), vec![machine(0.0, 720.0, None)], Wait(NOW + RECHECK)),
        ("the latest activity across machines wins", Away(5), vec![machine(0.0, 1000.0, None), machine(0.0, 300.0, Some(Some(100.0)))], Wait(NOW + RECHECK)),
    ];
    for (name, delivery, machines, expected) in cases {
        assert_eq!(decide(delivery, away_since(&machines), NOW), expected, "{name}");
    }
}

#[test]
fn preference_presence_read_matrix() {
    let held = candidate("attention");
    let unread = state(json!({}));
    let read = state(json!({"read":{"conversation_id":"c","reply_id":null,"attention_signature":"s1","at":1.0}}));
    let active = vec![machine(5.0, 1.0, None)];
    let away_long = vec![machine(0.0, 4000.0, None)];
    let none: Vec<Presence> = vec![];
    let cases = [
        (Delivery::Immediate, &active, &unread, "deliver"),
        (Delivery::Immediate, &active, &read, "drop"),
        (Delivery::Away(0), &active, &unread, "wait"),
        (Delivery::Away(0), &away_long, &unread, "deliver"),
        (Delivery::Away(0), &none, &unread, "deliver"),
        (Delivery::Away(0), &away_long, &read, "drop"),
        (Delivery::Away(30), &away_long, &unread, "deliver"),
        (Delivery::Away(30), &active, &unread, "wait"),
        (Delivery::Away(30), &none, &read, "drop"),
    ];
    for (delivery, machines, row, expected) in cases {
        assert_eq!(outcome(&held, row, delivery, machines), expected, "{delivery:?} {row}");
    }
}

#[test]
fn malformed_presence_is_missing() {
    for (name, presence) in [
        ("absent", Value::Null),
        ("not an object", json!("idle")),
        ("negative idle", json!({"idle_seconds":-1.0,"locked":false})),
        ("textual idle", json!({"idle_seconds":"5","locked":false})),
        ("missing lock state", json!({"idle_seconds":5.0})),
        ("textual lock duration", json!({"idle_seconds":5.0,"locked":true,"locked_seconds":"2"})),
        ("negative lock duration", json!({"idle_seconds":5.0,"locked":true,"locked_seconds":-2.0})),
    ] {
        assert_eq!(Presence::from_value(&presence, NOW), None, "{name}");
    }
    assert_eq!(
        Presence::from_value(&json!({"idle_seconds":5.0,"locked":false,"locked_seconds":null}), NOW),
        Some(Presence { received: NOW, away_since: NOW + 115.0 })
    );
}

#[test]
fn delivery_names_are_exact_and_default_to_away() {
    for name in ["immediate", "away", "away_5", "away_10", "away_15", "away_30"] {
        assert_eq!(Delivery::parse(name).map(Delivery::name), Some(name));
    }
    for name in ["", "AWAY", "away_45", "away_0", "never"] {
        assert_eq!(Delivery::parse(name), None, "{name}");
    }
    assert_eq!(Delivery::DEFAULT, Delivery::Away(0));
    assert_eq!(Delivery::from_snapshot(&json!({"sessions":[]})), None);
    assert_eq!(Delivery::from_snapshot(&json!({"preferences":{"mobile_delivery":"immediate"}})), Some(Delivery::Immediate));
    assert_eq!(Delivery::from_snapshot(&json!({"preferences":{"mobile_delivery":5}})), None);
}

#[test]
fn attention_and_error_candidates_drop_when_handled_read_or_replaced() {
    let read = |signature: &str, conversation: &str| json!({"conversation_id":conversation,"reply_id":null,"attention_signature":signature,"at":1.0});
    for kind in ["attention", "error"] {
        let held = candidate(kind);
        assert_eq!((held.attention_signature.as_deref(), held.reply_id.as_deref()), (Some("s1"), None));
        let cases = [
            ("unchanged and unread", state(json!({})), false),
            ("run changed", state(json!({"run_id":"r2"})), true),
            ("conversation changed", state(json!({"conversation_id":"c2"})), true),
            ("replaced by a new need", state(json!({"attention_signature":"s2"})), true),
            ("resolved", state(json!({"attention_signature":null})), true),
            ("read on any device", state(json!({ "read": read("s1", "c") })), true),
            ("an older read mark", state(json!({ "read": read("s0", "c") })), false),
            ("a mark for another conversation", state(json!({ "read": read("s1", "c0") })), false),
        ];
        for (name, row, dropped) in cases {
            assert_eq!(held.resolved(Some(&row)), dropped, "{kind}: {name}");
        }
        assert!(held.resolved(None), "{kind}: session gone");
    }
}

#[test]
fn an_older_read_mark_never_drops_a_new_codex_question() {
    // hgs derives the signature from the same Codex question index the connector publishes.
    let row = json!({"name":"s","run_id":"r","conversation_id":"c","phase":"working","attention_signature":"s2","mobile_attention":[{"question_id":"q2","question_hash":"h2"}]});
    let held = Candidate::capture(&json!({ "sessions": [row.clone()] }), "s", "attention");
    let older = with(row.clone(), json!({"read":{"conversation_id":"c","reply_id":null,"attention_signature":"s1","at":1.0}}));
    assert!(!held.resolved(Some(&older)));
    let current = with(row, json!({"read":{"conversation_id":"c","reply_id":null,"attention_signature":"s2","at":2.0}}));
    assert!(held.resolved(Some(&current)));
}

#[test]
fn candidates_without_signature_resolve_only_by_native_state() {
    let unsigned = |kind: &str| Candidate::capture(&json!({"sessions":[{"name":"s","run_id":"r","conversation_id":"c","phase":"working"}]}), "s", kind);
    let row = |fields: &Value| with(json!({"name":"s","run_id":"r","conversation_id":"c","phase":"working","read":{"conversation_id":"c","reply_id":null,"attention_signature":null,"at":1.0}}), fields.clone());
    let cases = [
        ("attention", json!({"phase":"approval"}), false),
        ("attention", json!({"mobile_attention":[{"question_id":"q"}]}), false),
        ("attention", json!({"pending_question_count":2}), false),
        ("attention", json!({}), true),
        ("attention", json!({"phase":"input","attention_signature":"s9","read":{"conversation_id":"c","reply_id":null,"attention_signature":"s9","at":2.0}}), false),
        ("error", json!({"phase":"error"}), false),
        ("error", json!({"recovery":{"state":"exhausted","request_id":"x1"}}), false),
        ("error", json!({"phase":"error","recovery":{"state":"waiting","request_id":"x1"}}), true),
        ("error", json!({"phase":"idle"}), true),
    ];
    for (kind, fields, dropped) in cases {
        assert_eq!(unsigned(kind).resolved(Some(&row(&fields))), dropped, "{kind}: {fields}");
    }
}

#[test]
fn completed_candidates_drop_when_read_superseded_or_busy() {
    let held = candidate("completed");
    assert_eq!((held.reply_id.as_deref(), held.attention_signature.as_deref()), (Some("p1"), None));
    let read = |reply: &str| json!({"conversation_id":"c","reply_id":reply,"attention_signature":null,"at":1.0});
    let cases = [
        ("an unread idle turn", state(json!({})), false),
        ("busy again", state(json!({"activity":"busy"})), true),
        ("a newer reply", state(json!({"reply_id":"p2"})), true),
        ("the reply was read", state(json!({ "read": read("p1") })), true),
        ("an older reply was read", state(json!({ "read": read("p0") })), false),
        ("new attention does not drop a finished turn", state(json!({"attention_signature":"s9"})), false),
    ];
    for (name, row, dropped) in cases {
        assert_eq!(held.resolved(Some(&row)), dropped, "{name}");
    }
}

#[test]
fn capture_uses_the_live_row_not_an_archive_with_the_same_name() {
    let snapshot = json!({"sessions":[
        {"name":"s","state":"archived","run_id":"old","conversation_id":"old","attention_signature":"x"},
        {"name":"s","run_id":"r","conversation_id":"c","attention_signature":"s1"}]});
    let c = Candidate::capture(&snapshot, "s", "attention");
    assert_eq!((c.run_id.as_deref(), c.conversation_id.as_deref(), c.attention_signature.as_deref()), (Some("r"), Some("c"), Some("s1")));
    assert_eq!(Candidate::capture(&snapshot, "missing", "attention").run_id, None);
}

#[test]
fn every_phone_alert_condition_produces_a_relay_event() {
    let listing = |fields: Value| json!({ "sessions": [with(json!({"name":"s","run_id":"r","conversation_id":"c","phase":"working","activity":"busy"}), fields)] });
    let cases = [
        ("recovery gives up", json!({}), json!({"recovery":{"state":"exhausted","request_id":"x1"}}), vec!["error"]),
        ("the same failed recovery", json!({"recovery":{"state":"exhausted","request_id":"x1"}}), json!({"recovery":{"state":"exhausted","request_id":"x1"}}), vec![]),
        ("another failed recovery", json!({"recovery":{"state":"blocked","request_id":"x1"}}), json!({"recovery":{"state":"blocked","request_id":"x2"}}), vec!["error"]),
        ("a recovery still retrying", json!({}), json!({"recovery":{"state":"retrying","request_id":"x1"}}), vec![]),
        ("a counted question appears", json!({}), json!({"pending_question_count":1}), vec!["attention"]),
        ("the same counted question", json!({"pending_question_count":1}), json!({"pending_question_count":1}), vec![]),
        ("another counted question", json!({"pending_question_count":1}), json!({"pending_question_count":2}), vec!["attention"]),
    ];
    for (name, before, after, expected) in cases {
        let previous = canonical(&listing(before)).unwrap();
        let kinds: Vec<String> = event_changes(Some(previous.as_str()), &listing(after)).unwrap().into_iter().map(|(_, kind)| kind).collect();
        assert_eq!(kinds, expected, "{name}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd services/mobile/relay && cargo +1.85.0 test --locked --test alerts`
Expected: FAIL to compile with `unresolved import zerus_relay::alerts`.

- [ ] **Step 3: Write the rule module**

Create `services/mobile/relay/src/alerts.rs`:

```rust
//! Presence-aware phone alert rule. Pure functions take the caller's clock.
//! Missing, stale or malformed information means "away": an extra phone
//! alert is preferred over a missed one.
use crate::registry::archived;
use serde_json::Value;

/// Seconds a machine's latest presence keeps it present.
pub const PRESENCE_FRESH: f64 = 120.0;
/// Input idle time after which an unlocked desktop stops counting as active.
pub const IDLE_AWAY: f64 = 120.0;
/// Candidates older than this are discarded without delivery.
pub const CANDIDATE_TTL: f64 = 86400.0;
/// Waiting candidates are evaluated again at least this often.
pub const RECHECK: f64 = 15.0;
/// One candidate per computer, session and kind; this bounds a workspace.
pub const WORKSPACE_CANDIDATES: i64 = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Immediate,
    /// Deliver once the owner has been away for this many minutes.
    Away(u32),
}
impl Delivery {
    pub const DEFAULT: Self = Self::Away(0);
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "immediate" => Self::Immediate,
            "away" => Self::Away(0),
            "away_5" => Self::Away(5),
            "away_10" => Self::Away(10),
            "away_15" => Self::Away(15),
            "away_30" => Self::Away(30),
            _ => return None,
        })
    }
    /// The swarm-wide choice carried by a gateway machine snapshot.
    pub fn from_snapshot(snapshot: &Value) -> Option<Self> {
        snapshot["preferences"]["mobile_delivery"]
            .as_str()
            .and_then(Self::parse)
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::Away(5) => "away_5",
            Self::Away(10) => "away_10",
            Self::Away(15) => "away_15",
            Self::Away(30) => "away_30",
            Self::Away(_) => "away",
        }
    }
}

/// One machine's latest presence, converted with the relay receipt clock so
/// machine clock skew does not matter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Presence {
    pub received: f64,
    /// End of the last activity: the idle threshold crossing, or the lock time when earlier.
    pub away_since: f64,
}
impl Presence {
    /// Reads the heartbeat `presence` object (the listing's `desktop_presence`).
    pub fn from_value(p: &Value, received: f64) -> Option<Self> {
        let seconds = |v: &Value| v.as_f64().filter(|s| s.is_finite() && *s >= 0.0);
        let idle = seconds(&p["idle_seconds"])?;
        let mut away_since = received - idle + IDLE_AWAY;
        if p["locked"].as_bool()? {
            // An unreported lock time is bounded by the last input.
            let lock = if p["locked_seconds"].is_null() {
                idle
            } else {
                seconds(&p["locked_seconds"])?
            };
            away_since = away_since.min(received - lock);
        }
        Some(Self {
            received,
            away_since,
        })
    }
}

/// T: the end of the owner's last activity over every machine with remembered
/// presence. Presence older than `PRESENCE_FRESH` can no longer make the owner
/// active, but its last activity still counts. `None`: never received.
pub fn away_since(machines: &[Presence]) -> Option<f64> {
    machines
        .iter()
        .map(|m| m.away_since.min(m.received + PRESENCE_FRESH))
        .reduce(f64::max)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Decision {
    Deliver,
    /// Keep waiting; evaluate again no later than this time.
    Wait(f64),
}
pub fn decide(delivery: Delivery, away_since: Option<f64>, now: f64) -> Decision {
    let due = match (delivery, away_since) {
        (Delivery::Immediate, _) | (_, None) => return Decision::Deliver,
        (Delivery::Away(minutes), Some(since)) => since + f64::from(minutes) * 60.0,
    };
    if now >= due {
        Decision::Deliver
    } else {
        Decision::Wait(due.min(now + RECHECK))
    }
}

/// Recovery states the phone shows as errors.
pub fn recovery_failed(v: &Value) -> bool {
    matches!(v["recovery"]["state"].as_str(), Some("uncertain" | "blocked" | "exhausted"))
}
/// Whether the native session row still shows the need behind this kind,
/// using the phone's input/approval and error definitions.
pub fn native_need(v: &Value, kind: &str) -> bool {
    if kind == "error" {
        (v["phase"] == "error"
            && !matches!(v["recovery"]["state"].as_str(), Some("waiting" | "dispatching" | "retrying")))
            || recovery_failed(v)
    } else {
        matches!(v["phase"].as_str(), Some("approval" | "input"))
            || v["mobile_attention"].as_array().is_some_and(|a| !a.is_empty())
            || v["pending_question_count"].as_u64().is_some_and(|n| n > 0)
    }
}

/// The live (non-archived) session row with this name.
pub fn live<'a>(snapshot: &'a Value, session: &str) -> Option<&'a Value> {
    snapshot["sessions"]
        .as_array()?
        .iter()
        .find(|v| v["name"] == session && !archived(v))
}
fn text(v: &Value, key: &str) -> Option<String> {
    v[key].as_str().map(str::to_owned)
}

/// The identity a derived event had when it became a candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub session: String,
    pub kind: String,
    pub run_id: Option<String>,
    pub conversation_id: Option<String>,
    pub attention_signature: Option<String>,
    pub reply_id: Option<String>,
}
impl Candidate {
    pub fn capture(snapshot: &Value, session: &str, kind: &str) -> Self {
        let row = live(snapshot, session).unwrap_or(&Value::Null);
        let completed = kind == "completed";
        Self {
            session: session.into(),
            kind: kind.into(),
            run_id: text(row, "run_id"),
            conversation_id: text(row, "conversation_id"),
            attention_signature: if completed { None } else { text(row, "attention_signature") },
            reply_id: if completed { text(row, "reply_id") } else { None },
        }
    }
    /// Reads a stored `alert_candidates` row.
    pub fn from_row(r: &Value) -> Self {
        Self {
            session: text(r, "session").unwrap_or_default(),
            kind: text(r, "kind").unwrap_or_default(),
            run_id: text(r, "run_id"),
            conversation_id: text(r, "conversation_id"),
            attention_signature: text(r, "attention_signature"),
            reply_id: text(r, "reply_id"),
        }
    }
    /// True when the current live row proves the item was handled, read or
    /// replaced. `None` means the snapshot no longer has the live session.
    pub fn resolved(&self, session: Option<&Value>) -> bool {
        let Some(v) = session else {
            return true;
        };
        let current = |key: &str| v[key].as_str();
        if current("run_id") != self.run_id.as_deref()
            || current("conversation_id") != self.conversation_id.as_deref()
        {
            return true;
        }
        // A mark for another conversation never acknowledges this one.
        let read = if v["read"]["conversation_id"].as_str() == self.conversation_id.as_deref() {
            &v["read"]
        } else {
            &Value::Null
        };
        if self.kind == "completed" {
            return v["activity"] == "busy"
                || current("reply_id") != self.reply_id.as_deref()
                || (self.reply_id.is_some() && read["reply_id"].as_str() == self.reply_id.as_deref());
        }
        let Some(captured) = self.attention_signature.as_deref() else {
            // Without a signature only the native state resolves it; read marks never do.
            return !native_need(v, &self.kind);
        };
        current("attention_signature") != Some(captured)
            || read["attention_signature"].as_str() == Some(captured)
    }
}
```

In `services/mobile/relay/src/lib.rs`, add `pub mod alerts;` as the first line (before `pub mod config;`).

- [ ] **Step 4: Share `archived` and emit every alertable transition**

In `services/mobile/relay/src/registry.rs`, change the imports (lines 2-7) to add `alerts::recovery_failed`:

```rust
use crate::{
    alerts::recovery_failed,
    args,
    db::{f, i, now, s, Db, Tx},
    error::{Error, Result},
    json::{canonical, digest},
};
```

Delete the nested function inside `event_changes` (lines 435-437):

```rust
    fn archived(v: &Value) -> bool {
        v["archived"] == true || v["kind"] == "archive" || v["state"] == "archived"
    }
```

and add this module function directly above `pub fn event_changes` (line 430), so `event_changes` keeps calling `archived(v)` unchanged:

```rust
/// Archived rows never produce events or alerts.
pub fn archived(v: &Value) -> bool {
    v["archived"] == true || v["kind"] == "archive" || v["state"] == "archived"
}
```

Inside `event_changes`, next to `fn question`, add:

```rust
    fn questions(v: &Value) -> u64 {
        v["pending_question_count"].as_u64().unwrap_or(0)
    }
```

and replace the `let kind = if ... else if phase == "error" ... { "error" }` part (lines 471-478) with:

```rust
        // Every condition a phone notifies on must surface as an event here.
        let kind = if !p.is_subset(&b)
            || (!p.is_empty() && identity(v) != identity(before))
            || (["approval", "input"].contains(&phase)
                && (v["phase"] != before["phase"] || changed))
            || (questions(v) > 0 && (questions(v) > questions(before) || changed))
        {
            "attention"
        } else if (phase == "error" && (before["phase"] != "error" || changed))
            || (recovery_failed(v)
                && (!recovery_failed(before)
                    || v["recovery"]["request_id"] != before["recovery"]["request_id"]
                    || changed))
        {
            "error"
```

(the following `} else if v["activity"] == "idle" ...` completion branch stays unchanged). The released Python assertions replayed by `test_rust_relay.py` contain neither `recovery` nor `pending_question_count`, so the Python reference relay needs no change for compatibility.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd services/mobile/relay && cargo +1.85.0 fmt && cargo +1.85.0 test --locked --test alerts && cargo +1.85.0 clippy --locked --all-targets -- -D warnings`
Expected: 10 tests PASS; clippy reports no warnings.

- [ ] **Step 6: Commit**

```bash
git add services/mobile/relay/src/alerts.rs services/mobile/relay/src/lib.rs services/mobile/relay/src/registry.rs services/mobile/relay/tests/alerts.rs
git commit -F - <<'EOF'
Add the presence alert rule and relay events for every phone alert

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 2: Presence beside the snapshot (connector, protocol and storage)

**Files:**
- Modify: `services/mobile/zerus_mobile/connector.py:276-296` (init), `:432-454` (`snapshot`), `:1063-1083` (`heartbeats`)
- Modify: `services/mobile/zerus_mobile/fleet.py:143-158` (`publish`)
- Modify: `services/mobile/tests/fixture_peer_hgs.py:37-40`
- Test: `services/mobile/tests/test_connector.py`, `services/mobile/tests/test_connector_peers.py`
- Modify: `services/mobile/relay/src/protocol.rs:590-628`, `services/mobile/relay/src/http.rs:531-545`
- Modify: `services/mobile/relay/src/sqlite.sql:28,48` and append after line 53; `services/mobile/relay/src/postgres.sql` (append after line 62); `services/mobile/relay/src/db.rs:357-401,408-413`
- Modify: `services/mobile/relay/src/store/snapshots.rs:1-153`
- Test: `services/mobile/relay/tests/contracts.rs:25-95` (fixture helpers) and a new test

**Interfaces:**
- Consumes: `alerts::{Delivery, Presence}` (Task 1).
- Produces:
  - Connector: `Connector.presence` (dict or `None`, per machine context), `Connector.presence_retry_at: float`, `async def post_with_presence(self, client, path: str, body: dict, presence) -> None`. Snapshots sent to the relay never contain `desktop_presence`.
  - Relay protocol: `pub fn presence(&Value) -> Result<()>`, `pub fn peer_heartbeat(&Value) -> Result<()>`; `heartbeat` accepts optional `presence`.
  - Columns `nodes.presence_at`, `nodes.presence_until` (double), `nodes.mobile_delivery` (text), `computer_routes.presence_at`, `computer_routes.presence_until` (double), `events.alert_kind` (text); table `alert_candidates(id, workspace_id, computer_id, session, kind, run_id, conversation_id, attention_signature, reply_id, created, next_at)` unique on `(workspace_id, computer_id, session, kind)`, indexes on `(next_at,id)` and `(created,id)`.
  - `presence_at`/`presence_until` hold the latest valid presence (receipt time and `Presence::away_since`); a heartbeat with `null`, absent or malformed `presence` keeps the remembered values. `mobile_delivery` = `Delivery::name()` of the stored gateway snapshot, or `NULL`.
  - `Store::snapshot(&self, c, peer, snapshot: &Value, presence: &Value, encoded: &str)`; `Store::remember_presence(t, route: Option<&str>, gateway: &str, Presence)`.
  - Test helpers in `tests/contracts.rs`: `Fixture::beat(&self, &Value)` (no `presence` field, an older connector), `Fixture::beat_with(&self, &Value, presence: &Value)`, `Fixture::node_row(&self, columns: &str) -> Value`, `fn presence(idle: f64, locked: Option<f64>) -> Value`.

- [ ] **Step 1: Write the failing connector tests**

In `services/mobile/tests/fixture_peer_hgs.py`, replace the `ls` branch output line (line 40) with:

```python
    listing = {'sessions': sessions, 'peers': [{'via': 'never-export-third-peer'}]}
    if isinstance(inventory.get('presence', {}).get(machine), dict):
        listing['desktop_presence'] = inventory['presence'][machine]
    print(json.dumps(listing))
```

Add to `services/mobile/tests/test_connector.py` (class `ConnectorTests`):

```python
    async def test_desktop_presence_travels_beside_the_snapshot_with_old_relay_fallback(self):
        presence = {'idle_seconds': 4.0, 'locked': False, 'locked_seconds': None}
        self.hgs.write_text(FAKE_HGS.replace("'hgs_version': '9.8.7', ", "'hgs_version': '9.8.7', 'desktop_presence': %r, " % presence))
        snapshot = await self.connector.snapshot()
        self.assertNotIn('desktop_presence', snapshot)
        self.assertEqual(self.connector.presence, presence)
        sent = []

        async def http(client, method, path, body=None):
            sent.append(body)
            if 'presence' in body and len(sent) == 1:
                raise RelayError(400)  # A relay predating the additive field.
            return {'ok': True}

        self.connector.http = http
        for _ in range(2):
            await self.connector.post_with_presence(None, '/v1/node/heartbeat', {'snapshot': snapshot}, self.connector.presence)
        self.assertEqual([sorted(body) for body in sent], [['presence', 'snapshot'], ['snapshot'], ['snapshot']])
        self.connector.presence_retry_at = 0.0  # The periodic probe found a newer relay.
        await self.connector.post_with_presence(None, '/v1/node/heartbeat', {'snapshot': snapshot}, None)
        self.assertEqual(sent[-1], {'snapshot': snapshot, 'presence': None})
        self.hgs.write_text(FAKE_HGS)  # The desktop GUI stopped reporting presence.
        await self.connector.snapshot()
        self.assertIsNone(self.connector.presence)
```

Add to `services/mobile/tests/test_connector_peers.py` (class `PeerConnectorTests`):

```python
    async def test_peer_presence_travels_beside_its_snapshot(self):
        presence = {'idle_seconds': 7.0, 'locked': True, 'locked_seconds': 3.0}
        self.inventory['presence'] = {PEER: presence}
        self.write_inventory()
        self.connector.fleet.confirm_manifest(self.connector.fleet.heartbeat(await self.connector.snapshot()))
        with patch.object(self.connector, 'http', AsyncMock(return_value={})) as http:
            await self.connector.fleet.publish(None)
        path, body = http.call_args.args[2], http.call_args.args[3]
        self.assertEqual(path, f'/v1/node/peers/{self.identity}/heartbeat')
        self.assertNotIn('desktop_presence', body['snapshot'])
        self.assertEqual(body['presence'], presence)
        self.assertIsNone(self.connector.presence)
```

- [ ] **Step 2: Run the connector tests to verify they fail**

Sync to arch (Global Constraints), then run:
`ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts && ~/build/zerus-presence-venv/bin/python -m unittest discover -s services/mobile/tests -p "test_connector*.py" -v'`
Expected: the two new tests FAIL (`desktop_presence` still in the snapshot; `AttributeError: 'Connector' object has no attribute 'presence'`).

- [ ] **Step 3: Implement the connector side**

In `services/mobile/zerus_mobile/connector.py`:

At the end of `_init_machine_context` (after `self.capabilities_next_poll = 0.0`), add:

```python
        self.presence = None  # Latest listing desktop_presence; sent beside, never inside, the snapshot.
```

In `__init__`, after `self.fleet = Fleet(self, ConnectorError)`, add:

```python
        self.presence_retry_at = 0.0  # Relays predating `presence` reject it; probe again later.
```

In `snapshot()`, directly after `snapshot.pop("peers", None)`, add:

```python
        # Presence ages on every listing; keeping it out of the snapshot preserves
        # the relay's unchanged-snapshot shortcut.
        presence = snapshot.pop("desktop_presence", None)
        self.presence = presence if isinstance(presence, dict) else None
```

Add a method after `heartbeats`:

```python
    async def post_with_presence(self, client: aiohttp.ClientSession, path: str, body: dict, presence) -> None:
        """Send presence beside a snapshot. An older relay rejects the additive
        field with 400/422; retry without it and probe again after five minutes."""
        if time.monotonic() < self.presence_retry_at:
            await self.http(client, "POST", path, body)
            return
        try:
            await self.http(client, "POST", path, {**body, "presence": presence})
        except RelayError as error:
            if error.status not in {400, 422}:
                raise
            self.presence_retry_at = time.monotonic() + 300
            await self.http(client, "POST", path, body)
```

In `heartbeats`, replace the first `await self.http(client, "POST", "/v1/node/heartbeat", body)` (inside the `try:` before `except RelayError`) with:

```python
                    await self.post_with_presence(client, "/v1/node/heartbeat", body, self.presence)
```

(the existing peer-routing fallback below keeps its plain `self.http` call without presence).

In `services/mobile/zerus_mobile/fleet.py` `publish`, replace `await self.connector.http(client, "POST", f"/v1/node/peers/{identity}/heartbeat", body)` with:

```python
                await self.connector.post_with_presence(client, f"/v1/node/peers/{identity}/heartbeat",
                                                        body, self.contexts[identity].presence)
```

- [ ] **Step 4: Run the connector tests to verify they pass**

Sync to arch (reinstalling the package), then rerun the Step 2 command.
Expected: all connector tests PASS, including the two new ones.

- [ ] **Step 5: Write the failing relay contract test**

In `services/mobile/relay/tests/contracts.rs`, add to `impl Fixture` (before `async fn close`):

```rust
    /// A heartbeat from a connector that never sends presence.
    async fn beat(&self, snapshot: &Value) {
        self.store
            .heartbeat(&self.node, &json!({ "snapshot": snapshot }), &canonical(snapshot).unwrap())
            .await
            .unwrap();
    }
    async fn beat_with(&self, snapshot: &Value, presence: &Value) {
        self.store
            .heartbeat(&self.node, &json!({"snapshot":snapshot,"presence":presence}), &canonical(snapshot).unwrap())
            .await
            .unwrap();
    }
    async fn node_row(&self, columns: &str) -> Value {
        self.store.db.one(&format!("SELECT {columns} FROM nodes WHERE id=?"), args![&self.node.id]).await.unwrap().unwrap()
    }
```

After the `impl Fixture` block add:

```rust
fn presence(idle: f64, locked: Option<f64>) -> Value {
    json!({"idle_seconds":idle,"locked":locked.is_some(),"locked_seconds":locked})
}
```

Add the test after `snapshot_events_and_push_claims`:

```rust
#[tokio::test]
async fn heartbeats_store_presence_outside_the_snapshot() {
    let empty = json!({"sessions":[]});
    assert!(protocol::heartbeat(&json!({"snapshot":empty,"presence":null})).is_ok());
    assert!(protocol::heartbeat(&json!({"snapshot":empty,"presence":presence(1.0, None)})).is_ok());
    assert!(protocol::heartbeat(&json!({"snapshot":empty,"presence":"idle"})).is_err());
    let machine = Uuid::new_v4().to_string();
    assert!(protocol::peer_heartbeat(&json!({"machine_id":machine,"snapshot":empty,"presence":presence(1.0, None)})).is_ok());
    assert!(protocol::peer_heartbeat(&json!({"machine_id":machine,"snapshot":empty,"presence":[]})).is_err());
    let f = Fixture::new().await;
    let snapshot = json!({"sessions":[],"preferences":{"mobile_delivery":"away_10"}});
    let before = now();
    f.beat_with(&snapshot, &presence(30.0, None)).await;
    let row = f.node_row("presence_at,presence_until,mobile_delivery").await;
    let at = row["presence_at"].as_f64().unwrap();
    assert!(at >= before && at <= now());
    assert!((row["presence_until"].as_f64().unwrap() - (at - 30.0 + 120.0)).abs() < 1e-6);
    assert_eq!(row["mobile_delivery"], "away_10");
    // A presence-only change is remembered without rewriting the unchanged snapshot.
    f.store.db.exec("UPDATE nodes SET snapshot='{}' WHERE id=?", args![&f.node.id]).await.unwrap();
    f.beat_with(&snapshot, &presence(5.0, Some(1.0))).await;
    let row = f.node_row("snapshot,presence_at,presence_until").await;
    assert_eq!(row["snapshot"], "{}");
    let locked = row["presence_at"].as_f64().unwrap();
    assert!(locked >= at && (row["presence_until"].as_f64().unwrap() - (locked - 1.0)).abs() < 1e-6);
    // Null, absent and malformed presence keep the remembered value.
    let remembered = f.node_row("presence_at,presence_until").await;
    f.beat_with(&snapshot, &Value::Null).await;
    f.beat(&snapshot).await;
    f.beat_with(&json!({"sessions":[],"preferences":{"mobile_delivery":"away_45"}}), &json!({"idle_seconds":"5","locked":false})).await;
    assert_eq!(f.node_row("presence_at,presence_until").await, remembered);
    assert!(f.node_row("mobile_delivery").await["mobile_delivery"].is_null());
    f.close().await;
}
```

- [ ] **Step 6: Run the relay test to verify it fails**

Sync to arch, then run:
`ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts/services/mobile/relay && cargo +1.85.0 test --locked --test contracts heartbeats_store_presence'`
Expected: FAIL to compile with `cannot find function peer_heartbeat in module protocol`.

- [ ] **Step 7: Validate `presence` additively**

In `services/mobile/relay/src/protocol.rs`, add above `pub fn heartbeat` (line 605):

```rust
/// Optional desktop presence beside a snapshot. The alert rule reads its
/// fields; a malformed object only counts as missing presence.
pub fn presence(v: &Value) -> Result<()> {
    if v.is_null() || v.as_object().is_some_and(|o| o.len() <= 16) {
        Ok(())
    } else {
        Err(Error::BAD)
    }
}
pub fn peer_heartbeat(v: &Value) -> Result<()> {
    fields(v, &["machine_id", "snapshot"], &["presence"])?;
    uuid(&v["machine_id"], false)?;
    snapshot(&v["snapshot"])?;
    if let Some(p) = v.get("presence") {
        presence(p)?;
    }
    Ok(())
}
```

In `heartbeat`, change the first line to `fields(v, &["snapshot"], &["machine_id", "peers", "presence"])?;` and add after `snapshot(&v["snapshot"])?;`:

```rust
    if let Some(p) = v.get("presence") {
        presence(p)?;
    }
```

In `services/mobile/relay/src/http.rs` (peer heartbeat, lines 536-538), replace the three lines `protocol::fields(...)`, `protocol::uuid(&b["machine_id"], false)?;`, `protocol::snapshot(&b["snapshot"])?;` with `protocol::peer_heartbeat(&b)?;`. The snapshot hash (`json::canonical(&b["snapshot"])`) stays computed over the snapshot alone.

- [ ] **Step 8: Add the schema**

`services/mobile/relay/src/sqlite.sql`, line 28 becomes:

```sql
CREATE TABLE IF NOT EXISTS events(id INTEGER PRIMARY KEY AUTOINCREMENT,workspace_id text NOT NULL REFERENCES workspaces(id),node_id text NOT NULL,session text NOT NULL,kind text NOT NULL,created double precision NOT NULL,alert_kind text);
```

line 48 becomes:

```sql
CREATE TABLE IF NOT EXISTS computer_routes(gateway_id TEXT NOT NULL REFERENCES nodes(id),route_id TEXT NOT NULL,computer_id TEXT NOT NULL REFERENCES computers(id),machine_id TEXT,local INTEGER NOT NULL DEFAULT 0,active INTEGER NOT NULL DEFAULT 1,online INTEGER NOT NULL DEFAULT 0,guarded INTEGER NOT NULL DEFAULT 0,name TEXT NOT NULL,snapshot TEXT,snapshot_hash TEXT,last_seen DOUBLE PRECISION,presence_at DOUBLE PRECISION,presence_until DOUBLE PRECISION,PRIMARY KEY(gateway_id,route_id));
```

and append after the final `UPDATE nodes SET computer_id=id WHERE computer_id IS NULL;` (AUTOINCREMENT matters: a replaced candidate never reuses an id a concurrent plan still holds):

```sql

-- Presence-aware phone alerts (additive; relay_schema stays 1).
CREATE TABLE IF NOT EXISTS alert_candidates(id INTEGER PRIMARY KEY AUTOINCREMENT,workspace_id TEXT NOT NULL REFERENCES workspaces(id),computer_id TEXT NOT NULL,session TEXT NOT NULL,kind TEXT NOT NULL,run_id TEXT,conversation_id TEXT,attention_signature TEXT,reply_id TEXT,created DOUBLE PRECISION NOT NULL,next_at DOUBLE PRECISION NOT NULL,UNIQUE(workspace_id,computer_id,session,kind));
CREATE INDEX IF NOT EXISTS alert_candidate_due ON alert_candidates(next_at,id);
CREATE INDEX IF NOT EXISTS alert_candidate_expiry ON alert_candidates(created,id);
```

`services/mobile/relay/src/postgres.sql`, append after the final line:

```sql

-- Presence-aware phone alerts (additive; relay_schema stays 1).
ALTER TABLE nodes ADD COLUMN IF NOT EXISTS presence_at double precision;
ALTER TABLE nodes ADD COLUMN IF NOT EXISTS presence_until double precision;
ALTER TABLE nodes ADD COLUMN IF NOT EXISTS mobile_delivery text;
ALTER TABLE computer_routes ADD COLUMN IF NOT EXISTS presence_at double precision;
ALTER TABLE computer_routes ADD COLUMN IF NOT EXISTS presence_until double precision;
ALTER TABLE events ADD COLUMN IF NOT EXISTS alert_kind text;
CREATE TABLE IF NOT EXISTS alert_candidates(id bigint GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,workspace_id text NOT NULL REFERENCES workspaces(id),computer_id text NOT NULL,session text NOT NULL,kind text NOT NULL,run_id text,conversation_id text,attention_signature text,reply_id text,created double precision NOT NULL,next_at double precision NOT NULL,UNIQUE(workspace_id,computer_id,session,kind));
CREATE INDEX IF NOT EXISTS alert_candidate_due ON alert_candidates(next_at,id);
CREATE INDEX IF NOT EXISTS alert_candidate_expiry ON alert_candidates(created,id);
```

In `services/mobile/relay/src/db.rs` `initialize`, extend the SQLite upgrade list (lines 357-391) so existing Python or Rust databases gain the columns: the `"nodes"` vector gains

```rust
                        ("presence_at", "DOUBLE PRECISION"),
                        ("presence_until", "DOUBLE PRECISION"),
                        ("mobile_delivery", "TEXT"),
```

and add two entries after the `"push_jobs"` entry:

```rust
                ("events", vec![("alert_kind", "TEXT")]),
                (
                    "computer_routes",
                    vec![
                        ("presence_at", "DOUBLE PRECISION"),
                        ("presence_until", "DOUBLE PRECISION"),
                    ],
                ),
```

In the SQLite `schema.replace(...)` at line 410, the first replacement target string becomes:

```rust
"snapshot_hash text,computer_id TEXT,machine_id TEXT,manifest_hash TEXT,presence_at DOUBLE PRECISION,presence_until DOUBLE PRECISION,mobile_delivery TEXT)"
```

- [ ] **Step 9: Remember presence without rewriting unchanged snapshots**

In `services/mobile/relay/src/store/snapshots.rs`, change the imports (lines 1-7) to:

```rust
use super::*;
use crate::{
    alerts::{Delivery, Presence},
    db::{f, Arg, Tx},
    protocol::{self, MIB},
    registry,
};
use std::collections::BTreeSet;
```

In `heartbeat` (line 49) pass the field: `self.snapshot(c, None, &b["snapshot"], &b["presence"], encoded).await`; in `peer_heartbeat` (lines 58-64) likewise insert `&b["presence"],` after `&b["snapshot"],`.

Change the `snapshot` signature (lines 66-72) to take `presence: &Value` after `snapshot: &Value`, and replace the unchanged-content shortcut (line 77) with:

```rust
        let presence = Presence::from_value(presence, now());
        if peer.is_none() {
            let mut t = self.db.begin().await?;
            if t.exec("UPDATE nodes SET last_seen=? WHERE id=? AND workspace_id=? AND revoked=0 AND (snapshot_hash=? OR EXISTS(SELECT 1 FROM computers c WHERE c.id=nodes.computer_id AND c.revoked=1))",args![now(),&c.id,&c.workspace,&hash]).await? != 0 {
                // Unchanged content: remember presence without rewriting the snapshot.
                if let Some(p) = presence {
                    Self::remember_presence(&mut t, None, &c.id, p).await?;
                }
                t.commit().await?;
                return Ok(());
            }
            // SQLite has one connection; release it before the full path.
            drop(t);
        }
```

Replace the two snapshot writes (lines 131-139, `if let Some((route, _)) = peer { ... } else { ... }`) with:

```rust
            if let Some((route, _)) = peer {
                t.exec("UPDATE computer_routes SET snapshot=?,snapshot_hash=?,last_seen=? WHERE gateway_id=? AND route_id=?",args![encoded,&hash,now(),&c.id,route]).await?;
            } else {
                let delivery = Delivery::from_snapshot(snapshot).map_or(Arg::Null, |d| Arg::Text(d.name()));
                t.exec("UPDATE nodes SET snapshot=?,snapshot_hash=?,last_seen=?,mobile_delivery=? WHERE id=?",args![encoded,&hash,now(),delivery,&c.id]).await?;
            }
            if let Some(p) = presence {
                Self::remember_presence(&mut t, peer.map(|(route, _)| route), &c.id, p).await?;
            }
```

Add after `snapshot`:

```rust
    /// Remembers a machine's latest presence with its receipt time. A heartbeat
    /// without valid presence keeps the stored value, which then goes stale.
    async fn remember_presence(t: &mut Tx, route: Option<&str>, gateway: &str, p: Presence) -> Result<()> {
        if let Some(route) = route {
            t.exec("UPDATE computer_routes SET presence_at=?,presence_until=? WHERE gateway_id=? AND route_id=?",args![p.received,p.away_since,gateway,route]).await?;
        } else {
            t.exec("UPDATE nodes SET presence_at=?,presence_until=? WHERE id=?",args![p.received,p.away_since,gateway]).await?;
        }
        Ok(())
    }
```

- [ ] **Step 10: Run the tests to verify they pass**

Sync to arch, then run:
`ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts/services/mobile/relay && cargo +1.85.0 test --locked --test contracts'`
Expected: all contract tests PASS, including `heartbeats_store_presence_outside_the_snapshot`. Locally: `cd services/mobile/relay && cargo +1.85.0 fmt && cargo +1.85.0 clippy --locked --all-targets -- -D warnings` reports nothing. Rerun the Step 2 Python command: PASS.

- [ ] **Step 11: Commit**

```bash
git add services/mobile/zerus_mobile/connector.py services/mobile/zerus_mobile/fleet.py services/mobile/tests/fixture_peer_hgs.py services/mobile/tests/test_connector.py services/mobile/tests/test_connector_peers.py services/mobile/relay/src services/mobile/relay/tests/contracts.rs
git commit -F - <<'EOF'
Send desktop presence beside connector snapshots and remember it per machine

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 3: Alert candidates, workspace evaluation and `alert` events

**Files:**
- Create: `services/mobile/relay/src/store/candidates.rs`
- Modify: `services/mobile/relay/src/store.rs:2-5` (declare the module)
- Modify: `services/mobile/relay/src/store/snapshots.rs` (imports, unchanged-content shortcut, `event_locks` call, snapshot tail, `event_locks`, `publish`, `events`)
- Modify: `services/mobile/relay/src/protocol.rs:53`
- Test: `services/mobile/relay/tests/contracts.rs` (new tests; update `snapshot_events_and_push_claims` and the `postgres_cross_worker_contract` events assertion)
- Test: `services/mobile/tests/test_relay.py:211,243`, `services/mobile/tests/test_rust_relay.py` (new Rust-only method)

**Interfaces:**
- Consumes: `alerts::*` (Task 1); presence columns, `remember_presence`, `alert_candidates`, `Fixture::beat`, `beat_with`, `node_row`, `presence` (Task 2).
- Produces (crate-internal, `impl Store`):
  - `pub(super) async fn event_locks(&self, t: &mut Tx, spaces: &[&str], count: usize) -> Result<()>`
  - `pub(super) async fn append_event(&self, t: &mut Tx, w: &str, computer: &str, session: &str, kind: &str, alert_kind: Option<&str>) -> Result<Value>` (returns the `RETURNING id` row)
  - `pub(super) async fn wake_devices(&self, t: &mut Tx, w: &str, event_id: &Value) -> Result<()>`
  - `pub(super) async fn record_candidates(&self, t: &mut Tx, w: &str, computer: &str, candidates: &[Candidate]) -> Result<()>`
  - `pub(super) async fn evaluate_workspace(&self, w: &str, accepted: Option<(&str, &Value)>) -> Result<()>`
  - private in `candidates.rs`: `struct AlertPlan`, `enum Action { Drop, Deliver, Wait(f64) }`, `fn alert_plan(&self, t, w, accepted: Option<(&str, &Value)>, due: Option<f64>, at: f64) -> Result<AlertPlan>`, `fn apply_alerts(&self, t, w, AlertPlan) -> Result<bool>`, `fn computer_snapshot(&self, t, computer) -> Result<Option<Value>>`
  - `protocol::FEATURES` contains `"presence_alerts"`.
  - Test helpers: `Fixture::count(&self, from: &str) -> i64`, `Fixture::alert_count(&self) -> i64`, `Fixture::events_of(&self, kind: &str) -> Vec<Value>`, `fn signature(digit: char) -> String`, `fn session_row(phase, activity, attention, signature: Option<&str>, read: Value) -> Value`, `fn listing(session: Value) -> Value`, `async fn gateway_beat(...)`, `async fn peer_beat(...)`.

Why the Python reference relay is not extended: `test_rust_relay.py` replays `RelayTests` by name against Rust. Two of those assert exact event-kind lists; with `alert` rows additive in API v1, the released contract is the ordinary sequence, so those assertions ignore `alert` rows and pass on both implementations. Alert-specific HTTP behavior is a Rust-only method on `RustHttpContracts`.

- [ ] **Step 1: Write the failing tests**

In `services/mobile/relay/tests/contracts.rs`, add to `impl Fixture`:

```rust
    /// Counts the rows of a FROM clause, for example `alert_candidates WHERE session='s0'`.
    async fn count(&self, from: &str) -> i64 {
        i(&self.store.db.one(&format!("SELECT count(*) AS n FROM {from}"), &[]).await.unwrap().unwrap(), "n")
    }
    async fn alert_count(&self) -> i64 {
        self.count("events WHERE kind='alert'").await
    }
    async fn events_of(&self, kind: &str) -> Vec<Value> {
        self.store.events(&self.phone, 0).await.unwrap()["events"].as_array().unwrap().iter().filter(|e| e["kind"] == kind).cloned().collect()
    }
```

Next to `fn presence` add:

```rust
fn signature(digit: char) -> String {
    digit.to_string().repeat(64)
}
fn session_row(phase: &str, activity: &str, attention: &str, signature: Option<&str>, read: Value) -> Value {
    json!({"name":"example","run_id":"r","conversation_id":"c","phase":phase,"activity":activity,"attention_id":attention,"attention_signature":signature,"read":read})
}
fn listing(session: Value) -> Value {
    json!({ "sessions": [session] })
}
async fn gateway_beat(f: &Fixture, root: &str, peers: &Value, snapshot: &Value) {
    f.store.heartbeat(&f.node, &json!({"snapshot":snapshot,"machine_id":root,"peers":peers,"presence":null}), &canonical(snapshot).unwrap()).await.unwrap();
}
async fn peer_beat(f: &Fixture, route: &str, machine: &str, snapshot: &Value, presence: &Value) {
    f.store.peer_heartbeat(&f.node, route, &json!({"machine_id":machine,"snapshot":snapshot,"presence":presence}), &canonical(snapshot).unwrap()).await.unwrap();
}
```

Replace the assertions of `snapshot_events_and_push_claims` after its heartbeat loop (from `let e = f.store.events(&f.phone, 0)` through `assert_eq!(e["events"][0]["kind"], "attention");`) with:

```rust
    // A connector without presence counts as away: each transition alerts at once.
    let e = f.store.events(&f.phone, 0).await.unwrap();
    let kinds: Vec<_> = e["events"].as_array().unwrap().iter().map(|v| v["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["attention", "alert"]);
    assert_eq!(e["events"][1]["alert_kind"], "attention");
```

and its final `assert_eq!(count["events"], 1);` becomes `assert_eq!(count["events"], 2);`.

In `postgres_cross_worker_contract`, the events assertion after the `for phase in ["idle", "input"]` loop expects `2` instead of `1` (attention plus its alert; the push-claim race below still yields exactly one job).

Add these tests after `heartbeats_store_presence_outside_the_snapshot`:

```rust
#[tokio::test]
async fn presence_holds_read_drops_and_lock_delivers_alert() {
    let f = Fixture::new().await;
    let features = protocol::capabilities(vec![])["features"].clone();
    assert!(features.as_array().unwrap().iter().any(|v| v == "presence_alerts"));
    f.store.register_push(&f.phone, "fcm", "synthetic-token").await.unwrap();
    let active = presence(3.0, None);
    let (first, second) = (signature('1'), signature('2'));
    f.beat_with(&listing(session_row("working", "busy", "", None, Value::Null)), &active).await;
    f.beat_with(&listing(session_row("input", "busy", "q1", Some(&first), Value::Null)), &active).await;
    assert_eq!(f.events_of("attention").await.len(), 1);
    assert_eq!((f.count("alert_candidates").await, f.alert_count().await, f.count("push_jobs").await), (1, 0, 0));
    // Reading the question on a desktop drops the held candidate.
    let read = json!({"conversation_id":"c","reply_id":null,"attention_signature":first,"at":1.0});
    f.beat_with(&listing(session_row("input", "busy", "q1", Some(&first), read.clone())), &active).await;
    assert_eq!(f.count("alert_candidates").await, 0);
    // A replacement question waits while the owner is active.
    let waiting = listing(session_row("input", "busy", "q2", Some(&second), read));
    f.beat_with(&waiting, &active).await;
    assert_eq!(f.count("alert_candidates").await, 1);
    // Identical snapshot content: the lock alone arrives and delivers.
    f.beat_with(&waiting, &presence(4.0, Some(2.0))).await;
    let alerts = f.events_of("alert").await;
    assert_eq!(alerts.len(), 1);
    assert_eq!(
        (alerts[0]["alert_kind"].as_str(), alerts[0]["session"].as_str(), alerts[0]["computer_id"].as_str()),
        (Some("attention"), Some("example"), Some(f.node.id.as_str()))
    );
    assert!(f.events_of("attention").await.iter().all(|e| e.get("alert_kind").is_none()));
    let jobs = f.store.claim_push_jobs().await.unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(serde_json::from_str::<Value>(s(&jobs[0], "payload")).unwrap(), json!({"event_id":alerts[0]["id"],"kind":"wake"}));
    assert_eq!(f.count("alert_candidates").await, 0);
    f.close().await;
}
#[tokio::test]
async fn immediate_preference_ignores_active_desktop() {
    let f = Fixture::new().await;
    let snapshot = |phase: &str| json!({"sessions":[session_row(phase,"busy","q1",Some(&signature('1')),Value::Null)],"preferences":{"mobile_delivery":"immediate"}});
    f.beat_with(&snapshot("working"), &presence(1.0, None)).await;
    f.beat_with(&snapshot("input"), &presence(1.0, None)).await;
    assert_eq!((f.alert_count().await, f.count("alert_candidates").await), (1, 0));
    f.close().await;
}
#[tokio::test]
async fn completed_candidates_follow_reply_read_and_busy_state() {
    let f = Fixture::new().await;
    let active = presence(1.0, None);
    let none = Value::Null;
    let turn = |activity: &str, reply: &str, read: &Value| listing(json!({"name":"example","run_id":"r","conversation_id":"c","phase":"idle","activity":activity,"reply_id":reply,"read":read}));
    f.beat_with(&turn("busy", "rp0", &none), &active).await;
    f.beat_with(&turn("idle", "rp1", &none), &active).await;
    assert_eq!(f.count("alert_candidates").await, 1);
    f.beat_with(&turn("busy", "rp1", &none), &active).await; // busy again
    assert_eq!(f.count("alert_candidates").await, 0);
    f.beat_with(&turn("idle", "rp2", &none), &active).await;
    let read = json!({"conversation_id":"c","reply_id":"rp2","attention_signature":null,"at":1.0});
    f.beat_with(&turn("idle", "rp2", &read), &active).await; // read on a desktop
    assert_eq!(f.count("alert_candidates").await, 0);
    f.beat_with(&turn("busy", "rp2", &read), &active).await;
    f.beat_with(&turn("idle", "rp3", &read), &active).await;
    f.beat_with(&turn("idle", "rp4", &read), &active).await; // a newer reply supersedes it
    assert_eq!(f.count("alert_candidates").await, 0);
    f.beat_with(&turn("busy", "rp4", &read), &active).await;
    f.beat_with(&turn("idle", "rp5", &read), &active).await;
    f.beat_with(&turn("idle", "rp5", &read), &presence(1.0, Some(0.0))).await;
    let kinds: Vec<_> = f.events_of("alert").await.iter().map(|e| e["alert_kind"].as_str().unwrap().to_owned()).collect();
    assert_eq!(kinds, ["completed"]);
    f.close().await;
}
#[tokio::test]
async fn peer_desktop_presence_holds_gateway_alerts() {
    let f = Fixture::new().await;
    let (root, machine, route) = (Uuid::new_v4().to_string(), Uuid::new_v4().to_string(), Uuid::new_v4().to_string());
    let caps = json!({"protocol_version":1,"operations":protocol::OPERATIONS,"features":["gateway_one_hop"]});
    let peers = json!([{"route_id":route,"machine_id":machine,"name":"Peer","online":true}]);
    // The gateway GUI is closed (presence null); the owner works at the peer.
    let gateway = |phase: &str| json!({"sessions":[session_row(phase,"busy","q1",Some(&signature('1')),Value::Null)],"mobile_capabilities":caps});
    let peer = json!({"sessions":[],"mobile_capabilities":caps});
    gateway_beat(&f, &root, &peers, &gateway("working")).await;
    peer_beat(&f, &route, &machine, &peer, &presence(1.0, None)).await;
    gateway_beat(&f, &root, &peers, &gateway("input")).await;
    assert_eq!((f.count("alert_candidates").await, f.alert_count().await), (1, 0));
    peer_beat(&f, &route, &machine, &peer, &presence(2.0, Some(1.0))).await;
    let alerts = f.events_of("alert").await;
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0]["computer_id"], f.node.id);
    f.close().await;
}
```

In `services/mobile/tests/test_relay.py`, line 211 becomes:

```python
        self.assertEqual([event["kind"] for event in result["events"] if event["kind"] != "alert"], ["attention", "attention", "completed"])
```

and line 243 becomes:

```python
        self.assertEqual([e["kind"] for e in (await self.call("GET", "/v1/events"))["events"] if e["kind"] != "alert"], ["attention", "attention"])
```

In `services/mobile/tests/test_rust_relay.py`, add after `test_existing_python_receipt_identity` (before `headers = test_relay.RelayTests.headers`):

```python
    async def test_presence_alerts_http_contract(self):
        # The database was created by the Python store, so this also covers the in-place upgrade.
        self.assertIn("presence_alerts", (await self.call("GET", "/v1/capabilities"))["features"])
        row = {"name": "codex/project/alert", "run_id": "r", "conversation_id": "c", "phase": "working", "activity": "busy"}

        async def heartbeat(session, idle):
            await self.call("POST", "/v1/node/heartbeat", {"snapshot": {"sessions": [session]},
                "presence": {"idle_seconds": idle, "locked": False, "locked_seconds": None}}, node=True)

        await heartbeat(row, 1.0)
        question = {**row, "phase": "input", "attention_id": "q1", "attention_signature": "a" * 64}
        await heartbeat(question, 1.0)
        self.assertEqual([e["kind"] for e in (await self.call("GET", "/v1/events"))["events"]], ["attention"])
        await heartbeat(question, 600.0)  # Same snapshot; only presence changes.
        events = (await self.call("GET", "/v1/events"))["events"]
        self.assertEqual([e["kind"] for e in events], ["attention", "alert"])
        self.assertNotIn("alert_kind", events[0])
        self.assertEqual(set(events[1]), {"id", "computer_id", "session", "kind", "alert_kind", "created_at"})
        self.assertEqual((events[1]["alert_kind"], events[1]["session"], events[1]["computer_id"]),
                         ("attention", "codex/project/alert", self.node["node_id"]))
```

- [ ] **Step 2: Run the tests to verify they fail**

Sync to arch, then run:
`ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts/services/mobile/relay && cargo +1.85.0 test --locked --test contracts -- presence_holds immediate_preference completed_candidates peer_desktop snapshot_events'`
Expected: FAIL — `presence_holds_read_drops_and_lock_delivers_alert` stops at the missing `presence_alerts` feature; the others fail on counts because no candidates or `alert` events exist yet (for example `left: (0, 0), right: (1, 0)`), and `snapshot_events_and_push_claims` sees only `["attention"]`.

- [ ] **Step 3: Advertise the feature**

`services/mobile/relay/src/protocol.rs` line 53 becomes:

```rust
pub const FEATURES: &[&str] = &["inspect_after", "inspect_agent", "presence_alerts", "send_agent"];
```

- [ ] **Step 4: Split event publication from phone wakes**

In `services/mobile/relay/src/store/snapshots.rs`, extend the `alerts` import to `alerts::{Candidate, Delivery, Presence},`.

Replace `event_locks` with:

```rust
    /// Locks global event accounting, then every workspace that may gain an
    /// event or wake job or lose its oldest one to a cap, in sorted order.
    pub(super) async fn event_locks(&self, t: &mut Tx, spaces: &[&str], count: usize) -> Result<()> {
        let fleet = t
            .one(
                "SELECT events,push_jobs FROM global_usage WHERE id=1 FOR UPDATE",
                &[],
            )
            .await?
            .ok_or(Error::BAD)?;
        let mut workspaces: BTreeSet<String> = spaces.iter().map(|w| (*w).to_string()).collect();
        for r in t
            .all(
                "SELECT workspace_id FROM events ORDER BY id LIMIT ?",
                args![(i(&fleet, "events") + count as i64 - 100000).max(0)],
            )
            .await?
        {
            workspaces.insert(s(&r, "workspace_id").into());
        }
        // At most 1000 registrations per workspace can create jobs during one publication.
        let jobs = i(&fleet, "push_jobs") + 1000 * spaces.len() as i64 - 10000;
        for r in t.all("SELECT d.workspace_id FROM push_jobs j JOIN devices d ON d.id=j.device_id ORDER BY j.id LIMIT ?",args![jobs.max(0)]).await?{workspaces.insert(s(&r,"workspace_id").into());}
        for w in workspaces {
            self.db.usage(t, &w).await?;
        }
        Ok(())
    }
```

and update its caller in `snapshot` to `self.event_locks(&mut t, &[c.workspace.as_str()], changes.len()).await?;`.

Replace `publish` with these three functions:

```rust
    /// Ordinary events refresh phone state; their candidates decide phone wakes.
    async fn publish(&self, t: &mut Tx, w: &str, computer: &str, candidates: &[Candidate]) -> Result<()> {
        for c in candidates {
            self.append_event(t, w, computer, &c.session, &c.kind, None)
                .await?;
        }
        self.record_candidates(t, w, computer, candidates).await
    }
    /// Appends one event inside the workspace and fleet caps. The caller holds event locks.
    pub(super) async fn append_event(
        &self,
        t: &mut Tx,
        w: &str,
        computer: &str,
        session: &str,
        kind: &str,
        alert_kind: Option<&str>,
    ) -> Result<Value> {
        let usage = t
            .one(
                "SELECT events FROM workspace_usage WHERE workspace_id=?",
                args![w],
            )
            .await?
            .ok_or(Error::BAD)?;
        let fleet = t
            .one("SELECT events FROM global_usage WHERE id=1", &[])
            .await?
            .ok_or(Error::BAD)?;
        let victim = if i(&usage, "events") >= 10000 {
            t.one(
                "SELECT id,workspace_id FROM events WHERE workspace_id=? ORDER BY id LIMIT 1",
                args![w],
            )
            .await?
        } else if i(&fleet, "events") >= 100000 {
            t.one(
                "SELECT id,workspace_id FROM events ORDER BY id LIMIT 1",
                &[],
            )
            .await?
        } else {
            None
        };
        if let Some(v) = victim {
            self.delete_event(t, &v).await?;
        }
        let event=t.one("INSERT INTO events(workspace_id,node_id,session,kind,alert_kind,created) VALUES(?,?,?,?,?,?) RETURNING id",args![w,computer,session,kind,alert_kind.map_or(Arg::Null,Arg::Text),now()]).await?.ok_or(Error::BAD)?;
        t.exec(
            "UPDATE workspace_usage SET events=events+1 WHERE workspace_id=?",
            args![w],
        )
        .await?;
        t.exec("UPDATE global_usage SET events=events+1 WHERE id=1", &[])
            .await?;
        Ok(event)
    }
    /// Coalesces one generic wake per registered device toward the newest alert.
    pub(super) async fn wake_devices(&self, t: &mut Tx, w: &str, event_id: &Value) -> Result<()> {
        let payload = canonical(&json!({"event_id":event_id,"kind":"wake"}))?;
        let registrations=t.all("SELECT p.device_id FROM pushes p JOIN devices d ON d.id=p.device_id WHERE d.workspace_id=? AND d.revoked=0 ORDER BY p.device_id LIMIT 1000",args![w]).await?;
        for reg in registrations {
            if let Some(j) = t
                .one(
                    "SELECT id,lease_until FROM push_jobs WHERE device_id=? ORDER BY id LIMIT 1",
                    args![&reg["device_id"]],
                )
                .await?
            {
                if j["lease_until"].is_null() || f(&j, "lease_until") <= now() {
                    t.exec("UPDATE push_jobs SET event_id=?,payload=?,next_at=CASE WHEN next_at<? THEN next_at ELSE ? END WHERE id=?",args![event_id,&payload,now(),now(),&j["id"]]).await?;
                }
                continue;
            }
            let usage = t
                .one(
                    "SELECT push_jobs FROM workspace_usage WHERE workspace_id=?",
                    args![w],
                )
                .await?
                .ok_or(Error::BAD)?;
            let fleet = t
                .one("SELECT push_jobs FROM global_usage WHERE id=1", &[])
                .await?
                .ok_or(Error::BAD)?;
            let victim = if i(&usage, "push_jobs") >= 1000 {
                t.one("SELECT j.id,d.workspace_id FROM push_jobs j JOIN devices d ON d.id=j.device_id WHERE d.workspace_id=? ORDER BY j.id LIMIT 1",args![w]).await?
            } else if i(&fleet, "push_jobs") >= 10000 {
                t.one("SELECT j.id,d.workspace_id FROM push_jobs j JOIN devices d ON d.id=j.device_id ORDER BY j.id LIMIT 1",&[]).await?
            } else {
                None
            };
            if let Some(j) = victim {
                self.delete_job(t, &j).await?;
            }
            t.exec("INSERT INTO push_jobs(device_id,event_id,payload,next_at,created) VALUES(?,?,?,?,?)",args![&reg["device_id"],event_id,&payload,now(),now()]).await?;
            t.exec(
                "UPDATE workspace_usage SET push_jobs=push_jobs+1 WHERE workspace_id=?",
                args![w],
            )
            .await?;
            t.exec(
                "UPDATE global_usage SET push_jobs=push_jobs+1 WHERE id=1",
                &[],
            )
            .await?;
        }
        Ok(())
    }
```

In the unchanged-content shortcut written in Task 2, re-evaluate held candidates when presence arrives. Its body inside `if t.exec(...).await? != 0 {` becomes:

```rust
                // Unchanged content: remember presence without rewriting the snapshot.
                let mut held = false;
                if let Some(p) = presence {
                    Self::remember_presence(&mut t, None, &c.id, p).await?;
                    held = t.one("SELECT id FROM alert_candidates WHERE workspace_id=? LIMIT 1",args![&c.workspace]).await?.is_some();
                }
                t.commit().await?;
                if held && self.evaluate_workspace(&c.workspace, None).await.is_err() {
                    eprintln!("relay alert evaluation deferred");
                }
                return Ok(());
```

Replace the tail of the full path (from `if !changes.is_empty() {` after the presence write through `return Ok(());`) with:

```rust
            let computer = s(&current, "computer_id").to_owned();
            // Held candidates are re-evaluated with the presence and read state just stored.
            let held = !changes.is_empty()
                || t.one("SELECT id FROM alert_candidates WHERE workspace_id=? LIMIT 1", args![&c.workspace]).await?.is_some();
            if !changes.is_empty() {
                let candidates: Vec<_> = changes
                    .iter()
                    .map(|(session, kind)| Candidate::capture(snapshot, session, kind))
                    .collect();
                self.publish(&mut t, &c.workspace, &computer, &candidates)
                    .await?;
                let topic = format!("workspace:{}", c.workspace);
                t.notify(&topic).await?;
                t.commit().await?;
                self.db.signal(topic);
            } else {
                t.commit().await?;
            }
            // The snapshot is accepted; maintenance retries a failed evaluation.
            if held
                && self
                    .evaluate_workspace(&c.workspace, Some((computer.as_str(), snapshot)))
                    .await
                    .is_err()
            {
                eprintln!("relay alert evaluation deferred");
            }
            return Ok(());
```

In `events()`, the query becomes

```rust
        let rows=t.all("SELECT id,node_id,session,kind,alert_kind,created FROM events WHERE workspace_id=? AND id>? ORDER BY id LIMIT 100",args![&c.workspace,after]).await?;
```

and the event construction inside the loop becomes:

```rust
            let mut e = json!({"id":r["id"],"computer_id":r["node_id"],"session":r["session"],"kind":r["kind"],"created_at":r["created"]});
            // Ordinary events keep their released shape.
            if r["kind"] == "alert" {
                e["alert_kind"] = r["alert_kind"].clone();
            }
```

- [ ] **Step 5: Write candidate storage and evaluation**

In `services/mobile/relay/src/store.rs`, add `mod candidates;` after `mod background;`.

Create `services/mobile/relay/src/store/candidates.rs`:

```rust
//! Alert candidates: derived events held until the presence rule allows a
//! phone alert. Plans read without locks; deleting the candidate row in the
//! transaction that appends the `alert` event is the single-delivery claim
//! across workers. Rows are applied in id order so evaluators lock alike.
use super::*;
use crate::{
    alerts::{self, Candidate, Decision, Delivery, Presence},
    db::{f, Arg, Tx},
};
use std::collections::BTreeSet;

enum Action {
    Drop,
    Deliver,
    Wait(f64),
}
#[derive(Default)]
struct AlertPlan {
    rows: Vec<(Value, Action)>,
}
impl AlertPlan {
    fn deliveries(&self) -> usize {
        self.rows
            .iter()
            .filter(|(_, a)| matches!(a, Action::Deliver))
            .count()
    }
}
fn text(v: &Option<String>) -> Arg<'_> {
    v.as_deref().map_or(Arg::Null, Arg::Text)
}
impl Store {
    /// Records derived events of one computer. The caller holds event locks,
    /// which include this workspace, so the cap is transactional.
    pub(super) async fn record_candidates(&self, t: &mut Tx, w: &str, computer: &str, candidates: &[Candidate]) -> Result<()> {
        let mut count = i(&t.one("SELECT count(*) AS n FROM alert_candidates WHERE workspace_id=?", args![w]).await?.ok_or(Error::BAD)?, "n");
        for c in candidates {
            // A newer event for the same session and kind replaces the held one.
            count -= t.exec("DELETE FROM alert_candidates WHERE workspace_id=? AND computer_id=? AND session=? AND kind=?",args![w,computer,&c.session,&c.kind]).await? as i64;
            if count >= alerts::WORKSPACE_CANDIDATES {
                count -= t.exec("DELETE FROM alert_candidates WHERE id IN (SELECT id FROM alert_candidates WHERE workspace_id=? ORDER BY id LIMIT 1)",args![w]).await? as i64;
            }
            let at = now();
            t.exec("INSERT INTO alert_candidates(workspace_id,computer_id,session,kind,run_id,conversation_id,attention_signature,reply_id,created,next_at) VALUES(?,?,?,?,?,?,?,?,?,?)",args![w,computer,&c.session,&c.kind,text(&c.run_id),text(&c.conversation_id),text(&c.attention_signature),text(&c.reply_id),at,at]).await?;
            count += 1;
        }
        Ok(())
    }
    /// `None` for a revoked or unknown computer; `Some(Null)` when no route
    /// stores a readable snapshot, which is missing data.
    async fn computer_snapshot(&self, t: &mut Tx, computer: &str) -> Result<Option<Value>> {
        if t.one("SELECT id FROM computers WHERE id=? AND revoked=0", args![computer]).await?.is_none() {
            return Ok(None);
        }
        let routes=t.all("SELECT CASE WHEN r.local=1 THEN n.snapshot ELSE r.snapshot END AS snapshot,CASE WHEN r.local=1 THEN n.last_seen ELSE r.last_seen END AS seen FROM computer_routes r JOIN nodes n ON n.id=r.gateway_id WHERE r.computer_id=? AND r.active=1 AND n.revoked=0",args![computer]).await?;
        let latest = routes
            .iter()
            .filter(|r| r["snapshot"].is_string())
            .max_by(|a, b| f(a, "seen").total_cmp(&f(b, "seen")));
        Ok(Some(
            latest
                .and_then(|r| crate::json::parse(s(r, "snapshot").as_bytes(), 32, 50000).ok())
                .unwrap_or(Value::Null),
        ))
    }
    /// Plans one workspace. `accepted` is a snapshot just committed for one
    /// computer; `due` limits maintenance to candidates whose recheck came.
    async fn alert_plan(&self, t: &mut Tx, w: &str, accepted: Option<(&str, &Value)>, due: Option<f64>, at: f64) -> Result<AlertPlan> {
        let rows = t.all("SELECT * FROM alert_candidates WHERE workspace_id=? AND next_at<=? ORDER BY id LIMIT ?",args![w,due.unwrap_or(f64::MAX),alerts::WORKSPACE_CANDIDATES]).await?;
        let mut plan = AlertPlan::default();
        if rows.is_empty() {
            return Ok(plan);
        }
        let delivery = t.one("SELECT mobile_delivery FROM nodes WHERE workspace_id=? AND revoked=0 AND mobile_delivery IS NOT NULL AND last_seen IS NOT NULL ORDER BY last_seen DESC LIMIT 1",args![w]).await?
            .and_then(|r| r["mobile_delivery"].as_str().and_then(Delivery::parse))
            .unwrap_or(Delivery::DEFAULT);
        // Every remembered presence counts, fresh or stale (D2).
        let machines: Vec<Presence> = t.all("SELECT n.presence_at,n.presence_until FROM nodes n JOIN computers c ON c.id=n.computer_id WHERE n.workspace_id=? AND n.revoked=0 AND c.revoked=0 AND n.presence_at IS NOT NULL UNION ALL SELECT r.presence_at,r.presence_until FROM computer_routes r JOIN nodes n ON n.id=r.gateway_id JOIN computers c ON c.id=r.computer_id WHERE n.workspace_id=? AND n.revoked=0 AND c.revoked=0 AND r.local=0 AND r.active=1 AND r.presence_at IS NOT NULL",args![w,w]).await?
            .iter()
            .map(|r| Presence { received: f(r, "presence_at"), away_since: f(r, "presence_until") })
            .collect();
        let decision = alerts::decide(delivery, alerts::away_since(&machines), at);
        let mut resolved = vec![false; rows.len()];
        let mut checked = vec![false; rows.len()];
        if let Some((id, snapshot)) = accepted {
            for (k, r) in rows.iter().enumerate() {
                if s(r, "computer_id") == id {
                    checked[k] = true;
                    resolved[k] = Candidate::from_row(r).resolved(alerts::live(snapshot, s(r, "session")));
                }
            }
        }
        if decision == Decision::Deliver {
            // Other stored snapshots are parsed only before a delivery, one at a time.
            let computers: BTreeSet<&str> = rows
                .iter()
                .zip(&checked)
                .filter(|(_, done)| !**done)
                .map(|(r, _)| s(r, "computer_id"))
                .collect();
            for computer in computers {
                let snapshot = self.computer_snapshot(t, computer).await?;
                for (k, r) in rows.iter().enumerate() {
                    if checked[k] || s(r, "computer_id") != computer {
                        continue;
                    }
                    match &snapshot {
                        // A revoked or unknown computer has no session to alert about.
                        None => resolved[k] = true,
                        // No stored snapshot is missing data: keep the candidate.
                        Some(Value::Null) => {}
                        Some(current) => {
                            resolved[k] = Candidate::from_row(r).resolved(alerts::live(current, s(r, "session")))
                        }
                    }
                }
            }
        }
        for (k, r) in rows.into_iter().enumerate() {
            let action = if resolved[k] || f(&r, "created") <= at - alerts::CANDIDATE_TTL {
                Action::Drop
            } else {
                match decision {
                    Decision::Deliver => Action::Deliver,
                    Decision::Wait(next) => {
                        // Rewrite only an earlier or elapsed recheck time.
                        let current = f(&r, "next_at");
                        if next >= current && current > at {
                            continue;
                        }
                        Action::Wait(next)
                    }
                }
            };
            plan.rows.push((r, action));
        }
        Ok(plan)
    }
    /// Applies a plan. The caller holds event locks when it contains deliveries.
    /// Returns true when an alert was appended.
    async fn apply_alerts(&self, t: &mut Tx, w: &str, plan: AlertPlan) -> Result<bool> {
        let mut latest = None;
        for (r, action) in plan.rows {
            match action {
                Action::Drop => {
                    t.exec("DELETE FROM alert_candidates WHERE id=?", args![&r["id"]])
                        .await?;
                }
                Action::Wait(next) => {
                    t.exec("UPDATE alert_candidates SET next_at=? WHERE id=?", args![next, &r["id"]])
                        .await?;
                }
                Action::Deliver => {
                    // Another worker may already have delivered or replaced this row.
                    if t.exec("DELETE FROM alert_candidates WHERE id=?", args![&r["id"]]).await? == 1 {
                        latest = Some(self.append_event(t, w, s(&r, "computer_id"), s(&r, "session"), "alert", Some(s(&r, "kind"))).await?);
                    }
                }
            }
        }
        let Some(event) = latest else {
            return Ok(false);
        };
        self.wake_devices(t, w, &event["id"]).await?;
        Ok(true)
    }
    /// Re-evaluates a workspace after a committed snapshot or new presence.
    pub(super) async fn evaluate_workspace(&self, w: &str, accepted: Option<(&str, &Value)>) -> Result<()> {
        let at = now();
        let mut t = self.db.begin().await?;
        let plan = self.alert_plan(&mut t, w, accepted, None, at).await?;
        if plan.rows.is_empty() {
            return Ok(());
        }
        let deliveries = plan.deliveries();
        if deliveries > 0 {
            self.event_locks(&mut t, &[w], deliveries).await?;
        }
        let topic = format!("workspace:{w}");
        let published = self.apply_alerts(&mut t, w, plan).await?;
        if published {
            t.notify(&topic).await?;
        }
        t.commit().await?;
        if published {
            self.db.signal(topic);
        }
        Ok(())
    }
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Locally: `cd services/mobile/relay && cargo +1.85.0 fmt && cargo +1.85.0 clippy --locked --all-targets -- -D warnings && cargo +1.85.0 test --locked --test alerts` — no warnings, PASS.

Sync to arch, then:

```sh
ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts/services/mobile/relay && cargo +1.85.0 test --locked && cargo +1.85.0 build --locked'
ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts && ~/build/zerus-presence-venv/bin/python -m unittest discover -s services/mobile/tests -p test_relay.py -v'
ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts && export ZERUS_RELAY_BINARY="$PWD/services/mobile/relay/target/debug/zerus-relay" PATH="$PWD/services/mobile/relay/target/debug:$PATH" && ~/build/zerus-presence-venv/bin/python -m unittest discover -s services/mobile/tests -p test_rust_relay.py -v && ~/build/zerus-presence-venv/bin/python -m unittest discover -s services/mobile/tests -p "test*end_to_end.py" -v'
```

Start the disposable PostgreSQL (Global Constraints), then:
`ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts/services/mobile/relay && ZERUS_RELAY_TEST_DATABASE_URL=postgresql://postgres@127.0.0.1:55432/postgres cargo +1.85.0 test --locked --test contracts postgres_cross_worker -- --ignored'` and stop the container.
Expected: every test PASS (the Python reference run proves the shared assertions still hold without alerts; `test_rust_relay.py` proves them with alerts plus the new HTTP contract).

- [ ] **Step 7: Commit**

```bash
git add services/mobile/relay/src services/mobile/relay/tests/contracts.rs services/mobile/tests/test_relay.py services/mobile/tests/test_rust_relay.py
git commit -F - <<'EOF'
Hold relay notifications as alert candidates and deliver alert events

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 4: Maintenance pass, expiry, bounds and cross-worker delivery

**Files:**
- Modify: `services/mobile/relay/src/store/candidates.rs` (append `maintain_alerts`)
- Modify: `services/mobile/relay/src/store/background.rs:177-187` (`elected` visibility), `:345-357` (call the pass)
- Modify: `services/mobile/relay/README.md:59-97` (Boundaries), `:145-175` (Upgrade and operation)
- Test: `services/mobile/relay/tests/contracts.rs` (new tests including `postgres_cross_worker_single_alert`)

**Interfaces:**
- Consumes: `alert_plan`, `apply_alerts`, `AlertPlan::deliveries` (Task 3, same file); `event_locks` (Task 3); helpers `beat`, `beat_with`, `count`, `alert_count`, `events_of`, `presence`, `signature`, `session_row`, `listing` (Tasks 2–3).
- Produces: `pub(super) async fn maintain_alerts(&self, at: f64) -> Result<()>`, called at the end of `Store::maintain`; advisory election key `735628114`; `pub(super) async fn elected(t: &mut Tx, key: i64) -> Result<bool>`.

- [ ] **Step 1: Write the failing tests**

Add to `services/mobile/relay/tests/contracts.rs`:

```rust
#[tokio::test]
async fn away_minutes_preference_waits_from_end_of_activity() {
    let f = Fixture::new().await;
    // Input stopped 200 s ago: away for 80 s, so `away_5` waits another 220 s.
    let snapshot = |phase: &str| json!({"sessions":[session_row(phase,"busy","q1",Some(&signature('1')),Value::Null)],"preferences":{"mobile_delivery":"away_5"}});
    f.beat_with(&snapshot("working"), &presence(200.0, None)).await;
    f.beat_with(&snapshot("input"), &presence(200.0, None)).await;
    f.store.maintain(false).await.unwrap();
    assert_eq!((f.count("alert_candidates").await, f.alert_count().await), (1, 0));
    let next = f.store.db.one("SELECT next_at FROM alert_candidates", &[]).await.unwrap().unwrap();
    assert!(next["next_at"].as_f64().unwrap() <= now() + 15.0);
    // Five away minutes later the maintenance pass delivers it.
    f.store.db.exec("UPDATE nodes SET presence_until=presence_until-220", &[]).await.unwrap();
    f.store.db.exec("UPDATE alert_candidates SET next_at=0", &[]).await.unwrap();
    f.store.maintain(false).await.unwrap();
    assert_eq!((f.count("alert_candidates").await, f.alert_count().await), (0, 1));
    f.close().await;
}
#[tokio::test]
async fn stale_presence_keeps_last_activity_for_away_minutes() {
    let f = Fixture::new().await;
    let snapshot = |phase: &str| json!({"sessions":[session_row(phase,"busy","q1",Some(&signature('1')),Value::Null)],"preferences":{"mobile_delivery":"away_10"}});
    // The lid closes and locks the screen; then heartbeats stop.
    f.beat_with(&snapshot("working"), &presence(0.0, Some(0.0))).await;
    f.beat_with(&snapshot("input"), &presence(0.0, Some(0.0))).await;
    let five_minutes = "UPDATE nodes SET presence_at=presence_at-300,presence_until=presence_until-300";
    f.store.db.exec(five_minutes, &[]).await.unwrap();
    f.store.db.exec("UPDATE alert_candidates SET next_at=0", &[]).await.unwrap();
    f.store.maintain(false).await.unwrap();
    // Stale presence is not present, but only five of ten away minutes have passed.
    assert_eq!((f.count("alert_candidates").await, f.alert_count().await), (1, 0));
    f.store.db.exec(five_minutes, &[]).await.unwrap();
    f.store.db.exec("UPDATE alert_candidates SET next_at=0", &[]).await.unwrap();
    f.store.maintain(false).await.unwrap();
    assert_eq!((f.count("alert_candidates").await, f.alert_count().await), (0, 1));
    f.close().await;
}
#[tokio::test]
async fn maintenance_recovers_unevaluated_candidates() {
    let f = Fixture::new().await;
    f.beat(&json!({"sessions":[{"name":"example","run_id":"r","conversation_id":"c","phase":"input","activity":"busy","attention_id":"q1"}]})).await;
    // A committed candidate whose post-snapshot evaluation never ran (worker exit or BUSY).
    f.store.db.exec("INSERT INTO alert_candidates(workspace_id,computer_id,session,kind,run_id,conversation_id,created,next_at) VALUES(?,?,'example','attention','r','c',?,?)", args![&f.node.workspace, &f.node.id, now(), now()]).await.unwrap();
    f.store.maintain(false).await.unwrap();
    let alerts = f.events_of("alert").await;
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0]["alert_kind"], "attention");
    assert_eq!(f.count("alert_candidates").await, 0);
    f.close().await;
}
#[tokio::test]
async fn revoked_computer_drops_held_candidate() {
    let f = Fixture::new().await;
    let active = presence(1.0, None);
    f.beat_with(&listing(session_row("working", "busy", "q1", Some(&signature('1')), Value::Null)), &active).await;
    f.beat_with(&listing(session_row("input", "busy", "q1", Some(&signature('1')), Value::Null)), &active).await;
    assert_eq!(f.count("alert_candidates").await, 1);
    f.store.revoke_computer(&f.node.id).await.unwrap();
    f.store.db.exec("UPDATE alert_candidates SET next_at=0", &[]).await.unwrap();
    f.store.maintain(false).await.unwrap();
    assert_eq!((f.count("alert_candidates").await, f.alert_count().await), (0, 0));
    f.close().await;
}
#[tokio::test]
async fn alert_candidates_are_bounded_and_expire() {
    let f = Fixture::new().await;
    let fleet = |phase: &str| json!({"sessions":(0..1001).map(|k| json!({"name":format!("s{k}"),"run_id":"r","conversation_id":"c","phase":phase})).collect::<Vec<_>>()});
    f.beat_with(&fleet("working"), &presence(1.0, None)).await;
    f.beat_with(&fleet("input"), &presence(1.0, None)).await;
    assert_eq!(f.count("alert_candidates").await, 1000);
    assert_eq!(f.count("alert_candidates WHERE session='s0'").await, 0);
    assert_eq!(f.alert_count().await, 0);
    f.store.db.exec("UPDATE alert_candidates SET created=created-86401", &[]).await.unwrap();
    f.store.maintain(false).await.unwrap();
    assert_eq!(f.count("alert_candidates").await, 1000 - 256);
    for _ in 0..3 {
        f.store.maintain(false).await.unwrap();
    }
    assert_eq!((f.count("alert_candidates").await, f.alert_count().await), (0, 0));
    f.close().await;
}

#[tokio::test]
#[ignore = "requires isolated ZERUS_RELAY_TEST_DATABASE_URL; run explicitly in CI"]
async fn postgres_cross_worker_single_alert() {
    use sqlx::Connection;
    let base = std::env::var("ZERUS_RELAY_TEST_DATABASE_URL").expect("isolated PostgreSQL required");
    let mut admin = sqlx::PgConnection::connect(&base).await.unwrap();
    let name = format!("relay_test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {name}")).execute(&mut admin).await.unwrap();
    let mut url = url::Url::parse(&base).unwrap();
    url.set_path(&name);
    let cfg = Config { database_url: Some(url.to_string()), background: false, ..Config::default() };
    let f = Fixture::config(cfg.clone()).await;
    let second = Store::new(Db::open(std::path::Path::new("unused"), Arc::new(cfg)).await.unwrap());
    f.store.register_push(&f.phone, "fcm", "synthetic-token").await.unwrap();
    let active = presence(1.0, None);
    let question = |attention: &str, digit: char| listing(session_row("input", "busy", attention, Some(&signature(digit)), Value::Null));
    f.beat_with(&listing(session_row("working", "busy", "q0", None, Value::Null)), &active).await;
    f.beat_with(&question("q1", '1'), &active).await;
    assert_eq!(f.count("alert_candidates").await, 1);
    // Both workers' maintenance passes race for one due candidate once presence is stale.
    f.store.db.exec("UPDATE nodes SET presence_at=presence_at-121", &[]).await.unwrap();
    f.store.db.exec("UPDATE alert_candidates SET next_at=0", &[]).await.unwrap();
    let (a, b) = tokio::join!(f.store.maintain(false), second.maintain(false));
    a.unwrap();
    b.unwrap();
    assert_eq!(f.alert_count().await, 1);
    // Presence-only heartbeats on two workers race for the next candidate.
    let waiting = question("q2", '2');
    f.beat_with(&waiting, &active).await;
    assert_eq!(f.count("alert_candidates").await, 1);
    let (a, b) = tokio::join!(
        f.store.heartbeat(&f.node, &json!({"snapshot":waiting,"presence":presence(600.0, None)}), &canonical(&waiting).unwrap()),
        second.heartbeat(&f.node, &json!({"snapshot":waiting,"presence":presence(601.0, None)}), &canonical(&waiting).unwrap())
    );
    a.unwrap();
    b.unwrap();
    assert_eq!((f.alert_count().await, f.count("alert_candidates").await), (2, 0));
    // Both alerts coalesce into the device's single pending wake.
    assert_eq!(f.count("push_jobs").await, 1);
    second.db.close().await;
    f.close().await;
    sqlx::query(&format!("DROP DATABASE {name}")).execute(&mut admin).await.unwrap();
    admin.close().await.unwrap();
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Sync to arch, then:
`ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts/services/mobile/relay && cargo +1.85.0 test --locked --test contracts -- away_minutes stale_presence maintenance_recovers revoked_computer_drops alert_candidates_are_bounded'`
Expected: FAIL — `maintain` neither delivers due candidates nor expires old ones (`left: (1, 0), right: (0, 1)` style mismatches; `alert_candidates_are_bounded_and_expire` keeps 1000 rows; `revoked_computer_drops_held_candidate` keeps its candidate).

- [ ] **Step 3: Implement the elected pass**

In `services/mobile/relay/src/store/background.rs`, change line 177 to `pub(super) async fn elected(t: &mut Tx, key: i64) -> Result<bool> {` and replace the final `Ok(())` of `maintain` (after the poll-lease loop) with:

```rust
        // Last, so an alert failure never blocks request expiry or poll cleanup.
        self.maintain_alerts(at).await
```

Append inside the `impl Store` of `services/mobile/relay/src/store/candidates.rs`:

```rust
    /// Elected maintenance: expire old candidates and evaluate due ones in one
    /// transaction, which keeps the election for the whole bounded pass.
    pub(super) async fn maintain_alerts(&self, at: f64) -> Result<()> {
        let batch = self.db.cfg.maintenance_batch;
        let mut t = self.db.begin().await?;
        if !Self::elected(&mut t, 735628114).await? {
            return Ok(());
        }
        t.exec("DELETE FROM alert_candidates WHERE id IN (SELECT id FROM alert_candidates WHERE created<=? ORDER BY created,id LIMIT ?)",args![at-alerts::CANDIDATE_TTL,batch]).await?;
        let due: Vec<String> = t.all("SELECT DISTINCT workspace_id FROM (SELECT workspace_id FROM alert_candidates WHERE next_at<=? ORDER BY next_at,id LIMIT ?) due ORDER BY workspace_id LIMIT 32",args![at,batch]).await?
            .iter()
            .map(|r| s(r, "workspace_id").to_owned())
            .collect();
        let mut plans = Vec::with_capacity(due.len());
        for w in &due {
            plans.push(self.alert_plan(&mut t, w, None, Some(at), at).await?);
        }
        let deliveries: usize = plans.iter().map(AlertPlan::deliveries).sum();
        if deliveries > 0 {
            let spaces: Vec<&str> = due.iter().map(String::as_str).collect();
            self.event_locks(&mut t, &spaces, deliveries).await?;
        }
        let mut topics = vec![];
        for (w, plan) in due.iter().zip(plans) {
            if self.apply_alerts(&mut t, w, plan).await? {
                let topic = format!("workspace:{w}");
                t.notify(&topic).await?;
                topics.push(topic);
            }
        }
        t.commit().await?;
        for topic in topics {
            self.db.signal(topic);
        }
        Ok(())
    }
```

- [ ] **Step 4: Document the relay behavior**

In `services/mobile/relay/README.md` "Boundaries", replace the `store/snapshots.rs` bullet with:

```markdown
- `store/snapshots.rs` separates liveness and event-free snapshot changes from
  shared payload accounting. Event publication and wake jobs have transactional
  caps. Lock order is global event accounting, payload shard when needed,
  workspace, credential, then route/request; multi-workspace locks are sorted.
  Heartbeats may carry `presence` beside the snapshot. The relay remembers each
  machine's latest presence with its receipt time without rewriting an
  unchanged snapshot; a heartbeat without presence keeps the remembered value.
  The gateway snapshot's `preferences.mobile_delivery` is stored with it.
- `alerts.rs` is the pure presence rule: preference parsing, owner away time
  over every remembered presence, and candidate resolution from
  `attention_signature`, `reply_id`, shared read marks and native need.
  Missing or malformed information means away.
- `store/candidates.rs` holds at most one alert candidate per computer, session
  and kind (1,000 per workspace, 24-hour expiry). Ordinary events refresh phone
  state only. Changed snapshots, new presence and an elected maintenance pass
  (at most 32 workspaces, rechecking waiting candidates within 15 seconds)
  evaluate candidates from unlocked reads. Deleting the candidate row in the
  transaction that appends the `alert` event and coalesces device wakes is the
  cross-worker single-delivery claim; candidate rows are locked in id order
  after workspace locks.
```

In "Upgrade and operation", append this paragraph after the one starting "An existing PostgreSQL installation needs a runtime replacement":

```markdown
Presence alerts add the `alert_candidates` table and nullable columns
(`nodes.presence_at`, `presence_until`, `mobile_delivery`,
`computer_routes.presence_at`, `presence_until`, `events.alert_kind`); the
schema version stays 1 and the Python reference ignores them. Deploy the relay
after `hgs`, the desktop, the connector and Android. Replace all PostgreSQL
workers together: an older worker would wake phones for every event, reject the
`presence` field and serve `alert` rows without `alert_kind`. The offline
SQLite-to-PostgreSQL migration does not carry candidates or `alert_kind`;
phones ignore such migrated alert rows.
```

- [ ] **Step 5: Run the tests to verify they pass**

Locally: `cd services/mobile/relay && cargo +1.85.0 fmt && cargo +1.85.0 fmt --check && cargo +1.85.0 clippy --locked --all-targets -- -D warnings`.
Sync to arch; `ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts/services/mobile/relay && cargo +1.85.0 test --locked'`; start the disposable PostgreSQL and run `ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts/services/mobile/relay && ZERUS_RELAY_TEST_DATABASE_URL=postgresql://postgres@127.0.0.1:55432/postgres cargo +1.85.0 test --locked --test contracts postgres_cross_worker -- --ignored'`; stop the container.
Expected: all PASS, including `postgres_cross_worker_contract` and `postgres_cross_worker_single_alert`.

- [ ] **Step 6: Commit**

```bash
git add services/mobile/relay/src/store/candidates.rs services/mobile/relay/src/store/background.rs services/mobile/relay/tests/contracts.rs services/mobile/relay/README.md
git commit -F - <<'EOF'
Expire, bound and deliver due relay alert candidates in maintenance

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 5: Android alert-only notifications

Ships before the relay (D3); alert-only mode stays off until a relay advertises `presence_alerts`.

**Files:**
- Modify: `mobile/android/app/src/main/java/app/zerus/mobile/NotificationPolicy.kt:26,35-38,61-77,79-155,174-205`
- Modify: `mobile/android/app/src/main/java/app/zerus/mobile/NotificationCatalog.kt:55-67`
- Modify: `mobile/android/app/src/main/java/app/zerus/mobile/Notifications.kt:103-144`
- Modify: `mobile/android/app/src/main/java/app/zerus/mobile/RelayApi.kt:69`
- Modify: `mobile/android/README.md:277-311`
- Test: `mobile/android/app/src/test/java/app/zerus/mobile/NotificationPolicyTest.kt`, `NotificationCatalogTest.kt`

**Interfaces:**
- Consumes: relay `GET /v1/capabilities` `features` containing `presence_alerts`; `alert` event JSON (Task 3).
- Produces:
  - `data class NotificationEvent(..., val alertKind: String? = null)` with `val signal: String` (the announced transition: `alert_kind` for alert rows, `kind` otherwise) and `val isAlert: Boolean`.
  - `NotificationState.presenceAlerts: Boolean = false` (persisted as `presence_alerts`); pending rows persist `alert_kind`.
  - `NotificationPolicy.plan(..., active: Set<String> = emptySet(), alertsOnly: Boolean = false)`.
  - `NotificationCatalogParser.presenceAlerts(capabilities: JSONObject): Boolean`; `RelayApi.capabilities(connection: Connection): JSONObject`.

- [ ] **Step 1: Write the failing tests**

In `NotificationPolicyTest.kt`, add helpers after `roundtrip`:

```kotlin
    private fun alert(id: Long = 2, name: String = "review", kind: String = "attention", at: Double = now) =
        NotificationEvent(id, "computer", name, "alert", at, kind)
    private fun alertPlan(state: NotificationState, value: NotificationCurrent, valueEvent: NotificationEvent?,
        prefs: NotificationPreferences = preferences, at: Double = now) =
        NotificationPolicy.plan(state, listOf(value), valueEvent?.let { listOf(value.slot to it) }.orEmpty(), prefs, at, alertsOnly = true)
```

and these tests before the final closing brace:

```kotlin
    @Test fun presenceAlertsPostOnlyForMatchingAlertRows() {
        val changed = current(identity = "question-2")
        val waiting = alertPlan(ready(), changed, event())
        assertTrue(waiting.desired.isEmpty())
        assertTrue(waiting.retained.isEmpty())
        assertTrue(alertPlan(waiting.state, changed, alert(kind = "error")).desired.isEmpty())
        val alerted = alertPlan(waiting.state, changed, alert(3))
        assertEquals(SessionAlertKind.Input, alerted.desired.single().kind)
        assertTrue(alerted.desired.single().fresh)
    }

    @Test fun presenceAlertsKeepPostedCardAndCancelReplacedNeedWithoutAlert() {
        val changed = current(identity = "question-2")
        val alerted = alertPlan(ready(), changed, alert())
        val posted = roundtrip(NotificationPolicy.reserve(alerted.state, alerted.desired.single(), true, 10_000))
        val refresh = alertPlan(posted, changed, event(3), at = now + 1)
        assertEquals(setOf(changed.slot.key), refresh.retained)
        assertFalse(refresh.desired.single().fresh)
        val replaced = alertPlan(refresh.state, changed.copy(identity = "question-3"), event(4), at = now + 2)
        assertTrue(replaced.desired.isEmpty())
        assertTrue(replaced.retained.isEmpty())
    }

    @Test fun presenceAlertsAnnounceCompletionOnlyFromCompletedAlert() {
        val idle = current(kind = null, idle = true)
        val enabled = preferences.copy(finished = true)
        assertTrue(alertPlan(ready(), idle, event(kind = "completed"), enabled).desired.isEmpty())
        assertEquals(SessionAlertKind.Finished, alertPlan(ready(), idle, alert(kind = "completed"), enabled).desired.single().kind)
    }

    @Test fun withoutPresenceAlertsAlertRowsNeitherAnnounceNorHideCompletion() {
        val idle = current(kind = null, idle = true)
        val enabled = preferences.copy(finished = true)
        val both = listOf(idle.slot to event(1, kind = "completed"), idle.slot to alert(2, kind = "completed"))
        assertEquals(SessionAlertKind.Finished, NotificationPolicy.plan(ready(), listOf(idle), both, enabled, now).desired.single().kind)
        val onlyAlert = listOf(idle.slot to alert(kind = "completed"))
        assertTrue(NotificationPolicy.plan(ready(), listOf(idle), onlyAlert, enabled, now).desired.isEmpty())
        val input = current(identity = "question-2")
        assertEquals(SessionAlertKind.Input, plan(ready(), input, alert()).desired.single().kind)
    }

    @Test fun alertRowsSurviveLaterRefreshRowsAndCodecRoundtrip() {
        val page = NotificationPolicy.ingest(NotificationState(bootstrap = false), listOf(event(1), alert(2), event(3, kind = "completed")))
        val state = roundtrip(page.state.copy(presenceAlerts = true))
        assertEquals(listOf(2L, 3L), state.pending.map { it.id }.sorted())
        assertEquals("attention", state.pending.single { it.kind == "alert" }.alertKind)
        assertNull(state.pending.single { it.kind == "completed" }.alertKind)
        assertTrue(state.presenceAlerts)
    }
```

In `NotificationCatalogTest.kt`, add:

```kotlin
    @Test fun alertRowsCarryOnlyKnownKindsAndCapabilitiesEnableAlertMode() {
        fun row(id: Int, kind: String, alertKind: String? = null) = JSONObject().put("id", id).put("computer_id", computer)
            .put("session", "review").put("kind", kind).put("created_at", 1_000.0).apply { alertKind?.let { put("alert_kind", it) } }
        val events = NotificationCatalogParser.events(JSONObject().put("events", JSONArray(listOf(
            row(1, "attention"), row(2, "alert", "completed"), row(3, "alert", "future"), row(4, "attention", "error")))))
        assertEquals(listOf(null, "completed", null, null), events.map { it.alertKind })
        assertTrue(NotificationCatalogParser.presenceAlerts(JSONObject().put("features", JSONArray(listOf("inspect_after", "presence_alerts")))))
        assertFalse(NotificationCatalogParser.presenceAlerts(JSONObject().put("features", JSONArray(listOf("inspect_after")))))
        assertFalse(NotificationCatalogParser.presenceAlerts(JSONObject()))
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Sync to arch, then:
`ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts/mobile/android && JAVA_HOME=$HOME/.cache/zerus-toolchains/jdk-17.0.20.1+1 ANDROID_HOME=$HOME/Android/Sdk ./gradlew testDebugUnitTest --tests app.zerus.mobile.NotificationPolicyTest --tests app.zerus.mobile.NotificationCatalogTest'`
Expected: FAIL to compile (`Too many arguments for NotificationEvent`, `No parameter with name 'alertsOnly'`, unresolved `presenceAlerts`).

- [ ] **Step 3: Extend the policy types, ingest, plan and codec**

In `NotificationPolicy.kt`, line 26 becomes:

```kotlin
data class NotificationEvent(val id: Long, val computer: String, val session: String, val kind: String, val at: Double,
    val alertKind: String? = null) {
    /** The transition this row announces; relay alert rows carry it in alert_kind. */
    val signal get() = if (kind == "alert") alertKind.orEmpty() else kind
    val isAlert get() = kind == "alert" && alertKind != null
}
```

In `NotificationState` (line 38), replace the declaration's last parameter `val pendingRestoreKinds: Set<SessionAlertKind> = emptySet())` with:

```kotlin
val pendingRestoreKinds: Set<SessionAlertKind> = emptySet(), val presenceAlerts: Boolean = false)
```

Above `ingest` (line 61) add

```kotlin
    // Alert rows keep their own pending entry so a later refresh row cannot hide them.
    private fun pendingKey(event: NotificationEvent) = JSONArray(if (event.kind == "alert")
        listOf(event.computer, event.session, "alert") else listOf(event.computer, event.session)).toString()
```

and in `ingest` replace `state.pending.associateByTo(linkedMapOf()) { JSONArray(listOf(it.computer, it.session)).toString() }` with `state.pending.associateByTo(linkedMapOf()) { pendingKey(it) }`, and `val key = JSONArray(listOf(event.computer, event.session)).toString()` with `val key = pendingKey(event)`.

In `plan` (lines 79-155): the signature gains `, alertsOnly: Boolean = false` after `active: Set<String> = emptySet()`. Line 82 becomes:

```kotlin
        // With presence alerts only relay alert rows announce; other rows refresh state.
        val announcing = events.filter { (_, event) -> if (alertsOnly) event.isAlert else event.kind != "alert" }
        val candidates = announcing.groupBy { it.first.key }.mapValues { (_, rows) -> rows.maxBy { it.second.id }.second }
```

On line 105 replace `event?.kind == "completed"` with `event?.signal == "completed"`. Directly after the closing brace of `if (kind == null && session.idle && !state.bootstrap) { ... }` (line 119) insert:

```kotlin
            if (alertsOnly && (kind == SessionAlertKind.Input || kind == SessionAlertKind.Error) &&
                previous?.postedFingerprint != identity) {
                val announced = if (kind == SessionAlertKind.Input) "attention" else "error"
                // Observed while the owner was at a computer: no card until the relay alerts.
                if (event?.signal != announced) { kind = null; identity = "" }
            }
```

In `NotificationStateCodec.encode`, the pending row map becomes

```kotlin
        .put("pending", JSONArray(state.pending.map { JSONObject().put("id", it.id).put("computer", it.computer)
            .put("session", it.session).put("kind", it.kind).put("at", it.at).put("alert_kind", it.alertKind) }))
```

and the chain ends with `.put("restore_kinds", JSONArray(state.pendingRestoreKinds.map { it.name })).put("presence_alerts", state.presenceAlerts)`. In `decode`, the pending mapping becomes `NotificationEvent(it.getLong("id"), it.getString("computer"), it.getString("session"), it.getString("kind"), it.getDouble("at"), it.string("alert_kind").ifEmpty { null })`, and the `NotificationState(...)` call gains a last argument `, raw.optBoolean("presence_alerts")` after the `restore_kinds` expression.

- [ ] **Step 4: Parse alert rows and the capability; use them in sync**

In `NotificationCatalog.kt`, inside `object NotificationCatalogParser` add

```kotlin
    private val alertKinds = setOf("attention", "error", "completed")
    /** Relay capability that makes alert rows the only source of new cards. */
    fun presenceAlerts(capabilities: JSONObject) = capabilities.optJSONArray("features")?.let { features ->
        (0 until features.length()).any { features.optString(it) == "presence_alerts" } } == true
```

and in `events` replace `NotificationEvent(id, computer, session, row.getString("kind"), at)` with:

```kotlin
            val kind = row.getString("kind")
            // Unknown alert kinds stay plain refresh rows instead of failing the page.
            NotificationEvent(id, computer, session, kind, at, row.string("alert_kind").takeIf { kind == "alert" && it in alertKinds })
```

In `RelayApi.kt` after line 69 add:

```kotlin
    suspend fun capabilities(connection: Connection) = call(connection.url, connection.token, "/v1/capabilities")
```

In `Notifications.kt` `SessionNotifications`, add before `sync`:

```kotlin
    /** null keeps the last known mode when optional capability discovery fails. */
    private suspend fun presenceAlerts(connection: Connection): Boolean? = try {
        NotificationCatalogParser.presenceAlerts(api.capabilities(connection))
    } catch (e: kotlinx.coroutines.CancellationException) { throw e }
    catch (e: RelayException) { if (e.status in listOf(401, 403)) throw e else null }
    catch (_: Exception) { null }
```

In `sync`, declare `var capabilitiesChecked = false` next to `var operations = 0`; at the start of the `if (needsCatalog && ...) {` block, before `val raw = api.computers(...)`, insert:

```kotlin
                    if (!capabilitiesChecked) {
                        capabilitiesChecked = true
                        operations++
                        presenceAlerts(connection)?.let { state = state.copy(presenceAlerts = it) }
                    }
```

and change the `NotificationPolicy.plan(...)` call's last line to `store.notificationPreferences(), System.currentTimeMillis() / 1000.0, active, alertsOnly = state.presenceAlerts)`.

- [ ] **Step 5: Document the phone behavior**

In `mobile/android/README.md` "Notifications", insert after the paragraph ending "A resolved input request can remain until the next refresh when the relay emits no new event.":

```markdown
When the relay advertises `presence_alerts`, a new card needs a relay `alert`
event for the same workspace, computer and session whose kind matches the
session's current need: attention for input and approvals, error, or completed
for an opted-in finished turn. The relay sends it only while the item is unread
and unresolved, and only when the owner is away from every Zerus desktop or
chose immediate delivery. Ordinary events still refresh and withdraw cards; a
card already shown for the same need stays without a new alert. Relays without
the feature keep the behavior above, so this build ships before the relay.
```

- [ ] **Step 6: Run the tests to verify they pass**

Sync to arch, then:
`ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts/mobile/android && export JAVA_HOME=$HOME/.cache/zerus-toolchains/jdk-17.0.20.1+1 ANDROID_HOME=$HOME/Android/Sdk && ./gradlew testDebugUnitTest lintDebug && if [ -f app/google-services.json ]; then ./gradlew -PzerusFirebase=true testDebugUnitTest lintDebug; else echo "Firebase variant unavailable: no private google-services.json"; fi'`
Expected: BUILD SUCCESSFUL; all unit tests, including the six new ones, PASS; lint reports no new errors. Record whether the Firebase variant ran.

- [ ] **Step 7: Commit**

```bash
git add mobile/android/app/src/main/java/app/zerus/mobile/NotificationPolicy.kt mobile/android/app/src/main/java/app/zerus/mobile/NotificationCatalog.kt mobile/android/app/src/main/java/app/zerus/mobile/Notifications.kt mobile/android/app/src/main/java/app/zerus/mobile/RelayApi.kt mobile/android/app/src/test/java/app/zerus/mobile/NotificationPolicyTest.kt mobile/android/app/src/test/java/app/zerus/mobile/NotificationCatalogTest.kt mobile/android/README.md
git commit -F - <<'EOF'
Create Android session cards only from relay alert events

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 6: End-to-end presence scenario and architecture documentation

**Files:**
- Create: `services/mobile/tests/test_presence_end_to_end.py`
- Modify: `services/mobile/tests/fixture_hgs.py:255-258`
- Modify: `docs/mobile-architecture.md:614,622,662-667`

**Interfaces:**
- Consumes: everything above; `fixture_hgs.initial()`, `fixture_hgs.persist(path, record)`; `relay_fixture.start_relay`; `Connector` (Task 2 strips and forwards presence).
- Produces: `fixture-config.json` keys `desktop_presence` and `preferences`, forwarded verbatim at the top level of the fixture's `ls --json --local` output.

- [ ] **Step 1: Write the failing end-to-end test**

Create `services/mobile/tests/test_presence_end_to_end.py`:

```python
"""Presence-aware alerts across the real connector, the Rust relay and the phone API."""
import asyncio
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from aiohttp import ClientSession

from zerus_mobile.connector import Connector
from zerus_mobile.server import Config
from zerus_mobile.store import Store
from relay_fixture import start_relay

spec = importlib.util.spec_from_file_location("fixture_presence", Path(__file__).with_name("fixture_hgs.py"))
fixture_hgs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture_hgs)

FIRST, SECOND = "1" * 64, "2" * 64


@unittest.skipUnless(os.environ.get("ZERUS_RELAY_BINARY"), "presence alerts are evaluated by the Rust relay")
class PresenceEndToEndTests(unittest.IsolatedAsyncioTestCase):
    async def test_active_desktop_holds_read_drops_and_lock_delivers(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture = Path(__file__).with_name("fixture_hgs.py")
            fixture.chmod(0o755)
            store = Store(root / "relay" / "state.sqlite3")
            workspace = store.workspace("Integration")
            node = store.node(workspace, "Fixture computer")
            invitation = store.invite(workspace)
            runner, url = await start_relay(store, Config(background=False))

            def presence(idle, locked=None):
                fixture_hgs.persist(root / "fixture-config.json", {"desktop_presence": {
                    "idle_seconds": idle, "locked": locked is not None, "locked_seconds": locked}})

            def session(**fields):
                fixture_hgs.persist(root / "session.json", {**fixture_hgs.initial(), **fields})

            def candidates():
                return store.db.execute("SELECT count(*) FROM alert_candidates").fetchone()[0]

            presence(1.0)
            session()
            connector = Connector({"server_url": url, "node_token": node["node_token"],
                "hgs_path": str(fixture), "state_dir": str(root / "journal"), "poll_interval": 1},
                allow_insecure_localhost=True)
            with patch.dict(os.environ, {"ZERUS_MOBILE_FIXTURE_DIR": directory}):
                task = asyncio.create_task(connector.run())
                try:
                    async with ClientSession() as client:
                        async with client.post(url + "/v1/pair", json={"code": invitation["pair_code"], "device_name": "Test phone"}) as response:
                            self.assertEqual(response.status, 200)
                            headers = {"Authorization": "Bearer " + (await response.json())["device_token"]}

                        async def get(path):
                            async with client.get(url + path, headers=headers) as response:
                                self.assertEqual(response.status, 200)
                                return await response.json()

                        async def kinds(kind):
                            return [e for e in (await get("/v1/events?after=0"))["events"] if e["kind"] == kind]

                        async def until(check):
                            for _ in range(200):
                                if await check():
                                    return
                                await asyncio.sleep(0.05)
                            self.fail("condition was not reached")

                        async def online():
                            computers = (await get("/v1/computers"))["computers"]
                            return bool(computers) and computers[0]["online"]

                        await until(online)
                        # Active at the desktop: the question is an event but not an alert.
                        question = {"phase": "input", "activity": "busy", "attention_id": "q1", "attention_signature": FIRST}
                        session(**question)
                        await until(lambda: kinds("attention"))
                        await asyncio.sleep(2.5)
                        self.assertEqual(await kinds("alert"), [])
                        self.assertEqual(candidates(), 1)
                        # Read on the desktop: the held candidate is dropped and never alerts.
                        read = {"conversation_id": "fixture-conversation", "reply_id": None, "attention_signature": FIRST, "at": 1.0}
                        session(**question, read=read)

                        async def dropped():
                            return candidates() == 0

                        await until(dropped)
                        presence(600.0)
                        await asyncio.sleep(2.5)
                        self.assertEqual(await kinds("alert"), [])
                        # A new question waits while active and alerts once the screen locks.
                        presence(1.0)
                        session(**{**question, "attention_id": "q2", "attention_signature": SECOND}, read=read)

                        async def second_question():
                            return len(await kinds("attention")) == 2

                        await until(second_question)
                        await asyncio.sleep(2.5)
                        self.assertEqual(await kinds("alert"), [])
                        presence(130.0, locked=130.0)
                        await until(lambda: kinds("alert"))
                        alert = (await kinds("alert"))[0]
                        self.assertEqual(set(alert), {"id", "computer_id", "session", "kind", "alert_kind", "created_at"})
                        self.assertEqual((alert["alert_kind"], alert["session"], alert["computer_id"]),
                                         ("attention", fixture_hgs.initial()["name"], node["node_id"]))
                        self.assertEqual(len(await kinds("alert")), 1)
                finally:
                    task.cancel()
                    await asyncio.gather(task, return_exceptions=True)
                    connector.journal.close()
                    await runner.cleanup()
                    store.close()


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run the test to verify it fails**

Sync to arch (the relay binary from Task 4 must be built; the package reinstalled after Task 2) and run:
`ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts && export ZERUS_RELAY_BINARY="$PWD/services/mobile/relay/target/debug/zerus-relay" PATH="$PWD/services/mobile/relay/target/debug:$PATH" && ~/build/zerus-presence-venv/bin/python -m unittest discover -s services/mobile/tests -p test_presence_end_to_end.py -v'`
Expected: FAIL at the first `assertEqual(await kinds("alert"), [])`: the fixture does not list presence yet, so no presence ever reaches the relay, the owner counts as away and the first question alerts immediately.

- [ ] **Step 3: Forward synthetic presence from the fixture**

In `services/mobile/tests/fixture_hgs.py`, directly after `result = {"host": "Integration fixture", "ok": True, "projects": {}, "sessions": summaries}` in the `ls` branch, add:

```python
        for key in ("desktop_presence", "preferences"):  # Phase 2 listing fields, synthetic here.
            if isinstance(fixture_config.get(key), dict):
                result[key] = fixture_config[key]
```

Sync to arch and run the Step 2 command again.
Expected: PASS in roughly 15 seconds (the connector strips `desktop_presence` and sends it beside the snapshot).

- [ ] **Step 4: Document the protocol and the delivery rule**

In `docs/mobile-architecture.md`, the protocol table row for capabilities (line 614) becomes:

```markdown
| `GET /v1/capabilities` | Device | Protocol version, operations, features (inspect extensions, `presence_alerts`), push providers and attachment limits |
```

and the heartbeat row (line 622) becomes:

```markdown
| `POST /v1/node/heartbeat` | Computer | `{snapshot, presence?}` using local `hgs ls --json --local` output; its `desktop_presence` travels as `presence` beside the snapshot |
```

Replace the first paragraph of "## Notifications" (lines 664-667) with:

```markdown
The relay detects attention, failure and completion transitions from computer
snapshots, including failed recovery and counted native questions, so every
condition a phone notifies on has an event. An initial baseline is silent. Each
transition is stored as an ordinary event, which refreshes phone state, and as
an alert candidate. Push payloads contain only generic wake information, never
conversation text, project names or credentials. Opening a notification
refreshes the authenticated session state.

### Presence-aware delivery

Relays advertising the `presence_alerts` feature hold candidates while the
owner works at a Zerus desktop. The connector sends each machine's
`desktop_presence` (`idle_seconds`, `locked`, `locked_seconds`) as `presence`
beside the snapshot, so changing idle time never rewrites snapshots; the relay
remembers the latest presence per machine with its own receipt time. The
gateway snapshot carries the shared `preferences.mobile_delivery` choice:
`immediate`, `away` (the default), or `away_5`, `away_10`, `away_15` and
`away_30`. The owner is active while a machine whose presence is at most 120
seconds old is unlocked with less than 120 seconds of input idle time. Away time
counts from the end of the latest activity over every remembered machine, or
the earlier lock time; a machine that stopped reporting (lid closed, asleep)
cannot keep the owner active but still marks when activity ended. With no
presence ever received the owner is away, so connectors without these fields
behave as before.

A candidate is dropped when its session disappears, is archived, or changes run
or conversation; when its attention signature changes or is marked read; for a
candidate without a signature, only when the native need is resolved; or, for a
completed turn, when that reply is read, a newer reply exists or the session is
busy again. Otherwise it is delivered immediately, as soon as the owner is away,
or after the chosen number of away minutes. Changed snapshots, new presence and
maintenance re-evaluate candidates at least every 15 seconds; candidates expire
after 24 hours and are capped at 1,000 per workspace.

A delivered candidate appends `{id, computer_id, session, kind: "alert",
alert_kind, created_at}` to `GET /v1/events`, with `alert_kind` `attention`,
`error` or `completed`, and coalesces one generic wake per registered phone.
Ordinary events no longer wake phones themselves. Phones that know the feature
create cards only from alert events.

Roll out `hgs`, the desktop and the connector first, then Android, then the
relay. The connector retries without `presence` when an older relay rejects it,
and Android alert-only mode starts only when the relay advertises the feature.
Android builds released before this change keep only the newest event per
session in a batch, so an alert following a completion can hide their opt-in
completion card.
```

- [ ] **Step 5: Final verification**

From the repository root locally: `python3 scripts/ci/check-source.py` and, in `services/mobile/relay`, `cargo +1.85.0 fmt --check && cargo +1.85.0 clippy --locked --all-targets -- -D warnings && cargo +1.85.0 test --locked --test alerts`.
Sync to arch, then run the relay suite (`cargo +1.85.0 test --locked` and `cargo +1.85.0 build --locked`), the PostgreSQL contracts with the disposable container, and:

```sh
ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts && export ZERUS_RELAY_BINARY="$PWD/services/mobile/relay/target/debug/zerus-relay" PATH="$PWD/services/mobile/relay/target/debug:$PATH" && for p in test_rust_relay.py "test*end_to_end.py" test_peer_migration.py; do ~/build/zerus-presence-venv/bin/python -m unittest discover -s services/mobile/tests -p "$p" -v || exit 1; done'
ssh -o BatchMode=yes arch 'cd ~/build/zerus-presence-alerts && ~/build/zerus-presence-venv/bin/python -m unittest discover -s services/mobile/tests -v'
```

and the Android command from Task 5 Step 6.
Expected: every suite PASS; the reference-only run reports `test_presence_end_to_end` and `RustHttpContracts` as skipped and exercises the connector's `presence` fallback against the strict Python reference. Record any unavailable check (Firebase variant, Docker) explicitly.

- [ ] **Step 6: Commit**

```bash
git add services/mobile/tests/fixture_hgs.py services/mobile/tests/test_presence_end_to_end.py docs/mobile-architecture.md
git commit -F - <<'EOF'
Verify presence-aware alerts end to end and document delivery

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```
