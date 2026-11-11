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

struct ResumeAtView {
    native: super::super::native::Native,
    launch: String,
    free_reads: usize,
}
impl Ports for ResumeAtView {
    fn changes(&mut self, _: &str, _: &str, _: u64) -> Result<zerus_replica::Changes> {
        Ok(zerus_replica::Changes {
            generation: "journal".into(),
            offset: 0,
            documents: Some(Default::default()),
        })
    }
    fn observe(&mut self, _: &mut Store, _: &TaskRef) -> Result<()> {
        Ok(())
    }
    fn free(&mut self, t: &Team, m: &Member) -> Result<bool> {
        let free = self.native.free(t, m)?;
        self.free_reads += 1;
        if self.free_reads == 1 {
            resumed(
                &self.native.config.state,
                &t.id,
                &m.id,
                &self.launch,
                "new-run",
            );
        }
        Ok(free)
    }
    fn deliver(
        &mut self,
        _: &Team,
        _: &Node,
        _: &Member,
        _: &str,
    ) -> crate::teams::ops::HandoutOutcome {
        panic!("busy resumed member must not be sent a task")
    }
}
fn fleet_runs(native: &super::super::native::Native, launch: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&native.config.state, std::fs::Permissions::from_mode(0o700)).unwrap();
    crate::platform::private_dir(&native.config.dir).unwrap();
    crate::platform::private_dir(&native.config.state.join("desktop")).unwrap();
    let path = native.config.state.join("desktop/state.sqlite3");
    crate::sqlite_file::create(&path).unwrap();
    let db = rusqlite::Connection::open(path).unwrap();
    zerus_desktop_catalog::migrate(&db).unwrap();
    db.execute(
        "UPDATE desktop_catalog_owner SET epoch=?1 WHERE id=1",
        [&native.epoch],
    )
    .unwrap();
    db.execute("INSERT INTO desktop_catalog_sources(computer,epoch,incarnation,generation,stream,seq,visible,complete,active,hello) VALUES('',?1,1,1,'stream',1,1,1,1,?2)",rusqlite::params![native.epoch,json!({"machine_id":native.machine}).to_string()]).unwrap();
    db.execute(
        "INSERT INTO desktop_catalog_generations(id,computer) VALUES(1,'')",
        [],
    )
    .unwrap();
    for (run, activity) in [("old-run", "idle"), ("new-run", "busy")] {
        let row=json!({"name":"codex/resumed","agent":"codex","cmd":"codex","state":"running","process_state":"running","activity":activity,"phase":"idle","run_id":run,"launch_id":launch,"conversation_id":"conversation","conversation_state":"active","process_work":{"count":0},"subagents_active":0,"optional_questions":{"count":0}}).to_string();
        db.execute(
            "INSERT INTO desktop_catalog_rows(generation,key,json,bytes) VALUES(1,?1,?2,?3)",
            rusqlite::params![format!("run:{run}"), row, row.len()],
        )
        .unwrap();
    }
}
#[test]
fn a_member_resumed_between_the_view_and_the_intent_is_rechecked_on_its_new_run() {
    let mut h = Harness::new();
    let launch = uuid::Uuid::new_v4().to_string();
    let mut native = super::production::native(&h);
    let sample = fixture()
        .members
        .into_iter()
        .find(|m| m.id == "m1")
        .unwrap();
    h.run(
        &service(),
        crate::teams::ops::Operation::SetMemberSession {
            member: "m1".into(),
            session: zerus_team_plan::model::SessionKey {
                machine: native.machine.clone(),
                session_id: format!("launch_id:{launch}"),
            },
            account: sample.account,
            worktree: sample.worktree,
            ownership_nonce: sample.ownership_nonce,
        },
    )
    .unwrap();
    let node = h.add(&person(), 1);
    fleet_runs(&native, &launch);
    resumed(h.temp.path(), TEAM, "m1", &launch, "old-run");
    super::super::native::TEST_INVENTORY.with(|s| {
        *s.borrow_mut() = Some(json!({"node_id":native.machine,"organization":{"projects":[]}}))
    });
    let team = h.store.team(TEAM).unwrap().unwrap();
    assert!(native
        .free(&team, &h.store.member(TEAM, "m1").unwrap().unwrap())
        .unwrap());
    assert!(!crate::teams::ops::readiness(&h.store, TEAM)
        .unwrap()
        .nodes
        .is_empty());
    let mut a = super::super::Runtime::open(h.temp.path(), &native.machine).unwrap();
    let mut p = ResumeAtView {
        native,
        launch,
        free_reads: 0,
    };
    a.tick(&mut h.store, &mut p, TEAM, 100, false).unwrap();
    assert_eq!(p.free_reads, 2);
    assert!(h.node(node).assignee.is_none());
    assert!(h.store.effects(TEAM, 16, None).unwrap().rows.is_empty());
    super::super::native::TEST_INVENTORY.with(|s| s.borrow_mut().take());
}
