use super::*;
#[test]
fn lead_pause_resume_and_person_stop_archive_have_state_guards() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    let node = h.add(&person(), 1);
    refused(h.run(&person(), Operation::Archive), code::INVALID_STATE);
    h.run(&lead, Operation::Pause).unwrap();
    refused(h.run(&lead, Operation::Pause), code::INVALID_STATE);
    h.run(&lead, Operation::Resume).unwrap();
    h.run(&person(), Operation::Stop).unwrap();
    assert!(h.node(node).hold);
    assert_eq!(h.node(node).state, NodeState::Pending);
    refused(h.run(&person(), Operation::Stop), code::INVALID_STATE);
    h.run(&person(), Operation::Archive).unwrap();
    assert_eq!(
        h.store.team(TEAM).unwrap().unwrap().status,
        TeamStatus::Archived
    );
}
#[test]
fn lead_cannot_resume_a_team_the_person_paused() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    h.run(&person(), Operation::Pause).unwrap();
    refused(h.run(&lead, Operation::Resume), code::INVALID_STATE);
    h.run(&person(), Operation::Resume).unwrap();
}
#[test]
fn branch_must_be_nonempty_and_not_used_by_another_live_team() {
    let mut h = Harness::empty();
    let mut op = create();
    if let Operation::Create {
        integration_branch, ..
    } = &mut op
    {
        integration_branch.clear();
    }
    refused(h.run(&person(), op), code::INVALID_STATE);
    assert!(h.store.db.is_none());
    h.run(&person(), create()).unwrap();
    refused(h.run_for(TEAM_B, &person(), create()), code::INVALID_STATE);
    h.run(&person(), Operation::Stop).unwrap();
    h.run_for(TEAM_B, &person(), create()).unwrap();
}
#[test]
fn settings_validate_phase_acceptance_and_admitted_caps() {
    let mut h = Harness::new();
    h.add(&person(), 1);
    let settings = Settings {
        phase_acceptance: Acceptance::Pipeline,
        ..Settings::default()
    };
    refused(
        h.run(&person(), Operation::ChangeSettings { settings }),
        code::INVALID_STATE,
    );
    h.settings(|s| s.caps.max_open_tasks = 1);
    refused(
        h.run(
            &person(),
            Operation::AddTask {
                task: task(2),
                role: None,
                pipeline: None,
            },
        ),
        code::OVER_CAP,
    );
}
#[test]
fn preview_revokes_existing_store_handles() {
    let mut h = Harness::new();
    std::fs::remove_file(h.temp.path().join("teams/preview")).unwrap();
    refused(h.store.team(TEAM), code::TEAMS_INACTIVE);
    refused(h.run(&person(), Operation::Pause), code::TEAMS_INACTIVE);
}
