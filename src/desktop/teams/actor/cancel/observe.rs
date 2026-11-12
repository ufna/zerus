//! A cancelled answer names work for the ordinary observer. It grants no
//! separate writer of a live task's end and needs no durable answer marker.
use super::*;
impl Runtime {
    pub(super) fn cancel_needs_observation(
        &self,
        store: &Store,
        team: &Team,
        row: &Entry,
    ) -> Result<bool> {
        let legacy = row.fact.as_ref().is_some_and(|f| !f.refused && !f.released);
        if row.pending.task.is_none()
            || !(matches!(row.reply, Some(TaskReply::Cancelled))
                || legacy && matches!(row.reply, Some(TaskReply::Released)))
        {
            return Ok(false);
        }
        Ok(store
            .team(&team.id)?
            .is_some_and(|t| matches!(t.status, TeamStatus::Running | TeamStatus::Paused)))
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn observe_cancelled(
        &mut self,
        store: &mut Store,
        p: &mut impl Ports,
        team: &Team,
        key: &str,
        row: &mut Entry,
        now: i64,
    ) -> Result<bool> {
        let Some(task) = row.pending.task.as_ref() else {
            return Ok(false);
        };
        let fresh = store
            .team(&team.id)?
            .ok_or_else(|| problem("Cancel team disappeared"))?;
        if !matches!(fresh.status, TeamStatus::Running | TeamStatus::Paused) {
            return Ok(false);
        }
        boundary("before_observe");
        p.observe(store, task)?;
        boundary("observed");
        if self.cancel_still_pending(store, p, &fresh, row)? {
            self.changed_cancel_visit(
                store,
                p,
                &fresh,
                key,
                row,
                "The ordinary observation left the same cancel decision pending.",
                now,
            )?;
        } else {
            self.cancel_done(store, &fresh, key, now)?;
        }
        Ok(true)
    }
}
