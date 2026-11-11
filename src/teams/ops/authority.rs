use super::*;

/// The only operation policy. Actors have already passed the native caller
/// boundary; apply also rechecks a session's exact persisted binding.
pub fn authority(actor: &Actor, operation: &Operation, team: &Team) -> Result<(), Problem> {
    let denied = || {
        Problem::new(
            code::NOT_ALLOWED,
            "This actor is not allowed to perform this team operation.",
        )
    };
    if actor.person() != team.owner {
        return Err(denied());
    }
    if matches!(actor, Actor::Person { .. }) {
        return Ok(());
    }
    let lead =
        matches!(actor,Actor::Session{team:id,member,..} if id==&team.id && member==&team.lead);
    let member = matches!(actor,Actor::Session{team:id,..} if id==&team.id);
    let service = matches!(actor,Actor::Service{team:id,..} if id==&team.id);
    let allowed = match operation {
        Operation::Create { .. }
        | Operation::Stop
        | Operation::Archive
        | Operation::StopMember { .. }
        | Operation::ArchiveMember { .. }
        | Operation::ChangeSettings { .. }
        | Operation::DecideCard { .. } => false,
        Operation::SetMemberSession { .. }
        | Operation::SetMemberState { .. }
        | Operation::HandoutOutcome { .. }
        | Operation::ObserveTask { .. } => service,
        Operation::HandoutIntent { .. } | Operation::OpenCard { .. } => lead || service,
        Operation::RecordReport { .. } => member || service,
        Operation::Ask { .. } => member && !lead,
        Operation::AddTask { .. } => true, // Other sessions submit for admission only.
        Operation::Accept {
            integration_commit, ..
        } => {
            lead && team.settings.delegations.accept_results
                && (integration_commit.is_none() || team.settings.delegations.merge_accepted)
        }
        Operation::Reply { .. } => lead && team.settings.delegations.answer_members,
        Operation::Pause
        | Operation::Resume
        | Operation::PauseMember { .. }
        | Operation::ResumeMember { .. }
        | Operation::Admit { .. }
        | Operation::Route { .. }
        | Operation::Hold { .. }
        | Operation::Release { .. }
        | Operation::Cancel { .. }
        | Operation::Return { .. }
        | Operation::Reassign { .. } => lead,
    };
    if allowed {
        Ok(())
    } else {
        Err(denied())
    }
}
