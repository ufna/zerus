//! zerus-63rru.45: a stage that loses every member who could take it goes the
//! way a stage nobody could take goes at its creation.
use super::*;
use std::collections::BTreeSet;

fn reconcile(h: &mut Harness) -> Result<Reply> {
    h.run(&service(), Operation::ReconcileUnstaffed)
}
/// A created stage task, observed To do and not handed out.
fn created_stage(h: &mut Harness, parent: u32) -> u32 {
    let stage = stages(h, parent)[0].id;
    let id = uuid::Uuid::new_v4().to_string();
    h.run(&service(), creation(stage, &id)).unwrap();
    h.run(&service(), created(&id, 1000 + stage)).unwrap();
    h.observe(stage, TaskStage::Todo);
    stage
}
fn remove(h: &mut Harness, member: &str) {
    h.run(
        &person(),
        Operation::ArchiveMember {
            member: member.into(),
        },
    )
    .unwrap();
}
fn run_state(h: &Harness, parent: u32) -> String {
    h.store
        .db
        .as_ref()
        .unwrap()
        .query_row(
            "SELECT state FROM pipeline_runs WHERE team=?1 AND node=?2",
            rusqlite::params![TEAM, parent],
            |r| r.get(0),
        )
        .unwrap()
}
fn report_items(h: &Harness, node: u32) -> Vec<String> {
    h.store
        .inbox(TEAM, 200, None)
        .unwrap()
        .rows
        .into_iter()
        .filter(|i| i.node == node && i.kind == "report" && i.state == "open")
        .map(|i| i.text)
        .collect()
}
fn unstaffed_items(h: &Harness) -> usize {
    h.store
        .inbox(TEAM, 200, None)
        .unwrap()
        .rows
        .into_iter()
        .filter(|i| i.kind.starts_with("unstaffed:"))
        .count()
}
/// The kernel's set before the open-effect rule, as the store reads it.
fn kernel_set(h: &Harness) -> Vec<u32> {
    let team = h.store.team(TEAM).unwrap().unwrap();
    let r = crate::teams::ops::readiness(&h.store, TEAM).unwrap();
    let members = h.store.members(TEAM, 200, None).unwrap().rows;
    crate::teams::ops::unstaffed::unstaffed_of(&team, &r, &members, &BTreeSet::new())
        .into_iter()
        .map(|u| u.node)
        .collect()
}

/// The stage's reviewer is removed after its creation: the reconcile
/// retires the stage and hands the parent to the lead with the kernel's
/// reason, exactly as a reviewer removed before the creation does: the same
/// run state, the same report item and text, no unstaffed item.
#[test]
fn an_unstaffed_stage_is_retired_and_its_parent_handed_to_the_lead_as_at_creation() {
    // At creation: the reviewer is removed before the report.
    let mut at_creation = pipeline_harness("reviewed");
    at_creation.bind("m1");
    at_creation.bind("m3");
    remove(&mut at_creation, "m3");
    let first = at_creation.add(&person(), 1);
    at_creation.submit(first, "m1");
    at_creation.report(first, "m1");
    assert!(stages(&at_creation, first).is_empty());
    let expected = report_items(&at_creation, first);
    assert_eq!(expected.len(), 1);
    // Later: the stage exists, then its reviewer is removed.
    let (mut h, parent, _) = wanted();
    let stage = created_stage(&mut h, parent);
    assert_eq!(run_state(&h, parent), "active");
    remove(&mut h, "m3");
    assert_eq!(kernel_set(&h), vec![stage]);
    reconcile(&mut h).unwrap();
    assert!(stages(&h, parent).is_empty(), "the stage is retired");
    assert_eq!(run_state(&h, parent), "lead");
    assert!(!h.node(parent).hold);
    assert_eq!(report_items(&h, parent), expected);
    assert!(expected[0].contains("No member of the team has the role reviewer."));
    assert_eq!(unstaffed_items(&h), 0, "no unstaffed item for a stage");
    let e = reconcile(&mut h).unwrap_err();
    assert_eq!(e.code, code::UNSTAFFED_UNCHANGED);
}

/// A stage that is pending a cancel, held, still has a member who could take
/// it (paused, or Active and busy), or has an open effect is never in the
/// set, so the reconcile never retires it.
#[test]
fn a_stage_that_is_not_ready_or_still_staffed_is_never_retired() {
    for case in ["pending_cancel", "held", "paused", "busy", "open_effect"] {
        let (mut h, parent, _) = wanted();
        let stage = created_stage(&mut h, parent);
        match case {
            "pending_cancel" => {
                h.run(
                    &person(),
                    Operation::Cancel {
                        node: stage,
                        reason: "Stop this review".into(),
                    },
                )
                .unwrap();
                remove(&mut h, "m3");
            }
            "held" => {
                h.run(&person(), Operation::Hold { node: stage }).unwrap();
                remove(&mut h, "m3");
            }
            "paused" => {
                h.run(
                    &person(),
                    Operation::PauseMember {
                        member: "m3".into(),
                    },
                )
                .unwrap();
            }
            "busy" => {}
            _ => {
                h.run(
                    &service(),
                    Operation::HandoutIntent {
                        node: stage,
                        member: "m3".into(),
                        effect_request_id: uuid::Uuid::new_v4().to_string(),
                    },
                )
                .unwrap();
                remove(&mut h, "m3");
                // The kernel alone would name it; the open effect keeps it out.
                assert_eq!(kernel_set(&h), vec![stage], "{case}");
            }
        }
        let e = reconcile(&mut h).unwrap_err();
        assert_eq!(e.code, code::UNSTAFFED_UNCHANGED, "{case}");
        assert_eq!(
            stages(&h, parent).iter().map(|n| n.id).collect::<Vec<_>>(),
            vec![stage],
            "{case}: the stage stays"
        );
        assert_eq!(run_state(&h, parent), "active", "{case}");
        assert_eq!(unstaffed_items(&h), 0, "{case}");
    }
}
