use super::*;
use crate::teams::ops::{KeyClearState, StageTaskOutcome};
impl Runtime {
    pub(super) fn settle_command(
        &mut self,
        store: &mut Store,
        team: &Team,
        key: &str,
        row: &mut Command,
        now: i64,
    ) -> Result<()> {
        if row.receipt.is_none() {
            let operation = operation(row)?;
            row.receipt = Some(request(store, &team.id, operation)?.1);
            self.put(&team.id, key, row)?;
        }
        let actor = Actor::Service {
            person: team.owner.clone(),
            team: team.id.clone(),
        };
        match ops::apply(
            store,
            &actor,
            row.receipt.as_ref().expect("reserved receipt"),
            now,
        ) {
            Ok(_) => {
                boundary("recorded");
                self.command_settled(store, team, key, now)
            }
            Err(error) if error.code == code::STALE_REVISION => {
                row.receipt = None;
                self.put(&team.id, key, row).map(|_| ())
            }
            Err(error) => Err(error),
        }
    }
}
fn operation(row: &Command) -> Result<Operation> {
    let reply = row
        .reply
        .as_ref()
        .ok_or_else(|| problem("Task command has no known outcome"))?;
    if row.creation {
        let outcome = match reply {
            TaskReply::Created => StageTaskOutcome::Created {
                task: row.task.clone(),
            },
            TaskReply::Refused { reason } => StageTaskOutcome::Refused {
                reason: cut(reason, 2048),
            },
            _ => return Err(problem("Creation has no terminal outcome")),
        };
        Ok(Operation::StageTaskOutcome {
            effect_request_id: row.task.uuid.clone(),
            outcome,
        })
    } else {
        let (state, reason) = match reply {
            TaskReply::Cleared => (KeyClearState::Cleared, String::new()),
            TaskReply::Refused { reason } => (KeyClearState::Refused, cut(reason, 2048)),
            _ => return Err(problem("Clearing has no terminal outcome")),
        };
        Ok(Operation::RecordKeyClear {
            node: row.node,
            state,
            reason,
        })
    }
}
