//! zerus-63rru.77: a member removed while the real `hgs task team-deliver`
//! child for its hand-out runs. The child checks its permit twice, the
//! second time under the team store's write lock just before it reserves the
//! task attempt; after that reservation nothing stops it from typing.
use super::*;

fn removal(f: &mut Fixture) {
    f.h.run(
        &person(),
        ops::Operation::ArchiveMember {
            member: "m1".into(),
        },
    )
    .unwrap();
}
fn wait_for(f: &Fixture, prefix: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    while !std::fs::read_dir(&f.config.state)
        .unwrap()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().starts_with(prefix))
    {
        assert!(Instant::now() < until, "the child never reached {prefix}");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn effect_state(f: &Fixture) -> String {
    let rows = f.h.store.effects(TEAM, 200, None).unwrap().rows;
    rows.into_iter()
        .find(|e| e.request_id == f.body.id)
        .unwrap()
        .state
}

/// The removal lands after the child's first check and before its locked
/// one: the child refuses in its own words, reserves no attempt and types
/// nothing.
#[test]
fn a_removal_before_the_childs_locked_check_is_refused_and_nothing_is_typed() {
    let mut f = Fixture::new();
    let (child, path) = f.spawn(
        "submitted",
        &f.args(false),
        &serde_json::to_string(&f.body).unwrap(),
        true,
    );
    wait_for(&f, "ready-");
    removal(&mut f);
    assert_eq!(effect_state(&f), "superseded");
    std::fs::write(f.config.state.join("release"), "go").unwrap();
    let out = f.finish(child, path);
    assert_eq!(out["reply"]["state"], "refused", "{out}");
    assert_eq!(
        out["reply"]["error"], "Effect is closed or belongs to another generation.",
        "{out}"
    );
    assert_eq!(f.count(), 0, "the child typed");
    assert!(f.document().attempt(&f.body.id).unwrap().is_none());
}

/// The removal lands after the child reserved the attempt and saved it:
/// nothing stops the child; it types and answers submitted. That is why the
/// removal cannot say the prompt never arrived.
#[test]
fn a_removal_after_the_childs_reservation_does_not_stop_its_typing() {
    let mut f = Fixture::new();
    let (child, path) = f.spawn(
        "type-after-pause",
        &f.args(false),
        &serde_json::to_string(&f.body).unwrap(),
        false,
    );
    wait_for(&f, "typing-");
    assert!(f.document().attempt(&f.body.id).unwrap().is_some());
    removal(&mut f);
    assert_eq!(effect_state(&f), "superseded");
    std::fs::write(f.config.state.join("release-typing"), "go").unwrap();
    let out = f.finish(child, path);
    assert_eq!(out["reply"]["state"], "submitted", "{out}");
    assert_eq!(f.count(), 1, "the child typed once");
}
