use super::*;
#[test]
fn create_and_add_replay_the_first_reply_and_one_journal_entry() {
    let mut h = Harness::empty();
    let r = request(0, create());
    let first = super::super::apply(&mut h.store, &person(), &r, 1).unwrap();
    assert_eq!(
        super::super::apply(&mut h.store, &person(), &r, 2).unwrap(),
        first
    );
    assert_eq!(h.store.members(TEAM, 200, None).unwrap().rows.len(), 4);
    let r = request(
        1,
        Operation::AddTask {
            task: task(1),
            role: None,
            pipeline: None,
        },
    );
    let first = super::super::apply(&mut h.store, &person(), &r, 1).unwrap();
    assert_eq!(
        super::super::apply(&mut h.store, &person(), &r, 2).unwrap(),
        first
    );
    assert_eq!(h.store.journal(TEAM, 200, None).unwrap().rows.len(), 2);
}
#[test]
fn replay_compares_actor_and_full_content() {
    let mut h = Harness::new();
    let r = request(1, Operation::Pause);
    super::super::apply(&mut h.store, &person(), &r, 1).unwrap();
    let other = Actor::Person {
        person: "local".into(),
        origin: PersonOrigin::CliInterim,
    };
    refused(
        super::super::apply(&mut h.store, &other, &r, 2),
        code::DUPLICATE,
    );
    let mut changed = r;
    changed.operation = Operation::Resume;
    refused(
        super::super::apply(&mut h.store, &person(), &changed, 2),
        code::DUPLICATE,
    );
}
#[test]
fn stale_revision_refusal_is_durable_and_rolls_back_the_operation() {
    let mut h = Harness::new();
    let r = request(0, Operation::Pause);
    refused(
        super::super::apply(&mut h.store, &person(), &r, 1),
        code::STALE_REVISION,
    );
    h.run(&person(), Operation::Pause).unwrap();
    refused(
        super::super::apply(&mut h.store, &person(), &r, 2),
        code::STALE_REVISION,
    );
    assert_eq!(h.store.journal(TEAM, 200, None).unwrap().rows.len(), 2);
}
#[test]
fn exact_receipt_replay_is_read_only_after_delegation_revocation() {
    let mut h = Harness::new();
    let (node, lead) = h.reported();
    let r = request(
        h.store.team(TEAM).unwrap().unwrap().revision,
        Operation::Accept { node },
    );
    let first = super::super::apply(&mut h.store, &lead, &r, 1).unwrap();
    h.settings(|s| s.delegations.accept_results = false);
    assert_eq!(
        super::super::apply(&mut h.store, &lead, &r, 2).unwrap(),
        first
    );
    refused(h.run(&lead, Operation::Accept { node }), code::NOT_ALLOWED);
}
#[test]
fn request_wire_refuses_unknown_fields_and_sender_observations() {
    for op in [
        Operation::Pause,
        Operation::AddTask {
            task: task(1),
            role: None,
            pipeline: None,
        },
    ] {
        let mut v = serde_json::to_value(request(0, op)).unwrap();
        v["observation"] = serde_json::json!({"stage":"done"});
        assert!(serde_json::from_value::<Request>(v).is_err());
    }
    let mut v = serde_json::to_value(request(0, Operation::Accept { node: 1 })).unwrap();
    v["integration_commit"] = COMMIT.into();
    assert!(serde_json::from_value::<Request>(v).is_err());
}
#[test]
fn rejected_creation_does_not_create_a_store() {
    let mut h = Harness::empty();
    refused(h.run(&other(), create()), code::NOT_ALLOWED);
    assert!(!h.temp.path().join("teams/teams-v1.sqlite3").exists());
}
#[test]
fn failed_operation_has_no_partial_rows_or_journal() {
    let mut h = Harness::new();
    let node = h.add(&person(), 1);
    let old = h.store.team(TEAM).unwrap();
    let journal = h.store.journal(TEAM, 200, None).unwrap();
    let mut r = route(node);
    if let Operation::Route { role, .. } = &mut r {
        *role = "missing".into();
    }
    refused(h.run(&person(), r), code::UNKNOWN_ROLE);
    assert_eq!(h.store.team(TEAM).unwrap(), old);
    assert_eq!(h.store.journal(TEAM, 200, None).unwrap(), journal);
}
