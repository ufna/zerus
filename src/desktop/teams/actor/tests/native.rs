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
    assert!(matches!(
        delivery_reply(
            Ended::Exited {
                code: 1,
                output: br#"{"schema":1,"id":"effect","state":"submitted","error":""}"#.to_vec()
            },
            "effect"
        ),
        HandoutOutcome::Uncertain { .. }
    ));
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
