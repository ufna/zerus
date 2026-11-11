use super::*;
use crate::desktop::original_store::{self, Roots};
use rusqlite::{params, Connection, OpenFlags};
use std::{io::Read, path::Path};

fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or("").into()
}
impl Native<'_> {
    pub fn snapshot(&mut self, b: &Binding) -> Result<Snapshot> {
        let (_, name) = self.binding(&b.team, &b.member)?;
        let observed = self.current(b, &name);
        match observed {
            Ok((snapshot, raw)) => {
                self.capture = Some((snapshot.clone(), raw));
                Ok(snapshot)
            }
            Err(_) => {
                self.capture = None;
                Ok(Snapshot {
                    machine: self.machine.into(),
                    launch: b.launch.clone(),
                    session: b.session.clone(),
                    name,
                    run: b.run.clone(),
                    conversation: b.conversation.clone(),
                    agent: b.agent.clone(),
                    account: b.account.clone(),
                    folder: b.folder.clone(),
                    live: false,
                    ready: false,
                    paused: false,
                    ended: true,
                    self_exit: false,
                    exit_identity: String::new(),
                    archive: None,
                    receipt_at: None,
                    questions: Vec::new(),
                })
            }
        }
    }
    fn current(&self, b: &Binding, name: &str) -> Result<(Snapshot, Value)> {
        let roots =
            Roots::open(&self.config.state, "question_receipts").map_err(|e| e.to_string())?;
        let args = vec![
            "inspect".into(),
            "--session-id".into(),
            b.session.clone(),
            "--after".into(),
            i64::MAX.to_string(),
            "--skip-processes".into(),
        ];
        if let Ok(view) = self.read(args, None) {
            let raw = original_store::session_record(
                &roots,
                &text(&view, "name"),
                &b.run,
                &text(&view, "conversation_id"),
            ).ok_or("A current native session has different or unreadable identity; no archive was selected")?;
            return self.read_view(b, &view, raw, None);
        }
        let (id, raw) = archive(&self.config.state, &roots, b, name)?;
        let view = self.read(
            vec![
                "inspect".into(),
                name.into(),
                "--archive".into(),
                id.clone(),
                "--after".into(),
                i64::MAX.to_string(),
                "--skip-processes".into(),
            ],
            None,
        )?;
        self.read_view(b, &view, raw, Some(id))
    }
    fn read_view(
        &self,
        b: &Binding,
        v: &Value,
        raw: Value,
        archive: Option<String>,
    ) -> Result<(Snapshot, Value)> {
        let mut s = parse_view(self.machine, b, v, &raw, archive)?;
        s.ready = s.live
            && crate::state::resumed_readiness(&raw) == crate::state::ResumedReadiness::Ready;
        if s.live && s.agent == "codex" && !s.conversation.is_empty() && s.questions.is_empty() {
            let request =
                json!({"expected_run_id":s.run,"expected_conversation_id":s.conversation})
                    .to_string();
            s.ready = self
                .read(
                    vec![
                        "send".into(),
                        s.name.clone(),
                        "--observe".into(),
                        "--json".into(),
                    ],
                    Some(request.as_bytes()),
                )
                .is_ok_and(|v| v["ready"] == true);
        }
        Ok((s, raw))
    }
}
pub(super) fn parse_view(
    machine: &str,
    b: &Binding,
    v: &Value,
    raw: &Value,
    archive: Option<String>,
) -> Result<Snapshot> {
    if raw["worktree_owner"]["id"] != b.nonce
        || ["name", "agent", "cwd"]
            .iter()
            .any(|key| v[key] != raw[key])
        || !v["pending_questions_error"].is_null()
        || v["tracked"] != true
        || v["run_id"] != b.run
        || v["session_id"] != b.session
        || v["launch_id"] != b.launch
        || v["conversation_id"] != raw["conversation_id"]
    {
        return Err("The fresh member inspection is incomplete or changed".into());
    }
    let questions = v["pending_questions"]
        .as_array()
        .ok_or("Pending questions are unavailable")?
        .clone();
    let live = v["runtime_state"] == "live" && v["process_state"] == "running";
    let ended = archive.is_some() || v["process_state"] == "exited";
    let s = Snapshot {
        machine: machine.into(),
        launch: text(raw, "launch_id"),
        session: text(raw, "session_id"),
        name: text(raw, "name"),
        run: text(raw, "run_id"),
        conversation: text(raw, "conversation_id"),
        agent: text(raw, "agent"),
        account: text(raw, "account_id"),
        folder: text(raw, "cwd"),
        live,
        ready: false,
        paused: paused(raw),
        ended,
        self_exit: self_exit(raw, archive.is_some()),
        exit_identity: super::super::engine::key(&json!([
            raw["completion_source"],
            raw["completion_reason"],
            raw["exited_at"],
            raw["exit_code"],
            raw["termination_signal"],
            archive
        ])),
        archive,
        receipt_at: None,
        questions,
    };
    super::super::policy::session(b, &s)?;
    Ok(s)
}
pub(super) fn paused(v: &Value) -> bool {
    v["paused"] != Value::Null && v["paused"] != false
        || ["pausing", "terminating", "restore_archive_cleanup"]
            .iter()
            .any(|k| v.get(k).is_some())
}
pub(super) fn self_exit(v: &Value, archived: bool) -> bool {
    if paused(v)
        || v["completion_source"] != "supervisor"
        || !v["exited_at"]
            .as_f64()
            .is_some_and(|t| t.is_finite() && t > 0.0)
        || v["conversation_id"].as_str().is_none_or(str::is_empty)
    {
        return false;
    }
    match v["completion_reason"].as_str() {
        Some("clean_exit") => v["exit_code"] == 0 && v["termination_signal"].is_null(),
        Some("unconfirmed_exit" | "signal") if !archived => {
            v["exit_code"].as_i64().is_some()
                || v["termination_signal"].as_i64().is_some_and(|n| n > 0)
        }
        _ => false,
    }
}
fn archive(state: &Path, roots: &Roots, b: &Binding, name: &str) -> Result<(String, Value)> {
    let db = Connection::open_with_flags(
        state.join("archive-catalog-v1.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|e| e.to_string())?;
    db.busy_timeout(Duration::from_millis(100))
        .map_err(|e| e.to_string())?;
    let mut q = db
        .prepare("SELECT id FROM archive_rows WHERE name=?1 AND run=?2 LIMIT 2")
        .map_err(|e| e.to_string())?;
    let ids = q
        .query_map(params![name, b.run], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if ids.len() != 1
        || !uuid::Uuid::parse_str(&ids[0]).is_ok_and(|id| !id.is_nil() && id.to_string() == ids[0])
    {
        return Err("No exact unique exit archive".into());
    }
    let file = roots
        .open_file(&["archives", &format!("{}.json", ids[0])])
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take((1 << 20) + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1 << 20 {
        return Err("Exit archive exceeds one MiB".into());
    }
    let raw: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if raw["archive_id"] != ids[0]
        || raw["name"] != name
        || raw["run_id"] != b.run
        || raw["state"] != "archived"
    {
        return Err("The exact archive identity changed".into());
    }
    Ok((ids[0].clone(), raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn native_fixture() -> (Binding, Value, Value) {
        let (b, _, s) = crate::desktop::teams::confirm::policy::tests::sample();
        let raw = json!({"name":s.name,"run_id":b.run,"conversation_id":"","session_id":b.session,"launch_id":b.launch,"agent":b.agent,"account_id":b.account,"cwd":b.folder,"worktree_owner":{"id":b.nonce}});
        let mut view = raw.clone();
        view["tracked"] = json!(true);
        view["runtime_state"] = json!("live");
        view["process_state"] = json!("running");
        view["pending_questions"] = json!(s.questions);
        (b, view, raw)
    }
    #[test]
    fn native_read_requires_the_bound_launch_run_session_agent_account_folder_and_owner() {
        let (b, v, raw) = native_fixture();
        let s = parse_view(&b.machine, &b, &v, &raw, None).unwrap();
        super::super::super::policy::trust(&b, &s).unwrap();
        for field in [
            "run_id",
            "session_id",
            "launch_id",
            "agent",
            "account_id",
            "cwd",
        ] {
            let mut other = raw.clone();
            other[field] = json!("other");
            let mut view = v.clone();
            view[field] = other[field].clone();
            assert!(
                parse_view(&b.machine, &b, &view, &other, None).is_err(),
                "{field}"
            );
        }
        let mut other = raw.clone();
        other["worktree_owner"]["id"] = json!("other");
        assert!(parse_view(&b.machine, &b, &v, &other, None).is_err());
        for field in [
            "tracked",
            "pending_questions",
            "pending_questions_error",
            "name",
            "conversation_id",
            "run_id",
            "session_id",
            "launch_id",
            "agent",
            "cwd",
        ] {
            let mut view = v.clone();
            view[field] = json!("other");
            assert!(
                parse_view(&b.machine, &b, &view, &raw, None).is_err(),
                "{field}"
            );
        }
        let mut bound = b.clone();
        bound.conversation = "conversation-1".into();
        assert!(parse_view(&b.machine, &bound, &v, &raw, None).is_err());
    }
    #[test]
    fn clean_supervisor_archive_and_explicit_crashes_are_eligible_but_ambiguous_exits_are_manual() {
        let clean = json!({"completion_source":"supervisor","completion_reason":"clean_exit","exited_at":1000.0,"exit_code":0,"termination_signal":null,"conversation_id":"conversation-1"});
        assert!(self_exit(&clean, true));
        for (key, value) in [
            ("completion_source", json!("manual_archive")),
            ("completion_source", json!("manual_terminate")),
            ("completion_reason", json!("user_archived")),
            ("completion_reason", json!("user_terminated")),
            ("completion_reason", json!("pause")),
            ("exited_at", Value::Null),
            ("conversation_id", json!("")),
            ("exit_code", json!(1)),
            ("paused", json!(true)),
            ("pausing", Value::Null),
            ("terminating", Value::Null),
        ] {
            let mut v = clean.clone();
            v[key] = value;
            assert!(!self_exit(&v, true), "{key}");
        }
        let mut crash = clean.clone();
        crash["completion_reason"] = json!("unconfirmed_exit");
        crash["exit_code"] = json!(1);
        assert!(self_exit(&crash, false));
        assert!(!self_exit(&crash, true));
        crash["completion_reason"] = json!("signal");
        crash["exit_code"] = Value::Null;
        crash["termination_signal"] = json!(9);
        assert!(self_exit(&crash, false));
        assert!(!self_exit(&crash, true));
        crash["termination_signal"] = Value::Null;
        assert!(!self_exit(&crash, false));
    }
}
