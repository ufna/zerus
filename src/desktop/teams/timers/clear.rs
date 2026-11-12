//! After Stop or Archive the service clears the team key of each task the
//! team still marks, one small page per pass. Cleared and refused are final;
//! cutting over and no answer are tried on a later sweep, and the page moves
//! past them so later nodes never wait behind them.
use super::{
    ports::{Answer, ClearRequest, Effects, Result},
    record,
};
use crate::teams::{
    ops::{KeyClearState, Operation},
    store::Store,
};
use zerus_team_plan::model::TeamStatus;

pub(super) const PAGE: u32 = 16;

#[derive(Default)]
pub(super) struct Sweep {
    team: Option<String>,
    after_team: String,
    after_node: u32,
}
impl Sweep {
    fn current(&mut self, store: &Store) -> Result<Option<String>> {
        if self.team.is_none() {
            let mut next = store.keyed_teams(&self.after_team, 1)?.into_iter().next();
            if next.is_none() && !self.after_team.is_empty() {
                // Wrap to the first pending team in the same pass.
                next = store.keyed_teams("", 1)?.into_iter().next();
            }
            self.after_team = next.clone().unwrap_or_default();
            self.team = next;
            self.after_node = 0;
        }
        Ok(self.team.clone())
    }
    fn next_team(&mut self) {
        self.team = None;
        self.after_node = 0;
    }
}

pub(super) fn step(
    store: &mut Store,
    effects: &mut impl Effects,
    s: &mut Sweep,
    now: i64,
) -> Result<()> {
    let Some(id) = s.current(store)? else {
        return Ok(());
    };
    let team = store.team(&id)?;
    let Some(team) =
        team.filter(|t| matches!(t.status, TeamStatus::Stopped | TeamStatus::Archived))
    else {
        s.next_team();
        return Ok(());
    };
    let page = store.keyed_tasks(&id, s.after_node, PAGE)?;
    for keyed in &page {
        s.after_node = keyed.node;
        let request = ClearRequest {
            schema: 1,
            team: id.clone(),
            node: keyed.node,
        };
        let (state, reason) = match effects.clear(&keyed.task, &request) {
            Answer::Cleared => (KeyClearState::Cleared, String::new()),
            Answer::Refused(why) => (KeyClearState::Refused, refusal(&why)),
            Answer::CuttingOver | Answer::None(_) => continue,
        };
        let op = Operation::RecordKeyClear {
            node: keyed.node,
            state,
            reason,
        };
        record(store, &team, op, now)?;
    }
    if page.len() < PAGE as usize {
        store.settle_keyed(&id)?;
        s.next_team();
    }
    Ok(())
}
fn refusal(why: &str) -> String {
    let why = why.trim();
    if why.is_empty() {
        return "The task command refused without a reason.".into();
    }
    let mut end = why.len().min(4096);
    while !why.is_char_boundary(end) {
        end -= 1;
    }
    why[..end].into()
}
