//! The local Desktop Send route and its separate, display-only host observation.
use serde::{Deserialize, Serialize};
use zerus_desktop_migration::canonical_uuid;

pub const HELP_MARKER: &str = "Desktop verified send ABI: desktop-verified-send-v1";
pub const VERSION: u8 = 1;
/// The same bound as the owned original helper's stdin and pipe custody.
pub const ENVELOPE_BYTES: usize = 1 << 20;

/// A host's corroborated observation of one run. Never an input permit.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Readiness {
    pub version: u8,
    pub session_id: String,
    pub run_id: String,
    pub conversation_id: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub send: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pause: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub terminate: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stop: bool,
}

impl Readiness {
    pub fn matches(&self, session: &str, run: &str, conversation: &str) -> bool {
        self.version == VERSION
            && canonical_uuid(session)
            && self.session_id == session
            && !run.is_empty()
            && self.run_id == run
            && !conversation.is_empty()
            && self.conversation_id == conversation
    }
}

/// One explicit composer submit. The existing checkpoint binds the exact
/// request and queued draft. Only the native frontend registers this request.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub session_id: String,
    pub send: super::deferred_send::Intent,
}

impl Intent {
    pub fn valid(&self) -> bool {
        self.verify().is_ok()
    }

    pub fn verify(&self) -> Result<super::deferred_send::Plan, &'static str> {
        if !canonical_uuid(&self.session_id)
            || !self.send.session.starts_with("codex/")
            || self.send.original.len() > ENVELOPE_BYTES
            || self.send.checkpoint.len() > zerus_watch_protocol::CONTROL_LINE_BYTES
        {
            return Err("invalid_verified_send_intent");
        }
        let plan = self.send.clone().verify_original()?.plan;
        if plan.conversation_id.is_empty() {
            return Err("invalid_verified_send_conversation");
        }
        Ok(plan)
    }
}

/// A reply names the same original even when the native write is uncertain.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub request_id: String,
    pub run_id: String,
    pub conversation_id: String,
    pub outcome: Outcome,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    Submitted,
    Refused { reason: String },
    Unknown,
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::operation_recovery::deferred_input::{self, Checkpoint, Kind};
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde_json::json;

    pub(in crate::desktop) fn intent(bytes: &[u8]) -> Intent {
        let name = "private.txt";
        let mime = "text/plain";
        let checkpoint = Checkpoint::reserve(deferred_input::Intent {
            kind: Kind::Send,
            draft_revision: 1,
            message_sha256: deferred_input::message_sha256(
                "private fixture",
                [(name, mime, "", bytes)],
            ),
            run_id: "run-one".into(),
            conversation_id: "conversation-one".into(),
            question: None,
            start: None,
            retry_of: None,
            consent: false,
        });
        Intent {
            session_id: "11111111-1111-4111-8111-111111111111".into(),
            send: super::super::deferred_send::Intent {
                scope: super::super::deferred_send::Scope {
                    configuration: "0".repeat(64),
                    service_epoch: "22222222-2222-4222-8222-222222222222".into(),
                },
                session: "codex/private/original".into(),
                request_id: checkpoint.request_id.clone(),
                checkpoint: serde_json::to_string(&checkpoint).unwrap(),
                original: json!({
                    "request_id": checkpoint.request_id, "text": "private fixture",
                    "expected_run_id": checkpoint.run_id,
                    "expected_conversation_id": checkpoint.conversation_id,
                    "attachments": [{"name":name, "mime":mime, "data_base64":STANDARD.encode(bytes)}],
                }).to_string(),
            },
        }
    }

    #[test]
    fn verified_send_accepts_an_attachment_above_control_limit_and_refuses_large_input() {
        let normal = intent(&vec![b'x'; 64 << 10]);
        assert!(normal.send.original.len() > zerus_watch_protocol::CONTROL_LINE_BYTES);
        assert_eq!(normal.verify().unwrap().conversation_id, "conversation-one");
        assert!(matches!(
            normal.send.verify(),
            Err("deferred_send_original_too_large")
        ));

        let oversized = intent(&vec![b'x'; ENVELOPE_BYTES]);
        assert!(oversized.send.original.len() > ENVELOPE_BYTES);
        assert_eq!(
            oversized.verify().unwrap_err(),
            "invalid_verified_send_intent"
        );
    }
}
