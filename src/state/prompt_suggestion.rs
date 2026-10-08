//! Right after a turn ends, Claude proposes the person's likely next message and
//! shows it faintly in its prompt. An internal agent writes it, and Claude reports
//! that agent only through SubagentStop: no SubagentStart, no agent type and no
//! Agent call. It is no subagent of the conversation, so it is kept as the
//! session's suggestion instead of a subagent or an Activity event.
use super::*;

/// Claude writes the suggestion within seconds of the turn's end. Later internal
/// reports while idle are recaps, which are neither short nor meant for input.
const WINDOW: f64 = 30.0;
const LIMIT: usize = 300;

/// Returns true when the event is consumed as the current suggestion.
pub(super) fn observe(record: &mut Value, event: &Value) -> bool {
    if string(record, "agent") != "claude" {
        return false;
    }
    let kind = string(event, "hook_event_name");
    let main = string(event, "agent_id").is_empty();
    if main && ["UserPromptSubmit", "SessionStart"].contains(&kind) {
        // A new request or conversation makes the suggestion stale.
        let fields = record.as_object_mut().unwrap();
        fields.remove("prompt_suggestion");
        fields.remove("turn_ended_at");
        return false;
    }
    if main && kind == "Stop" {
        record["turn_ended_at"] = json!(now());
        record.as_object_mut().unwrap().remove("prompt_suggestion");
        return false;
    }
    if kind != "SubagentStop" || main || !candidate(record, event) {
        return false;
    }
    let text = string(event, "last_assistant_message").trim().to_owned();
    record["prompt_suggestion"] = json!({"text": text, "at": now(), "agent_id": string(event, "agent_id")});
    true
}

fn candidate(record: &Value, event: &Value) -> bool {
    let text = string(event, "last_assistant_message").trim();
    let started = record["subagents"].get(string(event, "agent_id")).is_some();
    let ended = record["turn_ended_at"].as_f64().unwrap_or(0.0);
    string(event, "agent_type").is_empty()
        && string(event, "agent_name").is_empty()
        && !started
        && string(record, "activity") == "idle"
        && string(record, "phase") == "idle"
        && ended > 0.0
        && now() - ended <= WINDOW
        && !text.is_empty()
        && !text.contains('\n')
        && text.chars().count() <= LIMIT
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle() -> Value {
        json!({"agent": "claude", "activity": "idle", "phase": "idle", "turn_ended_at": now() - 3.0, "subagents": {}})
    }
    fn stop(text: &str) -> Value {
        json!({"hook_event_name": "SubagentStop", "agent_id": "a1b2", "last_assistant_message": text})
    }

    #[test]
    fn internal_report_after_turn_becomes_the_suggestion() {
        let mut record = idle();
        assert!(observe(&mut record, &stop("  commit and push  ")));
        assert_eq!(record["prompt_suggestion"]["text"], "commit and push");
        assert_eq!(record["prompt_suggestion"]["agent_id"], "a1b2");
    }

    #[test]
    fn real_subagents_and_other_internal_reports_stay_subagents() {
        let typed = json!({"hook_event_name": "SubagentStop", "agent_id": "a1b2", "agent_type": "general-purpose", "last_assistant_message": "done"});
        let mut started = idle();
        started["subagents"] = json!({"a1b2": {"name": "Explore", "state": "working"}});
        let mut working = idle();
        working["activity"] = json!("busy");
        working["phase"] = json!("working");
        let mut late = idle();
        late["turn_ended_at"] = json!(now() - WINDOW - 5.0);
        let mut never = idle();
        never.as_object_mut().unwrap().remove("turn_ended_at");
        let mut kimi = idle();
        kimi["agent"] = json!("kimi");
        for (mut record, event) in [
            (idle(), typed),
            (started, stop("done")),
            (working, stop("Checking the tracker")),
            (late, stop("Goal: finish the release")),
            (never, stop("next")),
            (kimi, stop("next")),
            (idle(), stop("<analysis>\nThe conversation")),
            (idle(), stop("")),
            (idle(), stop(&"x".repeat(LIMIT + 1))),
        ] {
            assert!(!observe(&mut record, &event));
            assert!(record.get("prompt_suggestion").is_none());
        }
    }

    #[test]
    fn a_new_request_turn_or_conversation_clears_the_suggestion() {
        for kind in ["UserPromptSubmit", "SessionStart", "Stop"] {
            let mut record = idle();
            assert!(observe(&mut record, &stop("run the tests")));
            assert!(!observe(&mut record, &json!({"hook_event_name": kind})));
            assert!(record.get("prompt_suggestion").is_none(), "{kind}");
        }
        // Subagent events of the next turn do not touch the main turn's state.
        let mut record = idle();
        assert!(observe(&mut record, &stop("run the tests")));
        assert!(!observe(&mut record, &json!({"hook_event_name": "Stop", "agent_id": "a9"})));
        assert_eq!(record["prompt_suggestion"]["text"], "run the tests");
    }
}
