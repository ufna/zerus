use super::super::native::*;
use crate::teams::ops::HandoutOutcome;
#[test]
fn literal_delivery_replies_preserve_each_authoritative_state() {
    assert!(matches!(
        decode_delivery(
            br#"{"schema":1,"id":"effect","state":"submitted","error":"","task":{"id":"a"}}"#,
            "effect"
        ),
        HandoutOutcome::Submitted { .. }
    ));
    assert!(matches!(
        decode_delivery(
            br#"{"schema":1,"id":"effect","state":"refused","error":"Busy"}"#,
            "effect"
        ),
        HandoutOutcome::Refused { .. }
    ));
    assert!(matches!(
        decode_delivery(
            br#"{"schema":1,"id":"effect","state":"uncertain","error":"Lost"}"#,
            "effect"
        ),
        HandoutOutcome::Uncertain { .. }
    ));
    assert!(matches!(
        decode_delivery(
            br#"{"schema":1,"id":"effect","state":"cutting_over","error":"Cutover"}"#,
            "effect"
        ),
        HandoutOutcome::CuttingOver { .. }
    ));
    for raw in [
        b"{}".as_slice(),
        br#"{"state":"submitted","schema":2}"#,
        br#"{"status":"submitted"}"#,
        b"not json",
    ] {
        assert!(matches!(
            decode_delivery(raw, "effect"),
            HandoutOutcome::Uncertain { .. }
        ));
    }
}
#[test]
fn reply_of_another_effect_is_never_a_delivery_receipt() {
    assert!(matches!(
        decode_delivery(
            br#"{"schema":1,"id":"other","state":"submitted","error":""}"#,
            "effect"
        ),
        HandoutOutcome::Uncertain { .. }
    ));
}
#[test]
fn literal_finishing_replies_echo_the_team_and_node_and_have_no_uncertain_state() {
    use super::super::ports::Finish;
    assert_eq!(
        decode_finish(
            br#"{"schema":1,"team":"team","node":7,"state":"done","error":""}"#,
            "team",
            7
        )
        .unwrap(),
        Finish::Done
    );
    assert_eq!(
        decode_finish(
            br#"{"schema":1,"team":"team","node":7,"state":"refused","error":"No accept"}"#,
            "team",
            7
        )
        .unwrap(),
        Finish::Refused("No accept".into())
    );
    assert_eq!(
        decode_finish(
            br#"{"schema":1,"team":"team","node":7,"state":"cutting_over","error":"Held"}"#,
            "team",
            7
        )
        .unwrap(),
        Finish::CuttingOver("Held".into())
    );
    for (team, node, state) in [
        ("other", 7, "done"),
        ("team", 8, "done"),
        ("team", 7, "uncertain"),
        ("team", 7, "new"),
    ] {
        let raw = serde_json::json!({"schema":1,"team":team,"node":node,"state":state,"error":""})
            .to_string();
        assert!(decode_finish(raw.as_bytes(), "team", 7).is_err());
    }
    assert!(decode_finish(b"{}", "team", 7).is_err());
    assert!(decode_finish(b"bad", "team", 7).is_err());
}
#[test]
fn delivery_requires_the_echoed_id_and_version_and_never_accepts_a_failed_child() {
    use crate::desktop::teams::child::Ended;
    for raw in [
        br#"{"schema":1,"state":"submitted","error":""}"#.as_slice(),
        br#"{"id":"effect","state":"submitted","error":""}"#,
    ] {
        assert!(matches!(
            decode_delivery(raw, "effect"),
            HandoutOutcome::Uncertain { .. }
        ));
    }
    for state in ["submitted", "refused", "uncertain", "cutting_over", "stale"] {
        let output = serde_json::json!({"schema":1,"id":"effect","state":state,"error":"Reason"})
            .to_string()
            .into_bytes();
        assert!(
            matches!(
                delivery_reply(Ended::Exited { code: 1, output }, "effect"),
                HandoutOutcome::Uncertain { .. }
            ),
            "{state}"
        );
    }
}

#[test]
fn adapter_argument_vectors_use_the_shared_builders() {
    let task = zerus_team_plan::model::TaskRef {
        project: "project-1".into(),
        uuid: "00000000-0000-4000-8000-000000000001".into(),
    };
    let occupancy = occupancy_arguments("project-1", "run_id:review-run");
    assert!(occupancy.windows(2).any(|a| a == ["--stage", "active"]));
    let deliver = task_arguments(&task, "team-deliver");
    let done = task_arguments(&task, "team-done");
    assert_eq!(deliver[1], "team-deliver");
    assert_eq!(done[1], "team-done");
    assert_eq!(deliver[2], "project-1/00000000-0000-4000-8000-000000000001");
    println!(
        "TEAM_ACTOR_ARGUMENTS={}",
        serde_json::json!({"occupancy":occupancy,"deliver":deliver,"done":done})
    );
}

#[test]
fn production_occupancy_excludes_busy_tasks_and_unknown_indexes() {
    use super::*;
    let mut h = Harness::new();
    h.bind("m1");
    let n = super::production::native(&h);
    let member = h.store.member(TEAM, "m1").unwrap().unwrap();
    TEST_INVENTORY.with(|s|*s.borrow_mut()=Some(serde_json::json!({"node_id":n.machine,"organization":{"projects":[{"id":"project-1"}]}})));
    for (reply, expected) in [
        (
            serde_json::json!({"damaged_total":0,"tasks":[{"id":"busy"}]}),
            Some(false),
        ),
        (
            serde_json::json!({"damaged_total":0,"tasks":[]}),
            Some(true),
        ),
        (serde_json::json!({"damaged_total":1,"tasks":[]}), None),
    ] {
        crate::desktop::teams::child::FIXTURE_EXIT
            .with(|s| *s.borrow_mut() = Some(reply.to_string().into_bytes()));
        assert_eq!(n.unoccupied(&member).ok(), expected);
    }
    TEST_INVENTORY.with(|s| *s.borrow_mut() = None);
}
#[test]
fn production_delivery_and_done_refuse_a_pause_before_the_spawn() {
    use super::*;
    for done in [false, true] {
        let mut h = Harness::new();
        let (node, id, decision) = if done {
            h.settings(|s| s.integration = Integration::None);
            let (node, lead) = h.reported();
            h.run(&lead, crate::teams::ops::Operation::Accept { node })
                .unwrap();
            (
                node,
                uuid::Uuid::new_v4().to_string(),
                Some(
                    h.store
                        .pending_accepts(TEAM, 200, None)
                        .unwrap()
                        .rows
                        .remove(0),
                ),
            )
        } else {
            h.bind("m1");
            let node = h.add(&person(), 1);
            (node, h.intent(node, "m1"), None)
        };
        let team = h.store.team(TEAM).unwrap().unwrap();
        let row = h.store.node(TEAM, node).unwrap().unwrap();
        let member = h.store.member(TEAM, "m1").unwrap().unwrap();
        h.run(&person(), crate::teams::ops::Operation::Pause)
            .unwrap();
        let mut native = super::production::native(&h);
        let reply = if done {
            serde_json::json!({"schema":1,"team":TEAM,"node":node,"state":"done","error":""})
        } else {
            serde_json::json!({"schema":1,"id":id,"state":"submitted","error":""})
        };
        crate::desktop::teams::child::FIXTURE_EXIT
            .with(|s| *s.borrow_mut() = Some(reply.to_string().into_bytes()));
        if let Some(d) = decision {
            assert!(super::super::ports::Ports::finish(&mut native, &team, &row, &d, &id).is_err());
        } else {
            assert!(matches!(
                super::super::ports::Ports::deliver(&mut native, &team, &row, &member, &id),
                super::super::ports::DeliveryResult::Outcome(HandoutOutcome::Refused { .. })
            ));
        }
    }
}
#[test]
fn production_command_keeps_its_transaction_until_spawn_returns() {
    use super::*;
    use crate::desktop::original_child;
    for done in [false, true] {
        let mut h = Harness::new();
        let (node, id, decision) = if done {
            h.settings(|s| s.integration = Integration::None);
            let (node, lead) = h.reported();
            h.run(&lead, crate::teams::ops::Operation::Accept { node })
                .unwrap();
            (
                node,
                uuid::Uuid::new_v4().to_string(),
                Some(
                    h.store
                        .pending_accepts(TEAM, 200, None)
                        .unwrap()
                        .rows
                        .remove(0),
                ),
            )
        } else {
            h.bind("m1");
            let node = h.add(&person(), 1);
            (node, h.intent(node, "m1"), None)
        };
        let team = h.store.team(TEAM).unwrap().unwrap();
        let row = h.store.node(TEAM, node).unwrap().unwrap();
        let member = h.store.member(TEAM, "m1").unwrap().unwrap();
        let mut native = super::production::native(&h);
        let path = h.temp.path().join("teams/teams-v1.sqlite3");
        let before = path.clone();
        let checked = std::rc::Rc::new(std::cell::Cell::new(0));
        let seen = checked.clone();
        original_child::AFTER_FINAL_CHECK.with(|s| {
            *s.borrow_mut() = Some(Box::new(move || {
                let db = rusqlite::Connection::open(before).unwrap();
                db.busy_timeout(std::time::Duration::ZERO).unwrap();
                assert!(
                    db.execute_batch("BEGIN IMMEDIATE").is_err(),
                    "admission released before spawn"
                );
                seen.set(1);
            }))
        });
        let seen = checked.clone();
        original_child::AFTER_SPAWN.with(|s| {
            *s.borrow_mut() = Some(Box::new(move || {
                let db = rusqlite::Connection::open(path).unwrap();
                db.execute_batch("BEGIN IMMEDIATE;ROLLBACK").unwrap();
                seen.set(2);
            }))
        });
        let reply = if done {
            serde_json::json!({"schema":1,"team":TEAM,"node":node,"state":"done","error":""})
        } else {
            serde_json::json!({"schema":1,"id":id,"state":"submitted","error":""})
        };
        crate::desktop::teams::child::FIXTURE_EXIT
            .with(|s| *s.borrow_mut() = Some(reply.to_string().into_bytes()));
        if let Some(d) = decision {
            assert!(super::super::ports::Ports::finish(&mut native, &team, &row, &d, &id).is_ok());
        } else {
            assert!(matches!(
                super::super::ports::Ports::deliver(&mut native, &team, &row, &member, &id),
                super::super::ports::DeliveryResult::Outcome(HandoutOutcome::Submitted { .. })
            ));
        }
        assert_eq!(checked.get(), 2);
    }
}

#[test]
fn the_real_delivery_success_envelope_may_omit_the_error_field() {
    assert!(matches!(decode_delivery(
        br#"{"schema":1,"id":"effect","team":"team","node":1,"member":"m1","state":"submitted"}"#,
        "effect"), HandoutOutcome::Submitted { .. }));
    for state in ["refused", "uncertain", "cutting_over"] {
        let raw = serde_json::json!({"schema":1,"id":"effect","state":state}).to_string();
        assert!(matches!(
            decode_delivery(raw.as_bytes(), "effect"),
            HandoutOutcome::Uncertain { .. }
        ));
    }
}

#[test]
fn literal_stale_replies_are_positive_nonwrites_only_for_the_exact_successful_child() {
    use super::super::ports::{DeliveryResult, Finish};
    use crate::desktop::teams::child::Ended;
    let raw = br#"{"schema":1,"id":"effect","state":"stale","error":"Observe first"}"#.to_vec();
    assert!(matches!(
        delivery_result(
            Ended::Exited {
                code: 0,
                output: raw.clone()
            },
            "effect"
        ),
        DeliveryResult::Stale
    ));
    for (code, id) in [(1, "effect"), (0, "other")] {
        assert!(matches!(
            delivery_result(
                Ended::Exited {
                    code,
                    output: raw.clone()
                },
                id
            ),
            DeliveryResult::Outcome(HandoutOutcome::Uncertain { .. })
        ));
    }
    assert_eq!(
        decode_finish(
            br#"{"schema":1,"team":"team","node":1,"state":"stale","error":"Observe first"}"#,
            "team",
            1
        )
        .unwrap(),
        Finish::Stale
    );
}

#[test]
fn project_limit_refusals_keep_the_task_commands_stable_reason_unchanged() {
    for reason in [
        "project_agents_other: This project allows agents to start tasks only in their own session.",
        "project_agent_limit: The project's hourly agent start limit is reached.",
    ] {
        let raw = serde_json::json!({"schema":1,"id":"effect","state":"refused","error":reason}).to_string();
        match decode_delivery(raw.as_bytes(), "effect") {
            HandoutOutcome::Refused {reason: actual} => assert_eq!(actual, reason),
            outcome => panic!("rule refusal was lost: {outcome:?}"),
        }
    }
}

/// A hand-out command's own sentence is recorded cleaned: decoded by the
/// production deliver, recorded through the outcome operation the actor
/// applies, read back from the store.
#[test]
fn a_delivery_commands_sentence_is_recorded_without_control_characters() {
    use super::*;
    for state in ["refused", "uncertain", "cutting_over"] {
        let mut h = Harness::new();
        h.bind("m1");
        let node = h.add(&person(), 1);
        let id = h.intent(node, "m1");
        let team = h.store.team(TEAM).unwrap().unwrap();
        let row = h.store.node(TEAM, node).unwrap().unwrap();
        let member = h.store.member(TEAM, "m1").unwrap().unwrap();
        let mut native = super::production::native(&h);
        let reply =
            serde_json::json!({"schema":1,"id":id,"state":state,"error":super::hostile_sentence()});
        crate::desktop::teams::child::FIXTURE_EXIT
            .with(|s| *s.borrow_mut() = Some(reply.to_string().into_bytes()));
        let super::super::ports::DeliveryResult::Outcome(outcome) =
            super::super::ports::Ports::deliver(&mut native, &team, &row, &member, &id)
        else {
            panic!("{state}: no outcome")
        };
        h.outcome(&id, outcome);
        let recorded = h
            .store
            .effects(TEAM, 200, None)
            .unwrap()
            .rows
            .into_iter()
            .find(|e| e.request_id == id)
            .and_then(|e| e.outcome)
            .map(|v| serde_json::from_value::<HandoutOutcome>(v).unwrap())
            .expect("the recorded outcome");
        let reason = match recorded {
            HandoutOutcome::Refused { reason }
            | HandoutOutcome::Uncertain { reason }
            | HandoutOutcome::CuttingOver { reason } => reason,
            other => panic!("{state}: {other:?}"),
        };
        super::assert_clean(&[reason], state);
        let items = h.store.inbox(TEAM, 200, None).unwrap().rows;
        // Items keep their own line breaks; no other control character.
        assert!(
            items
                .iter()
                .all(|i| !i.text.chars().any(|c| c.is_control() && c != '\n')),
            "{state}: an inbox item carries a control character"
        );
    }
}
