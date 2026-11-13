//! zerus-63rru.88 live tests: a child's end is final for its ID. A
//! non-start event of an ended child (a delayed PreToolUse or PostToolUse
//! applied after its SubagentStop) is recorded as history only and never
//! sets the child working again; Claude and Codex name every subagent
//! invocation with a fresh ID, so no event of theirs revives an ended
//! child; a Kimi profile child really resumes under its profile name, so
//! only its own start event revives it. Each record starts as the
//! supervisor and its main SessionStart hook build it, the child events go
//! through the production writer (`telemetry::observe` then
//! `journal::update_activity`, the order the hook entry uses), and each
//! row is made by the listing's own steps.
use super::*;
use crate::state::listing_fixture::{confirmed, launched_record, listed_live_row};
use serde_json::{json, Value};

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
        telemetry::observe(record, event);
        journal::update_activity(record, event);
    }
}

fn child(record: &Value, id: &str) -> Value {
    record["subagents"][id].clone()
}

/// Claude Code names a child by `agent_id` and its `agent_type`.
fn claude(kind: &str, id: &str) -> Value {
    json!({"hook_event_name":kind,"agent_id":id,"agent_type":"general-purpose",
        "last_assistant_message":"the child's result","error_message":"the child failed"})
}

/// A provider that names its children only by agent ID.
fn codex(kind: &str, id: &str) -> Value {
    json!({"hook_event_name":kind,"agent_id":id,"error_message":"the child failed"})
}

/// Kimi's own hooks name a profile, never a child's identity.
fn kimi(kind: &str, profile: &str) -> Value {
    json!({"hook_event_name":kind,"agent_id":"main","agent_name":profile})
}

/// The reproduction's live form (zerus-63rru.88): the Stop applied before
/// a delayed PreToolUse or PostToolUse of the same child, for the Claude
/// frame (id plus type) and the Codex frame (id only). The child stays
/// finished, the event is recorded as history, and the listed row reads no
/// active child.
#[test]
fn a_delayed_child_tool_event_after_its_stop_is_recorded_as_history_only() {
    for (agent, builder, kinds) in [
        (
            "claude",
            claude as fn(&str, &str) -> Value,
            &["PreToolUse", "PostToolUse"][..],
        ),
        (
            "codex",
            codex as fn(&str, &str) -> Value,
            &["PreToolUse", "PostToolUse"][..],
        ),
    ] {
        let mut record = record(agent);
        let id = if agent == "claude" { "agent-a1" } else { "d1" };
        apply(
            &mut record,
            &[builder("SubagentStart", id), builder("SubagentStop", id)],
        );
        assert_eq!(child(&record, id)["state"], "finished", "{agent}");
        for kind in kinds {
            apply(&mut record, &[builder(kind, id)]);
            assert_eq!(
                child(&record, id)["state"],
                "finished",
                "{agent} {kind}: the delayed event set the child working again"
            );
            assert_eq!(
                child(&record, id)["tracking_evidence"]["kind"].as_str(),
                Some(*kind),
                "{agent} {kind}: the event is not recorded"
            );
            let row = listed_live_row(record.clone()).expect("a listed row");
            assert_ne!(
                row["subagent_active_count"].as_u64(),
                Some(1),
                "{agent} {kind}: the member reads one active child after a delayed event"
            );
        }
    }
}

/// A Claude child is never revived: its late Start is dropped (the fresh
/// agent_id rule), and a tool event after the end is history only.
#[test]
fn a_claude_child_is_never_revived() {
    let mut record = record("claude");
    apply(
        &mut record,
        &[
            claude("SubagentStart", "agent-a1"),
            claude("SubagentStop", "agent-a1"),
            claude("SubagentStart", "agent-a1"),
            claude("PreToolUse", "agent-a1"),
        ],
    );
    assert_eq!(child(&record, "agent-a1")["state"], "finished");
}

/// A Kimi profile child really resumes under its profile name: a delayed
/// tool event is history only, and the profile's own start event revives
/// it.
#[test]
fn a_kimi_profile_child_resumes_with_its_own_start() {
    let mut record = record("kimi");
    apply(
        &mut record,
        &[
            kimi("SubagentStart", "profile-1"),
            kimi("SubagentStop", "profile-1"),
            kimi("PreToolUse", "profile-1"),
        ],
    );
    assert_eq!(
        child(&record, "profile-1")["state"],
        "finished",
        "a delayed tool event revived the profile child"
    );
    apply(&mut record, &[kimi("SubagentStart", "profile-1")]);
    assert_eq!(
        child(&record, "profile-1")["state"],
        "working",
        "the profile's own start did not revive it"
    );
}
