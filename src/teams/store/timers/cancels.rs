//! The pending cancel decisions the service carries out, read through the
//! partial index of unsettled decisions: at most one per live node, so the
//! work never grows with settled history.
use super::*;

pub(crate) mod sql {
    pub const PENDING: &str = "SELECT d.node,d.generation,d.seq,i.task_project,i.task_uuid FROM decisions d LEFT JOIN node_identity i ON i.team=d.team AND i.id=d.node WHERE d.team=?1 AND d.state<>'settled' AND d.node>?2 AND d.kind='cancel' AND d.state='pending' AND d.generation=COALESCE((SELECT n.generation FROM nodes n WHERE n.team=d.team AND n.id=d.node),(SELECT a.generation FROM node_archive a WHERE a.team=d.team AND a.id=d.node)) AND NOT EXISTS(SELECT 1 FROM cancel_outcomes c WHERE c.decision=d.seq) ORDER BY d.node LIMIT ?3";
    /// Paged by the row: a node may have refused cancels of several decisions.
    pub const REFUSED: &str = "SELECT node,decision,reason FROM cancel_outcomes WHERE team=?1 AND state='refused' AND (node,decision)>(?2,?3) ORDER BY node,decision LIMIT ?4";
}

/// One cancel decision for the service to carry out. `task` is the bound
/// task team-cancel names, or none when the node has no task at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingCancel {
    pub node: u32,
    pub generation: u32,
    pub decision: u64,
    pub task: Option<TaskRef>,
}

impl Store {
    /// Cancel decisions that are Pending for their node's current generation
    /// and have no recorded outcome, in node order after `after`, for a team
    /// of any status.
    pub fn pending_cancels(
        &self,
        team: &str,
        limit: u32,
        after: u32,
    ) -> Result<Vec<PendingCancel>> {
        let page = super::limit(limit)?;
        self.read(|db| {
            rows(db, sql::PENDING, params![team, after, page], |r| {
                let project: Option<String> = r.get(3)?;
                let uuid: Option<String> = r.get(4)?;
                let task = match (project, uuid) {
                    (Some(project), Some(uuid)) if !uuid.is_empty() => {
                        Some(TaskRef { project, uuid })
                    }
                    _ => None,
                };
                Ok(PendingCancel {
                    node: r.get(0)?,
                    generation: r.get(1)?,
                    decision: r.get(2)?,
                    task,
                })
            })
        })
    }
    /// The cancels the team-cancel child refused, with their reasons, for the
    /// team's status: `(node, decision, reason)` after the row `after`.
    pub fn cancel_refusals(
        &self,
        team: &str,
        after: (u32, u64),
        limit: u32,
    ) -> Result<Vec<(u32, u64, String)>> {
        let page = super::limit(limit)?;
        self.read(|db| {
            rows(
                db,
                sql::REFUSED,
                params![team, after.0, after.1, page],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        })
    }
}
