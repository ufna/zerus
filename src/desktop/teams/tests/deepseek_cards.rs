use super::*;
use crate::teams::ops::Operation;
use ops::tests::{person, TEAM};
use serde_json::{json, Value};

fn add_deepseek_member(h: &mut Harness) -> String {
    h.run(
        &person(),
        Operation::AddMember {
            role: "developer".into(),
            agent: "dsh".into(),
            model: String::new(),
            effort: String::new(),
            account: AccountChoice::Freest,
        },
    )
    .unwrap()
    .member
    .unwrap()
}

fn waiting_fixture(lead: bool) -> (Harness, FakeEffects, String, Value) {
    let (mut h, member) = if lead {
        let mut h = Harness::empty();
        let mut create = ops::tests::create();
        if let Operation::Create { bundle, .. } = &mut create {
            bundle.template.lead.agent = "dsh".into();
        }
        h.run(&person(), create).unwrap();
        (h, "lead".into())
    } else {
        let mut h = Harness::new();
        let member = add_deepseek_member(&mut h);
        (h, member)
    };
    let mut dsh_account = account("native-dsh");
    dsh_account.agent = "dsh".into();
    let mut effects = FakeEffects {
        older_deepseek_host: true,
        accounts: vec![account("a"), account("b"), dsh_account],
        ..Default::default()
    };
    let mut service = runtime(&h);
    advance(&mut service, &mut h, &mut effects, 45);
    assert_eq!(row(&h, &member).state, State::Blocked);
    let mut waiting = h.store.waiting(10, None).unwrap().rows;
    assert_eq!(waiting.len(), 1);
    let waiting = waiting.pop().unwrap();
    let mut card = serde_json::to_value(waiting.card).unwrap();
    card["team"] = json!(waiting.team);
    let view = zerus_client_core::teams::view::waiting(&json!({ "rows": [card] }));
    let item = serde_json::to_value(view).unwrap()["items"][0].clone();
    (h, effects, member, item)
}

#[test]
#[ignore = "captures the real service and core Waiting cards for the renderer fixtures"]
fn capture_older_deepseek_host_waiting_cards() {
    println!("MEMBER_CARD {}", waiting_fixture(false).3);
    println!("LEAD_CARD {}", waiting_fixture(true).3);
}

#[test]
fn deepseek_host_cards_match_the_waiting_view_fixtures() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../../../client/src/ui/teams/fixtures/deepseek-host-cards.json"
    ))
    .unwrap();
    assert_eq!(waiting_fixture(false).3, fixtures["member"]);
    assert_eq!(waiting_fixture(true).3, fixtures["lead"]);
}

#[test]
fn an_older_deepseek_host_requires_a_new_member_after_recovery() {
    let (mut h, mut effects, member, card) = waiting_fixture(false);
    assert_eq!(card["decisions"], json!(["acknowledged"]));
    let previous = effects.events.clone();
    effects.older_deepseek_host = false;
    let mut service = Runtime::open(h.temp.path(), MACHINE, "new-epoch").unwrap();
    for now in [1_000_000, 2_000_000, 3_000_000] {
        service.tick(&mut h.store, &mut effects, TEAM, now).unwrap();
    }
    assert_eq!(row(&h, &member).state, State::Blocked);
    assert_eq!(
        effects.events, previous,
        "recovery does not relaunch the old seat"
    );
    h.run(
        &person(),
        Operation::ArchiveMember {
            member: member.clone(),
        },
    )
    .unwrap();
    let replacement = add_deepseek_member(&mut h);
    assert_ne!(replacement, member);
    until(&mut service, &mut h, &mut effects, |h, _| {
        h.store
            .member(TEAM, &replacement)
            .unwrap()
            .unwrap()
            .session
            .is_some()
    });
    assert!(effects.events.contains(&format!("launch:{replacement}")));
    assert!(
        card["text"].as_str().unwrap().contains("add a replacement"),
        "{card}"
    );
    assert!(
        !card["text"].as_str().unwrap().contains("invalid_state"),
        "{card}"
    );
    assert!(
        card["parameters"]
            .as_array()
            .unwrap()
            .contains(&json!(["reason", "older_deepseek_host"])),
        "{card}"
    );
}
