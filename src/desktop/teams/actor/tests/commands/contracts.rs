use super::*;
use crate::teams::ops::{Operation, StageTaskOutcome};
pub(super) fn run() {
    delivery();
    finish();
    creation_and_clear();
}
fn delivery() {
    let mut f = Fixture::new();
    f.submitted();
    assert_eq!(f.inputs(), 1);
    let recovered = f.delivery(&f.id, "m1", true);
    assert_eq!(recovered["state"], "submitted");
    assert_eq!(f.inputs(), 1);
    f.reported();
    f.h.run(
        &person(),
        Operation::Return {
            node: f.node,
            reason: "Revise this work".into(),
        },
    )
    .unwrap();
    let id = f.h.reopen(f.node, "m1");
    let reopened = f.delivery(&id, "m1", false);
    assert_eq!(reopened["state"], "submitted");
    assert_eq!(f.inputs(), 2);
}
fn finish() {
    let mut f = Fixture::new();
    f.submitted();
    f.reported();
    f.h.run(&person(), Operation::Accept { node: f.node })
        .unwrap();
    let decision = f.h.accept_decision(f.node);
    f.h.run(
        &service(),
        Operation::RecordIntegration {
            node: f.node,
            decision,
            commit: COMMIT.into(),
        },
    )
    .unwrap();
    let team = f.h.store.team(TEAM).unwrap().unwrap();
    let node = f.h.node(f.node);
    let request = task_request(&team, &node, None, "unused");
    for _ in 0..2 {
        let reply = f.call(&task_arguments(&f.task, "team-done"), &request, false);
        assert_eq!(
            reply,
            json!({"schema":1,"team":TEAM,"node":f.node,"state":"done","error":""})
        );
    }
    let tasks = ProjectStore::open(&f.config.state.join("replica"), &f.task.project).unwrap();
    assert_eq!(
        tasks.load(&f.task.uuid).unwrap().view().unwrap().stage,
        Stage::Done
    );
}
fn creation_and_clear() {
    let mut f = Fixture::new();
    f.submitted();
    f.reported();
    let node = f.h.store.stage_tasks(TEAM, 1, None).unwrap().rows[0].node;
    let id = uuid::Uuid::new_v4().to_string();
    f.h.run(
        &service(),
        Operation::StageTaskIntent {
            node,
            effect_request_id: id.clone(),
        },
    )
    .unwrap();
    let effect =
        f.h.store
            .stage_task_effect(TEAM, node, &id)
            .unwrap()
            .unwrap();
    let request = stage_request(&effect);
    let crashed = f.process(
        &stage_arguments(),
        &request.to_string(),
        Some("stage-crash"),
    );
    assert_eq!(crashed.status.code(), Some(78));
    let task = TaskRef {
        project: f.task.project.clone(),
        uuid: id.clone(),
    };
    let before = {
        let tasks = ProjectStore::open(&f.config.state.join("replica"), &task.project).unwrap();
        tasks.load(&id).unwrap().revision()
    };
    let reply = f.call(&stage_arguments(), &request, false);
    assert_eq!(reply["state"], "created");
    assert_eq!(reply["id"], id);
    assert_eq!(reply["team"], TEAM);
    assert_eq!(reply["node"], node);
    assert_eq!(reply["error"], "");
    {
        let tasks = ProjectStore::open(&f.config.state.join("replica"), &task.project).unwrap();
        assert_eq!(tasks.load(&id).unwrap().revision(), before);
    }
    f.h.run(
        &service(),
        Operation::StageTaskOutcome {
            effect_request_id: id,
            outcome: StageTaskOutcome::Created { task: task.clone() },
        },
    )
    .unwrap();
    f.h.run(
        &person(),
        Operation::Return {
            node: f.node,
            reason: "Revise the task".into(),
        },
    )
    .unwrap();
    let team = f.h.store.team(TEAM).unwrap().unwrap();
    let request = clear_request(&team, node);
    {
        use crate::desktop::teams::timers::{Answer, ClearRequest, Effects, Native};
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut adapter = Native {
            config: &f.config,
            machine: &team.home,
            stop: &stop,
        };
        let answer = Native::fixture_program(&f.cli, || {
            adapter.clear(
                &task,
                &ClearRequest {
                    schema: 1,
                    team: TEAM.into(),
                    node,
                },
            )
        });
        assert_eq!(answer, Answer::Cleared);
        eprintln!("REAL_COMMAND package10 Native::clear => {answer:?}");
    }
    for _ in 0..2 {
        let reply = f.call(&task_arguments(&task, "team-clear"), &request, false);
        assert_eq!(
            reply,
            json!({"schema":1,"team":TEAM,"node":node,"state":"cleared","error":""})
        );
    }
    let tasks = ProjectStore::open(&f.config.state.join("replica"), &task.project).unwrap();
    assert!(tasks.load(&task.uuid).unwrap().team().unwrap().is_none());
}
