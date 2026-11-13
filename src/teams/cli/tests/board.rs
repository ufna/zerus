use super::harness::*;
use super::verdict::{configured_stage, stage_harness, REVIEWER_SESSION};
use crate::teams::cli::actor::Caller;
use serde_json::json;
use zerus_team_plan::catalog::StageKind;

#[test]
fn queue_stage_metadata_comes_from_the_pinned_pipeline() {
    for (kind, name) in [
        (StageKind::Review, "review"),
        (StageKind::Test, "test"),
        (StageKind::Land, "land"),
    ] {
        let (h, stage) = configured_stage(kind, 5, None);
        let (reply, exit) = h.cli(
            &Caller::Session(key(LEAD_SESSION)),
            &format!("queue --team {TEAM} --limit 200"),
            "",
        );
        assert_eq!(exit, 0, "{reply}");
        let rows = reply["rows"].as_array().unwrap();
        let task = rows.iter().find(|n| n["id"] == 1).unwrap();
        let row = rows.iter().find(|n| n["id"] == stage).unwrap();
        assert_eq!(task["kind"], "task");
        assert_eq!(task["rounds"], 5);
        assert!(task.get("stage").is_none());
        assert_eq!(row["kind"], "stage");
        assert_eq!(row["rounds"], 5);
        assert_eq!(row["role"], "reviewer");
        assert_eq!(row["stage"], json!({"kind":name,"target":1,"round":1}));
        // The frozen checked round is not the target's later board round.
        let mut parent = h.store().node(TEAM, 1).unwrap().unwrap();
        parent.round = 8;
        h.store()
            .db
            .as_ref()
            .unwrap()
            .execute(
                "UPDATE nodes SET data=?3 WHERE team=?1 AND id=1",
                rusqlite::params![
                    TEAM,
                    1,
                    crate::teams::store::rows::node_payload(&parent).unwrap()
                ],
            )
            .unwrap();
        let (reply, exit) = h.cli(&Caller::Person, &format!("queue --team {TEAM}"), "");
        assert_eq!(exit, 0, "{reply}");
        assert_eq!(
            reply["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == stage)
                .unwrap()["stage"]["round"],
            1
        );
    }
}
#[test]
fn status_adds_the_roles_model_only_when_one_is_pinned() {
    for model in [None, Some("example-model")] {
        let (h, _) = configured_stage(StageKind::Review, 0, model);
        let (reply, exit) = h.cli(&Caller::Person, &format!("status --team {TEAM}"), "");
        assert_eq!(exit, 0, "{reply}");
        assert!(reply["team"].get("bundle").is_none());
        for row in reply["members"].as_array().unwrap() {
            match (row["role"].as_str(), model) {
                (Some("reviewer"), Some(model)) => assert_eq!(row["model"], model),
                _ => assert!(row.get("model").is_none()),
            }
        }
    }
}
#[test]
fn current_shows_the_stage_to_its_holder_and_not_to_its_implementer() {
    let (h, stage) = stage_harness();
    let (reply, exit) = h.cli(&Caller::Session(key(REVIEWER_SESSION)), "current", "");
    assert_eq!(exit, 0, "{reply}");
    assert!(reply["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["id"] == stage));
    let (reply, exit) = h.cli(&Caller::Session(key(M1_SESSION)), "current", "");
    assert_eq!(exit, 0, "{reply}");
    assert!(!reply["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["id"] == stage));
}

#[test]
fn current_follows_a_reassignment_and_never_the_plan_or_a_reservation() {
    use crate::teams::ops::{HandoutOutcome, Operation, TaskStage};
    let h = Harness::new();
    h.create();
    h.bind("lead", LEAD_SESSION);
    h.bind("m1", M1_SESSION);
    let m2 = "launch_id:00000000-0000-4000-8000-000000000032";
    h.bind("m2", m2);
    let node = h.add(1);
    let id = uid(0x7100);
    h.apply(
        &service(),
        Operation::HandoutIntent {
            node,
            member: "m1".into(),
            effect_request_id: id.clone(),
        },
    );
    for session in [M1_SESSION, m2] {
        let (reply, exit) = h.cli(&Caller::Session(key(session)), "current", "");
        assert_eq!(exit, 0);
        assert!(reply["nodes"].as_array().unwrap().is_empty());
    }
    h.apply(
        &service(),
        Operation::HandoutOutcome {
            effect_request_id: id,
            outcome: HandoutOutcome::Submitted {
                receipt: "Delivered".into(),
            },
        },
    );
    h.observe(node, TaskStage::InProgress);
    let (reply, exit) = h.cli(&Caller::Session(key(M1_SESSION)), "current", "");
    assert_eq!(exit, 0);
    assert_eq!(reply["nodes"][0]["generation"], 1);
    h.apply(
        &member("lead", LEAD_SESSION),
        Operation::Reassign {
            node,
            member: "m2".into(),
            reason: "Another implementer takes this round".into(),
        },
    );
    let (old, exit) = h.cli(&Caller::Session(key(M1_SESSION)), "current", "");
    assert_eq!(exit, 0);
    assert_eq!(old["nodes"][0]["id"], node);
    let (new, exit) = h.cli(&Caller::Session(key(m2)), "current", "");
    assert_eq!(exit, 0);
    assert!(new["nodes"].as_array().unwrap().is_empty());
    let id = uid(0x7101);
    h.apply(
        &service(),
        Operation::ReopenIntent {
            node,
            member: "m2".into(),
            effect_request_id: id.clone(),
        },
    );
    for session in [M1_SESSION, m2] {
        let (reply, exit) = h.cli(&Caller::Session(key(session)), "current", "");
        assert_eq!(exit, 0);
        assert!(reply["nodes"].as_array().unwrap().is_empty());
    }
    h.apply(
        &service(),
        Operation::ReopenOutcome {
            effect_request_id: id,
            outcome: HandoutOutcome::Submitted {
                receipt: "Reassigned".into(),
            },
        },
    );
    h.observe(node, TaskStage::InProgress);
    let (reply, exit) = h.cli(&Caller::Session(key(M1_SESSION)), "current", "");
    assert_eq!(exit, 0);
    assert!(reply["nodes"].as_array().unwrap().is_empty());
    let (reply, exit) = h.cli(&Caller::Session(key(m2)), "current", "");
    assert_eq!(exit, 0);
    assert_eq!(reply["nodes"][0]["id"], node);
    assert_eq!(reply["nodes"][0]["generation"], 2);
    h.observe(node, TaskStage::InReview);
    h.apply(
        &service(),
        Operation::RecordReport {
            node,
            generation: 2,
            member: "m2".into(),
            text: "Report of the new attempt".into(),
            commit: COMMIT.into(),
        },
    );
    h.observe(node, TaskStage::Todo);
    let (reply, exit) = h.cli(&Caller::Session(key(m2)), "current", "");
    assert_eq!(exit, 0);
    assert!(reply["nodes"].as_array().unwrap().is_empty());
}
#[test]
fn current_stops_after_three_pages_even_in_an_oversized_store() {
    let h = Harness::new();
    h.create();
    h.bind("m1", M1_SESSION);
    // A deliberately oversized old store must not turn current into an
    // unbounded inventory. All history rows here have a completed report.
    let store = h.store();
    let db = store.db.as_ref().unwrap();
    let node = crate::teams::store::rows::node_payload(&fixture().nodes[1]).unwrap();
    db.execute("WITH RECURSIVE ids(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM ids WHERE n<650) INSERT INTO node_identity(team,id,task_project,task_uuid) SELECT ?1,n,'example-project',printf('00000000-0000-4000-8000-%012x',n) FROM ids",[TEAM]).unwrap();
    db.execute("INSERT INTO nodes(team,id,data,rank,admitted,routed,task_revision,task_stage,task_cancelled) SELECT team,id,json_set(?2,'$.id',id,'$.task.uuid',task_uuid),printf('a%08d',id),1,1,'fixture','in_review',0 FROM node_identity WHERE team=?1",rusqlite::params![TEAM,node]).unwrap();
    db.execute("INSERT INTO effects(team,node,member,generation,request_id,actor,state,created_ms) SELECT team,id,'m1',1,task_uuid,'{}','reported',1 FROM node_identity WHERE team=?1",[TEAM]).unwrap();
    let (reply, exit) = h.cli(&Caller::Session(key(M1_SESSION)), "current", "");
    assert_eq!(exit, 0, "{reply}");
    assert_eq!(reply["nodes"].as_array().unwrap().len(), 600);
}

#[test]
fn row_kind_uses_the_node_kind_for_a_reviewer_task_and_developer_stage() {
    use crate::teams::ops::{HandoutOutcome, Operation, TaskStage};
    let h = Harness::new();
    let mut team = fixture().team;
    team.bundle.pipelines[0].implementer = "reviewer".into();
    team.bundle.pipelines[0].stages[0].role = "developer".into();
    h.apply(
        &person(),
        Operation::Create {
            expected_template: None,
            delegations: None,
            project: team.project,
            name: team.name,
            goal: team.goal,
            home: team.home,
            bundle: Box::new(team.bundle),
            integration_branch: team.integration_branch,
            base_commit: team.base_commit,
        },
    );
    h.bind("m3", REVIEWER_SESSION);
    let node = h.add(1);
    let id = uid(0x7500);
    h.apply(
        &service(),
        Operation::HandoutIntent {
            node,
            member: "m3".into(),
            effect_request_id: id.clone(),
        },
    );
    h.apply(
        &service(),
        Operation::HandoutOutcome {
            effect_request_id: id,
            outcome: HandoutOutcome::Submitted {
                receipt: "Task delivered".into(),
            },
        },
    );
    h.observe(node, TaskStage::InProgress);
    h.observe(node, TaskStage::InReview);
    h.apply(
        &service(),
        Operation::RecordReport {
            node,
            generation: 1,
            member: "m3".into(),
            text: "Task implemented by the reviewer role".into(),
            commit: COMMIT.into(),
        },
    );
    let (reply, exit) = h.cli(&Caller::Person, &format!("queue --team {TEAM}"), "");
    assert_eq!(exit, 0, "{reply}");
    let rows = reply["rows"].as_array().unwrap();
    let task = rows.iter().find(|r| r["role"] == "reviewer").unwrap();
    assert_eq!(task["kind"], "task");
    let stage = rows.iter().find(|r| r["role"] == "developer").unwrap();
    assert_eq!(stage["kind"], "stage");
    assert_eq!(stage["stage"]["kind"], "review");
}

/// The queue reply carries `pending_replaceable` beside `pending_decision`,
/// always: false with nothing pending, true for a Return that has not
/// started, false once its reopen is reserved.
#[test]
fn queue_rows_say_whether_the_pending_decision_can_be_replaced() {
    use crate::teams::ops::Operation;
    let h = seeded();
    let node = h.reported(1);
    let facts = |h: &Harness| {
        let (reply, exit) = h.cli(
            &Caller::Person,
            &format!("queue --team {TEAM} --limit 200"),
            "",
        );
        assert_eq!(exit, 0, "{reply}");
        let row = reply["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == node)
            .unwrap()
            .clone();
        (
            row["pending_decision"].clone(),
            row["pending_replaceable"].clone(),
        )
    };
    assert_eq!(facts(&h), (json!(null), json!(false)));
    h.apply(
        &person(),
        Operation::Return {
            node,
            reason: "Revise the label".into(),
        },
    );
    assert_eq!(facts(&h), (json!("return"), json!(true)));
    h.apply(
        &service(),
        Operation::ReopenIntent {
            node,
            member: "m1".into(),
            effect_request_id: uid(0x9001),
        },
    );
    assert_eq!(facts(&h), (json!("return"), json!(false)));
}

/// The queue reply carries `unstaffed`, always: null, then the sentence of
/// the lead's open item once no member can take the ready node.
#[test]
fn queue_rows_carry_the_unstaffed_sentence() {
    use crate::teams::ops::Operation;
    let h = seeded();
    let node = h.add(1);
    let unstaffed = |h: &Harness| {
        let (reply, exit) = h.cli(
            &Caller::Person,
            &format!("queue --team {TEAM} --limit 200"),
            "",
        );
        assert_eq!(exit, 0, "{reply}");
        reply["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == node)
            .unwrap()["unstaffed"]
            .clone()
    };
    assert_eq!(unstaffed(&h), json!(null));
    for member in ["m1", "m2"] {
        h.apply(
            &person(),
            Operation::ArchiveMember {
                member: member.into(),
            },
        );
    }
    h.apply(&service(), Operation::ReconcileUnstaffed);
    let text = unstaffed(&h);
    assert!(
        text.as_str()
            .unwrap()
            .contains("No member of the team has the role developer."),
        "{text}"
    );
}
