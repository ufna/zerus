use super::super::ports::Ports;
use super::*;
use zerus_replica::{Actor as TaskActor, Position, ProjectStore, SessionKey, Stage, Target};

fn reported_task(h: &mut Harness) -> (TaskRef, u32) {
    let store = ProjectStore::open(&h.temp.path().join("replica"), "example-project").unwrap();
    let mut doc = store
        .new_task(
            "Work",
            Stage::Todo,
            Target::Any,
            TaskActor::Person,
            &Position::End,
            "example",
            1,
        )
        .unwrap();
    let task = TaskRef {
        project: "example-project".into(),
        uuid: doc.uuid().into(),
    };
    store.save(&mut doc).unwrap();
    drop(store);
    let caller = crate::teams::tasks::Caller {
        state: h.temp.path(),
        machine: "00000000-0000-4000-8000-0000000000aa",
        actor: TaskActor::Person,
    };
    let node = crate::teams::tasks::send(&caller, &task, TEAM, None, None)
        .unwrap()
        .node;
    let store = ProjectStore::open(&h.temp.path().join("replica"), &task.project).unwrap();
    let mut doc = store.load(&task.uuid).unwrap();
    let holder = SessionKey {
        machine: "example-machine".into(),
        session_id: "launch_id:holder".into(),
        session: "codex/holder".into(),
    };
    doc.record_send(&holder, "dispatch", &TaskActor::Person, 2)
        .unwrap();
    doc.record_report(&"界".repeat(5000), &TaskActor::Agent(holder), 3)
        .unwrap();
    store.save(&mut doc).unwrap();
    (task, node.expect("bound node"))
}

#[test]
fn native_observe_uses_the_trusted_task_reader_and_records_its_current_revision() {
    let mut h = Harness::new();
    let (task, node) = reported_task(&mut h);
    let mut native = super::production::native(&h);
    native.observe(&mut h.store, &task).unwrap();
    let tasks = ProjectStore::open(&h.temp.path().join("replica"), &task.project).unwrap();
    let doc = tasks.load(&task.uuid).unwrap();
    let observation = h.store.task_observation(TEAM, node).unwrap().unwrap();
    assert_eq!(observation.revision, doc.revision());
    assert_eq!(observation.stage, crate::teams::ops::TaskStage::InReview);
}

#[test]
fn native_report_reads_the_holders_bounded_text_without_changing_the_document() {
    let mut h = Harness::new();
    let (task, _) = reported_task(&mut h);
    let before = {
        let tasks = ProjectStore::open(&h.temp.path().join("replica"), &task.project).unwrap();
        tasks.load(&task.uuid).unwrap().revision()
    };
    let text = super::production::native(&h)
        .report(&task)
        .unwrap()
        .unwrap();
    assert_eq!(text, "界".repeat(4000));
    let tasks = ProjectStore::open(&h.temp.path().join("replica"), &task.project).unwrap();
    assert_eq!(tasks.load(&task.uuid).unwrap().revision(), before);
}

pub(super) fn resumed(state: &std::path::Path, team: &str, member: &str, launch: &str, run: &str) {
    let dir = state.join("teams/standing-v1");
    crate::platform::private_dir(&dir).unwrap();
    let path = dir.join("journal.sqlite3");
    crate::sqlite_file::create(&path).unwrap();
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute_batch("CREATE TABLE IF NOT EXISTS standing_bindings(team TEXT,member TEXT,launch TEXT,parent TEXT,body TEXT,receipt_ms INTEGER,PRIMARY KEY(team,member,launch))").unwrap();
    let body = json!({"team":team,"member":member,"person":"local","machine":"machine","role":"developer","agent":"codex","account":"account","launch":launch,"session":"native-session","run":run,"conversation":"conversation","folder":"fixture-folder","common":"fixture-repo","nonce":"nonce"});
    db.execute(
        "INSERT OR REPLACE INTO standing_bindings VALUES(?1,?2,?3,'completed',?4,100)",
        rusqlite::params![team, member, launch, body.to_string()],
    )
    .unwrap();
}

#[test]
fn current_run_prefers_the_completed_resume_and_does_not_infer_liveness() {
    let h = Harness::new();
    let native = super::production::native(&h);
    let mut member = fixture()
        .members
        .into_iter()
        .find(|m| m.id == "m1")
        .unwrap();
    member.session = Some(zerus_team_plan::model::SessionKey {
        machine: "machine".into(),
        session_id: "launch_id:launch".into(),
    });
    assert_eq!(
        super::super::availability::current_run(&native.config, TEAM, &member).unwrap(),
        None
    );
    resumed(h.temp.path(), TEAM, "m1", "launch", "resumed-run");
    assert_eq!(
        super::super::availability::current_run(&native.config, TEAM, &member)
            .unwrap()
            .as_deref(),
        Some("resumed-run")
    );
    assert!(!matches!(
        super::super::availability::free(&native.config, "epoch", TEAM, &member),
        Ok(true)
    ));
}
