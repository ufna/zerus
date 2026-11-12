//! Persistent package-10 state. The actor owns the entry points and
//! their retry scopes; each effect remains behind its package's ports.
use super::super::items::{self, kind::KEY_CLEAR};
use super::super::timers;
use super::native::Native;
use super::*;
use std::path::PathBuf;
pub(in crate::desktop::teams) struct State {
    state: PathBuf,
    engine: timers::Timers,
    ledger: Option<timers::Ledger>,
}
impl State {
    pub fn new(state: &Path, epoch: &str) -> Self {
        Self {
            state: state.into(),
            engine: timers::Timers::new(epoch),
            ledger: None,
        }
    }
    fn prepare(&mut self) -> Result<()> {
        if self.ledger.is_none() {
            self.ledger = Some(timers::Ledger::open(&self.state)?);
        }
        Ok(())
    }
    pub fn team(&mut self, n: &Native, store: &mut Store, team: &Team, now: i64) -> Result<()> {
        self.prepare()?;
        let mut sessions = SessionRows(n);
        let mut effects = timers::Native {
            config: &n.config,
            machine: &n.machine,
            stop: &n.stop,
        };
        self.engine.team(
            store,
            self.ledger.as_ref().expect("opened timer ledger"),
            &mut sessions,
            &mut effects,
            &team.id,
            now,
        )
    }
    pub fn answers(&mut self, n: &Native, store: &mut Store, team: &Team, now: i64) -> Result<()> {
        self.prepare()?;
        let mut sessions = SessionRows(n);
        let mut effects = timers::Native {
            config: &n.config,
            machine: &n.machine,
            stop: &n.stop,
        };
        self.engine.answers(
            store,
            self.ledger.as_ref().expect("opened timer ledger"),
            &mut sessions,
            &mut effects,
            &team.id,
            now,
        )
    }
    pub fn terminal(&mut self, n: &Native, store: &mut Store, now: i64) -> Result<()> {
        self.prepare()?;
        let mut effects = timers::Native {
            config: &n.config,
            machine: &n.machine,
            stop: &n.stop,
        };
        self.engine.terminal(
            store,
            self.ledger.as_mut().expect("opened timer ledger"),
            &mut effects,
            now,
        )
    }
    /// The teams whose key sweep reached its end since the last call.
    pub fn finished_sweeps(&mut self) -> Vec<timers::Finished> {
        self.engine.finished_sweeps()
    }
    #[cfg(test)]
    pub fn push_finished(&mut self, finished: timers::Finished) {
        self.engine.push_finished(finished);
    }
}
struct SessionRows<'a>(&'a Native);
impl timers::Sessions for SessionRows<'_> {
    fn presence(&mut self, team: &Team, member: &Member) -> Option<timers::Presence> {
        presence(self.0, team, member)
    }
}
pub(super) fn presence(n: &Native, team: &Team, member: &Member) -> Option<timers::Presence> {
    if member.session.as_ref()?.machine != n.machine {
        return None;
    }
    let row = super::availability::record(&n.config, &n.epoch, &team.id, member).ok()??;
    let name = row["name"].as_str()?;
    let run = row["run_id"].as_str()?;
    let conversation = row["conversation_id"].as_str()?;
    if name.is_empty() || name.len() > 256 || run.is_empty() || conversation.is_empty() {
        return None;
    }
    let activity = row["activity"].as_str()?;
    if !matches!(activity, "idle" | "busy" | "working") {
        return None;
    }
    let background = row["process_work"]["count"].as_u64()? > 0;
    let subagents = row["subagents_active"].as_u64()?;
    let questions = row["optional_questions"]["count"].as_u64()?;
    let working = activity != "idle" || subagents > 0;
    let phase = row["phase"].as_str()?;
    let pending = questions > 0 || (phase != "idle" && !working);
    if !super::availability::idle(&row) && !working && !background && !pending {
        return None;
    }
    Some(timers::Presence {
        name: name.into(),
        run: run.into(),
        conversation: conversation.into(),
        live: row["state"] == "running",
        working,
        background,
        pending,
    })
}
impl Runtime {
    pub(super) fn team_entry(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        now: i64,
    ) -> Result<()> {
        let stamp = if team.status == TeamStatus::Paused {
            "paused"
        } else {
            "running"
        };
        if !self.due(store, team, "timers:team", stamp, now)? {
            return Ok(());
        }
        match ports.team_timers(&mut self.auxiliary, store, team, now) {
            Ok(()) => self.recovered(store, team, "timers:team", now),
            Err(error) => self.failure(store, team, "timers:team", stamp, &error, now),
        }
    }
    pub(super) fn answers_entry(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        team: &Team,
        now: i64,
    ) -> Result<()> {
        let stamp = if team.status == TeamStatus::Paused {
            "paused"
        } else {
            "running"
        };
        if !self.due(store, team, "timers:answers", stamp, now)? {
            return Ok(());
        }
        match ports.answer_timers(&mut self.auxiliary, store, team, now) {
            Ok(()) => self.recovered(store, team, "timers:answers", now),
            Err(error) => self.failure(store, team, "timers:answers", stamp, &error, now),
        }
    }
    /// Each team whose key sweep reached its end since the last pass: its
    /// service item says how many keys got no final answer, counting the
    /// sweeps that left some, or goes once a sweep leaves none.
    pub(super) fn sweep_items(&mut self, now: i64) -> Result<()> {
        for done in self.auxiliary.finished_sweeps() {
            let Some((node, why)) = done.last.as_ref().filter(|_| done.unanswered > 0) else {
                self.ledger.clear_item(&done.team, KEY_CLEAR, "")?;
                continue;
            };
            let failures = self
                .ledger
                .item_failures(&done.team, KEY_CLEAR, "")?
                .unwrap_or(0)
                .saturating_add(1);
            let reason = format!(
                "{} task keys got no final answer on the last sweep; the last: {why}",
                done.unanswered
            );
            self.ledger.put_item(&items::Failure {
                team: &done.team,
                kind: KEY_CLEAR,
                subject: "",
                node: Some(*node),
                reason: &reason,
                failures,
                now,
                // The sweep comes back on a later terminal pass, at the
                // latest after the terminal page's longest backoff.
                next_ms: Some(now.saturating_add(super::retry::delay(u32::MAX))),
            })?;
        }
        Ok(())
    }
    pub(super) fn terminal_entry(
        &mut self,
        store: &mut Store,
        ports: &mut impl Ports,
        now: i64,
    ) -> Result<()> {
        if ports.stopped()
            || self
                .ledger
                .terminal_retry()?
                .is_some_and(|r| now < r.next_ms)
        {
            return Ok(());
        }
        let cancelled = self.terminal_cancels(store, ports, now);
        let timers = ports.terminal_timers(&mut self.auxiliary, store, now);
        let swept = self.sweep_items(now);
        match cancelled.and(timers).and(swept) {
            Ok(()) => self.ledger.terminal_succeeded(),
            Err(error) => {
                eprintln!("hgs: Teams terminal page will retry: {}", error.code);
                self.ledger.terminal_failed(now, &error.message)
            }
        }
    }
}
