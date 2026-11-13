use super::*;
use serde_json::json;
use zerus_team_plan::{model::SessionKey, TeamFixture};

pub(in crate::desktop::teams::confirm) fn sample() -> (Binding, Facts, Snapshot) {
    let mut fixture: TeamFixture = serde_json::from_str(include_str!(
        "../../../../../crates/team-plan/fixtures/team-queue.json"
    ))
    .unwrap();
    let mut member = fixture.members.remove(1);
    member.state = MemberState::Active;
    member.session = Some(SessionKey {
        machine: fixture.team.home.clone(),
        session_id: "launch_id:launch-1".into(),
    });
    member.worktree = "/workspace/generated-member".into();
    member.ownership_nonce = "owner-1".into();
    let binding = Binding {
        team: fixture.team.id.clone(),
        member: member.id.clone(),
        person: fixture.team.owner.clone(),
        machine: fixture.team.home.clone(),
        role: member.role.clone(),
        agent: "claude".into(),
        account: member.account.clone(),
        launch: "launch-1".into(),
        session: "session-1".into(),
        run: "run-1".into(),
        conversation: String::new(),
        folder: member.worktree.clone(),
        common: "/workspace/repository/.git".into(),
        nonce: member.ownership_nonce.clone(),
    };
    member.agent = "claude".into();
    fixture.team.status = TeamStatus::Running;
    let facts = Facts {
        team: fixture.team,
        member,
        binding: binding.clone(),
        preview: true,
    };
    let question = json!({"source":"claude_folder_trust","question_id":"trust-1","question_hash":"ab".repeat(32),"run_id":"run-1","conversation_id":"","can_answer":true,"answer_transport":"claude_tui","questions":[{"id":"trust","question":"Trust this folder?","body":"/workspace/generated-member\n\nDisclosure","multi_select":false,"allow_other":false,"options":[{"id":"trust_0","label":"No, exit"},{"id":"trust_1","label":"Yes, I trust this folder"}]}]});
    let snapshot = Snapshot {
        machine: binding.machine.clone(),
        launch: binding.launch.clone(),
        session: binding.session.clone(),
        name: "claude/example/member".into(),
        run: "run-1".into(),
        conversation: "".into(),
        agent: binding.agent.clone(),
        account: binding.account.clone(),
        folder: binding.folder.clone(),
        live: true,
        ready: false,
        paused: false,
        ended: false,
        self_exit: false,
        exit_identity: String::new(),
        archive: None,
        receipt_at: None,
        restore_refusal: None,
        questions: vec![question],
    };
    (binding, facts, snapshot)
}

#[test]
fn exact_bound_startup_folder_trust_is_eligible() {
    let (binding, facts, snapshot) = sample();
    stored(&binding, &facts).unwrap();
    session(&binding, &snapshot).unwrap();
    assert_eq!(trust(&binding, &snapshot).unwrap(), snapshot.questions[0]);
}

#[test]
fn every_stored_identity_is_required() {
    for field in [
        "preview",
        "status",
        "home",
        "owner",
        "team",
        "member",
        "role",
        "agent",
        "account",
        "session",
        "session_machine",
        "folder",
        "nonce",
        "binding",
    ] {
        let (binding, mut facts, _) = sample();
        match field {
            "preview" => facts.preview = false,
            "status" => facts.team.status = TeamStatus::Paused,
            "home" => facts.team.home.push('x'),
            "owner" => facts.team.owner.push('x'),
            "team" => facts.team.id.push('x'),
            "member" => facts.member.id.push('x'),
            "role" => facts.member.role.push('x'),
            "agent" => facts.member.agent.push('x'),
            "account" => facts.member.account.push('x'),
            "session" => facts.member.session.as_mut().unwrap().session_id.push('x'),
            "session_machine" => facts.member.session.as_mut().unwrap().machine.push('x'),
            "folder" => facts.member.worktree.push('x'),
            "nonce" => facts.member.ownership_nonce.push('x'),
            "binding" => facts.binding.launch.push('x'),
            _ => unreachable!(),
        }
        assert!(stored(&binding, &facts).is_err(), "{field}");
    }
}

#[test]
fn every_native_identity_is_required() {
    for field in [
        "machine", "launch", "session", "run", "agent", "account", "folder",
    ] {
        let (binding, _, mut snapshot) = sample();
        match field {
            "machine" => snapshot.machine.push('x'),
            "launch" => snapshot.launch.push('x'),
            "session" => snapshot.session.push('x'),
            "run" => snapshot.run.push('x'),
            "agent" => snapshot.agent.push('x'),
            "account" => snapshot.account.push('x'),
            "folder" => snapshot.folder.push('x'),
            _ => unreachable!(),
        }
        assert!(session(&binding, &snapshot).is_err(), "{field}");
    }
}

#[test]
fn all_other_prompts_and_folders_refuse() {
    for field in [
        "ordinary",
        "hooks",
        "permission",
        "sign_in",
        "kimi_cache_hint",
        "two",
        "cannot_answer",
        "run",
        "conversation",
        "hash",
        "question_id",
        "items",
        "three_choices",
        "folder",
        "repository_scope",
        "options",
        "multi",
        "other",
        "paused",
        "ended",
        "not_live",
    ] {
        let (binding, _, mut snapshot) = sample();
        let q = &mut snapshot.questions[0];
        match field {
            "ordinary" => q["source"] = json!("hook"),
            "hooks" => q["source"] = json!("codex_hooks_trust"),
            "permission" => q["source"] = json!("claude_tool_approval"),
            "sign_in" => q["source"] = json!("sign_in"),
            "kimi_cache_hint" => q["source"] = json!("kimi_cache_hint"),
            "two" => snapshot.questions.push(snapshot.questions[0].clone()),
            "cannot_answer" => q["can_answer"] = json!(false),
            "run" => q["run_id"] = json!("other"),
            "conversation" => q["conversation_id"] = json!("other"),
            "hash" => q["question_hash"] = json!("short"),
            "question_id" => q["question_id"] = json!(""),
            "items" => q["questions"].as_array_mut().unwrap().push(json!({})),
            "three_choices" => q["questions"][0]["options"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"third","label":"Other"})),
            "folder" => q["questions"][0]["body"] = json!("/workspace/other\n\nDisclosure"),
            "repository_scope" => {
                q["questions"][0]["question"] = json!("Trust this Git repository?")
            }
            "options" => q["questions"][0]["options"][1]["label"] = json!("Always allow"),
            "multi" => q["questions"][0]["multi_select"] = json!(true),
            "other" => q["questions"][0]["allow_other"] = json!(true),
            "paused" => snapshot.paused = true,
            "ended" => snapshot.ended = true,
            "not_live" => snapshot.live = false,
            _ => unreachable!(),
        }
        assert!(trust(&binding, &snapshot).is_err(), "{field}");
    }
}

#[test]
fn a_learned_conversation_and_every_ownership_field_are_required() {
    let (mut b, _, mut s) = sample();
    b.conversation = "conversation-1".into();
    s.conversation = "conversation-1".into();
    session(&b, &s).unwrap();
    s.conversation.push('x');
    assert!(session(&b, &s).is_err());
    for field in ["exists", "unchanged", "folder", "common", "nonce", "launch"] {
        let mut o = Ownership {
            folder: b.folder.clone(),
            common: b.common.clone(),
            nonce: b.nonce.clone(),
            launch: b.launch.clone(),
            exists: true,
            unchanged: true,
        };
        match field {
            "exists" => o.exists = false,
            "unchanged" => o.unchanged = false,
            "folder" => o.folder.push('x'),
            "common" => o.common.push('x'),
            "nonce" => o.nonce.push('x'),
            _ => o.launch.push('x'),
        }
        assert!(ownership(&b, &o).is_err(), "{field}");
    }
}

#[test]
fn resume_requires_each_self_exit_fact() {
    let (_, _, mut s) = sample();
    s.live = false;
    s.ended = true;
    s.self_exit = true;
    s.questions.clear();
    s.conversation = "conversation-1".into();
    s.exit_identity = "exit-1".into();
    resume(&s).unwrap();
    for field in [
        "live",
        "ended",
        "paused",
        "questions",
        "self_exit",
        "exit",
        "conversation",
    ] {
        let mut v = s.clone();
        match field {
            "live" => v.live = true,
            "ended" => v.ended = false,
            "paused" => v.paused = true,
            "questions" => v.questions.push(json!({})),
            "self_exit" => v.self_exit = false,
            "exit" => v.exit_identity.clear(),
            _ => v.conversation.clear(),
        }
        assert!(resume(&v).is_err(), "{field}");
    }
}

#[test]
fn each_reviewed_agent_has_only_its_folder_grant() {
    for (agent, source, transport, id, label) in [
        (
            "claude",
            "claude_folder_trust",
            "claude_tui",
            "trust_1",
            "Yes, I trust this folder",
        ),
        (
            "codex",
            "codex_folder_trust",
            "codex_tui",
            "trust_0",
            "Trust and continue",
        ),
        (
            "kimi",
            "kimi_folder_trust",
            "kimi_tui",
            "trust_0",
            "Trust this folder",
        ),
    ] {
        let (mut b, _, mut s) = sample();
        b.agent = agent.into();
        s.agent = agent.into();
        let q = &mut s.questions[0];
        q["source"] = json!(source);
        q["answer_transport"] = json!(transport);
        q["questions"][0]["options"] = json!([{"id":id,"label":label},{"id":if id=="trust_0" {"trust_1"}else{"trust_0"},"label":"Exit"}]);
        trust(&b, &s).unwrap();
        s.questions[0]["questions"][0]["options"][1]["id"] = json!(id);
        assert!(trust(&b, &s).is_err(), "{agent}");
    }
}

#[test]
fn a_consistent_folder_prompt_after_session_start_is_manual() {
    let (mut b, _, mut s) = sample();
    b.conversation = "conversation-1".into();
    s.conversation = b.conversation.clone();
    s.questions[0]["conversation_id"] = json!(b.conversation);
    session(&b, &s).unwrap();
    assert!(trust(&b, &s).is_err());
}

/// Claude Code 2.1.322's startup card as `hgs inspect` showed it for a
/// generated member in the first real-agent probe, with synthetic paths and
/// identifiers of the same lengths. Its conversation is null, not empty.
const REAL_CLAUDE_CARD: &str =
    include_str!("../../../../../tests/fixtures/claude-folder-trust-card-2.1.322.json");

pub(in crate::desktop::teams::confirm) fn real_card() -> (Binding, Snapshot) {
    let (mut b, _, mut s) = sample();
    let card: Value = serde_json::from_str(REAL_CLAUDE_CARD).unwrap();
    let body = card["questions"][0]["body"].as_str().unwrap();
    let folder = body.split_once("\n\n").unwrap().0.to_owned();
    let run = card["run_id"].as_str().unwrap().to_owned();
    (b.folder, s.folder) = (folder.clone(), folder);
    (b.run, s.run) = (run.clone(), run);
    s.questions = vec![card];
    (b, s)
}

#[test]
fn a_real_startup_card_with_a_null_conversation_is_eligible() {
    let (b, s) = real_card();
    assert!(s.questions[0]["conversation_id"].is_null());
    assert_eq!(s.conversation, "");
    assert_eq!(trust(&b, &s).unwrap(), s.questions[0]);
    // An empty string is the same absent conversation, as today.
    let mut empty = s.clone();
    empty.questions[0]["conversation_id"] = json!("");
    trust(&b, &empty).unwrap();
}

#[test]
fn only_an_absent_conversation_matches_a_startup_snapshot() {
    let (b, s) = real_card();
    let mut missing = s.clone();
    missing.questions[0]
        .as_object_mut()
        .unwrap()
        .remove("conversation_id");
    let mut cases = vec![("missing", missing)];
    for (name, value) in [
        ("named", json!("conversation-1")),
        ("number", json!(0)),
        ("false", json!(false)),
        ("list", json!([])),
        ("object", json!({})),
    ] {
        let mut other = s.clone();
        other.questions[0]["conversation_id"] = value;
        cases.push((name, other));
    }
    // The run and the question hash stay bound as before.
    for (name, value) in [("run_id", json!(null)), ("run_id", json!(""))] {
        let mut other = s.clone();
        other.questions[0][name] = value;
        cases.push((name, other));
    }
    let mut hash = s.clone();
    hash.questions[0]["question_hash"] = json!(null);
    cases.push(("hash", hash));
    for (name, case) in cases {
        assert_eq!(
            trust(&b, &case).unwrap_err(),
            if name == "hash" {
                "The prompt hash is unavailable"
            } else {
                "The prompt identity changed"
            },
            "{name}"
        );
    }
    // A null card in a session that has a conversation is never a startup.
    let (mut b, mut s) = real_card();
    b.conversation = "conversation-1".into();
    s.conversation = b.conversation.clone();
    assert!(trust(&b, &s).is_err());
}

/// zerus-63rru.60: each producer's startup card, its conversation null as the
/// producers copy it, is the snapshot's startup prompt (the snapshot reads
/// the same record's null as "").
#[test]
fn each_producers_startup_card_is_eligible() {
    for agent in ["claude", "codex", "kimi"] {
        let (mut b, _, mut s) = sample();
        let card = crate::state::trust_fixture::startup_card(agent);
        assert!(card["conversation_id"].is_null(), "{agent}");
        b.agent = agent.into();
        b.folder = crate::state::trust_fixture::FOLDER.into();
        b.run = card["run_id"].as_str().unwrap().into();
        s.agent = b.agent.clone();
        s.folder = b.folder.clone();
        s.run = b.run.clone();
        s.conversation = String::new();
        s.questions = vec![card.clone()];
        assert_eq!(trust(&b, &s), Ok(card), "{agent}");
    }
}
