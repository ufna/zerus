use super::*;
const MINUTE: i64 = 60_000;
impl Runtime {
    pub(super) fn cached_accounts(
        &mut self,
        effects: &mut impl Effects,
        agent: &str,
    ) -> Result<Vec<zerus_team_plan::accounts::AccountUsage>> {
        self.accounts
            .entry(agent.into())
            .or_insert_with(|| effects.accounts(agent))
            .clone()
    }
    pub(super) fn retry(
        &mut self,
        store: &mut Store,
        row: &mut Row,
        problem: Problem,
        now: i64,
    ) -> Result<()> {
        row.state = State::Retry;
        self.failure(store, row, problem, now, true)
    }
    pub(super) fn failure(
        &mut self,
        store: &mut Store,
        row: &mut Row,
        problem: Problem,
        now: i64,
        notify: bool,
    ) -> Result<()> {
        self.admitted = None;
        let requested = std::mem::take(&mut row.retry_seen);
        row.problem = Some(bounded_problem(problem));
        row.failures = row.failures.saturating_add(1);
        let reset = row
            .problem
            .as_ref()
            .filter(|p| p.code == code::NO_ACCOUNT_AVAILABLE)
            .and_then(|p| p.refs.first())
            .and_then(|s| s.parse::<i64>().ok())
            .map(|s| s.saturating_mul(1000));
        row.retry_at = reset
            .unwrap_or(now.saturating_add(backoff(row.failures)))
            .max(now.saturating_add(MINUTE));
        self.save(row, now)?;
        if notify || requested || row.failures >= 3 {
            self.card(store, row, now)?;
        }
        Ok(())
    }
    pub(super) fn due(&mut self, store: &Store, row: &mut Row, now: i64) -> Result<bool> {
        if !row.retry_seen {
            if let Some(id) = row.card_id {
                if store
                    .card(&row.team, id)?
                    .is_some_and(|c| c.decision.as_deref() == Some("retry"))
                {
                    // The old card has a durable person decision. Clear its
                    // outbox once, before any retryable state's next check.
                    row.card_id = None;
                    row.card_request = None;
                    row.card_done = false;
                    row.retry_seen = true;
                    row.retry_at = now;
                    self.save(row, now)?;
                    return Ok(true);
                }
            }
        }
        Ok(now >= row.retry_at)
    }
    pub(super) fn close_stale(
        &mut self,
        store: &mut Store,
        team: &Team,
        row: &mut Row,
        now: i64,
    ) -> Result<()> {
        if let Some(card) = row.card_id {
            if store
                .card(&row.team, card)?
                .is_some_and(|c| c.state == "open")
            {
                apply(
                    store,
                    team,
                    Operation::CloseCard {
                        card,
                        reason: "The launch precondition recovered; this card is stale.".into(),
                    },
                    &mut row.close_request,
                    now,
                )?;
            }
        }
        row.card_id = None;
        row.card_request = None;
        row.card_done = false;
        row.retry_seen = false;
        row.problem = None;
        row.failures = 0;
        row.retry_at = 0;
        Ok(())
    }
    pub(super) fn record_failure(
        &mut self,
        store: &mut Store,
        id: &str,
        error: Problem,
        now: i64,
    ) -> Result<()> {
        let Some((team, member)) = self.selected.clone().filter(|(t, _)| t == id) else {
            return Ok(());
        };
        let Some(mut row) = self.ledger().get(&team, &member)? else {
            return Ok(());
        };
        // An unknown effect intent must be handled as interrupted on the next
        // pass, never changed into retryable pre-effect preparation.
        if matches!(
            row.state,
            State::WorktreeIntent | State::LaunchIntent | State::BranchIntent | State::ParentIntent
        ) {
            return Ok(());
        }
        self.failure(store, &mut row, error, now, false)
    }
    pub(super) fn note_removed(&mut self, team: &Team, member: &Member, now: i64) -> Result<()> {
        self.selected = None;
        if let Some(mut row) = self.ledger().get(&team.id, &member.id)? {
            self.selected = Some((row.team.clone(), row.member.clone()));
            if self.owns(&row) {
                self.admitted = None;
            }
            if !row.late_after_stop
                && matches!(
                    row.state,
                    State::LaunchIntent | State::Submitted | State::LaunchUnknown
                )
            {
                row.late_after_stop = true;
                self.save(&mut row, now)?;
            }
        }
        Ok(())
    }
    pub(super) fn note_late(&mut self, team: &Team, now: i64) -> Result<()> {
        if self.ledger.is_none() {
            if !self.state.join("teams/member-launch-v1.sqlite3").is_file() {
                return Ok(());
            }
            self.ledger = Some(Ledger::open(&self.state)?);
        }
        let mut after = String::new();
        for _ in 0..2 {
            let page = self.ledger().rows_page(&team.id, &after)?;
            after = page.last().map(|r| r.member.clone()).unwrap_or_default();
            let more = page.len() == 200;
            for mut row in page {
                if !row.late_after_stop
                    && matches!(
                        row.state,
                        State::LaunchIntent | State::Submitted | State::LaunchUnknown
                    )
                {
                    row.late_after_stop = true;
                    self.save(&mut row, now)?;
                }
            }
            if !more {
                break;
            }
        }
        Ok(())
    }
}
pub(super) fn backoff(failures: u32) -> i64 {
    MINUTE
        .saturating_mul(1i64 << failures.saturating_sub(1).min(4))
        .min(15 * MINUTE)
}
fn bounded_problem(mut p: Problem) -> Problem {
    p.message = super::card::cut(&p.message, 4096).into();
    p.refs = p
        .refs
        .into_iter()
        .take(8)
        .map(|r| super::card::cut(&r, 256).into())
        .collect();
    p
}
