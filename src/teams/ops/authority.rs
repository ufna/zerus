use super::*;

/// Operation policy is centralized here. Service facts never inherit Person
/// authority. Native caller and exact persisted session identity are verified
/// separately, before operation authority or a saved receipt is consulted.
pub fn authority(actor: &Actor, op: &Operation, team: &Team) -> Result<()> {
    let denied = || {
        Problem::new(
            code::NOT_ALLOWED,
            "This actor cannot perform this team operation.",
        )
    };
    if actor.person() != team.owner {
        return Err(denied());
    }
    let service = matches!(actor, Actor::Service { team: id, .. } if id == &team.id);
    let person = matches!(actor, Actor::Person { .. });
    let lead = matches!(actor, Actor::Session { team: id, member, .. } if id == &team.id && member == &team.lead);
    let member = matches!(actor, Actor::Session { team: id, member, .. } if id == &team.id && member != &team.lead);
    let allowed = match op {
        Operation::Verdict { .. } => member,
        Operation::CloseCard { .. } => service,
        Operation::ObserveTask { .. }
        | Operation::HandoutIntent { .. }
        | Operation::HandoutOutcome { .. }
        | Operation::ReopenIntent { .. }
        | Operation::ReopenOutcome { .. }
        | Operation::SetMemberSession { .. }
        | Operation::SetMemberHandle { .. }
        | Operation::SetMemberState { .. }
        | Operation::RecordReport { .. }
        | Operation::StageTaskIntent { .. }
        | Operation::StageTaskOutcome { .. }
        | Operation::RecordIntegration { .. }
        | Operation::RecordIntegrationConflict { .. }
        | Operation::FireTimer { .. }
        | Operation::RecordKeyClear { .. }
        | Operation::RecordLateSession { .. }
        | Operation::RecordLateReport { .. }
        | Operation::RecordCancel { .. }
        | Operation::ReconcileUnstaffed
        | Operation::EndStaleUnstaffed
        | Operation::SettleLeftHandout { .. } => service,
        Operation::Create { .. }
        | Operation::Stop
        | Operation::Archive
        | Operation::StopMember { .. }
        | Operation::ArchiveMember { .. }
        | Operation::ChangeSettings { .. }
        | Operation::DecideCard { .. } => person,
        Operation::AddTask { .. } => true,
        Operation::Ask { .. } => member || person,
        Operation::Accept { .. } => person || (lead && team.settings.delegations.accept_results),
        Operation::Reply { .. } => person || (lead && team.settings.delegations.answer_members),
        Operation::OpenCard { .. } => person || lead || service,
        Operation::Pause
        | Operation::Resume
        | Operation::PauseMember { .. }
        | Operation::ResumeMember { .. }
        | Operation::Admit { .. }
        | Operation::Route { .. }
        | Operation::Hold { .. }
        | Operation::Release { .. }
        | Operation::Return { .. }
        | Operation::Reassign { .. }
        | Operation::Cancel { .. } => person || lead,
    };
    if allowed {
        Ok(())
    } else {
        Err(denied())
    }
}
