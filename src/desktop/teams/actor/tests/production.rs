//! Production report
//! reader `Native::work_reported` against real Git worktrees with a real
//! Zerus ownership receipt.
use super::super::*;
use crate::teams::ops::tests::*;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{atomic::AtomicBool, Arc},
};

fn git(dir: &Path, empty: &Path, args: &[&str]) -> String {
    let mut cmd = Command::new("git");
    for (k, _) in std::env::vars_os() {
        if k.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(k);
        }
    }
    let out = cmd
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", empty)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "user.name=Review",
            "-c",
            "user.email=review@example.test",
            "-c",
            "init.templateDir=",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
fn native(h: &Harness) -> native::Native {
    native::Native {
        config: crate::config::Config {
            home: String::new(),
            dir: h.temp.path().join("config"),
            state: h.temp.path().to_owned(),
            selves: vec![],
            peers: vec![],
            base_peers: vec![],
            peers_from_environment: false,
            machines: Default::default(),
            tab: false,
            tab_colors: String::new(),
        },
        machine: h.store.team(TEAM).unwrap().unwrap().home,
        epoch: "fixture".into(),
        stop: Arc::new(AtomicBool::new(false)),
    }
}
struct Owned {
    wt: PathBuf,
    root: PathBuf,
    empty: PathBuf,
    nonce: String,
}
/// A Zerus-owned linked worktree on the launch branch the package 7 adapter
/// names (`zerus/team/member-{launch}`), with the receipt written where the
/// maintained ownership reader looks.
fn owned(h: &Harness) -> Owned {
    let base = crate::platform::canonical(h.temp.path()).unwrap();
    let root = base.join("repo");
    std::fs::create_dir(&root).unwrap();
    let empty = base.join("empty-config");
    std::fs::write(&empty, "").unwrap();
    git(&root, &empty, &["init", "-q", "--initial-branch", "main"]);
    git(&root, &empty, &["commit", "--allow-empty", "-qm", "base"]);
    let wt = base.join("zerus-team-launch");
    git(
        &root,
        &empty,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "zerus/team/member-launch",
            wt.to_str().unwrap(),
        ],
    );
    let metadata = PathBuf::from(git(&wt, &empty, &["rev-parse", "--absolute-git-dir"]));
    let common = root.join(".git");
    let nonce = uuid::Uuid::new_v4().to_string();
    let owner = serde_json::json!({"version":1,"id":nonce,"common_dir":common,"path":wt,"metadata":metadata,"branch":"zerus/team/member-launch","head":"fixture"});
    let receipt = crate::state::team_worktree_receipt_path(h.temp.path(), &common, &wt);
    std::fs::create_dir_all(receipt.parent().unwrap()).unwrap();
    std::fs::write(&receipt, owner.to_string()).unwrap();
    std::fs::write(metadata.join("zerus-owner.json"), owner.to_string()).unwrap();
    native_work::TEST_REPO
        .with(|s| *s.borrow_mut() = Some(common.to_str().unwrap().to_owned()));
    Owned {
        wt,
        root,
        empty,
        nonce,
    }
}
fn member(o: &Owned) -> Member {
    Member {
        id: "m1".into(),
        role: "developer".into(),
        agent: "codex".into(),
        account: String::new(),
        state: MemberState::Active,
        session: None,
        worktree: o.wt.to_str().unwrap().into(),
        ownership_nonce: o.nonce.clone(),
    }
}
fn commit(o: &Owned, text: &str) -> String {
    std::fs::write(o.wt.join("work.txt"), format!("{text}\n")).unwrap();
    git(&o.wt, &o.empty, &["add", "work.txt"]);
    git(&o.wt, &o.empty, &["commit", "-qm", text]);
    git(&o.wt, &o.empty, &["rev-parse", "HEAD"])
}

#[test]
fn production_report_reads_the_owned_launch_branch_and_marks_a_dirty_tree() {
    let h = Harness::new();
    let o = owned(&h);
    let head = commit(&o, "on launch branch");
    let team = h.store.team(TEAM).unwrap().unwrap();
    let r = native(&h).work_reported(&team, &member(&o));
    eprintln!("REVIEW08 launch-branch report: {r:?}");
    let r = r.unwrap();
    assert_eq!(r.commit, head);
    assert_eq!(r.branch, "zerus/team/member-launch");
    assert!(!r.dirty);
    std::fs::write(o.wt.join("untracked.txt"), "x\n").unwrap();
    let dirty = native(&h).work_reported(&team, &member(&o)).unwrap();
    eprintln!("REVIEW08 dirty report: {dirty:?}");
    assert!(dirty.dirty);
}

/// Spec, Folders, branches and integration: "Each task gets a branch. The
/// hand-out names the base commit (the integration tip) and the branch; the
/// member's agent creates it."
#[test]
fn production_report_reads_the_member_created_task_branch() {
    let h = Harness::new();
    let o = owned(&h);
    git(&o.wt, &o.empty, &["switch", "-q", "-c", "teams/task-1"]);
    let head = commit(&o, "task one on its own branch");
    let team = h.store.team(TEAM).unwrap().unwrap();
    let r = native(&h).work_reported(&team, &member(&o));
    eprintln!("REVIEW08 task-branch report (HEAD {head}): {r:?}");
    let report = r.expect("the owned task branch must be reportable");
    assert_eq!(report.commit, head);
    assert_eq!(report.branch, "teams/task-1");
}

#[test]
fn production_report_refuses_a_replaced_worktree_at_the_same_path() {
    let h = Harness::new();
    let o = owned(&h);
    commit(&o, "original");
    git(
        &o.root,
        &o.empty,
        &["worktree", "remove", "--force", o.wt.to_str().unwrap()],
    );
    git(
        &o.root,
        &o.empty,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "replacement",
            o.wt.to_str().unwrap(),
        ],
    );
    let team = h.store.team(TEAM).unwrap().unwrap();
    let r = native(&h).work_reported(&team, &member(&o));
    eprintln!("REVIEW08 replaced worktree report: {r:?}");
    assert!(r.is_err());
}

impl Drop for Owned {
    fn drop(&mut self) {
        native_work::TEST_REPO.with(|s| *s.borrow_mut() = None);
        native_work::AFTER_REPORTED.with(|s| {
            s.borrow_mut().take();
        });
    }
}
#[test]
fn production_report_checks_ownership_before_reading_the_head() {
    let h = Harness::new();
    let o = owned(&h);
    commit(&o, "original");
    let mut m = member(&o);
    m.ownership_nonce = uuid::Uuid::new_v4().to_string();
    let read = std::rc::Rc::new(std::cell::Cell::new(false));
    let observed = read.clone();
    native_work::AFTER_REPORTED
        .with(|s| *s.borrow_mut() = Some(Box::new(move || observed.set(true))));
    let t = h.store.team(TEAM).unwrap().unwrap();
    assert!(native(&h).work_reported(&t, &m).is_err());
    assert!(!read.get(), "an unowned checkout's HEAD was read");
}
#[test]
fn production_report_checks_ownership_again_after_reading_the_head() {
    let h = Harness::new();
    let o = owned(&h);
    commit(&o, "original");
    let common = o.root.join(".git");
    let receipt = crate::state::team_worktree_receipt_path(h.temp.path(), &common, &o.wt);
    let metadata = PathBuf::from(git(&o.wt, &o.empty, &["rev-parse", "--absolute-git-dir"]));
    native_work::AFTER_REPORTED.with(|s| {
        *s.borrow_mut() = Some(Box::new(move || {
            let mut owner: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
            owner["id"] = serde_json::json!(uuid::Uuid::new_v4().to_string());
            std::fs::write(receipt, owner.to_string()).unwrap();
            std::fs::write(metadata.join("zerus-owner.json"), owner.to_string()).unwrap();
        }))
    });
    let t = h.store.team(TEAM).unwrap().unwrap();
    assert!(
        native(&h).work_reported(&t, &member(&o)).is_err(),
        "ownership changed during the read"
    );
}
