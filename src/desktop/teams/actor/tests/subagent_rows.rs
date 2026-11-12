//! What a fleet row's subagent count means, by its producer (zerus-63rru.61).
//! Each row here is made by the listing's own steps from a session record
//! with the children its producers record, in a child test process with a
//! private state root. A count whose incompleteness is history counts; one
//! about children that may be running now is unknown.
use super::super::fleet::{Facts, Incomplete, SOURCES};
use super::super::ports::Fact;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const CHILD: &str = "desktop::teams::actor::tests::subagent_rows::";

/// A record of `kind` with `extra` set, as its producers leave it.
fn with(kind: &str, root: &str, extra: Value) -> Value {
    let mut record = super::rows::record(kind, root);
    for (key, value) in extra.as_object().unwrap() {
        if value.is_null() {
            record.as_object_mut().unwrap().remove(key);
        } else {
            record[key] = value.clone();
        }
    }
    record
}

/// One row per value and meaning the producers write.
fn produce() -> BTreeMap<&'static str, Value> {
    use crate::state::listing_fixture::{listed_deepseek_row, listed_live_row};
    let root = std::env::var("HOME").unwrap();
    let live = |kind: &str, extra: Value| listed_live_row(with(kind, &root, extra)).unwrap();
    let child = |state: &str| json!({"name":"reviewer","state":state,"updated":1793000001.0});
    let groups =
        json!({"coder":{"label":"coder","active_count":0,"completed_count":1,"total_count":1}});
    let external = json!({"helper":{"name":"helper","state":"finished","external":true,"updated":1793000001.0}});
    let mut rows = BTreeMap::new();
    rows.insert("hooks", live("claude", json!({})));
    rows.insert(
        "hooks_ended_child",
        live("claude", json!({"subagents":{"child-1":child("ended")}})),
    );
    rows.insert(
        "hooks_interrupted_child",
        live(
            "codex",
            json!({"subagents":{"child-1":
        {"name":"reviewer","state":"working","display_state":"unknown","updated":1793000001.0}}}),
        ),
    );
    rows.insert(
        "hooks_working_child",
        live("claude", json!({"subagents":{"child-1":child("working")}})),
    );
    rows.insert(
        "kimi_native_history",
        live(
            "kimi",
            json!({"kimi_native_roster":true,"subagents":{"agent-1":
        {"name":"coder","state":"ended","native_roster":true,"updated":1793000001.0}}}),
        ),
    );
    rows.insert(
        "hook_profiles_incomplete",
        live(
            "kimi",
            json!({"subagent_groups":groups,"subagent_groups_complete":false}),
        ),
    );
    rows.insert(
        "hook_profiles_complete",
        live(
            "kimi",
            json!({"subagent_groups":groups,"subagent_groups_complete":true}),
        ),
    );
    rows.insert(
        "mixed_hooks_incomplete",
        live(
            "kimi",
            json!({"subagent_groups":groups,"subagent_groups_complete":false,"subagents":external}),
        ),
    );
    rows.insert(
        "mixed_hooks_complete",
        live(
            "kimi",
            json!({"subagent_groups":groups,"subagent_groups_complete":true,"subagents":external}),
        ),
    );
    rows.insert("unavailable", live("claude", json!({"last_event_at":null})));
    let dsh = |settled: bool| {
        let record = with("dsh", &root, json!({}));
        let conversation = record["conversation_id"].as_str().unwrap().to_owned();
        let mut list = super::rows::deepseek_list(&conversation);
        list["items"][0]["projections"] = json!({"values":{"subagentCatalog":[{"id":"child-1","label":"helper","mode":"subagent"}]}});
        if settled {
            list["items"].as_array_mut().unwrap().push(json!({"sessionId":"child-1",
                "parentSessionId":conversation,"origin":"subagent","running":false,"agentAvailable":true}));
        }
        super::rows::assert_pinned_list(&list);
        listed_deepseek_row(&record, &list)
    };
    rows.insert("dsh_settled_child", dsh(true));
    rows.insert("dsh_unsettled_child", dsh(false));
    rows
}

#[test]
#[ignore = "run by its isolated wrapper"]
fn subagent_rows_child() {
    println!("SUBAGENT-ROWS {}", json!(produce()));
}

fn produced() -> BTreeMap<String, Value> {
    let text = super::real_rows::isolated(&format!("{CHILD}subagent_rows_child"));
    let line = text
        .lines()
        .find_map(|line| line.split_once("SUBAGENT-ROWS ").map(|(_, rows)| rows))
        .expect("subagent rows");
    serde_json::from_str(line).unwrap()
}

/// Each producer-made row: its source, its flag, and what the reader takes.
#[test]
fn a_subagent_count_is_settled_only_when_every_child_that_may_run_now_is_counted() {
    use Fact::{Background, Children, ChildrenUnsettled};
    let rows = produced();
    // (row, source, flag, count, unsettled, free, the fact an otherwise free member names)
    #[allow(clippy::type_complexity)]
    let expected: [(&str, &str, bool, Option<u64>, bool, bool, Option<Fact>); 12] = [
        ("hooks", "hooks", true, Some(0), false, true, None),
        // History never keeps a member: a child that ended without a
        // completion, or was interrupted, is not current work.
        (
            "hooks_ended_child",
            "hooks",
            false,
            Some(0),
            false,
            true,
            None,
        ),
        (
            "hooks_interrupted_child",
            "hooks",
            false,
            Some(0),
            false,
            true,
            None,
        ),
        (
            "hooks_working_child",
            "hooks",
            true,
            Some(1),
            false,
            false,
            None,
        ),
        (
            "kimi_native_history",
            "kimi_native",
            false,
            Some(0),
            false,
            true,
            None,
        ),
        // Children that may be running now and are not counted: unsettled.
        (
            "hook_profiles_incomplete",
            "hook_profiles",
            false,
            Some(0),
            true,
            false,
            Some(ChildrenUnsettled),
        ),
        (
            "hook_profiles_complete",
            "hook_profiles",
            true,
            Some(0),
            false,
            true,
            None,
        ),
        (
            "mixed_hooks_incomplete",
            "mixed_hooks",
            false,
            Some(0),
            true,
            false,
            Some(ChildrenUnsettled),
        ),
        (
            "mixed_hooks_complete",
            "mixed_hooks",
            true,
            Some(0),
            false,
            true,
            None,
        ),
        (
            "unavailable",
            "unavailable",
            false,
            None,
            false,
            false,
            Some(Children),
        ),
        // A DeepSeek list row's background is unknown today (no jobs in a
        // list answer), named first; its children are read on their own.
        (
            "dsh_settled_child",
            "dsh_native",
            true,
            Some(0),
            false,
            false,
            Some(Background),
        ),
        (
            "dsh_unsettled_child",
            "dsh_native",
            false,
            Some(0),
            true,
            false,
            Some(Background),
        ),
    ];
    assert_eq!(rows.len(), expected.len());
    for (name, source, complete, count, unsettled, free, fact) in expected {
        let row = &rows[name];
        assert_eq!(row["subagent_source"], source, "{name}");
        assert_eq!(row["subagent_counts_complete"], complete, "{name}");
        let facts = Facts::of(row);
        assert_eq!(facts.children, count, "{name}");
        assert_eq!(facts.children_unsettled, unsettled, "{name}");
        assert_eq!(facts.free(), free, "{name}");
        assert_eq!(facts.unknown(), fact, "{name}");
        if unsettled || count.is_none() {
            assert_eq!(
                facts.seen(),
                None,
                "{name}: no presence while a count is unknown"
            );
        }
    }
    // Every value the reader classifies has a producer-made row here.
    let made: std::collections::BTreeSet<&str> = rows
        .values()
        .filter_map(|row| row["subagent_source"].as_str())
        .collect();
    let classified: std::collections::BTreeSet<&str> = SOURCES.iter().map(|(s, _)| *s).collect();
    assert_eq!(made, classified);
}

/// A row whose source the reader does not know is unknown, whatever its count.
#[test]
fn an_unclassified_source_is_unknown() {
    let mut row = super::rows::seed_row("claude", &[]);
    assert_eq!(Facts::of(&row).children, Some(0));
    for source in [json!("future_source"), Value::Null, json!(7)] {
        row["subagent_source"] = source.clone();
        let facts = Facts::of(&row);
        assert!(facts.children_unsettled, "{source}");
        assert!(!facts.free(), "{source}");
        assert_eq!(facts.unknown(), Some(Fact::ChildrenUnsettled), "{source}");
    }
}

/// Every `subagent_source` a producer writes anywhere under src/ is one the
/// reader classifies, and every classified one is written: a producer that
/// gains a value fails here before a member is read wrongly.
#[test]
fn every_subagent_source_a_producer_writes_is_classified() {
    fn files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    files(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut paths,
    );
    let mut written = std::collections::BTreeSet::new();
    for path in paths {
        let text = std::fs::read_to_string(&path).unwrap();
        // The key followed by its string value in a JSON literal.
        for (at, key) in text.match_indices("\"subagent_source\":\"") {
            let rest = &text[at + key.len()..];
            written.insert(rest[..rest.find('"').unwrap()].to_owned());
        }
        // An assignment to the key's index: every literal up to its end.
        for (at, key) in text.match_indices("[\"subagent_source\"] =") {
            let rest = &text[at + key.len()..];
            let statement = &rest[..rest.find(';').unwrap()];
            for literal in statement.split('"').skip(1).step_by(2) {
                written.insert(literal.to_owned());
            }
        }
    }
    let classified: std::collections::BTreeSet<String> =
        SOURCES.iter().map(|(s, _)| (*s).to_owned()).collect();
    assert_eq!(written, classified, "a producer writes a subagent_source the reader does not classify, or no longer writes one it does");
    // The reader names the meaning of each.
    assert!(SOURCES
        .iter()
        .all(|(s, m)| m.is_some() || *s == "unavailable"));
    let _ = Incomplete::History;
}
