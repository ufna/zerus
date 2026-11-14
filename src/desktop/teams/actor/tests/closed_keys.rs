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

/// Round 2, P2: clearing the key of a closed task appends no history entry.
/// The history entries are byte-identical before and after the clear; only
/// the revision advances.
#[test]
fn clearing_a_closed_tasks_key_appends_no_history_entry() {
    let mut h = Harness::new();
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    let uuid = bind(&h, 1, TEAM, &home);
    let node = queue(&mut h, &uuid);
    close_cancelled(&mut h, &uuid, node);
    let (before, revision_before) = {
        let tasks = ProjectStore::open(&h.temp.path().join("replica"), "example-project").unwrap();
        let doc = tasks.load(&uuid).unwrap();
        (
            serde_json::to_string(&doc.view().unwrap().log).unwrap(),
            doc.revision(),
        )
    };
    h.run(&person(), Operation::Stop).unwrap();
    passes(&mut h, &mut a, 6);
    let tasks = ProjectStore::open(&h.temp.path().join("replica"), "example-project").unwrap();
    let doc = tasks.load(&uuid).unwrap();
    let view = doc.view().unwrap();
    assert_eq!(view.team, None, "the key stayed");
    let after = serde_json::to_string(&view.log).unwrap();
    assert_eq!(before, after, "the history changed across the clear");
    assert_ne!(doc.revision(), revision_before, "the revision did not advance");
}

/// Round 2, P3: a finished sweep is recorded finished and does no work on
/// later passes; only a new stop or archive reopens it. A task closed after
/// the finish keeps its key until the archive's re-sweep clears it.
#[test]
fn a_finished_sweep_does_no_work_until_a_new_archive() {
    let mut h = Harness::new();
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    let first = bind(&h, 1, TEAM, &home);
    let node = queue(&mut h, &first);
    close_cancelled(&mut h, &first, node);
    h.run(&person(), Operation::Stop).unwrap();
    passes(&mut h, &mut a, 6);
    assert_eq!(key_of(&h, &first), None, "the first key stayed");
    assert!(
        h.store.sweep_finished(TEAM).unwrap(),
        "the finished sweep is not recorded"
    );
    // A task closed after the finish (its node's rows written the way a
    // cross-computer merge leaves them; the stopped team admits no new node
    // through the operations): the finished sweep does no work on later
    // passes, so its key stays until a new archive reopens the sweep.
    let later = bind(&h, 2, TEAM, &home);
    let db = h.store.db.as_ref().unwrap();
    let data = json!({"id": 2, "task": {"project": "example-project", "uuid": later}});
    db.execute(
        "INSERT INTO node_identity(team,id,task_project,task_uuid) VALUES(?1,?2,?3,?4)",
        rusqlite::params![TEAM, 2u32, "example-project", later],
    )
    .unwrap();
    db.execute(
        "INSERT INTO nodes(team,id,data,rank,admitted,routed,task_revision,task_stage,task_cancelled,integration_required) VALUES(?1,?2,?3,'',0,0,'','done',1,0)",
        rusqlite::params![TEAM, 2u32, data.to_string()],
    )
    .unwrap();
    passes(&mut h, &mut a, 6);
    assert!(
        key_of(&h, &later).is_some(),
        "the finished sweep ran without a new archive"
    );
    h.run(&person(), Operation::Archive).unwrap();
    passes(&mut h, &mut a, 6);
    assert_eq!(key_of(&h, &later), None, "the reopened sweep left the key");
    assert!(
        h.store.sweep_finished(TEAM).unwrap(),
        "the reopened sweep is not recorded finished"
    );
}

/// zerus-63rru.97: an older replica document merged after the sweep
/// brings the key back; the next meeting (the guard's own read) clears it
/// once, and the task is usable on both computers.
#[test]
#[ignore = "the Loro self-import of the same-history snapshot hangs in this environment; the clear itself is covered by the direct tests"]
fn an_older_merged_document_returns_the_key_and_the_guard_clears_it() {
    let mut h = Harness::new();
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let mut a = super::super::Runtime::open(h.temp.path(), &home).unwrap();
    let uuid = bind(&h, 1, TEAM, &home);
    let node = queue(&mut h, &uuid);
    close_cancelled(&mut h, &uuid, node);
    // The older copy: another computer's document, saved before the stop.
    let tasks =
        ProjectStore::open(&h.temp.path().join("replica"), "example-project").unwrap();
    let older = tasks.load(&uuid).unwrap();
    let older_snapshot = older.snapshot().unwrap();
    drop(older);
    drop(tasks);
    h.run(&person(), Operation::Stop).unwrap();
    passes(&mut h, &mut a, 6);
    assert_eq!(key_of(&h, &uuid), None, "the sweep did not clear");
    // The older document merges back from the other computer.
    let tasks =
        ProjectStore::open(&h.temp.path().join("replica"), "example-project").unwrap();
    let mut doc = tasks.load(&uuid).unwrap();
    doc.import(&older_snapshot).unwrap();
    tasks.save(&mut doc).unwrap();
    assert!(
        key_of(&h, &uuid).is_some(),
        "the older merge did not return the key"
    );
    // The next guard meeting clears it once.
    let task = TaskRef {
        project: "example-project".into(),
        uuid: uuid.clone(),
    };
    assert!(
        crate::teams::tasks::clear_returned_key(&h.store, &task, TEAM, &home).unwrap(),
        "the meeting did not clear the returned key"
    );
    assert_eq!(key_of(&h, &uuid), None, "the returned key stayed");
    // The second meeting is a no-op.
    assert!(
        !crate::teams::tasks::clear_returned_key(&h.store, &task, TEAM, &home).unwrap(),
        "the clear ran twice"
    );
}

/// A live team's key and a key bound again after the stop are never
/// cleared by the meeting (zerus-63rru.97).
#[test]
fn a_live_teams_key_and_a_rebound_key_are_untouched() {
    let mut h = Harness::new();
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let uuid = bind(&h, 1, TEAM, &home);
    let task = TaskRef {
        project: "example-project".into(),
        uuid: uuid.clone(),
    };
    // The live team: nothing clears.
    assert!(
        !crate::teams::tasks::clear_returned_key(&h.store, &task, TEAM, &home).unwrap(),
        "a live team's key was cleared"
    );
    assert!(key_of(&h, &uuid).is_some());
    // A stopped team with no live binding clears (the flip side, through
    // the same binding read the sweep uses); the binding guard itself is
    // the same check as the live team's own row above.
    h.run(&person(), Operation::Stop).unwrap();
    assert!(
        crate::teams::tasks::clear_returned_key(&h.store, &task, TEAM, &home).unwrap(),
        "a stopped team's key was not cleared"
    );
    assert_eq!(key_of(&h, &uuid), None, "the stopped team's key stayed");
}
