use super::*;
use serde_json::json;

const EPOCH: &str = "00000000-0000-4000-8000-000000000001";
const REQUEST: &str = "00000000-0000-4000-8000-000000000002";

fn counts() -> Value {
    json!({"epoch":EPOCH,"scope":"shared_host_state","display_only":true,"authority":false,
        "local_marks_included":false,"computers":[{"computer":"peer","online":false,"complete":false,"total":4,"attention":null}],
        "aggregate":{"total":4,"known_total":4,"known_attention":0,"attention":null,"complete":false}})
}
fn status() -> Value {
    let configuration = "a".repeat(64);
    json!({"abi":ABI,"configuration":configuration,"epoch":EPOCH,"generation":3,
        "lifecycle":{"abi":"desktop-lifecycle-v1","phase":"serving","configuration":configuration,
            "epoch":EPOCH,"generation":3,"ipc_ready":true,"background":{}},"attention_counts":counts()})
}
fn frame(seq: u64, kind: &str, value: Value) -> Value {
    json!({"abi":ABI,"epoch":EPOCH,"seq":seq,"t":kind,"value":value})
}
fn state() -> State {
    State::new("a".repeat(64)).unwrap()
}

#[test]
fn one_explicit_startup_switch_is_off_by_default() {
    for value in [
        None,
        Some(""),
        Some("0"),
        Some("true"),
        Some("yes"),
        Some("1 "),
    ] {
        assert!(!enabled(value));
    }
    assert!(enabled(Some("1")));
}
#[test]
fn ipc_readiness_does_not_advertise_missing_background_components() {
    let mut state = state();
    state.frame(&frame(0, "hello", status())).unwrap();
    assert!(state.snapshot().connected);
    assert_eq!(state.snapshot().code, "background_unready");
    for name in COMPONENTS {
        assert!(!state.snapshot().component_ready(name));
    }
    let mut ready = status();
    ready["lifecycle"]["background"] = json!({"fleet":{"ready":true,"code":"running"}});
    state.frame(&frame(0, "status", ready)).unwrap();
    assert!(state.snapshot().component_ready("fleet"));
    assert!(!state.snapshot().background_ready());
}
#[test]
fn null_and_incomplete_shared_counts_never_become_local_zero_counts() {
    let mut state = state();
    state.status(&status()).unwrap();
    let projection = state.snapshot().attention_counts.as_ref().unwrap();
    assert_eq!(projection, &counts());
    assert!(projection["aggregate"]["attention"].is_null());
    assert_eq!(projection["aggregate"]["complete"], false);
    state.disconnect("service_unavailable");
    assert_eq!(state.snapshot().attention_counts, None);
    assert!(!state.snapshot().connected);
}
#[test]
fn shared_projection_cannot_claim_local_marks_or_authority() {
    for (key, value) in [
        ("local_marks_included", json!(true)),
        ("authority", json!(true)),
        ("display_only", json!(false)),
    ] {
        let mut input = status();
        input["attention_counts"][key] = value;
        let mut state = state();
        assert_eq!(state.status(&input), Err("invalid_attention_projection"));
        assert!(!state.snapshot().connected);
    }
}
#[test]
fn wrong_configuration_epoch_and_generation_clear_current_state() {
    for field in ["configuration", "epoch", "generation"] {
        let mut state = state();
        state.status(&status()).unwrap();
        let mut next = status();
        next["lifecycle"][field] = json!("wrong");
        assert!(state.status(&next).is_err());
        assert!(!state.snapshot().connected);
        assert!(state.snapshot().attention_counts.is_none());
    }
    let mut state = state();
    state.status(&status()).unwrap();
    let mut next = status();
    next["generation"] = json!(2);
    next["lifecycle"]["generation"] = json!(2);
    assert_eq!(state.status(&next), Err("stale_service_generation"));
}
#[test]
fn targeted_sequences_may_skip_but_stale_epochs_and_regressions_are_refused() {
    let mut state = state();
    state.frame(&frame(2, "hello", status())).unwrap();
    state.frame(&frame(9, "status", status())).unwrap();
    state.frame(&frame(9, "status", status())).unwrap();
    assert_eq!(
        state.frame(&frame(8, "status", status())),
        Err("stale_service_frame")
    );
    assert!(!state.snapshot().connected);
    state.frame(&frame(0, "hello", status())).unwrap();
    let mut next = frame(1, "status", status());
    next["epoch"] = json!(REQUEST);
    assert_eq!(state.frame(&next), Err("service_epoch_changed"));
}
#[test]
fn explicit_handoff_refusal_is_not_repaired_by_an_ipc_ready_status() {
    let mut value = status();
    value["background_start"] =
        json!({"admitted":false,"code":"explicit_handoff_required","durably_blocked":true});
    for name in COMPONENTS {
        value["lifecycle"]["background"][name] = json!({"ready":false,"code":"not_started"});
    }
    let mut state = state();
    state.status(&value).unwrap();
    assert!(state.snapshot().connected);
    assert_eq!(state.snapshot().code, "explicit_handoff_required");
    assert!(!state.snapshot().background_ready());
}
#[test]
fn catalog_invalidation_and_disconnect_do_not_admit_effects() {
    let mut state = state();
    state.frame(&frame(0, "hello", status())).unwrap();
    let changed = frame(
        1,
        "catalog_changed",
        json!({"computer":"peer","removed":true,"attention_counts":counts()}),
    );
    assert_eq!(
        state.frame(&changed),
        Ok(Update::Catalog {
            computer: "peer".into(),
            removed: true
        })
    );
    assert_eq!(
        state.frame(&frame(2, "gap", Value::Null)),
        Err("service_resnapshot_required")
    );
    assert!(!state.snapshot().connected);
}
fn page() -> Value {
    json!({"request_id":REQUEST,"abi":"desktop-catalog-v1","accepted":true,"durable":false,
        "epoch":EPOCH,"computer":"peer","display_only":true,"authority":false,"complete":false,
        "items":[{"key":"run:example","row":{"name":"claude/example/session"}}],"next_cursor":null})
}
#[test]
fn catalog_pages_bind_the_request_and_epoch_without_inventing_completeness() {
    let page = CatalogPage::checked(page(), REQUEST, EPOCH, "peer").unwrap();
    assert!(!page.complete);
    assert_eq!(page.items.len(), 1);
    assert!(page.next_cursor.is_none());
    for field in ["request_id", "epoch", "computer"] {
        let mut value = self::page();
        value[field] = json!("wrong");
        assert!(CatalogPage::checked(value, REQUEST, EPOCH, "peer").is_err());
    }
}
#[test]
fn oversized_or_authoritative_catalog_pages_are_refused() {
    let mut value = page();
    value["items"] = json!(vec![json!({"key":"example","row":{}}); CATALOG_ROWS + 1]);
    assert!(CatalogPage::checked(value, REQUEST, EPOCH, "peer").is_err());
    for field in ["durable", "authority"] {
        let mut value = page();
        value[field] = json!(true);
        assert!(CatalogPage::checked(value, REQUEST, EPOCH, "peer").is_err());
    }
}

#[test]
fn read_commit_requires_the_original_identity_operation_and_durable_actor_receipt() {
    let receipt = json!({"request_id":REQUEST,"op":"read_intent","accepted":true,"durable":true});
    assert!(read_committed(&receipt, REQUEST));
    for (key, value) in [
        ("request_id", json!(EPOCH)),
        ("op", json!("delivery_result")),
        ("accepted", json!(false)),
        ("durable", json!(false)),
    ] {
        let mut bad = receipt.clone();
        bad[key] = value;
        assert!(!read_committed(&bad, REQUEST));
    }
}
