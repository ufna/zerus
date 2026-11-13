//! zerus-63rru.32: a stopped or archived team clears its key from every
//! task it bound, closed ones included, in bounded background pages.
use super::*;
use crate::teams::ops::{self, Operation};
use zerus_replica::{Actor as TaskActor, Position, ProjectStore, Stage, Target};

fn key_of(h: &Harness, uuid: &str) -> Option<zerus_replica::TaskTeam> {
    let tasks = ProjectStore::open(&h.temp.path().join("replica"), "example-project").unwrap();
    tasks
        .load(uuid)
        .unwrap()
        .view()
        .unwrap()
        .team
}
fn queue(h: &mut Harness, uuid: &str) -> u32 {
    let node = h
        .run(
            &person(),
            Operation::AddTask {
                task: TaskRef {
                    project: "example-project".into(),
                    uuid: uuid.to_string(),
                },
                role: None,
                pipeline: None,
            },
        )
        .unwrap()
        .node
        .unwrap();
    h.observe(node, crate::teams::ops::TaskStage::Todo);
    node
}
fn bind(h: &Harness, n: u32, team: &str, home: &str) -> String {
    let tasks = ProjectStore::open(&h.temp.path().join("replica"), "example-project").unwrap();
    let (mut doc, _) = tasks
        .new_named_task(
            &format!("Task {n}"),
            "Body",
            &[],
            Stage::Todo,
            Target::Any,
            TaskActor::Person,
            &Position::End,
            "example",
            1,
            None,
            false,
        )
        .unwrap();
    doc.set_team_named(
        Some(&zerus_replica::TaskTeam {
            team: team.into(),
            home: home.into(),
        }),
        "Example team",
        &TaskActor::Person,
        1,
    )
    .unwrap();
    let uuid = doc.uuid().to_string();
    tasks.save(&mut doc).unwrap();
    uuid
}
/// The real writer pair for a closed node: the cancel decision, the recorded
/// answer and the observation that ends the node as Cancelled.
fn close_cancelled(h: &mut Harness, uuid: &str, node: u32) {
    {
        let tasks = ProjectStore::open(&h.temp.path().join("replica"), "example-project").unwrap();
        let mut doc = tasks.load(uuid).unwrap();
        let home = h.store.team(TEAM).unwrap().unwrap().home;
        doc.record_send(
            &zerus_replica::SessionKey {
                machine: home,
                session: format!("codex/task-{node}"),
                session_id: String::new(),
            },
            "request-1",
            &TaskActor::Person,
            2,
        )
        .unwrap();
        doc.record_cancel("Superseded", &TaskActor::Person, 3).unwrap();
        tasks.save(&mut doc).unwrap();
    }
    let lead = if h.store.member(TEAM, "lead").unwrap().unwrap().session.is_none() {
        h.bind("lead")
    } else {
        let m = h.store.member(TEAM, "lead").unwrap().unwrap();
        ops::Actor::Session {
            person: "local".into(),
            team: TEAM.into(),
            member: "lead".into(),
            session: m.session.unwrap(),
        }
    };
    h.run(
        &lead,
        Operation::Cancel {
            node,
            reason: "Superseded".into(),
        },
    )
    .unwrap();
    // In a live team the task's end is recorded by its observation; the
    // pending cancel settles with the node.
    crate::teams::tasks::observe(
        h.temp.path(),
        &TaskRef {
            project: "example-project".into(),
            uuid: uuid.to_string(),
        },
    )
    .unwrap();
}
fn passes(h: &mut Harness, a: &mut super::super::Runtime, n: usize) {
    for i in 0..n {
        a.begin_pass();
        let teams = h.store.teams(200, None).unwrap().rows;
        let mut ports = super::core::Fake::new();
        a.page(&mut h.store, &mut ports, &teams, 10_000 + i as i64, false)
            .unwrap();
    }
}
fn keyed(h: &Harness) -> usize {
    let tasks = ProjectStore::open(&h.temp.path().join("replica"), "example-project").unwrap();
    let (docs, _) = tasks.listing().unwrap();
    docs.iter()
        .filter(|doc| doc.view().unwrap().team.is_some())
        .count()
}

#[test]
fn a_stopped_team_clears_its_key_from_closed_tasks_in_bounded_pages() {
    let mut h = Harness::new();
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    let uuids: Vec<String> = (1..=3)
        .map(|n| {
            let uuid = bind(&h, n, TEAM, &home);
            let node = queue(&mut h, &uuid);
            close_cancelled(&mut h, &uuid, node);
            uuid
        })
        .collect();
    h.run(&person(), Operation::Stop).unwrap();
    passes(&mut h, &mut a, 6);
    for uuid in uuids {
        assert_eq!(key_of(&h, &uuid), None, "the key stayed on a closed task");
    }
}

#[test]
fn more_than_two_hundred_closed_tasks_need_more_than_one_pass() {
    let mut h = Harness::new();
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    for n in 1..=220u32 {
        let uuid = bind(&h, n, TEAM, &home);
        let node = queue(&mut h, &uuid);
        close_cancelled(&mut h, &uuid, node);
    }
    h.run(&person(), Operation::Stop).unwrap();
    passes(&mut h, &mut a, 1);
    // One pass clears at most one page (200), and the cursor is stored for
    // the restart.
    let remaining = keyed(&h);
    assert!(
        (1..=20).contains(&remaining),
        "one pass cleared nothing or everything: {remaining}"
    );
    let cursor: Option<(String, String, bool)> = a.load(TEAM, "key_clear_cursor").unwrap();
    assert!(cursor.is_some(), "the cursor is not stored");
    passes(&mut h, &mut a, 3);
    assert_eq!(keyed(&h), 0, "a key stayed after enough passes");
}

#[test]
fn a_restart_in_the_middle_resumes_the_sweep() {
    let mut h = Harness::new();
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    for n in 1..=60u32 {
        let uuid = bind(&h, n, TEAM, &home);
        let node = queue(&mut h, &uuid);
        close_cancelled(&mut h, &uuid, node);
    }
    h.run(&person(), Operation::Stop).unwrap();
    passes(&mut h, &mut a, 1);
    drop(a);
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    passes(&mut h, &mut a, 2);
    assert_eq!(keyed(&h), 0, "the resumed sweep left a key");
}

#[test]
fn a_task_whose_key_names_another_team_is_untouched() {
    let mut h = Harness::new();
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    let uuid = bind(&h, 1, TEAM, &home);
    let node = queue(&mut h, &uuid);
    close_cancelled(&mut h, &uuid, node);
    {
        let tasks = ProjectStore::open(&h.temp.path().join("replica"), "example-project").unwrap();
        let mut doc = tasks.load(&uuid).unwrap();
        doc.set_team_named(
            Some(&zerus_replica::TaskTeam {
                team: "00000000-0000-4000-8000-0000000000cc".into(),
                home: "00000000-0000-4000-8000-0000000000cc".into(),
            }),
            "Other team",
            &TaskActor::Person,
            3,
        )
        .unwrap();
        tasks.save(&mut doc).unwrap();
    }
    h.run(&person(), Operation::Stop).unwrap();
    passes(&mut h, &mut a, 6);
    let key = key_of(&h, &uuid).unwrap();
    assert_eq!(key.team, "00000000-0000-4000-8000-0000000000cc", "{key:?}");
}

#[test]
fn the_other_computers_guard_no_longer_refuses_a_done_task_after_the_clear() {
    let mut h = Harness::new();
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    let uuid = bind(&h, 1, TEAM, &home);
    let node = queue(&mut h, &uuid);
    close_cancelled(&mut h, &uuid, node);
    h.run(&person(), Operation::Stop).unwrap();
    passes(&mut h, &mut a, 6);
    let tasks = ProjectStore::open(&h.temp.path().join("replica"), "example-project").unwrap();
    let doc = tasks.load(&uuid).unwrap();
    assert!(
        !crate::teams::tasks::guarded(h.temp.path(), &home, &doc).unwrap(),
        "the cleared task is still guarded"
    );
}
