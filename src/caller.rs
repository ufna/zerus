//! Request-bound person authority (zerus-46l1q, S1). An agent acts as the
//! person only for the message the person typed to its own session: the
//! window marked it at admission, the session's prompt hook bound it to the
//! native turn that accepted it, and no other input has reached the session
//! since, for at most twelve hours.
//!
//! [`person_request`] proves that for the command that runs it, from the
//! session's own live record, the window's mark and the CLI's receipt, every
//! time. Its [`PersonRequest`] cannot be cloned, serialized or rebuilt from
//! text: no environment variable, flag, file or forwarded field carries it, so
//! a child, a team member, another computer or a later command never inherits
//! it. A writer checks it again under its own lock ([`PersonRequest::recheck`]).
//!
//! Same-user limit: this is no boundary against a process of the same user
//! that writes Zerus state files directly; it removes the accidental-use class
//! and records the message identity with every action.
use crate::state::{self, person_turn};
use serde_json::{json, Value};

/// The proven request of the person's message, for one command. Neither
/// `Clone` nor `Deserialize`: only [`person_request`] makes one.
#[derive(Debug)]
pub(crate) struct PersonRequest {
    machine: String,
    session: String,
    session_id: String,
    run_id: String,
    conversation_id: String,
    request_id: String,
    origin: String,
    admitted_at: i64,
    bound_at: i64,
}

/// Why no person request holds for this command; its text tells the agent
/// what to do. Only Linux marks the person's own message today; on macOS
/// (until its own mark, zerus-uav2j) and on Windows it never lets an agent
/// act for them (review D4).
#[cfg(target_os = "linux")]
pub(crate) const ASK: &str = "ask the person in your own conversation; only their own message to this session, typed in the Zerus window, lets an agent act for them";
#[cfg(target_os = "macos")]
pub(crate) const ASK: &str = "on macOS the person's own message does not yet let an agent act for them; ask the person to do this themselves";
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) const ASK: &str = "on Windows the person's own message does not let an agent act for them; ask the person to do this themselves";

impl PersonRequest {
    /// The message identity a writer records with the action.
    pub(crate) fn identity(&self) -> Value {
        json!({"machine": self.machine, "session": self.session, "session_id": self.session_id,
            "run_id": self.run_id, "conversation_id": self.conversation_id,
            "request_id": self.request_id, "origin": self.origin})
    }

    /// The message identity as the task history records it.
    pub(crate) fn reference(&self) -> zerus_replica::PersonRequestRef {
        zerus_replica::PersonRequestRef {
            machine: self.machine.clone(),
            session: self.session.clone(),
            session_id: self.session_id.clone(),
            run_id: self.run_id.clone(),
            conversation_id: self.conversation_id.clone(),
            request_id: self.request_id.clone(),
            origin: self.origin.clone(),
        }
    }

    /// Whether `other` is this same request of the same session and run.
    pub(crate) fn same(&self, other: &PersonRequest) -> bool {
        self.request_id == other.request_id
            && self.run_id == other.run_id
            && self.conversation_id == other.conversation_id
            && self.session == other.session
            && self.machine == other.machine
    }

    /// Whether this request may stand where an earlier guard of the same
    /// command admitted `held`: only that same request (review S2-1).
    pub(crate) fn continues(&self, held: Option<&PersonRequest>) -> bool {
        held.is_none_or(|held| held.same(self))
    }

    /// Whether the request belongs to the acting session `own`.
    pub(crate) fn is_of(&self, own: &zerus_replica::SessionKey) -> bool {
        self.machine == own.machine
            && if own.session_id.is_empty() {
                self.session == own.session
            } else {
                self.session_id == own.session_id
            }
    }

    /// What `hgs self` shows: the request, its origin and its age.
    pub(crate) fn summary(&self, now_ms: i64) -> Value {
        json!({"request_id": self.request_id, "origin": self.origin,
            "age_seconds": (now_ms - self.admitted_at).max(0) / 1000,
            "expires_in_seconds": (crate::desktop::person_mark::TURN_MS
                - (now_ms - self.bound_at)).max(0) / 1000})
    }

    /// The same request still holds, read again from the session's record,
    /// the mark and the receipt; a writer calls this under its own lock.
    pub(crate) fn recheck(&self) -> Result<(), String> {
        let again = person_request()?;
        if !self.same(&again) {
            return Err(format!(
                "the person's request ended before this action; {ASK}"
            ));
        }
        Ok(())
    }
}

/// The person request of the session that runs this command, or why there is
/// none.
pub(crate) fn person_request() -> Result<PersonRequest, String> {
    if state::session_graph::hosted_agent_call() {
        return Err(format!(
            "a DeepSeek tool call holds no person request; {ASK}"
        ));
    }
    let record = state::session_graph::caller_record().ok_or_else(|| {
        format!("no live Zerus session of this computer runs this command; {ASK}")
    })?;
    let config = crate::config::Config::load().map_err(|e| e.to_string())?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    proven(&config, &record, now_ms)
}

fn proven(
    config: &crate::config::Config,
    record: &Value,
    now_ms: i64,
) -> Result<PersonRequest, String> {
    let none = || format!("this session has no person request now; {ASK}");
    let turn = &record[person_turn::FIELD];
    if !turn.is_object() || record["state"] == "archived" {
        return Err(none());
    }
    let (request, receipt, mark) = person_turn::marked_pending(config, record).ok_or_else(none)?;
    // Delivery first: an admitted message that the native agent has not
    // shown it accepted grants nothing, whatever a hook reported (S1-1).
    if !person_turn::delivered(record, &receipt, &turn["native_turn"]) {
        return Err(format!(
            "the person's message is not yet proven delivered to this session; {ASK}"
        ));
    }
    let text = |value: &Value, key: &str| value[key].as_str().unwrap_or_default().to_owned();
    let bound_at = turn["bound_at"].as_i64().unwrap_or(0);
    // The turn lasts at most twelve hours from the hook that bound it.
    if turn["request_id"] != request.as_str()
        || turn["run_id"] != record["run_id"]
        || turn["conversation_id"] != record["conversation_id"]
        || turn["origin"] != mark.origin.as_str()
        || turn["admitted_at"] != mark.at
        || !(mark.at..=now_ms).contains(&bound_at)
        || now_ms - bound_at > crate::desktop::person_mark::TURN_MS
    {
        return Err(none());
    }
    // One bounded read of the existing machine identity: a proof never loads,
    // locks or saves the catalog, which a writer may already hold.
    let machine = state::session_graph::caller_machine(config).map_err(str::to_owned)?;
    Ok(PersonRequest {
        machine,
        session: text(record, "name"),
        session_id: crate::teams::caller::identity(record),
        run_id: text(record, "run_id"),
        conversation_id: text(record, "conversation_id"),
        request_id: request,
        origin: mark.origin,
        admitted_at: mark.at,
        bound_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: &str, run: &str) -> PersonRequest {
        PersonRequest {
            machine: "m1".into(),
            session: "claude/project/asked".into(),
            session_id: "launch_id:asked".into(),
            run_id: run.into(),
            conversation_id: "conversation-1".into(),
            request_id: id.into(),
            origin: "desktop".into(),
            admitted_at: 1,
            bound_at: 2,
        }
    }

    #[test]
    fn a_later_guard_admits_only_the_same_request() {
        let first = request("r1", "run-1");
        assert!(first.continues(None));
        assert!(request("r1", "run-1").continues(Some(&first)));
        assert!(!request("r2", "run-1").continues(Some(&first)));
        assert!(!request("r1", "run-2").continues(Some(&first)));
    }
}
