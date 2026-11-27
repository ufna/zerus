use super::*;
use crate::composer::send::SendReply;
use crate::composer::send::tests::{Fake, ready, send_hub_on, send_hub_with, state};
use crate::composer::tests::{dir, drafts, hold, session, until};
use crate::composer::{ComposerView, Job, SessionRef, lock};
use serde_json::{Value, json};

fn target_session(agent: &str) -> SessionRef {
    SessionRef {
        name: format!("{agent}/sample-project/review"),
        ..session()
    }
}

fn details(target: &SessionRef, agent: &str, is_busy: bool, pending: Option<bool>) -> Value {
    let mut value = ready();
    value["name"] = json!(target.name);
    value["agent"] = json!(agent);
    value["activity"] = json!(if is_busy { "busy" } else { "idle" });
    value["phase"] = json!(if is_busy { "tool" } else { "idle" });
    value["guarded_idle_send"] = json!(true);
    if agent == "claude" {
        value["claude_outbox_supported"] = json!(true);
    }
    if dsh_agent(agent) {
        value["dsh_outbox_supported"] = json!(true);
        value["dsh_outbox_ready"] = json!(!is_busy);
    }
    if let Some(pending) = pending {
        value["previous_message_pending"] = json!(pending);
    }
    value
}

fn native_queue_details(target: &SessionRef, agent: &str) -> Value {
    let mut value = details(target, agent, true, Some(true));
    if agent == "opencode" {
        value["opencode_busy_outbox_supported"] = json!(true);
    } else {
        value["dsh_busy_outbox_supported"] = json!(true);
        value["dsh_busy_outbox_ready"] = json!(true);
    }
    value["input_queue"] = json!({"id":"earlier-native"});
    value["pending_questions"] = json!([]);
    value
}

fn present(c: &Composers, fake: &Fake, target: &SessionRef, agent: &str) -> ComposerView {
    let mut row = lock(&fake.row);
    row.name.clone_from(&target.name);
    row.agent = if agent == "dsh" { "deepseek" } else { agent }.into();
    row.process_state = "running".into();
    row.phase = Some("tool".into());
    drop(row);
    c.open(Some(target.clone()));
    c.observe(&state(details(target, agent, true, Some(true))));
    until(c, |view| view.loaded && view.offers_outbox)
}

fn enqueue(c: &Composers, text: &str) -> ComposerView {
    let view = c.current().unwrap();
    let reply = c.send_message(
        view.handle,
        view.rev + 1,
        view.text_epoch,
        text.into(),
        0,
        0,
    );
    let SendReply::Accepted {
        view,
        waiting: true,
    } = reply
    else {
        panic!("expected a draft accepted into the local queue");
    };
    *view
}

fn settle_next(
    c: &Composers,
    fake: &Fake,
    store: &zerus_client_core::drafts::DraftStore,
    inspected: Value,
) {
    *lock(&fake.inspected) = Some(inspected);
    let now = {
        let mut clock = lock(&fake.now);
        *clock += 4.0;
        *clock
    };
    c.tick_outbox(now);
    store.flush();
}

#[test]
fn ordinary_send_accepts_a_busy_draft_without_native_input() {
    for agent in ["codex", "kimi", "opencode", "claude", "dsh"] {
        let root = dir("ordinary-send-busy");
        let (c, fake) = send_hub_on(&drafts(&root));
        let target = target_session(agent);
        let view = present(&c, &fake, &target, agent);
        c.send_message(
            view.handle,
            view.rev + 1,
            view.text_epoch,
            "queued by Send".into(),
            0,
            0,
        );
        c.0.store.flush();
        let saved = zerus_client_core::drafts::read_draft(c.0.store.root(), &view.key);
        assert_eq!(saved.draft.outbox.len(), 1);
        assert_eq!(saved.draft.outbox[0].text, "queued by Send");
        assert_eq!(saved.draft.outbox[0].target.name, target.name);
        assert!(saved.draft.draft.text.is_empty());
        assert!(lock(&fake.calls).is_empty());
    }
}

#[test]
fn ordinary_terminal_send_offers_while_working_and_never_overtakes_an_unknown_head() {
    for agent in ["claude", "kimi"] {
        let root = dir("busy-native-terminal-outbox");
        let store = drafts(&root);
        let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
        let target = target_session(agent);
        present(&c, &fake, &target, agent);
        let mut inspect = details(&target, agent, true, Some(false));
        inspect["terminal_native_accept_supported"] = json!(true);
        inspect["input_queue"] = json!({"id":"native-earlier"});
        let mut idle = inspect.clone();
        idle["activity"] = json!("idle");
        assert_eq!(terminal_native_route(agent, &idle), agent == "claude");
        c.observe(&state(inspect.clone()));
        *lock(&fake.outcome) = Box::new(|_, _| Outcome::Uncertain {
            detail: "The original acknowledgement was lost".into(),
        });
        let editor = c.current().unwrap();
        c.edit(editor.handle, editor.rev + 1, editor.text_epoch, "Offer now".into(), 9, 9);
        let before = until(&c, |view| view.offers_native_queue && view.can_send);
        assert!(matches!(c.send_message(before.handle, before.rev + 1, before.text_epoch,
            "Offer now".into(), 0, 0), SendReply::Accepted { waiting: false, .. }));
        let held = until(&c, |view| view.waiting_in_zerus[0].state == State::Unknown);
        let request = held.waiting_in_zerus[0].request_id.clone();
        assert!(held.text.is_empty());
        let later = enqueue(&c, "After the uncertain original");
        assert_eq!(later.waiting_in_zerus[1].state, State::Waiting);
        settle_next(&c, &fake, &store, inspect);
        let calls = lock(&fake.calls);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].2, request);
        assert!(matches!(&calls[0].1, Action::SendWhenNativeAccepts { text, .. } if text == "Offer now"));
    }
}

#[test]
fn ordinary_send_starts_the_exact_guarded_offer_before_the_busy_native_turn_ends() {
    for agent in ["opencode", "dsh"] {
        let root = dir("native-queue-ordinary-send");
        let store = drafts(&root);
        let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
        let target = target_session(agent);
        present(&c, &fake, &target, agent);
        c.observe(&state(native_queue_details(&target, agent)));
        let view = until(&c, |view| view.offers_native_queue);
        assert!(view.offers_outbox);
        let name = target.name.clone();
        *lock(&fake.outcome) = Box::new(move |_, id| Outcome::Done {
            receipt: json!({"status":"submitted","request_id":id,"name":name,"run_id":"run-1",
                "conversation_id":"conv-1","queued":true}),
            cleanup: None,
        });
        let SendReply::Accepted { view, waiting: false } = c.send_message(
            view.handle, view.rev + 1, view.text_epoch, "Native next turn".into(), 0, 0,
        ) else {
            panic!("ordinary Send waited for idle despite exact native queue admission");
        };
        let original = &view.waiting_in_zerus[0];
        assert_eq!(original.state, State::Reserved);
        until(&c, |view| view.waiting_in_zerus.is_empty());
        let calls = lock(&fake.calls);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].2, original.request_id);
        assert_eq!(calls[0].0.name, target.name);
        assert!(matches!(&calls[0].1, Action::SendToNativeQueue { text, .. } if text == "Native next turn"));
    }
}

#[test]
fn older_saved_fifo_and_file_can_use_the_busy_native_route_after_restart() {
    for agent in ["opencode", "dsh"] {
        let root = dir("native-queue-saved-fifo");
        let store = drafts(&root);
        let (first, fake) = send_hub_on(&store);
        let target = target_session(agent);
        let view = present(&first, &fake, &target, agent);
        let mut legacy = details(&target, agent, true, Some(true));
        legacy["attachments_supported"] = json!(true);
        first.observe(&state(legacy));
        until(&first, |view| view.attachments_available);
        let path = root.join("input.txt");
        std::fs::write(&path, b"original file before restart").unwrap();
        first.attach_paths(view.handle, vec![path.clone()], Default::default());
        let attached = until(&first, |view| view.attachments.len() == 1);
        let one = enqueue(&first, &attached.text).waiting_in_zerus[0].request_id.clone();
        let view = enqueue(&first, "two");
        let two = view.waiting_in_zerus[1].request_id.clone();
        first.edit(view.handle, view.rev + 1, view.text_epoch, "separate draft".into(), 0, 0);
        first.flush();
        store.flush();
        drop(first);
        drop(store);
        std::fs::remove_file(path).unwrap();

        let store = drafts(&root);
        let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
        present(&c, &fake, &target, agent);
        until(&c, |view| view.waiting_in_zerus.len() == 2);
        let ready = native_queue_details(&target, agent);
        for (key, value) in [
            ("pending_questions", json!([{"question_id":"real-question"}])),
            ("pending_questions_complete", json!(false)),
            ("conversation_id", json!("foreign-conversation")),
            ("run_id", json!("foreign-run")),
        ] {
            let mut blocked = ready.clone();
            blocked[key] = value;
            settle_next(&c, &fake, &store, blocked);
            assert!(lock(&fake.calls).is_empty(), "{agent} {key}");
        }
        let name = target.name.clone();
        *lock(&fake.outcome) = Box::new(move |_, id| Outcome::Done {
            receipt: json!({"status":"submitted","request_id":id,"name":name,"run_id":"run-1",
                "conversation_id":"conv-1","queued":true}),
            cleanup: None,
        });
        settle_next(&c, &fake, &store, ready);
        assert_eq!(until(&c, |view| view.waiting_in_zerus.is_empty()).text, "separate draft");
        let calls = lock(&fake.calls);
        assert_eq!(calls.iter().map(|call| &call.2).collect::<Vec<_>>(), [&one, &two]);
        assert!(matches!(&calls[0].1, Action::SendToNativeQueue { attachments, .. }
            if attachments.len() == 1 && attachments[0].bytes.as_ref() == b"original file before restart"));
        assert!(matches!(&calls[1].1, Action::SendToNativeQueue { text, .. } if text == "two"));
    }
}

#[test]
fn manual_send_to_a_working_codex_uses_ordinary_send_and_keeps_file_identity() {
    manual_turn_route(false);
}

#[test]
fn fresh_manual_send_uses_host_ordering_after_local_fifo_settlement() {
    manual_turn_route(true);
}

fn manual_turn_route(ordered: bool) {
    let root = dir(if ordered { "ordered-codex-turn" } else { "manual-codex-turn" });
    let store = drafts(&root);
    let (c, fake) = send_hub_on(&store);
    let session = target_session("codex");
    let view = present(&c, &fake, &session, "codex");
    let path = root.join("fixture.txt");
    std::fs::write(&path, b"current turn file").unwrap();
    c.attach_paths(view.handle, vec![path], Default::default());
    until(&c, |v| v.attachments.len() == 1);
    let mut busy = details(&session, "codex", true, Some(false));
    busy["codex_busy_outbox_supported"] = json!(true);
    busy["codex_manual_turn_send"] = json!(true);
    busy["codex_ordered_manual_turn_send"] = json!(ordered);
    busy["pending_questions_complete"] = json!(true);
    busy["pending_questions"] = json!([]);
    c.observe(&state(busy));
    let view = until(&c, |v| !v.offers_outbox && v.can_send);
    c.send_message(view.handle, view.rev + 1, view.text_epoch, "manual turn".into(), 0, 0);
    crate::composer::send::tests::eventually(|| !lock(&fake.calls).is_empty());
    let calls = lock(&fake.calls);
    assert_eq!(calls.len(), 1);
    let (target, action, request) = &calls[0];
    let (text, attachments) = match action {
        Action::SendToOrderedCodexTurn { text, attachments } if ordered => (text, attachments),
        Action::Send { text, attachments, .. } if !ordered => (text, attachments),
        other => panic!("unexpected manual Send route: {other:?}"),
    };
    assert_eq!(text, "manual turn");
    assert_eq!(attachments[0].bytes.as_ref(), b"current turn file");
    assert_eq!(target.run_id, "run-1");
    assert_eq!(target.conversation_id, "conv-1");
    assert!(uuid::Uuid::parse_str(request).is_ok());
    if !ordered { assert!(c.current().unwrap().waiting_in_zerus.is_empty()); }
}

#[test]
fn interrupted_codex_queues_require_exact_positive_native_readiness() {
    let root = dir("interrupted-codex-queued-input");
    let store = drafts(&root);
    let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
    let target = target_session("codex");
    present(&c, &fake, &target, "codex");
    let original = enqueue(&c, "queued").waiting_in_zerus[0].request_id.clone();
    lock(&fake.row).phase = Some("interrupted".into());
    let mut ready = details(&target, "codex", false, Some(false));
    ready["phase"] = json!("interrupted");
    ready["activity"] = json!("unknown");
    ready["interrupted_message_can_send"] = json!(true);
    ready["codex_interrupted_outbox_supported"] = json!(true);
    ready["pending_questions_complete"] = json!(true);
    ready["pending_questions"] = json!([]);
    for (field, value) in [
        ("interrupted_message_can_send", json!(false)),
        ("codex_interrupted_outbox_supported", Value::Null),
        ("previous_message_pending", json!(true)),
        ("pending_questions_complete", Value::Null),
        ("pending_questions", json!([{"question_id":"q"}])),
        ("input_queue", json!({"id":"native-queue"})),
        ("run_id", json!("other-run")),
    ] {
        let mut blocked = ready.clone();
        blocked[field] = value;
        settle_next(&c, &fake, &store, blocked);
        assert!(lock(&fake.calls).is_empty(), "{field}");
    }
    settle_next(&c, &fake, &store, ready);
    until(&c, |_| !lock(&fake.calls).is_empty());
    assert_eq!(lock(&fake.calls)[0].2, original);
}

#[test]
fn busy_codex_waits_for_exact_native_queue_support_then_offers_the_closed_editors_original() {
    let root = dir("busy-codex-native-outbox");
    let store = drafts(&root);
    let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
    let session = target_session("codex");
    present(&c, &fake, &session, "codex");
    let original = enqueue(&c, "first").waiting_in_zerus[0].request_id.clone();
    let second = enqueue(&c, "first").waiting_in_zerus[1].request_id.clone();
    c.open(None);
    assert!(c.current().is_none());
    let mut busy = details(&session, "codex", true, Some(false));
    busy["codex_busy_outbox_supported"] = json!(true);
    busy["pending_questions"] = json!([]);
    busy["pending_questions_complete"] = json!(true);
    for (field, value) in [
        ("codex_busy_outbox_supported", Value::Null), ("pending_questions_complete", Value::Null),
        ("pending_questions", json!([{"question_id":"question"}])), ("input_queue", json!({"id":"queued"})),
        ("previous_message_pending", json!(true)), ("activity", json!("unknown")),
        ("phase", json!("approval")), ("conversation_id", json!("other")), ("run_id", json!("other")),
    ] {
        let mut blocked = busy.clone();
        blocked[field] = value;
        settle_next(&c, &fake, &store, blocked);
        assert!(lock(&fake.calls).is_empty(), "{field}");
    }
    let name = session.name.clone();
    let first_request = original.clone();
    let native_ids = ["00000001-2222-4444-8888-000000000001", "00000002-2222-4444-8888-000000000002"];
    *lock(&fake.outcome) = Box::new(move |_, id| Outcome::Done {
        receipt: json!({"status":"submitted","transport":"codex_queue","codex_outbox":true,"request_id":id,"name":name,
            "run_id":"run-1","conversation_id":"conv-1","native_queued_message_id":native_ids[usize::from(id != first_request)]}),
        cleanup: None,
    });
    // The catalog and inspect may observe different substates of the same turn.
    // One tick and no artificial clock advance must suffice for both native offers.
    lock(&fake.row).phase = Some("compacting".into());
    busy["phase"] = json!("working");
    settle_next(&c, &fake, &store, busy.clone());
    crate::composer::send::tests::eventually(|| lock(&fake.calls).len() == 2);
    assert!(c.current().is_none());
    let calls = lock(&fake.calls);
    assert!(calls.iter().all(|call| matches!(&call.1, Action::SendToCodexQueue { .. })));
    assert_eq!(calls.iter().map(|call| call.2.clone()).collect::<Vec<_>>(), [original.clone(), second.clone()]);
    drop(calls);
    c.open(Some(session));
    c.observe(&state(busy));
    let view = until(&c, |v| v.status == "Queued in Codex");
    assert_eq!(view.local.len(), 2);
    for ((row, id), native) in view.local.iter().zip([original, second]).zip(native_ids) {
        assert_eq!(row.source.as_deref(), Some("zerus_codex_queue"));
        assert_eq!(row.id, id);
        assert_eq!(row.message_id.as_deref(), Some(native));
        assert_eq!(row.text, "first");
    }
}

#[test]
fn deepseek_waits_for_native_idle_and_offers_only_the_exact_original_head() {
    let root = dir("deepseek-ordinary-queue-drain");
    let store = drafts(&root);
    let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
    let target = target_session("dsh");
    present(&c, &fake, &target, "dsh");
    let original = enqueue(&c, "first").waiting_in_zerus[0].request_id.clone();
    enqueue(&c, "second");
    lock(&fake.row).phase = Some("idle".into());
    let idle = details(&target, "dsh", false, None);
    let mut still_queued = idle.clone();
    still_queued["dsh_outbox_ready"] = json!(false);
    settle_next(&c, &fake, &store, still_queued);
    let mut wrong = idle.clone();
    wrong["conversation_id"] = json!("different-conversation");
    settle_next(&c, &fake, &store, wrong);
    assert!(lock(&fake.calls).is_empty());

    *lock(&fake.outcome) = Box::new(|_, _| Outcome::Uncertain {
        detail: "Lost acknowledgement".into(),
    });
    settle_next(&c, &fake, &store, idle.clone());
    let held = until(&c, |view| view.waiting_in_zerus[0].state == State::Unknown);
    assert!(!held.waiting_in_zerus[0].can_check_terminal);
    assert_eq!(held.waiting_in_zerus[1].state, State::Waiting);
    settle_next(&c, &fake, &store, idle);
    let calls = lock(&fake.calls);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0.name, target.name);
    assert_eq!(calls[0].2, original);
}

#[test]
fn ordinary_ready_send_attempts_immediately_and_a_proven_busy_result_keeps_the_original() {
    for agent in ["codex", "kimi"] {
        let root = dir("ordinary-send-ready-race");
        let store = drafts(&root);
        let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
        let target = target_session(agent);
        let view = present(&c, &fake, &target, agent);
        let idle = details(&target, agent, false, Some(false));
        lock(&fake.row).phase = Some("idle".into());
        let mut selected = idle.clone();
        selected["codex_idle_outbox_supported"] = json!(agent == "codex");
        selected["pending_questions_complete"] = json!(true);
        selected["pending_questions"] = json!([]);
        c.observe(&state(selected));
        let name = target.name.clone();
        *lock(&fake.outcome) = Box::new(move |_, id| Outcome::Done {
            receipt: json!({"status":"waiting_for_idle", "proof":"native_input_lock",
                "request_id":id, "name":name, "run_id":"run-1", "conversation_id":"conv-1"}),
            cleanup: None,
        });
        let reply = c.send_message(
            view.handle,
            view.rev + 1,
            view.text_epoch,
            "first".into(),
            0,
            0,
        );
        let SendReply::Accepted {
            view,
            waiting: false,
        } = reply
        else {
            panic!("ready Send should start its attempt without waiting for the ticker");
        };
        let original = &view.waiting_in_zerus[0];
        assert_eq!(original.state, State::Reserved);
        let waiting = until(&c, |v| {
            v.waiting_in_zerus
                .first()
                .is_some_and(|m| m.state == State::Waiting)
        });
        assert_eq!(waiting.waiting_in_zerus[0].request_id, original.request_id);
        assert_eq!(waiting.waiting_in_zerus[0].queued_at, original.queued_at);
        assert_eq!(lock(&fake.calls).len(), 1);
        assert!(matches!(
            lock(&fake.calls)[0].1,
            Action::SendWhenIdle { .. }
        ));

        let next = enqueue(&c, "second");
        assert_ne!(next.waiting_in_zerus[1].request_id, original.request_id);
        assert_eq!(
            lock(&fake.calls).len(),
            1,
            "backlog keeps the second message waiting"
        );

        let name = target.name.clone();
        *lock(&fake.outcome) = Box::new(move |_, id| Outcome::Done {
            receipt: json!({"status":"submitted","request_id":id,"name":name,
                "run_id":"run-1","conversation_id":"conv-1"}),
            cleanup: None,
        });
        settle_next(&c, &fake, &store, idle.clone());
        until(&c, |v| v.waiting_in_zerus.len() == 1);
        store.flush();
        settle_next(&c, &fake, &store, idle);
        until(&c, |v| v.waiting_in_zerus.is_empty());
        let calls = lock(&fake.calls);
        assert_eq!(calls[1].2, original.request_id);
        assert_eq!(calls[2].2, next.waiting_in_zerus[1].request_id);
        assert!(c.current().unwrap().waiting_in_zerus.is_empty());
    }
}

#[test]
fn ordinary_send_never_pastes_a_busy_unsupported_or_mismatched_target() {
    for (agent, reason) in [
        ("claude", QUEUE_OLD_HOST),
        ("dsh", QUEUE_OLD_HOST),
        ("codex", QUEUE_TARGET_CHANGED),
    ] {
        let root = dir("single-send-unsupported");
        let (c, fake) = send_hub_on(&drafts(&root));
        let session = target_session(agent);
        {
            let mut row = lock(&fake.row);
            row.name = session.name.clone();
            row.agent = agent.into();
            row.process_state = "running".into();
            row.phase = Some("tool".into());
        }
        let mut busy = details(&session, agent, true, Some(false));
        if reason == QUEUE_TARGET_CHANGED {
            busy["run_id"] = json!("replacement-run");
        }
        if reason == QUEUE_OLD_HOST {
            let capability = if dsh_agent(agent) { "dsh_outbox_supported" } else { "claude_outbox_supported" };
            busy.as_object_mut().unwrap().remove(capability);
            busy.as_object_mut().unwrap().remove("dsh_outbox_ready");
        }
        c.open(Some(session));
        c.observe(&state(busy));
        let view = until(&c, |v| v.loaded);
        assert_eq!(view.ordinary_send_block.as_deref(), Some(reason));
        assert!(matches!(
            c.send_message(view.handle, view.rev + 1, view.text_epoch, "keep draft".into(), 0, 0),
            SendReply::Notice { text, .. } if text == reason
        ));
        let view = c.current().unwrap();
        assert_eq!(view.text, "keep draft");
        assert!(view.waiting_in_zerus.is_empty());
        assert!(lock(&fake.calls).is_empty());
    }
}


#[test]
fn ordinary_send_keeps_the_older_hosts_ready_route_and_preserves_its_busy_draft() {
    let root = dir("single-send-old-host");
    let store = drafts(&root);
    let (c, fake) = send_hub_on(&store);
    let session = target_session("codex");
    let view = present(&c, &fake, &session, "codex");
    let mut busy = details(&session, "codex", true, Some(false));
    busy.as_object_mut().unwrap().remove("guarded_idle_send");
    c.observe(&state(busy));
    assert!(matches!(
        c.send_message(view.handle, view.rev + 1, view.text_epoch, "old draft".into(), 0, 0),
        SendReply::Notice { text, .. } if text == QUEUE_OLD_HOST
    ));
    assert!(lock(&fake.calls).is_empty());
    let kept = c.current().unwrap();
    assert_eq!(kept.text, "old draft");
    assert!(kept.waiting_in_zerus.is_empty());

    let mut idle = details(&session, "codex", false, Some(false));
    idle.as_object_mut().unwrap().remove("guarded_idle_send");
    lock(&fake.row).phase = Some("idle".into());
    c.observe(&state(idle));
    assert_eq!(
        c.send_message(
            kept.handle,
            kept.rev + 1,
            kept.text_epoch,
            "old draft".into(),
            0,
            0
        ),
        SendReply::Sending
    );
    until(&c, |v| !v.sending);
    let calls = lock(&fake.calls);
    assert_eq!(calls.len(), 1);
    assert!(matches!(&calls[0].1, Action::Send { text, .. } if text == "old draft"));
    assert!(c.current().unwrap().waiting_in_zerus.is_empty());
}

fn watch_reconnecting() -> zerus_client_core::Batch {
    let mut batch = state(Value::Null);
    batch.activity[0].kind = "connection".into();
    batch.activity[0].payload = json!({
        "ready": false, "pending": true, "code": "watch_reconnecting"
    });
    batch
}

#[test]
fn first_native_send_after_reconnect_needs_the_current_unbound_run() {
    let root = dir("first-native-after-reconnect");
    let (c, fake) = send_hub_on(&drafts(&root));
    let target = target_session("codex");
    let view = present(&c, &fake, &target, "codex");
    {
        let mut row = lock(&fake.row);
        row.conversation_id = None;
        row.session_id = Some("test-session".into());
        row.account_binding_reported = true;
        row.account_id = "fixture-account".into();
        row.phase = Some("unknown".into());
    }
    c.observe(&watch_reconnecting());
    let mut first = details(&target, "codex", false, None);
    first["conversation_id"] = Value::Null;
    first["session_id"] = json!("test-session");
    first["account_id"] = json!("fixture-account");
    first["phase"] = json!("unknown");
    first["activity"] = json!("unknown");
    first["first_message_can_send"] = json!(true);
    for (field, value) in [("run_id", json!("older-run")), ("name", json!("codex/another")),
        ("session_id", json!("another-session")), ("account_id", json!("other-account")),
        ("agent", json!("kimi")), ("first_message_can_send", json!(false)),
        ("conversation_id", json!("previous-conversation"))] {
        let mut wrong = first.clone();
        wrong[field] = value;
        c.observe(&state(wrong));
        assert!(c.current().unwrap().ordinary_send_block.is_some(), "{field}");
        assert!(lock(&fake.calls).is_empty());
    }
    c.observe(&state(first.clone()));
    assert!(c.current().unwrap().ordinary_send_block.is_none());

    // A row replacement after a valid read cannot reuse the old first prompt.
    lock(&fake.row).run_id = Some("replacement-run".into());
    assert!(c.current().unwrap().ordinary_send_block.is_some());
    lock(&fake.row).run_id = Some("run-1".into());
    c.observe(&watch_reconnecting());
    assert!(c.current().unwrap().ordinary_send_block.is_some());
    c.observe(&state(first));
    let current = c.current().unwrap();
    assert_eq!(current.ordinary_send_block, None);
    assert_eq!(c.send_message(view.handle, current.rev + 1, current.text_epoch,
        "first message".into(), 0, 0), SendReply::Sending);
    until(&c, |v| !v.sending);
    let calls = lock(&fake.calls);
    assert!(matches!(calls.as_slice(), [(target, Action::Send { text, .. }, _)]
        if target.run_id == "run-1" && target.conversation_id.is_empty() && text == "first message"));
    assert!(c.current().unwrap().waiting_in_zerus.is_empty());
}

#[test]
fn same_key_watch_recovery_needs_a_new_exact_owning_host_read() {
    for use_stream in [true, false] {
        let root = dir("single-send-watch-replaced");
        let store = drafts(&root);
        let (c, fake) = send_hub_on(&store);
        let session = target_session("codex");
        let view = present(&c, &fake, &session, "codex");
        let idle = details(&session, "codex", false, Some(false));
        lock(&fake.row).phase = Some("idle".into());
        c.observe(&state(idle.clone()));

        // The successor catalog has caught up for this same run before
        // its first Activity update. An old-positive read never crosses loss.
        c.observe(&watch_reconnecting());
        let mut wrong = idle.clone();
        wrong["conversation_id"] = json!("other-conversation");
        let check_block = || {
            let current = c.current().unwrap();
            let blocked = c.send_message(
                current.handle,
                current.rev + 1,
                current.text_epoch,
                "draft across reconnect".into(),
                0,
                0,
            );
            assert!(matches!(blocked, SendReply::Notice { .. }));
            let kept = c.current().unwrap();
            assert_eq!(kept.handle, view.handle);
            assert_eq!(kept.text, "draft across reconnect");
            assert!(kept.waiting_in_zerus.is_empty());
            assert!(lock(&fake.calls).is_empty());
        };
        check_block();
        c.observe(&state(wrong));
        check_block();

        let mut older = idle;
        older.as_object_mut().unwrap().remove("guarded_idle_send");
        if use_stream {
            c.observe(&state(older));
        } else {
            *lock(&fake.inspected) = Some(older);
            c.refresh_details();
        }
        let kept = until(&c, |v| v.ordinary_send_block.is_none() && v.can_send);
        assert_eq!(
            c.send_message(kept.handle, kept.rev + 1, kept.text_epoch, kept.text, 0, 0),
            SendReply::Sending
        );
        until(&c, |v| !v.sending);
        assert!(matches!(
            &lock(&fake.calls).as_slice(),
            [(_, Action::Send { .. }, _)]
        ));
        assert!(c.current().unwrap().waiting_in_zerus.is_empty());
    }
}

#[test]
fn a_route_loss_before_the_guarded_offer_keeps_the_same_head_waiting() {
    for immediate in [true, false] {
        let root = dir("single-send-route-while-reserved");
        let store = drafts(&root);
        let jobs = std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::VecDeque::<Job>::new(),
        ));
        let paused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (c, fake) = send_hub_with(
            &store,
            std::sync::Arc::new({
                let (jobs, paused) = (jobs.clone(), paused.clone());
                move |job| {
                    if paused.load(std::sync::atomic::Ordering::SeqCst) {
                        lock(&jobs).push_back(job);
                    } else {
                        job();
                    }
                }
            }),
        );
        let session = target_session("codex");
        let view = present(&c, &fake, &session, "codex");
        let head = if immediate {
            None
        } else {
            Some(enqueue(&c, "original").waiting_in_zerus[0].clone())
        };
        let idle = details(&session, "codex", false, Some(false));
        lock(&fake.row).phase = Some("idle".into());
        c.observe(&state(idle.clone()));
        paused.store(true, std::sync::atomic::Ordering::SeqCst);
        if immediate {
            assert!(matches!(
                c.send_message(
                    view.handle,
                    view.rev + 1,
                    view.text_epoch,
                    "original".into(),
                    0,
                    0
                ),
                SendReply::Accepted { waiting: false, .. }
            ));
        } else {
            *lock(&fake.inspected) = Some(idle);
            c.tick_outbox(1_793_000_104.0);
            let inspect = lock(&jobs).pop_front().unwrap();
            inspect();
        }
        store.flush();
        let reserved = c.current().unwrap().waiting_in_zerus[0].clone();
        assert_eq!(reserved.state, State::Reserved);
        if let Some(head) = head {
            assert_eq!(reserved.request_id, head.request_id);
        }
        c.observe(&watch_reconnecting());
        let offer = lock(&jobs).pop_front().unwrap();
        offer();
        store.flush();
        let waiting = c.current().unwrap().waiting_in_zerus[0].clone();
        assert_eq!(waiting.state, State::Waiting);
        assert_eq!(waiting.request_id, reserved.request_id);
        assert_eq!(waiting.queued_at, reserved.queued_at);
        assert!(lock(&fake.calls).is_empty());
    }
}

#[test]
fn a_route_change_inside_the_dispatcher_blocks_the_original_child() {
    let root = dir("single-send-route-in-dispatcher");
    let store = drafts(&root);
    let (c, fake) = send_hub_on(&store);
    let session = target_session("codex");
    let view = present(&c, &fake, &session, "codex");
    lock(&fake.row).phase = Some("idle".into());
    c.observe(&state(details(&session, "codex", false, Some(false))));
    let invoked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    c.set_outbox_dispatch(std::sync::Arc::new({
        let invoked = std::sync::Arc::clone(&invoked);
        move |target, action, id, admission| {
            assert!(matches!(action, Action::SendWhenIdle { .. }));
            lock(&invoked).push((target, id.clone()));
            // The dispatcher is already selected. The host changes during
            // its read-only preparation, before trying the original child.
            admission.composers.observe(&watch_reconnecting());
            Ok(admission.spawn(|| panic!("a stale offer must not start a child")))
        }
    }));
    let SendReply::Accepted {
        view,
        waiting: false,
    } = c.send_message(
        view.handle,
        view.rev + 1,
        view.text_epoch,
        "original".into(),
        0,
        0,
    )
    else {
        panic!("ready Send must reserve before the dispatcher runs");
    };
    let reserved = &view.waiting_in_zerus[0];
    let waiting = until(&c, |v| v.waiting_in_zerus[0].state == State::Waiting);
    assert_eq!(waiting.waiting_in_zerus[0].request_id, reserved.request_id);
    assert_eq!(waiting.waiting_in_zerus[0].queued_at, reserved.queued_at);
    let calls = lock(&invoked);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0.name, session.name);
    assert_eq!(calls[0].1, reserved.request_id);
    assert!(lock(&fake.calls).is_empty());
}

#[test]
fn three_busy_composer_enqueues_survive_restart_and_drain_in_order_to_the_frozen_run() {
    for agent in ["codex", "kimi"] {
        let root = dir(&format!("outbox-{agent}"));
        let store = drafts(&root);
        let (c, fake) = send_hub_on(&store);
        let target = target_session(agent);
        let view = present(&c, &fake, &target, agent);
        assert_eq!(view.previous_message_pending, Some(true));
        let ids: Vec<_> = ["same", "same", "last"]
            .into_iter()
            .map(|text| {
                let reply = enqueue(&c, text);
                assert!(reply.text.is_empty());
                assert!(!reply.can_send);
                reply.waiting_in_zerus.last().unwrap().request_id.clone()
            })
            .collect();
        assert_ne!(ids[0], ids[1]);
        assert!(
            lock(&fake.calls).is_empty(),
            "nothing touched native input on enqueue"
        );
        assert!(lock(&fake.inspected_targets).is_empty());
        let view = c.current().unwrap();
        c.edit(
            view.handle,
            view.rev + 1,
            view.text_epoch,
            "later edit".into(),
            0,
            0,
        );
        c.flush();
        drop(c);

        let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
        let loaded = present(&c, &fake, &target, agent);
        assert_eq!(loaded.text, "later edit");
        assert_eq!(loaded.waiting_in_zerus.len(), 3);
        let other = SessionRef {
            key: "run:other".into(),
            name: format!("{agent}/sample-project/other"),
            ..target.clone()
        };
        c.open(Some(other));
        assert!(until(&c, |view| view.loaded).waiting_in_zerus.is_empty());

        lock(&fake.row).phase = Some("idle".into());
        let ready = details(&target, agent, false, Some(false));
        for invalid in [
            details(&target, agent, false, None),
            details(&target, agent, false, Some(true)),
            {
                let mut older = ready.clone();
                older.as_object_mut().unwrap().remove("guarded_idle_send");
                older
            },
            {
                let mut wrong = ready.clone();
                wrong["run_id"] = json!("other");
                wrong
            },
            {
                let mut queued = ready.clone();
                queued["input_queue"] = json!({"id": "native"});
                queued
            },
        ] {
            settle_next(&c, &fake, &store, invalid);
            assert!(lock(&fake.calls).is_empty());
        }
        let name = target.name.clone();
        *lock(&fake.outcome) = Box::new(move |_, id| Outcome::Done {
            receipt: json!({"status":"submitted","request_id":id,"name":name,"run_id":"run-1","conversation_id":"conv-1"}),
            cleanup: None,
        });
        for (index, text) in ["same", "same", "last"].into_iter().enumerate() {
            settle_next(&c, &fake, &store, ready.clone());
            // The reservation's callback schedules dispatch, whose settlement
            // is another durable writer job.
            store.flush();
            let calls = lock(&fake.calls);
            assert_eq!(calls.len(), index + 1);
            let (bound, action, id) = &calls[index];
            assert_eq!(
                (
                    &bound.name,
                    bound.run_id.as_str(),
                    bound.conversation_id.as_str()
                ),
                (&target.name, "run-1", "conv-1")
            );
            assert_eq!(id, &ids[index]);
            assert!(matches!(action, Action::SendWhenIdle { text: actual, .. } if actual == text));
        }
        let again = present(&c, &fake, &target, agent);
        assert!(again.waiting_in_zerus.is_empty());
        assert_eq!(again.text, "later edit");
    }
}

#[test]
fn restart_with_an_original_reservation_blocks_the_next_item_without_replay() {
    let root = dir("outbox-crash");
    let store = drafts(&root);
    let (jobs_tx, jobs_rx) = std::sync::mpsc::channel::<Job>();
    let (c, fake) = send_hub_with(
        &store,
        std::sync::Arc::new(move |job| {
            jobs_tx.send(job).unwrap();
        }),
    );
    let target = target_session("codex");
    present(&c, &fake, &target, "codex");
    let first = enqueue(&c, "original").waiting_in_zerus[0].clone();
    enqueue(&c, "later");
    lock(&fake.row).phase = Some("idle".into());
    *lock(&fake.inspected) = Some(details(&target, "codex", false, Some(false)));
    let before_save = hold(&store);
    c.tick_outbox(*lock(&fake.now));
    jobs_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap()();
    assert!(
        jobs_rx.try_recv().is_err(),
        "no dispatch before durable reservation"
    );
    before_save.send(()).unwrap();
    // Stop after reservation but before the scheduled native call.
    drop(
        jobs_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap(),
    );
    c.flush();
    assert!(lock(&fake.calls).is_empty());
    drop(c);

    let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
    let view = present(&c, &fake, &target, "codex");
    assert_eq!(view.waiting_in_zerus[0].request_id, first.request_id);
    assert_eq!(view.waiting_in_zerus[0].queued_at, first.queued_at);
    assert_eq!(view.waiting_in_zerus[0].state, State::Unknown);
    lock(&fake.row).phase = Some("idle".into());
    settle_next(
        &c,
        &fake,
        &store,
        details(&target, "codex", false, Some(false)),
    );
    assert!(lock(&fake.calls).is_empty());
    assert_eq!(c.current().unwrap().waiting_in_zerus.len(), 2);
}

#[test]
fn a_definite_refusal_or_unmatched_receipt_keeps_the_original_and_later_draft() {
    for failed in [true, false] {
        let root = dir(if failed {
            "outbox-failed"
        } else {
            "outbox-wrong-receipt"
        });
        let store = drafts(&root);
        let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
        let target = target_session("kimi");
        present(&c, &fake, &target, "kimi");
        let first = enqueue(&c, "first").waiting_in_zerus[0].request_id.clone();
        enqueue(&c, "next");
        *lock(&fake.outcome) = Box::new(move |_, _| {
            if failed {
                Outcome::Failed {
                    detail: "native refusal".into(),
                }
            } else {
                Outcome::Done {
                    receipt: json!({"request_id":"other"}),
                    cleanup: None,
                }
            }
        });
        lock(&fake.row).phase = Some("idle".into());
        settle_next(
            &c,
            &fake,
            &store,
            details(&target, "kimi", false, Some(false)),
        );
        let updated = until(&c, |v| {
            v.waiting_in_zerus[0].state
                == if failed {
                    State::Failed
                } else {
                    State::Unknown
                }
        });
        assert_eq!(updated.waiting_in_zerus[0].request_id, first);
        assert_eq!(updated.waiting_in_zerus[1].preview, "next");
        settle_next(
            &c,
            &fake,
            &store,
            details(&target, "kimi", false, Some(false)),
        );
        assert_eq!(lock(&fake.calls).len(), 1);
    }
}

#[test]
fn submitted_outbox_row_takes_over_only_after_the_queue_write_and_keeps_file_metadata() {
    use crate::composer::send::tests::{held_spawn, next_job};

    let root = dir("outbox-activity-settlement");
    let store = drafts(&root);
    let (spawn, jobs) = held_spawn();
    let (c, fake) = send_hub_with(&store, spawn);
    let session = target_session("codex");
    let view = present(&c, &fake, &session, "codex");
    let path = root.join("fixture.txt");
    std::fs::write(&path, b"queued file").unwrap();
    c.attach_paths(view.handle, vec![path], Default::default());
    next_job(&jobs)();
    let draft = until(&c, |view| view.attachments.len() == 1);
    let queued = enqueue(&c, &draft.text);
    let original = &queued.waiting_in_zerus[0];
    assert_eq!(original.attachments[0].name, "fixture.txt");
    assert_eq!(original.attachments[0].bytes, 11);
    let projected = serde_json::to_value(original).unwrap();
    assert!(projected["attachments"][0].get("data_base64").is_none());
    assert!(projected["attachments"][0].get("path").is_none());

    lock(&fake.row).phase = Some("idle".into());
    *lock(&fake.inspected) = Some(details(&session, "codex", false, Some(false)));
    let name = session.name.clone();
    *lock(&fake.outcome) = Box::new(move |_, request| Outcome::Done {
        receipt: json!({"status": "submitted", "transport": "terminal", "request_id": request, "name": name,
            "run_id": "run-1", "conversation_id": "conv-1", "submitted_at": 1_793_000_104.5}),
        cleanup: None,
    });
    c.tick_outbox(1_793_000_104.0);
    next_job(&jobs)();
    store.flush();
    let release = hold(&store);
    next_job(&jobs)();
    let writing = c.current().unwrap();
    assert!(writing.local.is_empty());
    assert_eq!(writing.waiting_in_zerus[0].state, State::Reserved);
    assert_eq!(writing.waiting_in_zerus[0].request_id, original.request_id);

    release.send(()).unwrap();
    store.flush();
    let sent = c.current().unwrap();
    assert!(sent.waiting_in_zerus.is_empty());
    assert_eq!(sent.local.len(), 1);
    assert_eq!(sent.local[0].id, original.request_id);
    assert_eq!(sent.local[0].attachments[0]["name"], "fixture.txt");
    assert_eq!(sent.local[0].attachments[0]["request_id"], original.request_id);
    assert!(sent.local[0].attachments[0].get("local_path").is_none());
    assert_eq!(sent.local[0].submitted_at, 1_793_000_104.5);
}

#[test]
fn a_submitted_text_only_outbox_row_waits_for_exact_history_identity() {
    let root = dir("outbox-equal-text-history");
    let store = drafts(&root);
    let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
    let session = target_session("codex");
    present(&c, &fake, &session, "codex");
    let text = "equal text sent on an earlier turn too";
    let view = enqueue(&c, text);
    let id = view.waiting_in_zerus[0].request_id.clone();
    let name = session.name.clone();
    *lock(&fake.outcome) = Box::new(move |_, request| Outcome::Done {
        receipt: json!({"status": "submitted", "request_id": request, "name": name,
            "run_id": "run-1", "conversation_id": "conv-1", "submitted_at": 1_793_000_104.5}),
        cleanup: None,
    });
    lock(&fake.row).phase = Some("idle".into());
    settle_next(&c, &fake, &store, details(&session, "codex", false, Some(false)));
    let sent = until(&c, |view| view.waiting_in_zerus.is_empty() && view.local.len() == 1);
    let same_text = json!({"type": "UserPromptSubmit", "detail": text, "seq": 41, "at": 1_793_000_103.5});
    let session_details = details(&session, "codex", false, Some(false));
    let mut hub = c.lock();
    let ignored = hub.local.reconcile(&sent.key, &session_details, std::slice::from_ref(&same_text));
    assert_eq!(ignored.removed, 0, "an earlier equal-text prompt is not the FIFO receipt");
    assert_eq!(hub.local.rows(&sent.key)[0].id, id);
    assert_eq!(hub.local.rows(&sent.key)[0].source.as_deref(), Some("zerus_outbox"));

    let mut exact = same_text;
    exact["source"] = json!("hgs_delivery");
    exact["message_id"] = json!(id);
    exact["run_id"] = json!("unrelated-run");
    assert_eq!(hub.local.reconcile(&sent.key, &session_details, std::slice::from_ref(&exact)).removed, 0);
    exact["run_id"] = json!("run-1");
    assert_eq!(hub.local.reconcile(&sent.key, &session_details, &[exact]).removed, 1);
}

#[test]
fn a_codex_terminal_acknowledgement_is_status_until_native_history_arrives() {
    outbox_transport("codex", "terminal", true);
}

#[test]
fn the_native_codex_queue_and_kimi_keep_their_outgoing_rows() {
    outbox_transport("codex", "codex_queue", false);
    outbox_transport("kimi", "terminal", false);
}

fn outbox_transport(agent: &str, transport: &'static str, terminal_status: bool) {
    let root = dir("outbox-terminal-handoff");
    let store = drafts(&root);
    let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
    let session = target_session(agent);
    present(&c, &fake, &session, agent);
    enqueue(&c, "Synthetic native prompt");
    let name = session.name.clone();
    *lock(&fake.outcome) = Box::new(move |_, request| Outcome::Done {
        receipt: json!({"status":"submitted", "transport":transport, "request_id":request,
            "name":name, "run_id":"run-1", "conversation_id":"conv-1"}),
        cleanup: None,
    });
    lock(&fake.row).phase = Some("idle".into());
    settle_next(&c, &fake, &store, details(&session, agent, false, Some(false)));
    let view = until(&c, |view| view.waiting_in_zerus.is_empty());
    assert_eq!(view.local.len(), usize::from(!terminal_status));
    assert_eq!(view.status == "Submitted to terminal", terminal_status);
    assert!(!view.status_error);
    assert_eq!(lock(&fake.calls).len(), 1);
    if terminal_status {
        lock(&fake.row).conversation_id = Some("successor".into());
        assert_ne!(
            c.current().unwrap().status,
            "Submitted to terminal",
            "a new catalog conversation must not show the old acknowledgement"
        );
        lock(&fake.row).conversation_id = Some("conv-1".into());
        assert_eq!(c.current().unwrap().status, "Submitted to terminal");
        c.edit(view.handle, view.rev + 1, view.text_epoch, "New draft".into(), 0, 0);
        c.edit(view.handle, view.rev + 2, view.text_epoch, String::new(), 0, 0);
        assert_ne!(c.current().unwrap().status, "Submitted to terminal");
    }
}

#[test]
fn acknowledgement_loss_on_enqueue_cannot_duplicate_a_saved_message_or_its_files() {
    let root = dir("outbox-lost-ack-files");
    let store = drafts(&root);
    let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
    let session = target_session("codex");
    let view = present(&c, &fake, &session, "codex");
    let path = root.join("fixture.txt");
    std::fs::write(&path, b"saved attachment").unwrap();
    c.attach_paths(view.handle, vec![path], Default::default());
    let view = until(&c, |v| v.attachments.len() == 1);
    let rx = c
        .offer_manual_send(
            &mut c.lock(),
            view.handle,
            view.rev,
            view.text_epoch,
            &view.text,
        )
        .unwrap()
        .unwrap();
    drop(rx);
    let saved = until(&c, |v| v.waiting_in_zerus.len() == 1);
    assert!(saved.attachments.is_empty());
    assert!(matches!(
        c.send_message(
            view.handle,
            view.rev,
            view.text_epoch,
            view.text.clone(),
            0,
            0
        ),
        SendReply::Busy
    ));
    drop(c);

    let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
    let view = present(&c, &fake, &session, "codex");
    assert_eq!(view.waiting_in_zerus.len(), 1);
    lock(&fake.row).phase = Some("idle".into());
    lock(&fake.row).run_id = Some("replacement".into());
    let inspected = details(&session, "codex", false, Some(false));
    settle_next(&c, &fake, &store, inspected.clone());
    assert!(
        lock(&fake.calls).is_empty(),
        "the original name cannot substitute for the run"
    );

    lock(&fake.row).run_id = Some("run-1".into());
    let bound = Target {
        computer: session.computer.clone(),
        name: session.name.clone(),
        run_id: "run-1".into(),
        conversation_id: "conv-1".into(),
    };
    *lock(&fake.outcome) = Box::new(move |action, id| {
        let call = zerus_client_core::actions::invocation(&bound, action, id).unwrap();
        let stdin: Value = serde_json::from_slice(call.stdin.as_deref().unwrap()).unwrap();
        assert_eq!(stdin["expected_run_id"], "run-1");
        assert_eq!(stdin["expected_conversation_id"], "conv-1");
        assert_eq!(stdin["request_id"], id);
        assert_eq!(stdin["attachments"][0]["name"], "fixture.txt");
        zerus_client_core::actions::classify(
            &bound,
            action,
            &call,
            zerus_client_core::Exit::TimedOut,
        )
    });
    settle_next(&c, &fake, &store, inspected);
    let view = until(&c, |v| v.waiting_in_zerus[0].state == State::Unknown);
    assert_eq!(view.waiting_in_zerus.len(), 1);
    let calls = lock(&fake.calls);
    let Action::SendWhenIdle { attachments, .. } = &calls[0].1 else {
        panic!("expected guarded Send");
    };
    assert_eq!(attachments[0].bytes.as_ref(), b"saved attachment");
    assert_eq!(calls[0].2, view.waiting_in_zerus[0].request_id);
}

#[test]
fn enqueue_time_is_saved_with_the_request_and_survives_queue_edits_and_reopen() {
    use sha2::{Digest, Sha256};

    const ENQUEUED: f64 = 1_793_000_200.25;
    const AFTER_WRITE: f64 = ENQUEUED + 20.0;
    let root = dir("outbox-enqueue-time");
    let store = drafts(&root);
    let (c, fake) = send_hub_on(&store);
    let target = target_session("codex");
    let view = present(&c, &fake, &target, "codex");
    let rev = view.rev + 1;
    c.edit(view.handle, rev, view.text_epoch, "same".into(), 4, 4);
    let release = hold(&store);
    *lock(&fake.now) = ENQUEUED;
    let rx = c
        .offer_manual_send(&mut c.lock(), view.handle, rev, view.text_epoch, "same")
        .unwrap()
        .unwrap();
    *lock(&fake.now) = AFTER_WRITE;
    release.send(()).unwrap();
    let SendReply::Accepted {
        view,
        waiting: true,
    } = rx.blocking_recv().unwrap()
    else {
        panic!("queue write did not settle");
    };
    let first = view.waiting_in_zerus[0].clone();
    assert_eq!(first.queued_at, Some(ENQUEUED));

    let second = enqueue(&c, "remove this").waiting_in_zerus[1].clone();
    assert_eq!(second.queued_at, Some(AFTER_WRITE));
    // A wall-clock correction cannot reorder the FIFO or change its saved times.
    *lock(&fake.now) = ENQUEUED - 3.0;
    let third = enqueue(&c, "same").waiting_in_zerus[2].clone();
    assert_ne!(first.request_id, third.request_id);
    assert_eq!(third.queued_at, Some(ENQUEUED - 3.0));

    let (tx, rx) = oneshot::channel();
    c.remove_waiting(view.handle, &second.request_id, tx);
    let shortened = rx.blocking_recv().unwrap().unwrap();
    assert_eq!(shortened.waiting_in_zerus, vec![first, third]);
    c.flush();
    drop(c);

    let (c, fake) = send_hub_on(&store);
    let reopened = present(&c, &fake, &target, "codex");
    assert_eq!(reopened.waiting_in_zerus, shortened.waiting_in_zerus);
    c.flush();
    drop(c);

    // Simulate the previous manifest shape by removing just the new
    // optional field from a genuine saved entry.
    let key = &reopened.key;
    let folder = format!("{:x}", Sha256::digest(key.as_bytes()));
    let path = store.root().join("outbox").join(folder).join("queue.json");
    let mut raw: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(raw["entries"][0]["queued_at"], json!(ENQUEUED));
    raw["entries"][0]
        .as_object_mut()
        .unwrap()
        .remove("queued_at");
    std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();

    let (c, fake) = send_hub_on(&store);
    let legacy = present(&c, &fake, &target, "codex");
    assert_eq!(legacy.waiting_in_zerus[0].queued_at, None);
    assert_eq!(
        legacy.waiting_in_zerus[0].request_id,
        reopened.waiting_in_zerus[0].request_id
    );
    assert_eq!(legacy.waiting_in_zerus[1], reopened.waiting_in_zerus[1]);
    enqueue(&c, "later");
    let raw: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(raw["entries"][0].get("queued_at").is_none());
    assert_eq!(raw["entries"][1]["queued_at"], json!(ENQUEUED - 3.0));
}

#[test]
fn host_busy_after_the_observation_releases_only_the_exact_pre_input_reservation() {
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicUsize, Ordering};

    let root = dir("outbox-idle-admission");
    let store = drafts(&root);
    let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
    let session = target_session("codex");
    present(&c, &fake, &session, "codex");
    let original = enqueue(&c, "first").waiting_in_zerus[0].clone();
    enqueue(&c, "later");
    lock(&fake.row).phase = Some("idle".into());
    let target = Target {
        computer: session.computer.clone(),
        name: session.name.clone(),
        run_id: "run-1".into(),
        conversation_id: "conv-1".into(),
    };
    let offers = AtomicUsize::new(0);
    *lock(&fake.outcome) = Box::new(move |action, id| {
        let inv = zerus_client_core::actions::invocation(&target, action, id).unwrap();
        assert!(
            inv.args
                .windows(2)
                .any(|pair| pair == ["--when-idle", "--json"])
        );
        let input = inv.stdin.as_deref().unwrap();
        let request: Value = serde_json::from_slice(input).unwrap();
        assert_eq!(request["only_when_idle"], true);
        let mut reply = json!({
            "schema":1, "status":"submitted", "request_id":id, "name":target.name,
            "run_id":target.run_id, "conversation_id":target.conversation_id,
        });
        if offers.fetch_add(1, Ordering::SeqCst) == 0 {
            reply["status"] = json!("waiting_for_idle");
            reply["proof"] = json!("native_input_lock");
            reply["input_sha256"] = json!(format!("{:x}", Sha256::digest(input)));
        }
        zerus_client_core::actions::classify(
            &target,
            action,
            &inv,
            zerus_client_core::Exit::Code {
                code: 0,
                stdout: serde_json::to_vec(&reply).unwrap(),
                stderr: String::new(),
            },
        )
    });
    let inspected = details(&session, "codex", false, Some(false));
    settle_next(&c, &fake, &store, inspected.clone());
    store.flush();
    let waiting = c.current().unwrap().waiting_in_zerus;
    assert_eq!(waiting[0], original);
    assert_eq!(lock(&fake.calls).len(), 1);
    settle_next(&c, &fake, &store, inspected);
    store.flush();
    let waiting = c.current().unwrap().waiting_in_zerus;
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].preview, "later");
    let calls = lock(&fake.calls);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].2, original.request_id);
    assert_eq!(calls[1].2, original.request_id);
}

#[test]
fn opencode_lost_acknowledgement_advances_only_on_exact_receipt_after_reopen() {
    let root = dir("opencode-outbox-lost-acknowledgement");
    let store = drafts(&root);
    let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
    let session = target_session("opencode");
    present(&c, &fake, &session, "opencode");
    let queued = enqueue(&c, "same");
    let first = queued.waiting_in_zerus[0].request_id.clone();
    let queued = enqueue(&c, "same");
    let second = queued.waiting_in_zerus[1].request_id.clone();
    assert_ne!(first, second);
    *lock(&fake.outcome) = Box::new(|_, _| Outcome::Uncertain { detail: "lost stdout".into() });
    lock(&fake.row).phase = Some("idle".into());
    let idle = details(&session, "opencode", false, Some(false));
    settle_next(&c, &fake, &store, idle.clone());
    until(&c, |v| v.waiting_in_zerus.first().is_some_and(|m| m.state == State::Unknown));
    assert_eq!(lock(&fake.calls).iter().map(|call| call.2.as_str()).collect::<Vec<_>>(), [first.as_str()]);
    drop(c);

    let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
    present(&c, &fake, &session, "opencode");
    let reply = std::sync::Arc::new(std::sync::Mutex::new(None::<Value>));
    let reply_for_status = reply.clone();
    c.set_outbox_status(std::sync::Arc::new(move |target, action, id| {
        use zerus_client_core::actions::{outbox_receipt, outbox_status};
        let query = outbox_status(target, action, id).ok()?;
        let input: Value = serde_json::from_slice(query.stdin.as_deref()?).ok()?;
        let mut response = lock(&reply_for_status).clone()?;
        if response["input_sha256"].is_null() {
            response["input_sha256"] = input["input_sha256"].clone();
        }
        outbox_receipt(target, &query, zerus_client_core::Exit::Code {
            code: 0, stdout: serde_json::to_vec(&response).unwrap(), stderr: String::new(),
        })
    }));
    let accepted = json!({"schema": 1, "status": "submitted", "transport": "native_opencode",
        "proof": "native_opencode_receipt", "name": session.name, "native_message_id": "msg_Original",
        "run_id": "run-1", "conversation_id": "conv-1", "request_id": first});
    let mut wrong = accepted.clone();
    wrong["conversation_id"] = json!("replacement");
    *lock(&reply) = Some(wrong);
    settle_next(&c, &fake, &store, idle.clone());
    let mut wrong = accepted.clone();
    wrong["input_sha256"] = json!("wrong input");
    *lock(&reply) = Some(wrong);
    settle_next(&c, &fake, &store, idle.clone());
    assert_eq!(c.current().unwrap().waiting_in_zerus.len(), 2);
    assert!(lock(&fake.calls).is_empty());

    *lock(&reply) = Some(accepted);
    settle_next(&c, &fake, &store, details(&session, "opencode", true, Some(true)));
    let remaining = until(&c, |v| v.waiting_in_zerus.len() == 1);
    assert_eq!(remaining.waiting_in_zerus[0].request_id, second);
    assert!(lock(&fake.calls).is_empty(), "the accepted turn is still working");

    lock(&fake.row).phase = Some("idle".into());
    let name = session.name.clone();
    *lock(&fake.outcome) = Box::new(move |_, request| Outcome::Done {
        receipt: json!({"status": "submitted", "transport": "native_opencode", "request_id": request,
            "name": name, "run_id": "run-1", "conversation_id": "conv-1", "native_message_id": "msg_Second"}),
        cleanup: None,
    });
    settle_next(&c, &fake, &store, idle);
    until(&c, |v| v.waiting_in_zerus.is_empty());
    assert_eq!(lock(&fake.calls).iter().map(|call| call.2.as_str()).collect::<Vec<_>>(), [second.as_str()]);
}

#[test]
fn claude_fifo_waits_for_native_queue_and_reconciles_one_lost_acknowledgement() {
    let root = dir("claude-outbox-receipt");
    let store = drafts(&root);
    let (composers, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
    let session = target_session("claude");
    present(&composers, &fake, &session, "claude");
    let first = enqueue(&composers, "same").waiting_in_zerus[0].request_id.clone();
    let second = enqueue(&composers, "same").waiting_in_zerus[1].request_id.clone();
    let idle = details(&session, "claude", false, Some(false));
    let mut before = idle.clone();
    before["input_queue"] = json!({"id":"native-existing-queue"});
    lock(&fake.row).phase = Some("idle".into());
    settle_next(&composers, &fake, &store, before);
    assert!(lock(&fake.calls).is_empty());
    *lock(&fake.outcome) = Box::new(|_, _| Outcome::Uncertain { detail: "lost stdout".into() });
    settle_next(&composers, &fake, &store, idle.clone());
    let held = until(&composers, |view| view.waiting_in_zerus[0].state == State::Unknown);
    assert_eq!(held.waiting_in_zerus[0].request_id, first);
    assert_eq!(lock(&fake.calls).len(), 1);

    let prompt = "937ff07b-b157-401f-8c90-4ecfc308744e";
    let native_row = "abba57d9-57a4-441d-a81d-e0278da2eaf8";
    let original = first.clone();
    composers.set_outbox_status(std::sync::Arc::new(move |target, action, request| {
        let query = zerus_client_core::actions::outbox_status(target, action, request).unwrap();
        assert_eq!(request, original);
        let input: Value = serde_json::from_slice(query.stdin.as_deref().unwrap()).unwrap();
        let receipt = json!({"schema":1,"status":"submitted","proof":"native_claude_receipt",
            "claude_outbox":true,"transport":"terminal","native":"transcript","native_input":{
                "prompt_id":prompt,"uuid":native_row},
            "request_id":request,"name":target.name,"run_id":target.run_id,
            "conversation_id":target.conversation_id,"input_sha256":input["input_sha256"]});
        zerus_client_core::actions::outbox_receipt(target, &query, zerus_client_core::Exit::Code {
            code:0, stdout:serde_json::to_vec(&receipt).unwrap(), stderr:String::new(),
        })
    }));
    settle_next(&composers, &fake, &store, idle);
    let waiting = until(&composers, |view| view.waiting_in_zerus.len() == 1);
    assert_eq!(waiting.waiting_in_zerus[0].request_id, second);
    assert_eq!(waiting.local[0].id, first);
    assert_eq!(waiting.local[0].message_id.as_deref(), Some(prompt));
    assert_eq!(lock(&fake.calls).len(), 1);
}

#[test]
fn codex_refused_before_input_recovers_the_original_file_and_fifo_after_restart() {
    use sha2::{Digest, Sha256};
    use zerus_client_core::actions::{invocation, outbox_receipt, outbox_status};

    let root = dir("codex-refused-outbox-restart");
    let store = drafts(&root);
    let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
    let session = target_session("codex");
    let view = present(&c, &fake, &session, "codex");
    let path = root.join("file.txt");
    std::fs::write(&path, b"original bytes").unwrap();
    c.attach_paths(view.handle, vec![path.clone()], Default::default());
    let file = until(&c, |v| v.attachments.len() == 1);
    let first = enqueue(&c, &file.text).waiting_in_zerus[0].clone();
    let queued = enqueue(&c, "second");
    let second = queued.waiting_in_zerus[1].clone();
    c.edit(queued.handle, queued.rev + 1, queued.text_epoch, "next draft".into(), 0, 0);
    lock(&fake.row).phase = Some("idle".into());
    let idle = details(&session, "codex", false, Some(false));
    *lock(&fake.outcome) = Box::new(|_, _| Outcome::Uncertain { detail: "lost reply".into() });
    settle_next(&c, &fake, &store, idle.clone());
    until(&c, |v| v.waiting_in_zerus[0].state == State::Unknown);
    let input = {
        let calls = lock(&fake.calls);
        assert_eq!(calls.len(), 1);
        let (target, action, id) = &calls[0];
        invocation(target, action, id).unwrap().stdin.unwrap()
    };
    c.flush();
    drop(c);
    std::fs::remove_file(path).unwrap();

    let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
    let restored = present(&c, &fake, &session, "codex");
    assert_eq!(restored.text, "next draft");
    assert_eq!(restored.waiting_in_zerus[0].state, State::Unknown);
    assert_eq!(restored.waiting_in_zerus[1], second);
    let response = std::sync::Arc::new(std::sync::Mutex::new(None::<Value>));
    let for_status = response.clone();
    c.set_outbox_status(std::sync::Arc::new(move |target, action, id| {
        let query = outbox_status(target, action, id).ok()?;
        let value = lock(&for_status).clone()?;
        outbox_receipt(target, &query, zerus_client_core::Exit::Code {
            code: 0, stderr: String::new(), stdout: serde_json::to_vec(&value).unwrap(),
        })
    }));
    let receipt = json!({
        "schema": 1, "status": "waiting_for_idle", "transport": "terminal", "proof": "native_input_lock",
        "request_id": first.request_id, "name": session.name, "run_id": "run-1",
        "conversation_id": "conv-1", "input_sha256": format!("{:x}", Sha256::digest(&input)),
    });
    settle_next(&c, &fake, &store, idle.clone());
    assert_eq!(c.current().unwrap().waiting_in_zerus[0].state, State::Unknown);
    for (key, wrong) in [
        ("request_id", second.request_id.as_str()), ("conversation_id", "other"),
        ("input_sha256", "wrong"), ("status", "unknown"),
    ] {
        let mut invalid = receipt.clone();
        invalid[key] = json!(wrong);
        *lock(&response) = Some(invalid);
        settle_next(&c, &fake, &store, idle.clone());
        assert_eq!(c.current().unwrap().waiting_in_zerus[0].state, State::Unknown);
    }
    *lock(&response) = Some(receipt);
    settle_next(&c, &fake, &store, idle.clone());
    let waiting = until(&c, |v| v.waiting_in_zerus[0].state == State::Waiting);
    assert_eq!(waiting.waiting_in_zerus, vec![first.clone(), second.clone()]);
    settle_next(&c, &fake, &store, details(&session, "codex", true, Some(true)));
    assert!(lock(&fake.calls).is_empty(), "a receipt check never dispatches");

    let name = session.name.clone();
    *lock(&fake.outcome) = Box::new(move |_, id| Outcome::Done {
        receipt: json!({"status":"submitted","transport":"terminal","request_id":id,
            "name":name,"run_id":"run-1","conversation_id":"conv-1"}),
        cleanup: None,
    });
    lock(&fake.row).phase = Some("idle".into());
    settle_next(&c, &fake, &store, idle.clone());
    until(&c, |v| v.waiting_in_zerus.len() == 1);
    settle_next(&c, &fake, &store, idle);
    assert_eq!(until(&c, |v| v.waiting_in_zerus.is_empty()).text, "next draft");
    let calls = lock(&fake.calls);
    assert_eq!(calls.len(), 2);
    let (target, action, id) = &calls[0];
    assert_eq!(id, &first.request_id);
    assert!(matches!(action, Action::SendWhenIdle { .. }));
    assert_eq!(invocation(target, action, id).unwrap().stdin.unwrap(), input);
    assert_eq!(calls[1].2, second.request_id);
}

#[test]
fn cold_start_drains_saved_fifo_without_opening_a_composer() {
    cold_start_idle_fifo(false);
}

#[test]
fn cold_start_prefers_the_native_codex_queue_on_a_capable_idle_host() {
    cold_start_idle_fifo(true);
}

fn cold_start_idle_fifo(native: bool) {
    use zerus_client_core::drafts::read_draft;
    let root = dir(if native { "outbox-native-restart" } else { "outbox-background-restart" });
    let store = drafts(&root);
    let (first, fake) = send_hub_on(&store);
    let session = target_session("codex");
    let view = present(&first, &fake, &session, "codex");
    let path = root.join("original.txt");
    std::fs::write(&path, b"queued attachment").unwrap();
    first.attach_paths(view.handle, vec![path.clone()], Default::default());
    let attached = until(&first, |v| v.attachments.len() == 1);
    let one = enqueue(&first, &attached.text).waiting_in_zerus[0].request_id.clone();
    let view2 = enqueue(&first, "two");
    let two = view2.waiting_in_zerus[1].request_id.clone();
    first.edit(view2.handle, view2.rev + 1, view2.text_epoch, "independent".into(), 0, 0);
    first.flush();
    store.flush();
    drop(first);
    drop(store);
    std::fs::remove_file(path).unwrap();
    let store = drafts(&root);

    let (c, fake) = send_hub_with(&store, std::sync::Arc::new(|job| job()));
    {
        let mut row = lock(&fake.row);
        row.name.clone_from(&session.name);
        row.agent = "codex".into();
        row.process_state = "running".into();
        row.phase = Some("idle".into());
    }
    let name = session.name.clone();
    *lock(&fake.outcome) = Box::new(move |_, id| Outcome::Done {
        receipt: json!({"status":"submitted","transport":if native { "codex_queue" } else { "terminal" },
            "codex_outbox":native,"native_queued_message_id":id,"request_id":id,"name":name,
            "run_id":"run-1","conversation_id":"conv-1"}),
        cleanup: None,
    });
    let mut idle = details(&session, "codex", false, Some(false));
    // Hosts supporting only busy native handoff retain their guarded idle path.
    idle["codex_busy_outbox_supported"] = json!(true);
    let blocked = if native {
        idle["codex_idle_outbox_supported"] = json!(true);
        idle["pending_questions_complete"] = json!(true);
        idle["pending_questions"] = json!([]);
        let mut question = idle.clone();
        question["pending_questions"] = json!([{"question_id":"dialog"}]);
        question
    } else {
        details(&session, "codex", true, Some(true))
    };
    // Follow app startup on a second store without ever opening the editor.
    // A tick driven by the test would conceal a missed wake-up on restart.
    struct Ticker<'a>(&'a Composers);
    impl Drop for Ticker<'_> {
        fn drop(&mut self) {
            self.0.flush();
        }
    }
    let _ticker = Ticker(&c);
    *lock(&fake.inspected) = Some(blocked);
    c.start_ticker(true);
    crate::composer::send::tests::eventually(|| {
        fake.inspects.load(std::sync::atomic::Ordering::SeqCst) > 0 && !c.lock().outbox_probe
    });
    assert_eq!(c.lock().composers[&view.key].outbox[0].state, State::Waiting);
    assert!(c.current().is_none());
    assert!(lock(&fake.calls).is_empty());
    *lock(&fake.inspected) = Some(idle);
    let expected_counts: &[usize] = if native { &[0] } else { &[1, 0] };
    for &remaining in expected_counts {
        *lock(&fake.now) += 4.0;
        crate::composer::send::tests::eventually(|| {
            let hub = c.lock();
            hub.composers.get(&view.key).is_some_and(|composer| {
                !composer.queue_writing && composer.outbox.len() == remaining
            })
        });
    }
    let calls = lock(&fake.calls);
    assert_eq!(calls.iter().map(|call| &call.2).collect::<Vec<_>>(), [&one, &two]);
    assert!(calls.iter().all(|call| match &call.1 {
        Action::SendToCodexQueue { .. } => native,
        Action::SendWhenIdle { .. } => !native,
        _ => false,
    }));
    let (Action::SendToCodexQueue { attachments, .. } | Action::SendWhenIdle { attachments, .. }) = &calls[0].1 else {
        unreachable!()
    };
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].bytes.as_ref(), b"queued attachment");
    assert!(c.current().is_none());
    let saved = read_draft(store.root(), &view.key);
    assert_eq!(saved.draft.draft.text, "independent");
    assert!(saved.draft.outbox.is_empty());
}
