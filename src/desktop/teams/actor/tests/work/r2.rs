use super::*;
#[test]
fn a_saved_conflict_refusal_is_replaced_without_replaying_the_bad_name_or_git() {
    let mut h = Harness::new();
    let (node, lead) = h.reported();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let mut a = actor(&h);
    let mut p = Work {
        merge_reply: Some(MergeResult::Conflict {
            files: vec!["   ".into(), "good.txt".into()],
        }),
        ..Default::default()
    };
    super::super::super::integration::CRASH.with(|s| s.set(Some("outcome")));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(
            &mut h, &mut a, &mut p, 100
        )))
        .is_err()
    );
    let decision = h.accept_decision(node);
    let key = format!("merge:{decision}");
    let mut row = a.ledger.get(TEAM, &key).unwrap().unwrap();
    // A pre-fix row already contains the invalid name and its saved refusal.
    row.data["outcome"] = serde_json::to_value(MergeResult::Conflict {
        files: vec!["   ".into(), "good.txt".into()],
    })
    .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let revision = h.store.team(TEAM).unwrap().unwrap().revision;
    let request = crate::teams::ops::Request {
        request_id: id.clone(),
        team: TEAM.into(),
        expected_revision: revision,
        operation: Operation::RecordIntegrationConflict {
            node,
            decision,
            conflicts: vec!["   ".into(), "good.txt".into()],
        },
    };
    assert!(crate::teams::ops::apply(&mut h.store, &service(), &request, 101).is_err());
    assert!(h.store.request_refused(TEAM, &id).unwrap());
    row.data["fact_id"] = json!(id);
    row.data["fact_revision"] = json!(revision);
    a.ledger.save(&mut row).unwrap();
    drop(a);
    let mut a = actor(&h);
    run(&mut h, &mut a, &mut p, 102);
    let fact = h
        .store
        .integration_fact(TEAM, node)
        .unwrap()
        .expect("a certain conflict remained stuck on its refused name");
    assert_eq!(fact.conflicts, vec!["good.txt", "and 1 more files"]);
    assert!(!a.ledger.merge_pending(TEAM).unwrap());
    run(&mut h, &mut a, &mut p, 180_100);
    assert_eq!(p.merges, 1);
}
