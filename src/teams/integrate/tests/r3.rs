//! The reviewer's turn-contention probe, with executable assertions.
use super::*;
fn hold_ref_turn(seconds: u64) -> std::thread::JoinHandle<()> {
    let (ready, receive) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _turn = crate::platform::spawn_turn();
        ready.send(()).unwrap();
        std::thread::sleep(Duration::from_secs(seconds));
    });
    receive.recv_timeout(Duration::from_secs(3)).unwrap();
    holder
}
#[test]
fn turn_contention_is_unavailable_without_taking_the_store_admission() {
    let f = Fixture::new();
    let incoming = f.branch("member", "new.txt", "incoming");
    create_branch(&f.repo, "teams/held", &f.base).unwrap();
    let holder = Rc::new(std::cell::RefCell::new(None));
    let held = holder.clone();
    BEFORE_REF_TURN.with(|s| {
        *s.borrow_mut() = Some(Box::new(move || {
            *held.borrow_mut() = Some(hold_ref_turn(6));
        }))
    });
    let admitted = Cell::new(false);
    let result = merge_admitted(&f.repo, "teams/held", &incoming, "Admitted", || {
        admitted.set(true);
        Ok(())
    });
    holder.borrow_mut().take().unwrap().join().unwrap();
    let error = result.unwrap_err();
    assert_eq!(error.code, code::INTEGRATION_UNAVAILABLE);
    assert!(error.message.contains("did not start"), "{error:?}");
    assert!(!admitted.get(), "turn wait held the store admission");
    assert_eq!(git(&f.main, &["rev-parse", "teams/held"]), f.base);
}
#[test]
fn turn_wait_does_not_use_the_admitted_writers_five_seconds() {
    let f = Fixture::new();
    let incoming = f.branch("member", "new.txt", "incoming");
    create_branch(&f.repo, "teams/held", &f.base).unwrap();
    let holder = Rc::new(std::cell::RefCell::new(None));
    let held = holder.clone();
    BEFORE_REF_TURN.with(|s| {
        *s.borrow_mut() = Some(Box::new(move || {
            *held.borrow_mut() = Some(hold_ref_turn(2));
        }))
    });
    let path = f.root.path().join("delayed-git");
    executable(&path, "#!/bin/sh\nfor arg do\n if [ \"$arg\" = update-ref ]; then sleep 4; break; fi\ndone\nexec \"$HGS_FIXTURE_REAL_GIT\" \"$@\"\n");
    let old = PROGRAM.with(|p| p.replace(Some(path)));
    let started = Instant::now();
    let result = merge_admitted(&f.repo, "teams/held", &incoming, "Admitted", || Ok(()));
    PROGRAM.with(|p| *p.borrow_mut() = old);
    holder.borrow_mut().take().unwrap().join().unwrap();
    assert!(matches!(result, Ok(Merge::Merged { .. })), "{result:?}");
    assert!(started.elapsed() >= Duration::from_secs(6));
}
