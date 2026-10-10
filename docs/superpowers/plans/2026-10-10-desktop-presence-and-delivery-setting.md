# Desktop Presence and Shared Delivery Setting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every Zerus desktop records its owner's input idle time and screen lock through `hgs`, and one swarm-wide "When I'm not at a computer, send notifications to my phone" choice can be read and changed from `hgs`, desktop Settings and the Android app.

**Architecture:** `hgs` is the source of truth. The tray agent samples the OS (CoreGraphics on macOS; D-Bus and KIdleTime on Linux; Zerus-window input as a fallback) and writes `desktop-presence.json` through `hgs presence set`. The delivery choice is a new catalog key (`Preference { field }`) replicated by the existing swarm journal; exchanges with members from before this change omit the new key kind so their project sync keeps working. `hgs ls --json --local` lists both, so the connector's snapshots carry them to the relay unchanged. The phone changes the choice with the additive relay operation `set_mobile_delivery`, executed on the gateway computer by the connector. This is Phase 2 of the spec; relay alert decisions are Phase 3 and read marks are Phase 1.

**Tech Stack:** Rust 2021 (hgs CLI and Axum relay), Python 3.11 (connector, API v1 oracle, CLI integration tests), Qt 6 Widgets/DBus with KDE Frameworks 6 KIdleTime (desktop), CoreGraphics (macOS), Kotlin and Jetpack Compose (Android).

**Spec:** `docs/superpowers/specs/2026-10-10-mobile-presence-notifications-design.md` (Beads epic `zerus-rk5b`)

## Global Constraints

- Shared contracts are fixed. C2: `hgs presence set --json`; stdin `{"idle_seconds": number >= 0, "locked": bool}`; stdout `{"ok": true}`; file `desktop-presence.json` in the hgs state directory, written atomically: `{"idle_seconds": number, "locked": bool, "locked_since": epoch|null, "written_at": epoch}`; `locked_since` is kept while consecutive writes stay locked; `hgs ls --json --local` top-level `"desktop_presence": {"idle_seconds": number aged to listing time, "locked": bool, "locked_seconds": number|null}`, omitted when `written_at` is older than 120 s.
- C3: catalog key variant `Preference { field: String }` (serde tag `"preference"`), field `"mobile_delivery"`, values `"immediate","away","away_5","away_10","away_15","away_30"`, default `"away"`; `hgs swarm preference get --json` → `{"mobile_delivery": "..."}`; `hgs swarm preference set mobile_delivery <value> --json` → `{"ok": true, "mobile_delivery": "..."}`; `hgs ls --json --local` top-level `"preferences": {"mobile_delivery": "..."}`.
- C4: relay operation `"set_mobile_delivery"`: computer-scoped request to the gateway computer, no session (`"session": ""`); payload `{"value": <C3 value>}`; the connector executes `hgs swarm preference set mobile_delivery <value> --json`; the request result is its stdout object.
- Desktop copy, verbatim: "When I'm not at a computer, send notifications to my phone"; choices "Immediately, even if I'm at a computer", "As soon as I'm away" (default), "After 5 minutes away", "After 10 minutes away", "After 15 minutes away", "After 30 minutes away". The phone shows the same six labels.
- Keep the `hgs` CLI, `HGS_*` variables, state/config paths and service identifiers compatible. New commands and fields are additive. Keep API v1 and durable request identities compatible; new relay behavior is additive.
- Tests never restart or kill native agents, tmux servers or DeepSeek hosts and never read live agent state. CLI tests use isolated `HGS_STATE_DIR`/`HGS_CONFIG_DIR` and a fake `tmux` on `PATH`; Qt D-Bus tests run inside `dbus-run-session`.
- Rust: the relay crate keeps Rust 1.85 compatibility: in `services/mobile/relay` run `cargo +1.85.0 fmt --check`, `cargo +1.85.0 clippy --locked --all-targets -- -D warnings`, `cargo +1.85.0 test --locked`. The CLI also runs `cargo +1.85.0 test --locked` at the repository root (CONTRIBUTING.md).
- Relay contract tests fail on the macOS development machine for an unrelated SQLite fixture reason; run relay and Linux validation on the `arch` host (`ssh -o BatchMode=yes arch`) in a disposable copy of the branch. Rust HTTP contracts run with `ZERUS_RELAY_BINARY=<relay target/debug/zerus-relay> python -m unittest discover -s services/mobile/tests -p test_rust_relay.py`.
- Android on `arch`: `JAVA_HOME=$HOME/.cache/zerus-toolchains/jdk-17.0.20.1+1 ANDROID_HOME=$HOME/Android/Sdk ./gradlew testDebugUnitTest lintDebug` from `mobile/android`, then again with `-PzerusFirebase=true`.
- The owner's Linux desktop is Arch with KDE Plasma 6 on Wayland; there `org.freedesktop.ScreenSaver.GetSessionIdleTime` fails with "GetSessionIdleTime is not supported on this platform" (verified 2026-10-10) while `GetActive` works. The Mac runs macOS (deployment target 12.0).
- KDE Frameworks 6 KIdleTime (`kidletime`) becomes a Linux desktop build and runtime dependency; add it wherever `kwindowsystem` is listed (packaging, CI, docs, notices). Do not add paid runners or raise cache limits. Validate workflow edits locally (actionlint when available, `python3 scripts/ci/check-source.py`, `python3 -m unittest discover -s tests -p 'test_ci_security.py'`).
- Repository text is English. Match the surrounding code style and comment density. Use synthetic fixtures and reserved example domains only.
- Commit steps apply only when the owner has authorized commits for this execution; otherwise stage nothing and report the change set. Commit messages end with the attribution trailer the executing session requires. Never push or synchronize Beads without explicit authorization.

## Review Focus

- KDE Plasma Wayland refuses `GetSessionIdleTime`, and a missing KIdleTime plugin never emits events: the source must report "unavailable" (and fall back to away-biased Zerus-window input) rather than idle 0 forever, or the owner looks permanently present and phone alerts are lost. Test: Task 6 `waylandIdleNeedsVerifiedThresholdEvents`.
- A presence file written long ago or with a wall clock that later moved backwards (`written_at` far in the future) must not be listed; a reasonable person expects "unknown = away". Tests: Task 1 `listings_age_values_and_omit_stale_or_future_records` and `test_presence_is_written_atomically_aged_and_expires`.
- One machine still runs an older `hgs` when the owner picks a delivery choice: project and folder sync with that machine must keep working, it simply does not receive the choice, and Settings names it. Test: Task 3 `test_older_members_keep_project_sync_without_the_delivery_choice`.
- The desktop and the phone change the choice while computers are offline: every member must converge on one value without a conflict appearing in the Swarm review. Tests: Task 2 `delivery_preference_validates_merges_and_defaults` and `test_concurrent_delivery_choices_converge_without_conflict_review`.
- The phone is paired with an older relay or gateway connector: the choice must be shown read-only with an explanation and no request is created; an unconfirmed request is never repeated. Tests: Task 9 `olderGatewaysOrRelaysExplainWhyTheChoiceIsReadOnly` and `onlyAnExactNativeAcknowledgementConfirmsTheChange`.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `src/presence.rs` (new) | `hgs presence set --json`, the atomic presence record, the aged `desktop_presence` listing |
| `src/cli.rs` | Usage lines, `presence` dispatch, additive `desktop_presence`/`preferences` in `local_json` |
| `src/swarm/catalog.rs` | `Key::Preference`, value validation, key kinds, `Catalog::preferences()` |
| `src/swarm/mod.rs` | `hgs swarm preference get|set`, lock-free `preferences(config)` for listings |
| `src/swarm/model.rs` | `preferences` in `hgs swarm get`; no conflict review for preferences |
| `src/swarm/store.rs` | `Peer.catalog_outdated` |
| `src/swarm/transport.rs` | Kind-aware `hello`/`exchange`, outdated peers, joining keeps the swarm's choice |
| `src/mobile_peers.rs` | Gateway-guarded native allowlist for `swarm preference set` |
| `services/mobile/relay/src/protocol.rs` | `set_mobile_delivery` in API v1 operations and request validation |
| `services/mobile/zerus_mobile/context.py`, `server.py`, `connector.py` | Shared Python operation set, oracle validation, connector execution and capability |
| `tray/src/DesktopPresence.h/.cpp` (new) | Presence sample type, platform source interface, sampling/transition/fallback controller |
| `tray/src/LinuxPresenceSource.h/.cpp` (new) | Asynchronous D-Bus probes and KIdleTime thresholds |
| `tray/src/MacPresenceSource.cpp` (new) | CoreGraphics idle time and session lock |
| `tray/src/HgsClient.h/.cpp` | Coalesced `hgs presence set` writer |
| `tray/src/TrayAgent.h/.cpp` | Starts presence for the lifetime of the tray agent |
| `tray/src/MobileDeliverySettings.h` (new) | Settings widget for the shared choice and outdated members |
| `tray/src/SettingsPage.h`, `SessionsWindow.cpp` | Place the widget on the Mobile connection page; forward swarm snapshots |
| `mobile/android/.../MobileDelivery.kt` (new) | Pure phone-side rules: values, labels, gateway choice, reasons, receipts, status |
| `mobile/android/.../ZerusViewModel.kt`, `SessionAction.kt`, `Models.kt`, `MachineCatalog.kt` | Snapshot value, durable request and receipt settlement |
| `mobile/android/.../NotificationSettings.kt`, `MachinesUi.kt`, `SessionDetailsUi.kt` | Notifications choice and recovery card label |

---

### Task 1: Record desktop presence in hgs

**Files:**
- Create: `src/presence.rs`
- Modify: `src/main.rs:1-11` (module list)
- Modify: `src/cli.rs:70-71` (USAGE), `src/cli.rs:328-330` (dispatch), `src/cli.rs:632-633` (`local_json`)
- Create: `tests/test_presence.py`
- Modify: `docs/reference.md:1143-1146` (JSON section)

**Interfaces:**
- Consumes: `crate::cli::{Error, Result}`, `crate::config::Config` (`config.state` is the hgs state directory, `HGS_STATE_DIR`).
- Produces: `pub fn crate::presence::dispatch(config: &Config, args: &[String], dry: bool) -> cli::Result<i32>`, `pub fn crate::presence::listing(root: &Path, at: f64) -> Option<serde_json::Value>`, `pub fn crate::presence::now() -> f64`. Tasks 5-7 run `hgs presence set --json`.

- [ ] **Step 1: Write the failing unit tests**

Create `src/presence.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consecutive_locked_writes_keep_the_lock_time() {
        let dir = tempfile::tempdir().unwrap();
        let input = |idle_seconds, locked| Input { idle_seconds, locked };
        assert_eq!(write(dir.path(), &input(10.0, true), 100.0).unwrap().locked_since, Some(100.0));
        assert_eq!(write(dir.path(), &input(40.0, true), 130.0).unwrap().locked_since, Some(100.0));
        assert_eq!(write(dir.path(), &input(0.0, false), 160.0).unwrap().locked_since, None);
        assert_eq!(write(dir.path(), &input(1.0, true), 190.0).unwrap().locked_since, Some(190.0));
    }

    #[test]
    fn listings_age_values_and_omit_stale_or_future_records() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(listing(dir.path(), 100.0), None);
        write(dir.path(), &Input { idle_seconds: 5.0, locked: false }, 100.0).unwrap();
        assert_eq!(
            listing(dir.path(), 130.0),
            Some(json!({"idle_seconds": 35.0, "locked": false, "locked_seconds": null}))
        );
        assert_eq!(listing(dir.path(), 220.0).unwrap()["idle_seconds"], 125.0);
        assert_eq!(listing(dir.path(), 220.5), None);
        // A wall clock moved backwards: presence written "in the future" is unknown.
        assert_eq!(listing(dir.path(), -30.0), None);
        write(dir.path(), &Input { idle_seconds: 0.0, locked: true }, 300.0).unwrap();
        assert_eq!(
            listing(dir.path(), 310.0),
            Some(json!({"idle_seconds": 10.0, "locked": true, "locked_seconds": 10.0}))
        );
    }

    #[test]
    fn input_accepts_only_bounded_idle_seconds_and_a_lock_flag() {
        assert!(parse(br#"{"idle_seconds":0,"locked":false}"#).is_ok());
        assert!(parse(br#"{"idle_seconds":12.5,"locked":true}"#).is_ok());
        let bad: [&[u8]; 7] = [
            br#"{"idle_seconds":-1,"locked":false}"#,
            br#"{"idle_seconds":1}"#,
            br#"{"idle_seconds":"1","locked":false}"#,
            br#"{"idle_seconds":1,"locked":false,"extra":1}"#,
            br#"{"idle_seconds":1e12,"locked":false}"#,
            b"[]",
            b"{",
        ];
        for input in bad {
            assert!(parse(input).is_err(), "{}", String::from_utf8_lossy(input));
        }
    }

    #[test]
    fn unreadable_records_are_replaced_and_never_listed() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE), b"{\"damaged\":").unwrap();
        assert_eq!(listing(dir.path(), 10.0), None);
        let record = write(dir.path(), &Input { idle_seconds: 2.0, locked: true }, 10.0).unwrap();
        assert_eq!(record.locked_since, Some(10.0));
    }
}
```

Add `mod presence;` to `src/main.rs` after `mod platform;`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --locked presence::`
Expected: FAIL to compile with "cannot find type `Input`" and "cannot find function `write`".

- [ ] **Step 3: Implement the module**

Insert above the test module in `src/presence.rs`:

```rust
//! Desktop input idle time and screen lock, written by the Zerus tray agent and
//! listed for the mobile relay. Durations are relative, so readers on other
//! machines need no clock agreement. Nothing else about the desktop is recorded.
use crate::{cli, config::Config};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const FILE: &str = "desktop-presence.json";
/// Listings omit older presence: the GUI closed or crashed, or the machine slept.
const STALE_SECONDS: f64 = 120.0;
const MAX_INPUT_BYTES: u64 = 4096;
const MAX_IDLE_SECONDS: f64 = 10.0 * 365.0 * 86400.0;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    idle_seconds: f64,
    locked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Record {
    idle_seconds: f64,
    locked: bool,
    locked_since: Option<f64>,
    written_at: f64,
}

pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

pub fn dispatch(config: &Config, args: &[String], dry: bool) -> cli::Result<i32> {
    if dry {
        return Err(cli::Error::new(1, "presence does not support --dry-run"));
    }
    if args != ["set", "--json"] {
        return Err(cli::Error::new(1, "usage: hgs presence set --json"));
    }
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| cli::Error::new(1, format!("read presence: {e}")))?;
    let input = parse(&bytes).map_err(|e| cli::Error::new(1, format!("{e:#}")))?;
    write(&config.state, &input, now()).map_err(|e| cli::Error::new(1, format!("{e:#}")))?;
    println!("{}", json!({"ok": true}));
    Ok(0)
}

fn parse(bytes: &[u8]) -> Result<Input> {
    ensure!(
        bytes.len() as u64 <= MAX_INPUT_BYTES,
        "presence input is too large"
    );
    let input: Input = serde_json::from_slice(bytes).context("invalid presence input")?;
    ensure!(
        input.idle_seconds.is_finite() && (0.0..=MAX_IDLE_SECONDS).contains(&input.idle_seconds),
        "idle_seconds must be a non-negative number of seconds"
    );
    Ok(input)
}

/// Consecutive locked writes keep the first lock time; unlocking clears it.
fn next(previous: Option<&Record>, input: &Input, at: f64) -> Record {
    let locked_since = input.locked.then(|| {
        previous
            .filter(|p| p.locked)
            .and_then(|p| p.locked_since)
            .unwrap_or(at)
    });
    Record {
        idle_seconds: input.idle_seconds,
        locked: input.locked,
        locked_since,
        written_at: at,
    }
}

fn read(path: &Path) -> Option<Record> {
    let record: Record = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    (record.idle_seconds.is_finite() && record.idle_seconds >= 0.0 && record.written_at.is_finite())
        .then_some(record)
}

fn write(root: &Path, input: &Input, at: f64) -> Result<Record> {
    fs::create_dir_all(root).context("create the hgs state directory")?;
    let path = root.join(FILE);
    let record = next(read(&path).as_ref(), input, at);
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    file.write_all(&serde_json::to_vec(&record)?)?;
    file.as_file().sync_all()?;
    file.persist(&path).map_err(|e| e.error)?;
    Ok(record)
}

/// `desktop_presence` for `hgs ls --json --local`, aged to `at`. Records written
/// more than two minutes ago, or that far in the future, are not listed.
pub fn listing(root: &Path, at: f64) -> Option<Value> {
    let record = read(&root.join(FILE))?;
    let age = at - record.written_at;
    if !(-STALE_SECONDS..=STALE_SECONDS).contains(&age) {
        return None;
    }
    let age = age.max(0.0);
    let locked_seconds = record
        .locked
        .then(|| record.locked_since.map_or(age, |since| (at - since).max(0.0)));
    Some(json!({
        "idle_seconds": record.idle_seconds + age,
        "locked": record.locked,
        "locked_seconds": locked_seconds,
    }))
}
```

- [ ] **Step 4: Run the unit tests**

Run: `cargo test --locked presence::`
Expected: PASS (4 tests).

- [ ] **Step 5: Write the failing CLI integration tests**

Create `tests/test_presence.py`:

```python
#!/usr/bin/env python3
"""Desktop presence and shared preferences through the real CLI with isolated state."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest

REPO = Path(__file__).resolve().parents[1]
HGS = Path(os.environ.get('HGS_TEST_BIN', REPO / 'target/debug/hgs')).resolve()


class Presence(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix='hgs-presence-')
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        (self.root / 'bin').mkdir()
        (self.root / 'config').mkdir()
        tmux = self.root / 'bin/tmux'
        tmux.write_text('#!/bin/sh\nif [ "$1" = -V ]; then echo "tmux 3.7"; fi\n')
        tmux.chmod(0o755)
        self.state = self.root / 'state'
        self.file = self.state / 'desktop-presence.json'
        self.env = dict(os.environ, HOME=str(self.root), HGS_CONFIG_DIR=str(self.root / 'config'),
                        HGS_STATE_DIR=str(self.state), HGS_TAB='0',
                        PATH=str(self.root / 'bin') + ':' + os.environ['PATH'])
        for key in ('HGS_PEERS', 'HGS_SELF', 'TMUX', 'TMUX_PANE', 'HGS_RUN_ID', 'HGS_SESSION', 'HGS_EXECUTABLE'):
            self.env.pop(key, None)

    def hgs(self, *args, data=None, ok=True):
        result = subprocess.run([str(HGS), *args], input=None if data is None else json.dumps(data),
                                env=self.env, text=True, capture_output=True, timeout=20)
        if ok:
            self.assertEqual(result.returncode, 0, result.stderr)
            return json.loads(result.stdout)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        return result.stderr

    def listing(self):
        return self.hgs('ls', '--json', '--local')

    def test_presence_is_written_atomically_aged_and_expires(self):
        self.assertNotIn('desktop_presence', self.listing())
        before = time.time()
        self.assertEqual(self.hgs('presence', 'set', '--json', data={'idle_seconds': 3, 'locked': False}), {'ok': True})
        record = json.loads(self.file.read_text())
        self.assertEqual(set(record), {'idle_seconds', 'locked', 'locked_since', 'written_at'})
        self.assertEqual((record['idle_seconds'], record['locked'], record['locked_since']), (3, False, None))
        self.assertGreaterEqual(record['written_at'], before - 1)
        self.assertEqual([p.name for p in self.state.iterdir() if p.name.startswith('.tmp')], [])
        presence = self.listing()['desktop_presence']
        self.assertEqual(set(presence), {'idle_seconds', 'locked', 'locked_seconds'})
        self.assertGreaterEqual(presence['idle_seconds'], 3)
        self.assertLess(presence['idle_seconds'], 33)
        self.assertEqual((presence['locked'], presence['locked_seconds']), (False, None))
        for written_at in (time.time() - 121, time.time() + 600):
            self.file.write_text(json.dumps({**record, 'written_at': written_at}))
            self.assertNotIn('desktop_presence', self.listing())

    def test_lock_time_survives_consecutive_locked_writes(self):
        self.hgs('presence', 'set', '--json', data={'idle_seconds': 0, 'locked': True})
        first = json.loads(self.file.read_text())['locked_since']
        self.assertIsNotNone(first)
        time.sleep(0.05)
        self.hgs('presence', 'set', '--json', data={'idle_seconds': 40, 'locked': True})
        self.assertEqual(json.loads(self.file.read_text())['locked_since'], first)
        listed = self.listing()['desktop_presence']
        self.assertTrue(listed['locked'])
        self.assertGreaterEqual(listed['locked_seconds'], 0)
        self.hgs('presence', 'set', '--json', data={'idle_seconds': 1, 'locked': False})
        self.assertIsNone(json.loads(self.file.read_text())['locked_since'])

    def test_invalid_input_and_shapes_leave_the_record_unchanged(self):
        self.hgs('presence', 'set', '--json', data={'idle_seconds': 5, 'locked': False})
        original = self.file.read_bytes()
        for data in ({'idle_seconds': -1, 'locked': False}, {'idle_seconds': 1}, {'idle_seconds': '1', 'locked': False},
                     {'idle_seconds': 1, 'locked': 'no'}, {'idle_seconds': 1, 'locked': False, 'extra': 1}, [],
                     {'idle_seconds': 1e12, 'locked': False}):
            self.hgs('presence', 'set', '--json', data=data, ok=False)
        self.hgs('presence', 'set', data={'idle_seconds': 1, 'locked': False}, ok=False)
        self.hgs('--dry-run', 'presence', 'set', '--json', data={'idle_seconds': 1, 'locked': False}, ok=False)
        self.assertEqual(self.file.read_bytes(), original)


if __name__ == '__main__':
    unittest.main()
```

- [ ] **Step 6: Run them to verify they fail**

Run: `cargo build --locked && python3 -m unittest discover -s tests -p test_presence.py -v`
Expected: FAIL; `presence set` falls through to agent launch and exits with "unknown option --json (agent flags go after --)".

- [ ] **Step 7: Wire the command, usage and listing**

In `src/cli.rs` USAGE, after the line `       hgs swarm assign-launch --json   assign an exact launch to its selected project` insert:

```text
       hgs presence set --json        desktop idle/lock time from JSON stdin (written by the tray)
```

In `dispatch`, before `"swarm" => crate::swarm::dispatch(&config, args, dry),` insert:

```rust
        "presence" => crate::presence::dispatch(&config, args, dry),
```

In `local_json`, replace

```rust
    let raw = json!({"host":config.host(),"ok":true,"hgs_version":env!("CARGO_PKG_VERSION"),"projects":projects,"sessions":sessions,"peers":config.peers,"metrics":crate::metrics::snapshot(&config.state)});
    state::merge_snapshot(raw).map_err(|e| Error::new(1, e))
```

with

```rust
    let mut raw = json!({"host":config.host(),"ok":true,"hgs_version":env!("CARGO_PKG_VERSION"),"projects":projects,"sessions":sessions,"peers":config.peers,"metrics":crate::metrics::snapshot(&config.state)});
    // Phone alerts wait while the owner is active here; see src/presence.rs.
    if let Some(presence) = crate::presence::listing(&config.state, crate::presence::now()) {
        raw["desktop_presence"] = presence;
    }
    state::merge_snapshot(raw).map_err(|e| Error::new(1, e))
```

- [ ] **Step 8: Run the integration tests**

Run: `cargo build --locked && python3 -m unittest discover -s tests -p test_presence.py -v`
Expected: PASS (3 tests).

- [ ] **Step 9: Document the listing field**

In `docs/reference.md`, after the paragraph ending "`ok: false` indicates an unavailable host." add:

```markdown
A running Zerus desktop records input idle time and screen lock with
`hgs presence set --json` (`{"idle_seconds": 12, "locked": false}` on stdin) every
30 seconds and when the screen locks, unlocks or the owner returns. The listing then
adds `desktop_presence: {idle_seconds, locked, locked_seconds}`, aged to the moment
of listing. It is omitted when the record is more than two minutes old (Zerus
closed, crashed or the machine slept) or written by a clock that has since moved
back. The record is `desktop-presence.json` in the hgs state directory and holds
no other desktop information.
```

- [ ] **Step 10: Run the full Rust suite and formatting**

Run: `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo +1.85.0 test --locked`
Expected: PASS.

- [ ] **Step 11: Commit**

```bash
git add src/presence.rs src/main.rs src/cli.rs tests/test_presence.py docs/reference.md
git commit -m "Record desktop presence in hgs listings"
```

---

### Task 2: Swarm-wide delivery preference in hgs

**Files:**
- Modify: `src/swarm/catalog.rs:3-93` (imports, `Key`, validation), `src/swarm/catalog.rs:131-362` (`Catalog::preferences`), tests at `src/swarm/catalog.rs:364-434`
- Modify: `src/swarm/mod.rs:1-106` (module doc, `preferences`, `preference` command, usage)
- Modify: `src/swarm/model.rs:361-381` (`snapshot` conflicts and `preferences`)
- Modify: `src/swarm/transport.rs:348-382` (`join` keeps the swarm's choice)
- Modify: `src/mobile_peers.rs:223-243` (allowlist) and its tests
- Modify: `src/cli.rs:70-71` (USAGE), `src/cli.rs:632-637` (`local_json`)
- Modify: `tests/test_presence.py`, `tests/test_swarm.py`
- Modify: `docs/reference.md:604-646` (swarm section) and the JSON paragraph from Task 1

**Interfaces:**
- Consumes: Task 1 `local_json` shape.
- Produces: `catalog::Key::Preference { field: String }`, `Key::preference(field: &str) -> Key`, `Key::kind(&self) -> &'static str`, `catalog::MOBILE_DELIVERY: [&str; 6]`, `catalog::DEFAULT_MOBILE_DELIVERY`, `catalog::default_preferences() -> Value`, `Catalog::preferences(&self) -> Value` (`{"mobile_delivery": ...}`), `pub(crate) fn crate::swarm::preferences(config: &Config) -> Value`, `pub(crate) use crate::swarm::MOBILE_DELIVERY`. `hgs swarm get` gains top-level `"preferences"`. Task 3 uses `Key::kind`; Task 4's connector depends on the usage marker `swarm preference set mobile_delivery`; Task 8 reads `hgs swarm preference get|set` and the `preferences` field of `hgs swarm get`.

- [ ] **Step 1: Write the failing catalog tests**

Append inside `mod tests` in `src/swarm/catalog.rs`:

```rust
    #[test]
    fn delivery_preference_validates_merges_and_defaults() {
        let (a, b) = (new_id(), new_id());
        let key = Key::preference("mobile_delivery");
        let mut first = Catalog::default();
        assert_eq!(first.preferences(), json!({"mobile_delivery": "away"}));
        assert!(first.set(&a, key.clone(), json!("later")).is_err());
        assert!(first.set(&a, key.clone(), json!(5)).is_err());
        assert!(first.set(&a, Key::preference("theme"), json!("away")).is_err());
        let mut second = first.clone();
        first.set(&a, key.clone(), json!("away_5")).unwrap();
        second.set(&b, key.clone(), json!("immediate")).unwrap();
        merge(&mut first, &second);
        merge(&mut second, &first);
        assert_eq!(first.heads()[&key].len(), 2);
        assert_eq!(first.preferences(), second.preferences());
        first.set(&a, key.clone(), json!("away_30")).unwrap();
        merge(&mut second, &first);
        assert_eq!(second.heads()[&key].len(), 1);
        assert_eq!(second.preferences(), json!({"mobile_delivery": "away_30"}));
    }
    #[test]
    fn preference_keys_use_their_wire_tag_as_kind() {
        let key = Key::preference("mobile_delivery");
        assert_eq!(
            serde_json::to_value(&key).unwrap(),
            json!({"kind": "preference", "field": "mobile_delivery"})
        );
        for key in [
            Key::project("p", "name"),
            Key::Folder { id: "f".into() },
            Key::Session { machine: new_id(), session: "s".into() },
            Key::machine(&new_id(), "name"),
            key,
        ] {
            assert_eq!(serde_json::to_value(&key).unwrap()["kind"], key.kind());
        }
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --locked swarm::catalog`
Expected: FAIL to compile ("no function or associated item named `preference`").

- [ ] **Step 3: Implement the key, validation and value**

In `src/swarm/catalog.rs`:

Change `use serde_json::Value;` to `use serde_json::{json, Value};`.

Add the variant last, so existing keys keep their order:

```rust
pub enum Key {
    Project { id: String, field: String },
    Folder { id: String },
    Session { machine: String, session: String },
    Machine { id: String, field: String },
    Preference { field: String },
}
```

Below `pub fn machine(...)` add:

```rust
    pub fn preference(field: &str) -> Self {
        Self::Preference {
            field: field.into(),
        }
    }
    /// The serde tag. Exchanges use it to omit kinds an older member rejects.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Project { .. } => "project",
            Self::Folder { .. } => "folder",
            Self::Session { .. } => "session",
            Self::Machine { .. } => "machine",
            Self::Preference { .. } => "preference",
        }
    }
```

In `validate`, after the `Self::Machine { id, field } => { ... }` arm add:

```rust
            Self::Preference { field } => {
                ensure!(
                    match field.as_str() {
                        "mobile_delivery" => value
                            .as_str()
                            .is_some_and(|s| MOBILE_DELIVERY.contains(&s)),
                        _ => false,
                    },
                    "invalid shared preference"
                );
            }
```

After `pub fn new_id()` add:

```rust
/// When the phone notifies: at once, or after the owner is away from every
/// Zerus desktop for the given number of minutes.
pub const MOBILE_DELIVERY: [&str; 6] = ["immediate", "away", "away_5", "away_10", "away_15", "away_30"];
pub const DEFAULT_MOBILE_DELIVERY: &str = "away";
pub fn default_preferences() -> Value {
    json!({"mobile_delivery": DEFAULT_MOBILE_DELIVERY})
}
```

In `impl Catalog`, after `pub fn values(&self)` add:

```rust
    /// Shared preferences with defaults. Concurrent edits resolve to the newest
    /// head on every replica; the next choice on any device supersedes them all.
    pub fn preferences(&self) -> Value {
        let key = Key::preference("mobile_delivery");
        let heads = self.raw_heads(&key);
        let value = Self::selected(&key, &heads)
            .and_then(|op| op.value.as_str())
            .unwrap_or(DEFAULT_MOBILE_DELIVERY);
        json!({"mobile_delivery": value})
    }
```

- [ ] **Step 4: Run the catalog tests**

Run: `cargo test --locked swarm::catalog`
Expected: PASS (5 tests).

- [ ] **Step 5: Write the failing CLI and swarm integration tests**

Append to class `Presence` in `tests/test_presence.py`:

```python
    def test_shared_delivery_preference_defaults_validates_and_lists(self):
        self.assertEqual(self.listing()['preferences'], {'mobile_delivery': 'away'})
        self.assertEqual(self.hgs('swarm', 'preference', 'get', '--json'), {'mobile_delivery': 'away'})
        self.assertEqual(self.hgs('swarm', 'preference', 'set', 'mobile_delivery', 'away_10', '--json'),
                         {'ok': True, 'mobile_delivery': 'away_10'})
        self.assertEqual(self.hgs('swarm', 'preference', 'get', '--json'), {'mobile_delivery': 'away_10'})
        self.assertEqual(self.listing()['preferences'], {'mobile_delivery': 'away_10'})
        for args in (('set', 'mobile_delivery', 'later'), ('set', 'theme', 'away'), ('set', 'mobile_delivery'), ('unset',)):
            self.hgs('swarm', 'preference', *args, '--json', ok=False)
        self.assertEqual(self.hgs('swarm', 'preference', 'get', '--json'), {'mobile_delivery': 'away_10'})
        # A listing never fails because of the catalog; damage reads as the default.
        (self.root / 'config/swarm/catalog.json').write_text('{"damaged":')
        self.assertEqual(self.listing()['preferences'], {'mobile_delivery': 'away'})
```

Append to class `Swarm` in `tests/test_swarm.py`:

```python
    def test_concurrent_delivery_choices_converge_without_conflict_review(self):
        self.join('a', 'b'); self.join('b', 'a'); self.join('c', 'b'); self.join('b', 'c')
        self.converge()
        self.call('a', 'preference', 'set', 'mobile_delivery', 'away_5', '--json')
        self.call('c', 'preference', 'set', 'mobile_delivery', 'immediate', '--json')
        snapshots = self.converge()
        values = {self.call(node, 'preference', 'get', '--json')['mobile_delivery'] for node in 'abc'}
        self.assertEqual(len(values), 1)
        self.assertIn(values.pop(), {'away_5', 'immediate'})
        for snapshot in snapshots:
            self.assertEqual(snapshot['conflicts'], [])
            self.assertEqual(snapshot['preferences'], snapshots[0]['preferences'])
        self.call('b', 'preference', 'set', 'mobile_delivery', 'away_30', '--json')
        self.converge()
        self.assertEqual({self.call(node, 'preference', 'get', '--json')['mobile_delivery'] for node in 'abc'}, {'away_30'})

    def test_joining_keeps_the_swarm_delivery_choice(self):
        self.call('b', 'preference', 'set', 'mobile_delivery', 'immediate', '--json')
        self.call('a', 'preference', 'set', 'mobile_delivery', 'away_5', '--json')
        self.join('a', 'b')
        self.assertEqual(self.call('a', 'preference', 'get', '--json'), {'mobile_delivery': 'immediate'})
        self.assertEqual(self.call('a', 'get')['conflicts'], [])
        self.call('c', 'preference', 'set', 'mobile_delivery', 'away_30', '--json')
        self.join('c', 'b')
        self.assertEqual(self.call('c', 'preference', 'get', '--json'), {'mobile_delivery': 'immediate'})
```

- [ ] **Step 6: Run them to verify they fail**

Run: `cargo build --locked && python3 -m unittest discover -s tests -p test_presence.py -k preference -v && python3 -m unittest discover -s tests -p test_swarm.py -k delivery -v`
Expected: FAIL; `hgs swarm preference` reports "usage: hgs swarm get | initialize | ..." and listings lack `preferences`.

- [ ] **Step 7: Implement the command, listing field and snapshot**

In `src/swarm/mod.rs`, replace the module doc:

```rust
//! Shared metadata only. Transport credentials and per-device presentation stay
//! local; the swarm-wide phone delivery choice is shared. All commands use one
//! locked, atomically replaced store.
```

After `use store::LockedStore;` add:

```rust
pub(crate) use catalog::MOBILE_DELIVERY;

/// Shared preferences for `hgs ls --json --local`. Lock-free: the store is
/// replaced atomically and listings never wait for a sync. A missing or
/// unreadable catalog reports the defaults.
pub(crate) fn preferences(config: &Config) -> Value {
    std::fs::read(config.dir.join("swarm/catalog.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<store::Store>(&bytes).ok())
        .map_or_else(catalog::default_preferences, |store| {
            store.catalog.preferences()
        })
}

fn preference(config: &Config, args: &[String]) -> Result<Value> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let args = args.strip_suffix(&["--json"]).unwrap_or(&args[..]);
    match args {
        ["get"] => Ok(LockedStore::load(config)?.data.catalog.preferences()),
        ["set", field, value] => {
            let mut store = LockedStore::load(config)?;
            let actor = store.data.node_id.clone();
            store
                .data
                .catalog
                .set(&actor, catalog::Key::preference(field), json!(value))?;
            store.save()?;
            let mut result = store.data.catalog.preferences();
            result["ok"] = json!(true);
            Ok(result)
        }
        _ => bail!("usage: hgs swarm preference get --json | set mobile_delivery VALUE --json"),
    }
}
```

In `execute`, before `"worker" => ...` add:

```rust
        "preference" => preference(config, &args[1..]),
```

and change the final usage to:

```rust
        _ => bail!("usage: hgs swarm get | initialize | apply | preview PEER | join PEER | sync | worker | resolve | disconnect PEER | preference get|set"),
```

In `src/swarm/model.rs` `snapshot`, change the start of the conflicts chain to:

```rust
        let conflicts: Vec<_> = self
            .catalog
            .heads()
            .into_iter()
            // Concurrent preference edits resolve to the newest head and the
            // next choice on any device supersedes them; nothing to review.
            .filter(|(key, _)| !matches!(key, Key::Preference { .. }))
            .filter_map(|(key, heads)| {
```

and in the returned object add `"preferences":self.catalog.preferences(),` after `"conflicts":conflicts,`.

In `src/swarm/transport.rs` `join`, inside `for (mut key, mut value) in old {`, before the existing `if request.prefer_peer_memberships` block insert:

```rust
            // The swarm's delivery choice wins over the joining machine's own.
            if matches!(key, Key::Preference { .. }) && existing.contains_key(&key) {
                continue;
            }
```

In `src/cli.rs` `local_json`, after the `desktop_presence` block from Task 1 add:

```rust
    raw["preferences"] = crate::swarm::preferences(config);
```

In USAGE, after the `hgs presence set --json` line add:

```text
       hgs swarm preference get --json   shared phone notification timing
       hgs swarm preference set mobile_delivery VALUE --json   immediate|away|away_5|away_10|away_15|away_30
```

- [ ] **Step 8: Allow the gateway connector to set it**

The gateway connector routes local native calls through `hgs swarm __mobile-peer-local` whenever one-hop routing is active. In `src/mobile_peers.rs` `validate`, replace the `"swarm"` arm's `else` branch:

```rust
            } else if rest.len() == 5
                && rest[0] == "preference"
                && rest[1] == "set"
                && rest[2] == "mobile_delivery"
                && crate::swarm::MOBILE_DELIVERY.contains(&rest[3].as_str())
                && rest[4] == "--json"
            {
                // The gateway connector changes the swarm-wide delivery choice.
                no_payload()?;
            } else {
                ensure!(
                    rest.len() == 1 && matches!(rest[0].as_str(), "get" | "hello"),
                    "swarm operation is not allowed"
                );
                no_payload()?;
            }
```

Add to `mod tests` in `src/mobile_peers.rs`:

```rust
    #[test]
    fn shared_delivery_setting_accepts_only_exact_known_values() {
        let set = ["swarm", "preference", "set", "mobile_delivery", "away_10", "--json"];
        assert!(validate(&request(&set, None)).is_ok());
        assert!(validate(&request(&set, Some(json!({})))).is_err());
        for argv in [
            vec!["swarm", "preference", "set", "mobile_delivery", "later", "--json"],
            vec!["swarm", "preference", "set", "theme", "away", "--json"],
            vec!["swarm", "preference", "set", "mobile_delivery", "away"],
            vec!["swarm", "preference", "get", "--json"],
        ] {
            assert!(validate(&request(&argv, None)).is_err(), "{argv:?}");
        }
    }
```

- [ ] **Step 9: Run the tests**

Run: `cargo test --locked swarm:: && cargo test --locked mobile_peers:: && cargo build --locked && python3 -m unittest discover -s tests -p test_presence.py -v && python3 -m unittest discover -s tests -p test_swarm.py -v && python3 -m unittest discover -s tests -p test_mobile_peers.py -v`
Expected: PASS.

- [ ] **Step 10: Document the shared field**

In `docs/reference.md` "Projects and P2P synchronization", change "Shared fields are machine IDs/names, project IDs/names/colors, folder locations and session membership." to "Shared fields are machine IDs/names, project IDs/names/colors, folder locations, session membership and the phone notification timing." After the paragraph ending "worker installation." add:

```markdown
### Phone notification timing

`hgs swarm preference get --json` prints `{"mobile_delivery": "away"}`;
`hgs swarm preference set mobile_delivery away_10 --json` changes it for every
member. Values are `immediate`, `away` (default), `away_5`, `away_10`, `away_15`
and `away_30`. Concurrent changes settle on one value without a review; the next
change from any computer or the phone replaces them. A machine joining a swarm
adopts the swarm's existing choice. Without a swarm the local catalog keeps it the
same way.
```

Extend the Task 1 JSON paragraph with: "The listing also includes `preferences: {mobile_delivery}` from the shared catalog, or the default when the catalog is missing or unreadable."

- [ ] **Step 11: Run the full Rust suite**

Run: `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo +1.85.0 test --locked`
Expected: PASS.

- [ ] **Step 12: Commit**

```bash
git add src/swarm src/mobile_peers.rs src/cli.rs tests/test_presence.py tests/test_swarm.py docs/reference.md
git commit -m "Share the phone delivery timing through the swarm catalog"
```

---

### Task 3: Keep older swarm members syncing

Older members deserialize `Exchange` and `Operation` with `deny_unknown_fields` and a closed `Key` enum: one `preference` operation, or an unknown `kinds` field, makes their whole exchange fail ("unknown variant `preference`"). New members therefore advertise the kinds they parse and never send other kinds to a member that does not advertise them.

**Files:**
- Modify: `src/swarm/catalog.rs` (kind helpers and tests)
- Modify: `src/swarm/store.rs:21-29` (`Peer`)
- Modify: `src/swarm/transport.rs:25-31` (`Exchange`), `122-150` (`hello`, `exchange`), `152-230` (`sync_peer`)
- Modify: `tests/test_swarm.py:14-24` (fake SSH), new test
- Modify: `docs/reference.md` (Task 2 subsection)

**Interfaces:**
- Consumes: `Key::kind()` from Task 2.
- Produces: `catalog::LEGACY_KINDS`, `catalog::kinds() -> BTreeSet<String>`, `catalog::accepted(kinds: Option<&BTreeSet<String>>, key: &Key) -> bool`; `hgs swarm hello|inventory` gain `"kinds"`; `Exchange.kinds: Option<BTreeSet<String>>`; `hgs swarm get` → `peers[alias].catalog_outdated: true` (omitted when false). Task 8 reads `catalog_outdated`.

- [ ] **Step 1: Write the failing unit test**

Append to `mod tests` in `src/swarm/catalog.rs`:

```rust
    #[test]
    fn legacy_members_receive_only_kinds_they_parse() {
        let preference = Key::preference("mobile_delivery");
        assert!(accepted(None, &Key::project("p", "name")));
        assert!(accepted(None, &Key::Folder { id: "f".into() }));
        assert!(!accepted(None, &preference));
        assert!(accepted(Some(&kinds()), &preference));
        let older: BTreeSet<String> = LEGACY_KINDS.iter().map(|k| (*k).to_owned()).collect();
        assert!(!accepted(Some(&older), &preference));
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --locked swarm::catalog::tests::legacy`
Expected: FAIL to compile ("cannot find function `accepted`").

- [ ] **Step 3: Implement the kind helpers**

In `src/swarm/catalog.rs`, after `default_preferences`:

```rust
/// Key kinds every member parses. Members from before shared preferences reject
/// any other kind, so exchanges with them omit newer kinds.
pub const LEGACY_KINDS: [&str; 4] = ["project", "folder", "session", "machine"];
pub fn kinds() -> BTreeSet<String> {
    ["project", "folder", "session", "machine", "preference"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}
/// Whether a member advertising `kinds` (or nothing, if older) accepts `key`.
pub fn accepted(kinds: Option<&BTreeSet<String>>, key: &Key) -> bool {
    kinds.map_or(LEGACY_KINDS.contains(&key.kind()), |kinds| {
        kinds.contains(key.kind())
    })
}
```

- [ ] **Step 4: Run it**

Run: `cargo test --locked swarm::catalog`
Expected: PASS.

- [ ] **Step 5: Write the failing integration test with an emulated older member**

In `tests/test_swarm.py`, replace the `SSH` script with:

```python
SSH = '''#!/usr/bin/env python3
import json, os, pathlib, shlex, subprocess, sys
root = pathlib.Path(os.environ['SWARM_TEST_ROOT'])
alias = sys.argv[-2]
if (root / ('offline-' + alias)).exists():
    print('fixture offline', file=sys.stderr); sys.exit(255)
env = dict(os.environ, HGS_CONFIG_DIR=str(root / alias), HGS_STATE_DIR=str(root / alias / 'state'))
args = shlex.split(sys.argv[-1])[1:]
with (root / 'calls').open('a') as file: file.write(alias + ' ' + ' '.join(args) + '\\n')
if not (root / ('old-' + alias)).exists():
    os.execve(os.environ['SWARM_TEST_HGS'], [os.environ['SWARM_TEST_HGS'], *args], env)
# A member from before shared preferences: it rejects unknown fields and key
# kinds and never advertises the kinds it accepts.
request = sys.stdin.read()
if args[1:] == ['exchange']:
    data = json.loads(request)
    if 'kinds' in data or any(op['key']['kind'] not in ('project', 'folder', 'session', 'machine') for op in data['operations']):
        print('unknown field or variant from a newer member', file=sys.stderr); sys.exit(1)
result = subprocess.run([os.environ['SWARM_TEST_HGS'], *args], input=request, env=env, text=True, capture_output=True)
sys.stderr.write(result.stderr)
if result.returncode: sys.exit(result.returncode)
output = json.loads(result.stdout)
output.pop('kinds', None)
print(json.dumps(output))
'''
```

Append to class `Swarm`:

```python
    def test_older_members_keep_project_sync_without_the_delivery_choice(self):
        self.join('a', 'b'); self.join('b', 'a'); self.join('c', 'b'); self.join('b', 'c')
        self.converge()
        (self.root / 'old-c').touch()  # c now answers b like a release without shared preferences
        self.call('a', 'preference', 'set', 'mobile_delivery', 'away_10', '--json')
        for node in ('a', 'b', 'b'):
            self.assertTrue(all(row['ok'] for row in self.call(node, 'sync').values()), node)
        self.assertEqual(self.call('b', 'preference', 'get', '--json'), {'mobile_delivery': 'away_10'})
        self.assertEqual(self.call('c', 'preference', 'get', '--json'), {'mobile_delivery': 'away'})
        before = self.call('b', 'get')
        desired = copy.deepcopy(before['organization'])
        next(p for p in desired['projects'] if p['id'] == 'b')['name'] = 'Renamed after the preference'
        self.patch('b', before, desired)
        self.assertTrue(self.call('b', 'sync')['c']['ok'])
        projects = self.call('c', 'get')['organization']['projects']
        self.assertEqual(next(p for p in projects if p['id'] == 'b')['name'], 'Renamed after the preference')
        peers = self.call('b', 'get')['peers']
        self.assertTrue(peers['c']['catalog_outdated'])
        self.assertNotIn('catalog_outdated', peers['a'])
        # A legacy requester omits accepted kinds and receives only legacy kinds.
        inventory = self.call('b', 'inventory')
        response = self.call('b', 'exchange', data={'swarm_id': inventory['swarm_id'], 'known': [], 'operations': []})
        self.assertTrue(response['operations'])
        self.assertNotIn('preference', {op['key']['kind'] for op in response['operations']})
        # After the update the member receives the choice and the notice clears.
        (self.root / 'old-c').unlink()
        self.assertTrue(self.call('b', 'sync')['c']['ok'])
        self.assertEqual(self.call('c', 'preference', 'get', '--json'), {'mobile_delivery': 'away_10'})
        self.assertNotIn('catalog_outdated', self.call('b', 'get')['peers']['c'])
```

- [ ] **Step 6: Run it to verify it fails**

Run: `cargo build --locked && python3 -m unittest discover -s tests -p test_swarm.py -k older -v`
Expected: FAIL; `b sync` reports `c: unknown field or variant from a newer member`.

- [ ] **Step 7: Implement kind-aware exchange**

In `src/swarm/store.rs`, extend `Peer`:

```rust
pub struct Peer {
    pub node: String,
    #[serde(default)]
    pub last_sync: u64,
    #[serde(default)]
    pub error: String,
    /// The member runs an hgs without shared preferences; it keeps syncing
    /// projects and folders but does not receive the delivery choice.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub catalog_outdated: bool,
}
```

In `src/swarm/transport.rs`, change the import to `use super::{catalog::{self, Catalog, Key, Operation}, store::{LockedStore, Peer}};` and extend `Exchange`:

```rust
pub struct Exchange {
    pub swarm_id: String,
    pub known: BTreeSet<String>,
    pub operations: Vec<Operation>,
    /// Key kinds the requester parses. Older members neither send nor accept
    /// this field; responses to them contain only legacy kinds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinds: Option<BTreeSet<String>>,
}
```

In `hello`, add `"kinds":catalog::kinds()` to the `result` object.

In `exchange`, change the `missing` filter to:

```rust
        .filter(|op| {
            !request.known.contains(&op.id) && catalog::accepted(request.kinds.as_ref(), &op.key)
        })
```

In `sync_peer`, replace

```rust
    local
        .data
        .bind(alias, identity["node_id"].as_str().unwrap())?;
    local.save()?;
```

with

```rust
    local
        .data
        .bind(alias, identity["node_id"].as_str().unwrap())?;
    // Older members reject unknown catalog kinds. They keep receiving the kinds
    // they parse; Settings names them until they are updated.
    let peer_kinds: Option<BTreeSet<String>> = serde_json::from_value(identity["kinds"].clone()).ok();
    if let Some(peer) = local.data.peers.get_mut(alias) {
        peer.catalog_outdated = !peer_kinds
            .as_ref()
            .is_some_and(|kinds| kinds.contains("preference"));
    }
    local.save()?;
```

and build the request as:

```rust
        let request = Exchange {
            swarm_id: local.data.catalog.swarm_id.clone(),
            known: local.data.catalog.operations.keys().cloned().collect(),
            operations: local
                .data
                .catalog
                .operations
                .values()
                .filter(|op| !known.contains(&op.id) && catalog::accepted(peer_kinds.as_ref(), &op.key))
                .cloned()
                .collect(),
            kinds: peer_kinds.as_ref().map(|_| catalog::kinds()),
        };
```

- [ ] **Step 8: Run the swarm suites**

Run: `cargo test --locked swarm:: && cargo build --locked && python3 -m unittest discover -s tests -p test_swarm.py -v`
Expected: PASS (all existing tests unchanged).

- [ ] **Step 9: Document the compatibility rule**

Append to the "Phone notification timing" subsection in `docs/reference.md`:

```markdown
Members running an older hgs keep synchronizing projects, folders and sessions but
do not receive this choice; Settings → Mobile connection names them until they are
updated. An older machine cannot preview or join a swarm whose catalog already
contains the choice; update it first.
```

- [ ] **Step 10: Run the full Rust suite**

Run: `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo +1.85.0 test --locked`
Expected: PASS.

- [ ] **Step 11: Commit**

```bash
git add src/swarm tests/test_swarm.py docs/reference.md
git commit -m "Keep older swarm members syncing without the shared preference"
```

---

### Task 4: Relay and connector operation `set_mobile_delivery`

**Files:**
- Modify: `services/mobile/relay/src/protocol.rs:23-52` (constants), `124-151` (`request`)
- Modify: `services/mobile/relay/tests/contracts.rs` (new test)
- Modify: `services/mobile/zerus_mobile/context.py:5-14` (constants, validator)
- Modify: `services/mobile/zerus_mobile/server.py:22, 277-289` (oracle validation)
- Modify: `services/mobile/zerus_mobile/connector.py:33, 278-296, 432-484, 560-566, 747-750, 870-871`
- Modify: `services/mobile/tests/fixture_hgs.py:252-258` and its final `else`
- Modify: `services/mobile/tests/test_relay.py` (new test), `services/mobile/tests/test_rust_relay.py:89-99` (replay list)
- Create: `services/mobile/tests/test_mobile_delivery.py`
- Modify: `docs/mobile-architecture.md:626-635`

**Interfaces:**
- Consumes: Task 2 CLI `hgs swarm preference set mobile_delivery VALUE --json` → `{"ok": true, "mobile_delivery": VALUE}`, usage marker `swarm preference set mobile_delivery`, `ls` field `preferences`.
- Produces: relay `protocol::COMPUTER`, `protocol::MOBILE_DELIVERY`; Python `context.MOBILE_DELIVERY`, `context.COMPUTER_OPERATIONS`, `context.validate_mobile_delivery(payload)`; connector attribute `preference_supported`; relay capabilities list `set_mobile_delivery`; gateway snapshots advertise it in `mobile_capabilities.operations`. Android (Task 9) relies on these.

- [ ] **Step 1: Write the failing Rust relay contract test**

Append to `services/mobile/relay/tests/contracts.rs`:

```rust
#[tokio::test]
async fn mobile_delivery_is_computer_scoped_and_capability_gated() {
    let f = Fixture::new().await;
    let id = Uuid::new_v4().to_string();
    let b = json!({"request_id":id,"computer_id":f.node.id,"operation":"set_mobile_delivery","session":"","payload":{"value":"away_10"}});
    assert_eq!(f.submit(&b).await.unwrap_err().0.as_u16(), 409);
    let snapshot = json!({"sessions":[],"mobile_capabilities":{"protocol_version":1,"operations":["set_mobile_delivery"],"features":[]}});
    f.store
        .heartbeat(&f.node, &json!({"snapshot":snapshot}), &canonical(&snapshot).unwrap())
        .await
        .unwrap();
    assert_eq!(f.submit(&b).await.unwrap()["state"], "queued");
    assert!(protocol::OPERATIONS.contains(&"set_mobile_delivery"));
    for (field, value) in [
        ("session", json!("codex/example/tag")),
        ("payload", json!({"value":"later"})),
        ("payload", json!({})),
        ("payload", json!({"value":"away","request_id":id})),
    ] {
        let mut invalid = b.clone();
        invalid["request_id"] = json!(Uuid::new_v4().to_string());
        invalid[field] = value;
        assert!(matches!(
            zerus_relay::store::Submission::new(invalid),
            Err(e) if e.0.as_u16() == 400
        ));
    }
    f.close().await;
}
```

- [ ] **Step 2: Run it to verify it fails (on `arch`)**

Run: `cd services/mobile/relay && cargo +1.85.0 test --locked --test contracts mobile_delivery`
Expected: FAIL; the first submission is rejected with 400 instead of 409 (unknown operation).

- [ ] **Step 3: Implement the relay validation**

In `services/mobile/relay/src/protocol.rs`, after `LAUNCH`:

```rust
/// Computer-scoped: no session, delivered to the gateway computer itself.
pub const COMPUTER: &[&str] = &["set_mobile_delivery"];
pub const MOBILE_DELIVERY: &[&str] = &["immediate", "away", "away_5", "away_10", "away_15", "away_30"];
```

Insert `"set_mobile_delivery",` in `OPERATIONS` between `"send_now",` and `"settings",`.

In `request`, replace

```rust
    text(&v["session"], 1024, LAUNCH.contains(&op))?;
    if LAUNCH.contains(&op) {
        if v["session"] != "" {
            return Err(Error::BAD);
        }
        launch(op, p, id)?;
    } else if op == "inspect" {
```

with

```rust
    text(&v["session"], 1024, LAUNCH.contains(&op) || COMPUTER.contains(&op))?;
    if LAUNCH.contains(&op) {
        if v["session"] != "" {
            return Err(Error::BAD);
        }
        launch(op, p, id)?;
    } else if COMPUTER.contains(&op) {
        fields(p, &["value"], &[])?;
        if v["session"] != "" || !MOBILE_DELIVERY.iter().any(|x| p["value"] == *x) {
            return Err(Error::BAD);
        }
    } else if op == "inspect" {
```

- [ ] **Step 4: Run the relay checks (on `arch`)**

Run: `cd services/mobile/relay && cargo +1.85.0 fmt --check && cargo +1.85.0 clippy --locked --all-targets -- -D warnings && cargo +1.85.0 test --locked`
Expected: PASS.

- [ ] **Step 5: Write the failing oracle and connector tests**

Append to `RelayTests` in `services/mobile/tests/test_relay.py`:

```python
    async def test_set_mobile_delivery_request_validation(self):
        value = {"request_id": str(uuid.uuid4()), "computer_id": self.node["node_id"],
                 "operation": "set_mobile_delivery", "session": "", "payload": {"value": "away_10"}}
        self.assertIn("set_mobile_delivery", (await self.call("GET", "/v1/capabilities"))["operations"])
        await self.call("POST", "/v1/requests", value, expected=409)
        snapshot = {"sessions": [], "mobile_capabilities": {"protocol_version": 1, "operations": ["set_mobile_delivery"], "features": []}}
        await self.call("POST", "/v1/node/heartbeat", {"snapshot": snapshot}, node=True)
        self.assertEqual((await self.call("POST", "/v1/requests", value, expected=202))["state"], "queued")
        delivered = (await self.call("GET", "/v1/node/requests", node=True))["requests"][0]
        self.assertEqual((delivered["operation"], delivered["session"], delivered["payload"]),
                         ("set_mobile_delivery", "", {"value": "away_10"}))
        for change in ({"session": "codex/project/deep/tag"}, {"payload": {"value": "later"}}, {"payload": {}},
                       {"payload": {"value": "away", "request_id": value["request_id"]}}):
            await self.call("POST", "/v1/requests", {**value, "request_id": str(uuid.uuid4()), **change}, expected=400)
```

Add `'set_mobile_delivery_request_validation',` to the name tuple in `services/mobile/tests/test_rust_relay.py`.

Create `services/mobile/tests/test_mobile_delivery.py`:

```python
"""Shared phone delivery setting through the connector, with synthetic native hgs only."""
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import AsyncMock, patch
import uuid

from zerus_mobile.connector import Connector


def command(value="away_10", **extra):
    return {"request_id": str(uuid.uuid4()), "operation": "set_mobile_delivery", "session": "",
            "payload": {"value": value}, **extra}


class MobileDeliveryConnectorTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.connector = Connector({"server_url": "https://relay.example.invalid", "node_token": "fixture-token",
                                    "state_dir": self.temp.name})
        self.connector.preference_supported = True

    async def asyncTearDown(self):
        self.connector.journal.close()
        self.temp.cleanup()

    async def test_exact_native_command_and_receipt_are_returned_once(self):
        request = command()
        with patch.object(self.connector, "native", AsyncMock(return_value={"ok": True, "mobile_delivery": "away_10"})) as native:
            response = await self.connector.execute(request)
            self.assertEqual(response, {"state": "completed", "result": {"ok": True, "mobile_delivery": "away_10"}, "error": None})
            self.assertEqual(await self.connector.execute(request), response)
            native.assert_awaited_once_with(["swarm", "preference", "set", "mobile_delivery", "away_10", "--json"], timeout=10)

    async def test_mismatched_acknowledgement_is_uncertain_and_never_replayed(self):
        request = command()
        with patch.object(self.connector, "native", AsyncMock(return_value={"ok": True, "mobile_delivery": "away"})) as native:
            self.assertEqual((await self.connector.execute(request))["state"], "uncertain")
            self.assertEqual((await self.connector.execute(request))["state"], "uncertain")
            self.assertEqual(native.await_count, 1)

    async def test_invalid_values_sessions_or_fields_never_spawn(self):
        invalid = [command("later"), command(session="codex/example/mobile"), {**command(), "payload": {}},
                   {**command(), "payload": {"value": "away", "request_id": "fixture"}}, {**command(), "payload": ["away"]}]
        with patch.object(self.connector, "native", AsyncMock()) as native:
            for request in invalid:
                self.assertEqual((await self.connector.execute(request))["state"], "failed")
            native.assert_not_awaited()

    async def test_unsupported_native_cli_fails_before_any_mutation(self):
        self.connector.preference_supported = False
        with patch.object(self.connector, "native", AsyncMock()) as native:
            self.assertEqual((await self.connector.execute(command()))["state"], "failed")
            native.assert_not_awaited()


class MobileDeliveryFixtureTests(unittest.IsolatedAsyncioTestCase):
    async def test_only_the_gateway_advertises_and_its_snapshot_reports_the_value(self):
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {"ZERUS_MOBILE_FIXTURE_DIR": directory}):
            connector = Connector({"server_url": "https://relay.example.invalid", "node_token": "fixture-token",
                                   "hgs_path": str(Path(__file__).with_name("fixture_hgs.py")),
                                   "state_dir": str(Path(directory) / "journal")})
            try:
                snapshot = await connector.snapshot()
                self.assertIn("set_mobile_delivery", snapshot["mobile_capabilities"]["operations"])
                self.assertEqual(snapshot["preferences"], {"mobile_delivery": "away"})
                self.assertEqual((await connector.execute(command("away_15")))["state"], "completed")
                self.assertEqual((await connector.snapshot())["preferences"], {"mobile_delivery": "away_15"})
                peer = connector.peer_context(str(uuid.uuid4()), connector.native)
                self.assertNotIn("set_mobile_delivery", (await peer.snapshot())["mobile_capabilities"]["operations"])
            finally:
                connector.journal.close()
```

- [ ] **Step 6: Run them to verify they fail (on `arch`, in the `services/mobile` virtual environment)**

Run: `python -m unittest discover -s services/mobile/tests -p test_mobile_delivery.py -v && python -m unittest discover -s services/mobile/tests -p test_relay.py -k set_mobile_delivery -v`
Expected: FAIL; the connector reports "unsupported operation" and the oracle rejects the operation with 400.

- [ ] **Step 7: Implement the shared Python contract and oracle validation**

In `services/mobile/zerus_mobile/context.py`, after `LAUNCH_OPERATIONS`:

```python
COMPUTER_OPERATIONS = frozenset({"set_mobile_delivery"})
MOBILE_DELIVERY = ("immediate", "away", "away_5", "away_10", "away_15", "away_30")
```

change `OPERATIONS = LAUNCH_OPERATIONS | LIFECYCLE_OPERATIONS | TERMINAL_OPERATIONS | frozenset({...})` to start with `LAUNCH_OPERATIONS | LIFECYCLE_OPERATIONS | TERMINAL_OPERATIONS | COMPUTER_OPERATIONS | frozenset({...})`, and add:

```python
def validate_mobile_delivery(payload):
    """The shared delivery choice targets the gateway computer and carries only its value."""
    if not isinstance(payload, dict) or set(payload) != {"value"} or payload["value"] not in MOBILE_DELIVERY:
        raise ValueError("invalid phone notification timing")
```

In `services/mobile/zerus_mobile/server.py`, import `COMPUTER_OPERATIONS` and `validate_mobile_delivery` from `.context`. In `validate_request`, change the session line to:

```python
    text(value["session"], 1024, empty=value["operation"] in LAUNCH_OPERATIONS or value["operation"] in COMPUTER_OPERATIONS)
```

and after the `if op in LAUNCH_OPERATIONS:` block add:

```python
    if op in COMPUTER_OPERATIONS:
        if value["session"] != "": raise ValueError("the delivery setting uses no session target")
        validate_mobile_delivery(payload)
        return
```

- [ ] **Step 8: Implement the connector**

In `services/mobile/zerus_mobile/connector.py`:

Add `COMPUTER_OPERATIONS` and `validate_mobile_delivery` to the `from .context import ...` line.

In `_init_machine_context`, after `self.worktree_supported = False` add `self.preference_supported = False`.

In `probe_capabilities`, add `self.preference_supported = False` to the resets and, after the `worktree_supported` assignment:

```python
            self.preference_supported = isinstance(help_text,str) and "swarm preference set mobile_delivery" in help_text
```

In `snapshot`, append to the `operations = (...)` expression:

```python
 - (set() if self.preference_supported and getattr(self, "fleet", None) is not None else COMPUTER_OPERATIONS)
```

(only the gateway context owns a `fleet`; peer contexts never advertise it).

In `validate`, after the unsupported-operation check:

```python
        if operation in COMPUTER_OPERATIONS:
            if session != "": raise ConnectorError("the delivery setting uses no session target")
            try: validate_mobile_delivery(request.get("payload"))
            except ValueError as error: raise ConnectorError(str(error)) from None
            return operation, session, request["payload"]
```

In `_execute_claimed`, after `if operation in LAUNCH_OPERATIONS and not self.launch_supported: ...`:

```python
            if operation in COMPUTER_OPERATIONS and (not self.preference_supported or getattr(self, "fleet", None) is None):
                raise ConnectorError("the shared delivery setting is changed on the gateway computer with a current hgs")
```

Before `            elif operation == "inspect":` insert:

```python
            elif operation == "set_mobile_delivery":
                result = await self.native(["swarm", "preference", "set", "mobile_delivery", payload["value"], "--json"], timeout=10)
                if not isinstance(result, dict) or result.get("ok") is not True or result.get("mobile_delivery") != payload["value"]:
                    raise ConnectorError("native delivery setting acknowledgement did not match")
```

In `services/mobile/tests/fixture_hgs.py`, append `\nhgs swarm preference set mobile_delivery VALUE --json` to the `--help` text, add `"preferences": json.loads((root / "preferences.json").read_text()) if (root / "preferences.json").exists() else {"mobile_delivery": "away"}` to the `ls --json --local` result dict, and before the final `else: raise SystemExit("unsupported fixture operation")` add:

```python
    elif args[:4] == ["swarm", "preference", "set", "mobile_delivery"] and len(args) == 6 and args[5] == "--json":
        assert args[4] in ("immediate", "away", "away_5", "away_10", "away_15", "away_30")
        (root / "preferences.json").write_text(json.dumps({"mobile_delivery": args[4]}))
        result = {"ok": True, "mobile_delivery": args[4]}
```

- [ ] **Step 9: Run the service suites (on `arch`)**

Run: `python -m unittest discover -s services/mobile/tests -v` and then `cd services/mobile/relay && cargo +1.85.0 build --locked && cd - && ZERUS_RELAY_BINARY=$PWD/services/mobile/relay/target/debug/zerus-relay python -m unittest discover -s services/mobile/tests -p test_rust_relay.py -v`
Expected: PASS, including `test_mobile_context` (its `set(...) == OPERATIONS` assertions now include `set_mobile_delivery` through the fixture marker).

- [ ] **Step 10: Document the operation**

In `docs/mobile-architecture.md` "Protocol version 1", change the additive list ending "`worktree_create` and `recovery_action`." to "`worktree_create`, `recovery_action` and `set_mobile_delivery`." and add after the paragraph:

```markdown
`set_mobile_delivery` is computer-scoped: an empty session and payload
`{value}` with `immediate`, `away`, `away_5`, `away_10`, `away_15` or `away_30`.
The gateway connector runs `hgs swarm preference set mobile_delivery VALUE --json`
and returns its `{ok, mobile_delivery}` object. Only a gateway whose hgs lists that
command advertises the operation; direct peers never do. Snapshots report the
current value as `preferences.mobile_delivery`, and each machine's desktop activity
as `desktop_presence`.
```

- [ ] **Step 11: Commit**

```bash
git add services/mobile docs/mobile-architecture.md
git commit -m "Add the set_mobile_delivery relay operation"
```

---

### Task 5: Desktop presence controller and hgs writer

**Files:**
- Create: `tray/src/DesktopPresence.h`, `tray/src/DesktopPresence.cpp`
- Modify: `tray/src/HgsClient.h:116-117, 201-202, 276-277`, `tray/src/HgsClient.cpp` (after `requestSwarm`)
- Modify: `tray/CMakeLists.txt:76` (add `src/DesktopPresence.cpp` after `src/AttentionTracker.cpp`)
- Create: `tray/tests/test_desktoppresence.cpp`
- Modify: `tray/tests/test_hgsclient.cpp:37` and before `QTEST_GUILESS_MAIN`
- Modify: `tray/tests/CMakeLists.txt` (append)

**Interfaces:**
- Consumes: Task 1 `hgs presence set --json`.
- Produces:
  - `struct PresenceSample { bool available = false; double idleSeconds = 0; bool locked = false; };`
  - `class PresenceSource : public QObject { virtual void sample() = 0; signals: void sampled(const PresenceSample &sample); };`
  - `std::unique_ptr<PresenceSource> makePresenceSource();` (declared here, defined per platform in Tasks 6 and 7)
  - `class DesktopPresence : public QObject` with `SampleMs = 30000`, `CheckMs = 5000`, `ActiveSeconds = 120`, `using Writer = std::function<void(double idleSeconds, bool locked)>`, `using Clock = std::function<qint64()>`, constructor `DesktopPresence(std::unique_ptr<PresenceSource> source, Writer writer, Clock clock = {}, QObject *parent = nullptr)`, `void start(int sampleMs = SampleMs, int checkMs = CheckMs)`, `void poll(bool periodic)`, `void noteInput()`, `static bool transition(double writtenIdle, bool writtenLocked, double elapsedSeconds, double idle, bool locked)`.
  - `void HgsClient::setPresence(double idleSeconds, bool locked)` and signal `void HgsClient::presenceFinished(const QString &error)` (empty on success).

- [ ] **Step 1: Write the failing controller test**

Create `tray/tests/test_desktoppresence.cpp`:

```cpp
#include <QtTest>
#include "DesktopPresence.h"

class FakeSource : public PresenceSource {
public:
    PresenceSample next{true, 0, false};
    bool immediate = true;
    int requests = 0;
    void sample() override { ++requests; if (immediate) emit sampled(next); }
    void answer() { emit sampled(next); }
};

struct Write { double idle; bool locked; };

class TestDesktopPresence : public QObject {
    Q_OBJECT
private slots:
    void transitionsWriteOnlyLockChangesAndReturns();
    void periodicSamplesAlwaysWriteAndChecksWriteOnlyTransitions();
    void slowSourcesAreNotQueuedTwice();
    void withoutPlatformIdleOnlyZerusInputCounts();
};

void TestDesktopPresence::transitionsWriteOnlyLockChangesAndReturns()
{
    QVERIFY(DesktopPresence::transition(10, false, 0, 10, true));
    QVERIFY(DesktopPresence::transition(10, true, 0, 10, false));
    QVERIFY(DesktopPresence::transition(150, false, 0, 3, false));
    QVERIFY(DesktopPresence::transition(30, false, 100, 2, false));
    QVERIFY(!DesktopPresence::transition(30, false, 20, 2, false));
    QVERIFY(!DesktopPresence::transition(10, false, 200, 210, false));
    QVERIFY(!DesktopPresence::transition(300, true, 0, 1, true));
}

void TestDesktopPresence::periodicSamplesAlwaysWriteAndChecksWriteOnlyTransitions()
{
    qint64 now = 0; QList<Write> writes;
    auto *fake = new FakeSource;
    DesktopPresence presence(std::unique_ptr<PresenceSource>(fake),
        [&writes](double idle, bool locked) { writes.append({idle, locked}); }, [&now] { return now; });
    presence.start(3600000, 3600000);
    QCOMPARE(writes.size(), 1); QCOMPARE(writes[0].idle, 0.0);
    fake->next = {true, 4, false}; now = 5000; presence.poll(false); QCOMPARE(writes.size(), 1);
    fake->next = {true, 4, true}; presence.poll(false); QCOMPARE(writes.size(), 2); QVERIFY(writes[1].locked);
    fake->next = {true, 35, true}; now = 35000; presence.poll(true); QCOMPARE(writes.size(), 3);
    fake->next = {true, 1, false}; presence.poll(false); QCOMPARE(writes.size(), 4); QVERIFY(!writes[3].locked);
    fake->next = {true, 200, false}; now = 300000; presence.poll(false); QCOMPARE(writes.size(), 4);
    fake->next = {true, 2, false}; presence.poll(false); QCOMPARE(writes.size(), 5); QCOMPARE(writes[4].idle, 2.0);
}

void TestDesktopPresence::slowSourcesAreNotQueuedTwice()
{
    qint64 now = 0; QList<Write> writes;
    auto *fake = new FakeSource; fake->immediate = false;
    DesktopPresence presence(std::unique_ptr<PresenceSource>(fake),
        [&writes](double idle, bool locked) { writes.append({idle, locked}); }, [&now] { return now; });
    presence.poll(true); presence.poll(false); presence.poll(true);
    QCOMPARE(fake->requests, 1);
    fake->answer(); QCOMPARE(writes.size(), 1);
    fake->answer(); QCOMPARE(writes.size(), 1); // an unsolicited reply is ignored
    now = 1000; presence.poll(false); presence.poll(false); QCOMPARE(fake->requests, 2);
    now = 12000; presence.poll(false); QCOMPARE(fake->requests, 3); // a hung source is asked again
}

void TestDesktopPresence::withoutPlatformIdleOnlyZerusInputCounts()
{
    qint64 now = 0; QList<Write> writes; QObject target;
    DesktopPresence presence(nullptr,
        [&writes](double idle, bool locked) { writes.append({idle, locked}); }, [&now] { return now; });
    now = 30000; presence.poll(true);
    QCOMPARE(writes.last().idle, 150.0); // no input here yet: away
    now = 40000; presence.noteInput();
    QCOMPARE(writes.size(), 2); QCOMPARE(writes.last().idle, 0.0);
    now = 50000; presence.noteInput(); QCOMPARE(writes.size(), 2);
    now = 60000; QEvent key(QEvent::KeyPress); QCoreApplication::sendEvent(&target, &key);
    now = 400000; presence.poll(true); QCOMPARE(writes.last().idle, 340.0);
}

QTEST_GUILESS_MAIN(TestDesktopPresence)
#include "test_desktoppresence.moc"
```

Append to `tray/tests/CMakeLists.txt`:

```cmake
add_executable(test_desktoppresence test_desktoppresence.cpp ../src/DesktopPresence.cpp)
target_include_directories(test_desktoppresence PRIVATE ../src)
target_link_libraries(test_desktoppresence PRIVATE Qt6::Test Qt6::Core)
add_test(NAME desktoppresence COMMAND test_desktoppresence)
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cmake -S tray -B .ci-build/gui -DBUILD_TESTING=ON $( [ "$(uname -s)" = Darwin ] && echo -DCMAKE_PREFIX_PATH=$(brew --prefix qt) ) && cmake --build .ci-build/gui --target test_desktoppresence`
Expected: FAIL to configure or compile ("DesktopPresence.h: No such file").

- [ ] **Step 3: Implement the controller**

Create `tray/src/DesktopPresence.h`:

```cpp
#pragma once

#include <QElapsedTimer>
#include <QMetaType>
#include <QObject>
#include <QTimer>
#include <functional>
#include <memory>

struct PresenceSample {
    bool available = false; // false: the platform reports no idle time
    double idleSeconds = 0;
    bool locked = false;
};
Q_DECLARE_METATYPE(PresenceSample)

// Reads OS input idle time and screen lock. Every sample() emits sampled()
// once, possibly later: D-Bus replies are asynchronous.
class PresenceSource : public QObject {
    Q_OBJECT
public:
    using QObject::QObject;
    virtual void sample() = 0;
signals:
    void sampled(const PresenceSample &sample);
};

// Defined per platform (LinuxPresenceSource.cpp, MacPresenceSource.cpp).
std::unique_ptr<PresenceSource> makePresenceSource();

// Records whether the owner uses this desktop, for phone alert decisions. The
// tray agent runs it with the window closed. It writes every 30 s and at once
// when the screen locks or unlocks or the owner returns after being away;
// listings age the last value, so going idle needs no write.
class DesktopPresence : public QObject {
    Q_OBJECT
public:
    static constexpr int SampleMs = 30000;
    static constexpr int CheckMs = 5000;
    static constexpr double ActiveSeconds = 120;
    using Writer = std::function<void(double idleSeconds, bool locked)>;
    using Clock = std::function<qint64()>;
    DesktopPresence(std::unique_ptr<PresenceSource> source, Writer writer, Clock clock = {}, QObject *parent = nullptr);
    void start(int sampleMs = SampleMs, int checkMs = CheckMs);
    // Timers call this; tests drive it directly. Periodic samples always write.
    void poll(bool periodic);
    // Input in a Zerus window: the only activity signal when the platform has none.
    void noteInput();
    static bool transition(double writtenIdle, bool writtenLocked, double elapsedSeconds, double idle, bool locked);
protected:
    bool eventFilter(QObject *watched, QEvent *event) override;
private:
    void sampled(const PresenceSample &sample);
    std::unique_ptr<PresenceSource> m_source;
    Writer m_writer;
    Clock m_clock;
    QElapsedTimer m_monotonic;
    QTimer m_sampleTimer, m_checkTimer;
    qint64 m_started = 0, m_lastInput = -1, m_requestedAt = -1, m_writtenAt = -1;
    double m_writtenIdle = 0;
    bool m_writtenLocked = false, m_periodic = false;
};
```

Create `tray/src/DesktopPresence.cpp`:

```cpp
#include "DesktopPresence.h"

#include <QCoreApplication>
#include <QEvent>
#include <utility>

DesktopPresence::DesktopPresence(std::unique_ptr<PresenceSource> source, Writer writer, Clock clock, QObject *parent)
    : QObject(parent), m_source(std::move(source)), m_writer(std::move(writer)), m_clock(std::move(clock))
{
    if (!m_clock) { m_monotonic.start(); m_clock = [this] { return m_monotonic.elapsed(); }; }
    m_started = m_clock();
    if (m_source) connect(m_source.get(), &PresenceSource::sampled, this, &DesktopPresence::sampled);
    connect(&m_sampleTimer, &QTimer::timeout, this, [this] { poll(true); });
    connect(&m_checkTimer, &QTimer::timeout, this, [this] { poll(false); });
    if (auto *app = QCoreApplication::instance()) app->installEventFilter(this);
}

void DesktopPresence::start(int sampleMs, int checkMs)
{
    m_sampleTimer.start(sampleMs);
    m_checkTimer.start(checkMs);
    poll(true);
}

void DesktopPresence::poll(bool periodic)
{
    m_periodic = m_periodic || periodic;
    const qint64 now = m_clock();
    // One sample at a time; a source that never answers is asked again after 10 s.
    if (m_requestedAt >= 0 && now - m_requestedAt < 10000) return;
    m_requestedAt = now;
    if (m_source) m_source->sample(); else sampled({});
}

void DesktopPresence::sampled(const PresenceSample &sample)
{
    if (m_requestedAt < 0) return;
    m_requestedAt = -1;
    const qint64 now = m_clock();
    double idle = sample.idleSeconds;
    if (!sample.available) {
        // Only Zerus windows count. Before any input here the owner counts as
        // away: an extra phone alert is better than a missed one.
        idle = m_lastInput < 0 ? ActiveSeconds + (now - m_started) / 1000.0 : (now - m_lastInput) / 1000.0;
    }
    idle = qMax(0.0, idle);
    const bool periodic = std::exchange(m_periodic, false);
    if (!periodic && m_writtenAt >= 0
        && !transition(m_writtenIdle, m_writtenLocked, (now - m_writtenAt) / 1000.0, idle, sample.locked))
        return;
    m_writtenAt = now; m_writtenIdle = idle; m_writtenLocked = sample.locked;
    m_writer(idle, sample.locked);
}

bool DesktopPresence::transition(double writtenIdle, bool writtenLocked, double elapsedSeconds, double idle, bool locked)
{
    if (locked != writtenLocked) return true;
    return !locked && writtenIdle + elapsedSeconds >= ActiveSeconds && idle < ActiveSeconds;
}

void DesktopPresence::noteInput()
{
    const qint64 now = m_clock();
    const bool returning = m_lastInput < 0 || now - m_lastInput >= qint64(ActiveSeconds * 1000);
    m_lastInput = now;
    if (returning) poll(false);
}

bool DesktopPresence::eventFilter(QObject *watched, QEvent *event)
{
    switch (event->type()) {
    case QEvent::KeyPress: case QEvent::MouseButtonPress: case QEvent::MouseMove:
    case QEvent::Wheel: case QEvent::TouchBegin:
        noteInput();
        break;
    default:
        break;
    }
    return QObject::eventFilter(watched, event);
}
```

Add `src/DesktopPresence.cpp` to the `hgs-tray` source list in `tray/CMakeLists.txt` after `src/AttentionTracker.cpp`.

- [ ] **Step 4: Run the controller test**

Run: `cmake --build .ci-build/gui --target test_desktoppresence && ctest --test-dir .ci-build/gui -R desktoppresence --output-on-failure`
Expected: PASS (4 functions).

- [ ] **Step 5: Write the failing writer test**

In `tray/tests/test_hgsclient.cpp` add `void presenceWritesAreCoalescedAndReportFailures();` after `void localPollingReportsStartAndRecoversFromFailure();` and before `QTEST_GUILESS_MAIN(TestHgsClient)` add:

```cpp
void TestHgsClient::presenceWritesAreCoalescedAndReportFailures()
{
    QTemporaryDir dir; QVERIFY(dir.isValid());
    const QString path = dir.filePath("hgs");
    QFile script(path); QVERIFY(script.open(QIODevice::WriteOnly));
    script.write(R"(#!/usr/bin/env python3
import json, pathlib, sys, time
root = pathlib.Path(__file__).parent
payload = json.load(sys.stdin)
with (root / 'calls').open('a') as file: file.write(json.dumps([sys.argv[1:], payload]) + '\n')
time.sleep(0.2)
if (root / 'fail').exists(): print('presence unavailable', file=sys.stderr); sys.exit(1)
print('{"ok":true}')
)");
    script.close(); QVERIFY(script.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner));
    HgsClient client(path); QSignalSpy finished(&client, &HgsClient::presenceFinished);
    client.setPresence(1, false); client.setPresence(2, false); client.setPresence(-3, true);
    QTRY_COMPARE_WITH_TIMEOUT(finished.size(), 2, 10000);
    QFile calls(dir.filePath("calls")); QVERIFY(calls.open(QIODevice::ReadOnly));
    const auto lines = calls.readAll().trimmed().split('\n'); QCOMPARE(lines.size(), 2);
    const auto first = QJsonDocument::fromJson(lines[0]).array(), last = QJsonDocument::fromJson(lines[1]).array();
    QCOMPARE(first[0].toArray(), (QJsonArray{"presence", "set", "--json"}));
    QCOMPARE(first[1].toObject(), (QJsonObject{{"idle_seconds", 1}, {"locked", false}}));
    QCOMPARE(last[1].toObject(), (QJsonObject{{"idle_seconds", 0}, {"locked", true}})); // newest only, clamped
    QVERIFY(finished[0][0].toString().isEmpty()); QVERIFY(finished[1][0].toString().isEmpty());
    QFile fail(dir.filePath("fail")); QVERIFY(fail.open(QIODevice::WriteOnly)); fail.close();
    client.setPresence(5, false);
    QTRY_COMPARE_WITH_TIMEOUT(finished.size(), 3, 10000);
    QVERIFY(finished[2][0].toString().contains("presence unavailable"));
}
```

Add `#include <QJsonArray>` and `#include <QJsonObject>` to the includes.

- [ ] **Step 6: Run it to verify it fails**

Run: `cmake --build .ci-build/gui --target test_hgsclient`
Expected: FAIL to compile ("no member named 'setPresence'").

- [ ] **Step 7: Implement the coalesced writer**

In `tray/src/HgsClient.h`, after `requestSwarm(...)` declare:

```cpp
    // `hgs presence set --json`. At most one write runs; while it does, only the
    // newest sample waits, so a slow CLI never builds a queue of stale samples.
    void setPresence(double idleSeconds, bool locked);
```

after `void swarmFailed(...)` add `void presenceFinished(const QString &error); // Empty on success.`, and among the private members add:

```cpp
    bool m_presenceInFlight = false, m_presencePending = false;
    QPair<double, bool> m_pendingPresence;
```

In `tray/src/HgsClient.cpp`, after `HgsClient::requestSwarm`:

```cpp
void HgsClient::setPresence(double idleSeconds, bool locked)
{
    if (m_presenceInFlight) { m_pendingPresence = qMakePair(idleSeconds, locked); m_presencePending = true; return; }
    m_presenceInFlight = true;
    const auto input = QJsonDocument(QJsonObject{{"idle_seconds", qMax(0.0, idleSeconds)}, {"locked", locked}}).toJson(QJsonDocument::Compact);
    const auto finished = [this](const QString &error) {
        m_presenceInFlight = false;
        emit presenceFinished(error);
        if (std::exchange(m_presencePending, false)) setPresence(m_pendingPresence.first, m_pendingPresence.second);
    };
    runHgs(m_hgs, {QStringLiteral("presence"), QStringLiteral("set"), QStringLiteral("--json")}, 5000, this,
        [finished](const QByteArray &) { finished({}); },
        [finished](const QString &, const QString &detail) { finished(detail); }, input);
}
```

Add `#include <utility>` if absent.

- [ ] **Step 8: Run both tests**

Run: `cmake --build .ci-build/gui --target test_hgsclient test_desktoppresence && ctest --test-dir .ci-build/gui -R "hgsclient|desktoppresence" --output-on-failure`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add tray/src/DesktopPresence.h tray/src/DesktopPresence.cpp tray/src/HgsClient.h tray/src/HgsClient.cpp tray/CMakeLists.txt tray/tests/test_desktoppresence.cpp tray/tests/test_hgsclient.cpp tray/tests/CMakeLists.txt
git commit -m "Sample desktop presence and write it through hgs"
```

---

### Task 6: Linux idle and lock source

Run and validate this task on `arch`.

**Files:**
- Create: `tray/src/LinuxPresenceSource.h`, `tray/src/LinuxPresenceSource.cpp`
- Modify: `tray/CMakeLists.txt:26-32` (find KF6IdleTime), `101-110` (sources), `131-133` (link)
- Create: `tray/tests/test_linuxpresence.cpp`
- Modify: `tray/tests/CMakeLists.txt:7-21` (Linux block)
- Modify: `packaging/aur/zerus/PKGBUILD.in:9`, `packaging/aur/zerus-git/PKGBUILD:9`, `packaging/aur/zerus-git/.SRCINFO:15-16`, `scripts/release_common.py:12-13`, `.github/workflows/checks.yml:210`, `.github/workflows/release.yml:71`, `.github/workflows/nightly.yml:44`, `docs/installation.md:23-24, 42-43`, `docs/ci-and-aur.md:338`, `THIRD_PARTY_NOTICES.md:37-38`

**Interfaces:**
- Consumes: `PresenceSource`, `PresenceSample` from Task 5.
- Produces: `class LinuxPresenceSource : public PresenceSource` with `ThresholdMs = 10000`, `TimeoutMs = 2000`, constructor `LinuxPresenceSource(QDBusConnection bus, std::function<qint64()> clock = {}, QObject *parent = nullptr)`, `void thresholdReached()`, `void resumed()`; Linux definition of `makePresenceSource()`.

- [ ] **Step 1: Write the failing D-Bus test**

Create `tray/tests/test_linuxpresence.cpp`:

```cpp
#include <QtTest>
#include <QDBusConnection>
#include <QDBusContext>
#include <QDBusError>
#include <optional>
#include "LinuxPresenceSource.h"

class ScreenSaver : public QObject, protected QDBusContext {
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.freedesktop.ScreenSaver")
public:
    bool active = false, idleSupported = true; uint idle = 0;
public slots:
    bool GetActive() { return active; }
    uint GetSessionIdleTime() {
        if (!idleSupported) sendErrorReply(QDBusError::NotSupported, "GetSessionIdleTime is not supported on this platform");
        return idle;
    }
};
class MutterIdle : public QObject {
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.gnome.Mutter.IdleMonitor")
public:
    quint64 idle = 0;
public slots:
    quint64 GetIdletime() { return idle; }
};
class GnomeScreenSaver : public QObject {
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.gnome.ScreenSaver")
public:
    bool active = false;
public slots:
    bool GetActive() { return active; }
};

class TestLinuxPresence : public QObject {
    Q_OBJECT
    static std::optional<PresenceSample> sample(LinuxPresenceSource &source) {
        std::optional<PresenceSample> result;
        const auto connection = QObject::connect(&source, &PresenceSource::sampled, [&result](const PresenceSample &s) { result = s; });
        source.sample();
        QTest::qWaitFor([&result] { return result.has_value(); }, 5000);
        QObject::disconnect(connection);
        return result;
    }
    static QDBusConnection serve(const QString &name, const QString &service, const QString &path, QObject *object) {
        auto bus = QDBusConnection::connectToBus(QDBusConnection::SessionBus, name);
        if (!bus.registerService(service) || !bus.registerObject(path, object, QDBusConnection::ExportAllSlots)) qFatal("fake D-Bus service failed");
        return bus;
    }
private slots:
    void cleanup() {
        for (const auto *name : {"fake-screensaver", "fake-mutter", "fake-gnome"}) QDBusConnection::disconnectFromBus(name);
    }
    void x11ScreenSaverReportsIdleSecondsAndLock() {
        ScreenSaver saver; saver.idle = 42; saver.active = true;
        serve("fake-screensaver", "org.freedesktop.ScreenSaver", "/org/freedesktop/ScreenSaver", &saver);
        LinuxPresenceSource source(QDBusConnection::sessionBus());
        const auto result = sample(source); QVERIFY(result);
        QVERIFY(result->available); QCOMPARE(result->idleSeconds, 42.0); QVERIFY(result->locked);
    }
    void waylandIdleNeedsVerifiedThresholdEvents() {
        ScreenSaver saver; saver.idleSupported = false;
        serve("fake-screensaver", "org.freedesktop.ScreenSaver", "/org/freedesktop/ScreenSaver", &saver);
        qint64 now = 1000000;
        LinuxPresenceSource source(QDBusConnection::sessionBus(), [&now] { return now; });
        auto result = sample(source); QVERIFY(result);
        QVERIFY(!result->available); QVERIFY(!result->locked); // unverified thresholds never mean "active"
        source.thresholdReached(); now += 20000;
        result = sample(source); QVERIFY(result && result->available); QCOMPARE(result->idleSeconds, 30.0);
        source.resumed();
        result = sample(source); QVERIFY(result && result->available); QCOMPARE(result->idleSeconds, 0.0);
    }
    void gnomeUsesMutterMillisecondsAndItsLock() {
        MutterIdle mutter; mutter.idle = 90500; GnomeScreenSaver gnome; gnome.active = true;
        serve("fake-mutter", "org.gnome.Mutter.IdleMonitor", "/org/gnome/Mutter/IdleMonitor/Core", &mutter);
        serve("fake-gnome", "org.gnome.ScreenSaver", "/org/gnome/ScreenSaver", &gnome);
        LinuxPresenceSource source(QDBusConnection::sessionBus());
        const auto result = sample(source); QVERIFY(result);
        QVERIFY(result->available); QCOMPARE(result->idleSeconds, 90.5); QVERIFY(result->locked);
    }
    void missingServicesReportUnavailable() {
        LinuxPresenceSource source(QDBusConnection::sessionBus());
        const auto result = sample(source); QVERIFY(result);
        QVERIFY(!result->available); QVERIFY(!result->locked);
    }
};

QTEST_GUILESS_MAIN(TestLinuxPresence)
#include "test_linuxpresence.moc"
```

In the `if(LINUX)` block of `tray/tests/CMakeLists.txt` add:

```cmake
    add_executable(test_linuxpresence test_linuxpresence.cpp ../src/LinuxPresenceSource.cpp ../src/DesktopPresence.cpp)
    target_include_directories(test_linuxpresence PRIVATE ../src)
    target_link_libraries(test_linuxpresence PRIVATE Qt6::Test Qt6::Core Qt6::DBus KF6::IdleTime)
    add_test(NAME linuxpresence COMMAND ${DBUS_RUN_SESSION} -- $<TARGET_FILE:test_linuxpresence>)
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cmake -S tray -B .ci-build/gui -DBUILD_TESTING=ON && cmake --build .ci-build/gui --target test_linuxpresence`
Expected: FAIL ("LinuxPresenceSource.h: No such file" or missing `KF6::IdleTime` target).

- [ ] **Step 3: Implement the source**

In `tray/CMakeLists.txt` add `find_package(KF6IdleTime REQUIRED)` after `find_package(KF6WindowSystem REQUIRED)`, `target_sources(hgs-tray PRIVATE src/LinuxPresenceSource.cpp)` after `src/LinuxApplicationBadge.cpp`, and `KF6::IdleTime` to the Linux `target_link_libraries(hgs-tray ...)`.

Create `tray/src/LinuxPresenceSource.h`:

```cpp
#pragma once

#include "DesktopPresence.h"

#include <QDBusConnection>
#include <QElapsedTimer>
#include <functional>
#include <optional>

class QDBusMessage;

// org.freedesktop.ScreenSaver reports the lock everywhere and idle time on X11;
// GNOME reports idle time through Mutter. KDE Plasma on Wayland refuses
// GetSessionIdleTime, so compositor idle thresholds (KIdleTime) fill the gap,
// but only after they delivered an event: a missing plugin must not look like
// constant activity. Calls are asynchronous and never start services.
class LinuxPresenceSource : public PresenceSource {
    Q_OBJECT
public:
    static constexpr int ThresholdMs = 10000;
    static constexpr int TimeoutMs = 2000;
    explicit LinuxPresenceSource(QDBusConnection bus, std::function<qint64()> clock = {}, QObject *parent = nullptr);
    void sample() override;
    // Input stopped for ThresholdMs / input resumed (KIdleTime).
    void thresholdReached();
    void resumed();
private:
    void call(const QString &service, const QString &path, const QString &interface, const QString &method,
              std::function<void(const QDBusMessage &)> done);
    std::optional<double> thresholdIdle() const;
    QDBusConnection m_bus;
    std::function<qint64()> m_clock;
    QElapsedTimer m_monotonic;
    bool m_thresholdsVerified = false;
    qint64 m_idleSince = -1;
};
```

Create `tray/src/LinuxPresenceSource.cpp`:

```cpp
#include "LinuxPresenceSource.h"

#include <KIdleTime>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <memory>

namespace {
std::optional<bool> boolean(const QDBusMessage &reply)
{
    if (reply.type() != QDBusMessage::ReplyMessage || reply.arguments().size() != 1) return std::nullopt;
    const QVariant value = reply.arguments().constFirst();
    if (value.metaType().id() != QMetaType::Bool) return std::nullopt;
    return value.toBool();
}

std::optional<double> number(const QDBusMessage &reply, double perSecond)
{
    if (reply.type() != QDBusMessage::ReplyMessage || reply.arguments().size() != 1) return std::nullopt;
    const QVariant value = reply.arguments().constFirst();
    const int type = value.metaType().id();
    if (type != QMetaType::UInt && type != QMetaType::ULongLong && type != QMetaType::Int && type != QMetaType::LongLong)
        return std::nullopt;
    bool ok = false;
    const double result = value.toULongLong(&ok) / perSecond;
    if (!ok) return std::nullopt;
    return result;
}
}

LinuxPresenceSource::LinuxPresenceSource(QDBusConnection bus, std::function<qint64()> clock, QObject *parent)
    : PresenceSource(parent), m_bus(std::move(bus)), m_clock(std::move(clock))
{
    if (!m_clock) { m_monotonic.start(); m_clock = [this] { return m_monotonic.elapsed(); }; }
}

void LinuxPresenceSource::sample()
{
    struct Replies {
        int remaining = 4;
        std::optional<bool> freedesktopLock, gnomeLock;
        std::optional<double> freedesktopIdle, mutterIdle;
    };
    auto replies = std::make_shared<Replies>();
    const auto finished = [this, replies] {
        if (--replies->remaining) return;
        PresenceSample result;
        result.locked = replies->freedesktopLock.value_or(false) || replies->gnomeLock.value_or(false);
        auto idle = replies->freedesktopIdle ? replies->freedesktopIdle : replies->mutterIdle;
        if (!idle) idle = thresholdIdle();
        if (idle) { result.available = true; result.idleSeconds = *idle; }
        emit sampled(result);
    };
    const QString screenSaver = QStringLiteral("org.freedesktop.ScreenSaver");
    const QString screenSaverPath = QStringLiteral("/org/freedesktop/ScreenSaver");
    call(screenSaver, screenSaverPath, screenSaver, QStringLiteral("GetActive"),
         [replies, finished](const QDBusMessage &reply) { replies->freedesktopLock = boolean(reply); finished(); });
    call(screenSaver, screenSaverPath, screenSaver, QStringLiteral("GetSessionIdleTime"),
         [replies, finished](const QDBusMessage &reply) { replies->freedesktopIdle = number(reply, 1); finished(); });
    call(QStringLiteral("org.gnome.Mutter.IdleMonitor"), QStringLiteral("/org/gnome/Mutter/IdleMonitor/Core"),
         QStringLiteral("org.gnome.Mutter.IdleMonitor"), QStringLiteral("GetIdletime"),
         [replies, finished](const QDBusMessage &reply) { replies->mutterIdle = number(reply, 1000); finished(); });
    call(QStringLiteral("org.gnome.ScreenSaver"), QStringLiteral("/org/gnome/ScreenSaver"),
         QStringLiteral("org.gnome.ScreenSaver"), QStringLiteral("GetActive"),
         [replies, finished](const QDBusMessage &reply) { replies->gnomeLock = boolean(reply); finished(); });
}

void LinuxPresenceSource::call(const QString &service, const QString &path, const QString &interface, const QString &method,
                               std::function<void(const QDBusMessage &)> done)
{
    auto message = QDBusMessage::createMethodCall(service, path, interface, method);
    message.setAutoStartService(false);
    auto *watcher = new QDBusPendingCallWatcher(m_bus.asyncCall(message, TimeoutMs), this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this, [watcher, done = std::move(done)] {
        done(watcher->reply());
        watcher->deleteLater();
    });
}

void LinuxPresenceSource::thresholdReached()
{
    m_thresholdsVerified = true;
    m_idleSince = m_clock() - ThresholdMs;
}

void LinuxPresenceSource::resumed()
{
    m_thresholdsVerified = true;
    m_idleSince = -1;
}

std::optional<double> LinuxPresenceSource::thresholdIdle() const
{
    if (!m_thresholdsVerified) return std::nullopt;
    // Between events the last input is less than ThresholdMs old. Zero delays
    // "away" by at most that much; it never causes a missed phone alert.
    return m_idleSince < 0 ? 0.0 : (m_clock() - m_idleSince) / 1000.0;
}

std::unique_ptr<PresenceSource> makePresenceSource()
{
    auto source = std::make_unique<LinuxPresenceSource>(QDBusConnection::sessionBus());
    auto *idle = KIdleTime::instance();
    const int threshold = idle->addIdleTimeout(LinuxPresenceSource::ThresholdMs);
    auto *raw = source.get();
    QObject::connect(idle, &KIdleTime::timeoutReached, raw, [raw, threshold](int identifier, int) {
        if (identifier != threshold) return;
        raw->thresholdReached();
        KIdleTime::instance()->catchNextResumeEvent();
    });
    QObject::connect(idle, &KIdleTime::resumingFromIdle, raw, [raw] { raw->resumed(); });
    return source;
}
```

- [ ] **Step 4: Run the Linux tests**

Run: `cmake -S tray -B .ci-build/gui -DBUILD_TESTING=ON && cmake --build .ci-build/gui --target hgs-tray test_linuxpresence && ctest --test-dir .ci-build/gui -R linuxpresence --output-on-failure`
Expected: PASS (4 functions), and `hgs-tray` links with `KF6::IdleTime`.

- [ ] **Step 5: Declare the new dependency everywhere `kwindowsystem` is**

- `packaging/aur/zerus/PKGBUILD.in` and `packaging/aur/zerus-git/PKGBUILD`: `depends=('qt6-base' 'qt6-webengine' 'qt6-svg' 'kstatusnotifieritem' 'kwindowsystem' 'kidletime'`.
- `packaging/aur/zerus-git/.SRCINFO`: add `	depends = kidletime` after `	depends = kwindowsystem`.
- `scripts/release_common.py`: `BINARY_LIBRARIES = ("qt6-base", "qt6-webengine", "qt6-svg", "kstatusnotifieritem", "kwindowsystem", "kidletime", "libgcc", "libstdc++", "glibc")`.
- `.github/workflows/checks.yml`, `release.yml`, `nightly.yml`: change `kstatusnotifieritem kwindowsystem` to `kstatusnotifieritem kwindowsystem kidletime` in the `pacman -Syu` lines.
- `docs/installation.md`: "Linux also needs KDE Frameworks 6 KStatusNotifierItem, KWindowSystem and KIdleTime" and add `kidletime` to the Arch package list.
- `docs/ci-and-aur.md` Desktop runtime row: add `kidletime` after `kwindowsystem` and "idle detection" to its purpose.
- `THIRD_PARTY_NOTICES.md`: "the KDE Frameworks KStatusNotifierItem, KWindowSystem and KIdleTime".

- [ ] **Step 6: Validate packaging and workflow edits**

Run: `python3 scripts/ci/check-source.py && python3 -m unittest discover -s tests -p 'test_*publication.py' && python3 -m unittest discover -s tests -p 'test_ci_security.py' && (command -v actionlint >/dev/null && actionlint || echo "actionlint unavailable; recorded")`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add tray/src/LinuxPresenceSource.h tray/src/LinuxPresenceSource.cpp tray/CMakeLists.txt tray/tests/test_linuxpresence.cpp tray/tests/CMakeLists.txt packaging scripts/release_common.py .github/workflows docs/installation.md docs/ci-and-aur.md THIRD_PARTY_NOTICES.md
git commit -m "Read Linux desktop idle time and screen lock"
```

---

### Task 7: macOS idle and lock source; start presence in the tray agent

Run on the Mac; then rebuild on `arch` (Task 11).

**Files:**
- Create: `tray/src/MacPresenceSource.cpp`
- Modify: `tray/CMakeLists.txt:114-126` (Apple sources and frameworks)
- Create: `tray/tests/test_macpresence.cpp`
- Modify: `tray/tests/CMakeLists.txt` (Apple block)
- Modify: `tray/src/TrayAgent.h:11-21, 100-107`, `tray/src/TrayAgent.cpp:229-235`
- Modify: `docs/reference.md` ("Desktop workspace" section, short paragraph)

**Interfaces:**
- Consumes: `PresenceSource`, `DesktopPresence`, `HgsClient::setPresence`, `HgsClient::presenceFinished` (Task 5); Linux `makePresenceSource()` (Task 6).
- Produces: macOS `makePresenceSource()`; `TrayAgent::m_presence` running for the agent's lifetime.

- [ ] **Step 1: Write the failing macOS test**

Create `tray/tests/test_macpresence.cpp`:

```cpp
#include <QtTest>
#include <cmath>
#include <optional>
#include "DesktopPresence.h"

class TestMacPresence : public QObject {
    Q_OBJECT
private slots:
    void samplesSynchronouslyWithoutPermissionPrompts() {
        auto source = makePresenceSource(); QVERIFY(source);
        std::optional<PresenceSample> result;
        connect(source.get(), &PresenceSource::sampled, this, [&result](const PresenceSample &sample) { result = sample; });
        source->sample();
        QVERIFY(result.has_value());
        if (result->available) QVERIFY(std::isfinite(result->idleSeconds) && result->idleSeconds >= 0);
    }
};

QTEST_GUILESS_MAIN(TestMacPresence)
#include "test_macpresence.moc"
```

In the `if(APPLE)` block of `tray/tests/CMakeLists.txt` add:

```cmake
    add_executable(test_macpresence test_macpresence.cpp ../src/MacPresenceSource.cpp ../src/DesktopPresence.cpp)
    target_include_directories(test_macpresence PRIVATE ../src)
    target_link_libraries(test_macpresence PRIVATE Qt6::Test Qt6::Core "-framework CoreGraphics" "-framework CoreFoundation")
    add_test(NAME macpresence COMMAND test_macpresence)
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cmake -S tray -B .ci-build/gui -DBUILD_TESTING=ON -DCMAKE_PREFIX_PATH=$(brew --prefix qt) && cmake --build .ci-build/gui --target test_macpresence`
Expected: FAIL ("MacPresenceSource.cpp: No such file").

- [ ] **Step 3: Implement the source**

Create `tray/src/MacPresenceSource.cpp`:

```cpp
#include "DesktopPresence.h"

#include <CoreGraphics/CoreGraphics.h>
#include <cmath>

namespace {
bool flag(CFDictionaryRef dictionary, CFStringRef key, bool fallback)
{
    const void *value = CFDictionaryGetValue(dictionary, key);
    if (!value || CFGetTypeID(value) != CFBooleanGetTypeID()) return fallback;
    return CFBooleanGetValue(static_cast<CFBooleanRef>(value));
}

// Neither call needs Accessibility or Input Monitoring permission.
class MacPresenceSource : public PresenceSource {
public:
    void sample() override
    {
        PresenceSample result;
        const double idle = CGEventSourceSecondsSinceLastEventType(kCGEventSourceStateCombinedSessionState, kCGAnyInputEventType);
        if (std::isfinite(idle) && idle >= 0) { result.available = true; result.idleSeconds = idle; }
        if (CFDictionaryRef session = CGSessionCopyCurrentDictionary()) {
            // A locked screen, or a session switched away by fast user switching.
            result.locked = flag(session, CFSTR("CGSSessionScreenIsLocked"), false)
                || !flag(session, kCGSessionOnConsoleKey, true);
            CFRelease(session);
        }
        emit sampled(result);
    }
};
}

std::unique_ptr<PresenceSource> makePresenceSource()
{
    return std::make_unique<MacPresenceSource>();
}
```

In `tray/CMakeLists.txt` Apple block: add `target_sources(hgs-tray PRIVATE src/MacPresenceSource.cpp)` and `"-framework CoreGraphics"` to `target_link_libraries(hgs-tray PRIVATE "-framework AppKit" "-framework UserNotifications")`.

- [ ] **Step 4: Run it**

Run: `cmake --build .ci-build/gui --target test_macpresence && ctest --test-dir .ci-build/gui -R macpresence --output-on-failure`
Expected: PASS.

- [ ] **Step 5: Start presence in the tray agent**

In `tray/src/TrayAgent.h` add `#include "DesktopPresence.h"` and, as the last members (destroyed before `m_client`):

```cpp
    // Records desktop activity for phone alerts; see DesktopPresence.h.
    std::unique_ptr<DesktopPresence> m_presence;
    QString m_presenceError;
```

In `tray/src/TrayAgent.cpp`, after `m_client.requestLocal();` that follows `m_localTimer.start();`:

```cpp
    // Phone alerts wait while the owner uses any Zerus desktop. This runs with
    // the window closed; failures are logged once per distinct message.
    m_presence = std::make_unique<DesktopPresence>(makePresenceSource(),
        [this](double idleSeconds, bool locked) { m_client.setPresence(idleSeconds, locked); });
    connect(&m_client, &HgsClient::presenceFinished, this, [this](const QString &error) {
        if (error == m_presenceError) return;
        m_presenceError = error;
        if (!error.isEmpty()) qWarning().noquote() << "hgs zerus: desktop presence:" << error;
    });
    m_presence->start();
```

An older `hgs` rejects `presence set --json` with "unknown option --json"; it never launches anything, and the message is logged once.

- [ ] **Step 6: Build and run the desktop suite on the Mac**

Run: `bash scripts/ci/gui.sh`
Expected: PASS.

- [ ] **Step 7: Document the desktop behavior**

In `docs/reference.md` "Desktop workspace", add:

```markdown
While Zerus runs, even with its window closed, it records keyboard/mouse idle time
and screen lock for phone notifications. macOS uses CoreGraphics without a
permission prompt. Linux uses the desktop's screen-saver and idle interfaces (KDE
Plasma, GNOME and other compositors with Wayland idle notifications). When none
is available only input in Zerus windows counts, which can only cause extra phone
notifications.
```

- [ ] **Step 8: Commit**

```bash
git add tray/src/MacPresenceSource.cpp tray/src/TrayAgent.h tray/src/TrayAgent.cpp tray/CMakeLists.txt tray/tests/test_macpresence.cpp tray/tests/CMakeLists.txt docs/reference.md
git commit -m "Record desktop presence from the tray agent"
```

---

### Task 8: Desktop Settings choice

**Files:**
- Create: `tray/src/MobileDeliverySettings.h`
- Modify: `tray/src/SettingsPage.h:7, 94, 112, 141`
- Modify: `tray/src/SessionsWindow.cpp:1016`
- Create: `tray/tests/test_mobiledeliverysettings.cpp`
- Modify: `tray/tests/CMakeLists.txt` (append), `tray/tests/test_sessionswindow.cpp` (one function)

**Interfaces:**
- Consumes: Task 2 `hgs swarm preference get|set --json` and `hgs swarm get` → `preferences`; Task 3 `peers[alias].catalog_outdated`; `HgsClient::requestSwarm`, `swarmReady`, `swarmFailed`; `SwarmController::snapshotChanged(const QJsonObject &)`.
- Produces: `class MobileDeliverySettings : public QWidget` with `MobileDeliverySettings(const QString &executable, QWidget *parent = nullptr)`, `static QList<QPair<QString, QString>> options()`, `void refresh()`, `void setSwarmSnapshot(const QJsonObject &snapshot)`; object names `mobileDelivery` (QComboBox), `mobileDeliveryStatus`, `mobileDeliveryOutdated`; `SettingsPage::setSwarmSnapshot(const QJsonObject &)`.

- [ ] **Step 1: Write the failing widget test**

Create `tray/tests/test_mobiledeliverysettings.cpp`:

```cpp
#include <QtTest>
#include <QComboBox>
#include <QFile>
#include <QJsonObject>
#include <QLabel>
#include <QTemporaryDir>
#include "MobileDeliverySettings.h"

class TestMobileDeliverySettings : public QObject {
    Q_OBJECT
    QTemporaryDir m_dir;
    QString m_hgs;
    void touch(const QString &name, const QByteArray &text = {}) {
        QFile file(m_dir.filePath(name)); QVERIFY(file.open(QIODevice::WriteOnly)); file.write(text);
    }
    QStringList calls() const {
        QFile file(m_dir.filePath("calls")); if (!file.open(QIODevice::ReadOnly)) return {};
        return QString::fromUtf8(file.readAll()).split('\n', Qt::SkipEmptyParts);
    }
private slots:
    void init() {
        QVERIFY(m_dir.isValid());
        for (const auto *name : {"calls", "value", "old", "fail-set"}) QFile::remove(m_dir.filePath(name));
        m_hgs = m_dir.filePath("hgs");
        QFile script(m_hgs); QVERIFY(script.open(QIODevice::WriteOnly));
        script.write(R"PY(#!/usr/bin/env python3
import json, pathlib, sys
root = pathlib.Path(__file__).parent
args = sys.argv[1:]
with (root / 'calls').open('a') as file: file.write(' '.join(args) + '\n')
value = root / 'value'
if (root / 'old').exists() or args[:2] != ['swarm', 'preference']:
    print('usage: hgs swarm get | initialize | apply', file=sys.stderr); sys.exit(1)
if args[2:] == ['get', '--json']:
    print(json.dumps({'mobile_delivery': value.read_text() if value.exists() else 'away'}))
elif args[2:4] == ['set', 'mobile_delivery'] and args[5:] == ['--json']:
    if (root / 'fail-set').exists(): print('catalog is busy', file=sys.stderr); sys.exit(1)
    value.write_text(args[4]); print(json.dumps({'ok': True, 'mobile_delivery': args[4]}))
else:
    sys.exit(2)
)PY");
        script.close(); QVERIFY(script.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner));
    }
    void loadsTheSharedValueAndOffersSixChoices() {
        touch("value", "away_15");
        MobileDeliverySettings settings(m_hgs);
        auto *choice = settings.findChild<QComboBox *>("mobileDelivery"); QVERIFY(choice);
        QVERIFY(!choice->isEnabled()); QVERIFY(calls().isEmpty()); // nothing runs until Settings opens
        const QStringList values{"immediate", "away", "away_5", "away_10", "away_15", "away_30"};
        const QStringList labels{"Immediately, even if I'm at a computer", "As soon as I'm away", "After 5 minutes away",
                                 "After 10 minutes away", "After 15 minutes away", "After 30 minutes away"};
        QCOMPARE(choice->count(), 6);
        for (int i = 0; i < 6; ++i) { QCOMPARE(choice->itemData(i).toString(), values[i]); QCOMPARE(choice->itemText(i), labels[i]); }
        settings.refresh();
        QTRY_VERIFY(choice->isEnabled());
        QCOMPARE(choice->currentData().toString(), QString("away_15"));
        QCOMPARE(calls(), QStringList{"swarm preference get --json"});
    }
    void choosingSavesThroughHgsAndKeepsTheConfirmedValue() {
        MobileDeliverySettings settings(m_hgs); auto *choice = settings.findChild<QComboBox *>("mobileDelivery");
        settings.refresh(); QTRY_VERIFY(choice->isEnabled());
        choice->setCurrentIndex(3); choice->activated(3);
        QVERIFY(!choice->isEnabled());
        QTRY_VERIFY(choice->isEnabled());
        QCOMPARE(choice->currentData().toString(), QString("away_10"));
        QVERIFY(calls().contains("swarm preference set mobile_delivery away_10 --json"));
        QVERIFY(settings.findChild<QLabel *>("mobileDeliveryStatus")->text().startsWith("Saved"));
    }
    void failedSaveShowsTheErrorAndRestoresTheStoredValue() {
        MobileDeliverySettings settings(m_hgs); auto *choice = settings.findChild<QComboBox *>("mobileDelivery");
        settings.refresh(); QTRY_VERIFY(choice->isEnabled());
        touch("fail-set");
        choice->setCurrentIndex(0); choice->activated(0);
        auto *status = settings.findChild<QLabel *>("mobileDeliveryStatus");
        QTRY_VERIFY(status->text().contains("catalog is busy"));
        QTRY_COMPARE(choice->currentData().toString(), QString("away"));
        QTRY_VERIFY(choice->isEnabled());
        QVERIFY(status->text().contains("catalog is busy"));
    }
    void olderCliKeepsTheChoiceDisabled() {
        touch("old");
        MobileDeliverySettings settings(m_hgs); settings.refresh();
        QTRY_VERIFY(settings.findChild<QLabel *>("mobileDeliveryStatus")->text().contains("usage: hgs swarm get"));
        QVERIFY(!settings.findChild<QComboBox *>("mobileDelivery")->isEnabled());
    }
    void swarmSnapshotUpdatesTheValueAndNamesOutdatedMembers() {
        MobileDeliverySettings settings(m_hgs);
        auto *choice = settings.findChild<QComboBox *>("mobileDelivery");
        auto *outdated = settings.findChild<QLabel *>("mobileDeliveryOutdated");
        settings.setSwarmSnapshot({{"preferences", QJsonObject{{"mobile_delivery", "away_30"}}},
            {"peers", QJsonObject{{"mac", QJsonObject{{"node", "n1"}, {"catalog_outdated", true}}}, {"build", QJsonObject{{"node", "n2"}}}}}});
        QCOMPARE(choice->currentData().toString(), QString("away_30")); QVERIFY(choice->isEnabled());
        QVERIFY(!outdated->isHidden()); QVERIFY(outdated->text().contains("mac")); QVERIFY(!outdated->text().contains("build"));
        settings.setSwarmSnapshot({{"preferences", QJsonObject{{"mobile_delivery", "unknown"}}}, {"peers", QJsonObject{}}});
        QCOMPARE(choice->currentData().toString(), QString("away_30")); QVERIFY(outdated->isHidden());
    }
};

QTEST_MAIN(TestMobileDeliverySettings)
#include "test_mobiledeliverysettings.moc"
```

Append to `tray/tests/CMakeLists.txt`:

```cmake
add_executable(test_mobiledeliverysettings test_mobiledeliverysettings.cpp ../src/HgsClient.cpp ../src/ProcessRunner.cpp)
target_include_directories(test_mobiledeliverysettings PRIVATE ../src)
target_link_libraries(test_mobiledeliverysettings PRIVATE Qt6::Test Qt6::Widgets)
add_test(NAME mobiledeliverysettings COMMAND test_mobiledeliverysettings)
set_tests_properties(mobiledeliverysettings PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cmake -S tray -B .ci-build/gui -DBUILD_TESTING=ON $( [ "$(uname -s)" = Darwin ] && echo -DCMAKE_PREFIX_PATH=$(brew --prefix qt) ) && cmake --build .ci-build/gui --target test_mobiledeliverysettings`
Expected: FAIL ("MobileDeliverySettings.h: No such file").

- [ ] **Step 3: Implement the widget**

Create `tray/src/MobileDeliverySettings.h`:

```cpp
#pragma once
#include "HgsClient.h"
#include <QComboBox>
#include <QJsonObject>
#include <QLabel>
#include <QPair>
#include <QSignalBlocker>
#include <QVBoxLayout>

// When the phone notifies. hgs stores the choice in the shared swarm catalog;
// every Zerus desktop and the phone edit the same value.
class MobileDeliverySettings : public QWidget {
public:
    static QList<QPair<QString, QString>> options() {
        return {{"immediate", tr("Immediately, even if I'm at a computer")}, {"away", tr("As soon as I'm away")},
                {"away_5", tr("After 5 minutes away")}, {"away_10", tr("After 10 minutes away")},
                {"away_15", tr("After 15 minutes away")}, {"away_30", tr("After 30 minutes away")}};
    }
    explicit MobileDeliverySettings(const QString &executable, QWidget *parent = nullptr) : QWidget(parent), client(executable, this) {
        setObjectName("mobileDeliverySettings");
        auto *layout = new QVBoxLayout(this); layout->setContentsMargins(0, 0, 12, 0); layout->setSpacing(10);
        auto *heading = new QLabel(tr("Phone notifications")); heading->setObjectName("heading"); layout->addWidget(heading);
        auto *question = new QLabel(tr("When I'm not at a computer, send notifications to my phone")); question->setWordWrap(true); layout->addWidget(question);
        choice = new QComboBox; choice->setObjectName("mobileDelivery"); choice->setAccessibleName(question->text());
        for (const auto &option : options()) choice->addItem(option.second, option.first);
        choice->setEnabled(false); question->setBuddy(choice); layout->addWidget(choice, 0, Qt::AlignLeft);
        status = new QLabel; status->setObjectName("mobileDeliveryStatus"); status->setWordWrap(true); layout->addWidget(status);
        outdated = new QLabel; outdated->setObjectName("mobileDeliveryOutdated"); outdated->setWordWrap(true); outdated->hide(); layout->addWidget(outdated);
        auto *hint = new QLabel(tr("You count as away after two minutes without keyboard or mouse input on every computer running Zerus, or while their screens are locked. Your computers and the phone share this choice."));
        hint->setWordWrap(true); layout->addWidget(hint);
        connect(choice, &QComboBox::activated, this, [this](int index) { save(choice->itemData(index).toString()); });
        connect(&client, &HgsClient::swarmReady, this, [this](quint64 id, const QJsonObject &result) { received(id, result, {}); });
        connect(&client, &HgsClient::swarmFailed, this, [this](quint64 id, const QString &error) {
            received(id, {}, error.isEmpty() ? tr("hgs did not respond") : error);
        });
    }
    // Settings calls this when it opens; nothing runs while it stays closed.
    void refresh() { if (!setRequest) getRequest = client.requestSwarm({"preference", "get", "--json"}); }
    // The 5-second swarm read keeps the value current and names members whose
    // hgs is too old to receive it.
    void setSwarmSnapshot(const QJsonObject &snapshot) {
        const auto value = snapshot.value("preferences").toObject().value("mobile_delivery").toString();
        if (!setRequest && choice->findData(value) >= 0 && value != current) { current = value; show(value); }
        QStringList names;
        const auto peers = snapshot.value("peers").toObject();
        for (auto it = peers.begin(); it != peers.end(); ++it)
            if (it.value().toObject().value("catalog_outdated").toBool()) names << it.key();
        outdated->setVisible(!names.isEmpty());
        outdated->setText(tr("Update Zerus on %1. Until then it keeps syncing projects but does not receive this choice.").arg(names.join(", ")));
    }
private:
    void save(const QString &value) {
        if (setRequest || value == current) { show(current); return; }
        choice->setEnabled(false); status->setText(tr("Saving…"));
        setRequest = client.requestSwarm({"preference", "set", "mobile_delivery", value, "--json"});
    }
    void received(quint64 id, const QJsonObject &result, const QString &error) {
        const auto value = result.value("mobile_delivery").toString();
        if (id && id == setRequest) {
            setRequest = 0;
            if (error.isEmpty() && result.value("ok").toBool() && choice->findData(value) >= 0) {
                current = value; show(value);
                status->setText(tr("Saved. Your other computers and the phone receive it with the next sync."));
            } else {
                status->setText(tr("Could not save: %1").arg(error.isEmpty() ? tr("unexpected response from hgs") : error));
                show(current); refresh();
            }
            return;
        }
        if (!id || id != getRequest) return;
        getRequest = 0;
        if (!error.isEmpty() || choice->findData(value) < 0) {
            loadFailed = true; choice->setEnabled(false);
            status->setText(tr("Could not read the shared setting: %1").arg(error.isEmpty() ? tr("unexpected response from hgs") : error));
            return;
        }
        if (std::exchange(loadFailed, false)) status->clear();
        current = value; show(value);
    }
    void show(const QString &value) {
        const QSignalBlocker block(choice);
        const int index = choice->findData(value);
        if (index >= 0) choice->setCurrentIndex(index);
        choice->setEnabled(!setRequest && index >= 0);
    }
    HgsClient client;
    QComboBox *choice; QLabel *status, *outdated;
    QString current;
    quint64 getRequest = 0, setRequest = 0;
    bool loadFailed = false;
};
```

- [ ] **Step 4: Run the widget test**

Run: `cmake --build .ci-build/gui --target test_mobiledeliverysettings && ctest --test-dir .ci-build/gui -R mobiledeliverysettings --output-on-failure`
Expected: PASS (5 functions).

- [ ] **Step 5: Place it in Settings and forward swarm snapshots**

In `tray/src/SettingsPage.h` add `#include "MobileDeliverySettings.h"` after `#include "RelaySettings.h"`, replace

```cpp
        auto *mobile=new RelaySettings::Panel;mobile->setMaximumWidth(850);addPage(mobile);
```

with

```cpp
        // This computer's connector configuration, then the timing every computer and the phone share.
        auto *mobile=new QWidget;mobile->setMaximumWidth(850);auto *mobileLayout=new QVBoxLayout(mobile);mobileLayout->setContentsMargins(0,0,0,0);mobileLayout->setSpacing(28);
        mobileLayout->addWidget(new RelaySettings::Panel);mobileDelivery=new MobileDeliverySettings(executable);mobileLayout->addWidget(mobileDelivery);mobileLayout->addStretch();addPage(mobile);
```

change `void refresh(){sync->refresh();if(nav->currentRow()==6)refreshCliVersion();}` to

```cpp
    void refresh(){sync->refresh();mobileDelivery->refresh();if(nav->currentRow()==6)refreshCliVersion();}
    void setSwarmSnapshot(const QJsonObject &snapshot){mobileDelivery->setSwarmSnapshot(snapshot);}
```

and append `MobileDeliverySettings *mobileDelivery;` to the private member line.

In `tray/src/SessionsWindow.cpp`, after `m_settingsPage=new SettingsPage(hgsPath);m_pages->addWidget(m_settingsPage);` add:

```cpp
    connect(m_swarm,&SwarmController::snapshotChanged,this,[this](const QJsonObject &snapshot){m_settingsPage->setSwarmSnapshot(snapshot);});
```

Add to `tray/tests/test_sessionswindow.cpp` (declare in the slot list, define with the others):

```cpp
void TestSessionsWindow::mobileConnectionSettingsOfferTheSharedDeliveryChoice(){
    SessionsWindow window(script()); window.show();
    window.findChild<QPushButton *>("workspaceSettings")->click();
    auto *choice=window.findChild<QComboBox *>("mobileDelivery"); QVERIFY(choice); QCOMPARE(choice->count(),6);
    QVERIFY(window.findChild<QWidget *>("relaySettings"));
}
```

- [ ] **Step 6: Run the desktop suite**

Run: `bash scripts/ci/gui.sh` (Mac), then the same on `arch` in Task 11.
Expected: PASS.

- [ ] **Step 7: Document the setting**

In `docs/reference.md` "Desktop workspace", add: "Settings → Mobile connection → Phone notifications sets when the phone notifies: immediately, as soon as you are away from every Zerus computer, or after 5, 10, 15 or 30 minutes away. The choice is shared through the swarm catalog; computers whose hgs is too old to receive it are named there."

- [ ] **Step 8: Commit**

```bash
git add tray/src/MobileDeliverySettings.h tray/src/SettingsPage.h tray/src/SessionsWindow.cpp tray/tests/test_mobiledeliverysettings.cpp tray/tests/test_sessionswindow.cpp tray/tests/CMakeLists.txt docs/reference.md
git commit -m "Choose the shared phone delivery timing in desktop Settings"
```

---

### Task 9: Android shared setting model and request

Run Android checks on `arch`.

**Files:**
- Create: `mobile/android/app/src/main/java/app/zerus/mobile/MobileDelivery.kt`
- Modify: `mobile/android/app/src/main/java/app/zerus/mobile/Models.kt:14-17` (`Machine`)
- Modify: `mobile/android/app/src/main/java/app/zerus/mobile/MachineCatalog.kt:21-25` (`parse`)
- Modify: `mobile/android/app/src/main/java/app/zerus/mobile/SessionAction.kt` (`SessionActionPolicies.result`)
- Modify: `mobile/android/app/src/main/java/app/zerus/mobile/ZerusViewModel.kt:721` and after `launchSession`
- Create: `mobile/android/app/src/test/java/app/zerus/mobile/MobileDeliveryTest.kt`

**Interfaces:**
- Consumes: Task 4 relay operation, capabilities and snapshot field `preferences.mobile_delivery`; existing `SessionAction`, `settleSessionAction`, `durable`, `api.submit`, `api.await`.
- Produces: `Machine.mobileDelivery: String` (empty when unreported); `object MobileDelivery` with `OPERATION`, `UNCONFIRMED`, `values`, `label(value)`, `parse(snapshot: JSONObject?)`, `arguments(value)`, `requested(action)`, `gateway(machines, supported)`, `reason(gateway, gatewaySupports, relaySupports, pending)`, `result(action, receipt)`, `shown(reported, action, now)`, `status(gateway, reported, action, reason, now)`; view-model `mobileDeliveryError`, `mobileDeliveryGateway(connectionId)`, `mobileDeliveryAction(connectionId)`, `mobileDeliveryReason(connectionId)`, `setMobileDelivery(connectionId, value)`. Task 10 uses all of them.

- [ ] **Step 1: Write the failing unit tests**

Create `mobile/android/app/src/test/java/app/zerus/mobile/MobileDeliveryTest.kt`:

```kotlin
package app.zerus.mobile

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class MobileDeliveryTest {
    private val gatewayId = "11111111-1111-4111-8111-111111111111"
    private val peerId = "33333333-3333-4333-8333-333333333333"
    private fun machine(id: String, route: MachineRoute = MachineRoute.Direct, online: Boolean = true, value: String = "away") =
        Machine("workspace", id, "Desktop", online, route = route, mobileDelivery = value)
    private fun action(value: String, status: String = "sending", createdAt: Long = 1_000) =
        SessionAction("request", Target(gatewayId, "", "", "", "workspace"), MobileDelivery.OPERATION,
            JSONObject().put("value", value).toString(), status = status, createdAt = createdAt)
    private fun receipt(result: JSONObject?, state: String = "completed") =
        JSONObject().put("request_id", "request").put("state", state).put("result", result ?: JSONObject.NULL)

    @Test fun snapshotValuesAreExactAndUnknownValuesStayUnreported() {
        MobileDelivery.values.forEach { assertEquals(it, MobileDelivery.parse(JSONObject().put("preferences", JSONObject().put("mobile_delivery", it)))) }
        listOf(JSONObject(), JSONObject().put("preferences", "away"), JSONObject().put("preferences", JSONObject().put("mobile_delivery", "later")),
            JSONObject().put("preferences", JSONObject().put("mobile_delivery", 5))).forEach { assertEquals("", MobileDelivery.parse(it)) }
        assertEquals("", MobileDelivery.parse(null))
        val raw = JSONObject().put("id", gatewayId).put("name", "Desktop").put("online", true)
            .put("snapshot", JSONObject().put("preferences", JSONObject().put("mobile_delivery", "away_10")))
        assertEquals("away_10", MachineCatalog.parse("workspace", raw).mobileDelivery)
        assertEquals("As soon as I'm away", MobileDelivery.label("away"))
        assertEquals("Immediately, even if I'm at a computer", MobileDelivery.label("immediate"))
    }
    @Test fun argumentsContainOnlyAKnownValue() {
        assertEquals("{\"value\":\"away_5\"}", MobileDelivery.arguments("away_5").toString())
        assertThrows(IllegalArgumentException::class.java) { MobileDelivery.arguments("later") }
    }
    @Test fun onlyTheAdvertisingGatewayCarriesTheChoice() {
        val gateway = machine(gatewayId, value = "away_15")
        val peer = machine(peerId, MachineRoute.Via(gatewayId), value = "immediate")
        assertEquals(gatewayId, MobileDelivery.gateway(listOf(peer, gateway)) { it.id == gatewayId }?.id)
        assertEquals(gatewayId, MobileDelivery.gateway(listOf(machine(peerId, online = false), gateway)) { true }?.id)
        assertEquals(gatewayId, MobileDelivery.gateway(listOf(peer, gateway)) { false }?.id) // read-only fallback
        assertNull(MobileDelivery.gateway(listOf(peer)) { false })
    }
    @Test fun olderGatewaysOrRelaysExplainWhyTheChoiceIsReadOnly() {
        val gateway = machine(gatewayId)
        assertEquals("No gateway computer reports this setting yet.", MobileDelivery.reason(null, false, true, false))
        assertEquals("Update Zerus on Desktop to change this from the phone.", MobileDelivery.reason(gateway, false, true, false))
        assertEquals("Update the relay to change this from the phone.", MobileDelivery.reason(gateway, true, false, false))
        assertEquals("Desktop is offline. Try again when it is connected.", MobileDelivery.reason(machine(gatewayId, online = false), true, true, false))
        assertEquals("Check the previous change before choosing again.", MobileDelivery.reason(gateway, true, true, true))
        assertEquals("", MobileDelivery.reason(gateway, true, true, false))
    }
    @Test fun onlyAnExactNativeAcknowledgementConfirmsTheChange() {
        val pending = action("away_10")
        assertEquals("completed", SessionActionPolicies.result(pending, receipt(JSONObject().put("ok", true).put("mobile_delivery", "away_10"))).status)
        listOf(JSONObject().put("ok", true).put("mobile_delivery", "away"), JSONObject().put("mobile_delivery", "away_10"),
            JSONObject().put("ok", "true").put("mobile_delivery", "away_10"), null).forEach {
            assertEquals("uncertain", SessionActionPolicies.result(pending, receipt(it)).status)
        }
        assertEquals("uncertain", SessionActionPolicies.result(pending, receipt(null, "uncertain")).status)
        assertEquals("failed", SessionActionPolicies.result(pending, receipt(null, "failed").put("error", "gateway offline")).status)
        assertEquals("uncertain", SessionActionPolicies.result(pending, receipt(null).put("request_id", "other")).status)
    }
    @Test fun pendingAndJustConfirmedChoicesAreShownUntilTheSnapshotCatchesUp() {
        assertEquals("away_10", MobileDelivery.shown("away", action("away_10"), 2_000))
        assertEquals("away_10", MobileDelivery.shown("away", action("away_10", "uncertain"), 2_000))
        assertEquals("away_10", MobileDelivery.shown("away", action("away_10", "completed"), 30_000))
        assertEquals("away", MobileDelivery.shown("away", action("away_10", "completed"), 120_000))
        assertEquals("away", MobileDelivery.shown("away", action("away_10", "failed"), 2_000))
        assertEquals("immediate", MobileDelivery.shown("immediate", null, 2_000))
    }
    @Test fun statusExplainsSubmittedConfirmedAndBlockedStates() {
        assertEquals("Saving on Desktop…", MobileDelivery.status("Desktop", "away", action("away_10"), "", 2_000))
        assertEquals(MobileDelivery.UNCONFIRMED, MobileDelivery.status("Desktop", "away", action("away_10", "uncertain"), "", 2_000))
        assertEquals("Shared by your computers through Desktop.", MobileDelivery.status("Desktop", "away_10", action("away_10", "completed"), "", 2_000))
        assertEquals("Saved on Desktop. The computers report it with their next update.", MobileDelivery.status("Desktop", "away", action("away_10", "completed"), "", 2_000))
        assertEquals("Desktop is offline. Try again when it is connected.", MobileDelivery.status("Desktop", "away", null, "Desktop is offline. Try again when it is connected.", 2_000))
        assertEquals("Not reported by Desktop yet.", MobileDelivery.status("Desktop", "", null, "", 2_000))
    }
}
```

- [ ] **Step 2: Run them to verify they fail (on `arch`)**

Run: `cd mobile/android && JAVA_HOME=$HOME/.cache/zerus-toolchains/jdk-17.0.20.1+1 ANDROID_HOME=$HOME/Android/Sdk ./gradlew testDebugUnitTest --tests app.zerus.mobile.MobileDeliveryTest`
Expected: FAIL to compile ("Unresolved reference: MobileDelivery", "mobileDelivery").

- [ ] **Step 3: Implement the model**

In `Models.kt`, extend `Machine`:

```kotlin
data class Machine(val connectionId: String, val id: String, val name: String, val online: Boolean,
    val nativeName: String = name, val serverAliases: Set<String> = emptySet(),
    val route: MachineRoute = MachineRoute.Unknown, val lastKnown: Boolean = false,
    val hgsVersion: String = "", val mobileDelivery: String = "")
```

In `MachineCatalog.kt`, replace `parse` with:

```kotlin
    fun parse(connectionId: String, raw: JSONObject) = Machine(
        connectionId, raw.getString("id"), raw.string("name", "id"), raw.optBoolean("online"),
        serverAliases = aliases(raw.opt("aliases")), route = route(raw),
        hgsVersion = (raw.optJSONObject("snapshot")?.opt("hgs_version") as? String)
            ?.takeIf { it.length <= 128 && versionFormat.matches(it) }.orEmpty(),
        mobileDelivery = MobileDelivery.parse(raw.optJSONObject("snapshot")))
```

Create `MobileDelivery.kt`:

```kotlin
package app.zerus.mobile

import org.json.JSONObject

/** The swarm-wide phone delivery choice. The gateway computer's hgs stores it; the phone requests changes. */
object MobileDelivery {
    const val OPERATION = "set_mobile_delivery"
    const val UNCONFIRMED = "The change was not confirmed. Check its original receipt in Drafts; it has not been repeated."
    val values = listOf("immediate", "away", "away_5", "away_10", "away_15", "away_30")
    private const val CONFIRMATION_MS = 60_000L

    fun label(value: String) = when (value) {
        "immediate" -> "Immediately, even if I'm at a computer"
        "away" -> "As soon as I'm away"
        "away_5" -> "After 5 minutes away"
        "away_10" -> "After 10 minutes away"
        "away_15" -> "After 15 minutes away"
        "away_30" -> "After 30 minutes away"
        else -> ""
    }
    /** Unknown or missing values stay unreported instead of pretending to be the default. */
    fun parse(snapshot: JSONObject?): String =
        (snapshot?.optJSONObject("preferences")?.opt("mobile_delivery") as? String)?.takeIf { it in values }.orEmpty()
    fun arguments(value: String): JSONObject {
        require(value in values) { "Choose a supported notification timing." }
        return JSONObject().put("value", value)
    }
    fun requested(action: SessionAction) = runCatching { JSONObject(action.arguments).string("value") }.getOrDefault("")
    /** Only gateway connectors advertise the operation; an older one still shows its value read-only. */
    fun gateway(machines: List<Machine>, supported: (Machine) -> Boolean): Machine? =
        machines.filter(supported).sortedWith(compareBy<Machine>({ !it.online || it.lastKnown }, { it.route != MachineRoute.Direct }, { it.id })).firstOrNull()
            ?: machines.filter { it.route == MachineRoute.Direct && it.mobileDelivery.isNotBlank() }.minByOrNull { it.id }
    fun reason(gateway: Machine?, gatewaySupports: Boolean, relaySupports: Boolean, pending: Boolean): String = when {
        gateway == null -> "No gateway computer reports this setting yet."
        !gatewaySupports -> "Update Zerus on ${gateway.name} to change this from the phone."
        !relaySupports -> "Update the relay to change this from the phone."
        !gateway.online || gateway.lastKnown -> "${gateway.name} is offline. Try again when it is connected."
        pending -> "Check the previous change before choosing again."
        else -> ""
    }
    /** Called after the generic identity and failure checks of SessionActionPolicies.result. */
    fun result(action: SessionAction, receipt: JSONObject): SessionAction {
        val native = receipt.optJSONObject("result")
        if (receipt.string("state") != "completed" || native == null || native.opt("ok") != true ||
            native.opt("mobile_delivery") != requested(action)) return action.copy(status = "uncertain", error = UNCONFIRMED)
        return action.copy(status = "completed", error = "")
    }
    fun shown(reported: String, action: SessionAction?, now: Long): String {
        val requested = action?.let(::requested).orEmpty()
        return when {
            action == null || requested.isBlank() -> reported
            action.blocksSending -> requested
            action.status == "completed" && now - action.createdAt in 0 until CONFIRMATION_MS -> requested
            else -> reported
        }
    }
    fun status(gateway: String, reported: String, action: SessionAction?, reason: String, now: Long): String {
        val recent = action != null && now - action.createdAt in 0 until CONFIRMATION_MS
        return when {
            action?.status == "sending" -> "Saving on $gateway…"
            action?.status == "uncertain" -> UNCONFIRMED
            action?.status == "failed" && recent -> action.error.ifBlank { "Not changed. Try again." }
            reason.isNotBlank() -> reason
            reported.isBlank() -> "Not reported by $gateway yet."
            action?.status == "completed" && recent && requested(action) != reported -> "Saved on $gateway. The computers report it with their next update."
            else -> "Shared by your computers through $gateway."
        }
    }
}
```

In `SessionActionPolicies.result` (`SessionAction.kt`), after the line returning `failed` for `receipt.string("state") == "failed"` add:

```kotlin
        if(action.operation == MobileDelivery.OPERATION) return MobileDelivery.result(action,receipt)
```

- [ ] **Step 4: Run the unit tests**

Run: `cd mobile/android && JAVA_HOME=$HOME/.cache/zerus-toolchains/jdk-17.0.20.1+1 ANDROID_HOME=$HOME/Android/Sdk ./gradlew testDebugUnitTest --tests app.zerus.mobile.MobileDeliveryTest --tests app.zerus.mobile.MachineCatalogTest --tests app.zerus.mobile.SessionActionTest`
Expected: PASS.

- [ ] **Step 5: Send the request from the view model**

In `ZerusViewModel.kt`, after `var launchError by mutableStateOf("");private set` add `var mobileDeliveryError by mutableStateOf("");private set`. After `launchSession`, add:

```kotlin
    fun mobileDeliveryGateway(connectionId:String)=MobileDelivery.gateway(machines.filter { it.connectionId==connectionId }) {
        MobileDelivery.OPERATION in machineOperations[MachineKey(it.connectionId,it.id)].orEmpty()
    }
    fun mobileDeliveryAction(connectionId:String)=sessionActions.lastOrNull { it.operation==MobileDelivery.OPERATION && it.target.connectionId==connectionId && it.status!="reviewed" }
    fun mobileDeliveryReason(connectionId:String):String {
        if(demo || !storageReady) return "Connect a workspace to change this setting."
        if(updateInstallPreparing) return "Saving drafts for an app update."
        val gateway=mobileDeliveryGateway(connectionId)
        return MobileDelivery.reason(gateway,
            gateway!=null && MobileDelivery.OPERATION in machineOperations[MachineKey(connectionId,gateway.id)].orEmpty(),
            MobileDelivery.OPERATION in relayOperations[connectionId].orEmpty(),
            sessionActions.any { it.operation==MobileDelivery.OPERATION && it.target.connectionId==connectionId && it.blocksSending })
    }
    /** A durable request with the usual receipt states. An unconfirmed change is checked, never repeated. */
    fun setMobileDelivery(connectionId:String,value:String) {
        val reason=mobileDeliveryReason(connectionId);if(reason.isNotBlank()) { mobileDeliveryError=reason;return }
        val gateway=mobileDeliveryGateway(connectionId) ?: return
        if(gateway.mobileDelivery==value) { mobileDeliveryError="";return }
        val args=try { MobileDelivery.arguments(value) } catch(e:Exception) { mobileDeliveryError=e.message.orEmpty();return }
        val connection=connections.find { it.id==connectionId } ?: return
        val target=Target(gateway.id,"","","",connectionId)
        val action=SessionAction(UUID.randomUUID().toString(),target,MobileDelivery.OPERATION,args.toString())
        mobileDeliveryError="";actionFlights+=action.requestId
        viewModelScope.launch {
            var attempted=false;var rejected=false
            try {
                durable({ state -> check(state.sessionActions.none { it.operation==MobileDelivery.OPERATION && it.target.connectionId==connectionId && it.blocksSending });state.action(action) },
                    { current,written -> current.action(written.sessionActions.first { it.requestId==action.requestId }) })
                attempted=true
                val initial=try { api.submit(connection,target,MobileDelivery.OPERATION,JSONObject(args.toString()),action.requestId) }
                    catch(e:RelayException) { rejected=e.status in listOf(400,401,403,404,409,413,429);throw e }
                settleSessionAction(action,api.await(connection,initial))
                val settled=messageState.sessionActions.find { it.requestId==action.requestId }
                if(settled?.status=="completed") refresh() else mobileDeliveryError=settled?.error.orEmpty()
            } catch(e:Exception) {
                val status=if(!attempted || rejected) "failed" else "uncertain"
                try { durable({ state -> state.sessionActions.find { it.requestId==action.requestId }?.let { if(it.status in setOf("sending","uncertain")) state.action(it.copy(status=status,error=if(status=="failed") e.message.orEmpty() else MobileDelivery.UNCONFIRMED)) else state } ?: state }) }
                catch(_:Exception) { mobileDeliveryError="Could not save the change result. Its original request remains recoverable." }
                if(e is CancellationException) throw e
                mobileDeliveryError=if(status=="failed") e.message.orEmpty() else MobileDelivery.UNCONFIRMED
            } finally { actionFlights-=action.requestId }
        }
    }
```

- [ ] **Step 6: Run the Android checks (on `arch`)**

Run: `cd mobile/android && JAVA_HOME=$HOME/.cache/zerus-toolchains/jdk-17.0.20.1+1 ANDROID_HOME=$HOME/Android/Sdk ./gradlew testDebugUnitTest lintDebug`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add mobile/android/app/src/main/java/app/zerus/mobile/MobileDelivery.kt mobile/android/app/src/main/java/app/zerus/mobile/Models.kt mobile/android/app/src/main/java/app/zerus/mobile/MachineCatalog.kt mobile/android/app/src/main/java/app/zerus/mobile/SessionAction.kt mobile/android/app/src/main/java/app/zerus/mobile/ZerusViewModel.kt mobile/android/app/src/test/java/app/zerus/mobile/MobileDeliveryTest.kt
git commit -m "Request the shared phone delivery timing from Android"
```

---

### Task 10: Android Settings → Notifications choice

The app's notification settings live in the Machines screen's "Notifications" section; the choice goes there.

**Files:**
- Modify: `mobile/android/app/src/main/java/app/zerus/mobile/NotificationSettings.kt` (imports, new composable)
- Modify: `mobile/android/app/src/main/java/app/zerus/mobile/MachinesUi.kt:53`
- Modify: `mobile/android/app/src/main/java/app/zerus/mobile/SessionDetailsUi.kt:228, 343-347`
- Modify: `docs/mobile-architecture.md` (Notifications section)

**Interfaces:**
- Consumes: Task 9 `MobileDelivery` and view-model members.
- Produces: `@Composable internal fun MobileDeliverySettings(model: ZerusViewModel)`.

- [ ] **Step 1: Add the composable**

In `NotificationSettings.kt` add imports `androidx.compose.foundation.selection.selectable`, `androidx.compose.foundation.selection.selectableGroup`, `androidx.compose.ui.semantics.Role`, then append:

```kotlin
/** One shared choice per workspace; the gateway computer's hgs stores it for every swarm desktop. */
@Composable internal fun MobileDeliverySettings(model: ZerusViewModel) {
    if (model.connections.isEmpty()) return
    Text("When I'm not at a computer, send notifications to my phone", Modifier.padding(start = 16.dp))
    model.connections.forEach { connection ->
        val gateway = model.mobileDeliveryGateway(connection.id)
        val action = model.mobileDeliveryAction(connection.id)
        val reason = model.mobileDeliveryReason(connection.id)
        val now = System.currentTimeMillis()
        val selected = MobileDelivery.shown(gateway?.mobileDelivery.orEmpty(), action, now)
        Column(Modifier.fillMaxWidth().padding(start = 16.dp).selectableGroup(), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            if (model.connections.size > 1) Text(connection.displayName, style = MaterialTheme.typography.labelLarge)
            MobileDelivery.values.forEach { value ->
                Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).selectable(selected = value == selected, enabled = reason.isBlank(),
                    role = Role.RadioButton, onClick = { model.setMobileDelivery(connection.id, value) }),
                    verticalAlignment = Alignment.CenterVertically) {
                    RadioButton(selected = value == selected, onClick = null, enabled = reason.isBlank())
                    Spacer(Modifier.width(8.dp))
                    Text(MobileDelivery.label(value))
                }
            }
            Text(MobileDelivery.status(gateway?.name.orEmpty(), gateway?.mobileDelivery.orEmpty(), action, reason, now),
                style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
    if (model.mobileDeliveryError.isNotBlank()) Text(model.mobileDeliveryError, style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.error)
}
```

In `MachinesUi.kt`, after `NotificationTypeSettings(model.notifications)` add `MobileDeliverySettings(model)`.

In `SessionDetailsUi.kt`, add `"set_mobile_delivery" -> "Phone notification timing";` to `actionLabel` before `else ->`, and change the card title to `Text(if(action.needsProjectReview) action.resultTarget?.session.orEmpty() else action.target.session.ifBlank { actionLabel(action.operation) },fontWeight = FontWeight.SemiBold)`.

- [ ] **Step 2: Run the Android checks, both flavors (on `arch`)**

Run: `cd mobile/android && export JAVA_HOME=$HOME/.cache/zerus-toolchains/jdk-17.0.20.1+1 ANDROID_HOME=$HOME/Android/Sdk && ./gradlew testDebugUnitTest lintDebug assembleDebug && ./gradlew -PzerusFirebase=true testDebugUnitTest lintDebug`
Expected: PASS.

- [ ] **Step 3: Check it on an emulator with a fixture relay**

Follow `docs/mobile-deployment.md` with a local relay and the connector pointed at `services/mobile/tests/fixture_hgs.py` (never a live agent). Verify: Machines → Notifications shows six choices with "As soon as I'm away" selected; choosing "After 10 minutes away" shows "Saving on …", then the selection stays and the status becomes "Shared by your computers through …" after the next heartbeat; with the connector stopped the choices are disabled and the reason names the offline gateway. Record the result or that the check was unavailable.

- [ ] **Step 4: Document the phone setting**

In `docs/mobile-architecture.md` "Notifications", add:

```markdown
Machines → Notifications offers "When I'm not at a computer, send notifications to
my phone" for each paired workspace, with the same six choices as desktop
Settings. It shows the gateway computer's reported value. A change is a durable
`set_mobile_delivery` request to that gateway: it shows as saving, then as shared
once the gateway confirms it; an unconfirmed change appears in Drafts for a
receipt check and is never repeated. Older gateways or relays show the value
read-only with the reason.
```

- [ ] **Step 5: Commit**

```bash
git add mobile/android/app/src/main/java/app/zerus/mobile/NotificationSettings.kt mobile/android/app/src/main/java/app/zerus/mobile/MachinesUi.kt mobile/android/app/src/main/java/app/zerus/mobile/SessionDetailsUi.kt docs/mobile-architecture.md
git commit -m "Show the shared phone delivery timing in Android notifications"
```

---

### Task 11: Cross-platform validation and handoff

**Files:**
- No source changes unless a check fails (fix inside the owning task's files).

- [ ] **Step 1: Run the local checks on the Mac**

Run: `python3 scripts/ci/check-source.py && python3 -m unittest discover -s tests -p 'test_*publication.py' && bash scripts/ci/cli.sh && bash scripts/ci/gui.sh`
Expected: PASS. `cli.sh` requires tmux 3.7+; record it if unavailable.

- [ ] **Step 2: Copy the branch to `arch` and run Linux checks**

Run: `rsync -a --delete --exclude target --exclude .ci-build --exclude artifacts --exclude 'mobile/android/**/build' --exclude services/mobile/relay/target ./ arch:zerus-phase2-validation/ && ssh -o BatchMode=yes arch 'cd zerus-phase2-validation && bash scripts/ci/cli.sh && bash scripts/ci/gui.sh'`
Expected: PASS, including `linuxpresence` under `dbus-run-session`. This never touches the running Zerus, tmux servers or agents on `arch`.

- [ ] **Step 3: Run the relay and service suites on `arch`**

Run (on `arch`, in `zerus-phase2-validation`, inside a Python 3.11 virtual environment with `services/mobile` installed): `cd services/mobile/relay && cargo +1.85.0 fmt --check && cargo +1.85.0 clippy --locked --all-targets -- -D warnings && cargo +1.85.0 test --locked && cd ../../.. && python -m unittest discover -s services/mobile/tests -v && ZERUS_RELAY_BINARY=$PWD/services/mobile/relay/target/debug/zerus-relay python -m unittest discover -s services/mobile/tests -p test_rust_relay.py -v`
Expected: PASS.

- [ ] **Step 4: Run Android both ways on `arch`**

Run: `cd mobile/android && export JAVA_HOME=$HOME/.cache/zerus-toolchains/jdk-17.0.20.1+1 ANDROID_HOME=$HOME/Android/Sdk && ./gradlew testDebugUnitTest lintDebug && ./gradlew -PzerusFirebase=true testDebugUnitTest lintDebug`
Expected: PASS.

- [ ] **Step 5: Owner-driven live check (only with the owner's go-ahead)**

After the owner installs the new `hgs` and Zerus on a machine (GUI update only; never restart agents or tmux), run `hgs ls --json --local | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("desktop_presence"), d.get("preferences"))'` while using the desktop, again after two idle minutes, and while the screen is locked. On KDE Plasma Wayland expect idle to stay near 0 while typing, rise after 10 s without input, and `locked: true` while locked.

- [ ] **Step 6: Record results and remove the copy**

Report every check run, its result, and every unavailable check. Remove `arch:zerus-phase2-validation` when done. Update Beads (`zerus-rk5b`) and export with `bd export -o .beads/issues.jsonl` only when the owner asks for tracker changes.
