use super::*;
#[test]
fn member_lifecycle_and_observed_state_have_guards() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    h.bind("m1");
    h.run(
        &lead,
        Operation::PauseMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    h.run(
        &lead,
        Operation::ResumeMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    h.run(&service(), observed_state(MemberState::Paused))
        .unwrap();
    refused(
        h.run(&service(), observed_state(MemberState::Paused)),
        code::INVALID_STATE,
    );
    h.run(&service(), observed_state(MemberState::Active))
        .unwrap();
    h.run(
        &person(),
        Operation::ArchiveMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    refused(
        h.run(
            &lead,
            Operation::ResumeMember {
                member: "m1".into(),
            },
        ),
        code::INVALID_STATE,
    );
}
#[test]
fn service_and_lead_cannot_reactivate_a_person_stopped_member() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    h.bind("m1");
    h.run(
        &person(),
        Operation::StopMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    refused(
        h.run(&service(), observed_state(MemberState::Active)),
        code::INVALID_STATE,
    );
    refused(
        h.run(
            &lead,
            Operation::ResumeMember {
                member: "m1".into(),
            },
        ),
        code::INVALID_STATE,
    );
    h.run(
        &person(),
        Operation::ResumeMember {
            member: "m1".into(),
        },
    )
    .unwrap();
}
#[test]
fn bindings_are_unique_immutable_and_use_identity_not_names() {
    let mut h = Harness::new();
    h.bind("m1");
    let m = fixture().members[1].clone();
    refused(
        h.run(
            &service(),
            Operation::SetMemberSession {
                member: "m1".into(),
                session: m.session.unwrap(),
                account: m.account,
                worktree: m.worktree,
                ownership_nonce: m.ownership_nonce,
            },
        ),
        code::INVALID_STATE,
    );
    let m = fixture().members[2].clone();
    let mut session = m.session.unwrap();
    session.session_id = "run_id:legacy-example".into();
    h.run(
        &service(),
        Operation::SetMemberSession {
            member: "m2".into(),
            session,
            account: m.account,
            worktree: m.worktree,
            ownership_nonce: m.ownership_nonce,
        },
    )
    .unwrap();
}

fn observed_state(state: MemberState) -> Operation {
    Operation::SetMemberState {
        member: "m1".into(),
        state,
    }
}
