use super::*;
use crate::teams::ops::{
    self,
    tests::{fixture, person, Harness, TEAM},
};
use zerus_replica::{Actor as TaskActor, Position, ProjectStore, Stage, Target, TaskDoc};
use zerus_team_plan::model::{NodeState, TaskRef};

fn caller<'a>(h: &'a Harness, actor: &'a TaskActor) -> Caller<'a> {
    Caller {
        state: h.temp.path(),
        machine: "00000000-0000-4000-8000-0000000000aa",
        actor,
    }
}

fn document(h: &Harness) -> TaskRef {
    let store = ProjectStore::open(&h.temp.path().join("replica"), "example-project").unwrap();
    let mut task = store
        .new_task(
            "Implement the change",
            Stage::Todo,
            Target::Any,
            TaskActor::Person,
            &Position::End,
            "example",
            1,
        )
        .unwrap();
    let task_ref = TaskRef {
        project: "example-project".into(),
        uuid: task.uuid().into(),
    };
    store.save(&mut task).unwrap();
    task_ref
}

fn task(h: &Harness, reference: &TaskRef) -> TaskDoc {
    ProjectStore::open(&h.temp.path().join("replica"), &reference.project)
        .unwrap()
        .load(&reference.uuid)
        .unwrap()
}

fn agent(actor: ops::Actor) -> TaskActor {
    let ops::Actor::Session { session, .. } = actor else {
        panic!("bound actor")
    };
    TaskActor::Agent(zerus_replica::SessionKey {
        machine: session.machine,
        session_id: session.session_id,
        session: "codex/renamed-label".into(),
    })
}

#[test]
fn sends_twice_share_one_node_and_service_reads_the_document() {
    let h = Harness::new();
    let reference = document(&h);
    let first = send(
        &caller(&h, &TaskActor::Person),
        &reference,
        TEAM,
        None,
        None,
    )
    .unwrap();
    let second = send(
        &caller(&h, &TaskActor::Person),
        &reference,
        TEAM,
        None,
        None,
    )
    .unwrap();
    assert_eq!(first.node, second.node);
    let store = super::super::store::Store::open(h.temp.path()).unwrap();
    let nodes = store.queue(TEAM, 200, None).unwrap().rows;
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].state, NodeState::Pending);
    let doc = task(&h, &reference);
    let observed = store.task_observation(TEAM, nodes[0].id).unwrap().unwrap();
    assert_eq!(observed.revision, doc.revision());
    assert_eq!(observed.rank, doc.rank());
    assert_eq!(
        observe(h.temp.path(), &reference.project, &reference.uuid)
            .unwrap()
            .revision,
        doc.revision()
    );
    assert_eq!(doc.view().unwrap().target, Target::Any);
    assert_eq!(doc.team().unwrap().unwrap().team, TEAM);
    let actors = store.journal(TEAM, 200, None).unwrap().rows;
    assert!(actors
        .iter()
        .any(|row| row.kind == "observe_task" && row.actor["kind"] == "service"));
    assert!(actors
        .iter()
        .any(|row| row.kind == "add_task" && row.actor["kind"] == "person"));
}

#[test]
fn admission_and_suggestions_use_the_verified_callers_binding() {
    let mut h = Harness::new();
    let lead = agent(h.bind("lead"));
    let member = agent(h.bind("m1"));
    let foreign = TaskActor::Agent(zerus_replica::SessionKey {
        machine: "00000000-0000-4000-8000-0000000000aa".into(),
        session_id: "run_id:outside".into(),
        session: "codex/other".into(),
    });
    for (actor, admitted) in [(&lead, true), (&member, false), (&foreign, false)] {
        let reference = document(&h);
        let reply = send(
            &caller(&h, actor),
            &reference,
            TEAM,
            Some("developer"),
            Some("reviewed"),
        )
        .unwrap();
        let store = super::super::store::Store::open(h.temp.path()).unwrap();
        let node = store.node(TEAM, reply.node.unwrap()).unwrap().unwrap();
        assert_eq!(node.admitted, admitted);
        assert_eq!(
            store.suggestion(TEAM, node.id).unwrap().is_some(),
            !admitted
        );
    }
}

#[test]
fn the_three_guards_allow_only_the_home_lead_or_the_person() {
    let mut h = Harness::new();
    let lead = agent(h.bind("lead"));
    let member = agent(h.bind("m1"));
    let reference = document(&h);
    send(
        &caller(&h, &TaskActor::Person),
        &reference,
        TEAM,
        None,
        None,
    )
    .unwrap();
    let doc = task(&h, &reference);
    for action in ["send", "handoff", "done"] {
        assert!(guard(&caller(&h, &member), &doc, action).is_err());
        assert!(guard(&caller(&h, &lead), &doc, action).is_ok());
        assert!(guard(&caller(&h, &TaskActor::Person), &doc, action).is_ok());
        let mut remote = caller(&h, &lead);
        remote.machine = "00000000-0000-4000-8000-0000000000cc";
        assert!(guard(&remote, &doc, action).is_err());
        let mut person = caller(&h, &TaskActor::Person);
        person.machine = remote.machine;
        assert!(guard(&person, &doc, action).is_ok());
    }
    // Display-name reuse grants nothing.
    let mut impostor = lead.clone();
    if let TaskActor::Agent(key) = &mut impostor {
        key.session_id = "run_id:replacement".into();
    }
    assert!(guard(&caller(&h, &impostor), &doc, "send").is_err());
}

#[test]
fn local_guards_work_without_the_replica_key_and_stopped_teams_release_them() {
    let mut h = Harness::new();
    let member = agent(h.bind("m1"));
    let reference = document(&h);
    h.run(
        &person(),
        ops::Operation::AddTask {
            task: reference.clone(),
            role: None,
            pipeline: None,
        },
    )
    .unwrap();
    let doc = task(&h, &reference);
    assert_eq!(doc.team().unwrap(), None);
    for action in ["send", "handoff", "done"] {
        assert!(guard(&caller(&h, &member), &doc, action).is_err());
    }
    h.run(&person(), ops::Operation::Stop).unwrap();
    assert!(guard(&caller(&h, &member), &doc, "send").is_ok());
}

#[test]
fn preview_off_does_not_write_intake_and_ordinary_tasks_stay_unchanged() {
    let h = Harness::new();
    let reference = document(&h);
    let before = task(&h, &reference).revision();
    std::fs::remove_file(h.temp.path().join("teams/preview")).unwrap();
    let error = send(
        &caller(&h, &TaskActor::Person),
        &reference,
        TEAM,
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, "teams_inactive");
    assert_eq!(task(&h, &reference).revision(), before);
    assert!(guard(
        &caller(&h, &TaskActor::Person),
        &task(&h, &reference),
        "send"
    )
    .is_ok());
}

#[test]
fn service_observation_records_a_persons_override_from_disk() {
    let mut h = Harness::new();
    h.settings(|s| s.integration = zerus_team_plan::model::Integration::None);
    let reference = document(&h);
    let reply = send(
        &caller(&h, &TaskActor::Person),
        &reference,
        TEAM,
        None,
        None,
    )
    .unwrap();
    {
        let store = ProjectStore::open(&h.temp.path().join("replica"), &reference.project).unwrap();
        let mut doc = store.load(&reference.uuid).unwrap();
        let session = zerus_replica::SessionKey {
            machine: fixture().team.home,
            session: "codex/example".into(),
            session_id: "run_id:example".into(),
        };
        doc.record_send(&session, "request", &TaskActor::Person, 2)
            .unwrap();
        doc.report(
            "Ready",
            zerus_replica::execution::ReportKind::Ready,
            None,
            &TaskActor::Person,
            3,
        )
        .unwrap();
        doc.set_stage(Stage::Done, &TaskActor::Person, 4).unwrap();
        store.save(&mut doc).unwrap();
    }
    changed(h.temp.path(), &reference).unwrap();
    let store = super::super::store::Store::open(h.temp.path()).unwrap();
    assert_eq!(
        store
            .node(TEAM, reply.node.unwrap())
            .unwrap()
            .unwrap()
            .state,
        NodeState::Accepted
    );
}
