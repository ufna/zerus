//! Fleet rows for the actor's tests, made by the product (zerus-63rru.51).
//!
//! `fixtures/fleet-rows.json` holds one idle live row per agent kind as the
//! listing writes it: Claude, Codex and Kimi through `watch_source::rows`'
//! own steps, DeepSeek through `native_rows`' own steps from a host's `list`
//! answer. Its paths are normalized to `/fixture`. Every seeded fleet row
//! starts from one of these and changes named keys only; a key the producer's
//! row does not have cannot be added. `the_fixture_rows_are_the_producers`
//! makes the rows again in a child test process with a private state root and
//! fails when the fixture and the producer have different keys.
use serde_json::{json, Value};

pub(in crate::desktop::teams) const KINDS: [&str; 4] = ["claude", "codex", "kimi", "dsh"];

const CHILD: &str = "desktop::teams::actor::tests::rows::";

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/fleet-rows.json")).unwrap()
}

/// The producer's idle live row of `kind`.
pub(in crate::desktop::teams) fn producer_row(kind: &str) -> Value {
    let row = fixture()["rows"][kind].clone();
    assert!(row.is_object(), "no producer row for {kind}");
    row
}

/// The producer's row of `kind` with each `(path, value)` set. A path is
/// dotted (`process_work.count`) and must already exist in the producer's
/// row: a seed cannot carry a key the product never writes.
pub(in crate::desktop::teams) fn seed_row(kind: &str, changes: &[(&str, Value)]) -> Value {
    let mut row = producer_row(kind);
    for (path, value) in changes {
        let mut slot = &mut row;
        for part in path.split('.') {
            assert!(
                slot.get(part).is_some(),
                "a seeded {kind} row would carry `{path}`, which the producer's row does not have"
            );
            slot = &mut slot[part];
        }
        *slot = value.clone();
    }
    row
}

/// Every key path of `value`, dotted, with the objects' own paths.
pub(in crate::desktop::teams) fn paths(value: &Value) -> std::collections::BTreeSet<String> {
    fn walk(value: &Value, prefix: &str, out: &mut std::collections::BTreeSet<String>) {
        if let Some(map) = value.as_object() {
            for (key, child) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                out.insert(path.clone());
                // Maps keyed by an identity (children, read marks) are data.
                if !matches!(key.as_str(), "subagents" | "action_reasons") {
                    walk(child, &path, out);
                }
            }
        }
    }
    let mut out = std::collections::BTreeSet::new();
    walk(value, "", &mut out);
    out
}

/// The keys a DeepSeek host's `list` answer and its rows may carry here,
/// pinned in one place (zerus-63rru.61). A list row carries the keys the
/// contract tests of `src/state/dsh.rs` build list rows with (its
/// `child_summaries` test and its `"list"` stubs), and `projections`, which
/// the bridge's list handler passes through from the host and reads itself
/// (`src/dsh/bridge.mjs`, case 'list') and which alone names a session's
/// children (`projections.values.subagentCatalog`, read by `dsh.rs`). It
/// never carries `jobs` or `shellCalls`: only `inspect` answers with them.
///
/// Follow-up zerus-yj7ef (pull request 2161): the bridge's own list answer,
/// captured and guarded as `tests/fixtures/dsh-bridge-list-row.json`, whose
/// rows carry `listJobs`. Once it reaches this branch through a main sync,
/// [`deepseek_list`] returns that file's rows and these pins compare with it.
pub(in crate::desktop::teams) const DEEPSEEK_LIST_KEYS: [&str; 2] = ["generation", "items"];
pub(in crate::desktop::teams) const DEEPSEEK_ROW_KEYS: [&str; 6] = [
    "sessionId",
    "parentSessionId",
    "origin",
    "running",
    "agentAvailable",
    "projections",
];
pub(in crate::desktop::teams) const DEEPSEEK_CATALOG_KEYS: [&str; 3] = ["id", "label", "mode"];

/// The DeepSeek host's `list` answer for one idle, available session: the
/// seam the captured bridge answer replaces (see [`DEEPSEEK_ROW_KEYS`]).
pub(in crate::desktop::teams) fn deepseek_list(conversation: &str) -> Value {
    json!({"generation":"generation-1","items":[{"sessionId":conversation,"agentAvailable":true,
        "running":false}]})
}

/// Every key of a list answer is a pinned one.
pub(in crate::desktop::teams) fn assert_pinned_list(list: &Value) {
    let keys = |v: &Value| -> Vec<String> {
        v.as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    };
    for key in keys(list) {
        assert!(DEEPSEEK_LIST_KEYS.contains(&key.as_str()), "list key {key}");
    }
    for row in list["items"].as_array().unwrap() {
        for key in keys(row) {
            assert!(
                DEEPSEEK_ROW_KEYS.contains(&key.as_str()),
                "list row key {key}"
            );
        }
        if let Some(values) = row.get("projections") {
            assert_eq!(keys(values), ["values"], "projections");
            for key in keys(&values["values"]) {
                assert_eq!(key, "subagentCatalog", "projections.values key");
            }
            for child in values["values"]["subagentCatalog"]
                .as_array()
                .into_iter()
                .flatten()
            {
                for key in keys(child) {
                    assert!(
                        DEEPSEEK_CATALOG_KEYS.contains(&key.as_str()),
                        "catalog key {key}"
                    );
                }
            }
        }
    }
}

/// One idle live session of `kind`, as its saved binding records it.
pub(in crate::desktop::teams) fn record(kind: &str, root: &str) -> Value {
    let name = format!("{kind}/example/member");
    if kind == "dsh" {
        return json!({"backend":"dsh","name":name,"agent":"dsh","agent_home":format!("{root}/.dsh"),
            "cwd":"/","conversation_id":"session-00000000-0000-4000-8000-0000000000d5",
            "run_id":"run-dsh","created":1793000000.0,"launch_id":"00000000-0000-4000-8000-0000000000d4",
            "paused":false,"session_id":"00000000-0000-4000-8000-0000000000d6"});
    }
    json!({"version":1,"run_identity_version":1,"name":name,"agent":kind,
        "run_id":format!("run-{kind}"),"conversation_id":format!("conversation-{kind}"),
        "session_id":"00000000-0000-4000-8000-000000000051","launch_id":"00000000-0000-4000-8000-000000000050",
        "cwd":"/","launch_dir":"/","created":1793000000,
        "activity":"idle","phase":"idle","conversation_state":"active","last_event_at":1793000001.0})
}

/// Makes the four rows in this process, whose state root is private.
fn produce() -> Value {
    let root = std::env::var("HOME").unwrap();
    let mut rows = serde_json::Map::new();
    for kind in KINDS {
        let record = record(kind, &root);
        let row = if kind == "dsh" {
            let list = deepseek_list(record["conversation_id"].as_str().unwrap());
            crate::state::listing_fixture::listed_deepseek_row(&record, &list)
        } else {
            crate::state::listing_fixture::listed_live_row(record).unwrap()
        };
        rows.insert(kind.into(), row);
    }
    // Paths of this process's private root are not the producer's choice.
    let text = Value::Object(rows).to_string().replace(&root, "/fixture");
    json!({"about":"Idle live fleet rows made by the listing's own steps; regenerate with the ignored test regenerate_fleet_rows.","rows":serde_json::from_str::<Value>(&text).unwrap()})
}

#[test]
#[ignore = "run by its isolated wrapper"]
fn producer_rows_child() {
    println!("FLEET-ROWS {}", produce());
}

/// The rows the producer makes now, from a child with a private root.
pub(in crate::desktop::teams) fn produced() -> Value {
    let text = super::real_rows::isolated(&format!("{CHILD}producer_rows_child"));
    let line = text
        .lines()
        .find_map(|line| line.split_once("FLEET-ROWS ").map(|(_, rows)| rows))
        .expect("producer rows");
    serde_json::from_str(line).unwrap()
}

#[test]
fn the_fixture_rows_are_the_producers() {
    let fresh = produced();
    for kind in KINDS {
        let made = paths(&fresh["rows"][kind]);
        let kept = paths(&producer_row(kind));
        let extra: Vec<_> = kept.difference(&made).collect();
        let missing: Vec<_> = made.difference(&kept).collect();
        assert!(
            extra.is_empty() && missing.is_empty(),
            "{kind}: the fixture carries {extra:?} that the producer does not write and lacks {missing:?}; regenerate it"
        );
    }
}

#[test]
#[ignore = "writes src/desktop/teams/actor/tests/fixtures/fleet-rows.json"]
fn regenerate_fleet_rows() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/desktop/teams/actor/tests/fixtures/fleet-rows.json");
    std::fs::write(
        path,
        serde_json::to_string_pretty(&produced()).unwrap() + "\n",
    )
    .unwrap();
}

#[test]
fn a_seed_cannot_carry_a_key_the_producer_does_not_write() {
    for (kind, path) in [
        ("claude", "subagents_active"),
        ("codex", "state"),
        ("kimi", "process_work.total"),
    ] {
        let refused = std::panic::catch_unwind(|| seed_row(kind, &[(path, json!(0))]));
        assert!(refused.is_err(), "{kind} {path}");
    }
    assert_eq!(
        seed_row("dsh", &[("state", json!("paused"))])["state"],
        "paused"
    );
}

/// Every key the actor's reader reads exists in a producer's row; one that
/// must be present for a row to count is present in every kind's row.
#[test]
fn every_key_the_reader_reads_is_written_by_a_producer() {
    let rows: Vec<_> = KINDS
        .iter()
        .map(|kind| (kind, paths(&producer_row(kind))))
        .collect();
    for (path, required) in super::super::fleet::READ {
        let having: Vec<_> = rows
            .iter()
            .filter(|(_, p)| p.contains(path))
            .map(|(k, _)| **k)
            .collect();
        if required {
            assert_eq!(
                having.len(),
                KINDS.len(),
                "`{path}` is read as required but only {having:?} write it"
            );
        } else {
            assert!(
                !having.is_empty(),
                "`{path}` is read but no producer row has it"
            );
        }
    }
}

/// The reader's key list names every key it reads: removing any key of a
/// producer's row that changes what the reader answers must be listed.
#[test]
fn the_readers_key_list_names_every_key_it_reads() {
    use super::super::fleet::{Facts, Identity, READ};
    fn remove(row: &mut Value, path: &str) {
        let mut parts: Vec<&str> = path.split('.').collect();
        let last = parts.pop().unwrap();
        let mut slot = row;
        for part in parts {
            slot = &mut slot[part];
        }
        if let Some(map) = slot.as_object_mut() {
            map.remove(last);
        }
    }
    let answer = |row: &Value| {
        let mut facts = Facts::of(row);
        // The shared presentation's status reads its own keys (client core).
        facts.ready = true;
        let named = Identity::of(row);
        format!("{facts:?} {named:?}")
    };
    let listed = |path: &str| {
        READ.iter()
            .any(|(read, _)| *read == path || read.starts_with(&format!("{path}.")))
    };
    assert!(!READ.is_empty());
    for kind in KINDS {
        let row = producer_row(kind);
        let before = answer(&row);
        for path in paths(&row) {
            let mut changed = row.clone();
            remove(&mut changed, &path);
            if answer(&changed) != before {
                assert!(
                    listed(&path),
                    "{kind}: the reader reads `{path}`, which its key list does not name"
                );
            }
        }
    }
}

/// The fixture's DeepSeek list answer carries only pinned keys: none the
/// bridge's list handler does not pass (zerus-63rru.61).
#[test]
fn the_deepseek_list_answer_carries_only_the_pinned_keys() {
    assert_pinned_list(&deepseek_list(
        "session-00000000-0000-4000-8000-0000000000d5",
    ));
}

/// zerus-63rru.93 round 3: the fleet's own free read over the real listed
/// row — a quiet revived Kimi profile child leaves no active child and no
/// unsettled count, so the member reads free.
#[test]
fn a_quiet_revived_kimi_child_reads_free_in_the_fleet() {
    let mut record = record("kimi", "/fixture");
    record["subagents"] = json!({"profile-1": {
        "name":"profile-1","started":1793000000.0,"state":"working",
        "revived_at":1793000060.0,"updated":1792999940.0}});
    record["subagent_groups"] = json!({"profile-1": {
        "label":"profile-1","active_count":1,"completed_count":1,"total_count":2,
        "updated":1792999940.0}});
    record["subagent_groups_complete"] = json!(true);
    let row = crate::state::listing_fixture::listed_live_row(record).unwrap();
    assert_eq!(row["subagent_active_count"], 0, "{row}");
    let facts = super::super::fleet::Facts::of(&row);
    assert!(facts.free(), "{facts:?}");
}
