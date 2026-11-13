//! zerus-63rru.36: the
//! cancelled-then-moved-back task in private state. The team.s commands run
//! as real children; the person-side commands run through the exact product
//! APIs the CLI arms call after its caller proof, because the proof itself
//! refuses this harness's ancestry (captured in the log).
use super::*;

fn doc_view(f: &Fixture, uuid: &str) -> zerus_replica::TaskView {
    let store = crate::tasks::store(&f.config, "example-project").unwrap();
    let doc = store.load(uuid).unwrap();
    doc.view().unwrap()
}
fn fields(f: &Fixture, uuid: &str, step: &str) {
    let v = doc_view(f, uuid);
    eprintln!(
        "WP36 {step}: stage={} cancelled_at={:?} cancel_reason={:?} session={:?} execution={:?} writable={}",
        v.stage.as_str(),
        v.cancelled_at,
        v.cancel_reason,
        v.session,
        v.execution.as_ref().map(|e| (&e.current, &e.conflict)),
        v.writable
    );
}
fn intake(f: &Fixture, uuid: &str, team: &str, step: &str) -> Value {
    let task = TaskRef {
        project: "example-project".into(),
        uuid: uuid.to_string(),
    };
    let result = crate::teams::tasks::send(
        &crate::teams::tasks::Caller {
            state: &f.config.state,
            machine: &crate::swarm::local_node_id(&f.config).unwrap(),
            actor: Actor::Person,
        },
        &task,
        team,
        None,
        None,
    );
    let out = match &result {
        Ok(intake) => json!({
            "queued": intake.queued,
            "node": intake.node,
            "refusal": intake.refusal.as_ref().map(|p| json!({"code": p.code, "message": p.message})),
            "observation_pending": intake.observation_pending,
        }),
        Err(error) => json!({"error": {"code": error.code, "message": error.message}}),
    };
    eprintln!("WP36 {step}: intake={out}");
    out
}
fn add(f: &mut Fixture) -> (String, String) {
    let tasks = ProjectStore::open(&f.config.state.join("replica"), "example-project").unwrap();
    let (mut task, _) = tasks
        .new_named_task(
            "Reproduction task",
            "Reproduction body",
            &[],
            Stage::Todo,
            Target::Any,
            Actor::Person,
            &Position::End,
            "example",
            now(),
            None,
            false,
        )
        .unwrap();
    let id = task.id().to_string();
    let uuid = task.uuid().to_string();
    tasks.save(&mut task).unwrap();
    (id, uuid)
}
fn deliver(f: &mut Fixture, uuid: &str, member: &str) -> u32 {
    let out = intake(f, uuid, TEAM, "send to the team");
    assert_eq!(out["queued"], true, "{out}");
    let node = out["node"].as_u64().unwrap() as u32;
    if f.h
        .store
        .member(TEAM, member)
        .unwrap()
        .unwrap()
        .session
        .is_none()
    {
        f.bind(member, &crate::swarm::local_node_id(&f.config).unwrap());
    }
    let intent = f.h.intent(node, member);
    f.body.node = node;
    f.body.member = member.to_string();
    f.body.id = intent.clone();
    f.task.uuid = uuid.to_string();
    let delivered = f.run("submitted", false);
    eprintln!("WP36 deliver reply: {delivered}");
    assert_eq!(delivered["reply"]["state"], "submitted");
    // The deliver command itself wrote the task's send (the doc now shows In
    // progress); the observation records it for the team.
    crate::teams::tasks::observe(
        &f.config.state,
        &TaskRef {
            project: "example-project".into(),
            uuid: uuid.to_string(),
        },
    )
    .unwrap();
    node
}
fn team_cancel(f: &mut Fixture, node: u32) -> Value {
    let lead = ops::Actor::Session {
        person: "local".into(),
        team: TEAM.into(),
        member: "lead".into(),
        session: zerus_team_plan::model::SessionKey {
            machine: f.lead.machine.clone(),
            session_id: f.lead.session_id.clone(),
        },
    };
    f.h.run(
        &lead,
        ops::Operation::Cancel {
            node,
            reason: "Superseded".into(),
        },
    )
    .unwrap();
    f.body.node = node;
    let mut args = vec!["task".to_string(), "team-cancel".to_string()];
    args.extend(f.args(false));
    let body = json!({"schema":1,"team":TEAM,"node":node});
    let (child, path) = f.spawn("cli", &args, &body.to_string(), false);
    let result = f.wait(child);
    let reply: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    eprintln!(
        "WP36 team-cancel: reply={reply} stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    reply
}
fn move_back(f: &Fixture, uuid: &str, step: &str) {
    let store = crate::tasks::store(&f.config, "example-project").unwrap();
    let mut doc = store.load(uuid).unwrap();
    store
        .change_stage_placed(None, &mut doc, Stage::Todo, &Actor::Person, now())
        .unwrap();
    store.save(&mut doc).unwrap();
    eprintln!("WP36 {step}: moved back to To do");
}

#[test]
#[test]
fn wp36_reproduction() {
    let mut f = Fixture::new();
    // The proof's refusal in a real child, captured for the record: under
    // this harness the person proof cannot be established, so the person-side
    // commands run through the product APIs the CLI calls after its proof.
    let _ = f.cli_with(
        &["task".into(), "stage".into(), "x".into(), "todo".into()],
        "{}",
        &[("HGS_TEAMS_TEST_CONTEXT", "opaque")],
    );

    // Sequence A: the team's cancel.
    let (_id_a, uuid_a) = add(&mut f);
    fields(&f, &uuid_a, "A fresh");
    let node_a = deliver(&mut f, &uuid_a, "m2");
    fields(&f, &uuid_a, "A delivered");
    let answer = team_cancel(&mut f, node_a);
    eprintln!("WP36 A team-cancel answer: {answer}");
    fields(&f, &uuid_a, "A after team cancel");
    crate::teams::tasks::observe(
        &f.config.state,
        &TaskRef {
            project: "example-project".into(),
            uuid: uuid_a.clone(),
        },
    )
    .unwrap();
    eprintln!(
        "WP36 A node after the observation: {:?}",
        f.h.store.node(TEAM, node_a).unwrap().map(|n| n.state)
    );
    move_back(&f, &uuid_a, "A");
    fields(&f, &uuid_a, "A moved back to To do");
    intake(&f, &uuid_a, TEAM, "A resend to the same team");
    fields(&f, &uuid_a, "A after the same-team refusal");
    let mut create2 = ops::tests::create();
    if let ops::Operation::Create {
        home: machine,
        integration_branch,
        ..
    } = &mut create2
    {
        *machine = crate::swarm::local_node_id(&f.config).unwrap();
        *integration_branch = "other-branch".into();
    }
    f.h.run_for("00000000-0000-4000-8000-0000000000cc", &person(), create2)
        .unwrap();
    intake(
        &f,
        &uuid_a,
        "00000000-0000-4000-8000-0000000000cc",
        "A resend to another team",
    );
    fields(&f, &uuid_a, "A after the other-team refusal");

    // Sequence B: the person's ordinary cancel.
    let (_id_b, uuid_b) = add(&mut f);
    let node_b = deliver(&mut f, &uuid_b, "m2");
    fields(&f, &uuid_b, "B delivered");
    {
        let store = crate::tasks::store(&f.config, "example-project").unwrap();
        let mut doc = store.load(&uuid_b).unwrap();
        doc.record_cancel("Obsolete", &zerus_replica::Actor::Person, now())
            .unwrap();
        store.save(&mut doc).unwrap();
    }
    fields(&f, &uuid_b, "B after person cancel");
    move_back(&f, &uuid_b, "B");
    fields(&f, &uuid_b, "B moved back to To do");
    intake(&f, &uuid_b, TEAM, "B resend to the same team");
    let other_b = intake(
        &f,
        &uuid_b,
        "00000000-0000-4000-8000-0000000000cc",
        "B resend to another team",
    );
    fields(&f, &uuid_b, "B after the refusals");
    // The pinned refusal: a task moved back to To do after a cancel is still
    // refused by the intake, because the cancelled mark survives the move.
    assert_eq!(
        other_b["error"]["message"], "Send a task in Backlog or To do to a team.",
        "{other_b}"
    );
    let _ = node_b;
}
