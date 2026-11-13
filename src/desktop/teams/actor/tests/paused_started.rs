//! zerus-63rru.86 through the production Native and the real pass: a member
//! paused or stopped while it holds a started task keeps the task, and the
//! lead is told once per member and task at the first pass that sees the
//! member out of the active set; the member's return ends the memory, so a
//! later leave tells the lead again. No timer starts, and the person's own
//! pause is not stored, so the item is written in every case.
use super::real_rows::Production;
use super::rows::seed_row;
use crate::teams::ops::tests::*;
use crate::teams::ops::{Actor, Operation};
use serde_json::{json, Value};

const MINUTE: i64 = 60_000;
const T0: i64 = 10_000_000;

/// The member's committed fleet row, as its producer writes it.
fn row(p: &Production, h: &Harness, member: &str, changes: &[(&str, Value)]) {
    let m = h.store.member(TEAM, member).unwrap().unwrap();
    let launch = m.session.as_ref().unwrap().session_id["launch_id:".len()..].to_owned();
    let run = format!("fixture-{member}");
    let mut all = vec![
        ("name", json!(format!("{}/fixture/{member}", m.agent))),
        ("run_id", json!(run)),
        ("launch_id", json!(launch)),
        ("conversation_id", json!(format!("conversation-{member}"))),
    ];
    all.extend(changes.iter().cloned());
    let row = seed_row(&m.agent, &all).to_string();
    rusqlite::Connection::open(p.native.config.state.join("desktop/state.sqlite3"))
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO desktop_catalog_rows(generation,key,json,bytes) VALUES(1,?1,?2,?3)",
            rusqlite::params![format!("run:{run}"), row, row.len()],
        )
        .unwrap();
}

/// A running team whose task 1 can go to m1 only (m2 is busy), the lead
/// bound; the first pass has not run.
fn team() -> (Harness, super::super::Runtime, Production, Actor) {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    h.bind("m1");
    h.bind("m2");
    h.add(&person(), 1);
    let native = super::production::native(&h);
    let machine = h.store.team(TEAM).unwrap().unwrap().home;
    let a = super::super::Runtime::open(h.temp.path(), &machine).unwrap();
    let p = Production {
        native,
        deliveries: vec![],
    };
    row(&p, &h, "m2", &[("activity", json!("busy"))]);
    (h, a, p, lead)
}

fn items(h: &Harness) -> Vec<(String, String)> {
    h.store
        .inbox(TEAM, 200, None)
        .unwrap()
        .rows
        .into_iter()
        .map(|i| (i.kind, i.text))
        .collect()
}

fn effects(h: &Harness) -> Vec<(String, String, u32)> {
    h.store
        .effects(TEAM, 200, None)
        .unwrap()
        .rows
        .into_iter()
        .map(|e| (e.state, e.member, e.node))
        .collect()
}

/// A paused or stopped member that holds a started task is told once, in
/// every case; its return ends the memory, so a later leave tells the lead
/// again; nothing else is sent over many passes and the task stays with
/// the member.
#[test]
fn a_paused_or_stopped_member_holding_a_started_task_is_told_once() {
    for how in ["paused", "stopped"] {
        let (mut h, mut a, mut p, lead) = team();
        // The first pass hands task 1 to m1 and the child submits it: the
        // task is in progress with m1.
        a.tick(&mut h.store, &mut p, TEAM, T0, false).unwrap();
        assert_eq!(effects(&h), [("submitted".into(), "m1".into(), 1)], "{how}");
        match how {
            "paused" => {
                h.run(
                    &lead,
                    Operation::PauseMember {
                        member: "m1".into(),
                    },
                )
                .unwrap();
            }
            "stopped" => {
                h.run(
                    &person(),
                    Operation::StopMember {
                        member: "m1".into(),
                    },
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        a.tick(&mut h.store, &mut p, TEAM, T0 + MINUTE, false)
            .unwrap();
        let told = items(&h);
        assert_eq!(told.len(), 1, "{how}: {told:?}");
        assert!(
            told[0].0.starts_with("member_left:1:m1:"),
            "{how}: {:?}",
            told[0].0
        );
        assert_eq!(
            told[0].1,
            format!(
                "Member m1 was {how} while task 1 is in progress; the work stays with the member until it is resumed or the person moves the task."
            ),
            "{how}"
        );
        // Nothing more while the state lasts, and the task stays with m1.
        for minute in 2..20 {
            a.tick(&mut h.store, &mut p, TEAM, T0 + minute * MINUTE, false)
                .unwrap();
        }
        assert_eq!(items(&h).len(), 1, "{how}: {:?}", items(&h));
        assert_eq!(effects(&h), [("submitted".into(), "m1".into(), 1)], "{how}");
        // The member returns and leaves again: a second item.
        h.run(
            &person(),
            Operation::ResumeMember {
                member: "m1".into(),
            },
        )
        .unwrap();
        a.tick(&mut h.store, &mut p, TEAM, T0 + 20 * MINUTE, false)
            .unwrap();
        assert_eq!(items(&h).len(), 1, "{how}: the return tells nothing");
        match how {
            "paused" => {
                h.run(
                    &lead,
                    Operation::PauseMember {
                        member: "m1".into(),
                    },
                )
                .unwrap();
            }
            "stopped" => {
                h.run(
                    &person(),
                    Operation::StopMember {
                        member: "m1".into(),
                    },
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        a.tick(&mut h.store, &mut p, TEAM, T0 + 21 * MINUTE, false)
            .unwrap();
        assert_eq!(items(&h).len(), 2, "{how}: the second leave tells again");
        assert!(items(&h)[1].0.starts_with("member_left:1:m1:"), "{how}");
    }
}

/// A member holding no started task gets no item from this path.
#[test]
fn a_member_holding_no_started_task_gets_no_item() {
    let (mut h, mut a, mut p, lead) = team();
    // m1 is paused before the first pass: nothing was handed out.
    h.run(
        &lead,
        Operation::PauseMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    for minute in 0..10 {
        a.tick(&mut h.store, &mut p, TEAM, T0 + minute * MINUTE, false)
            .unwrap();
    }
    assert!(items(&h).is_empty(), "{:?}", items(&h));
    assert!(effects(&h).is_empty());
}

/// The ops' own verification: a TellPausedStarted call for a member
/// without started work on the task writes nothing.
#[test]
fn a_tell_for_a_member_without_started_work_writes_nothing() {
    let mut h = Harness::new();
    let lead = h.bind("lead");
    h.bind("m1");
    h.bind("m2");
    h.add(&person(), 1);
    h.run(
        &lead,
        Operation::PauseMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    let request = request(
        h.store.team(TEAM).unwrap().unwrap().revision,
        Operation::TellPausedStarted {
            member: "m1".into(),
            node: 1,
        },
    );
    crate::teams::ops::apply(&mut h.store, &service(), &request, T0).unwrap();
    assert!(items(&h).is_empty(), "{:?}", items(&h));
}

/// A removed member keeps the removal's path: the removal's own item, never
/// the paused-started one.
#[test]
fn a_removed_member_keeps_the_removal_path() {
    let (mut h, mut a, mut p, lead) = team();
    a.tick(&mut h.store, &mut p, TEAM, T0, false).unwrap();
    h.run(
        &person(),
        Operation::ArchiveMember {
            member: "m1".into(),
        },
    )
    .unwrap();
    for minute in 1..10 {
        a.tick(&mut h.store, &mut p, TEAM, T0 + minute * MINUTE, false)
            .unwrap();
    }
    assert!(items(&h)
        .iter()
        .any(|(kind, _)| kind.starts_with("member_removed:")));
    assert!(items(&h)
        .iter()
        .all(|(kind, _)| !kind.starts_with("member_left:")));
}
