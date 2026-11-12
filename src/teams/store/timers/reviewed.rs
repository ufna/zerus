//! The actor's read before it finishes, merges or reopens a reviewed task:
//! whether the newest observation still shows what the acceptance was
//! decided for. A read of one row of each table it names, on the ordinary
//! store path, whatever the node's history.
use super::*;

pub(crate) mod sql {
    pub const REVIEWED: &str = "SELECT EXISTS(SELECT 1 FROM task_observations o JOIN live_tasks l ON l.team=o.team AND l.node=o.node WHERE o.team=?1 AND o.node=?2 AND COALESCE((SELECT n.generation FROM nodes n WHERE n.team=?1 AND n.id=?2),(SELECT a.generation FROM node_archive a WHERE a.team=?1 AND a.id=?2))=?3 AND json_extract(o.data,'$.key') IS NULL AND json_extract(o.data,'$.stage')='in_review' AND NOT json_extract(o.data,'$.cancelled') AND NOT o.attempt_conflict AND ((o.attempt_id<>'' AND o.attempt_id=o.last_attempt_id) OR (o.attempt=0 AND o.attempt_id='' AND o.last_attempt_id='')))";
}

impl Store {
    /// True only when the node's current generation is `generation`, its
    /// task is bound, and the newest observation of that task has the key
    /// ours (an Ours key is not serialized), the stage In review, is not
    /// cancelled and has no conflict of attempts, and its current attempt is
    /// the execution the node took in: the remembered attempt, or, for a row
    /// that holds no attempt at all, the legacy pair on both sides. False in
    /// every other case, without an observation or a bound task included.
    pub fn reviewed_task_current(&self, team: &str, node: u32, generation: u32) -> Result<bool> {
        self.read(|db| {
            db.query_row(sql::REVIEWED, params![team, node, generation], |r| r.get(0))
                .map_err(storage)
        })
    }
}
