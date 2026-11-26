use super::*;
use crate::{
    desktop::teams::confirm::{engine, ledger::Ledger, types::Kind},
    teams::ops::Actor,
};
use serde_json::{Value, json};
use std::{cell::Cell, path::Path, rc::Rc};

#[test]
fn a_resume_revalidation_gap_does_not_add_a_standing_notice_to_card_history() {
    scan_fixture("standing_archive_gap");
}

#[test]
fn an_older_standing_notice_retires_against_its_saved_service_receipt() {
    scan_fixture("legacy_archive_gap");
}

#[test]
fn a_complete_manual_archive_during_revalidation_does_not_claim_a_notice() {
    scan_fixture("complete_archive_revalidation");
}

#[test]
fn complete_revalidation_retires_the_old_exact_opener_in_the_same_pass() {
    scan_fixture("complete_legacy_revalidation");
}

pub(super) fn revalidation(
    config: &Config,
    binding: &Binding,
    name: &str,
    store: &mut Store,
    mut record: Value,
    mode: &str,
) {
    let legacy = matches!(mode, "legacy_archive_gap" | "complete_legacy_revalidation");
    let complete = matches!(
        mode,
        "complete_archive_revalidation" | "complete_legacy_revalidation"
    );
    let state = &config.state;
    producer::confirmed(&mut record, &uuid::Uuid::new_v4().to_string());
    producer::save(&mut record).unwrap();
    let terminate = |run: &str| ["terminate", name, "--expected-run-id", run].map(str::to_owned);
    let wrong_run = uuid::Uuid::new_v4().to_string();
    assert!(
        crate::state::dispatch(&terminate(&wrong_run))
            .unwrap_err()
            .contains("session changed before termination")
    );
    assert_eq!(producer::saved_record(name).unwrap()["run_id"], binding.run);

    let stop = Arc::new(AtomicBool::new(false));
    let machine = &binding.machine;
    if legacy {
        test_inspect::use_cli_inspection();
        let mut native = Native::new(config, machine, &stop, store).unwrap();
        let snapshot = native.snapshot(binding).unwrap();
        assert!(snapshot.ended);
        let reason = crate::state::archive_catalog::INCOMPLETE;
        let detail = format!(
            "Whether member session {name} ended is unknown ({reason}); nothing was resumed"
        );
        engine::fixture_legacy_archive_notice(&mut native, binding, &snapshot, detail).unwrap();
    }

    let cli_cards = |store: &Store| {
        let lead = store
            .member(TEAM, "lead")
            .unwrap()
            .unwrap()
            .session
            .unwrap();
        let (page, code) = crate::teams::fixture_lead_cards(config, TEAM, lead);
        assert_eq!(code, 0, "{page}");
        assert_eq!(page["ok"], true);
        assert!(page["next"].is_null());
        page
    };
    let before = cli_cards(store);
    let terminated = Rc::new(Cell::new(false));
    let published = terminated.clone();
    let mut reads = 0;
    let session = binding.session.clone();
    let command = terminate(&binding.run);
    test_inspect::use_cli_inspection_before(move |args| {
        if args.get(3).map(String::as_str) == Some("--session-id") && args[4] == session {
            reads += 1;
            // Revalidation runs the real Terminate writer after the first
            // inspection. Its archive is present while coverage is incomplete.
            if reads == 2 {
                let result = if complete {
                    crate::state::dispatch(&command)
                } else {
                    producer::with_archive_catalog_gap(|| crate::state::dispatch(&command))
                };
                assert_eq!(result.unwrap(), 0);
                published.set(true);
            }
        }
    });

    let mut cards = actor::Runtime::open(state, machine).unwrap();
    let mut pass = |store: &mut Store, now| {
        step_at(
            config,
            ("epoch", machine),
            store,
            &stop,
            (&mut Scan::default(), &mut cards),
            || now,
            |_| {},
        )
        .unwrap();
    };
    let start = 1_000_000;
    pass(store, start);
    assert!(terminated.get(), "revalidation ran Terminate");
    let path = only_archive(state);
    let manual: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(manual["run_id"], binding.run);
    assert_eq!(manual["session_id"], binding.session);
    assert_eq!(manual["launch_id"], binding.launch);
    assert_eq!(manual["conversation_id"], record["conversation_id"]);
    assert_eq!(manual["completion_source"], "manual_terminate");
    assert_eq!(manual["completion_reason"], "user_terminated");
    assert!(producer::saved_record(name).is_err());

    let key = engine::resume_key(binding);
    let ledger = Ledger::open(state).unwrap();
    let standing = ledger.get(&key).unwrap();
    let prior_card = if legacy {
        let opener = ledger.card_request(&key).unwrap().unwrap();
        let service = Actor::Service {
            person: binding.person.clone(),
            team: binding.team.clone(),
        };
        let seq = store
            .saved_service_reply(&service, &opener)
            .unwrap()
            .unwrap()
            .item
            .unwrap();
        assert!(
            before["rows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|card| card["seq"] == seq)
        );
        Some(seq)
    } else {
        None
    };

    if complete {
        assert_eq!(
            crate::state::archive_catalog::find_run(state, name, &binding.run).unwrap(),
            manual["archive_id"].as_str().map(str::to_owned)
        );
        if let Some(seq) = prior_card {
            assert_closed_once(store, seq);
        }
        pass(store, start + 4_500);
        pass(store, start + 184_500);
        let after = cli_cards(store);
        println!(
            "TEAMS_CARD_ORACLE={}",
            json!({
                "before": before, "after": after, "opened": null, "name": name, "archive": manual,
                "reservation": standing, "resume_audit": state.join("approval-audit").exists(),
            })
        );
        if let Some(seq) = prior_card {
            assert_closed_once(store, seq);
        }
        assert!(!state.join("approval-audit").exists(), "no resume audit");
        assert_eq!(
            standing.is_some(),
            legacy,
            "exact manual retirement during revalidation claimed a standing row"
        );
        assert_eq!(ledger.get(&key).unwrap(), standing, "a resume was claimed");
        return;
    }
    if let Some(row) = &standing {
        assert_eq!(
            (&row.plan.kind, row.state.as_str()),
            (&Kind::Notice, "cancelled")
        );
        assert!(row.audit.is_none() && row.result.is_none() && row.card_done);
    }

    pass(store, start + 4_500);
    pass(store, start + 184_500);
    let [actor] = <[_; 1]>::try_from(
        store
            .cards(TEAM, 50, None)
            .unwrap()
            .rows
            .into_iter()
            .filter(|card| card.kind == "team_actor_blocked" && card.text.contains(name))
            .collect::<Vec<_>>(),
    )
    .expect("one delayed actor notice");
    assert_open(store, actor.seq, prior_card, "incomplete coverage");

    let held = state.join("fixture-held-exact-archive");
    std::fs::rename(&path, &held).unwrap();
    repair();
    let mut wrong = record.clone();
    wrong["run_id"] = json!(wrong_run);
    wrong["completion_source"] = json!("manual_terminate");
    wrong["completion_reason"] = json!("user_terminated");
    producer::archive(&wrong).unwrap();
    repair();
    pass(store, start + 190_000);
    assert_open(store, actor.seq, prior_card, "wrong run");

    std::fs::rename(&held, &path).unwrap();
    repair();
    let mut duplicate = wrong;
    duplicate["run_id"] = json!(binding.run);
    producer::confirmed(&mut duplicate, &uuid::Uuid::new_v4().to_string());
    let duplicate_id = producer::archive(&duplicate).unwrap();
    assert_ne!(manual["archive_id"], duplicate_id);
    repair();
    pass(store, start + 195_000);
    assert_open(store, actor.seq, prior_card, "duplicate run");

    std::fs::rename(
        state.join("archives").join(format!("{duplicate_id}.json")),
        state.join("fixture-held-duplicate-archive"),
    )
    .unwrap();
    repair();
    pass(store, start + 200_000);
    pass(store, start + 205_000);

    let after = cli_cards(store);
    println!(
        "TEAMS_CARD_ORACLE={}",
        json!({
            "before": before, "after": after, "opened": actor.seq, "name": name, "archive": manual,
        })
    );
    for seq in std::iter::once(actor.seq).chain(prior_card) {
        assert_closed_once(store, seq);
    }
    assert_eq!(
        standing.is_some(),
        legacy,
        "unexpected durable standing notice"
    );
    assert_eq!(ledger.get(&key).unwrap(), standing, "a resume was claimed");
    assert!(!state.join("approval-audit").exists(), "no resume audit");
}

fn assert_closed_once(store: &Store, seq: u64) {
    let retired = "The exact member run was deliberately archived or terminated.";
    assert_eq!(store.card(TEAM, seq).unwrap().unwrap().state, "closed");
    assert_eq!(close_note(store, seq), retired);
    assert_eq!(
        store
            .journal(TEAM, 50, None)
            .unwrap()
            .rows
            .iter()
            .filter(|event| {
                event.kind == "close_card" && event.refs.contains(&format!("card:{seq}"))
            })
            .count(),
        1
    );
}

fn only_archive(state: &Path) -> std::path::PathBuf {
    let [archive] = <[_; 1]>::try_from(
        std::fs::read_dir(state.join("archives"))
            .unwrap()
            .take(2)
            .map(|item| item.unwrap().path())
            .collect::<Vec<_>>(),
    )
    .expect("exactly one manual archive from Terminate");
    archive
}

fn repair() {
    assert_eq!(
        crate::state::dispatch_test_archive_catalog_worker().unwrap(),
        0
    );
}

fn assert_open(store: &Store, actor: u64, prior: Option<u64>, label: &str) {
    for card in std::iter::once(actor).chain(prior) {
        assert_eq!(
            store.card(TEAM, card).unwrap().unwrap().state,
            "open",
            "{label}"
        );
    }
    println!("{label}: observation cards still open");
}
