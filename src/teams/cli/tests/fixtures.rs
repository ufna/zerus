//! The read replies and an unknown write result, pinned as shared fixtures: the desktop's projections
//! are built from exactly the JSON a person's `hgs team` prints.
use super::harness::*;
use crate::teams::cli::actor::Caller;
use serde_json::Value;

const STATUS: &str = include_str!("../../../../crates/team-plan/fixtures/cli-status.json");
const QUEUE: &str = include_str!("../../../../crates/team-plan/fixtures/cli-queue.json");
const INBOX: &str = include_str!("../../../../crates/team-plan/fixtures/cli-inbox.json");
const CARDS: &str = include_str!("../../../../crates/team-plan/fixtures/cli-cards.json");
const JOURNAL: &str = include_str!("../../../../crates/team-plan/fixtures/cli-journal.json");
const LIST: &str = include_str!("../../../../crates/team-plan/fixtures/cli-list.json");
const WAITING: &str = include_str!("../../../../crates/team-plan/fixtures/cli-waiting.json");
const STORAGE_FAILED: &str =
    include_str!("../../../../crates/team-plan/fixtures/cli-storage-failed.json");

fn lead() -> Caller {
    Caller::Session(key(LEAD_SESSION))
}

fn replies() -> [(&'static str, Value); 8] {
    let harness = seeded();
    let (board, _) = super::verdict::configured_stage(
        zerus_team_plan::catalog::StageKind::Review,
        3,
        Some("example-model"),
    );
    let board_read = |caller: &Caller, line: &str| {
        let (reply, exit) = board.cli(caller, line, "");
        assert_eq!(exit, 0, "{line}: {reply}");
        reply
    };
    let read = |caller: &Caller, line: &str| {
        let (reply, exit) = harness.cli(caller, line, "");
        assert_eq!(exit, 0, "{line}: {reply}");
        reply
    };
    [
        (
            "cli-status.json",
            board_read(&Caller::Person, &format!("status --team {TEAM}")),
        ),
        (
            "cli-queue.json",
            board_read(&lead(), &format!("queue --team {TEAM}")),
        ),
        (
            "cli-inbox.json",
            read(&lead(), &format!("inbox --team {TEAM}")),
        ),
        (
            "cli-cards.json",
            read(&lead(), &format!("cards --team {TEAM}")),
        ),
        (
            "cli-journal.json",
            read(&lead(), &format!("journal --team {TEAM} --limit 200")),
        ),
        ("cli-list.json", read(&Caller::Person, "list")),
        ("cli-waiting.json", read(&Caller::Person, "waiting")),
        (
            "cli-storage-failed.json",
            super::storage_failure::failed_write(),
        ),
    ]
}

#[test]
fn the_read_fixtures_are_the_replies_the_cli_prints() {
    let pinned = [
        ("cli-status.json", STATUS),
        ("cli-queue.json", QUEUE),
        ("cli-inbox.json", INBOX),
        ("cli-cards.json", CARDS),
        ("cli-journal.json", JOURNAL),
        ("cli-list.json", LIST),
        ("cli-waiting.json", WAITING),
        ("cli-storage-failed.json", STORAGE_FAILED),
    ];
    let replies = replies();
    for ((name, actual), (pinned_name, text)) in replies.iter().zip(pinned) {
        assert_eq!(*name, pinned_name);
        let expected: Value =
            serde_json::from_str(text).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(actual, &expected, "{name} is not the reply this hgs prints");
        if *name == "cli-storage-failed.json" {
            assert_eq!(actual["ok"], false, "{name}");
            assert_eq!(actual["error"]["code"], "storage_failed", "{name}");
        } else {
            assert_eq!(actual["ok"], true, "{name}");
            assert!(actual.get("error").is_none(), "{name}");
        }
    }
}

/// Rewrites the fixtures from the seed. Run with:
/// `cargo +1.99.1 test --bin hgs teams:: -- --ignored regenerate_fixtures`
#[test]
#[ignore = "writes the shared fixtures; run only when a reply shape changes on purpose"]
fn regenerate_fixtures() {
    for (name, reply) in replies() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("crates/team-plan/fixtures")
            .join(name);
        let mut text = serde_json::to_string_pretty(&reply).unwrap();
        text.push('\n');
        std::fs::write(&path, text).unwrap();
    }
}
