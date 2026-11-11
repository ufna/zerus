use super::*;
use std::{path::PathBuf, process::Command};
struct Git {
    _temp: tempfile::TempDir,
    path: PathBuf,
    empty: PathBuf,
    hooks: PathBuf,
    base: String,
    incoming: String,
}
impl Git {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let hooks = temp.path().join("hooks");
        let empty = temp.path().join("config");
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(&hooks).unwrap();
        std::fs::write(&empty, "").unwrap();
        let mut f = Self {
            _temp: temp,
            path,
            empty,
            hooks,
            base: String::new(),
            incoming: String::new(),
        };
        f.run(&["init", "--initial-branch", "main"]);
        f.run(&["config", "user.name", "Teams Fixture"]);
        f.run(&["config", "user.email", "fixture@example.test"]);
        f.run(&["config", "commit.gpgSign", "false"]);
        f.base = f.commit("base");
        let integration = f.commit("integration");
        f.run(&["branch", "teams/integration", &integration]);
        f.run(&["checkout", "-b", "member", &f.base]);
        f.incoming = f.commit("member");
        f
    }
    fn run(&self, args: &[&str]) -> String {
        let mut cmd = Command::new("git");
        for (k, _) in std::env::vars_os() {
            if k.to_string_lossy().starts_with("GIT_") {
                cmd.env_remove(k);
            }
        }
        cmd.current_dir(&self.path)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &self.empty)
            .arg("-c")
            .arg(format!("core.hooksPath={}", self.hooks.display()))
            .args(["-c", "init.templateDir="])
            .args(args);
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().into()
    }
    fn commit(&self, text: &str) -> String {
        std::fs::write(self.path.join("file.txt"), format!("{text}\n")).unwrap();
        self.run(&["add", "file.txt"]);
        self.run(&["commit", "-m", text]);
        self.run(&["rev-parse", "HEAD"])
    }
}
#[test]
fn an_actor_conflict_records_its_decision_and_changes_no_ref_or_checkout() {
    let git = Git::new();
    let mut h = Harness::empty();
    let mut op = create();
    if let Operation::Create {
        base_commit,
        integration_branch,
        ..
    } = &mut op
    {
        *base_commit = git.base.clone();
        *integration_branch = "teams/integration".into();
    }
    h.run(&person(), op).unwrap();
    let lead = h.bind("lead");
    h.bind("m1");
    let node = h.add(&person(), 1);
    h.submit(node, "m1");
    h.observe(node, TaskStage::InReview);
    h.run(
        &service(),
        Operation::RecordReport {
            node,
            generation: 1,
            member: "m1".into(),
            text: "Conflicting report".into(),
            commit: git.incoming.clone(),
        },
    )
    .unwrap();
    h.run(&lead, Operation::Accept { node }).unwrap();
    let decision = h.accept_decision(node);
    let before = git.run(&["rev-parse", "refs/heads/teams/integration"]);
    let head = git.run(&["rev-parse", "HEAD"]);
    let mut a = actor(&h);
    let mut p = Work {
        repo: Some(integrate::Repo {
            git_dir: git.path.join(".git"),
        }),
        ..Default::default()
    };
    run(&mut h, &mut a, &mut p, 100);
    assert_eq!(p.merges, 1);
    assert_eq!(
        git.run(&["rev-parse", "refs/heads/teams/integration"]),
        before
    );
    assert_eq!(git.run(&["rev-parse", "HEAD"]), head);
    assert_eq!(git.run(&["status", "--porcelain"]), "");
    let fact = h.store.integration_fact(TEAM, node).unwrap().unwrap();
    assert_eq!(fact.decision, decision);
    assert_eq!(fact.conflicts, vec!["file.txt"]);
    assert!(fact.commit.is_none());
}
