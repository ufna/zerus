use super::*;
use crate::teams::ops::{
    self,
    tests::{person, Harness, TEAM},
};

fn setup() -> (Harness, Input) {
    let mut h = Harness::new();
    h.bind("lead");
    h.bind("m1");
    let node = h.add(&person(), 1);
    let id = h.intent(node, "m1");
    (
        h,
        Input {
            schema: 1,
            id,
            team: TEAM.into(),
            node,
            member: "m1".into(),
        },
    )
}

#[test]
fn exact_stored_effect_is_the_authority_without_a_caller() {
    let (h, body) = setup();
    let home = h.store.team(TEAM).unwrap().unwrap().home;
    let before = h.store.team(TEAM).unwrap().unwrap().revision;
    let bound = permit(&h.store, &body, &home).unwrap();
    assert_eq!(bound.kind, "handout");
    assert_eq!(bound.task.uuid, ops::tests::task(1).uuid);
    assert_eq!(h.store.team(TEAM).unwrap().unwrap().revision, before);
}

#[test]
fn every_effect_binding_refusal_is_read_only() {
    for change in 0..13 {
        let (h, mut body) = setup();
        let mut home = h.store.team(TEAM).unwrap().unwrap().home;
        let db = h.store.db.as_ref().unwrap();
        match change {
            0 => body.id = uuid::Uuid::new_v4().to_string(),
            1 => body.team = uuid::Uuid::new_v4().to_string(),
            2 => body.node += 1,
            3 => body.member = "m2".into(),
            4 => {
                db.execute("UPDATE effects SET kind='other'", []).unwrap();
            }
            5 => {
                db.execute("UPDATE effects SET generation=generation+1", [])
                    .unwrap();
            }
            6 => {
                db.execute("UPDATE effects SET state='closed'", []).unwrap();
            }
            7 => home = uuid::Uuid::new_v4().to_string(),
            8 => {
                db.execute("UPDATE teams SET status='\"paused\"'", [])
                    .unwrap();
            }
            9 => {
                db.execute(
                    "UPDATE members SET data=json_remove(data,'$.session') WHERE id='m1'",
                    [],
                )
                .unwrap();
            }
            10 => {
                db.execute(
                    "UPDATE members SET data=json_set(data,'$.state','paused') WHERE id='m1'",
                    [],
                )
                .unwrap();
            }
            11 => {
                db.execute(
                    "UPDATE members SET data=json_remove(data,'$.session') WHERE id='lead'",
                    [],
                )
                .unwrap();
            }
            12 => {
                db.execute(
                    "UPDATE node_identity SET task_uuid='00000000-0000-4000-8000-000000000099'",
                    [],
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let before = h.store.team(TEAM).unwrap().unwrap().revision;
        assert!(permit(&h.store, &body, &home).is_err(), "mutation {change}");
        assert_eq!(h.store.team(TEAM).unwrap().unwrap().revision, before);
    }
}

#[test]
fn request_rejects_unknown_fields_and_invalid_identity() {
    let (_, body) = setup();
    let mut value = serde_json::to_value(body).unwrap();
    value["actor"] = serde_json::json!({"kind":"service"});
    assert!(serde_json::from_value::<Input>(value).is_err());
}

#[cfg(unix)]
mod processes;
