use super::*;
use crate::teams::ops::{tests::*, Actor, Operation};

fn reopen(h: &mut Harness) {
    h.store = Store::open(h.temp.path()).unwrap();
}
fn db(h: &Harness) -> &Connection {
    h.store.db.as_ref().unwrap()
}

#[test]
fn the_terminal_index_is_filled_once_when_it_is_created() {
    let mut h = Harness::new();
    h.run(&person(), Operation::Stop).unwrap();
    db(&h)
        .execute_batch("DROP TRIGGER key_clear_terminal_v1; DROP TABLE key_clear_teams;")
        .unwrap();
    reopen(&mut h);
    assert_eq!(h.store.keyed_teams("", 10).unwrap(), vec![TEAM.to_owned()]);
    db(&h).execute("DELETE FROM key_clear_teams", []).unwrap();
    reopen(&mut h);
    assert!(
        h.store.keyed_teams("", 10).unwrap().is_empty(),
        "later opens never fill it again"
    );
}

#[test]
fn a_reopened_store_keeps_its_timer_triggers_under_their_versioned_names() {
    let mut h = Harness::new();
    reopen(&mut h);
    let names: Vec<String> = column(
        db(&h),
        "SELECT name FROM sqlite_master WHERE type='trigger' AND name LIKE '%_v1' ORDER BY name",
        [],
    )
    .unwrap();
    assert_eq!(
        names,
        [
            "key_clear_stage_archived_v1",
            "key_clear_stage_settled_v1",
            "key_clear_terminal_v1",
            "timer_cards_decided_v1",
            "timer_cards_item_v1"
        ]
    );
}

#[test]
fn the_inbox_view_counts_open_items_up_to_its_bound() {
    let h = Harness::new();
    assert_eq!(h.store.inbox_view(TEAM).unwrap(), None);
    for n in 0..250 {
        let state = if n % 50 == 0 { "closed" } else { "open" };
        db(&h).execute("INSERT INTO inbox(team,kind,node,generation,actor,text,state,created_ms) VALUES(?1,'report',?2,1,'{}','Example',?3,?2)",params![TEAM,n,state]).unwrap();
    }
    let view = h.store.inbox_view(TEAM).unwrap().unwrap();
    assert_eq!((view.open, view.newest), (201, 250));
    assert_eq!(view.oldest.unwrap().node, 1);
}

#[test]
fn the_doorbell_admission_reads_the_team_the_binding_and_a_new_item() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    let Actor::Session { session, .. } = lead.clone() else {
        unreachable!()
    };
    let bell = |after| Bell {
        team: TEAM.into(),
        member: "lead".into(),
        session: session.clone(),
        after,
    };
    assert!(admit_bell(db(&h), &bell(0)).is_err(), "nothing waits");
    h.bind("m1");
    let node = h.add(&person(), 1);
    h.submit(node, "m1");
    h.report(node, "m1");
    admit_bell(db(&h), &bell(0)).unwrap();
    assert!(
        admit_bell(db(&h), &bell(1)).is_err(),
        "nothing beyond the mark"
    );
    let mut other = bell(0);
    other.session.session_id = "run_id:another-run".into();
    assert!(admit_bell(db(&h), &other).is_err(), "another binding");
    h.run(&lead, Operation::Pause).unwrap();
    assert!(admit_bell(db(&h), &bell(0)).is_err(), "paused");
}

/// Every timer read is a search through an index: no full scan of a table
/// and no temporary sort. `--nocapture` prints the plans for the report.
#[test]
fn every_timer_read_searches_an_index() {
    let h = Harness::new();
    for (name, sql) in [
        ("NEWEST", sql::NEWEST),
        ("OPEN", sql::OPEN),
        ("OLDEST", sql::OLDEST),
        ("UNREPORTED", sql::UNREPORTED),
        ("UNSTARTED", sql::UNSTARTED),
        ("UNDECIDED", sql::UNDECIDED),
        ("FIRED", sql::FIRED),
        ("KEYED_TEAMS", sql::KEYED_TEAMS),
        ("KEYED_PAGE", sql::KEYED_PAGE),
        ("SETTLE", sql::SETTLE),
        ("LATE", sql::LATE),
        ("REFUSALS", sql::REFUSALS),
        ("SAVED_REFUSAL", sql::SAVED_REFUSAL),
        ("RELEASED_STAGES", sql::RELEASED_STAGES),
        ("WAITING", sql::WAITING),
    ] {
        let plan = explain(db(&h), sql);
        println!("{name}:\n  {}", plan.join("\n  "));
        for line in &plan {
            let table = !line.starts_with("SCAN (subquery-") && line != "SCAN CONSTANT ROW";
            let access = table && (line.starts_with("SCAN") || line.starts_with("SEARCH"));
            assert!(!access || line.contains("USING"), "{name}: {line}");
            assert!(!line.contains("TEMP B-TREE"), "{name}: {line}");
        }
    }
}
fn explain(db: &Connection, sql: &str) -> Vec<String> {
    let mut s = db.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    let nulls = vec![rusqlite::types::Null; s.parameter_count()];
    let rows = s.query_map(rusqlite::params_from_iter(nulls), |r| r.get::<_, String>(3));
    rows.unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
}
