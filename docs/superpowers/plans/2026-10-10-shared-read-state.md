# Shared Read State Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Phase 1 of presence-aware mobile notifications: one read state per session conversation, stored by `hgs` on the session's host and shared by every desktop and phone.

**Architecture:** `hgs` computes an opaque `attention_signature` for each listed session, keeps read marks in `read-marks.json` in its state directory, and accepts forward-only marks through `hgs [@host] read <session> --json`. The desktop applies marks optimistically, queues them in `QSettings`, writes them through `HgsClient` with retries, and migrates its old local marks once per host. The relay accepts the additive API v1 operation `mark_read`; the connector maps it to `hgs read` on the owning computer (local or guarded peer route); Android sends it after the phone has shown the current reply and shows host marks as read.

**Tech Stack:** Rust 2021 (CLI and relay, minimum Rust 1.85), Python 3.11+ asyncio/aiohttp (connector and API v1 oracle), C++17 Qt 6 Widgets + Qt Test (desktop), Kotlin + JUnit/MockWebServer (Android).

**Spec:** `docs/superpowers/specs/2026-10-10-mobile-presence-notifications-design.md` — read its "Sources of truth → Read marks", "Desktop", "Phone" and "Failure handling" sections before Task 1. Tracker: Beads epic `zerus-rk5b`. This plan covers implementation phase (1) only: no presence, no delivery preference, no relay `alert` events.

## Global Constraints

Fixed shared contracts (copied verbatim; do not change them):

- **C1. hgs read marks.** Storage: host-local hgs state next to session state; one record per (session, conversation_id): `{"conversation_id": str, "reply_id": str|null, "attention_signature": str|null, "at": float epoch seconds}`.
- C1 CLI: `hgs [@host] read <session> --json`; stdin `{"run_id": str, "conversation_id": str, "reply_id": str|null, "attention_signature": str|null}`. Each non-null field is applied only if it equals the session's current value and run_id/conversation_id match the live session; otherwise that field is reported stale and nothing changes for it. stdout: `{"ok": true, "applied": {"reply_id": bool, "attention_signature": bool}, "read": <record>|null}`. Unknown session: `{"ok": false, "error": "not_found"}` with exit code 1.
- C1 listing: `hgs ls --json` adds per session: `"attention_signature"`: lowercase hex SHA-256 string or null (null when nothing needs attention; digest over run_id, conversation_id, phase when approval/input/error, attention_id, question request identities, pending mobile_attention entries and actionable subagent requests; must not change on status-only updates), and `"read"`: the record or null.
- **C4 (mark_read).** Relay operation `"mark_read"`: session-scoped request (envelope session = session name, computer_id = owning computer); payload `{"run_id": str, "conversation_id": str, "reply_id": str|null, "attention_signature": str|null}`; connector executes C1 `hgs read` on that computer (local or via the existing peer route); request result = the hgs stdout object.

Spec rules (verbatim):

- "A field is applied only when it equals the session's current value; otherwise the command reports `stale` and changes nothing. Marks therefore only move forward and an old mark never hides a newer reply or question." (`applied.<field> == false` is the C1 spelling of "stale".)
- "A session is unread when `reply_id` differs from `read.reply_id`, and its attention is unacknowledged when `attention_signature` is non-null and differs from `read.attention_signature`."
- Desktop: "The UI applies the mark immediately; failed writes stay queued and retry without reverting the UI." and "One-time migration: a new GUI sends its existing `QSettings` marks to the sessions' hosts and records that migration finished. Old keys are kept."
- Phone: "Viewing a session's latest reply sends operation `mark_read` with exact computer, session, run, conversation, `reply_id` and `attention_signature`. It is idempotent; stale marks are ignored by `hgs`."
- "API v1 changes are additive". Non-goal: "changing desktop notification timing or sounds".

Project constraints:

- Keep the `hgs` CLI, `HGS_*` variables, state/config paths, application and service identifiers compatible. A host whose `hgs` predates `hgs read` must keep working: the desktop keeps its `QSettings` read state for it and never runs `read` there; the connector does not advertise `mark_read` for it.
- Keep API v1 additive and durable request identities intact: the connector journal claims each `request_id` once and replays its receipt; a request ID is never reused with a different body. `mark_read` is idempotent and forward-only, so a phone may send a newer mark under a new request ID; it never resubmits an existing one.
- Never restart or kill native agents, tmux servers or DeepSeek hosts in tests or validation. CLI tests use the private `HOME`, `HGS_STATE_DIR` and tmux socket of `tests/test_input.py` (`InputTransport`). Do not hot-reload a DeepSeek bridge.
- `tray/build/hgs-tray` may be the running GUI: build only test targets in `tray/build` (`--target test_…`), never `hgs-tray` or `all` there. The full Qt suite runs in `.ci-build/gui` through `scripts/ci/gui.sh`.
- Rust 1.85 compatibility for both crates. Relay: `cargo +1.85.0 fmt --check`, `cargo +1.85.0 clippy --locked --all-targets -- -D warnings`, `cargo +1.85.0 test --locked` in `services/mobile/relay`. CLI: `cargo +1.85.0 test --locked` at the repository root.
- Relay contract tests fail on the macOS development machine for an unrelated SQLite fixture reason; run Linux validation on the `arch` host (`ssh -o BatchMode=yes arch`). Rust HTTP contracts run with `ZERUS_RELAY_BINARY=<relay target/debug/zerus-relay>` via `python -m unittest discover -s services/mobile/tests -p test_rust_relay.py`.
- Android on `arch`: `JAVA_HOME=$HOME/.cache/zerus-toolchains/jdk-17.0.20.1+1 ANDROID_HOME=$HOME/Android/Sdk ./gradlew testDebugUnitTest lintDebug` (plain) and again with `-PzerusFirebase=true` (needs the private `app/google-services.json` from `~/.config/hgs/mobile/firebase/`, copied only into the disposable checkout and never staged).
- Repository text is English, including new code comments in files whose older comments are Russian. Match surrounding code style and comment density. Use synthetic fixture names and reserved example domains only.
- Other sessions may edit this worktree: run `git status --short <file>` before editing, use the Edit tool, and stage or commit only your own paths (`git commit --only -- <paths>`). Commit steps run only when the owner has authorized commits for this execution; otherwise skip them and leave the change uncommitted for review. Never push. Track progress in Beads (`bd prime`; epic `zerus-rk5b`) and run `bd export -o .beads/issues.jsonl` before staging tracker changes.

## Review Focus

- Mixed-version swarm (a peer or connector with an older `hgs`): the desktop must keep that host's per-device read state and never run `read` there, and the connector must not advertise or execute `mark_read` — tests in Task 6 (`olderHostsKeepLocalReadStateAndQueueNothing`) and Task 4 (`test_old_native_without_read_abi_is_not_advertised_or_executed`).
- A sleeping or unreachable peer while marking: the row stays read, the write retries with backoff, a delivered mark is not rewritten every poll, and migration of many marks never opens more than four SSH writes at once — tests in Task 7.
- The phone showing a stale snapshot or not scrolled to the newest message: no `mark_read` unless a fresh head was fetched for exactly this reply and the phone has read through it — test in Task 9 (`payloadCarriesExactIdentityOnlyAfterThisPhoneShowedTheReply`).
- Renaming a session after reading it: its read mark must follow the session, as the desktop documentation promises ("survives rename") — test in Task 2 (`test_marks_follow_a_renamed_session`).
- Two failures in the same conversation without a native error identity: the second must not count as already acknowledged — tests in Task 1 (`identity_less_errors_are_distinct_per_turn`, `test_repeated_identity_less_errors_have_distinct_signatures`) and Task 2 (`test_second_identity_less_error_is_unacknowledged`).

## Shared test commands

```bash
# hgs unit tests and one integration module (HGS_* must not leak into fixtures)
cargo +1.85.0 test --locked read_marks
cargo build --locked
bash -c 'for v in $(compgen -e | grep "^HGS_"); do unset "$v"; done; HGS_TEST_BIN="$PWD/target/debug/hgs" PYTHONPATH=tests python3 -m unittest -v test_read_marks'
# Desktop: one test target, then one suite or function
cmake --build tray/build --target test_fleetstate -j8
QT_QPA_PLATFORM=offscreen tray/build/tests/test_fleetstate
ctest --test-dir tray/build -R '^sessionswindow-' -j10 --output-on-failure
# Connector and API v1 oracle (Python 3.11+ venv with services/mobile installed)
python -m unittest discover -s services/mobile/tests -p 'test_mobile_read.py' -v
```

---

### Task 1: `attention_signature` and `read` in `hgs ls --json`

**Files:**
- Create: `src/state/read_marks.rs`
- Modify: `src/state/mod.rs:42-43` (module list), `src/state/mod.rs:191-269` (`merge_snapshot`)
- Modify: `src/state/dsh.rs:551-571` (`normalize`: native question identity), end of file (test module)
- Test: `src/state/read_marks.rs` (unit tests), `tests/test_read_marks.py` (new integration module)

**Interfaces:**
- Consumes: `journal::summary`, `journal::journal_name(&Value) -> &str`, `questions::current(&Value) -> Result<Vec<Value>>`, `storage::{root, process_alive}`.
- Produces (used by Task 2):
  - `pub(super) fn path() -> PathBuf` — `<HGS_STATE_DIR>/read-marks.json`
  - `pub(super) fn load() -> Value` — `{owner: {conversation_id: record}}`, `{}` when missing or invalid
  - `pub(super) fn record<'a>(marks: &'a Value, owner: &str, conversation: &str) -> &'a Value`
  - `pub(super) fn signature(session: &Value, questions: &[Value]) -> Option<String>`
  - `pub(super) fn pending_questions(record: &Value, session: &Value, live_pane: bool) -> Vec<Value>`
  - `pub(super) fn enrich(session: &mut Value, marks: &Value, owner: &str, questions: &[Value])`
  - Owner key = `journal::journal_name(record)` for tracked sessions (stable across `hgs rename`), the session name otherwise.

- [ ] **Step 1: Write the failing Rust unit tests**

Create `src/state/read_marks.rs` with its module header and the test module (the implementation follows in Step 3):

```rust
//! Shared read marks. The host that runs a session is their only writer and
//! source of truth: desktops and phones read them from `hgs ls --json` and
//! write them with `hgs [@host] read`.
use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting() -> Value {
        json!({"name":"codex/project/review","run_id":"run-1","conversation_id":"conversation-1",
            "phase":"input","attention_id":"call-1","process_state":"running","activity":"busy",
            "activity_summary":"Input needed","last_event_at":10.0,"turn_started":5.0,
            "subagent_source":"hooks","subagents":{}})
    }

    fn hex(value: &str) -> bool {
        value.len() == 64 && value.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    }

    #[test]
    fn signature_ignores_status_only_updates() {
        let first = signature(&waiting(), &[]).unwrap();
        assert!(hex(&first));
        let mut status = waiting();
        status["activity_summary"] = json!("Still waiting");
        status["last_event_at"] = json!(99.0);
        status["current_tool"] = json!("Bash");
        status["prompt"] = json!("Another preview");
        status["subagents"] = json!({"worker":{"state":"working","display_state":null,"updated":99.0}});
        assert_eq!(signature(&status, &[]), Some(first));
    }

    #[test]
    fn signature_changes_for_each_new_request() {
        let first = signature(&waiting(), &[]).unwrap();
        let mut next = waiting();
        next["attention_id"] = json!("call-2");
        assert_ne!(signature(&next, &[]).unwrap(), first);
        let question = json!({"question_id":"q-1","question_hash":"h-1"});
        assert_ne!(signature(&waiting(), &[question.clone()]).unwrap(), first);
        let mut child = waiting();
        child["subagents"] = json!({"helper":{"state":"working","display_state":"approval"}});
        assert_ne!(signature(&child, &[]).unwrap(), first);
        for (key, value) in [("run_id", "run-2"), ("conversation_id", "conversation-2"), ("phase", "approval")] {
            let mut changed = waiting();
            changed[key] = json!(value);
            assert_ne!(signature(&changed, &[]).unwrap(), first, "{key}");
        }
    }

    #[test]
    fn nothing_actionable_or_not_running_has_no_signature() {
        let mut working = waiting();
        working["phase"] = json!("working");
        assert_eq!(signature(&working, &[]), None);
        for (key, value) in [("state", "paused"), ("state", "stopped"), ("state", "archived"), ("process_state", "exited")] {
            let mut stopped = waiting();
            stopped[key] = json!(value);
            assert_eq!(signature(&stopped, &[]), None, "{key}={value}");
        }
        // Kimi profile groups carry no per-child request identity.
        let mut profiles = working.clone();
        profiles["subagent_source"] = json!("hook_profiles");
        profiles["subagents"] = json!({"helper":{"state":"input"}});
        assert_eq!(signature(&profiles, &[]), None);
    }

    #[test]
    fn pending_questions_need_attention_until_their_answer_is_submitted() {
        let mut working = waiting();
        working["phase"] = json!("working");
        let question = json!({"question_id":"q-1","question_hash":"h-1"});
        assert!(signature(&working, &[question.clone()]).is_some());
        let mut submitted = question;
        submitted["answer_delivery"] = json!({"status":"submitted"});
        assert_eq!(signature(&working, &[submitted]), None);
    }

    #[test]
    fn identity_less_errors_are_distinct_per_turn() {
        let mut failed = waiting();
        failed["phase"] = json!("error");
        failed["attention_id"] = json!("");
        let first = signature(&failed, &[]).unwrap();
        failed["last_event_at"] = json!(500.0);
        assert_eq!(signature(&failed, &[]).unwrap(), first);
        failed["turn_started"] = json!(6.0);
        assert_ne!(signature(&failed, &[]).unwrap(), first);
    }

    #[test]
    fn records_are_found_by_owner_and_conversation_only() {
        let marks = json!({"codex/project/original":{"conversation-1":{"conversation_id":"conversation-1",
            "reply_id":"7:100.5","attention_signature":null,"at":1.0}}});
        assert_eq!(record(&marks, "codex/project/original", "conversation-1")["reply_id"], "7:100.5");
        assert!(record(&marks, "codex/project/original", "conversation-2").is_null());
        assert!(record(&marks, "codex/project/original", "").is_null());
        let mut session = waiting();
        enrich(&mut session, &marks, "codex/project/original", &[]);
        assert_eq!(session["read"]["reply_id"], "7:100.5");
        assert!(hex(session["attention_signature"].as_str().unwrap()));
        enrich(&mut session, &marks, "codex/project/other-owner", &[]);
        assert!(session["read"].is_null());
    }
}
```

Register the module in `src/state/mod.rs` before `mod recipes;`:

```rust
mod read_marks;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo +1.85.0 test --locked read_marks`
Expected: compile errors `cannot find function `signature` in this scope` (and `record`, `enrich`).

- [ ] **Step 3: Implement the signature and listing helpers**

Insert between `use super::*;` and the test module in `src/state/read_marks.rs`:

```rust
use sha2::{Digest, Sha256};
use std::fs;

const ACTIONABLE: &[&str] = &["approval", "input", "error"];

pub(super) fn path() -> PathBuf {
    root().join("read-marks.json")
}

/// Marks by owner (a tracked session's stable journal name, otherwise its
/// name) and conversation. A missing or unreadable file means no marks.
pub(super) fn load() -> Value {
    fs::read_to_string(path())
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(|value| value["version"] == 1 && value["sessions"].is_object())
        .map(|value| value["sessions"].clone())
        .unwrap_or_else(|| json!({}))
}

pub(super) fn record<'a>(marks: &'a Value, owner: &str, conversation: &str) -> &'a Value {
    static NONE: Value = Value::Null;
    if conversation.is_empty() {
        return &NONE;
    }
    marks.get(owner).and_then(|m| m.get(conversation)).unwrap_or(&NONE)
}

/// Canonical identity of what needs the owner now, or None. Status-only
/// changes (activity text, timestamps, tool progress, finished children)
/// never change it; a new request, question or failed turn always does.
pub(super) fn signature(session: &Value, questions: &[Value]) -> Option<String> {
    let running = ["", "running"].contains(&string(session, "state"))
        && string(session, "process_state") != "exited";
    if !running {
        return None;
    }
    let phase = string(session, "phase");
    let actionable = ACTIONABLE.contains(&phase);
    let attention = if !actionable {
        Value::Null
    } else if !string(session, "attention_id").is_empty() {
        session["attention_id"].clone()
    } else if phase == "error" {
        // A failed turn without a native error identity is still a new event.
        json!(["turn", session["turn_started"]])
    } else {
        json!("")
    };
    // The same identities the mobile connector publishes as `mobile_attention`.
    let mut asked: Vec<Value> = questions
        .iter()
        .filter(|card| card["answer_delivery"]["status"] != "submitted")
        .map(|card| json!([card["question_id"], card["question_hash"]]))
        .collect();
    asked.sort_by_key(Value::to_string);
    asked.dedup();
    let mut children = Vec::new();
    if string(session, "subagent_source") != "hook_profiles" {
        for (id, child) in session["subagents"].as_object().into_iter().flatten() {
            let state = child["display_state"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| string(child, "state"));
            if ["approval", "input", "attention"].contains(&state) {
                let request = child["attention_id"]
                    .as_str()
                    .or_else(|| child["question_id"].as_str())
                    .unwrap_or("");
                children.push(json!([id, state, request]));
            }
        }
    }
    children.sort_by_key(Value::to_string);
    if !actionable && asked.is_empty() && children.is_empty() {
        return None;
    }
    let canonical = json!([1, session["run_id"], session["conversation_id"],
        if actionable { phase } else { "" }, attention, asked, children]);
    Some(format!("{:x}", Sha256::digest(canonical.to_string().as_bytes())))
}

/// Pending native question identities of a live tracked session. Only Codex
/// asks without changing phase; other providers are read only while waiting.
/// Errors contribute nothing: a listing never fails on optional indexes.
pub(super) fn pending_questions(record: &Value, session: &Value, live_pane: bool) -> Vec<Value> {
    if !live_pane
        || (string(record, "agent") != "codex" && !ACTIONABLE.contains(&string(session, "phase")))
        || !process_alive(record)
    {
        return Vec::new();
    }
    questions::current(record).unwrap_or_default()
}

/// Add the C1 fields to one `hgs ls --json` session object.
pub(super) fn enrich(session: &mut Value, marks: &Value, owner: &str, questions: &[Value]) {
    let mark = record(marks, owner, string(session, "conversation_id")).clone();
    let digest = signature(session, questions);
    session["attention_signature"] = json!(digest);
    session["read"] = mark;
}
```

- [ ] **Step 4: Run the unit tests to verify they pass**

Run: `cargo +1.85.0 test --locked read_marks`
Expected: 6 tests PASS. Warnings that `load` and `pending_questions` are unused disappear in Step 8; `path` stays unused until Task 2.

- [ ] **Step 5: Write the failing DeepSeek question identity test**

Append to `src/state/dsh.rs`:

```rust
#[cfg(test)]
mod question_identity_tests {
    use super::*;
    #[test]
    fn pending_native_questions_have_an_exact_attention_identity() {
        let record = json!({"name":"dsh/test/session","conversation_id":"session-one","run_id":"run"});
        let row = json!({"agentAvailable":true,"running":false});
        let first = normalize(&record, &row, &json!({"pendingQuestions":[{"id":"question-1"}]}));
        assert_eq!(first["phase"], "input");
        assert_eq!(first["attention_id"], "dsh_question:question-1");
        let next = normalize(&record, &row, &json!({"pendingQuestions":[{"id":"question-2"}]}));
        assert_ne!(next["attention_id"], first["attention_id"]);
        assert!(normalize(&record, &row, &json!({}))["attention_id"].is_null());
    }
}
```

Run: `cargo +1.85.0 test --locked pending_native_questions_have_an_exact_attention_identity`
Expected: FAIL — `left: Null, right: "dsh_question:question-1"`.

- [ ] **Step 6: Give DeepSeek questions an identity**

In `src/state/dsh.rs`, directly after the `let mut value = json!({...});` statement that ends with `"send_available":...});` (line 570) and before `if record["permissions_pending"] == true {`, insert:

```rust
    if pending {
        // Native question IDs distinguish one request from the next for read
        // marks and notifications; their generation-bound hashes do not.
        let ids: Vec<&str> = inspected["pendingQuestions"]
            .as_array()
            .or(row["pendingQuestions"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|q| q["id"].as_str())
            .collect();
        value["attention_id"] = json!(format!("dsh_question:{}", ids.join("\n")));
    }
```

Run: `cargo +1.85.0 test --locked pending_native_questions_have_an_exact_attention_identity`
Expected: PASS.

- [ ] **Step 7: Write the failing listing integration tests**

Create `tests/test_read_marks.py`:

```python
"""Shared read marks on the session's host: `hgs ls --json` fields and `hgs read`."""
import json
import re
import sqlite3
import subprocess
import unittest

import test_input as fixtures

HGS = fixtures.HGS
HEX = re.compile(r'[0-9a-f]{64}')


class ReadMarks(unittest.TestCase):
    setUp = fixtures.InputTransport.setUp
    tearDown = fixtures.InputTransport.tearDown
    script = fixtures.InputTransport.script
    tmux = fixtures.InputTransport.tmux
    wait_for = fixtures.InputTransport.wait_for
    write_record = fixtures.InputTransport.write_record

    def listing(self):
        result = subprocess.run([str(HGS), 'ls', '--json', '--local'], env=self.env,
                                text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        return next(row for row in json.loads(result.stdout)['sessions'] if row['name'] == self.name)

    def reply(self, at):
        db = sqlite3.connect(self.state / 'events.sqlite3')
        with db:
            db.execute('CREATE TABLE IF NOT EXISTS events (seq INTEGER PRIMARY KEY, name TEXT, conversation TEXT, payload TEXT)')
            db.execute('INSERT INTO events(name, conversation, payload) VALUES (?, ?, ?)',
                       (self.name, self.conversation_id, json.dumps({'type': 'Stop', 'agent_id': '', 'at': at})))
        db.close()
        return self.listing()['reply_id']

    def waiting(self, tool='call-1'):
        self.record.update(phase='input', activity='busy',
                           active_tools={tool: dict(name='request_user_input', started=1.0, detail='')})
        self.write_record()

    def test_idle_session_lists_null_read_and_signature(self):
        row = self.listing()
        self.assertIn('read', row)
        self.assertIsNone(row['read'])
        self.assertIsNone(row['attention_signature'])

    def test_signature_ignores_status_updates_and_changes_for_a_new_request(self):
        self.waiting()
        first = self.listing()['attention_signature']
        self.assertRegex(first, HEX)
        self.record.update(last_event_at=self.record['last_event_at'] + 50, prompt='A different status line',
                           subagents={'worker': dict(state='working', name='Worker', updated=5.0)})
        self.write_record()
        self.assertEqual(self.listing()['attention_signature'], first)
        self.waiting('call-2')
        second = self.listing()['attention_signature']
        self.assertRegex(second, HEX)
        self.assertNotEqual(second, first)
        self.record.update(phase='working', active_tools={})
        self.write_record()
        self.assertIsNone(self.listing()['attention_signature'])

    def test_repeated_identity_less_errors_have_distinct_signatures(self):
        self.record.update(phase='error', activity='attention', active_tools={}, turn_started=100.0)
        self.write_record()
        first = self.listing()['attention_signature']
        self.assertRegex(first, HEX)
        self.record.update(phase='working', activity='busy', turn_started=200.0)
        self.write_record()
        self.assertIsNone(self.listing()['attention_signature'])
        self.record.update(phase='error', activity='attention')
        self.write_record()
        second = self.listing()['attention_signature']
        self.assertRegex(second, HEX)
        self.assertNotEqual(second, first)


if __name__ == '__main__':
    unittest.main()
```

Run: `cargo build --locked && bash -c 'for v in $(compgen -e | grep "^HGS_"); do unset "$v"; done; HGS_TEST_BIN="$PWD/target/debug/hgs" PYTHONPATH=tests python3 -m unittest -v test_read_marks'`
Expected: FAIL — `AssertionError: 'read' not found in {...}` and `KeyError: 'attention_signature'`.

- [ ] **Step 8: Add the fields to every listed session**

In `src/state/mod.rs` `merge_snapshot`:

1. After `let archived = archive::records()?;` add:

```rust
    let marks = read_marks::load();
```

2. Replace the live branch body

```rust
            if matches(&record, snapshot.get(name)) {
                let session = &mut sessions[*index];
                session["resumable"] = json!(
                    !string(&record, "conversation_id").is_empty()
                        && string(&record, "error").is_empty()
                );
                let summary = journal::summary(&record, true);
                session
                    .as_object_mut()
                    .ok_or("invalid session")?
                    .extend(summary.as_object().ok_or("invalid summary")?.clone());
            }
```

with

```rust
            if matches(&record, snapshot.get(name)) {
                let session = &mut sessions[*index];
                session["resumable"] = json!(
                    !string(&record, "conversation_id").is_empty()
                        && string(&record, "error").is_empty()
                );
                let summary = journal::summary(&record, true);
                session
                    .as_object_mut()
                    .ok_or("invalid session")?
                    .extend(summary.as_object().ok_or("invalid summary")?.clone());
                let questions = read_marks::pending_questions(&record, session, true);
                read_marks::enrich(session, &marks, journal::journal_name(&record), &questions);
            }
```

3. In the stopped-record branch, immediately before its `sessions.push(session);`, and in the archived loop, immediately before its `sessions.push(session);`, add:

```rust
        read_marks::enrich(&mut session, &marks, journal::journal_name(&record), &[]);
```

4. Replace `sessions.extend(dsh::snapshots()?);` with:

```rust
    sessions.extend(dsh::snapshots()?);
    // DeepSeek and untracked live sessions are owned by their names.
    for session in sessions.iter_mut() {
        if session.get("attention_signature").is_none() {
            let owner = string(session, "name").to_owned();
            read_marks::enrich(session, &marks, &owner, &[]);
        }
    }
```

- [ ] **Step 9: Run all tests for this task**

Run: `cargo +1.85.0 test --locked && cargo build --locked && bash -c 'for v in $(compgen -e | grep "^HGS_"); do unset "$v"; done; HGS_TEST_BIN="$PWD/target/debug/hgs" PYTHONPATH=tests python3 -m unittest -v test_read_marks'`
Expected: all Rust tests PASS; 3 Python tests PASS.

- [ ] **Step 10: Commit**

```bash
git add src/state/read_marks.rs tests/test_read_marks.py
git commit --only -m "Add attention signatures and read marks to hgs session listings" -- src/state/read_marks.rs src/state/mod.rs src/state/dsh.rs tests/test_read_marks.py
```

---

### Task 2: `hgs [@host] read <session> --json`

**Files:**
- Modify: `src/state/read_marks.rs` (request parsing, apply, storage, dispatch)
- Modify: `src/state/mod.rs:416` (`state::dispatch`)
- Modify: `src/cli.rs:62` (`USAGE`), `src/cli.rs:300-307` (local dispatch), `src/cli.rs:527-528` (`remote` BatchMode list)
- Modify: `src/mobile_peers.rs:353-360` (argv allowlist), tests near `src/mobile_peers.rs:989`
- Modify: `docs/reference.md:1143` (JSON and external clients)
- Test: `src/state/read_marks.rs`, `src/mobile_peers.rs`, `tests/test_read_marks.py`

**Interfaces:**
- Consumes: Task 1 `load`, `record`, `signature`, `pending_questions`, `path`; `storage::{lock, atomic, read, record_path, records, live, matches}`; `archive::{reconcile, records, equivalent}`; `dsh::{exists, snapshots}`.
- Produces:
  - `pub(super) fn dispatch(args: &[String]) -> Result<i32>` — C1 command; prints JSON; exit 1 for `not_found`.
  - Help text line containing `read <session> --json` (the connector's capability probe in Task 4 matches this substring).
  - `src/mobile_peers.rs`: `fn read_mark(payload: &Option<Value>) -> Result<()>`; argv `["read", <session>, "--json"]` accepted on guarded peer routes.

- [ ] **Step 1: Write the failing Rust unit tests**

Append inside `mod tests` in `src/state/read_marks.rs`:

```rust
    fn request(reply: Option<&str>, signature: Option<&str>) -> Request {
        Request { run: "run-1".into(), conversation: "conversation-1".into(),
            reply: reply.map(Into::into), signature: signature.map(Into::into) }
    }

    #[test]
    fn requests_are_exact() {
        let valid = json!({"run_id":"run-1","conversation_id":"conversation-1","reply_id":null,
            "attention_signature":"a".repeat(64)});
        assert_eq!(parse(&valid).unwrap(), request(None, Some("a".repeat(64).as_str())));
        for (key, value) in [("run_id", json!("")), ("conversation_id", json!(3)), ("reply_id", json!("x\ny")),
            ("attention_signature", json!("A".repeat(64))), ("attention_signature", json!("a".repeat(63)))] {
            let mut invalid = valid.clone();
            invalid[key] = value;
            assert!(parse(&invalid).is_err(), "{key}");
        }
        let mut extra = valid.clone();
        extra["request_id"] = json!("fixture");
        assert!(parse(&extra).is_err());
        let mut missing = valid;
        missing.as_object_mut().unwrap().remove("reply_id");
        assert!(parse(&missing).is_err());
    }

    #[test]
    fn marks_move_only_to_current_values() {
        let mut current = waiting();
        current["reply_id"] = json!("7:100.5");
        let digest = signature(&current, &[]).unwrap();
        let (reply, attention, mark) = apply(&current, &[], &request(Some("7:100.5"), Some(digest.as_str())), &Value::Null, 50.0);
        assert!(reply && attention);
        assert_eq!(mark, json!({"conversation_id":"conversation-1","reply_id":"7:100.5","attention_signature":digest,"at":50.0}));
        // Older values are stale and keep the stored mark.
        let older_request = request(Some("6:90.5"), Some("b".repeat(64).as_str()));
        let (reply, attention, kept) = apply(&current, &[], &older_request, &mark, 60.0);
        assert!(!reply && !attention);
        assert_eq!(kept, mark);
        // Another run or conversation makes every field stale.
        let mut other = request(Some("7:100.5"), Some(digest.as_str()));
        other.run = "run-0".into();
        assert_eq!(apply(&current, &[], &other, &Value::Null, 70.0), (false, false, Value::Null));
        // A partial mark keeps the other field.
        let older = json!({"conversation_id":"conversation-1","reply_id":"6:90.5","attention_signature":null,"at":1.0});
        let (reply, attention, partial) = apply(&current, &[], &request(None, Some(digest.as_str())), &older, 80.0);
        assert!(!reply && attention);
        assert_eq!((partial["reply_id"].clone(), partial["at"].clone()), (json!("6:90.5"), json!(80.0)));
    }

    #[test]
    fn storage_keeps_the_newest_conversations_of_each_session() {
        let mut conversations = serde_json::Map::new();
        for index in 0..10 {
            conversations.insert(format!("conversation-{index}"), json!({"at": index as f64}));
        }
        bounded(&mut conversations);
        assert_eq!(conversations.len(), MAX_CONVERSATIONS);
        assert!(!conversations.contains_key("conversation-0") && !conversations.contains_key("conversation-1"));
        assert!(conversations.contains_key("conversation-9"));
    }
```

Run: `cargo +1.85.0 test --locked read_marks`
Expected: compile errors `cannot find struct `Request``, `cannot find function `parse``, `apply`, `bounded`, `MAX_CONVERSATIONS`.

- [ ] **Step 2: Implement parsing, application and storage**

Add to `src/state/read_marks.rs` below `enrich` (above the test module):

```rust
const MAX_CONVERSATIONS: usize = 8;
const USAGE: &str = "usage: hgs read <session> --json   (JSON stdin: run_id, conversation_id, reply_id, attention_signature)";

#[derive(Debug, PartialEq)]
struct Request {
    run: String,
    conversation: String,
    reply: Option<String>,
    signature: Option<String>,
}

fn parse(value: &Value) -> Result<Request> {
    const KEYS: [&str; 4] = ["run_id", "conversation_id", "reply_id", "attention_signature"];
    let object = value.as_object().ok_or("read mark must be a JSON object")?;
    if object.len() != KEYS.len() || KEYS.iter().any(|key| !object.contains_key(*key)) {
        return Err("read mark needs exactly run_id, conversation_id, reply_id and attention_signature".into());
    }
    let text = |key: &str| -> Result<String> {
        value[key]
            .as_str()
            .filter(|v| !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control))
            .map(str::to_owned)
            .ok_or_else(|| format!("invalid {key}"))
    };
    let optional = |key: &str| -> Result<Option<String>> {
        if value[key].is_null() { Ok(None) } else { text(key).map(Some) }
    };
    let signature = optional("attention_signature")?;
    if signature.as_deref().is_some_and(|s| {
        s.len() != 64 || !s.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    }) {
        return Err("attention_signature must be a lowercase hex SHA-256".into());
    }
    Ok(Request {
        run: text("run_id")?,
        conversation: text("conversation_id")?,
        reply: optional("reply_id")?,
        signature,
    })
}

/// Each requested field moves only to the session's current value, in the
/// same run and conversation. Stale fields leave the stored mark unchanged.
fn apply(current: &Value, questions: &[Value], request: &Request, existing: &Value, at: f64) -> (bool, bool, Value) {
    let same = string(current, "run_id") == request.run
        && string(current, "conversation_id") == request.conversation;
    let reply = same
        && request.reply.as_deref().is_some_and(|reply| current["reply_id"].as_str() == Some(reply));
    let attention = same && request.signature.is_some() && request.signature == signature(current, questions);
    if !reply && !attention {
        return (false, false, existing.clone());
    }
    let mut next = if existing.is_object() {
        existing.clone()
    } else {
        json!({"conversation_id": request.conversation, "reply_id": null, "attention_signature": null})
    };
    if reply {
        next["reply_id"] = json!(request.reply);
    }
    if attention {
        next["attention_signature"] = json!(request.signature);
    }
    next["at"] = json!(at);
    (reply, attention, next)
}

/// Keep the newest conversations of one session.
fn bounded(conversations: &mut serde_json::Map<String, Value>) {
    while conversations.len() > MAX_CONVERSATIONS {
        let Some(oldest) = conversations
            .iter()
            .min_by(|a, b| a.1["at"].as_f64().unwrap_or(0.0).total_cmp(&b.1["at"].as_f64().unwrap_or(0.0)))
            .map(|(key, _)| key.clone())
        else {
            return;
        };
        conversations.remove(&oldest);
    }
}

/// The session as `hgs ls --json` shows it, its mark owner and its pending
/// questions; None when the name has no current tracked or native session.
fn current(name: &str) -> Result<Option<(Value, String, Vec<Value>)>> {
    if dsh::exists(name) {
        return Ok(dsh::snapshots()?
            .into_iter()
            .find(|session| string(session, "name") == name)
            .map(|session| (session, name.to_owned(), Vec::new())));
    }
    archive::reconcile()?;
    let guard = lock(None)?;
    if !record_path(name).exists() {
        return Ok(None);
    }
    let record = read(name)?;
    if archive::equivalent(&record, &archive::records()?) {
        return Ok(None);
    }
    let snapshot = live()?;
    let panes = snapshot.get(name);
    // A name reused by another pane is not this session.
    if panes.is_some() && !matches(&record, panes) {
        return Ok(None);
    }
    let live_pane = panes.is_some();
    if !live_pane && string(&record, "conversation_id").is_empty() {
        return Ok(None);
    }
    let mut view = journal::summary(&record, live_pane);
    view["name"] = json!(name);
    if !live_pane {
        view["state"] = json!(if record["paused"].as_bool().unwrap_or(false) { "paused" } else { "stopped" });
    }
    drop(guard);
    let questions = pending_questions(&record, &view, live_pane);
    Ok(Some((view, journal::journal_name(&record).to_owned(), questions)))
}

/// `hgs read <session> --json`. An unknown session is a JSON answer with exit
/// status 1, so desktops and the mobile connector can tell it from a failure.
pub(super) fn dispatch(args: &[String]) -> Result<i32> {
    let [name, flag] = args else {
        return Err(USAGE.into());
    };
    if flag != "--json" || name.is_empty() || name.starts_with(['-', '@']) {
        return Err(USAGE.into());
    }
    let request = parse(&stdin_json()?)?;
    let Some((view, owner, questions)) = current(name)? else {
        println!("{}", json!({"ok": false, "error": "not_found"}));
        return Ok(1);
    };
    let _guard = lock(Some(&root().join("read-marks.lock")))?;
    let mut sessions = load();
    let existing = record(&sessions, &owner, &request.conversation).clone();
    let (reply, attention, next) = apply(&view, &questions, &request, &existing, now());
    if reply || attention {
        let owners: std::collections::BTreeSet<String> = records()?
            .iter()
            .map(|record| journal::journal_name(record).to_owned())
            .collect();
        let map = sessions.as_object_mut().ok_or("invalid read marks")?;
        map.retain(|key, _| key == &owner || owners.contains(key) || dsh::exists(key));
        let conversations = map.entry(owner.clone()).or_insert_with(|| json!({}));
        if !conversations.is_object() {
            *conversations = json!({});
        }
        let conversations = conversations.as_object_mut().ok_or("invalid read marks")?;
        conversations.insert(request.conversation.clone(), next.clone());
        bounded(conversations);
        atomic(&path(), &format!("{}\n", json!({"version": 1, "sessions": sessions})))?;
    }
    println!("{}", json!({"ok": true, "applied": {"reply_id": reply, "attention_signature": attention}, "read": next}));
    Ok(0)
}
```

Wire the command in `src/state/mod.rs` `dispatch`, next to `history` (before the `dsh::exists` fallthrough so DeepSeek sessions are handled here too):

```rust
    if command == "read" { return read_marks::dispatch(args); }
```

- [ ] **Step 3: Run the unit tests to verify they pass**

Run: `cargo +1.85.0 test --locked read_marks`
Expected: 9 tests PASS.

- [ ] **Step 4: Write the failing guarded-peer validation test**

Append inside `mod tests` in `src/mobile_peers.rs`:

```rust
    #[test]
    fn read_marks_require_an_exact_scoped_payload() {
        let mark = json!({"run_id":"run","conversation_id":"conversation","reply_id":null,"attention_signature":"a".repeat(64)});
        assert!(validate(&request(&["read", "codex/project", "--json"], Some(mark.clone()))).is_ok());
        assert!(validate(&request(&["read", "codex/project", "--json"], None)).is_err());
        assert!(validate(&request(&["read", "@peer", "--json"], Some(mark.clone()))).is_err());
        assert!(validate(&request(&["read", "codex/project"], Some(mark.clone()))).is_err());
        let mut extra = mark.clone();
        extra["request_id"] = json!(ID);
        assert!(validate(&request(&["read", "codex/project", "--json"], Some(extra))).is_err());
        let mut empty = mark;
        empty["run_id"] = json!("");
        assert!(validate(&request(&["read", "codex/project", "--json"], Some(empty))).is_err());
    }
```

Run: `cargo +1.85.0 test --locked read_marks_require_an_exact_scoped_payload`
Expected: FAIL at the first `is_ok()` (`native operation is not allowed`).

- [ ] **Step 5: Allow `read` on guarded peer routes**

In `src/mobile_peers.rs`, add after `fn scoped(...)`:

```rust
/// C1 stdin: exactly the run/conversation identity and two nullable marks.
fn read_mark(payload: &Option<Value>) -> Result<()> {
    let value = payload.as_ref().context("read mark payload is required")?;
    let object = value.as_object().context("native payload must be an object")?;
    ensure!(
        object.len() == 4
            && ["run_id", "conversation_id", "reply_id", "attention_signature"]
                .iter()
                .all(|key| object.contains_key(*key)),
        "invalid read mark fields"
    );
    ensure!(
        value["run_id"].as_str().is_some_and(|v| text(v, 128, false))
            && value["conversation_id"].as_str().is_some_and(|v| text(v, 256, false)),
        "native run and conversation identity are required"
    );
    for key in ["reply_id", "attention_signature"] {
        ensure!(
            value[key].is_null() || value[key].as_str().is_some_and(|v| text(v, 256, false)),
            "invalid read mark value"
        );
    }
    Ok(())
}
```

and in `validate`, before the `"history" | "terminal" | ...` arm:

```rust
        "read" => {
            ensure!(
                rest.len() == 2 && session(&rest[0]) && rest[1] == "--json",
                "native read requires scoped JSON"
            );
            read_mark(&request.payload)?;
        }
```

Run: `cargo +1.85.0 test --locked read_marks_require_an_exact_scoped_payload`
Expected: PASS.

- [ ] **Step 6: Wire the CLI command and its `@host` forwarding**

In `src/cli.rs`:

1. `USAGE`, after the `answer` line (line 62):

```text
       hgs [@host] read <session> --json   mark the current reply/attention read (JSON stdin)
```

2. Local dispatch (lines 300-306): add `| "read"` to the first match pattern after `"search"`, and add `"read"` to the no-dry-run `matches!` list:

```rust
        "inspect" | "processes" | "worktrees" | "git-status" | "dirs" | "send" | "send-now" | "session-action" | "terminal" | "history" | "interrupt" | "clear-context" | "compact-context" | "answer" | "effort" | "settings" | "search" | "read"
        | "attachment" | "recovery" => {
            if command == "worktrees" && args.first().is_some_and(|s| s == "create" || s == "remove") && dry {
                return Err(Error::new(1, "worktree creation and removal do not support --dry-run"));
            }
            if (matches!(command.as_str(), "processes" | "send" | "send-now" | "session-action" | "terminal" | "interrupt" | "clear-context" | "compact-context" | "answer" | "effort" | "settings" | "recovery" | "read") || (command=="attachment" && has(args,"--stage"))) && dry {
```

3. `remote` (lines 527-528): add `| "read"` after `"swarm"` so the command runs with `BatchMode=yes`, `ConnectTimeout=3`, no `-t`, and stdin forwarded:

```rust
        | "send" | "send-now" | "session-action" | "terminal" | "interrupt" | "clear-context" | "compact-context" | "answer" | "effort" | "settings" | "search" | "dsh" | "attachment" | "recovery" | "swarm" | "read" => {
```

- [ ] **Step 7: Write the failing CLI integration tests**

Append to class `ReadMarks` in `tests/test_read_marks.py` (above `if __name__`):

```python
    def read(self, name=None, remote=False, **fields):
        payload = {'run_id': self.run_id, 'conversation_id': self.conversation_id,
                   'reply_id': None, 'attention_signature': None, **fields}
        return subprocess.run([str(HGS), *(['@remote'] if remote else []), 'read', name or self.name, '--json'],
                              input=json.dumps(payload), env=self.env, text=True, capture_output=True, timeout=10)

    def mark(self, **fields):
        result = self.read(**fields)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def test_mark_applies_only_current_values_and_old_marks_stay_stale(self):
        first = self.reply(100.5)
        result = self.mark(reply_id=first)
        self.assertEqual(result['ok'], True)
        self.assertEqual(result['applied'], {'reply_id': True, 'attention_signature': False})
        self.assertEqual(set(result['read']), {'conversation_id', 'reply_id', 'attention_signature', 'at'})
        self.assertEqual(self.listing()['read']['reply_id'], first)
        second = self.reply(200.5)
        self.assertEqual(self.mark(reply_id=first)['applied'], {'reply_id': False, 'attention_signature': False})
        row = self.listing()
        self.assertEqual((row['reply_id'], row['read']['reply_id']), (second, first))
        self.assertFalse(self.mark(reply_id=second, run_id='another-run')['applied']['reply_id'])
        self.assertFalse(self.mark(reply_id=second, conversation_id='another-conversation')['applied']['reply_id'])
        self.assertEqual(self.listing()['read']['reply_id'], first)

    def test_attention_mark_holds_across_status_updates_until_a_new_request(self):
        self.waiting()
        signature = self.listing()['attention_signature']
        self.assertTrue(self.mark(attention_signature=signature)['applied']['attention_signature'])
        self.record.update(last_event_at=self.record['last_event_at'] + 10)
        self.write_record()
        row = self.listing()
        self.assertEqual(row['read']['attention_signature'], row['attention_signature'])
        self.waiting('call-2')
        row = self.listing()
        self.assertNotEqual(row['read']['attention_signature'], row['attention_signature'])

    def test_second_identity_less_error_is_unacknowledged(self):
        self.record.update(phase='error', activity='attention', active_tools={}, turn_started=100.0)
        self.write_record()
        self.mark(attention_signature=self.listing()['attention_signature'])
        self.record.update(phase='working', activity='busy', turn_started=200.0)
        self.write_record()
        self.record.update(phase='error', activity='attention')
        self.write_record()
        row = self.listing()
        self.assertNotEqual(row['attention_signature'], row['read']['attention_signature'])

    def test_unknown_session_reports_not_found(self):
        result = self.read(name='codex/missing-session', reply_id='1:1.0')
        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout), {'ok': False, 'error': 'not_found'})

    def test_malformed_marks_change_nothing(self):
        reply = self.reply(100.5)
        for fields in (dict(run_id=''), dict(reply_id=7), dict(attention_signature='A' * 64),
                       dict(attention_signature='a' * 63), dict(request_id='fixture')):
            result = self.read(**{'reply_id': reply, **fields})
            self.assertEqual(result.returncode, 1, fields)
            self.assertEqual(result.stdout, '', fields)
        self.assertIsNone(self.listing()['read'])
        self.assertFalse((self.state / 'read-marks.json').exists())

    def test_concurrent_writers_keep_both_fields(self):
        reply = self.reply(100.5)
        self.waiting()
        signature = self.listing()['attention_signature']
        base = {'run_id': self.run_id, 'conversation_id': self.conversation_id, 'reply_id': None, 'attention_signature': None}
        writers = []
        for fields in (dict(reply_id=reply), dict(attention_signature=signature)):
            process = subprocess.Popen([str(HGS), 'read', self.name, '--json'], stdin=subprocess.PIPE,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=self.env, text=True)
            writers.append((process, json.dumps({**base, **fields})))
        for process, payload in writers:
            _, error = process.communicate(payload, timeout=10)
            self.assertEqual(process.returncode, 0, error)
        read = self.listing()['read']
        self.assertEqual((read['reply_id'], read['attention_signature']), (reply, signature))

    def test_marks_follow_a_renamed_session(self):
        reply = self.reply(100.5)
        self.mark(reply_id=reply)
        subprocess.run([str(HGS), 'rename', self.name, 'codex/input-test/renamed'], env=self.env,
                       text=True, capture_output=True, check=True, timeout=10)
        self.name = 'codex/input-test/renamed'
        row = self.listing()
        self.assertEqual((row['reply_id'], row['read']['reply_id']), (reply, reply))

    def test_remote_mark_forwards_stdin_without_a_terminal(self):
        self.script('ssh', """#!/usr/bin/env python3
import json, os, pathlib, sys
pathlib.Path(os.environ['INPUT_FIXTURE'], 'ssh-args').write_text(json.dumps(sys.argv[1:]))
os.execv('/bin/sh', ['sh', '-c', sys.argv[-1]])
""")
        reply = self.reply(100.5)
        result = self.read(remote=True, reply_id=reply)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(json.loads(result.stdout)['applied']['reply_id'])
        args = json.loads((self.root / 'ssh-args').read_text())
        self.assertNotIn('-t', args)
        self.assertIn('BatchMode=yes', args)
        self.assertEqual(args[-1], '~/.local/bin/hgs read codex/input-test --json')
        missing = self.read(remote=True, name='codex/missing-session', reply_id=reply)
        self.assertEqual(missing.returncode, 1)
        self.assertEqual(json.loads(missing.stdout), {'ok': False, 'error': 'not_found'})
```

- [ ] **Step 8: Run the integration tests**

Run: `cargo build --locked && bash -c 'for v in $(compgen -e | grep "^HGS_"); do unset "$v"; done; HGS_TEST_BIN="$PWD/target/debug/hgs" PYTHONPATH=tests python3 -m unittest -v test_read_marks'`
Expected: 11 tests PASS. If Step 6 is skipped, `test_remote_mark_forwards_stdin_without_a_terminal` fails with `-t` in the SSH arguments — that is the regression it pins.

- [ ] **Step 9: Document the contract**

In `docs/reference.md`, after the paragraph ending "older live-session snapshots. `ok: false` indicates an unavailable host." (line 1143), insert:

````markdown
Every session also has `attention_signature` and `read`, either of which may be
`null`. `attention_signature` is a lowercase hex SHA-256 of what currently needs
the owner: run, conversation, the phase when it is `approval`, `input` or
`error`, the attention identity, pending question identities (the IDs and hashes
the mobile connector publishes as `mobile_attention`) and actionable subagent
requests. It is `null` when nothing needs attention and does not change on
status-only updates. `read` is the host's mark for the current conversation:
`{conversation_id, reply_id, attention_signature, at}`. A reply is unread while
`reply_id` differs from `read.reply_id`; attention is unacknowledged while a
non-null signature differs from `read.attention_signature`.

```sh
printf '%s' '{"run_id":"…","conversation_id":"…","reply_id":"12:1787840176.5","attention_signature":null}' |
  hgs @mac read codex/sample-project/review --json
```

`hgs [@host] read` applies each non-null field only while it equals the session's
current value in the same run and conversation; other fields are reported as not
applied and the mark does not change. It prints `{"ok": true, "applied":
{"reply_id": …, "attention_signature": …}, "read": …}`; an unknown session prints
`{"ok": false, "error": "not_found"}` and exits with status 1. Marks live in
`read-marks.json` in the hgs state directory, at most eight conversations per
session, and follow `hgs rename`.
````

- [ ] **Step 10: Run the CLI suite and commit**

Run: `cargo +1.85.0 test --locked && bash tests/test_hgs.sh` (with `HGS_TEST_BIN="$PWD/target/debug/hgs"` and `HGS_*` unset as above)
Expected: PASS.

```bash
git commit --only -m "Add hgs read for forward-only shared read marks" -- src/state/read_marks.rs src/state/mod.rs src/cli.rs src/mobile_peers.rs tests/test_read_marks.py docs/reference.md
```

---

### Task 3: Relay API v1 operation `mark_read`

**Files:**
- Modify: `services/mobile/relay/src/protocol.rs:24-52` (`OPERATIONS`), `services/mobile/relay/src/protocol.rs:152-190` (`request`)
- Test: `services/mobile/relay/tests/contracts.rs` (new test)

**Interfaces:**
- Consumes: C4.
- Produces: `protocol::OPERATIONS` contains `"mark_read"`; `/v1/capabilities` advertises it; `Submission::new` accepts exactly the C4 payload (no `request_id`/`expected_*` identity fields, unlike other session operations); `Store::submit` keeps the existing rule that a computer must advertise the operation in `mobile_capabilities.operations` (409 otherwise).

- [ ] **Step 1: Write the failing contract test**

Append to `services/mobile/relay/tests/contracts.rs`:

```rust
#[tokio::test]
async fn mark_read_payload_is_exact_and_capability_gated() {
    let f = Fixture::new().await;
    let id = Uuid::new_v4().to_string();
    let payload = json!({"run_id":"run","conversation_id":"conversation","reply_id":"7:100.5","attention_signature":"a".repeat(64)});
    let body = json!({"request_id":id,"computer_id":f.node.id,"operation":"mark_read","session":"codex/example/tag","payload":payload});
    // A computer that does not advertise the operation keeps rejecting it.
    assert_eq!(f.submit(&body).await.unwrap_err().0.as_u16(), 409);
    let snapshot = json!({"sessions":[],"mobile_capabilities":{"protocol_version":1,"operations":["mark_read"],"features":[]}});
    f.store
        .heartbeat(&f.node, &json!({"snapshot":snapshot}), &canonical(&snapshot).unwrap())
        .await
        .unwrap();
    assert_eq!(f.submit(&body).await.unwrap()["state"], "queued");
    let changes = [
        json!({"reply_id":7}),
        json!({"attention_signature":"A".repeat(64)}),
        json!({"attention_signature":"a".repeat(63)}),
        json!({"run_id":""}),
        json!({"reply_id":"bad\u{7}"}),
        json!({"request_id":id}),
    ];
    for change in changes {
        let mut invalid = body.clone();
        invalid["request_id"] = json!(Uuid::new_v4().to_string());
        for (key, value) in change.as_object().unwrap() {
            invalid["payload"][key] = value.clone();
        }
        assert!(zerus_relay::store::Submission::new(invalid).is_err());
    }
    let mut missing = body.clone();
    missing["payload"].as_object_mut().unwrap().remove("reply_id");
    assert!(zerus_relay::store::Submission::new(missing).is_err());
    let mut nulls = body.clone();
    nulls["request_id"] = json!(Uuid::new_v4().to_string());
    nulls["payload"]["reply_id"] = Value::Null;
    nulls["payload"]["attention_signature"] = Value::Null;
    assert!(zerus_relay::store::Submission::new(nulls).is_ok());
    assert!(protocol::capabilities(vec![])["operations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "mark_read"));
    f.close().await;
}
```

- [ ] **Step 2: Run it to verify it fails**

Run (in `services/mobile/relay`): `cargo +1.85.0 test --locked --test contracts mark_read_payload_is_exact_and_capability_gated`
Expected: FAIL — `assertion `left == right` failed: left: 400, right: 409`, because `Submission::new` rejects the unknown operation before the capability check.

- [ ] **Step 3: Accept the operation**

In `services/mobile/relay/src/protocol.rs`, add `"mark_read",` to `OPERATIONS` between `"launch",` and `"pause",`. In `request`, insert a branch between the `else if op == "inspect" { … }` block and the final `else {`:

```rust
    } else if op == "mark_read" {
        // C4: exact host read-mark identity; the host decides what is current.
        fields(
            p,
            &["run_id", "conversation_id", "reply_id", "attention_signature"],
            &[],
        )?;
        text(&p["run_id"], 128, false)?;
        text(&p["conversation_id"], 256, false)?;
        if !p["reply_id"].is_null() {
            text(&p["reply_id"], 256, false)?;
        }
        if !p["attention_signature"].is_null() {
            hex(&p["attention_signature"])?;
        }
```

- [ ] **Step 4: Run the relay checks**

Run (in `services/mobile/relay`): `cargo +1.85.0 fmt --check && cargo +1.85.0 clippy --locked --all-targets -- -D warnings && cargo +1.85.0 test --locked --test contracts mark_read_payload_is_exact_and_capability_gated`
Expected: PASS. The full `cargo +1.85.0 test --locked` runs on `arch` in Task 10.

- [ ] **Step 5: Commit**

```bash
git commit --only -m "Accept the additive mark_read relay operation" -- services/mobile/relay/src/protocol.rs services/mobile/relay/tests/contracts.rs
```

---

### Task 4: Connector and API v1 oracle: execute `mark_read`

**Files:**
- Modify: `services/mobile/zerus_mobile/context.py:7` (`OPERATIONS`), add `MARK_READ_FIELDS`, `validate_mark_read`, `mark_read_result`
- Modify: `services/mobile/zerus_mobile/server.py:22,277-325` (`validate_request`)
- Modify: `services/mobile/zerus_mobile/connector.py:31` (import), `:278-296` (`_init_machine_context`), `:308-320` (`native`), `:319-381` (`_native`), `:448` (advertised operations), `:456-481` (`probe_capabilities`), `:573-576` (`validate`), `:749-750` and `:976` (`_execute_claimed`)
- Modify: `services/mobile/zerus_mobile/fleet.py:93-104` (peer `native` callback)
- Modify: `services/mobile/tests/fixture_hgs.py:252-254,~505`, `services/mobile/tests/fixture_peer_hgs.py:35-36,~70`
- Modify: `services/mobile/README.md:575`, `docs/mobile-architecture.md:627-632`
- Test: `services/mobile/tests/test_mobile_read.py` (new), `services/mobile/tests/test_connector_peers.py`, `services/mobile/tests/test_relay.py`, `services/mobile/tests/test_rust_relay.py:93-104`

**Interfaces:**
- Consumes: Task 2 help line `read <session> --json`, C1 stdout and exit status 1 for `not_found`; Task 3 Rust validation.
- Produces:
  - `context.validate_mark_read(payload) -> None` (raises `ValueError`), `context.mark_read_result(result, payload) -> bool`, `"mark_read" in context.OPERATIONS`.
  - `Connector.read_supported: bool`; `Connector.native(argv, payload=None, *, timeout=None, json_output=True, failure_json=False)` and `_native(..., failure_json=False)`: with `failure_json=True` a nonzero exit that printed JSON returns that JSON.
  - Request result for `mark_read` is exactly the `hgs read` stdout object.

- [ ] **Step 1: Write the failing tests**

Create `services/mobile/tests/test_mobile_read.py`:

```python
"""Shared read marks through API v1 and the connector, with synthetic native ABIs only."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import AsyncMock, patch
import uuid

from zerus_mobile.connector import Connector
from zerus_mobile.context import OPERATIONS, mark_read_result, validate_mark_read
from zerus_mobile.server import validate_request

NAME = "codex/example/mobile"
SIGNATURE = "a" * 64


def mark(**changes):
    return {"run_id": "fixture-run", "conversation_id": "fixture-conversation",
            "reply_id": "7:100.5", "attention_signature": SIGNATURE, **changes}


def command(payload=None, session=NAME):
    return {"request_id": str(uuid.uuid4()), "operation": "mark_read", "session": session,
            "payload": mark() if payload is None else payload}


class MarkReadValidation(unittest.TestCase):
    def test_payload_is_exact_identity_with_nullable_marks(self):
        self.assertIn("mark_read", OPERATIONS)
        validate_mark_read(mark())
        validate_mark_read(mark(reply_id=None, attention_signature=None))
        for invalid in (mark(reply_id=7), mark(run_id=""), mark(conversation_id="x" * 257),
                        mark(attention_signature="A" * 64), mark(attention_signature="a" * 63),
                        {**mark(), "request_id": str(uuid.uuid4())},
                        {k: v for k, v in mark().items() if k != "reply_id"}, None):
            with self.assertRaises(ValueError):
                validate_mark_read(invalid)

    def test_relay_oracle_validates_the_same_envelope(self):
        request = {**command(), "computer_id": str(uuid.uuid4())}
        validate_request(request)
        with self.assertRaises(Exception):
            validate_request({**request, "payload": mark(attention_signature="A" * 64)})

    def test_results_must_be_native_read_receipts(self):
        ok = {"ok": True, "applied": {"reply_id": True, "attention_signature": False},
              "read": {"conversation_id": "fixture-conversation", "reply_id": "7:100.5", "attention_signature": None, "at": 1.0}}
        self.assertTrue(mark_read_result(ok, mark()))
        self.assertTrue(mark_read_result({**ok, "applied": {"reply_id": False, "attention_signature": False}, "read": None}, mark()))
        self.assertTrue(mark_read_result({"ok": False, "error": "not_found"}, mark()))
        for invalid in ({**ok, "read": {**ok["read"], "conversation_id": "other"}},
                        {**ok, "applied": {"reply_id": 1, "attention_signature": False}},
                        {"ok": False, "error": "other"}, {"ok": True}, None):
            self.assertFalse(mark_read_result(invalid, mark()))


class MarkReadConnector(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.connector = Connector({"server_url": "https://relay.example.invalid", "node_token": "fixture-token",
                                    "state_dir": self.temp.name})
        self.connector.sessions = {NAME}
        self.connector.read_supported = True

    async def asyncTearDown(self):
        self.connector.journal.close()
        self.temp.cleanup()

    async def test_maps_to_native_read_and_returns_its_stdout_once(self):
        receipt = {"ok": True, "applied": {"reply_id": True, "attention_signature": True},
                   "read": {"conversation_id": "fixture-conversation", "reply_id": "7:100.5",
                            "attention_signature": SIGNATURE, "at": 1.0}}
        request = command()
        with patch.object(self.connector, "native", AsyncMock(return_value=receipt)) as native:
            response = await self.connector.execute(request)
            self.assertEqual(response, {"state": "completed", "result": receipt, "error": None})
            self.assertEqual(native.call_args.args, (["read", NAME, "--json"], request["payload"]))
            self.assertEqual(native.call_args.kwargs, {"failure_json": True})
            self.assertEqual(await self.connector.execute(request), response)
            self.assertEqual(native.await_count, 1)

    async def test_not_found_is_a_completed_result(self):
        with patch.object(self.connector, "native", AsyncMock(return_value={"ok": False, "error": "not_found"})):
            response = await self.connector.execute(command())
        self.assertEqual((response["state"], response["result"]), ("completed", {"ok": False, "error": "not_found"}))

    async def test_invalid_or_unknown_targets_never_spawn(self):
        with patch.object(self.connector, "native", AsyncMock()) as native:
            for request in (command(mark(attention_signature="bad")), command(session="codex/other"),
                            command(session="@peer"), {**command(), "archive_id": str(uuid.uuid4())}):
                self.assertEqual((await self.connector.execute(request))["state"], "failed")
            native.assert_not_awaited()

    async def test_old_native_without_read_abi_is_not_advertised_or_executed(self):
        self.connector.read_supported = False
        with patch.object(self.connector, "native", AsyncMock()) as native:
            response = await self.connector.execute(command())
            native.assert_not_awaited()
        self.assertEqual(response["state"], "failed")
        self.assertIn("read", response["error"])
        old_help = "hgs session-action <session> --json scoped native lifecycle action"
        for help_text, supported in ((old_help, False), (old_help + "\nhgs [@host] read <session> --json   mark", True)):
            self.connector.capabilities_next_poll = 0
            with patch.object(self.connector, "native", AsyncMock(return_value=help_text)):
                await self.connector.probe_capabilities()
            self.assertIs(self.connector.read_supported, supported)


class MarkReadFixture(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.env = patch.dict(os.environ, {"ZERUS_MOBILE_FIXTURE_DIR": str(self.root)})
        self.env.start()
        self.connector = Connector({"server_url": "https://relay.example.invalid", "node_token": "fixture-token",
            "hgs_path": str(Path(__file__).with_name("fixture_hgs.py")), "state_dir": str(self.root / "journal")})
        self.snapshot = await self.connector.snapshot()

    async def asyncTearDown(self):
        self.connector.journal.close()
        self.env.stop()
        self.temp.cleanup()

    def config(self, **values):
        (self.root / "fixture-config.json").write_text(json.dumps(values))

    def writes(self):
        rows = [json.loads(line) for line in (self.root / "mutations.jsonl").read_text().splitlines()]
        return [row for row in rows if row["operation"] == "read"]

    async def test_native_exit_status_one_still_returns_not_found(self):
        self.assertIn("mark_read", self.snapshot["mobile_capabilities"]["operations"])
        self.config(read_current={"reply_id": "7:100.5"})
        applied = await self.connector.execute(command(mark(attention_signature=None)))
        self.assertEqual(applied["state"], "completed")
        self.assertTrue(applied["result"]["applied"]["reply_id"])
        stale = await self.connector.execute(command(mark(reply_id="6:90.5", attention_signature=None)))
        self.assertEqual(stale["result"]["applied"], {"reply_id": False, "attention_signature": False})
        self.config(read_not_found=True)
        missing = await self.connector.execute(command())
        self.assertEqual((missing["state"], missing["result"]), ("completed", {"ok": False, "error": "not_found"}))
        self.assertEqual(len(self.writes()), 2)


if __name__ == "__main__":
    unittest.main()
```

Add to `services/mobile/tests/test_connector_peers.py` (inside `PeerConnectorTests`):

```python
    async def test_mark_read_reaches_only_the_selected_machine(self):
        for peer in (True, False):
            run = 'peer-run' if peer else 'local-run'
            request = self.request('mark_read', peer=peer)
            request['payload'] = {'run_id': run, 'conversation_id': 'peer-conversation' if peer else 'local-conversation',
                                  'reply_id': 'reply-' + run, 'attention_signature': None}
            result = await self.connector.execute(request)
            self.assertEqual(result['state'], 'completed', result)
            self.assertTrue(result['result']['applied']['reply_id'])
        effects = [json.loads(row) for row in (self.root / 'native_effects.jsonl').read_text().splitlines()]
        self.assertEqual([(row['argv'], row['machine']) for row in effects if row['argv'][0] == 'read'],
                         [(['read', 'codex/example', '--json'], PEER), (['read', 'codex/example', '--json'], LOCAL)])
```

Add to `services/mobile/tests/test_relay.py` (`RelayTests`), and import `OPERATIONS` with `from zerus_mobile.context import OPERATIONS`:

```python
    async def test_mark_read_is_exact_and_capability_gated(self):
        value = self.command("mark_read")
        value["payload"] = {"run_id": "example-run", "conversation_id": "example-conversation",
                            "reply_id": "7:100.5", "attention_signature": "a" * 64}
        await self.call("POST", "/v1/requests", value, expected=409)
        snapshot = {"sessions": [], "mobile_capabilities": {"protocol_version": 1, "operations": ["mark_read"], "features": []}}
        await self.call("POST", "/v1/node/heartbeat", {"snapshot": snapshot}, node=True)
        self.assertEqual((await self.call("POST", "/v1/requests", value, expected=202))["state"], "queued")
        for change in ({"reply_id": 7}, {"attention_signature": "A" * 64}, {"attention_signature": "a" * 63},
                       {"run_id": ""}, {"request_id": value["request_id"]}):
            invalid = self.command("mark_read")
            invalid["payload"] = {**value["payload"], **change}
            await self.call("POST", "/v1/requests", invalid, expected=400)
        delivered = await self.call("GET", "/v1/node/requests", node=True)
        self.assertEqual(delivered["requests"][0]["payload"], value["payload"])
        self.assertEqual(set((await self.call("GET", "/v1/capabilities"))["operations"]), OPERATIONS)
```

and add `'mark_read_is_exact_and_capability_gated',` to the name tuple in `services/mobile/tests/test_rust_relay.py` so the same assertions run against the Rust binary (including Python/Rust operation parity).

- [ ] **Step 2: Run them to verify they fail**

Run: `python -m unittest discover -s services/mobile/tests -p 'test_mobile_read.py' -v`
Expected: `ImportError: cannot import name 'mark_read_result' from 'zerus_mobile.context'`.

- [ ] **Step 3: Add the shared validation (oracle)**

In `services/mobile/zerus_mobile/context.py`, change `OPERATIONS` to include `"mark_read"`:

```python
OPERATIONS = LAUNCH_OPERATIONS | LIFECYCLE_OPERATIONS | TERMINAL_OPERATIONS | frozenset({"inspect", "send", "answer", "interrupt", "send_now", "settings", "process_output", "process_stop", "history", "recovery_action", "mark_read", *CONTEXT_COMMANDS})
```

and append:

```python
MARK_READ_FIELDS = frozenset({"run_id", "conversation_id", "reply_id", "attention_signature"})


def validate_mark_read(payload):
    """C4: exact identity and two nullable marks; native hgs decides what is current."""
    if not isinstance(payload, dict) or set(payload) != MARK_READ_FIELDS:
        raise ValueError("mark_read requires exactly run, conversation, reply and attention fields")
    bounded(payload["run_id"], 128)
    bounded(payload["conversation_id"], 256)
    if payload["reply_id"] is not None:
        bounded(payload["reply_id"], 256)
    signature = payload["attention_signature"]
    if signature is not None and (not isinstance(signature, str) or not re.fullmatch(r"[0-9a-f]{64}", signature)):
        raise ValueError("invalid attention signature")


def mark_read_result(result, payload):
    """Only a native `hgs read` answer for this conversation is a receipt."""
    if result == {"ok": False, "error": "not_found"}:
        return True
    if not isinstance(result, dict) or result.get("ok") is not True:
        return False
    applied, read = result.get("applied"), result.get("read")
    return (isinstance(applied, dict) and set(applied) == {"reply_id", "attention_signature"}
            and all(type(value) is bool for value in applied.values())
            and (read is None or isinstance(read, dict) and read.get("conversation_id") == payload["conversation_id"]))
```

In `services/mobile/zerus_mobile/server.py`, import `validate_mark_read` with the other context imports (line 22) and add to `validate_request`, before `if op == "recovery_action":`:

```python
    if op == "mark_read":
        validate_mark_read(payload)
        return
```

- [ ] **Step 4: Gate, validate and execute in the connector**

In `services/mobile/zerus_mobile/connector.py`:

1. Import line 31: add `mark_read_result, validate_mark_read` to the `from .context import …` list.
2. `_init_machine_context`: add `self.read_supported = False` after `self.worktree_supported = False`.
3. `probe_capabilities`: add `self.read_supported = False` with the other resets, and after the `worktree_supported` assignment:

```python
            self.read_supported = isinstance(help_text, str) and "read <session> --json" in help_text
```

4. Line 448: append `- (set() if self.read_supported else {"mark_read"})` to the `operations = …` expression.
5. `native` and `_native` gain `failure_json: bool = False` and pass it through:

```python
    async def native(self, argv: list[str], payload: dict | None = None,
                     *, timeout: float | None = None, json_output: bool = True,
                     failure_json: bool = False) -> object:
        ...
        return await self._native(argv, payload, timeout=timeout, json_output=json_output,
                                  failure_json=failure_json)

    async def _native(self, argv: list[str], payload: dict | None = None,
                      *, timeout: float | None = None, json_output: bool = True,
                      max_output_bytes: int | None = None, failure_json: bool = False) -> object:
```

and replace `if code != 0:` (line 378) with:

```python
            # `hgs read` answers an unknown session with JSON and exit status 1.
            if code != 0 and not (failure_json and stdout.strip()):
```

6. `validate`: after the `if "archive_id" in request:` check (lines 573-574) insert:

```python
        if operation == "mark_read":
            if session not in self.sessions:
                raise ConnectorError("session must match an exact nonarchived snapshot entry")
            try:
                validate_mark_read(request.get("payload"))
            except ValueError as error:
                raise ConnectorError(str(error)) from None
            return operation, session, request["payload"]
```

7. `_execute_claimed`: next to the other ABI gates (after the worktree gate, line 750):

```python
            if operation == "mark_read" and not self.read_supported:
                raise ConnectorError("native shared read ABI is unavailable")
```

and before `elif operation in {"process_output", "process_stop"}:` (line 976):

```python
            elif operation == "mark_read":
                result = await self.native(["read", session, "--json"], payload, failure_json=True)
                if not mark_read_result(result, payload):
                    raise ConnectorError("native read acknowledgement did not match")
```

In `services/mobile/zerus_mobile/fleet.py`, give the peer callback the same keyword and pass it on:

```python
        async def native(argv, payload=None, *, timeout=None, json_output=True, failure_json=False):
            ...
                    return await self.connector._native(["swarm", "mobile-peer", "--json"], envelope,
                        timeout=budget, json_output=json_output, failure_json=failure_json,
                        max_output_bytes=MAX_PEER_SNAPSHOT if argv == ["ls", "--json", "--local"] else None)
```

- [ ] **Step 5: Teach the fixtures `hgs read`**

`services/mobile/tests/fixture_hgs.py`: append `\nhgs [@host] read <session> --json mark the current reply/attention read` to the `--help` string (line 253), and add before the final `else: raise SystemExit("unsupported fixture operation")`:

```python
    elif len(args) == 3 and args[0] == "read" and args[2] == "--json":
        payload = json.load(sys.stdin)
        row = next((row for row in [record, *states] if row["name"] == args[1] and row.get("state") != "archived"), None)
        if row is None or fixture_config.get("read_not_found"):
            print(json.dumps({"ok": False, "error": "not_found"}))
            raise SystemExit(1)
        current = {**row, **fixture_config.get("read_current", {})}
        same = payload["run_id"] == row["run_id"] and payload["conversation_id"] == row["conversation_id"]
        applied = {field: same and payload[field] is not None and payload[field] == current.get(field)
                   for field in ("reply_id", "attention_signature")}
        with (root / "mutations.jsonl").open("a") as output:
            output.write(json.dumps({"operation": "read", "name": args[1], "payload": payload}) + "\n")
        result = {"ok": True, "applied": applied, "read": {
            "conversation_id": row["conversation_id"],
            "reply_id": payload["reply_id"] if applied["reply_id"] else None,
            "attention_signature": payload["attention_signature"] if applied["attention_signature"] else None,
            "at": time.time()} if any(applied.values()) else None}
```

`services/mobile/tests/fixture_peer_hgs.py`: add `\nread <session> --json` to the `--help` output (line 36) and, before the final `else:`:

```python
elif argv[0] == 'read':
    with (root / 'native_effects.jsonl').open('a') as log:
        log.write(json.dumps({'argv': argv, 'machine': machine, 'payload': payload}) + '\n')
    applied = {'reply_id': payload['reply_id'] == 'reply-' + run, 'attention_signature': False}
    print(json.dumps({'ok': True, 'applied': applied, 'read': {'conversation_id': conversation,
        'reply_id': payload['reply_id'], 'attention_signature': None, 'at': 1.0} if applied['reply_id'] else None}))
```

- [ ] **Step 6: Run the tests**

Run: `python -m unittest discover -s services/mobile/tests -p 'test_mobile_read.py' -v && python -m unittest discover -s services/mobile/tests -p 'test_connector_peers.py' -v && python -m unittest discover -s services/mobile/tests -p 'test_relay.py' -v && python -m unittest discover -s services/mobile/tests -p 'test_mobile_context.py' -v`
Expected: all PASS (`test_fixture_telemetry_passthrough_capabilities_and_all_states` still sees every operation because the fixture help now lists `read`).

- [ ] **Step 7: Document the operation**

`docs/mobile-architecture.md` line 632: change "`worktree_create` and `recovery_action`." to "`worktree_create`, `recovery_action` and `mark_read`."

`services/mobile/README.md`, after the `recovery_action` paragraph (line 580):

```markdown
`mark_read` targets one exact session and carries only `run_id`,
`conversation_id`, `reply_id` and `attention_signature` (`null` for a field that
is not being marked; the signature is lowercase hex SHA-256). The connector
advertises it only when native help lists `read <session> --json` and runs
`hgs read <session> --json` on the owning computer, locally or through the
guarded peer route. The result is the native answer: `{"ok": true, "applied":
…, "read": …}` or `{"ok": false, "error": "not_found"}`. Marks are idempotent
and forward-only on the host, so a phone may send a newer mark under a new
request ID; an existing request ID is never replayed.
```

- [ ] **Step 8: Commit**

```bash
git add services/mobile/tests/test_mobile_read.py
git commit --only -m "Run mark_read through the connector on the owning computer" -- services/mobile/zerus_mobile/context.py services/mobile/zerus_mobile/server.py services/mobile/zerus_mobile/connector.py services/mobile/zerus_mobile/fleet.py services/mobile/tests/fixture_hgs.py services/mobile/tests/fixture_peer_hgs.py services/mobile/tests/test_mobile_read.py services/mobile/tests/test_connector_peers.py services/mobile/tests/test_relay.py services/mobile/tests/test_rust_relay.py services/mobile/README.md docs/mobile-architecture.md
```

---

### Task 5: Desktop `HgsClient`: shared read fields and `hgs read` requests

**Files:**
- Modify: `tray/src/HgsClient.h:28-32` (`SessionInfo`), `:157` (public API), `:205-212` (signals), `:283` (members)
- Modify: `tray/src/HgsClient.cpp:87` (`sessionFromJson`), end of file (`requestRead`)
- Test: `tray/tests/test_hgsclient.cpp`

**Interfaces:**
- Consumes: C1 listing fields and C1 CLI.
- Produces (used by Tasks 6-8):
  - `SessionInfo::sharedRead` (`bool`; true when the session object has the key `attention_signature`), `SessionInfo::attentionSignature`, `SessionInfo::readReplyId`, `SessionInfo::readAttentionSignature` (`QString`; read fields only when `read.conversation_id` equals the session's conversation).
  - `quint64 HgsClient::requestRead(const QString &host, const QString &name, const QJsonObject &payload);`
  - `signal void readFinished(quint64 request, bool delivered, const QJsonObject &result, const QString &error);` — `delivered` is true for a valid C1 answer, including `not_found`.

- [ ] **Step 1: Write the failing tests**

Declare two slots in `TestHgsClient` (`tray/tests/test_hgsclient.cpp`), `void parsesSharedReadState();` and `void readMarksUseStdinAndAcceptOnlyHostAnswers();`, and add:

```cpp
void TestHgsClient::parsesSharedReadState()
{
    const QString signature(64, 'a');
    const QJsonObject current{{"name", "codex/p/a"}, {"conversation_id", "c1"}, {"reply_id", "7:100.5"},
        {"attention_signature", signature}, {"read", QJsonObject{{"conversation_id", "c1"}, {"reply_id", "7:100.5"},
        {"attention_signature", QJsonValue()}, {"at", 1.0}}}};
    const QJsonObject otherConversation{{"name", "codex/p/b"}, {"conversation_id", "c2"}, {"attention_signature", QJsonValue()},
        {"read", QJsonObject{{"conversation_id", "old"}, {"reply_id", "1:1.0"}, {"attention_signature", QJsonValue()}, {"at", 1.0}}}};
    const QJsonObject olderHost{{"name", "codex/p/c"}, {"conversation_id", "c3"}};
    QString error;
    const auto box = HgsClient::parseBox(QJsonDocument(QJsonObject{{"host", "arch"}, {"ok", true},
        {"sessions", QJsonArray{current, otherConversation, olderHost}}}).toJson(), &error);
    QVERIFY(error.isEmpty());
    QVERIFY(box.sessions[0].sharedRead); QCOMPARE(box.sessions[0].attentionSignature, signature);
    QCOMPARE(box.sessions[0].readReplyId, QString("7:100.5")); QVERIFY(box.sessions[0].readAttentionSignature.isEmpty());
    QVERIFY(box.sessions[1].sharedRead); QVERIFY(box.sessions[1].readReplyId.isEmpty());
    QVERIFY(!box.sessions[2].sharedRead);
}

void TestHgsClient::readMarksUseStdinAndAcceptOnlyHostAnswers()
{
    QTemporaryDir directory; QFile script(directory.filePath("hgs")); QVERIFY(script.open(QIODevice::WriteOnly));
    script.write(R"(#!/usr/bin/env python3
import json,pathlib,sys
p=json.load(sys.stdin)
pathlib.Path(__file__).with_name('read.json').write_text(json.dumps({'argv':sys.argv[1:],'payload':p}))
mode=p['reply_id']
if mode=='missing': print(json.dumps({'ok':False,'error':'not_found'}));sys.exit(1)
if mode=='offline': print('ssh: connect to host mac port 22: Connection refused',file=sys.stderr);sys.exit(255)
if mode=='garbage': print('not json');sys.exit(0)
read={'conversation_id':'other' if mode=='wrong' else p['conversation_id'],'reply_id':p['reply_id'],'attention_signature':None,'at':1.0}
print(json.dumps({'ok':True,'applied':{'reply_id':True,'attention_signature':False},'read':read}))
)");
    script.close(); QVERIFY(script.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner));
    HgsClient client(script.fileName()); QSignalSpy done(&client, &HgsClient::readFinished);
    const auto payload = [](const QString &reply) {
        return QJsonObject{{"run_id", "run-1"}, {"conversation_id", "conversation-1"}, {"reply_id", reply}, {"attention_signature", QJsonValue()}};
    };
    const QString name = "codex/project '$HOME' `literal`";
    const auto id = client.requestRead("mac", name, payload("7:100.5"));
    QTRY_COMPARE(done.size(), 1); QCOMPARE(done[0][0].toULongLong(), id); QVERIFY(done[0][1].toBool());
    QFile log(directory.filePath("read.json")); QVERIFY(log.open(QIODevice::ReadOnly));
    const auto call = QJsonDocument::fromJson(log.readAll()).object();
    QCOMPARE(call.value("argv").toArray(), QJsonArray({"@mac", "read", name, "--json"}));
    QCOMPARE(call.value("payload").toObject(), payload("7:100.5"));
    client.requestRead({}, "codex/project", payload("missing")); QTRY_COMPARE(done.size(), 2);
    QVERIFY(done[1][1].toBool()); QCOMPARE(done[1][2].toJsonObject(), (QJsonObject{{"ok", false}, {"error", "not_found"}}));
    for (const char *mode : {"offline", "garbage", "wrong"}) {
        const int before = done.size(); client.requestRead({}, "codex/project", payload(mode));
        QTRY_COMPARE(done.size(), before + 1); QVERIFY(!done.last()[1].toBool()); QVERIFY(!done.last()[3].toString().isEmpty());
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cmake --build tray/build --target test_hgsclient -j8`
Expected: compile errors `no member named 'sharedRead' in 'SessionInfo'` and `no member named 'requestRead'`.

- [ ] **Step 3: Implement parsing and the request**

`tray/src/HgsClient.h`, in `SessionInfo` after `bool reviewLater = false, attentionAcknowledged = false;`:

```cpp
    // Shared read state from the session's host. sharedRead is false for hosts
    // whose hgs predates `hgs read`; those keep this device's own marks.
    bool sharedRead = false;
    QString attentionSignature, readReplyId, readAttentionSignature;
```

Public API after `requestAnswerQuestion(...)`:

```cpp
    // `hgs [@host] read <session> --json`: the host applies each field only while
    // it is current. A JSON answer, including not_found, is delivered.
    quint64 requestRead(const QString &host, const QString &name, const QJsonObject &payload);
```

Signal after `questionAnswerFailed(...)`:

```cpp
    void readFinished(quint64 request, bool delivered, const QJsonObject &result, const QString &error);
```

Member after `quint64 m_answerRequest = 0;`: `quint64 m_readRequest = 0;`

`tray/src/HgsClient.cpp`, in `sessionFromJson` after the `attentionId` line:

```cpp
    s.sharedRead = obj.contains(QStringLiteral("attention_signature"));
    s.attentionSignature = obj.value(QStringLiteral("attention_signature")).toString();
    const auto read = obj.value(QStringLiteral("read")).toObject();
    if (!s.conversationId.isEmpty() && read.value("conversation_id").toString() == s.conversationId) {
        s.readReplyId = read.value("reply_id").toString();
        s.readAttentionSignature = read.value("attention_signature").toString();
    }
```

and at the end of the file:

```cpp
quint64 HgsClient::requestRead(const QString &host, const QString &name, const QJsonObject &payload)
{
    const auto request = ++m_readRequest;
    QStringList args{"read", name, "--json"}; if (!host.isEmpty()) args.prepend('@' + host);
    const auto input = QJsonDocument(payload).toJson(QJsonDocument::Compact);
    auto *process = new QProcess(this); configureProcess(process, m_hgs, args);
    auto *timer = new QTimer(process); timer->setSingleShot(true);
    auto reported = std::make_shared<bool>(false);
    const auto fail = [this, request, reported](const QString &error) {
        if (*reported) return; *reported = true; emit readFinished(request, false, {}, error);
    };
    connect(process, &QProcess::started, process, [process, input, fail] {
        if (process->write(input) != input.size()) { fail(tr("Read state transfer failed.")); process->kill(); }
        process->closeWriteChannel();
    });
    connect(timer, &QTimer::timeout, process, [process, fail] { fail(tr("Saving read state timed out.")); process->kill(); });
    connect(process, &QProcess::errorOccurred, process, [process, timer, fail](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart) { timer->stop(); fail(process->errorString()); process->deleteLater(); }
    });
    connect(process, &QProcess::finished, process, [this, process, timer, fail, reported, request, payload](int code, QProcess::ExitStatus status) {
        timer->stop();
        if (!*reported) {
            QJsonParseError parse; const auto result = QJsonDocument::fromJson(process->readAllStandardOutput(), &parse).object();
            const auto applied = result.value("applied").toObject();
            const bool notFound = code == 1 && result == QJsonObject{{"ok", false}, {"error", "not_found"}};
            const bool answered = code == 0 && result.value("ok") == true && applied.value("reply_id").isBool()
                && applied.value("attention_signature").isBool()
                && (result.value("read").isNull()
                    || result.value("read").toObject().value("conversation_id") == payload.value("conversation_id"));
            if (status == QProcess::NormalExit && parse.error == QJsonParseError::NoError && (notFound || answered)) {
                *reported = true; emit readFinished(request, true, result, {});
            } else {
                const auto error = QString::fromUtf8(process->readAllStandardError()).trimmed().left(2000);
                fail(error.isEmpty() ? tr("Could not save read state on that machine.") : error);
            }
        }
        process->deleteLater();
    });
    timer->start(host.isEmpty() ? 15000 : 35000); process->start(); return request;
}
```

- [ ] **Step 4: Run the tests**

Run: `cmake --build tray/build --target test_hgsclient -j8 && QT_QPA_PLATFORM=offscreen tray/build/tests/test_hgsclient`
Expected: PASS (all existing functions plus the two new ones).

- [ ] **Step 5: Commit**

```bash
git commit --only -m "Parse shared read state and write hgs read marks from the desktop" -- tray/src/HgsClient.h tray/src/HgsClient.cpp tray/tests/test_hgsclient.cpp
```

---

### Task 6: Desktop `FleetState`: shared unread/acknowledged state, queued marks, migration marks

**Files:**
- Create: `tray/src/ReadMarks.h` (header-only, so every target that compiles `FleetState.cpp` needs no CMake change)
- Modify: `tray/src/FleetState.h`, `tray/src/FleetState.cpp:38-157`
- Test: `tray/tests/test_fleetstate.cpp`

**Interfaces:**
- Consumes: Task 5 `SessionInfo` fields.
- Produces (used by Tasks 7-8):
  - `struct ReadMark { QString host, name, runId, conversationId, replyId, attentionSignature; QString key() const; bool isEmpty() const; QJsonObject toJson() const; static ReadMark fromJson(const QJsonObject &); QJsonObject payload() const; static ReadMark merged(const ReadMark &old, const ReadMark &next); bool operator==(const ReadMark &) const; }` — `key()` is the compact JSON `[host, name, conversationId]`; `payload()` is C1 stdin with empty strings as `null`.
  - `namespace ReadMarkStore { QJsonObject load(); void save(const QJsonObject &); void add(const QList<ReadMark> &); void remove(const QList<ReadMark> &); bool migrated(const QString &host); void setMigrated(const QString &host); }` — `QSettings` keys `attention/pendingReads` and `attention/sharedReadMigrated`.
  - `FleetState::setPendingReads(const QJsonObject &)`, `pendingReads() const`, `takeNewReadMarks()`, `takeSettledReadMarks()`, `legacyReadMarks(const QString &host) const`, `supportsSharedRead(const QString &host) const`.

- [ ] **Step 1: Write the failing tests**

In `tray/tests/test_fleetstate.cpp`, add a helper after `box()`:

```cpp
static SessionInfo shared(const QString &reply = "7:100.5", const QString &signature = {})
{
    SessionInfo s; s.name = "codex/project/review"; s.cmd = "codex"; s.state = "running"; s.runId = "run-1";
    s.conversationId = "conversation-1"; s.created = 100; s.replyId = reply; s.sharedRead = true;
    s.attentionSignature = signature;
    if (!signature.isEmpty()) { s.phase = "input"; s.attentionId = "call-1"; }
    return s;
}
```

declare six slots (`sharedReadComesFromTheSessionHost`, `sharedMarksApplyAtOnceAndSettleFromSnapshots`, `queuedMarksNeverHideNewerRepliesOrRequests`, `legacyMarksMigrateOnlyCurrentValues`, `olderHostsKeepLocalReadStateAndQueueNothing`, `recoveryAttentionWithoutSignatureStaysLocal`) and add:

```cpp
void TestFleetState::sharedReadComesFromTheSessionHost()
{
    BoxState local = box("arch", true, 0); local.sessions = {shared()};
    FleetState f; f.setReadReplies({{"[\"\",\"codex\",\"conversation-1\"]", "7:100.5"}}); // an old local mark
    f.setLocal(local, 1000);
    QVERIFY(f.local().sessions[0].unreadReply); // the host has no mark: unread on every device
    local.sessions[0].readReplyId = "7:100.5"; f.setLocal(local, 2000);
    QVERIFY(!f.local().sessions[0].unreadReply);
    auto peer = local; peer.host = "mac"; peer.sessions[0].readReplyId.clear(); f.setPeer(peer, 2000);
    QVERIFY(f.peer("mac")->sessions[0].unreadReply);
}

void TestFleetState::sharedMarksApplyAtOnceAndSettleFromSnapshots()
{
    const QString signature(64, 'a');
    BoxState local = box("arch", true, 0); local.sessions = {shared("7:100.5", signature)};
    FleetState f; f.setLocal(local, 1000);
    const auto displayed = f.local().sessions[0];
    QVERIFY(displayed.needsAttention());
    QVERIFY(f.markSessionRead({}, displayed));
    QVERIFY(!f.local().sessions[0].needsAttention());
    QVERIFY(!f.takeNewReadMarks().isEmpty()); QVERIFY(f.takeNewReadMarks().isEmpty());
    const ReadMark key{{}, displayed.name, {}, displayed.conversationId, {}, {}};
    const auto queued = ReadMark::fromJson(f.pendingReads().value(key.key()).toObject());
    QCOMPARE(queued.runId, QString("run-1")); QCOMPARE(queued.replyId, QString("7:100.5"));
    QCOMPARE(queued.attentionSignature, signature);
    QCOMPARE(queued.payload(), (QJsonObject{{"run_id", "run-1"}, {"conversation_id", "conversation-1"},
        {"reply_id", "7:100.5"}, {"attention_signature", signature}}));
    // The host has not stored it yet: the queued mark keeps the session read.
    f.setLocal(local, 2000); QVERIFY(!f.local().sessions[0].needsAttention()); QVERIFY(f.takeSettledReadMarks().isEmpty());
    FleetState restarted; restarted.setPendingReads(f.pendingReads()); restarted.setLocal(local, 3000);
    QVERIFY(!restarted.local().sessions[0].needsAttention());
    // Once the host shows the mark, the queue entry is finished.
    local.sessions[0].readReplyId = "7:100.5"; local.sessions[0].readAttentionSignature = signature; f.setLocal(local, 4000);
    QCOMPARE(f.takeSettledReadMarks().size(), 1); QVERIFY(f.pendingReads().isEmpty());
    QVERIFY(!f.local().sessions[0].needsAttention());
}

void TestFleetState::queuedMarksNeverHideNewerRepliesOrRequests()
{
    const QString first(64, 'a'), second(64, 'b');
    BoxState local = box("arch", true, 0); local.sessions = {shared("7:100.5", first)};
    FleetState f; f.setLocal(local, 1000); const auto displayed = f.local().sessions[0];
    // A newer request arrived after the user opened the menu.
    local.sessions[0].attentionSignature = second; f.setLocal(local, 1500);
    QVERIFY(!f.markSessionRead({}, displayed)); QVERIFY(f.takeNewReadMarks().isEmpty());
    local.sessions[0].attentionSignature = first; f.setLocal(local, 1600);
    QVERIFY(f.markSessionRead({}, f.local().sessions[0]));
    local.sessions[0].replyId = "8:200.5"; local.sessions[0].attentionSignature = second; local.sessions[0].attentionId = "call-2";
    f.setLocal(local, 2000);
    QVERIFY(f.local().sessions[0].unreadReply); QVERIFY(!f.local().sessions[0].attentionAcknowledged);
    QCOMPARE(f.takeSettledReadMarks().size(), 1); // the host would report both fields stale
    // Marks for a session the host no longer lists are finished too.
    f.setLocal(local, 2100); QVERIFY(f.markReplyRead({}, local.sessions[0].name, "conversation-1", "8:200.5"));
    local.sessions.clear(); f.setLocal(local, 2200);
    QCOMPARE(f.takeSettledReadMarks().size(), 1); QVERIFY(f.pendingReads().isEmpty());
}

void TestFleetState::legacyMarksMigrateOnlyCurrentValues()
{
    const QString signature(64, 'a');
    BoxState local = box("arch", true, 0); local.sessions = {shared("7:100.5", signature)};
    auto other = shared("3:30.5"); other.name = "codex/project/old"; other.conversationId = "conversation-2";
    local.sessions << other;
    auto unshared = local; for (auto &s : unshared.sessions) s.sharedRead = false;
    unshared.sessions[1].replyId = "2:20.5"; // the old GUI read an earlier reply
    FleetState before; before.setLocal(unshared, 1000);
    QVERIFY(before.markSessionRead({}, before.local().sessions[0]));
    QVERIFY(before.markReplyRead({}, other.name, other.conversationId, "2:20.5"));
    QVERIFY(before.takeNewReadMarks().isEmpty()); // older hosts never get shared marks
    FleetState f; f.setReadReplies(before.readReplies()); f.setAttentionMarks(before.attentionMarks()); f.setLocal(local, 2000);
    QVERIFY(f.supportsSharedRead({})); QVERIFY(!f.supportsSharedRead("mac"));
    const auto marks = f.legacyReadMarks({});
    QCOMPARE(marks.size(), 1);
    QCOMPARE(marks[0].name, local.sessions[0].name); QCOMPARE(marks[0].runId, QString("run-1"));
    QCOMPARE(marks[0].replyId, QString("7:100.5")); QCOMPARE(marks[0].attentionSignature, signature);
}

void TestFleetState::olderHostsKeepLocalReadStateAndQueueNothing()
{
    BoxState local = box("arch", true, 0); auto s = shared(); s.sharedRead = false; local.sessions = {s};
    FleetState f; f.setLocal(local, 1000); QVERIFY(f.local().sessions[0].unreadReply);
    QVERIFY(f.markReplyRead({}, s.name, s.conversationId, s.replyId));
    QVERIFY(!f.local().sessions[0].unreadReply);
    QVERIFY(f.takeNewReadMarks().isEmpty()); QVERIFY(f.pendingReads().isEmpty());
    QVERIFY(!f.supportsSharedRead({}));
}

void TestFleetState::recoveryAttentionWithoutSignatureStaysLocal()
{
    BoxState local = box("arch", true, 0); auto s = shared(); s.phase = "working"; s.recovery = {{"state", "blocked"}};
    local.sessions = {s}; FleetState f; f.setLocal(local, 1000);
    QVERIFY(f.local().sessions[0].needsAction());
    QVERIFY(f.markSessionRead({}, f.local().sessions[0]));
    QVERIFY(f.local().sessions[0].attentionAcknowledged);
    f.setLocal(local, 2000); QVERIFY(f.local().sessions[0].attentionAcknowledged);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cmake --build tray/build --target test_fleetstate -j8`
Expected: compile errors `unknown type name 'ReadMark'`, `no member named 'takeNewReadMarks'`.

- [ ] **Step 3: Create `tray/src/ReadMarks.h`**

```cpp
#pragma once

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QList>
#include <QSettings>
#include <QString>

// One read mark for a session conversation, written to the session's own host
// with `hgs [@host] read`. The host applies a field only while it is still the
// session's current value, so a late mark never hides a newer reply or request.
struct ReadMark {
    QString host, name, runId, conversationId, replyId, attentionSignature;

    QString key() const {
        return QString::fromUtf8(QJsonDocument(QJsonArray{host, name, conversationId}).toJson(QJsonDocument::Compact));
    }
    bool isEmpty() const { return replyId.isEmpty() && attentionSignature.isEmpty(); }
    bool operator==(const ReadMark &other) const {
        return host == other.host && name == other.name && runId == other.runId && conversationId == other.conversationId
            && replyId == other.replyId && attentionSignature == other.attentionSignature;
    }
    QJsonObject toJson() const {
        return {{"host", host}, {"name", name}, {"run_id", runId}, {"conversation_id", conversationId},
                {"reply_id", replyId}, {"attention_signature", attentionSignature}};
    }
    static ReadMark fromJson(const QJsonObject &value) {
        return {value.value("host").toString(), value.value("name").toString(), value.value("run_id").toString(),
                value.value("conversation_id").toString(), value.value("reply_id").toString(),
                value.value("attention_signature").toString()};
    }
    // C1 stdin; a field that is not being marked is null, which hosts never apply.
    QJsonObject payload() const {
        const auto nullable = [](const QString &value) { return value.isEmpty() ? QJsonValue() : QJsonValue(value); };
        return {{"run_id", runId}, {"conversation_id", conversationId},
                {"reply_id", nullable(replyId)}, {"attention_signature", nullable(attentionSignature)}};
    }
    // A newer mark for the same run keeps the field it does not set.
    static ReadMark merged(const ReadMark &old, const ReadMark &next) {
        if (old.runId != next.runId || old.conversationId != next.conversationId) return next;
        auto result = next;
        if (result.replyId.isEmpty()) result.replyId = old.replyId;
        if (result.attentionSignature.isEmpty()) result.attentionSignature = old.attentionSignature;
        return result;
    }
};

// Marks not yet shown by their hosts' snapshots. Kept in QSettings so a GUI
// restart keeps retrying; the root agent and the workspace window share it.
namespace ReadMarkStore {
inline QJsonObject load() {
    return QJsonDocument::fromJson(QSettings().value("attention/pendingReads").toByteArray()).object();
}
inline void save(const QJsonObject &marks) {
    QSettings().setValue("attention/pendingReads", QJsonDocument(marks).toJson(QJsonDocument::Compact));
}
inline void add(const QList<ReadMark> &marks) {
    if (marks.isEmpty()) return;
    auto stored = load();
    for (const auto &mark : marks)
        if (!mark.isEmpty())
            stored[mark.key()] = ReadMark::merged(ReadMark::fromJson(stored.value(mark.key()).toObject()), mark).toJson();
    save(stored);
}
// Only entries still equal to the settled mark: a newer merged mark survives.
inline void remove(const QList<ReadMark> &marks) {
    if (marks.isEmpty()) return;
    auto stored = load();
    for (const auto &mark : marks)
        if (ReadMark::fromJson(stored.value(mark.key()).toObject()) == mark) stored.remove(mark.key());
    save(stored);
}
inline QJsonArray migratedHosts() {
    return QJsonDocument::fromJson(QSettings().value("attention/sharedReadMigrated").toByteArray()).array();
}
inline bool migrated(const QString &host) { return migratedHosts().contains(host); }
inline void setMigrated(const QString &host) {
    auto hosts = migratedHosts();
    if (!hosts.contains(host)) hosts.append(host);
    QSettings().setValue("attention/sharedReadMigrated", QJsonDocument(hosts).toJson(QJsonDocument::Compact));
}
} // namespace ReadMarkStore
```

- [ ] **Step 4: Extend `FleetState`**

`tray/src/FleetState.h`: add `#include "ReadMarks.h"` after `#include "HgsClient.h"`, and in the public section after `bool setReviewLater(...)`:

```cpp
    // Shared read state. Hosts with `hgs read` own unread/acknowledged state;
    // marks this GUI queued but no snapshot shows yet keep the UI read.
    void setPendingReads(const QJsonObject &pending);
    QJsonObject pendingReads() const { return m_pendingReads; }
    QList<ReadMark> takeNewReadMarks();       // queued by mark*Read since the last call
    QList<ReadMark> takeSettledReadMarks();   // shown by a snapshot or no longer current
    QList<ReadMark> legacyReadMarks(const QString &host) const;   // one-time QSettings migration
    bool supportsSharedRead(const QString &host) const;
```

private section:

```cpp
    void queueRead(const QString &host, const SessionInfo &s, bool reply, bool attention);
    QJsonObject m_pendingReads;
    QList<ReadMark> m_newReads, m_settledReads;
```

`tray/src/FleetState.cpp`: add `#include <QSet>` and `#include <utility>`; in the second anonymous namespace (after `attentionFingerprint`) add:

```cpp
// A queued mark is finished once the host shows it, or once none of its values
// is current any more: the host would report every field stale.
bool settled(const ReadMark &mark, const SessionInfo &s) {
    if (mark.runId != s.runId || mark.conversationId != s.conversationId) return true;
    const bool reply = mark.replyId.isEmpty() || mark.replyId != s.replyId || s.readReplyId == mark.replyId;
    const bool attention = mark.attentionSignature.isEmpty() || mark.attentionSignature != s.attentionSignature
        || s.readAttentionSignature == mark.attentionSignature;
    return reply && attention;
}
```

Replace `FleetState::updateUnread` with:

```cpp
void FleetState::updateUnread(const QString &host, BoxState &box)
{
    QSet<QString> listed;
    for (auto &s : box.sessions) {
        const auto key = attentionKey(host, s), fallback = attentionFallbackKey(host, s);
        if (key != fallback && m_attentionMarks.contains(fallback)) {
            if (!m_attentionMarks.contains(key)) m_attentionMarks[key] = m_attentionMarks.value(fallback);
            m_attentionMarks.remove(fallback);
        }
        const auto mark = m_attentionMarks.value(key).toObject();
        s.reviewLater = s.state != "archived" && mark.value("review").toBool();
        const bool localAcknowledged = mark.value("acknowledged").toString() == attentionFingerprint(s);
        if (!s.sharedRead) {
            // Hosts with an older hgs keep this device's own read state.
            s.unreadReply = s.state != "archived" && !s.conversationId.isEmpty() && !s.replyId.isEmpty()
                && m_readReplies.value(replyKey(host, s)).toString() != s.replyId;
            s.attentionAcknowledged = localAcknowledged;
            continue;
        }
        const auto readKey = ReadMark{host, s.name, {}, s.conversationId, {}, {}}.key();
        listed.insert(readKey);
        auto queued = ReadMark::fromJson(m_pendingReads.value(readKey).toObject());
        if (m_pendingReads.contains(readKey) && settled(queued, s)) {
            m_settledReads << queued; m_pendingReads.remove(readKey); queued = {};
        }
        const bool sameRun = !queued.runId.isEmpty() && queued.runId == s.runId && queued.conversationId == s.conversationId;
        const bool replyRead = s.readReplyId == s.replyId || (sameRun && queued.replyId == s.replyId);
        s.unreadReply = s.state != "archived" && !s.conversationId.isEmpty() && !s.replyId.isEmpty() && !replyRead;
        // Recovery-only attention has no host signature and stays a local choice.
        s.attentionAcknowledged = s.attentionSignature.isEmpty() ? localAcknowledged
            : s.attentionSignature == s.readAttentionSignature || (sameRun && queued.attentionSignature == s.attentionSignature);
    }
    if (!box.ok) return;
    // Marks for sessions this host no longer lists can never apply.
    for (auto it = m_pendingReads.begin(); it != m_pendingReads.end();) {
        const auto queued = ReadMark::fromJson(it.value().toObject());
        if (queued.host == host && !listed.contains(it.key())) { m_settledReads << queued; it = m_pendingReads.erase(it); }
        else ++it;
    }
}
```

In `markSessionRead`, extend the guard and queue before the existing reply mark:

```cpp
    if (!s || s->replyId != expected.replyId || attentionFingerprint(*s) != attentionFingerprint(expected)
        || s->attentionSignature != expected.attentionSignature) return false;
    queueRead(host, *s, true, true);
    markReplyRead(host, s->name, s->conversationId, s->replyId);
```

In `markRepliesRead`, replace the two inner lines

```cpp
            if (replies.value(key).toString() != s.replyId || m_readReplies.value(key).toString() == s.replyId) continue;
            m_readReplies[key] = s.replyId; ++marked;
```

with

```cpp
            if (replies.value(key).toString() != s.replyId) continue;
            if (!s.sharedRead && m_readReplies.value(key).toString() == s.replyId) continue;
            m_readReplies[key] = s.replyId; queueRead(host, s, true, false); ++marked;
```

In `markReplyRead`, replace `m_readReplies[replyKey(host, s)] = reply;` with `m_readReplies[replyKey(host, s)] = reply; queueRead(host, s, true, false);`.

Append the new members:

```cpp
void FleetState::queueRead(const QString &host, const SessionInfo &s, bool reply, bool attention)
{
    if (!s.sharedRead || s.state == "archived" || s.runId.isEmpty() || s.conversationId.isEmpty()) return;
    ReadMark mark{host, s.name, s.runId, s.conversationId,
        reply && s.readReplyId != s.replyId ? s.replyId : QString(),
        attention && s.readAttentionSignature != s.attentionSignature ? s.attentionSignature : QString()};
    if (mark.isEmpty()) return;
    mark = ReadMark::merged(ReadMark::fromJson(m_pendingReads.value(mark.key()).toObject()), mark);
    m_pendingReads[mark.key()] = mark.toJson(); m_newReads << mark;
}
void FleetState::setPendingReads(const QJsonObject &pending)
{
    m_pendingReads = pending; updateUnread({}, m_local);
    for (auto it = m_peers.begin(); it != m_peers.end(); ++it) updateUnread(it.key(), it->box);
}
QList<ReadMark> FleetState::takeNewReadMarks() { return std::exchange(m_newReads, {}); }
QList<ReadMark> FleetState::takeSettledReadMarks() { return std::exchange(m_settledReads, {}); }
bool FleetState::supportsSharedRead(const QString &host) const
{
    const BoxState *box = host.isEmpty() ? &m_local : peer(host);
    return box && box->ok && std::any_of(box->sessions.cbegin(), box->sessions.cend(),
        [](const SessionInfo &s) { return s.sharedRead; });
}
QList<ReadMark> FleetState::legacyReadMarks(const QString &host) const
{
    QList<ReadMark> marks;
    const BoxState *box = host.isEmpty() ? &m_local : peer(host);
    if (!box || !box->ok) return marks;
    for (const auto &s : box->sessions) {
        if (!s.sharedRead || s.state == "archived" || s.runId.isEmpty() || s.conversationId.isEmpty()) continue;
        // Only marks for the current values can still apply on the host.
        ReadMark mark{host, s.name, s.runId, s.conversationId, {}, {}};
        if (!s.replyId.isEmpty() && s.readReplyId != s.replyId && m_readReplies.value(replyKey(host, s)).toString() == s.replyId)
            mark.replyId = s.replyId;
        if (!s.attentionSignature.isEmpty() && s.readAttentionSignature != s.attentionSignature
            && m_attentionMarks.value(attentionKey(host, s)).toObject().value("acknowledged").toString() == attentionFingerprint(s))
            mark.attentionSignature = s.attentionSignature;
        if (!mark.isEmpty()) marks << mark;
    }
    return marks;
}
```

- [ ] **Step 5: Run the tests**

Run: `cmake --build tray/build --target test_fleetstate test_attention -j8 && QT_QPA_PLATFORM=offscreen tray/build/tests/test_fleetstate && QT_QPA_PLATFORM=offscreen tray/build/tests/test_attention`
Expected: PASS, including the unchanged legacy tests (`attentionMarksFollowConversationAndRejectStaleActions`, `unreadRepliesNotifyOnceAcrossResumeAndRead`).

- [ ] **Step 6: Commit**

```bash
git add tray/src/ReadMarks.h
git commit --only -m "Derive desktop unread state from host read marks with an optimistic queue" -- tray/src/ReadMarks.h tray/src/FleetState.h tray/src/FleetState.cpp tray/tests/test_fleetstate.cpp
```

---

### Task 7: Desktop `ReadMarkSync`: retrying writer

**Files:**
- Create: `tray/src/ReadMarkSync.h`, `tray/src/ReadMarkSync.cpp`, `tray/tests/test_readmarksync.cpp`
- Modify: `tray/CMakeLists.txt:52` (add `src/ReadMarkSync.cpp` after `src/FleetState.cpp`), `tray/tests/CMakeLists.txt` (new target after the `test_hgsclient` block)

**Interfaces:**
- Consumes: Task 5 `HgsClient::requestRead` / `readFinished`; Task 6 `ReadMark`.
- Produces (used by Task 8):
  - `class ReadMarkSync : QObject` — `explicit ReadMarkSync(HgsClient *client, QObject *parent = nullptr)`, `void setClock(std::function<qint64()>)`, `void setPending(const QJsonObject &pending)`, `void pump()`, `int inFlight() const`; constants `kFirstBackoffMs = 10000`, `kMaxBackoffMs = 300000`, `kConfirmedResendMs = 120000`, `kMaxInFlight = 4`; signal `markFinished(const QString &key, bool delivered, const QJsonObject &result, const QString &error)`.

- [ ] **Step 1: Write the failing tests**

Create `tray/tests/test_readmarksync.cpp`:

```cpp
#include <QtTest>
#include <QJsonDocument>
#include <QTemporaryDir>

#include "HgsClient.h"
#include "ReadMarkSync.h"

class TestReadMarkSync : public QObject {
    Q_OBJECT
private slots:
    void initTestCase();
    void init();
    void sendsExactPayloadToEachSessionHost();
    void unreachableHostBacksOffAndConfirmedMarksWaitForSnapshots();
    void limitsConcurrentHostWrites();
private:
    QList<QJsonObject> calls() const;
    void touch(const char *name) const {
        QFile file(m_dir.filePath(name));
        if (!file.open(QIODevice::WriteOnly)) QFAIL("cannot create the fixture flag");
    }
    QString hgs() const { return m_dir.filePath("hgs"); }
    QTemporaryDir m_dir;
};

void TestReadMarkSync::initTestCase()
{
    QVERIFY(m_dir.isValid());
    QFile script(hgs()); QVERIFY(script.open(QIODevice::WriteOnly));
    script.write(R"(#!/usr/bin/env python3
import json,pathlib,sys,time
root=pathlib.Path(__file__).parent
payload=json.load(sys.stdin)
with (root/'calls.jsonl').open('a') as log: log.write(json.dumps({'argv':sys.argv[1:],'payload':payload})+'\n')
if (root/'slow').exists(): time.sleep(0.5)
if (root/'fail').exists(): print('ssh: connect to host mac port 22: No route to host',file=sys.stderr); sys.exit(255)
read={'conversation_id':payload['conversation_id'],'reply_id':payload['reply_id'],'attention_signature':payload['attention_signature'],'at':1.0}
print(json.dumps({'ok':True,'applied':{'reply_id':payload['reply_id'] is not None,'attention_signature':payload['attention_signature'] is not None},'read':read}))
)");
    script.close(); QVERIFY(script.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner));
}

void TestReadMarkSync::init()
{
    for (const char *name : {"calls.jsonl", "slow", "fail"}) QFile::remove(m_dir.filePath(name));
}

QList<QJsonObject> TestReadMarkSync::calls() const
{
    QList<QJsonObject> result; QFile file(m_dir.filePath("calls.jsonl"));
    if (!file.open(QIODevice::ReadOnly)) return result;
    for (const auto &line : file.readAll().split('\n')) if (!line.isEmpty()) result << QJsonDocument::fromJson(line).object();
    return result;
}

void TestReadMarkSync::sendsExactPayloadToEachSessionHost()
{
    HgsClient client(hgs()); ReadMarkSync sync(&client); QSignalSpy done(&sync, &ReadMarkSync::markFinished);
    const ReadMark local{{}, "codex/project/review", "run-1", "conversation-1", "7:100.5", {}};
    const ReadMark peer{"mac", "claude/project/fix", "run-2", "conversation-2", {}, QString(64, 'a')};
    sync.setPending({{local.key(), local.toJson()}, {peer.key(), peer.toJson()}});
    QTRY_COMPARE(done.size(), 2); QVERIFY(done[0][1].toBool()); QVERIFY(done[1][1].toBool());
    QHash<QString, QJsonObject> byName;
    for (const auto &call : calls()) { const auto argv = call.value("argv").toArray(); byName.insert(argv.at(argv.size() - 2).toString(), call); }
    QCOMPARE(byName.value(local.name).value("argv").toArray(), QJsonArray({"read", local.name, "--json"}));
    QCOMPARE(byName.value(peer.name).value("argv").toArray(), QJsonArray({"@mac", "read", peer.name, "--json"}));
    QCOMPARE(byName.value(local.name).value("payload").toObject(), (QJsonObject{{"run_id", "run-1"},
        {"conversation_id", "conversation-1"}, {"reply_id", "7:100.5"}, {"attention_signature", QJsonValue()}}));
    QCOMPARE(byName.value(peer.name).value("payload").toObject(), (QJsonObject{{"run_id", "run-2"},
        {"conversation_id", "conversation-2"}, {"reply_id", QJsonValue()}, {"attention_signature", QString(64, 'a')}}));
}

void TestReadMarkSync::unreachableHostBacksOffAndConfirmedMarksWaitForSnapshots()
{
    qint64 now = 1000000;
    HgsClient client(hgs()); ReadMarkSync sync(&client); sync.setClock([&now] { return now; });
    QSignalSpy done(&sync, &ReadMarkSync::markFinished);
    touch("fail");
    const ReadMark mark{"mac", "codex/project/review", "run-1", "conversation-1", "7:100.5", {}};
    sync.setPending({{mark.key(), mark.toJson()}});
    QTRY_COMPARE(done.size(), 1); QVERIFY(!done[0][1].toBool()); QVERIFY(done[0][3].toString().contains("No route"));
    now += ReadMarkSync::kFirstBackoffMs - 1; sync.pump(); QTest::qWait(300); QCOMPARE(calls().size(), 1);
    QFile::remove(m_dir.filePath("fail"));
    now += 1; sync.pump(); QTRY_COMPARE(done.size(), 2); QVERIFY(done[1][1].toBool());
    // Delivered: written again only if no snapshot settles it for two minutes.
    now += ReadMarkSync::kConfirmedResendMs - 1; sync.pump(); QTest::qWait(300); QCOMPARE(calls().size(), 2);
    now += 1; sync.pump(); QTRY_COMPARE(done.size(), 3);
    // A newer mark for the same conversation is written at once.
    auto newer = mark; newer.replyId = "8:200.5"; sync.setPending({{newer.key(), newer.toJson()}});
    QTRY_COMPARE(done.size(), 4);
    QCOMPARE(calls().last().value("payload").toObject().value("reply_id").toString(), QString("8:200.5"));
    // A settled mark leaves the queue and is never written again.
    sync.setPending({}); now += ReadMarkSync::kConfirmedResendMs * 2; sync.pump(); QTest::qWait(300);
    QCOMPARE(calls().size(), 4);
}

void TestReadMarkSync::limitsConcurrentHostWrites()
{
    touch("slow");
    HgsClient client(hgs()); ReadMarkSync sync(&client); QSignalSpy done(&sync, &ReadMarkSync::markFinished);
    QJsonObject pending;
    for (int i = 0; i < 6; ++i) {
        const ReadMark mark{"mac", QString("codex/project/s%1").arg(i), "run", QString("conversation-%1").arg(i), "1:1.0", {}};
        pending[mark.key()] = mark.toJson();
    }
    sync.setPending(pending);
    QCOMPARE(sync.inFlight(), ReadMarkSync::kMaxInFlight);
    QTRY_COMPARE_WITH_TIMEOUT(done.size(), 6, 10000);
    QCOMPARE(calls().size(), 6);
}

QTEST_GUILESS_MAIN(TestReadMarkSync)
#include "test_readmarksync.moc"
```

Register it in `tray/tests/CMakeLists.txt` after the `test_hgsclient` block:

```cmake
add_executable(test_readmarksync test_readmarksync.cpp ../src/ReadMarkSync.cpp ../src/HgsClient.cpp ../src/ProcessRunner.cpp)
target_include_directories(test_readmarksync PRIVATE ../src)
target_link_libraries(test_readmarksync PRIVATE Qt6::Test Qt6::Core)
add_test(NAME readmarksync COMMAND test_readmarksync)
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cmake --build tray/build --target test_readmarksync -j8`
Expected: FAIL — `ReadMarkSync.h: No such file or directory`.

- [ ] **Step 3: Implement the writer**

`tray/src/ReadMarkSync.h`:

```cpp
#pragma once

#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QTimer>

#include <functional>

#include "ReadMarks.h"

class HgsClient;

// Writes queued read marks to their hosts. A mark stays queued until a
// snapshot shows it (FleetState settles it); meanwhile a delivered mark is
// written again only after kConfirmedResendMs, failed writes back off up to
// kMaxBackoffMs, and at most kMaxInFlight writes (SSH to peers) run at once.
class ReadMarkSync : public QObject {
    Q_OBJECT
public:
    static constexpr qint64 kFirstBackoffMs = 10000, kMaxBackoffMs = 300000, kConfirmedResendMs = 120000;
    static constexpr int kMaxInFlight = 4;
    explicit ReadMarkSync(HgsClient *client, QObject *parent = nullptr);
    void setClock(std::function<qint64()> clock) { m_clock = std::move(clock); }
    void setPending(const QJsonObject &pending);
    void pump();
    int inFlight() const { return int(m_requests.size()); }
signals:
    void markFinished(const QString &key, bool delivered, const QJsonObject &result, const QString &error);
private:
    struct Attempt { QJsonObject sent; qint64 dueAt = 0; int failures = 0; bool inFlight = false; };
    void finished(quint64 request, bool delivered, const QJsonObject &result, const QString &error);
    HgsClient *m_client;
    std::function<qint64()> m_clock;
    QJsonObject m_pending;
    QHash<QString, Attempt> m_attempts;
    QHash<quint64, QString> m_requests;
    QTimer m_timer;
};
```

`tray/src/ReadMarkSync.cpp`:

```cpp
#include "ReadMarkSync.h"

#include <QDateTime>

#include <algorithm>

#include "HgsClient.h"

ReadMarkSync::ReadMarkSync(HgsClient *client, QObject *parent)
    : QObject(parent), m_client(client), m_clock([] { return QDateTime::currentMSecsSinceEpoch(); })
{
    // Queued: a process that fails to start may report before pump() has
    // recorded which mark the request belongs to.
    connect(m_client, &HgsClient::readFinished, this, &ReadMarkSync::finished, Qt::QueuedConnection);
    m_timer.setInterval(5000);
    connect(&m_timer, &QTimer::timeout, this, &ReadMarkSync::pump);
    m_timer.start();
}

void ReadMarkSync::setPending(const QJsonObject &pending)
{
    m_pending = pending;
    for (auto it = m_attempts.begin(); it != m_attempts.end();)
        if (!m_pending.contains(it.key()) && !it->inFlight) it = m_attempts.erase(it); else ++it;
    pump();
}

void ReadMarkSync::pump()
{
    const qint64 now = m_clock();
    for (auto it = m_pending.constBegin(); it != m_pending.constEnd() && m_requests.size() < kMaxInFlight; ++it) {
        auto &attempt = m_attempts[it.key()];
        const auto entry = it.value().toObject();
        if (attempt.inFlight || (attempt.sent == entry && now < attempt.dueAt)) continue;
        const auto mark = ReadMark::fromJson(entry);
        if (mark.isEmpty() || mark.name.isEmpty() || mark.runId.isEmpty() || mark.conversationId.isEmpty()) continue;
        if (attempt.sent != entry) attempt.failures = 0;
        attempt.sent = entry; attempt.inFlight = true;
        m_requests.insert(m_client->requestRead(mark.host, mark.name, mark.payload()), it.key());
    }
}

void ReadMarkSync::finished(quint64 request, bool delivered, const QJsonObject &result, const QString &error)
{
    const auto key = m_requests.take(request);
    if (key.isEmpty()) return;
    auto &attempt = m_attempts[key];
    attempt.inFlight = false;
    const qint64 now = m_clock();
    if (delivered) {
        attempt.failures = 0; attempt.dueAt = now + kConfirmedResendMs;
    } else {
        attempt.dueAt = now + std::min(kMaxBackoffMs, kFirstBackoffMs << std::min(attempt.failures, 5));
        ++attempt.failures;
    }
    emit markFinished(key, delivered, result, error);
    pump();
}
```

Add `src/ReadMarkSync.cpp` to the `hgs-tray` sources in `tray/CMakeLists.txt` after `src/FleetState.cpp`.

- [ ] **Step 4: Run the tests**

Run: `cmake --build tray/build --target test_readmarksync -j8 && QT_QPA_PLATFORM=offscreen tray/build/tests/test_readmarksync`
Expected: 3 functions PASS.

- [ ] **Step 5: Commit**

```bash
git add tray/src/ReadMarkSync.h tray/src/ReadMarkSync.cpp tray/tests/test_readmarksync.cpp
git commit --only -m "Retry desktop read-mark writes per host with bounded concurrency" -- tray/src/ReadMarkSync.h tray/src/ReadMarkSync.cpp tray/tests/test_readmarksync.cpp tray/CMakeLists.txt tray/tests/CMakeLists.txt
```

---

### Task 8: Desktop wiring: agent, workspace window, notifications, migration

**Files:**
- Modify: `tray/src/TrayAgent.h` (include, two private functions, member `m_readSync` after `m_state`), `tray/src/TrayAgent.cpp:120-121` (constructor), `:323-336` (`onLocalReady`), `:348-356` (`onPeerReady`), `:525-529` (`showSessions` `publishReadState`)
- Modify: `tray/src/SessionsWindow.cpp:1681` (`setFleet`), `:2343` (`markAllRepliesRead`), `:2373` (`checkViewedReply`), `:3551` (context-menu Mark read)
- Modify: `tray/src/SessionSelection.cpp:230` (batch Mark read)
- Modify: `tray/src/AttentionTracker.cpp:47-73` (`observe`)
- Modify: `docs/reference.md:297-299`
- Test: `tray/tests/test_attention.cpp`, `tray/tests/test_sessionswindow.cpp`

**Interfaces:**
- Consumes: Task 6 `FleetState` shared-read API and `ReadMarkStore`; Task 7 `ReadMarkSync`.
- Produces: `TrayAgent::migrateReadMarks(const QString &host)`, `TrayAgent::publishReadMarks()`; acknowledged attention withdraws its system notification through the existing clear path.

- [ ] **Step 1: Write the failing tests**

`tray/tests/test_attention.cpp`: declare `void acknowledgedAttentionWithdrawsItsNotification();` and add:

```cpp
void TestAttention::acknowledgedAttentionWithdrawsItsNotification()
{
    SessionInfo s; s.name="codex/project/session"; s.cmd="codex"; s.runId="run"; s.conversationId="conversation";
    s.phase="input"; s.activity="busy"; s.attentionId="question-1"; s.sharedRead=true; s.attentionSignature=QString(64,'a');
    BoxState box; box.ok=true; box.host="mac"; box.sessions={s};
    FleetState fleet; fleet.setPeer(box,0); AttentionTracker tracker;
    QCOMPARE(tracker.observe("mac",*fleet.peer("mac")).raised.size(),1);
    // Another device acknowledged exactly this request: the banner goes away.
    box.sessions[0].readAttentionSignature=s.attentionSignature; fleet.setPeer(box,1);
    QVERIFY(fleet.peer("mac")->sessions[0].attentionAcknowledged);
    QCOMPARE(tracker.observe("mac",*fleet.peer("mac")).cleared.size(),1);
    // A new request has a new signature and notifies again.
    box.sessions[0].attentionId="question-2"; box.sessions[0].attentionSignature=QString(64,'b'); fleet.setPeer(box,2);
    QCOMPARE(tracker.observe("mac",*fleet.peer("mac")).raised.size(),1);
}
```

`tray/tests/test_sessionswindow.cpp`: declare `void sharedReadsQueueOnTheSessionHost();` and add:

```cpp
void TestSessionsWindow::sharedReadsQueueOnTheSessionHost()
{
    QSettings().remove("attention/pendingReads");
    auto state = fleet(); auto box = state.local();
    box.sessions[0].runId = "run-local"; box.sessions[0].conversationId = "shared-local";
    box.sessions[0].replyId = "10:100"; box.sessions[0].sharedRead = true;
    state.setLocal(box, QDateTime::currentMSecsSinceEpoch());
    auto remote = *state.peer("mac");
    remote.sessions[0].runId = "run-remote"; remote.sessions[0].conversationId = "shared-remote";
    remote.sessions[0].replyId = "20:200"; remote.sessions[0].sharedRead = true;
    state.setPeer(remote, QDateTime::currentMSecsSinceEpoch());
    SessionsWindow window(script()); window.setFleet(state); window.show();
    auto *action = window.findChild<QAction *>("markAllSessionsRead"); QVERIFY(action); QVERIFY(action->isEnabled());
    action->trigger();
    const auto pending = ReadMarkStore::load();
    const auto local = ReadMark::fromJson(pending.value(ReadMark{{}, box.sessions[0].name, {}, "shared-local", {}, {}}.key()).toObject());
    QCOMPARE(local.runId, QString("run-local")); QCOMPARE(local.replyId, QString("10:100"));
    const auto peer = ReadMark::fromJson(pending.value(ReadMark{"mac", remote.sessions[0].name, {}, "shared-remote", {}, {}}.key()).toObject());
    QCOMPARE(peer.runId, QString("run-remote")); QCOMPARE(peer.replyId, QString("20:200"));
    QSettings().remove("attention/pendingReads");
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cmake --build tray/build --target test_attention test_sessionswindow -j8 && QT_QPA_PLATFORM=offscreen tray/build/tests/test_attention acknowledgedAttentionWithdrawsItsNotification && QT_QPA_PLATFORM=offscreen tray/build/tests/test_sessionswindow sharedReadsQueueOnTheSessionHost`
Expected: `acknowledgedAttentionWithdrawsItsNotification` FAILS (`cleared.size()` is 0); `sharedReadsQueueOnTheSessionHost` FAILS (`local.runId` is empty: the window never stores its marks).

- [ ] **Step 3: Withdraw acknowledged attention notifications**

In `tray/src/AttentionTracker.cpp` `observe`, change `if (s.needsAction()) {` to:

```cpp
        // Acknowledged here or on another device: the existing clear path withdraws it.
        if (s.needsAction() && !s.attentionAcknowledged) {
```

and change the child-loop guard `if (s.state != "running" || s.processState == "exited" || s.activity == "unknown"` to start with `if (s.attentionAcknowledged || s.state != "running" || …` (rest unchanged).

- [ ] **Step 4: Store marks made in the workspace window**

`tray/src/SessionsWindow.cpp`:

- `setFleet` (line 1681): after `m_fleet.setAttentionMarks(QJsonDocument::fromJson(QSettings().value("attention/sessionMarks").toByteArray()).object());` insert ` m_fleet.setPendingReads(ReadMarkStore::load());`.
- `markAllRepliesRead`: after `const int count = m_fleet.markRepliesRead(replies);` insert `ReadMarkStore::add(m_fleet.takeNewReadMarks());`.
- `checkViewedReply`: change the final block to

```cpp
    if (m_fleet.markReplyRead(host, name, conversation, reply)) {
        ReadMarkStore::add(m_fleet.takeNewReadMarks());
        rebuild(); updateDashboard(); emit replyViewed(host, name, conversation, reply);
    }
```

- Context-menu Mark read (after the `if (!changed) { … return; }` block, line 3551): insert `ReadMarkStore::add(m_fleet.takeNewReadMarks());`.

`tray/src/SessionSelection.cpp` (after the `for (const auto &entry : targets) { … }` loop, line 230): insert `ReadMarkStore::add(m_fleet.takeNewReadMarks());`.

(`ReadMarks.h` is included through `FleetState.h`.)

- [ ] **Step 5: Publish, migrate and write marks from the agent**

`tray/src/TrayAgent.h`: `#include "ReadMarkSync.h"`; private functions `void migrateReadMarks(const QString &host);` and `void publishReadMarks();`; member `ReadMarkSync m_readSync;` declared directly after `FleetState m_state;`.

`tray/src/TrayAgent.cpp`:

- Constructor initializer list: after `, m_client(cfg.hgsPath)` add `, m_readSync(&m_client)`.
- `onLocalReady`: after the `m_state.setAttentionMarks(...)` line add `m_state.setPendingReads(ReadMarkStore::load());`; after `m_state.setLocal(box, QDateTime::currentMSecsSinceEpoch());` add `migrateReadMarks({}); publishReadMarks();`.
- `onPeerReady`: before `if (m_state.local().peersKnown && !m_state.local().peers.contains(box.host)) return;` add `m_state.setPendingReads(ReadMarkStore::load());`; after `m_state.setPeer(box, QDateTime::currentMSecsSinceEpoch());` add `migrateReadMarks(box.host); publishReadMarks();` (before `observeAttention`).
- `showSessions` `publishReadState` lambda: insert `publishReadMarks();` before `m_sessionsWindow->setFleet(m_state);`.
- New functions:

```cpp
void TrayAgent::migrateReadMarks(const QString &host)
{
    // Once per host: marks this GUI kept in QSettings move to the host when it
    // first reports shared read state. The old keys stay for older hosts.
    if (ReadMarkStore::migrated(host) || !m_state.supportsSharedRead(host)) return;
    ReadMarkStore::add(m_state.legacyReadMarks(host));
    ReadMarkStore::setMigrated(host);
}

void TrayAgent::publishReadMarks()
{
    ReadMarkStore::add(m_state.takeNewReadMarks());
    if (m_state.local().peersKnown) {
        // Marks for machines removed from the configuration can never be written.
        QList<ReadMark> orphaned;
        const auto peers = m_state.peerNames();
        for (const auto &value : ReadMarkStore::load()) {
            const auto mark = ReadMark::fromJson(value.toObject());
            if (!mark.host.isEmpty() && !peers.contains(mark.host)) orphaned << mark;
        }
        ReadMarkStore::remove(orphaned);
    }
    m_state.setPendingReads(ReadMarkStore::load());
    ReadMarkStore::remove(m_state.takeSettledReadMarks());
    m_readSync.setPending(ReadMarkStore::load());
}
```

- [ ] **Step 6: Update the desktop documentation**

In `docs/reference.md` (line 299) replace "Read state survives rename and resume on this device." with:

```markdown
Read state is kept on the session's host: a reply or request read on any desktop
or phone is read everywhere after the next poll and its notification is
withdrawn; it survives rename and resume. Hosts with an older `hgs` keep read
state on each device.
```

- [ ] **Step 7: Run the desktop suites**

Run: `cmake --build tray/build --target test_attention test_fleetstate test_readmarksync test_hgsclient test_traymenu test_sessionswindow -j8 && for t in attention fleetstate readmarksync hgsclient traymenu; do QT_QPA_PLATFORM=offscreen tray/build/tests/test_$t || exit 1; done && ctest --test-dir tray/build -R '^sessionswindow-' -j10 --output-on-failure`
Expected: PASS. (Building `hgs-tray` itself happens only in `.ci-build/gui`, Task 10.)

- [ ] **Step 8: Commit**

```bash
git commit --only -m "Share desktop read marks through session hosts" -- tray/src/TrayAgent.h tray/src/TrayAgent.cpp tray/src/SessionsWindow.cpp tray/src/SessionSelection.cpp tray/src/AttentionTracker.cpp tray/tests/test_attention.cpp tray/tests/test_sessionswindow.cpp docs/reference.md
```

---

### Task 9: Android: send `mark_read` and show host marks

**Files:**
- Create: `mobile/android/app/src/main/java/app/zerus/mobile/SharedRead.kt`, `mobile/android/app/src/test/java/app/zerus/mobile/SharedReadTest.kt`
- Modify: `mobile/android/app/src/main/java/app/zerus/mobile/RelayApi.kt:113` (after `history`)
- Modify: `mobile/android/app/src/main/java/app/zerus/mobile/ZerusViewModel.kt:104,121-126,303,330-333,346-356,362-369,1407-1408`
- Modify: `mobile/android/app/src/main/java/app/zerus/mobile/MainActivity.kt:596-597`
- Modify: `mobile/android/README.md:171`, `docs/mobile-architecture.md:436-438`
- Test: `SharedReadTest.kt`, `mobile/android/app/src/test/java/app/zerus/mobile/RelaySafetyTest.kt`

**Interfaces:**
- Consumes: C1 listing fields in each computer snapshot row (`Session.raw`), C4 relay operation, existing `relayOperations`/`machineOperations`, `readAllEvidence(target)`, `unreadCount(target)`, `ReadAllPolicies.apply`.
- Produces:
  - `object SharedReadPolicies { fun mark(row: Session): JSONObject?; fun replyRead(row: Session): Boolean; fun attentionAcknowledged(row: Session): Boolean; fun payload(row: Session, freshHead: Boolean, localUnread: Int?): JSONObject? }`
  - `suspend fun RelayApi.markRead(connection: Connection, target: Target, payload: JSONObject): JSONObject`

- [ ] **Step 1: Write the failing tests**

Create `SharedReadTest.kt`:

```kotlin
package app.zerus.mobile

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class SharedReadTest {
    private val target=Target("computer","codex/project/review","run","conversation","gateway")
    private val signature="a".repeat(64)
    private fun row(read:JSONObject?=null,reply:String="7:100.5",attention:String?=signature,t:Target=target)=Session(t,"Review","codex","input","project","",0,
        JSONObject().put("run_id",t.run).put("conversation_id",t.conversation).put("reply_id",reply)
            .put("attention_signature",attention ?: JSONObject.NULL).put("read",read ?: JSONObject.NULL))
    private fun mark(reply:String?,attention:String?,conversation:String="conversation")=JSONObject().put("conversation_id",conversation)
        .put("reply_id",reply ?: JSONObject.NULL).put("attention_signature",attention ?: JSONObject.NULL).put("at",1.0)

    @Test fun payloadCarriesExactIdentityOnlyAfterThisPhoneShowedTheReply() {
        val payload=SharedReadPolicies.payload(row(),true,0)!!
        assertEquals(setOf("run_id","conversation_id","reply_id","attention_signature"),payload.keys().asSequence().toSet())
        assertEquals("run",payload.getString("run_id"));assertEquals("conversation",payload.getString("conversation_id"))
        assertEquals("7:100.5",payload.getString("reply_id"));assertEquals(signature,payload.getString("attention_signature"))
        assertNull(SharedReadPolicies.payload(row(),false,0))   // no fresh head for this exact reply
        assertNull(SharedReadPolicies.payload(row(),true,2))    // newer messages not shown yet
        assertNull(SharedReadPolicies.payload(row(),true,null)) // unknown local count
    }
    @Test fun hostMarksCoverOnlyTheExactCurrentValues() {
        val read=row(mark("7:100.5",signature))
        assertTrue(SharedReadPolicies.replyRead(read));assertTrue(SharedReadPolicies.attentionAcknowledged(read))
        assertNull(SharedReadPolicies.payload(read,true,0))
        val newer=row(mark("7:100.5",signature),reply="8:200.5",attention="b".repeat(64))
        assertFalse(SharedReadPolicies.replyRead(newer));assertFalse(SharedReadPolicies.attentionAcknowledged(newer))
        val partial=SharedReadPolicies.payload(row(mark("7:100.5",null)),true,0)!!
        assertTrue(partial.isNull("reply_id"));assertEquals(signature,partial.getString("attention_signature"))
        assertFalse(SharedReadPolicies.replyRead(row(mark("7:100.5",signature,"other-conversation"))))
    }
    @Test fun childArchivedOrChangedRowsAreNeverMarked() {
        assertNull(SharedReadPolicies.payload(row(t=target.copy(agentId="child",parentConversation="conversation")),true,0))
        assertNull(SharedReadPolicies.payload(row(t=target.copy(archiveId="55555555-5555-4555-8555-555555555555")),true,0))
        val changed=row();changed.raw.put("run_id","resumed-run")
        assertNull(SharedReadPolicies.payload(changed,true,0))
        assertNull(SharedReadPolicies.payload(row(reply="",attention="not-a-signature"),true,0))
    }
}
```

Add to `RelaySafetyTest.kt` (imports `okhttp3.mockwebserver.Dispatcher` and `okhttp3.mockwebserver.RecordedRequest`):

```kotlin
    @Test fun markReadSubmitsExactSessionScopedPayload() = runBlocking {
        MockWebServer().use { server ->
            var envelope = JSONObject()
            server.dispatcher = object : Dispatcher() {
                override fun dispatch(request: RecordedRequest): MockResponse {
                    envelope = JSONObject(request.body.readUtf8())
                    return MockResponse().setBody(JSONObject().put("request_id", envelope.getString("request_id")).put("state", "completed")
                        .put("result", JSONObject().put("ok", true)).put("error", JSONObject.NULL).toString())
                }
            }
            server.start()
            val connection = Connection("gateway", "Test", server.url("/").toString().trimEnd('/'), "token")
            val target = Target("machine", "codex/project/review", "exact-run", "exact-conversation", "gateway")
            val payload = JSONObject().put("run_id", "exact-run").put("conversation_id", "exact-conversation")
                .put("reply_id", "7:100.5").put("attention_signature", JSONObject.NULL)
            assertEquals("completed", RelayApi().markRead(connection, target, payload).getString("state"))
            assertEquals("mark_read", envelope.getString("operation"))
            assertEquals("machine", envelope.getString("computer_id"))
            assertEquals("codex/project/review", envelope.getString("session"))
            val sent = envelope.getJSONObject("payload")
            assertEquals(4, sent.length())
            for (key in listOf("run_id", "conversation_id", "reply_id", "attention_signature")) assertEquals(payload.opt(key), sent.opt(key))
            assertTrue(runCatching { RelayApi().markRead(connection, target.copy(agentId = "child"), payload) }.isFailure)
        }
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run (on `arch`, in `mobile/android`): `JAVA_HOME=$HOME/.cache/zerus-toolchains/jdk-17.0.20.1+1 ANDROID_HOME=$HOME/Android/Sdk ./gradlew testDebugUnitTest --tests app.zerus.mobile.SharedReadTest --tests app.zerus.mobile.RelaySafetyTest`
Expected: compilation FAILS — `Unresolved reference: SharedReadPolicies`, `Unresolved reference: markRead`.

- [ ] **Step 3: Implement the policy and the API call**

Create `SharedRead.kt`:

```kotlin
package app.zerus.mobile

import org.json.JSONObject

/**
 * Read marks shared through the session's computer (`hgs read`, operation `mark_read`).
 * The computer applies a field only while it is current, so marks never hide newer work.
 */
object SharedReadPolicies {
    private val signature=Regex("[0-9a-f]{64}")
    /** The computer's mark for this row's current conversation. */
    fun mark(row:Session):JSONObject?=row.raw.optJSONObject("read")
        ?.takeIf { row.target.conversation.isNotBlank() && it.string("conversation_id")==row.target.conversation }
    /** Exactly the current reply was read on some device. */
    fun replyRead(row:Session):Boolean=row.raw.string("reply_id").let { it.isNotBlank() && mark(row)?.string("reply_id")==it }
    /** Exactly the current request was acknowledged on some device. */
    fun attentionAcknowledged(row:Session):Boolean=
        row.raw.string("attention_signature").let { it.isNotBlank() && mark(row)?.string("attention_signature")==it }
    /**
     * C4 payload once this phone has shown the current reply: a fresh head fetched for this exact
     * reply and read through on this phone. Null when the computer already has everything.
     */
    fun payload(row:Session,freshHead:Boolean,localUnread:Int?):JSONObject? {
        val t=row.target
        if(!freshHead || localUnread!=0 || t.archiveId.isNotBlank() || t.agentId.isNotBlank() || t.run.isBlank() || t.conversation.isBlank() ||
            row.raw.string("run_id")!=t.run || row.raw.string("conversation_id")!=t.conversation || SessionFilters.archived(row)) return null
        val reply=row.raw.string("reply_id").takeIf { it.isNotBlank() && !replyRead(row) }
        val attention=row.raw.string("attention_signature").takeIf { signature.matches(it) && !attentionAcknowledged(row) }
        if(reply==null && attention==null) return null
        return JSONObject().put("run_id",t.run).put("conversation_id",t.conversation)
            .put("reply_id",reply ?: JSONObject.NULL).put("attention_signature",attention ?: JSONObject.NULL)
    }
}
```

In `RelayApi.kt`, after `history(...)`:

```kotlin
    /** Shared read mark; the computer applies only current values, so each attempt has its own request ID. */
    suspend fun markRead(connection: Connection, target: Target, payload: JSONObject): JSONObject {
        require(target.archiveId.isBlank() && target.agentId.isBlank()) { "Read marks belong to the live parent conversation." }
        val id = UUID.randomUUID().toString()
        return await(connection, submit(connection, target, "mark_read", payload, id))
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: same command as Step 2.
Expected: PASS.

- [ ] **Step 5: Wire the view model and the session list**

`ZerusViewModel.kt`:

- After `private var unreadBoundaryCaptured=false` (line 104):

```kotlin
    // Marks submitted by this process; a failed submission is forgotten so the next view retries.
    private val sharedReadsSent=mutableSetOf<String>()
```

- `visibleMessages` (line 125): append `;shareRead(target)` after `refreshReadAttention()`.
- `markRead` (line 332): append `;shareRead(target)` after `refreshReadAttention()`.
- `markAllRead` (after `refreshReadAttention();writer.flushAsync()`): add `distinct.forEach { shareRead(it) }`.
- Replace `refreshReadAttention` (lines 362-369) with:

```kotlin
    private fun refreshReadAttention() {
        sessions=sessions.map { row ->
            val later=reviewLater(row.target)
            // A reply or request read on another device is read here too.
            val unread=!SharedReadPolicies.replyRead(row) && (unreadCount(row.target) ?: loadedUnreadCount(row.target) ?: 0)>0
            val acknowledged=SharedReadPolicies.attentionAcknowledged(row)
            if(row.raw.optBoolean("mobile_review_later")==later && row.raw.optBoolean("mobile_unread_reply")==unread &&
                row.raw.optBoolean("attention_acknowledged")==acknowledged) row
            else row.copy(raw=JSONObject(row.raw.toString()).put("mobile_review_later",later).put("mobile_unread_reply",unread)
                .put("attention_acknowledged",acknowledged))
        }
    }
```

- Add below it:

```kotlin
    /** Tell the session's computer that this phone showed its current reply (C4 `mark_read`). */
    private fun shareRead(target:Target) {
        if(demo || !readTrackingReady) return
        val row=sessions.find { it.target==target } ?: return
        if("mark_read" !in relayOperations[target.connectionId].orEmpty() ||
            "mark_read" !in machineOperations[MachineKey(target.connectionId,target.computerId)].orEmpty()) return
        val payload=SharedReadPolicies.payload(row,readAllEvidence(target)?.authoritative==true,unreadCount(target)) ?: return
        val connection=connections.find { it.id==target.connectionId } ?: return
        val key=target.key+"\n"+payload
        if(!sharedReadsSent.add(key)) return
        viewModelScope.launch {
            val completed=try { api.markRead(connection,target,payload).string("state")=="completed" }
                catch(e:Exception) { if(e is CancellationException) { sharedReadsSent.remove(key);throw e };false }
            if(!completed) sharedReadsSent.remove(key)
        }
    }
    /** A reply read on another device advances this phone's watermark, using only a fresh exact head. */
    private fun adoptSharedReads() {
        val writer=readWriter ?: return
        if(demo || !readTrackingReady) return
        val captured=sessions.filter { SharedReadPolicies.replyRead(it) && (unreadCount(it.target) ?: 0)>0 }
            .mapNotNull { row -> readAllEvidence(row.target)?.takeIf { it.authoritative }?.let { ReadAllCapture(row.target,it) } }
        if(captured.isEmpty()) return
        readState=writer.edit { state -> ReadAllPolicies.apply(state,captured,captured).state }
        refreshReadAttention()
    }
```

- `watchHistoryHeads`: after `historySnapshotHints[ConversationReadPolicies.key(target)]=candidates.first { it.first==target }.second` (line 303) add `adoptSharedReads();if(selected?.target==target) shareRead(target)`.
- Catalog refresh (line 1408): after `refreshReadAttention()` add `adoptSharedReads()`.

`MainActivity.kt` lines 596-597, replace the badge expression with:

```kotlin
                            // A reply read on another device needs no badge here.
                            (if (SharedReadPolicies.replyRead(session)) null else UnreadPresentation.badge(model.unreadCount(session.target), model.loadedUnreadCount(session.target),
                                session.raw.optBoolean("unread_reply") || session.raw.optBoolean("mobile_unread_reply")))?.let { unread ->
```

- [ ] **Step 6: Document phone behavior**

`mobile/android/README.md`, after "…reading anchors and drafts stay put." (line 171):

```markdown
When the computer advertises `mark_read`, reaching the newest reply (a fresh
complete head read through on this phone), Mark read and Read all also send the
exact run, conversation, reply and attention signature to the session's
computer, which shares them with every desktop and phone. A reply or request read
elsewhere is shown as read here. Review later stays on this phone.
```

`docs/mobile-architecture.md` lines 436-438, replace "Mark read and Review later are private phone state." with "Review later is private phone state. Mark read and automatic read progress also send `mark_read` when the computer advertises it, so the session's host marks the exact reply and request read for every device."

- [ ] **Step 7: Run the Android checks**

Run (on `arch`, in `mobile/android`): `JAVA_HOME=$HOME/.cache/zerus-toolchains/jdk-17.0.20.1+1 ANDROID_HOME=$HOME/Android/Sdk ./gradlew testDebugUnitTest lintDebug`
Expected: BUILD SUCCESSFUL.

- [ ] **Step 8: Commit**

```bash
git add mobile/android/app/src/main/java/app/zerus/mobile/SharedRead.kt mobile/android/app/src/test/java/app/zerus/mobile/SharedReadTest.kt
git commit --only -m "Share phone read marks with the session's computer" -- mobile/android/app/src/main/java/app/zerus/mobile/SharedRead.kt mobile/android/app/src/main/java/app/zerus/mobile/RelayApi.kt mobile/android/app/src/main/java/app/zerus/mobile/ZerusViewModel.kt mobile/android/app/src/main/java/app/zerus/mobile/MainActivity.kt mobile/android/app/src/test/java/app/zerus/mobile/SharedReadTest.kt mobile/android/app/src/test/java/app/zerus/mobile/RelaySafetyTest.kt mobile/android/README.md docs/mobile-architecture.md
```

---

### Task 10: Full validation and tracker update

**Files:** none changed except `.beads/issues.jsonl` (export).

- [ ] **Step 1: Local checks on macOS**

Run: `python3 scripts/ci/check-source.py && cargo +1.85.0 test --locked && bash scripts/ci/gui.sh`
Expected: PASS (`gui.sh` builds in `.ci-build/gui`, never in `tray/build`). Record any check that cannot run here.

- [ ] **Step 2: Copy the working tree to a disposable checkout on `arch`**

```bash
ssh -o BatchMode=yes arch 'mkdir -p ~/.cache/zerus-validate'
rsync -a --delete -e 'ssh -o BatchMode=yes' --exclude target/ --exclude .ci-build/ --exclude tray/build/ \
  --exclude services/mobile/relay/target/ --exclude mobile/android/.gradle/ --exclude 'mobile/android/**/build/' \
  ./ arch:.cache/zerus-validate/shared-read/
```

- [ ] **Step 3: Linux CLI, desktop and relay suites**

```bash
ssh -o BatchMode=yes arch 'cd ~/.cache/zerus-validate/shared-read && bash scripts/ci/cli.sh && bash scripts/ci/gui.sh'
ssh -o BatchMode=yes arch 'cd ~/.cache/zerus-validate/shared-read/services/mobile/relay && cargo +1.85.0 fmt --check && cargo +1.85.0 clippy --locked --all-targets -- -D warnings && cargo +1.85.0 test --locked && cargo build --locked'
ssh -o BatchMode=yes arch 'cd ~/.cache/zerus-validate/shared-read && python3 -m venv ../venv && ../venv/bin/pip install -q ./services/mobile && ../venv/bin/python -m unittest discover -s services/mobile/tests -v && ZERUS_RELAY_BINARY=$PWD/services/mobile/relay/target/debug/zerus-relay ../venv/bin/python -m unittest discover -s services/mobile/tests -p test_rust_relay.py -v'
```

Expected: all PASS, including `test_rust_relay.RustHttpContracts.test_mark_read_is_exact_and_capability_gated`.

- [ ] **Step 4: Android, plain and Firebase variants**

```bash
ssh -o BatchMode=yes arch 'cd ~/.cache/zerus-validate/shared-read/mobile/android && JAVA_HOME=$HOME/.cache/zerus-toolchains/jdk-17.0.20.1+1 ANDROID_HOME=$HOME/Android/Sdk ./gradlew testDebugUnitTest lintDebug'
ssh -o BatchMode=yes arch 'cd ~/.cache/zerus-validate/shared-read/mobile/android && cp ~/.config/hgs/mobile/firebase/google-services.json app/ && JAVA_HOME=$HOME/.cache/zerus-toolchains/jdk-17.0.20.1+1 ANDROID_HOME=$HOME/Android/Sdk ./gradlew -PzerusFirebase=true testDebugUnitTest lintDebug'
```

Expected: BUILD SUCCESSFUL twice. The copied `google-services.json` exists only in the disposable checkout.

- [ ] **Step 5: Update Beads and hand off**

Mark the Phase 1 work of epic `zerus-rk5b` done in Beads (see `.agents/skills/beads/SKILL.md`), record unavailable checks in its notes, then `bd export -o .beads/issues.jsonl`. Inspect `git status` and the staged diff. Commit the export only with the owner's authorization; never push or run `bd dolt push` without it.
