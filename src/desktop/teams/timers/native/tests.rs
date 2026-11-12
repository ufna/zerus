//! The production adapters with a stub in place of this computer's `hgs`.
//! The stub records its arguments and writes what the real CLI would.
use super::*;
use crate::teams::{
    ops::{tests::*, Actor, Operation},
    store::timers::{AnswerBell, Bell, Gate},
};
use std::{cell::RefCell, os::unix::fs::PermissionsExt, path::PathBuf};
type Hook = Box<dyn FnOnce()>;
std::thread_local! {
    pub(super) static PROGRAM: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    /// Runs inside the doorbell's admission, once its lock is held.
    pub(super) static LOCKED: RefCell<Option<Hook>> = const { RefCell::new(None) };
}
const STUB: &str = r#"#!/usr/bin/env python3
import hashlib, json, os, pathlib, sys
root = pathlib.Path(os.environ['HGS_STATE_DIR'])
assert int(os.environ['HGS_TEAMS_CUSTODY_FD']) > 2
assert 'HGS_SESSION' not in os.environ
raw = sys.stdin.buffer.read()
(root / 'stub-args').write_text(json.dumps(sys.argv[1:]))
mode = (root / 'stub-mode').read_text()
if sys.argv[3] == 'send':
    q = json.loads(raw)
    if mode == 'submitted':
        folder = root / 'input_receipts'
        folder.mkdir(mode=0o700, exist_ok=True)
        receipt = {'request_id': q['request_id'], 'name': sys.argv[4], 'run_id': q['expected_run_id'],
                   'conversation_id': q['expected_conversation_id'], 'digest': hashlib.sha256(raw).hexdigest(),
                   'status': 'submitted', 'transport': 'terminal'}
        p = folder / (q['request_id'] + '.json')
        p.write_text(json.dumps(receipt))
        p.chmod(0o600)
        print(json.dumps(receipt))
    elif mode == 'refuse':
        print(json.dumps({'ok': False, 'error': 'The expected run changed'}))
        sys.exit(1)
else:
    q = json.loads(raw)
    reply = dict(q)
    reply.update(json.loads(mode))
    print(json.dumps(reply))
    sys.exit(int(reply.pop('exit', 0)) if 'exit' in reply else 0)
"#;

struct Fixture {
    h: Harness,
    config: Config,
    stop: Arc<AtomicBool>,
}
impl Fixture {
    fn new() -> Self {
        let h = Harness::new();
        std::fs::set_permissions(h.temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = Config {
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
        };
        let stub = h.temp.path().join("stub-hgs");
        std::fs::write(&stub, STUB).unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o700)).unwrap();
        PROGRAM.with(|slot| *slot.borrow_mut() = Some(stub));
        Self {
            h,
            config,
            stop: Arc::new(AtomicBool::new(false)),
        }
    }
    fn mode(&self, mode: &str) {
        std::fs::write(self.h.temp.path().join("stub-mode"), mode).unwrap();
    }
    fn args(&self) -> Option<Vec<String>> {
        let raw = std::fs::read(self.h.temp.path().join("stub-args")).ok()?;
        Some(serde_json::from_slice(&raw).unwrap())
    }
    fn native(&self) -> Native<'_> {
        Native {
            config: &self.config,
            machine: "00000000-0000-4000-8000-0000000000aa",
            stop: &self.stop,
        }
    }
    fn ring(&mut self, agent: &str) -> Ring {
        let (_, lead) = self.h.reported();
        let Actor::Session { session, .. } = lead else {
            unreachable!()
        };
        Ring {
            id: uuid::Uuid::new_v4().to_string(),
            gate: Gate::Lead(Bell {
                team: TEAM.into(),
                member: "lead".into(),
                session,
                after: 0,
            }),
            agent: agent.into(),
            name: "claude/example/lead".into(),
            run: "run-1".into(),
            conversation: "conversation-1".into(),
            text: "1 item waits in your team inbox.".into(),
        }
    }
    /// A member's answer doorbell whose stored admission holds.
    fn answer(&mut self) -> Ring {
        let lead = self.h.bind("lead");
        let m1 = self.h.bind("m1");
        let node = self.h.add(&person(), 1);
        self.h.submit(node, "m1");
        let ask = Operation::Ask {
            node,
            generation: 1,
            text: "Which width?".into(),
        };
        let item = self.h.run(&m1, ask).unwrap().item.unwrap();
        self.h
            .run(
                &lead,
                Operation::Reply {
                    item,
                    text: "The old one.".into(),
                },
            )
            .unwrap();
        let mark = self.h.store.answer_marks(TEAM, "", 1).unwrap().remove(0);
        let Actor::Session { session, .. } = m1 else {
            unreachable!()
        };
        Ring {
            id: uuid::Uuid::new_v4().to_string(),
            gate: Gate::Answer(AnswerBell {
                team: TEAM.into(),
                member: "m1".into(),
                session,
                node,
                generation: 1,
                mark: mark.revision,
            }),
            agent: "claude".into(),
            name: "claude/example/m1".into(),
            run: "run-m1".into(),
            conversation: "conversation-m1".into(),
            text: "1 answer from your lead waits. Read it with: hgs team current".into(),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        PROGRAM.with(|slot| slot.borrow_mut().take());
    }
}

#[test]
fn a_doorbell_is_submitted_only_by_its_own_receipt() {
    let mut f = Fixture::new();
    let ring = f.ring("claude");
    f.mode("submitted");
    assert_eq!(f.native().ring(&f.h.store, &ring), Rung::Submitted);
    let args = f.args().unwrap();
    assert_eq!(args[2..], ["send", "claude/example/lead", "--json"]);
}

#[test]
fn a_refusing_exit_without_a_receipt_is_refused_only_for_receipted_agents() {
    let mut f = Fixture::new();
    let ring = f.ring("claude");
    f.mode("refuse");
    assert_eq!(
        f.native().ring(&f.h.store, &ring),
        Rung::Refused("The expected run changed".into())
    );
    let mut other = ring.clone();
    other.id = uuid::Uuid::new_v4().to_string();
    other.agent = "dsh".into();
    assert!(matches!(
        f.native().ring(&f.h.store, &other),
        Rung::Uncertain(_)
    ));
}

#[test]
fn a_clean_exit_without_a_receipt_is_uncertain() {
    let mut f = Fixture::new();
    let ring = f.ring("claude");
    f.mode("silent");
    assert!(matches!(
        f.native().ring(&f.h.store, &ring),
        Rung::Uncertain(_)
    ));
}

#[test]
fn a_refused_admission_starts_no_child() {
    let mut f = Fixture::new();
    let ring = f.ring("claude");
    f.mode("submitted");
    f.h.run(&person(), Operation::Pause).unwrap();
    assert!(matches!(
        f.native().ring(&f.h.store, &ring),
        Rung::NotStarted(_)
    ));
    assert!(f.args().is_none(), "the stub never ran");
}

#[test]
fn the_clear_child_answers_only_with_an_echo_of_its_request() {
    let f = Fixture::new();
    let task = task(1);
    let request = ClearRequest {
        schema: 1,
        team: TEAM.into(),
        node: 7,
    };
    for (mode, expected) in [
        (r#"{"state":"cleared"}"#, Some(Answer::Cleared)),
        (
            r#"{"state":"refused","error":"The task left"}"#,
            Some(Answer::Refused("The task left".into())),
        ),
        (r#"{"state":"cutting_over"}"#, Some(Answer::CuttingOver)),
        (r#"{"state":"done"}"#, None),
        (r#"{"state":"cleared","node":8}"#, None),
        (r#"{"state":"cleared","exit":3}"#, None),
    ] {
        f.mode(mode);
        let answer = f.native().clear(&task, &request);
        match expected {
            Some(expected) => assert_eq!(answer, expected, "{mode}"),
            None => assert!(matches!(answer, Answer::None(_)), "{mode}: {answer:?}"),
        }
    }
    let args = f.args().unwrap();
    assert_eq!(
        args[2..],
        [
            "task",
            "team-clear",
            task.uuid.as_str(),
            "--project",
            task.project.as_str(),
            "--json"
        ]
    );
}

#[test]
fn a_stop_after_the_lock_is_taken_starts_nothing() {
    let mut f = Fixture::new();
    let ring = f.ring("claude");
    f.mode("submitted");
    let stop = f.stop.clone();
    LOCKED.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || stop.store(true, Ordering::Release)))
    });
    assert!(matches!(
        f.native().ring(&f.h.store, &ring),
        Rung::NotStarted(_)
    ));
    assert!(f.args().is_none(), "the stub never ran");
}

#[test]
fn the_preview_switched_off_after_the_lock_is_taken_starts_nothing() {
    let mut f = Fixture::new();
    let ring = f.ring("claude");
    f.mode("submitted");
    let preview = f.config.state.join("teams/preview");
    LOCKED.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || std::fs::remove_file(preview).unwrap()))
    });
    assert!(matches!(
        f.native().ring(&f.h.store, &ring),
        Rung::NotStarted(_)
    ));
    assert!(f.args().is_none(), "the stub never ran");
}

#[test]
fn an_answer_doorbell_is_admitted_by_its_member_and_stopped_by_the_flag() {
    let mut f = Fixture::new();
    let ring = f.answer();
    f.mode("submitted");
    assert_eq!(f.native().ring(&f.h.store, &ring), Rung::Submitted);
    let mut again = ring.clone();
    again.id = uuid::Uuid::new_v4().to_string();
    let stop = f.stop.clone();
    LOCKED.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || stop.store(true, Ordering::Release)))
    });
    std::fs::remove_file(f.h.temp.path().join("stub-args")).unwrap();
    assert!(matches!(
        f.native().ring(&f.h.store, &again),
        Rung::NotStarted(_)
    ));
    assert!(f.args().is_none(), "the stub never ran");
}
