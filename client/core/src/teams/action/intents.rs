//! One intent, one request id, one send.
//!
//! The person's action makes a request id before the first send, and the core
//! keeps it. [`Intents`] records what became of each id: while it is pending a
//! second call does nothing; once settled a call returns the recorded outcome;
//! an uncertain intent is never sent again automatically, only the person's
//! `check_again` sends it, with the same id and content, and the store answers
//! from its own record.
use super::types::{canonical_uuid, TeamCall, TeamOutcome};
use super::wire::run_action;
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::Path;

/// Intents the desktop remembers; the oldest settled ones go first.
pub const INTENTS_KEPT: usize = 64;

/// Where an intent stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentState {
    Pending,
    Done,
    Failed,
    Uncertain,
}

/// What the intent table says about a call.
#[derive(Clone, Debug, PartialEq)]
pub enum Admission {
    /// Nothing is known about it: send it.
    Send,
    /// The same intent is in flight: send nothing.
    InFlight,
    /// It settled: this outcome stands.
    Recorded(TeamOutcome),
    /// The request id was reused with different content, or the table is full:
    /// nothing is sent.
    Refused(TeamOutcome),
}

#[derive(Debug)]
struct Intent {
    fingerprint: u64,
    state: IntentState,
    outcome: Option<TeamOutcome>,
}

/// The desktop's bounded table of intents by request id.
#[derive(Debug, Default)]
pub struct Intents {
    entries: BTreeMap<String, Intent>,
    order: VecDeque<String>,
}

/// The call's content, so one id reused for another action is refused.
fn fingerprint(call: &TeamCall) -> u64 {
    let mut hasher = DefaultHasher::new();
    if let Ok(content) = serde_json::to_string(call) {
        content.hash(&mut hasher);
    }
    hasher.finish()
}

fn state_of(outcome: &TeamOutcome) -> Option<IntentState> {
    match outcome {
        TeamOutcome::Done { .. } => Some(IntentState::Done),
        TeamOutcome::Failed { .. } => Some(IntentState::Failed),
        TeamOutcome::Uncertain { .. } => Some(IntentState::Uncertain),
        TeamOutcome::Confirm { .. } | TeamOutcome::Pending { .. } => None,
    }
}

fn pending(request_id: &str) -> TeamOutcome {
    TeamOutcome::Pending {
        request_id: request_id.to_owned(),
    }
}

fn refused(request_id: &str, detail: &str) -> TeamOutcome {
    TeamOutcome::Failed {
        detail: detail.to_owned(),
        code: String::new(),
        request_id: request_id.to_owned(),
    }
}

impl Intents {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Where the intent stands, when it is known.
    pub fn state(&self, request_id: &str) -> Option<IntentState> {
        self.entries.get(request_id).map(|intent| intent.state)
    }

    /// The outcome recorded for a settled intent.
    pub fn outcome(&self, request_id: &str) -> Option<&TeamOutcome> {
        self.entries
            .get(request_id)
            .and_then(|intent| intent.outcome.as_ref())
    }

    /// What to do with a call the desktop sends by itself.
    pub fn admit(&mut self, call: &TeamCall) -> Admission {
        if !canonical_uuid(&call.request_id) {
            return Admission::Refused(refused(&call.request_id, "Not a request id."));
        }
        let fingerprint = fingerprint(call);
        if let Some(intent) = self.entries.get(&call.request_id) {
            if intent.fingerprint != fingerprint {
                return Admission::Refused(refused(
                    &call.request_id,
                    "This request id was used for a different action.",
                ));
            }
            return match (&intent.state, &intent.outcome) {
                (IntentState::Pending, _) => Admission::InFlight,
                (_, Some(outcome)) => Admission::Recorded(outcome.clone()),
                _ => Admission::InFlight,
            };
        }
        if !self.make_room(&call.request_id) {
            return Admission::Refused(refused(
                &call.request_id,
                "Too many actions are in flight; wait for one to finish.",
            ));
        }
        self.entries.insert(
            call.request_id.clone(),
            Intent {
                fingerprint,
                state: IntentState::Pending,
                outcome: None,
            },
        );
        self.order.push_back(call.request_id.clone());
        Admission::Send
    }

    /// What to do with the person's explicit "check this intent again": the
    /// same id and content of an uncertain intent is sent again, because the
    /// store answers a repeated request from its record.
    pub fn admit_again(&mut self, call: &TeamCall) -> Admission {
        let fingerprint = fingerprint(call);
        let uncertain = self.entries.get(&call.request_id).is_some_and(|intent| {
            intent.fingerprint == fingerprint && intent.state == IntentState::Uncertain
        });
        if !uncertain {
            return self.admit(call);
        }
        if let Some(intent) = self.entries.get_mut(&call.request_id) {
            intent.state = IntentState::Pending;
        }
        Admission::Send
    }

    /// Records an outcome for an intent that was admitted. A confirmation sent
    /// nothing, so the intent stays available; anything else settles it.
    pub fn settle(&mut self, call: &TeamCall, outcome: TeamOutcome) -> TeamOutcome {
        let Some(state) = state_of(&outcome) else {
            // Nothing ran (a confirmation was asked): the intent may still be
            // triggered with the same id.
            self.entries.remove(&call.request_id);
            self.order.retain(|id| id != &call.request_id);
            return outcome;
        };
        let fingerprint = fingerprint(call);
        self.entries.insert(
            call.request_id.clone(),
            Intent {
                fingerprint,
                state,
                outcome: Some(outcome.clone()),
            },
        );
        if !self.order.iter().any(|id| id == &call.request_id) {
            self.order.push_back(call.request_id.clone());
        }
        outcome
    }

    /// Sends one intent once, unless the table already knows it.
    pub fn attempt(&mut self, hgs: &Path, call: &TeamCall) -> TeamOutcome {
        match self.admit(call) {
            Admission::Send => self.run(hgs, call),
            Admission::InFlight => pending(&call.request_id),
            Admission::Recorded(outcome) | Admission::Refused(outcome) => outcome,
        }
    }

    /// Sends an intent again because the person asked to check it.
    pub fn check_again(&mut self, hgs: &Path, call: &TeamCall) -> TeamOutcome {
        match self.admit_again(call) {
            Admission::Send => self.run(hgs, call),
            Admission::InFlight => pending(&call.request_id),
            Admission::Recorded(outcome) | Admission::Refused(outcome) => outcome,
        }
    }

    fn run(&mut self, hgs: &Path, call: &TeamCall) -> TeamOutcome {
        let outcome = run_action(hgs, call);
        self.settle(call, outcome)
    }

    /// Makes room for one more intent: the oldest settled one goes. False when
    /// every remembered intent is still pending.
    fn make_room(&mut self, request_id: &str) -> bool {
        if self.entries.contains_key(request_id) {
            return true;
        }
        while self.entries.len() >= INTENTS_KEPT {
            let Some(at) = self.order.iter().position(|id| {
                self.entries
                    .get(id)
                    .is_some_and(|intent| intent.state != IntentState::Pending)
            }) else {
                return false;
            };
            if let Some(id) = self.order.remove(at) {
                self.entries.remove(&id);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::teams::action::TeamAction;

    fn call(id: &str) -> TeamCall {
        TeamCall {
            team: "00000000-0000-4000-8000-00000000beef".into(),
            team_name: "Fixes".into(),
            action: TeamAction::Accept { node: 1 },
            expected_revision: Some(16),
            confirmed: false,
            request_id: id.into(),
        }
    }

    fn done(id: &str) -> TeamOutcome {
        TeamOutcome::Done {
            receipt: serde_json::json!({"ok": true, "revision": 17, "actor": {"kind": "person"},
                "team": "00000000-0000-4000-8000-00000000beef"}),
            request_id: id.into(),
        }
    }

    #[test]
    fn one_intent_is_pending_once_and_settles_to_its_outcome() {
        const ID: &str = "11111111-2222-4333-8444-555555555555";
        let mut intents = Intents::default();
        assert_eq!(intents.admit(&call(ID)), Admission::Send);
        assert_eq!(intents.state(ID), Some(IntentState::Pending));
        assert_eq!(intents.admit(&call(ID)), Admission::InFlight);
        assert_eq!(
            intents.settle(&call(ID), done(ID)),
            done(ID),
            "the outcome is recorded"
        );
        assert_eq!(intents.admit(&call(ID)), Admission::Recorded(done(ID)));
        assert_eq!(intents.state(ID), Some(IntentState::Done));
    }

    #[test]
    fn a_confirmation_does_not_settle_the_intent() {
        const ID: &str = "11111111-2222-4333-8444-555555555555";
        let mut intents = Intents::default();
        intents.admit(&call(ID));
        let confirmation = TeamOutcome::Confirm {
            confirmation: crate::teams::action::Confirmation {
                name: "teamStopConfirm".into(),
                title: "Stop team?".into(),
                text: "Stop it?".into(),
                action: "Stop team".into(),
            },
        };
        assert_eq!(
            intents.settle(&call(ID), confirmation.clone()),
            confirmation
        );
        assert_eq!(intents.state(ID), None, "nothing ran, so nothing settled");
        assert_eq!(intents.admit(&call(ID)), Admission::Send);
    }

    #[test]
    fn one_id_used_for_different_content_is_refused() {
        const ID: &str = "11111111-2222-4333-8444-555555555555";
        let mut intents = Intents::default();
        intents.admit(&call(ID));
        let mut other = call(ID);
        other.action = TeamAction::Hold { node: 2 };
        match intents.admit(&other) {
            Admission::Refused(TeamOutcome::Failed { detail, .. }) => {
                assert!(detail.contains("different action"), "{detail}");
            }
            other => panic!("a reused id is refused, got {other:?}"),
        }
    }

    #[test]
    fn the_table_is_bounded() {
        let mut intents = Intents::default();
        for at in 0..INTENTS_KEPT {
            let id = format!("00000000-0000-4000-8000-{at:012x}");
            assert_eq!(intents.admit(&call(&id)), Admission::Send, "{id}");
            intents.settle(&call(&id), done(&id));
        }
        assert_eq!(intents.len(), INTENTS_KEPT);
        let next = "ffffffff-0000-4000-8000-000000000001";
        assert_eq!(
            intents.admit(&call(next)),
            Admission::Send,
            "the oldest settled intent makes room"
        );
        assert_eq!(intents.len(), INTENTS_KEPT);
        // Every remembered intent pending: nothing more is admitted.
        let mut full = Intents::default();
        for at in 0..INTENTS_KEPT {
            let id = format!("00000000-0000-4000-8000-{at:012x}");
            full.admit(&call(&id));
        }
        assert!(matches!(
            full.admit(&call("ffffffff-0000-4000-8000-000000000001")),
            Admission::Refused(_)
        ));
    }

    #[test]
    fn an_uncertain_intent_is_not_sent_again_by_itself() {
        const ID: &str = "11111111-2222-4333-8444-555555555555";
        let mut intents = Intents::default();
        intents.admit(&call(ID));
        let uncertain = TeamOutcome::Uncertain {
            detail: "may have been applied".into(),
            request_id: ID.into(),
        };
        intents.settle(&call(ID), uncertain.clone());
        assert_eq!(intents.admit(&call(ID)), Admission::Recorded(uncertain));
        assert_eq!(intents.state(ID), Some(IntentState::Uncertain));
        // The person may ask to check it with the same id and content.
        assert_eq!(intents.admit_again(&call(ID)), Admission::Send);
        assert_eq!(intents.state(ID), Some(IntentState::Pending));
        // A settled intent is not sent again even by the person's request.
        intents.settle(&call(ID), done(ID));
        assert_eq!(
            intents.admit_again(&call(ID)),
            Admission::Recorded(done(ID))
        );
    }
}
