//! The reads: their shape, their bounded pages and who may run them.
use super::harness::*;
use crate::teams::cli::actor::Caller;
use serde_json::Value;

fn person_caller() -> Caller {
    Caller::Person
}

fn lead_caller() -> Caller {
    Caller::Session(key(LEAD_SESSION))
}

fn unbound_caller() -> Caller {
    Caller::Session(key(OTHER_SESSION))
}

fn status(harness: &Harness, caller: &Caller) -> Value {
    let (reply, exit) = harness.cli(caller, &format!("status --team {TEAM}"), "");
    assert_eq!(exit, 0, "{reply}");
    reply
}

#[test]
fn status_carries_the_team_its_members_and_counts_but_not_the_bundle() {
    let harness = seeded();
    let reply = status(&harness, &person_caller());
    assert_eq!(reply["ok"], true);
    assert_eq!(reply["team"]["id"], TEAM);
    assert_eq!(reply["team"]["name"], "Fixes");
    assert!(reply["team"].get("bundle").is_none());
    assert_eq!(reply["members"].as_array().unwrap().len(), 4);
    assert_eq!(reply["counts"]["members"]["active"], 2);
    assert_eq!(reply["counts"]["members"]["not_created"], 2);
    assert_eq!(reply["counts"]["nodes"]["pending"], 1);
    assert_eq!(reply["counts"]["nodes"]["reported"], 1);
    assert_eq!(reply["counts"]["inbox"]["open"], 1);
    assert_eq!(reply["counts"]["cards"]["open"], 1);
    assert_eq!(reply["actor"]["origin"], "cli_interim");
    let (reply, _) = harness.cli(
        &person_caller(),
        &format!("status --team {TEAM} --bundle"),
        "",
    );
    assert_eq!(reply["team"]["bundle"]["template"]["id"], "bugfix");
}

#[test]
fn every_lead_list_holds_one_page_and_names_the_next_cursor() {
    let harness = seeded();
    for (line, rows) in [
        (format!("queue --team {TEAM} --limit 1"), 1usize),
        (format!("inbox --team {TEAM}"), 2),
        (format!("cards --team {TEAM}"), 1),
        (format!("journal --team {TEAM} --limit 3"), 3),
    ] {
        let (reply, exit) = harness.cli(&lead_caller(), &line, "");
        assert_eq!(exit, 0, "{line}: {reply}");
        let list = reply["rows"].as_array().unwrap();
        assert_eq!(list.len(), rows, "{line}");
        if line.contains("--limit 1") || line.contains("--limit 3") {
            assert!(reply["next"].is_string(), "{line} pages on");
        }
    }
}

#[test]
fn a_page_stays_stable_while_rows_are_added() {
    let harness = Harness::new();
    harness.create();
    harness.bind("lead", LEAD_SESSION);
    for n in 1..=3 {
        harness.add(n);
    }
    let (first, exit) = harness.cli(
        &lead_caller(),
        &format!("queue --team {TEAM} --limit 2"),
        "",
    );
    assert_eq!(exit, 0, "{first}");
    let cursor = first["next"].as_str().expect("a next page").to_owned();
    let first_ids: Vec<u64> = first["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_u64().unwrap())
        .collect();
    assert_eq!(first_ids, [1, 2]);
    harness.add(4);
    let (second, exit) = harness.cli(
        &lead_caller(),
        &format!("queue --team {TEAM} --limit 2 --after {cursor}"),
        "",
    );
    assert_eq!(exit, 0, "{second}");
    let second_ids: Vec<u64> = second["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_u64().unwrap())
        .collect();
    assert_eq!(second_ids, [3], "the first page's ceiling stays");

    let (reply, exit) = harness.cli(
        &lead_caller(),
        &format!("inbox --team {TEAM} --limit 2 --after {cursor}"),
        "",
    );
    assert_eq!((code(&reply), exit), ("invalid_state", 1), "{reply}");
    let (reply, _) = harness.cli(
        &lead_caller(),
        &format!("queue --team {TEAM} --limit 201"),
        "",
    );
    assert_eq!(code(&reply), "over_cap");
}

#[test]
fn a_member_and_an_unbound_session_do_not_read_the_leads_lists() {
    let harness = seeded();
    for line in ["inbox", "queue", "cards", "journal"] {
        let (reply, exit) = harness.cli(
            &Caller::Session(key(M1_SESSION)),
            &format!("{line} --team {TEAM}"),
            "",
        );
        assert_eq!((code(&reply), exit), ("not_allowed", 1), "{line} as m1");
        let (reply, exit) = harness.cli(&unbound_caller(), &format!("{line} --team {TEAM}"), "");
        assert_eq!(
            (code(&reply), exit),
            ("not_allowed", 1),
            "{line} as another session"
        );
    }
    // Any other session may read status; naming the team is required.
    let reply = status(&harness, &unbound_caller());
    assert_eq!(reply["team"]["id"], TEAM);
    let (reply, _) = harness.cli(&unbound_caller(), "status", "");
    assert_eq!(code(&reply), "not_found");
    let (reply, _) = harness.cli(&person_caller(), "status", "");
    assert_eq!(code(&reply), "not_found", "the person names the team");
}

#[test]
fn the_inbox_and_the_journal_name_the_report_and_the_answer() {
    let harness = seeded();
    let (reply, _) = harness.cli(&lead_caller(), &format!("inbox --team {TEAM}"), "");
    let rows = reply["rows"].as_array().unwrap();
    assert_eq!(rows[0]["kind"], "report");
    assert_eq!(rows[0]["state"], "open");
    assert_eq!(rows[0]["node"], 2);
    assert_eq!(rows[1]["state"], "closed");
    assert_eq!(
        rows[1]["reply"],
        "Keep it, and say so in the commit message."
    );

    let (reply, _) = harness.cli(
        &lead_caller(),
        &format!("journal --team {TEAM} --limit 200"),
        "",
    );
    let kinds: Vec<&str> = reply["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["kind"].as_str().unwrap())
        .collect();
    for kind in [
        "create",
        "set_member_session",
        "add_task",
        "observe_task",
        "record_report",
    ] {
        assert!(kinds.contains(&kind), "{kind} is missing from {kinds:?}");
    }
}

#[test]
fn only_a_session_reads_the_current_work() {
    let harness = seeded();
    let (reply, exit) = harness.cli(&Caller::Session(key(LEAD_SESSION)), "current", "");
    assert_eq!(exit, 0, "{reply}");
    assert_eq!(reply["member"]["id"], "lead");
    assert!(reply["nodes"].as_array().unwrap().is_empty());
    let (reply, exit) = harness.cli(&Caller::Session(key(M1_SESSION)), "current", "");
    assert_eq!(exit, 0, "{reply}");
    let nodes = reply["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["implementer"], "m1");
    assert_eq!(nodes[0]["state"], "reported");
    for caller in [person_caller(), unbound_caller()] {
        let (reply, exit) = harness.cli(&caller, "current", "");
        assert_eq!((code(&reply), exit), ("not_allowed", 1), "{reply}");
    }
}

#[test]
fn a_read_refuses_words_it_does_not_take() {
    let harness = seeded();
    for (line, expected) in [
        (format!("status --team {TEAM} now"), "invalid_state"),
        (format!("status --team {TEAM} --limit 5"), "invalid_state"),
        ("status --team nope".into(), "invalid_state"),
        ("status --colour red".into(), "invalid_state"),
        (
            "queue --team 00000000-0000-4000-8000-00000000beee".to_owned(),
            "not_found",
        ),
    ] {
        let (reply, exit) = harness.cli(&lead_caller(), &line, "");
        assert_eq!((code(&reply), exit), (expected, 1), "{line}: {reply}");
    }
}
