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
