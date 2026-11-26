//! An exact child policy observation in its local supervisor's Activity.
use super::*;
use sha2::{Digest, Sha256};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};

const INTERVAL: Duration = Duration::from_secs(2);
const WRITE_WAIT: Duration = Duration::from_millis(100);
const UNAVAILABLE_SCAN_RETRY: Duration = Duration::from_secs(10);

/// Only a fresh Codex child with a verified local launch link starts an
/// observer. The supervisor still waits for and records native exit normally.
pub(in crate::state) fn start(run: &str, pid: u32) -> Option<Sender<()>> {
    if nonempty_env("HGS_AGENT").as_deref() != Some("codex") {
        return None;
    }
    let id = nonempty_env("HGS_SESSION_ID")?;
    let record = running_child(&id, run, pid)?;
    let link = listed(&record)?;
    if record["startup_kind"] != "new"
        || link["source"] != "launch"
        || !record["fork_parent_id"].is_null()
    {
        return None;
    }
    let config = crate::config::Config::load().ok()?;
    let machine = session_graph::caller_machine(&config).ok()?;
    if link["machine"] != machine {
        return None;
    }
    let run = run.to_owned();
    let (running, stop) = mpsc::channel();
    std::thread::Builder::new()
        .name("child-policy-activity".into())
        .spawn(move || {
            let mut watch = Watch::default();
            while stop.recv_timeout(INTERVAL) == Err(RecvTimeoutError::Timeout) {
                if supervisor::shutdown_requested() {
                    break;
                }
                if let Some(record) = running_child(&id, &run, pid) {
                    watch.tick(record, &machine);
                }
            }
        })
        .ok()?;
    Some(running)
}

fn running_child(id: &str, run: &str, pid: u32) -> Option<Value> {
    tracked(id).filter(|record| {
        record["run_id"] == run
            && record["pid"] == pid
            && record["supervisor"]["pid"] == std::process::id()
    })
}

fn tracked(id: &str) -> Option<Value> {
    let found = session_index::find_metadata(id)?;
    matches!(
        found.kind,
        session_index::Kind::Record | session_index::Kind::IdRecord
    )
    .then_some(found.record)
}

fn exact_parent(link: &Value) -> Option<Value> {
    let record = tracked(string(link, "session_id"))?;
    let pid = u32::try_from(record["pid"].as_u64()?).ok()?;
    (["session_id", "run_id", "conversation_id"]
        .iter()
        .all(|key| link[*key] == record[*key])
        && live(&record)
        && platform::process_state(pid, None) == Ok(platform::ProcessState::Running))
    .then_some(record)
}

#[derive(Default)]
struct Watch(Option<Scan>);

struct Scan {
    child: Value,
    stamp: (FileIdentity, u64),
    notice: Option<Observation>,
    retry_at: Option<Instant>,
}

impl Watch {
    fn tick(&mut self, child: Value, machine: &str) {
        let Some(link) = listed(&child)
            .filter(|link| link["machine"] == machine && floor(&child, link).is_some())
        else {
            return;
        };
        let Some(stamp) = History::open_file(Path::new(string(&child, "transcript")))
            .and_then(|file| History::stamp(&file))
        else {
            return;
        };
        // A settled scan needs fresh native bytes or a binding change.
        // Unreadable history is tried again at a lower rate.
        if self.0.as_ref().is_none_or(|scan| {
            scan.stamp != stamp
                || !same_observation(&scan.child, &child)
                || scan.retry_at.is_some_and(|at| Instant::now() >= at)
        }) {
            let read = scan_child(&child, &link);
            let retry_at = read
                .is_err()
                .then(|| Instant::now() + UNAVAILABLE_SCAN_RETRY);
            self.0 = Some(Scan {
                child: Value::Object(
                    OBSERVATION_FIELDS
                        .iter()
                        .map(|field| ((*field).to_owned(), child[*field].clone()))
                        .collect(),
                ),
                stamp,
                notice: read.ok().flatten(),
                retry_at,
            });
        }
        if let Some(scan) = self.0.as_mut() {
            if scan
                .notice
                .as_ref()
                .is_some_and(|item| item.publish(&scan.child, &link))
            {
                scan.notice = None;
            }
        }
    }
}

fn scan_child(child: &Value, parent: &Value) -> Result<Option<Observation>> {
    let Some(after) = floor(child, parent) else {
        return Ok(None);
    };
    let history = History::open(child).ok_or("Child history is unavailable")?;
    let Some(event) = provider_errors::scan_codex_policy_notice(child, &history.file)? else {
        return Ok(None);
    };
    let Some(at) = event["at"].as_f64().filter(|at| *at >= after) else {
        return Ok(None);
    };
    Ok(Some(Observation { history, event, at }))
}

impl Observation {
    /// True when this observation was published or is conclusively obsolete.
    /// A busy or unreadable writer is retried as a journal operation only.
    fn publish(&self, child: &Value, link: &Value) -> bool {
        if supervisor::shutdown_requested() {
            return true;
        }
        if exact_parent(link).is_none() || !self.history.held_by_child() {
            return false;
        }
        let Some(process) = platform::process_identity(self.history.pid) else {
            return false;
        };
        let Ok(mut db) = journal::event_db() else {
            return false;
        };
        if db.busy_timeout(WRITE_WAIT).is_err() {
            return false;
        }
        let Ok(Some(_writer)) =
            storage::existing_lock_until(&root().join(".lock"), Instant::now() + WRITE_WAIT)
        else {
            return false;
        };
        // Hooks take Root before the journal. Wait for both in that order,
        // then refresh the native observation after any journal contention.
        let Ok(tx) = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate) else {
            return false;
        };
        let Some(parent) = exact_parent(link) else {
            return false;
        };
        if !self.current(child) {
            return false;
        }
        match waits::completed_result_since(link, child, self.at) {
            Some(true) => return true,
            None => return false,
            Some(false) => {}
        }
        match platform::process_state(self.history.pid, Some(&process.birth)) {
            Ok(platform::ProcessState::Running) => {}
            Ok(platform::ProcessState::Exited) => return true,
            _ => return false,
        }
        if exact_parent(link).is_none() {
            return false;
        }
        let event = &self.event;
        let id = notice_id(child, event);
        let value = json!({
            "type": "Notification", "source": "zerus_child_policy", "origin": "task_notification",
            "at": now(), "agent_id": "", "run_id": parent["run_id"], "message_id": id,
            "detail": format!("The provider blocked a request in {}.", string(child, "name")),
            "child_policy_advisory": {
                "scope": "session",
                "child": {"name":child["name"],"session_id":child["session_id"],
                    "run_id":child["run_id"],"conversation_id":child["conversation_id"]},
                "event": event,
            },
        });
        journal::child_policy_notice(&tx, &parent, &id, &value).is_ok() && tx.commit().is_ok()
    }
}

fn notice_id(child: &Value, event: &Value) -> String {
    let digest = Sha256::digest(
        json!([
            "child-policy-activity-v1",
            child["session_id"],
            child["run_id"],
            child["conversation_id"],
            event["event_id"],
        ])
        .to_string(),
    );
    format!("{digest:x}")
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "driven by the disposable native policy Activity test"]
    fn reobserve_live_native_child() {
        let Some(scope) = nonempty_env("ZERUS_CHILD_POLICY_TEST_ROOT") else {
            return;
        };
        assert_eq!(root(), PathBuf::from(scope).join("state"));
        let id = nonempty_env("ZERUS_CHILD_POLICY_TEST_CHILD").unwrap();
        let record = tracked(&id).unwrap();
        assert_eq!(record["name"], "codex/p/child");
        let config =
            crate::config::Config::load().unwrap_or_else(|error| panic!("{}", error.message));
        let machine = session_graph::caller_machine(&config).unwrap();
        let mut watch = Watch::default();
        watch.tick(record, &machine);
        let scan = watch.0.unwrap();
        assert!(scan.notice.is_none(), "the exact observation must settle");
        // Ensure the cold process reached persistence with an eligible event.
        let link = listed(&scan.child).unwrap();
        let notice = scan_child(&scan.child, &link)
            .unwrap()
            .expect("native refusal must remain current");
        let id = notice_id(&scan.child, &notice.event);
        let db = journal::event_db().unwrap();
        let count: u64 = db
            .query_row(
                "SELECT count(*) FROM child_policy_notices_v1 WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
