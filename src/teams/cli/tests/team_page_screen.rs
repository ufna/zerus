//! The team page's archived case (zerus-63rru.198): a team the person stopped
//! and archived through the product's own operations, as the list shows it
//! under Show archived and as the page reads it by its ID.
use super::harness::*;
use crate::teams::cli::actor::Caller;
use crate::teams::ops::Operation;
use serde_json::{json, Value};
use zerus_client_core::teams::view;

fn read(h: &Harness, command: &str) -> Value {
    let (reply, exit) = h.cli(&Caller::Person, command, "");
    assert_eq!(exit, 0, "{reply}");
    reply
}

fn payload() -> Value {
    let h = Harness::new();
    let id = uid(0x6001);
    h.create_team(&id, "Archived team", "zerus/team/page-archived");
    h.bind_for(&id, "lead", &format!("launch_id:{}", uid(0x6101)));
    for operation in [Operation::Stop, Operation::Archive] {
        let reply = h.apply_for(&id, &person(), operation);
        assert!(reply.ok, "{reply:?}");
    }
    json!({
        "list": view::list(&read(&h, "list")),
        "team": view::team(&read(&h, &format!("status --team {id}")), &read(&h, &format!("queue --team {id}"))),
    })
}

fn path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("client/src/ui/screens/fixtures/team-page-archived.json")
}

#[test]
fn team_page_archived_fixture_is_written_by_product_operations_and_core_views() {
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(path()).unwrap()).unwrap();
    let actual = payload();
    assert_eq!(actual, expected);
    assert_eq!(actual["list"]["teams"][0]["status"], "archived");
    assert_eq!(actual["team"]["bar"]["status"], "archived");
}

#[test]
#[ignore = "regenerates the team page's archived fixture from the product writers"]
fn regenerate_team_page_archived_fixture() {
    let path = path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(&payload()).unwrap()),
    )
    .unwrap();
}
