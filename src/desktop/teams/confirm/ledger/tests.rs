use super::*;
use crate::desktop::teams::confirm::{policy::tests::sample, resumed::resumed_binding};

#[test]
fn reservation_tombstones_and_hourly_quota_survive_database_reopens() {
    let temp = tempfile::tempdir().unwrap();
    let (b, _, s) = sample();
    let mut last = None;
    for n in 0..4 {
        let mut db = Ledger::open(temp.path()).unwrap();
        let mut b = b.clone();
        b.run = format!("run-{n}");
        let mut s = s.clone();
        s.run = b.run.clone();
        let p = engine::candidate(&b, &s, Kind::Resume, serde_json::Value::Null);
        let c = db
            .claim(&p, &format!("epoch-{n}"), 1000 + n * 1000)
            .unwrap();
        assert!(c.fresh);
        assert_eq!(c.limited, n == 3);
        assert!(!db.claim(&p, "replacement-epoch", 10000).unwrap().fresh);
        last = Some(p);
    }
    let mut db = Ledger::open(temp.path()).unwrap();
    let mut p = last.unwrap();
    p.key.push('x');
    p.binding.run = "later-run".into();
    assert!(!db.claim(&p, "later", 3_605_000).unwrap().limited);
}

#[test]
fn unknown_never_publishes_a_binding_and_completed_receipt_publishes_exactly_one_fact() {
    let temp = tempfile::tempdir().unwrap();
    let (b, _, mut s) = sample();
    s.conversation = "conversation-1".into();
    assert!(resumed_binding(temp.path(), &b.team, &b.member, &b.launch)
        .unwrap()
        .is_none());
    let mut db = Ledger::open(temp.path()).unwrap();
    let p = engine::candidate(&b, &s, Kind::Resume, serde_json::Value::Null);
    let mut c = db.claim(&p, "epoch-1", 100).unwrap().record;
    c.state = "unknown".into();
    let mut incomplete = s.clone();
    incomplete.run = "unconfirmed-run".into();
    incomplete.receipt_at = Some(123000);
    c.result = Some(incomplete);
    db.save(&c).unwrap();
    assert!(db.binding(&b).unwrap().is_none());
    assert!(resumed_binding(temp.path(), &b.team, &b.member, &b.launch)
        .unwrap()
        .is_none());
    c.state = "approved".into();
    assert!(
        db.save(&c).is_err(),
        "A terminal Unknown cannot change to Approved"
    );
    c.plan.key.push('x');
    let mut c = db.claim(&c.plan, "epoch-2", 200).unwrap().record;
    c.state = "approved".into();
    let mut target = s;
    target.run = "new-run".into();
    target.conversation = "conversation-1".into();
    target.receipt_at = Some(123456);
    c.result = Some(target);
    db.save(&c).unwrap();
    let fact = resumed_binding(temp.path(), &b.team, &b.member, &b.launch)
        .unwrap()
        .unwrap();
    assert_eq!(fact.run_id, "new-run");
    assert_eq!(fact.conversation_id, "conversation-1");
    assert_eq!(fact.session_id, b.session);
    assert_eq!(fact.receipt_at, 123456);
    assert!(
        resumed_binding(temp.path(), &b.team, &b.member, "another-launch")
            .unwrap()
            .is_none()
    );
    let brief = db.pending_brief(&b).unwrap().unwrap();
    assert_eq!(brief.binding.run, fact.run_id);
    assert_eq!(brief.binding.conversation, fact.conversation_id);
    assert_eq!(brief.kind, Kind::Brief);
}

#[test]
fn claim_identity_is_immutable_and_pending_reads_are_indexed_and_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let (b, _, s) = sample();
    let mut db = Ledger::open(temp.path()).unwrap();
    let p = engine::candidate(&b, &s, Kind::Trust, s.questions[0].clone());
    let row = db.claim(&p, "epoch-1", 100).unwrap().record;
    let mut changed = row.clone();
    changed.original.push('x');
    assert!(db.save(&changed).is_err());
    changed = row.clone();
    changed.plan.binding.session.push('x');
    assert!(db.save(&changed).is_err());
    assert_eq!(db.unsettled(&b).unwrap().unwrap(), row);
    let query="EXPLAIN QUERY PLAN SELECT body FROM standing_effects WHERE team=?1 AND member=?2 AND launch=?3 AND pending=1 ORDER BY key LIMIT 1";
    let plan: String =
        db.0.query_row(query, params![b.team, b.member, b.launch], |r| r.get(3))
            .unwrap();
    assert!(plan.contains("standing_pending"), "{plan}");
    let plan:String=db.0.query_row("EXPLAIN QUERY PLAN SELECT 1 FROM standing_effects WHERE team=?1 AND member=?2 AND kind='resume' AND at>?3 LIMIT 3",params![b.team,b.member,100],|r|r.get(3)).unwrap();
    assert!(plan.contains("standing_quota"), "{plan}");
}
