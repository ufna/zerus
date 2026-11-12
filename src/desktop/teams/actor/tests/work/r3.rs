//! The reviewer's outcome probes, with exact first-request assertions.
use super::*;
#[test]
fn two_hundred_one_conflicts_keep_199_names_on_the_first_fact_request() {
    let mut h = Harness::new();
    let (node, lead) = h.reported();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let mut a = actor(&h);
    let mut p = Work {
        merge_reply: Some(MergeResult::Conflict {
            files: (0..201).map(|i| format!("f{i:03}.txt")).collect(),
        }),
        ..Default::default()
    };
    run(&mut h, &mut a, &mut p, 100);
    let fact = h.store.integration_fact(TEAM, node).unwrap().unwrap();
    assert_eq!(p.merges, 1);
    assert_eq!(fact.conflicts.len(), 200);
    assert_eq!(
        &fact.conflicts[..199],
        (0..199).map(|i| format!("f{i:03}.txt")).collect::<Vec<_>>()
    );
    assert_eq!(fact.conflicts.last().unwrap(), "and 2 more files");
    let refused: i64 = h.store.db.as_ref().unwrap().query_row(
        "SELECT count(*) FROM requests WHERE team=?1 AND json_type(result,'$.Refused')='object'",
        [TEAM], |r| r.get(0)).unwrap();
    assert_eq!(
        refused, 0,
        "a refused first fact silently discarded conflict names"
    );
}

#[test]
fn a_refused_corrected_fact_with_a_ready_accept_never_starts_git_again() {
    let mut h = Harness::new();
    let (node, lead) = h.reported();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let mut a = actor(&h);
    let mut p = Work {
        merge_reply: Some(MergeResult::Conflict {
            files: vec!["a.txt".into()],
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
    let key = format!("merge:{}", h.accept_decision(node));
    let mut row = a.ledger.get(TEAM, &key).unwrap().unwrap();
    row.data["refused_fact"] = json!(true);
    a.ledger.save(&mut row).unwrap();
    drop(a);
    let mut a = actor(&h);
    for now in [102, 180_100, 1_000_000, 3_000_000] {
        run(&mut h, &mut a, &mut p, now);
        assert_eq!(
            p.merges, 1,
            "refused fact authorized another original at {now}"
        );
        assert!(a.ledger.get(TEAM, &key).unwrap().is_some());
        assert!(h.store.integration_fact(TEAM, node).unwrap().is_none());
    }
}
