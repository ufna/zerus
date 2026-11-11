use super::*;
use crate::desktop::teams::ledger::BUDGET;
impl Runtime {
    pub(super) fn budget_due(&mut self, store: &mut Store, team: &Team, now: i64) -> Result<bool> {
        let Some(mut row) = self.ledger().get(&team.id, BUDGET)? else {
            return Ok(true);
        };
        if row.state == State::Done {
            return Ok(true);
        }
        self.card(store, &mut row, now)?;
        self.due(store, &mut row, now)
    }
    pub(super) fn notify_budget(
        &mut self,
        store: &mut Store,
        id: &str,
        problem: Problem,
        now: i64,
    ) -> Result<()> {
        let Some(team) = store.team(id)? else {
            return Ok(());
        };
        let mut row = match self.ledger().get(id, BUDGET)? {
            Some(row) => row,
            None => {
                let mut row = Row::new(&team, BUDGET, &self.epoch, State::Retry);
                row.problem = Some(problem.clone());
                row.retry_at = now;
                if !self.ledger().insert(&row, now)? {
                    return Ok(());
                }
                row
            }
        };
        self.retry(store, &mut row, problem, now)
    }
    pub(super) fn reserve_row(
        &mut self,
        store: &mut Store,
        team: &Team,
        row: &Row,
        now: i64,
    ) -> Result<bool> {
        let inserted = self.ledger().insert(row, now)?;
        if inserted {
            if let Some(mut notice) = self.ledger().get(&team.id, BUDGET)? {
                self.close_stale(store, team, &mut notice, now)?;
                notice.state = State::Done;
                self.save(&mut notice, now)?;
            }
        }
        Ok(inserted)
    }
}
