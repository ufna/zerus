//! Linux verified input adds the ordinary input state gates to the launch proof.
use super::*;
pub(super) use crate::state::native_launch::Witness;

pub(super) fn witness(record: &Value) -> Option<Witness> {
    Witness::read_checked(record, established)
}

pub(in crate::state) fn witness_for_inspection(record: &Value) -> bool {
    witness(record).is_some()
}

/// Select the exact Codex run without refreshing the presentation projection.
/// The ordinary input gate admits both idle and working. Child rollout reads
/// only refine those two states and are unnecessary for this observation.
pub(super) fn observation_record(name: &str, run: &str, conversation: &str) -> Result<Value> {
    let candidates = labels::candidates_metadata(name)?;
    let first = candidates
        .first
        .then(|| storage::read_metadata_at(&storage::label_record_path(name), name))
        .transpose()?;
    let mut matching = first.into_iter().chain(candidates.by_id).filter(|record| {
        string(record, "run_id") == run && string(record, "conversation_id") == conversation
    });
    let record = matching.next().ok_or("session identity changed; refresh before sending")?;
    if matching.next().is_some() {
        return Err("more than one session has this run and conversation; open Terminal".into());
    }
    Ok(record)
}

pub(in crate::state) fn established(record: &Value) -> bool {
    string(record, "expected_id").is_empty()
        && string(record, "error").is_empty()
        && record["last_event_at"].as_f64().is_some_and(|at| at > 0.0)
        && ["paused", "starting", "clearing", "resuming"]
            .iter()
            .all(|key| record[*key].is_null() || record[*key] == false)
        && [
            "pausing",
            "terminating",
            "resume_pending",
            "restore_archive_id",
            "restore_archive_cleanup",
            "session_end",
            "exited_at",
            "pending_settings",
        ]
        .iter()
        .all(|key| record[*key].is_null())
        && !resume_input::pending(record)
        && !resume_input::fork_pending(record)
        && !codex_tasks::multiple(record)
}
