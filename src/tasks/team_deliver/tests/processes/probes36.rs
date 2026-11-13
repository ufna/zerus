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
    let a_same = intake(&f, &uuid_a, TEAM, "A resend to the same team");
    assert_eq!(a_same["queued"], true, "{a_same}");
    fields(&f, &uuid_a, "A after the same-team resend");
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
    // The new node in this team is live: another team refuses the same task
    // (the existing live-node guard), so one task never runs in two teams.
    let a_other = intake(
        &f,
        &uuid_a,
        "00000000-0000-4000-8000-0000000000cc",
        "A resend to another team",
    );
    assert_eq!(
        a_other["error"]["code"], "duplicate",
        "{a_other}"
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
    // The team-cancel command on the revived task: no mark, no live
    // attempt of the team — it answers released, never a second cancel.
    {
        f.body.node = node_b;
        let mut args = vec!["task".to_string(), "team-cancel".to_string()];
        args.extend(f.args(false));
        let body = json!({"schema":1,"team":TEAM,"node":node_b});
        let (child, path) = f.spawn("cli", &args, &body.to_string(), false);
        let result = f.wait(child);
        let reply: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        eprintln!(
            "WP36 B team-cancel on the revived task: {reply} {}",
            String::from_utf8_lossy(&result.stdout)
        );
        assert_eq!(reply["reply"]["exit"], 1, "{reply}");
    }
    // The resend while the old node is still live: refused with the true
    // sentence (zerus-63rru.36 rule 3)...
    let refused = intake(&f, &uuid_b, TEAM, "B resend while the old node is live");
    assert_eq!(
        refused["error"]["message"],
        "The earlier hand-out of this task is still being settled in this team; send it again after it ends.",
        "{refused}"
    );
    // ... and the observation ends the old node from the attempt's phase
    // (rule 2), so the next resend is admitted.
    crate::teams::tasks::observe(
        &f.config.state,
        &TaskRef {
            project: "example-project".into(),
            uuid: uuid_b.clone(),
        },
    )
    .unwrap();
    // The node's live binding is what the observation ends; the resend's
    // acceptance proves it (the intake found no live node).
    let admitted = intake(&f, &uuid_b, TEAM, "B resend after the node ended");
    assert_eq!(admitted["queued"], true, "{admitted}");
    // The new node is live: another team refuses the same task, as before.
    let other_b = intake(
        &f,
        &uuid_b,
        "00000000-0000-4000-8000-0000000000cc",
        "B resend to another team",
    );
    assert_eq!(other_b["error"]["code"], "duplicate", "{other_b}");
    fields(&f, &uuid_b, "B after the new node");
    let _ = node_b;
}

/// Rule 2, the started variant: the member began the attempt before the
/// person's cancel. The move-back clears the mark; the observation still
/// ends the old node from the bound attempt's phase, and the resend is
/// refused before it and admitted after it.
#[test]
fn a_started_attempt_is_ended_by_the_observation_after_the_move_back() {
    let mut f = Fixture::new();
    let (_id, uuid) = add(&mut f);
    let node = deliver(&mut f, &uuid, "m2");
    {
        let store = crate::tasks::store(&f.config, "example-project").unwrap();
        let mut doc = store.load(&uuid).unwrap();
        let id = f.body.id.clone();
        let holder = doc.attempt(&id).unwrap().unwrap().executor;
        doc.begin_attempt(&id, &Actor::Agent(holder), now()).unwrap();
        store.save(&mut doc).unwrap();
    }
    crate::teams::tasks::observe(
        &f.config.state,
        &TaskRef {
            project: "example-project".into(),
            uuid: uuid.clone(),
        },
    )
    .unwrap();
    {
        let store = crate::tasks::store(&f.config, "example-project").unwrap();
        let mut doc = store.load(&uuid).unwrap();
        doc.record_cancel("Obsolete", &zerus_replica::Actor::Person, now())
            .unwrap();
        store.save(&mut doc).unwrap();
    }
    move_back(&f, &uuid, "started variant");
    let refused = intake(&f, &uuid, TEAM, "started: resend while the node is live");
    assert_eq!(
        refused["error"]["message"],
        "The earlier hand-out of this task is still being settled in this team; send it again after it ends.",
        "{refused}"
    );
    crate::teams::tasks::observe(
        &f.config.state,
        &TaskRef {
            project: "example-project".into(),
            uuid: uuid.clone(),
        },
    )
    .unwrap();
    let admitted = intake(&f, &uuid, TEAM, "started: resend after the node ended");
    assert_eq!(admitted["queued"], true, "{admitted}");
    let _ = node;
}

/// Rule 2, the team's own cancel with the observation deferred: the team
/// cancels, the person moves the task back before the observation reads the
/// mark, and the deferred observation still ends the node from the phase.
#[test]
fn the_teams_cancel_ends_the_node_even_when_the_observation_comes_after_the_move_back() {
    let mut f = Fixture::new();
    let (_id, uuid) = add(&mut f);
    let node = deliver(&mut f, &uuid, "m2");
    team_cancel(&mut f, node);
    fields(&f, &uuid, "team variant: after the team cancel");
    move_back(&f, &uuid, "team variant");
    crate::teams::tasks::observe(
        &f.config.state,
        &TaskRef {
            project: "example-project".into(),
            uuid: uuid.clone(),
        },
    )
    .unwrap();
    let admitted = intake(&f, &uuid, TEAM, "team variant: resend");
    assert_eq!(admitted["queued"], true, "{admitted}");
}

