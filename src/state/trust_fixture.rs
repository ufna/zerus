//! Startup folder-trust cards as the producers write them (zerus-63rru.60):
//! a new run's record, whose conversation is null until its first
//! SessionStart, at each producer's committed screen, through the producer's
//! own parse and card. Readers of these cards (the Teams policy,
//! the approval audit, the delivery request) test with them, never with a
//! hand-written question.
use super::*;

/// The folder every committed trust screen shows.
pub(crate) const FOLDER: &str = "/work/example";

/// A new run's record of `agent` before its first SessionStart, with the keys
/// the producers read: the supervisor writes `"conversation_id": null` at
/// launch (lifecycle.rs), and a startup card copies it.
pub(crate) fn record(agent: &str) -> Value {
    json!({"version":1,"run_identity_version":1,"name":format!("{agent}/example/member"),
        "agent":agent,"run_id":"00000000-0000-4000-8000-000000000601","pane":"%1",
        "cwd":FOLDER,"launch_dir":FOLDER,"activity":"unknown","phase":"unknown",
        "conversation_state":"unknown","conversation_id":null,"transcript":null,"startup_kind":"new",
        "launch_id":"00000000-0000-4000-8000-000000000602","account_id":format!("native-{agent}"),
        "pid":4242,"process_start":"fixture-start","supervisor":{"pid":4242,"start":"fixture-start"},
        "session_id":"00000000-0000-4000-8000-000000000603"})
}

/// The pending startup card `agent`'s producer writes for [`record`].
pub(crate) fn startup_card(agent: &str) -> Value {
    let record = record(agent);
    match agent {
        "claude" => claude_trust::produced(
            &record,
            include_str!("../../tests/fixtures/claude-folder-trust.txt"),
        ),
        "codex" => codex_trust::produced(
            &record,
            include_str!("../../tests/fixtures/codex-folder-trust.txt"),
        ),
        "kimi" => kimi_trust::produced(
            &record,
            include_str!("../../tests/fixtures/kimi-folder-trust.txt"),
        ),
        _ => None,
    }
    .unwrap_or_else(|| panic!("{agent}'s producer wrote no startup card"))
}
