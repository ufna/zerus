use super::*;
use crate::actions::Lane;
use std::time::Duration;
use zerus_client_core::end_action::UNDO_WINDOW;

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn actions() -> Actions {
    Actions::new(std::env::temp_dir().join(format!("zerus-undo-marks-{}.json", id())))
}
fn captured(tag: &str) -> Captured {
    Captured {
        target: Target {
            computer: "example-peer".into(),
            name: format!("codex/example/{tag}"),
            run_id: format!("run-{tag}"),
            conversation_id: format!("conversation-{tag}"),
        },
        action: Action::Archive,
        state: "paused".into(),
    }
}
fn due(delays: &Delays, id: &str) {
    lock(&delays.0).groups.get_mut(id).unwrap().window = Window::new(Instant::now() - UNDO_WINDOW);
    assert!(delays.wait(id));
}

#[test]
fn pending_reserves_the_existing_write_lane_without_blocking_messages() {
    let actions = actions();
    let delays = Delays::default();
    let c = captured("one");
    let id = id();
    delays.begin(&actions, id.clone(), vec![c.clone()]).unwrap();
    assert!(actions
        .begin(Lane::key(&c.target, &Action::Pause), &Action::Pause)
        .is_none());
    assert_eq!(actions.write_kind(&c.target.computer, &c.target.name), None);
    let send = Action::Send {
        text: "still allowed".into(),
        attachments: Vec::new(),
        agent_id: None,
        expected_compaction_id: None,
    };
    assert!(actions.begin(Lane::key(&c.target, &send), &send).is_some());
    assert!(delays
        .take(
            &Reference {
                id: id.clone(),
                index: 0
            },
            &c.target,
            &c.action
        )
        .is_err());
    delays.cancel(&id);
    assert!(actions
        .begin(Lane::key(&c.target, &Action::Pause), &Action::Pause)
        .is_some());
}

#[test]
fn partial_batch_reservation_is_released_and_invalid_input_reads_no_source() {
    let actions = actions();
    let delays = Delays::default();
    let a = captured("one");
    let b = captured("two");
    let _busy = actions
        .begin(Lane::key(&b.target, &Action::Pause), &Action::Pause)
        .unwrap();
    assert!(delays.begin(&actions, id(), vec![a.clone(), b]).is_err());
    assert!(actions
        .begin(Lane::key(&a.target, &Action::Pause), &Action::Pause)
        .is_some());
    assert!(lock(&delays.0).groups.is_empty());
    let mut bad = a.clone();
    bad.target.run_id.clear();
    assert!(delays.begin(&actions, id(), vec![bad]).is_err());
    assert!(delays
        .begin(&actions, "not-a-uuid".into(), vec![a.clone()])
        .is_err());
    assert!(delays.begin(&actions, id(), Vec::new()).is_err());
    assert!(delays.begin(&actions, id(), vec![a; TARGETS + 1]).is_err());
}

#[test]
fn undo_before_async_registration_still_prevents_the_late_window() {
    let actions = actions();
    let delays = Delays::default();
    let id = id();
    delays.cancel(&id);
    assert!(delays.begin(&actions, id, vec![captured("one")]).is_err());
}

#[test]
fn close_and_quit_cancel_waiters_and_never_flush_them_into_actions() {
    let actions = actions();
    let delays = Delays::default();
    let c = captured("one");
    let id = id();
    delays.begin(&actions, id.clone(), vec![c.clone()]).unwrap();
    let worker = delays.clone();
    let pending = id.clone();
    let wait = std::thread::spawn(move || worker.wait(&pending));
    delays.cancel_all(false);
    assert!(!wait.join().unwrap());
    assert!(actions
        .begin(Lane::key(&c.target, &Action::Pause), &Action::Pause)
        .is_some());
    assert!(
        delays.begin(&actions, id.clone(), vec![c.clone()]).is_err(),
        "an old waiter cannot bind to a reused UUID"
    );
    let id = uuid::Uuid::new_v4().to_string();
    delays.begin(&actions, id.clone(), vec![c.clone()]).unwrap();
    due(&delays, &id);
    delays.cancel_all(true);
    assert!(delays
        .take(
            &Reference {
                id: id.clone(),
                index: 0
            },
            &c.target,
            &c.action
        )
        .is_err());
    assert!(
        delays.begin(&actions, id, vec![c]).is_err(),
        "exit flush cannot admit another pending action"
    );
}

#[test]
fn reservation_is_exact_and_one_use_after_the_original_deadline() {
    let actions = actions();
    let delays = Delays::default();
    let c = captured("one");
    let id = id();
    delays.begin(&actions, id.clone(), vec![c.clone()]).unwrap();
    due(&delays, &id);
    let reference = Reference {
        id: id.clone(),
        index: 0,
    };
    assert!(delays
        .take(
            &Reference {
                id: id.clone(),
                index: 1
            },
            &c.target,
            &c.action
        )
        .is_err());
    assert!(delays
        .take(&reference, &captured("replacement").target, &c.action)
        .is_err());
    assert!(delays
        .take(&reference, &c.target, &Action::Terminate)
        .is_err());
    let (source, claim) = delays.take(&reference, &c.target, &c.action).unwrap();
    assert_eq!(source, c);
    assert!(delays.take(&reference, &c.target, &c.action).is_err());
    assert!(actions
        .begin(Lane::key(&c.target, &Action::Pause), &Action::Pause)
        .is_none());
    drop(claim);
    assert!(actions
        .begin(Lane::key(&c.target, &Action::Pause), &Action::Pause)
        .is_some());
}

#[test]
fn same_name_archives_share_one_lane_until_every_batch_member_finishes() {
    let actions = actions();
    let delays = Delays::default();
    let id = id();
    let mut a = captured("one");
    a.state = "archived".into();
    a.action = Action::ForgetArchive {
        archive_id: uuid::Uuid::new_v4().to_string(),
    };
    let mut b = a.clone();
    b.target.run_id = "older-run".into();
    b.action = Action::ForgetArchive {
        archive_id: uuid::Uuid::new_v4().to_string(),
    };
    delays
        .begin(&actions, id.clone(), vec![a.clone(), b.clone()])
        .unwrap();
    due(&delays, &id);
    let (_, first) = delays
        .take(
            &Reference {
                id: id.clone(),
                index: 0,
            },
            &a.target,
            &a.action,
        )
        .unwrap();
    drop(first);
    assert!(actions
        .begin(Lane::key(&a.target, &Action::Pause), &Action::Pause)
        .is_none());
    let (_, last) = delays
        .take(&Reference { id, index: 1 }, &b.target, &b.action)
        .unwrap();
    drop(last);
    assert!(actions
        .begin(Lane::key(&a.target, &Action::Pause), &Action::Pause)
        .is_some());
}

#[test]
fn native_wait_never_releases_early_or_restarts_when_the_webview_is_hidden() {
    let actions = actions();
    let delays = Delays::default();
    let c = captured("one");
    let id = id();
    let start = Instant::now();
    delays.begin(&actions, id.clone(), vec![c]).unwrap();
    // No webview timer or visibility state participates in this wait.
    assert!(delays.wait(&id));
    assert!(start.elapsed() >= UNDO_WINDOW);
    assert!(start.elapsed() < Duration::from_secs(15));
    assert!(!delays.wait(&id));
}

#[test]
fn groups_are_bounded_and_poisoned_accounting_refuses_all_pending_work() {
    let actions = actions();
    let delays = Delays::default();
    for n in 0..WINDOWS {
        delays
            .begin(&actions, id(), vec![captured(&n.to_string())])
            .unwrap();
    }
    assert!(delays
        .begin(&actions, id(), vec![captured("extra")])
        .is_err());
    let poisoned = delays.clone();
    assert!(std::thread::spawn(move || {
        let _guard = poisoned.0.model.lock().unwrap();
        panic!("synthetic accounting failure");
    })
    .join()
    .is_err());
    assert!(delays
        .begin(&actions, id(), vec![captured("later")])
        .is_err());
    for n in 0..WINDOWS {
        let c = captured(&n.to_string());
        assert!(actions
            .begin(Lane::key(&c.target, &Action::Pause), &Action::Pause)
            .is_some());
    }
}
