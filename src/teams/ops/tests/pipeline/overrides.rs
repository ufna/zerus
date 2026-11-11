use super::*;

#[test]
fn a_person_can_override_a_stage_but_the_lead_cannot_accept_before_it() {
    let (mut h, parent, stage) = wanted();
    let lead = h.bind("lead");
    refused(
        h.run(&lead, Operation::Accept { node: parent }),
        code::INVALID_STATE,
    );
    h.run(&person(), Operation::Accept { node: parent })
        .unwrap();
    assert_eq!(h.node(parent).decided_by, Some(Decider::Person));
    assert!(!h
        .store
        .queue(TEAM, 200, None)
        .unwrap()
        .rows
        .iter()
        .any(|n| n.id == stage));
}
#[test]
fn a_person_cancelling_a_stage_sends_its_parent_to_the_lead() {
    let (mut h, parent, _) = wanted();
    let stage = stage_attempt(&mut h, parent, "m3");
    h.cancel_fact(stage);
    assert!(!h.node(parent).hold);
    assert!(h
        .store
        .inbox(TEAM, 200, None)
        .unwrap()
        .rows
        .iter()
        .any(|i| i.node == parent && i.text.contains("stage")));
}

#[test]
fn a_late_observation_never_restores_a_retired_pipeline_stage() {
    for delivered in [false, true] {
        let (mut h, parent, stage) = wanted();
        if delivered {
            stage_attempt(&mut h, parent, "m3");
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            h.run(&service(), creation(stage, &id)).unwrap();
            h.run(&service(), created(&id, 1000 + stage)).unwrap();
        }
        h.observe(parent, TaskStage::Todo);
        h.observe(stage, TaskStage::InProgress);
        assert_eq!(
            h.store
                .queue(TEAM, 200, None)
                .unwrap()
                .rows
                .iter()
                .map(|n| n.id)
                .collect::<Vec<_>>(),
            vec![parent]
        );
        let bound: bool = h
            .store
            .db
            .as_ref()
            .unwrap()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM live_tasks WHERE team=?1 AND node=?2)",
                params![TEAM, stage],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!bound);
        assert_eq!(
            h.store
                .task_observation(TEAM, stage)
                .unwrap()
                .unwrap()
                .stage,
            TaskStage::InProgress
        );
    }
}
