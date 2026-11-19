//! The person turn (zerus-46l1q, S1): the native turn of one session that
//! accepted a message the person typed in the desktop window, until anyone
//! else's input reaches that session, its run or conversation changes, or
//! twelve hours pass (owner decisions, 2026-11-18).
//!
//! Only the prompt hook of a Claude, Codex or Kimi session starts one
//! ([`observe`]), and only when every one of these holds for the input that
//! hook reports:
//! - the session's pending request is the window's person mark for this exact
//!   session, run and conversation, admitted at most twelve hours ago (the
//!   turn then lasts at most twelve hours from this hook);
//! - the CLI's own receipt of that request carries exactly the stdin the window
//!   admitted (the mark's digest), and the prompt is that receipt's text;
//! - (for any use) the native agent itself proved that delivery ([`delivered`]);
//! - the hook reports it after the request was sent, through the input gate;
//! - no main tool call is running: a prompt is never reported from inside one;
//! - that request has not started a person turn before.
//!
//! Every other prompt of the main agent ends the turn, and so does every
//! receipt and every answer delivered into the session [`end`]. Nothing is copied into an environment variable, a flag, a
//! child or a team member; `caller::person_request` reads this record, the
//! mark and the receipt again for each command.
use super::*;

/// The agents whose prompt hook binds a person turn.
const AGENTS: [&str; 3] = ["claude", "codex", "kimi"];
/// The session record field that holds the live person turn.
pub(crate) const FIELD: &str = "person_turn";
/// The last request that started (or could no longer start) a person turn:
/// one message starts at most one.
const SPENT: &str = "person_turn_spent";

/// What the record held before the hook's own activity update.
pub(super) struct Before {
    tools: bool,
    seen: Value,
}

pub(super) fn before(record: &Value) -> Before {
    Before {
        tools: record["active_tools"]
            .as_object()
            .is_some_and(|tools| !tools.is_empty()),
        seen: record["input_seen_at"].clone(),
    }
}

/// Ends the person turn: any other input reached the session.
pub(crate) fn end(record: &mut Value) {
    if let Some(fields) = record.as_object_mut() {
        if let Some(turn) = fields.remove(FIELD) {
            fields.insert(SPENT.into(), turn["request_id"].clone());
        }
    }
}

fn main_event(record: &Value, event: &Value) -> bool {
    let agent = string(event, "agent_id");
    agent.is_empty() || (string(record, "agent") == "kimi" && agent == "main")
}

fn prompt_event(kind: &str) -> bool {
    matches!(
        kind,
        "UserPromptSubmit" | "UserPromptQueued" | "TurnStarted" | "TaskStarted"
    )
}

/// The text a prompt hook reports, as `agent` reports it.
fn prompt_text<'a>(agent: &str, event: &'a Value) -> Option<&'a str> {
    event["prompt"].as_str().or_else(|| {
        let parts = event["prompt"].as_array()?;
        let [part] = parts.as_slice() else {
            return None;
        };
        (agent == "kimi" && part["type"] == "text")
            .then(|| part["text"].as_str())
            .flatten()
    })
}

/// Whether this hook reports exactly the pending receipt's text.
fn same_text(agent: &str, event: &Value, receipt: &Value) -> bool {
    let submitted = string(receipt, "submitted_text");
    !submitted.is_empty()
        && prompt_text(agent, event).is_some_and(|prompt| {
            prompt == submitted || prompt.strip_suffix('\n') == Some(submitted)
        })
}

/// The event that starts a native turn for its input: Kimi's user turn
/// start, every other agent's prompt submission.
fn binding_event(agent: &str, event: &Value) -> bool {
    match agent {
        "kimi" => {
            string(event, "hook_event_name") == "TurnStarted"
                && event["origin_kind"] == "user"
                && event["turn_id"].as_u64().is_some()
        }
        _ => string(event, "hook_event_name") == "UserPromptSubmit",
    }
}

/// The marked request this record's pending input is, with its receipt,
/// when the window admitted it for exactly this session, run and
/// conversation and the receipt carries exactly the stdin it admitted:
/// `(request_id, receipt, mark)`.
pub(crate) fn marked_pending(
    config: &crate::config::Config,
    record: &Value,
) -> Option<(String, Value, crate::desktop::person_mark::Mark)> {
    if !AGENTS.contains(&string(record, "agent")) {
        return None;
    }
    let request = string(record, "input_pending_request");
    let conversation = string(record, "conversation_id");
    if request.is_empty() || conversation.is_empty() || string(record, "run_id").is_empty() {
        return None;
    }
    let receipt = input::acceptance::pending_receipt_of(record)?;
    if receipt["request_id"] != request || receipt["conversation_id"] != conversation {
        return None;
    }
    let mark = crate::desktop::person_mark::lookup(config, request)?;
    (mark.session == string(record, "name")
        && mark.run_id == string(record, "run_id")
        && mark.conversation_id == conversation
        && mark.input_sha256 == string(&receipt, "digest"))
    .then(|| (request.to_owned(), receipt, mark))
}

/// The native identity of the input a binding `event` reports, which the
/// delivery proof must show for exactly that input (zerus-46l1q, review D1):
/// Claude's `prompt_id` (its transcript's `promptId`), Codex's `turn_id` (its
/// rollout's turn), Kimi's user `turn_id`. Without one nothing binds.
fn native_identity(agent: &str, event: &Value) -> Option<Value> {
    match agent {
        "claude" => event["prompt_id"]
            .as_str()
            .filter(|id| {
                uuid::Uuid::parse_str(id)
                    .is_ok_and(|uuid| !uuid.is_nil() && uuid.to_string() == *id)
            })
            .map(Value::from),
        "codex" => input::acceptance::codex_turn(event),
        "kimi" => event["turn_id"]
            .as_u64()
            .filter(|id| *id <= i64::MAX as u64)
            .map(Value::from),
        _ => None,
    }
}

/// What one main prompt event does to the person turn.
#[derive(Debug, PartialEq)]
pub(super) enum Decision {
    /// A companion report of the same input: nothing changes.
    Keep,
    /// Another input reached the session: the turn ends, and the pending
    /// input can no longer start one.
    End,
    /// The native turn that accepted the person's marked message starts.
    Bind(Value),
}

/// The decision for one main prompt `event` on `record`, whose pending input
/// is `pending` when the window marked it, at `now_ms`.
pub(super) fn decide(
    record: &Value,
    event: &Value,
    before: &Before,
    pending: Option<&(String, Value, crate::desktop::person_mark::Mark)>,
    now_ms: i64,
) -> Decision {
    let kind = string(event, "hook_event_name");
    let agent = string(record, "agent");
    let turn = &record[FIELD];
    // Kimi reports one input with more than one hook; a companion of the
    // marked input (its submission before its turn starts, or the same turn
    // again) neither starts nor ends anything.
    if agent == "kimi" {
        if let Some((request, receipt, _)) = pending {
            let same_input = turn.is_null() || turn["request_id"] == request.as_str();
            if matches!(kind, "UserPromptSubmit" | "UserPromptQueued")
                && same_input
                && record[SPENT] != request.as_str()
                && same_text(agent, event, receipt)
            {
                return Decision::Keep;
            }
        }
        if kind == "TurnStarted"
            && turn.is_object()
            && turn["native_turn"].as_u64().is_some()
            && event["turn_id"].as_u64() == turn["native_turn"].as_u64()
        {
            return Decision::Keep;
        }
    }
    let seen = record["input_seen_at"].as_f64().unwrap_or(0.0);
    let Some((request, receipt, mark)) = pending else {
        return Decision::End;
    };
    let native = native_identity(agent, event);
    let bound = binding_event(agent, event)
        && native.is_some()
        // A message binds within twelve hours of its admission.
        && (0..=crate::desktop::person_mark::TURN_MS).contains(&(now_ms - mark.at))
        && record[SPENT] != request.as_str()
        && turn.is_null()
        && !before.tools
        && record["input_seen_at"] != before.seen
        && seen > record["input_pending_at"].as_f64().unwrap_or(f64::INFINITY)
        && same_text(agent, event, receipt);
    if !bound {
        return Decision::End;
    }
    Decision::Bind(json!({
        "request_id": request,
        "origin": mark.origin,
        "run_id": record["run_id"],
        "conversation_id": record["conversation_id"],
        "admitted_at": mark.at,
        "bound_at": now_ms,
        "native_turn": native,
    }))
}

/// Whether the send itself witnessed that the native agent accepted exactly
/// this receipt's input as the native input `native` that the person turn's
/// hook bound (zerus-46l1q, reviews S1-1, D1 and R3-1). For Claude the send
/// writer saw Claude's own typed user row of the message and recorded that
/// row's `promptId` in the receipt (`native_input`); the hook's `prompt_id`
/// must be exactly that. No fresh text-and-time match is made at use. A
/// queue entry, which no recorded consumer joins to a turn, witnesses
/// nothing; neither does a Codex or Kimi send, whose sender records no native
/// input identity: those sends still work, but grant nothing. Until then a
/// bound turn grants nothing; a send left uncertain never does.
pub(crate) fn delivered(record: &Value, receipt: &Value, native: &Value) -> bool {
    if receipt["status"] != "submitted" || native.is_null() {
        return false;
    }
    match string(record, "agent") {
        "claude" => {
            receipt["native"] == "transcript"
                && receipt["native_input"]["prompt_id"].as_str().is_some()
                && receipt["native_input"]["prompt_id"] == *native
        }
        _ => false,
    }
}

/// Binds or ends the person turn for one admitted main hook event, after the
/// hook's own activity and acceptance updates and before it writes.
pub(super) fn observe(record: &mut Value, event: &Value, before: &Before) {
    let kind = string(event, "hook_event_name");
    if kind == "SessionStart" && string(event, "agent_id").is_empty() {
        // A new run or conversation ends it; a start in the same run and
        // conversation (Claude's after compaction) is no one's input.
        let turn = &record[FIELD];
        if turn.is_object()
            && (turn["run_id"] != record["run_id"]
                || turn["conversation_id"] != record["conversation_id"])
        {
            end(record);
        }
        return;
    }
    if !prompt_event(kind) || !main_event(record, event) {
        return;
    }
    let Ok(config) = crate::config::Config::load() else {
        end(record);
        return;
    };
    let pending = marked_pending(&config, record);
    match decide(
        record,
        event,
        before,
        pending.as_ref(),
        (now() * 1000.0) as i64,
    ) {
        Decision::Keep => {}
        Decision::End => {
            end(record);
            if let Some((request, _, _)) = &pending {
                record[SPENT] = json!(request);
            }
        }
        Decision::Bind(turn) => {
            record[SPENT] = turn["request_id"].clone();
            record[FIELD] = turn;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(at: i64) -> crate::desktop::person_mark::Mark {
        crate::desktop::person_mark::Mark {
            session: "kimi/project/asked".into(),
            run_id: "run-1".into(),
            conversation_id: "conversation-1".into(),
            origin: "desktop".into(),
            input_sha256: "a".repeat(64),
            at,
        }
    }
    /// A record whose pending input is the marked message `text`, after the
    /// hook's activity update accepted it at 20 s.
    fn pending(
        agent: &str,
        text: &str,
    ) -> (
        Value,
        Before,
        (String, Value, crate::desktop::person_mark::Mark),
    ) {
        let record = json!({"name": format!("{agent}/project/asked"), "agent": agent, "run_id": "run-1",
            "conversation_id": "conversation-1", "input_pending_request": "r1", "input_pending_at": 10.0,
            "input_seen_at": 20.0});
        let before = Before {
            tools: false,
            seen: json!(5.0),
        };
        let receipt = json!({"request_id": "r1", "submitted_text": text});
        (record, before, ("r1".into(), receipt, mark(9_000)))
    }
    const NOW: i64 = 30_000;

    #[test]
    fn codex_binds_at_its_prompt_submission_of_exactly_the_marked_text() {
        let (record, before, marked) = pending("codex", "hold the cutover");
        let submit = |text: &str| json!({"hook_event_name": "UserPromptSubmit", "prompt": text, "turn_id": "turn-7"});
        let Decision::Bind(turn) = decide(
            &record,
            &submit("hold the cutover"),
            &before,
            Some(&marked),
            NOW,
        ) else {
            panic!("the marked prompt binds");
        };
        // The bound turn names its native turn, which delivery must show
        // (review D1); a prompt without one binds nothing.
        assert_eq!(
            (turn["request_id"].as_str(), turn["native_turn"].clone()),
            (Some("r1"), json!("turn-7"))
        );
        assert_eq!(
            decide(
                &record,
                &json!({"hook_event_name": "UserPromptSubmit", "prompt": "hold the cutover"}),
                &before,
                Some(&marked),
                NOW
            ),
            Decision::End
        );
        assert_eq!(
            decide(
                &record,
                &submit("another text"),
                &before,
                Some(&marked),
                NOW
            ),
            Decision::End
        );
        assert_eq!(
            decide(&record, &submit("hold the cutover"), &before, None, NOW),
            Decision::End
        );
        let busy = Before {
            tools: true,
            seen: json!(5.0),
        };
        assert_eq!(
            decide(
                &record,
                &submit("hold the cutover"),
                &busy,
                Some(&marked),
                NOW
            ),
            Decision::End
        );
        // Codex's turn start is no binding event of its own.
        let started = json!({"hook_event_name": "TurnStarted", "prompt": "hold the cutover"});
        assert_eq!(
            decide(&record, &started, &before, Some(&marked), NOW),
            Decision::End
        );
    }

    #[test]
    fn kimi_binds_at_its_user_turn_and_its_companion_reports_change_nothing() {
        let (mut record, before, marked) = pending("kimi", "hold the cutover");
        let text = json!([{"type": "text", "text": "hold the cutover"}]);
        let submit = json!({"hook_event_name": "UserPromptSubmit", "prompt": text});
        let started = |id: u64, origin: &str| {
            json!({"hook_event_name": "TurnStarted", "turn_id": id,
            "origin_kind": origin, "prompt": text})
        };
        // Its submission before the turn starts is a companion of the input.
        assert_eq!(
            decide(&record, &submit, &before, Some(&marked), NOW),
            Decision::Keep
        );
        assert_eq!(
            decide(&record, &started(3, "task"), &before, Some(&marked), NOW),
            Decision::End
        );
        let Decision::Bind(turn) =
            decide(&record, &started(3, "user"), &before, Some(&marked), NOW)
        else {
            panic!("Kimi's user turn binds");
        };
        assert_eq!(turn["native_turn"], 3);
        record[FIELD] = turn;
        record[SPENT] = json!("r1");
        // The same turn reported again keeps it; a new turn or another text ends it.
        assert_eq!(
            decide(&record, &started(3, "user"), &before, Some(&marked), NOW),
            Decision::Keep
        );
        assert_eq!(
            decide(&record, &started(4, "user"), &before, Some(&marked), NOW),
            Decision::End
        );
        let other = json!({"hook_event_name": "UserPromptSubmit", "prompt": [{"type": "text", "text": "x"}]});
        assert_eq!(
            decide(&record, &other, &before, Some(&marked), NOW),
            Decision::End
        );
        // Once spent, the same submission is no companion any more.
        record.as_object_mut().unwrap().remove(FIELD);
        assert_eq!(
            decide(&record, &submit, &before, Some(&marked), NOW),
            Decision::End
        );
    }

    #[test]
    fn a_start_in_the_same_run_and_conversation_keeps_the_turn_and_a_change_ends_it() {
        let turn = json!({"request_id": "r1", "run_id": "run-1", "conversation_id": "c1"});
        let start = json!({"hook_event_name": "SessionStart"});
        let before = Before {
            tools: false,
            seen: Value::Null,
        };
        let mut same = json!({"agent": "claude", "run_id": "run-1", "conversation_id": "c1", "person_turn": turn});
        observe(&mut same, &start, &before);
        assert!(same[FIELD].is_object(), "compaction is no one's input");
        for (field, value) in [("run_id", "run-2"), ("conversation_id", "c2")] {
            let mut changed = json!({"agent": "claude", "run_id": "run-1", "conversation_id": "c1", "person_turn": turn});
            changed[field] = json!(value);
            observe(&mut changed, &start, &before);
            assert!(changed.get(FIELD).is_none(), "{field}");
            assert_eq!(changed[SPENT], "r1");
        }
        // A subagent's start is not the session's.
        let mut child = json!({"agent": "claude", "run_id": "run-2", "conversation_id": "c1", "person_turn": turn});
        observe(
            &mut child,
            &json!({"hook_event_name": "SessionStart", "agent_id": "a1"}),
            &before,
        );
        assert!(child[FIELD].is_object());
    }

    #[test]
    fn claude_delivery_is_only_the_send_witnessed_row_of_the_bound_prompt() {
        // zerus-46l1q review R3-1: the receipt carries the exact row the send
        // saw; the hook's bound prompt must be that row's promptId.
        let prompt = uuid::Uuid::new_v4().to_string();
        let record = json!({"agent": "claude"});
        let receipt = |status: &str, native: Value, input: Value| json!({"status": status, "native": native, "native_input": input});
        let witnessed = json!({"prompt_id": prompt, "uuid": uuid::Uuid::new_v4().to_string()});
        let bound = json!(prompt);
        assert!(delivered(
            &record,
            &receipt("submitted", json!("transcript"), witnessed.clone()),
            &bound
        ));
        // Another prompt, no witness, a queue entry, an emptied composer, an
        // uncertain or unfinished send, or no bound prompt grant nothing.
        assert!(!delivered(
            &record,
            &receipt("submitted", json!("transcript"), witnessed.clone()),
            &json!(uuid::Uuid::new_v4().to_string())
        ));
        assert!(!delivered(
            &record,
            &receipt("submitted", json!("transcript"), Value::Null),
            &bound
        ));
        for native in [json!("queue"), json!("composer"), Value::Null] {
            assert!(!delivered(
                &record,
                &receipt("submitted", native, witnessed.clone()),
                &bound
            ));
        }
        for status in ["in_progress", "uncertain"] {
            assert!(!delivered(
                &record,
                &receipt(status, json!("transcript"), witnessed.clone()),
                &bound
            ));
        }
        assert!(!delivered(
            &record,
            &receipt("submitted", json!("transcript"), witnessed.clone()),
            &Value::Null
        ));
        // Codex and Kimi senders witness no native input: no grant.
        for agent in ["codex", "kimi", "opencode"] {
            assert!(!delivered(
                &json!({"agent": agent}),
                &receipt("submitted", json!("transcript"), witnessed.clone()),
                &bound
            ));
        }
    }

    #[test]
    fn every_agent_binds_only_with_the_native_identity_of_its_input() {
        let prompt = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            native_identity("claude", &json!({"prompt_id": prompt})),
            Some(json!(prompt))
        );
        for bad in [
            json!(null),
            json!(""),
            json!("not-a-uuid"),
            json!(7),
            json!(uuid::Uuid::nil().to_string()),
        ] {
            assert_eq!(native_identity("claude", &json!({"prompt_id": bad})), None);
        }
        assert_eq!(
            native_identity("codex", &json!({"turn_id": "turn-1"})),
            Some(json!("turn-1"))
        );
        assert_eq!(native_identity("codex", &json!({})), None);
        assert_eq!(
            native_identity("kimi", &json!({"turn_id": 4})),
            Some(json!(4))
        );
        assert_eq!(native_identity("kimi", &json!({"turn_id": "4"})), None);
        assert_eq!(native_identity("opencode", &json!({"turn_id": "4"})), None);
    }

    #[test]
    fn only_a_single_text_part_is_a_kimi_prompt() {
        let one = json!({"prompt":[{"type":"text","text":"exact"}]});
        assert_eq!(prompt_text("kimi", &one), Some("exact"));
        assert_eq!(prompt_text("claude", &one), None);
        for parts in [
            json!([]),
            json!([{"type":"text","text":"a"},{"type":"text","text":"b"}]),
            json!([{"type":"image","text":"exact"}]),
        ] {
            assert_eq!(prompt_text("kimi", &json!({"prompt":parts})), None);
        }
    }

    #[test]
    fn a_turn_ends_once_and_its_request_is_spent() {
        let mut record = json!({"person_turn":{"request_id":"r1"}});
        end(&mut record);
        assert!(record.get(FIELD).is_none());
        assert_eq!(record[SPENT], "r1");
        end(&mut record);
        assert_eq!(record[SPENT], "r1", "no turn, nothing changes");
    }
}
