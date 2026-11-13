//! The hook writer's own record of a child, in either event order
//! (zerus-63rru.78). Each record starts as the supervisor and its main
//! SessionStart hook build it, the child events go through the production
//! writer (`journal::update_activity`), and each row is made by the listing's
//! own steps. The test prints rows; its parent reads them with the actor's
//! facts, so nothing here hand-writes a count.
use super::*;
use crate::state::listing_fixture::{confirmed, launched_record, listed_live_row};
use serde_json::json;
use std::collections::BTreeSet;
use std::time::Duration;

const CLIENT: &str = "state::subagent_order::reordered_children_child";

fn record(agent: &str) -> Value {
    let mut record = launched_record(
        agent,
        &format!("{agent}/example/member"),
        "00000000-0000-4000-8000-0000000000a1",
        "00000000-0000-4000-8000-0000000000a2",
        &format!("run-{agent}"),
        "/",
        "account-example",
    );
    confirmed(&mut record, &format!("conversation-{agent}"));
    record
}

fn apply(record: &mut Value, events: &[Value]) {
    for event in events {
        journal::update_activity(record, event);
    }
}

fn row(record: Value) -> Value {
    listed_live_row(record).expect("a listed row")
}

/// Claude Code names a child by `agent_id` and its `agent_type`.
fn claude(kind: &str, id: &str) -> Value {
    json!({"hook_event_name":kind,"agent_id":id,"agent_type":"general-purpose",
        "last_assistant_message":"the child's result"})
}

/// A provider that names its children only by agent ID.
fn codex(kind: &str, id: &str) -> Value {
    json!({"hook_event_name":kind,"agent_id":id})
}

/// Kimi's own hooks name a profile, never a child's identity.
fn kimi(kind: &str, profile: &str) -> Value {
    json!({"hook_event_name":kind,"agent_id":"main","agent_name":profile})
}

fn produce() -> Value {
    let mut rows = serde_json::Map::new();
    // Claude: the two orders of one child's events.
    let mut start_stop = record("claude");
    apply(
        &mut start_stop,
        &[
            claude("SubagentStart", "agent-a15"),
            claude("SubagentStop", "agent-a15"),
        ],
    );
    rows.insert("claude_start_stop".into(), row(start_stop));
    let mut stop_start = record("claude");
    apply(
        &mut stop_start,
        &[
            claude("SubagentStop", "agent-a15"),
            claude("SubagentStart", "agent-a15"),
        ],
    );
    rows.insert("claude_stop_start".into(), row(stop_start));
    // The same reorder, its Start a second after its Stop.
    let mut late = record("claude");
    apply(&mut late, &[claude("SubagentStop", "agent-a15")]);
    std::thread::sleep(Duration::from_millis(1100));
    apply(&mut late, &[claude("SubagentStart", "agent-a15")]);
    rows.insert("claude_late_start".into(), row(late));
    // Codex: the same two orders, its events carry only the child's ID.
    let mut codex_start_stop = record("codex");
    apply(
        &mut codex_start_stop,
        &[codex("SubagentStart", "c1"), codex("SubagentStop", "c1")],
    );
    rows.insert("codex_start_stop".into(), row(codex_start_stop));
    let mut codex_stop_start = record("codex");
    apply(
        &mut codex_stop_start,
        &[codex("SubagentStop", "c1"), codex("SubagentStart", "c1")],
    );
    rows.insert("codex_stop_start".into(), row(codex_stop_start));
    // Kimi: profile counts, which carry no child identity.
    let mut kimi_start_stop = record("kimi");
    apply(
        &mut kimi_start_stop,
        &[
            kimi("SubagentStart", "coder"),
            kimi("SubagentStop", "coder"),
        ],
    );
    rows.insert("kimi_start_stop".into(), row(kimi_start_stop));
    let mut kimi_stop_start = record("kimi");
    apply(
        &mut kimi_stop_start,
        &[
            kimi("SubagentStop", "coder"),
            kimi("SubagentStart", "coder"),
        ],
    );
    rows.insert("kimi_stop_start".into(), row(kimi_stop_start));
    // Two children, one of them reordered after its own Stop.
    let mut both = record("claude");
    apply(
        &mut both,
        &[
            claude("SubagentStart", "a"),
            claude("SubagentStart", "b"),
            claude("SubagentStop", "a"),
            claude("SubagentStop", "b"),
            claude("SubagentStart", "b"),
        ],
    );
    rows.insert("two_children_one_reordered".into(), row(both));
    // A child pruned from a record above the cap; its late Start has no
    // ended entry to meet (the honest limit).
    let mut pruned = record("claude");
    let stops: Vec<Value> = (0..66)
        .map(|index| claude("SubagentStop", &format!("agent-p{index:02}")))
        .collect();
    apply(&mut pruned, &stops);
    let held: BTreeSet<String> = pruned["subagents"]
        .as_object()
        .map(|children| children.keys().cloned().collect())
        .unwrap_or_default();
    let missing = (0..66)
        .map(|index| format!("agent-p{index:02}"))
        .find(|id| !held.contains(id))
        .expect("one child was pruned");
    apply(&mut pruned, &[claude("SubagentStart", &missing)]);
    rows.insert("claude_pruned_start".into(), row(pruned));
    // A real new child still counts, and a Stop without a Start keeps its
    // meaning.
    let mut start_only = record("claude");
    apply(&mut start_only, &[claude("SubagentStart", "agent-a15")]);
    rows.insert("claude_start_only".into(), row(start_only));
    let mut stop_only = record("claude");
    apply(&mut stop_only, &[claude("SubagentStop", "agent-a15")]);
    rows.insert("claude_stop_only".into(), row(stop_only));
    json!({"rows": rows})
}

#[test]
#[ignore = "run by its isolated wrapper"]
fn reordered_children_child() {
    println!("SUBAGENT-ORDER {}", produce());
}
