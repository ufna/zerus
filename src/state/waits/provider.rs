//! A waiter can preserve an observed policy refusal without sending anything.
//! The wait journal stores a missing result separately from successful receipts.
use super::*;

struct Check<'a> {
    id: &'a str,
    machine: &'a str,
    owner: &'a Value,
}

/// Pin the actual waiter. Do not take the SQLite writer during native reads.
pub(super) fn check(
    input: &Value,
    config: &crate::config::Config,
    machine: &str,
) -> anyhow::Result<Value> {
    let owner = session_graph::send_origin().context("Run this check from the waiting session")?;
    let check = Check {
        id: text(input, "wait_id"),
        machine,
        owner: &owner,
    };
    let path = path();
    check.observe(&path, |value| {
        let reference =
            crate::session_reference::Reference::parse(text(value, "producer_reference"))
                .map_err(|error| anyhow::anyhow!(error.message))?;
        ensure!(
            reference.machine == machine && !reference.run.is_empty(),
            "Check a producer on this computer with an exact run reference"
        );
        crate::session_reference::inspect(config, &reference)
            .map_err(|error| anyhow::anyhow!(error.message))
    })
}

impl Check<'_> {
    fn read(&self, db: &Connection) -> anyhow::Result<(Value, String, f64)> {
        ensure!(uuid(self.id), "A canonical wait UUID is required");
        let (raw, state, updated): (String, String, f64) = db.query_row(
            "SELECT payload,state,updated FROM waits WHERE id=?1",
            [self.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let value: Value = serde_json::from_str(&raw)?;
        ensure!(
            text(&value["waiter"]["endpoint"], "machine") == self.machine
                && self.owner["endpoint"] == value["waiter"]["endpoint"]
                && self.owner["run_id"] == value["waiter"]["run_id"],
            "Wait belongs to a different waiting run or conversation"
        );
        Ok((value, state, updated))
    }

    fn unambiguous(&self, db: &Connection, value: &Value) -> anyhow::Result<()> {
        let producer = &value["producer"];
        let competing: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM waits
             WHERE waiter_run=?1 AND waiter_conversation=?2 AND id<>?3
             AND state IN ('awaiting_ack','awaiting_result')
             AND json_extract(payload,'$.producer.machine')=?4
             AND json_extract(payload,'$.producer.conversation_id')=?5
             AND json_extract(payload,'$.producer.provider')=?6
             AND (json_extract(payload,'$.producer.run_id') IN ('',?7)))",
            params![
                text(self.owner, "run_id"),
                text(&self.owner["endpoint"], "conversation"),
                self.id,
                text(producer, "machine"),
                text(producer, "conversation_id"),
                text(producer, "provider"),
                text(producer, "run_id"),
            ],
            |row| row.get(0),
        )?;
        ensure!(
            !competing,
            "Several dependencies name this producer; the refusal cannot identify one result"
        );
        Ok(())
    }

    fn observe(
        &self,
        path: &Path,
        inspect: impl FnOnce(&Value) -> anyhow::Result<Value>,
    ) -> anyhow::Result<Value> {
        let reader = open_at(path, false)?;
        let (value, state, updated) = self.read(&reader)?;
        if state != "awaiting_result" || value["producer_refusal"].is_object() {
            return Ok(public(&value, &state));
        }
        self.unambiguous(&reader, &value)?;
        let snapshot = inspect(&value)?;
        let after = value["acknowledged_at"].as_f64().unwrap_or(updated);
        let Some(evidence) = policy_evidence(&value["producer"], &snapshot, after, now()) else {
            return Ok(public(&value, &state));
        };
        let mut writer = open_at(path, true)?;
        let tx = writer.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (mut current, state, current_updated) = self.read(&tx)?;
        if state != "awaiting_result" || current_updated != updated {
            return Ok(public(&current, &state));
        }
        self.unambiguous(&tx, &current)?;
        current["producer_refusal"] = evidence;
        tx.execute(
            "UPDATE waits SET updated=max(updated+0.001,?2),payload=?3 WHERE id=?1",
            params![self.id, now(), current.to_string()],
        )?;
        tx.execute(
            "UPDATE owners SET revision=revision+1 WHERE run=?1 AND conversation=?2",
            params![
                text(self.owner, "run_id"),
                text(&self.owner["endpoint"], "conversation")
            ],
        )?;
        tx.commit()?;
        Ok(public(&current, &state))
    }
}

fn policy_evidence(producer: &Value, row: &Value, after: f64, observed: f64) -> Option<Value> {
    let run = text(producer, "run_id");
    let error = &row["provider_error"];
    if run.is_empty()
        || row["run_id"] != run
        || row["conversation_id"] != producer["conversation_id"]
        || row["agent"] != producer["provider"]
        || row["runtime_state"] != "live"
        || row["process_state"] != "running"
        || row["phase"] != "error"
        || error["agent_id"] != ""
        || error["type"] != "StopFailure"
        || error["error_kind"] != "provider_policy"
    {
        return None;
    }
    let at = error["at"].as_f64()?;
    let id = text(error, "message_id");
    let source = text(error, "source");
    if !after.is_finite()
        || !at.is_finite()
        || at <= after
        || at > observed
        || id.is_empty()
        || !clean(id, 1024)
    {
        return None;
    }
    if !match (text(row, "agent"), source) {
        (_, "provider_hook") => id
            .strip_prefix(run)
            .and_then(|tail| tail.strip_prefix(':'))
            .and_then(|time| time.parse::<f64>().ok())
            .is_some_and(|time| (time - at).abs() < 0.000_5),
        ("codex", "codex_native_log") => {
            id.strip_prefix("codex-error-")
                .is_some_and(|seq| seq.parse::<u64>().is_ok())
                && !text(error, "turn_id").is_empty()
                && row["turn_started"]
                    .as_f64()
                    .is_some_and(|start| start > 0.0 && start <= at)
        }
        ("codex", "codex_native_transcript") => {
            error["native_turn_complete"] == true && !text(error, "turn_id").is_empty()
        }
        ("dsh", "dsh_native") => id
            .strip_prefix("dsh-error-")
            .is_some_and(|seq| seq.parse::<u64>().is_ok()),
        ("opencode", "opencode_native") => id.starts_with("opencode-error-msg_"),
        _ => false,
    } {
        return None;
    }
    // Keep the native identity, not the request, refusal prose, or reply ticket.
    Some(
        json!({"kind":"provider_policy","source":source,"event_id":id,"at":at,
        "turn_id":error.get("turn_id")}),
    )
}

pub(super) fn missing_preview(
    db: &Connection,
    run: &str,
    conversation: &str,
) -> anyhow::Result<Option<Value>> {
    let mut query = db.prepare(
        "SELECT payload FROM waits WHERE waiter_run=?1 AND waiter_conversation=?2
         AND state='awaiting_result' AND json_type(payload,'$.producer_refusal')='object'
         ORDER BY seq DESC LIMIT ?3",
    )?;
    let rows = query.query_map(params![run, conversation, PREVIEW_LIMIT + 1], |row| {
        row.get::<_, String>(0)
    })?;
    let mut items = Vec::new();
    for row in rows {
        if items.len() == PREVIEW_LIMIT as usize {
            return Ok(Some(json!({"items":items,"truncated":true})));
        }
        items.push(public(&serde_json::from_str(&row?)?, "awaiting_result"));
    }
    Ok((!items.is_empty()).then(|| json!({"items":items,"truncated":false})))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{fixture, operation};
    use super::*;

    fn proof(at: f64) -> Value {
        json!({"agent":"codex","run_id":"run-two","conversation_id":"conversation-two",
            "runtime_state":"live","process_state":"running","phase":"error","turn_started":at-0.5,
            "provider_error":{"agent_id":"","type":"StopFailure","error_kind":"provider_policy",
                "source":"codex_native_log","message_id":"codex-error-91","turn_id":"turn-example",
                "at":at,"detail":"private provider text"}})
    }

    fn register(
        db: &mut Connection,
        mut input: Value,
        owner: &Value,
        run: &str,
    ) -> (Value, String) {
        let id = uuid::Uuid::new_v4().to_string();
        let machine = text(&owner["endpoint"], "machine");
        input["request_id"] = json!(id);
        input["producer_reference"] = json!(format!(
            "zerus://session?machine={machine}&host=fixture&name=codex%2Fexample%2Fworker&conversation=conversation-two&run={run}"
        ));
        let record = begin(db, &input, owner).unwrap();
        let ticket = record["reply_ticket"].clone();
        settle(
            db,
            "ack",
            &operation(&ticket),
            machine,
            |_, _| unreachable!(),
        )
        .unwrap();
        (ticket, id)
    }

    #[test]
    fn missing_review_is_reported_once_and_preserves_completed_and_independent_work() {
        let (dir, mut db, owner, input) = fixture();
        let db_path = dir.path().join("waits.sqlite3");
        let machine = text(&owner["endpoint"], "machine");
        let (old_ticket, old) = register(&mut db, input.clone(), &owner, "run-two");
        settle(
            &mut db,
            "finish",
            &operation(&old_ticket),
            machine,
            |_, _| Ok(json!({"request_id":"complete-original","submitted_at":now()})),
        )
        .unwrap();
        let (ticket, failed) = register(&mut db, input.clone(), &owner, "run-two");
        let (_, independent) = register(&mut db, input, &owner, "other-run");
        let at = now();
        let check = Check {
            id: &failed,
            machine,
            owner: &owner,
        };
        let blocked = check.observe(&db_path, |_| Ok(proof(at))).unwrap();
        assert_eq!(blocked["state"], "awaiting_result");
        assert_eq!(blocked["producer_refusal"]["turn_id"], "turn-example");
        assert!(!blocked.to_string().contains("private provider text"));
        assert!(!blocked.to_string().contains("reply_token"));
        let session = json!({"run_id":"run-one","conversation_id":"conversation-one"});
        let before = summary(&db, &session).unwrap();
        assert_eq!(before["count"], 2);
        assert_eq!(before["items"][0]["id"], failed);
        assert_eq!(before["items"][1]["id"], independent);
        assert_eq!(before["missing_results"]["items"], json!([blocked]));
        let page = page(&db, "run-one", "conversation-one", i64::MAX).unwrap();
        assert_eq!(
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["id"] == old)
                .unwrap()["state"],
            "completed"
        );
        assert_eq!(
            check.observe(&db_path, |_| unreachable!()).unwrap(),
            blocked
        );
        assert_eq!(summary(&db, &session).unwrap(), before);
        // A later unrelated failure cannot invalidate the authorized result.
        let completed = settle(&mut db, "finish", &operation(&ticket), machine, |_, _| {
            Ok(json!({"request_id":"original-result","submitted_at":now()}))
        })
        .unwrap();
        assert_eq!(completed["state"], "completed");
        assert!(summary(&db, &session).unwrap()["missing_results"].is_null());
        let (stored, _, _) = check.read(&db).unwrap();
        assert_eq!(stored["producer_refusal"], blocked["producer_refusal"]);
    }

    #[test]
    fn uncertain_transient_ambiguous_and_mismatched_outcomes_do_not_settle_or_retry() {
        let (dir, mut db, owner, input) = fixture();
        let db_path = dir.path().join("waits.sqlite3");
        let machine = text(&owner["endpoint"], "machine");
        let (_, id) = register(&mut db, input.clone(), &owner, "run-two");
        let check = Check {
            id: &id,
            machine,
            owner: &owner,
        };
        let (value, _, _) = check.read(&db).unwrap();
        let before = session_revision(&db);
        let at = now();
        for altered in [
            ("run_id", json!("replacement")),
            ("phase", json!("idle")),
            ("process_state", json!("unknown")),
        ] {
            let mut row = proof(at);
            row[altered.0] = altered.1;
            assert_eq!(
                check.observe(&db_path, |_| Ok(row)).unwrap()["state"],
                "awaiting_result"
            );
        }
        let mut transient = proof(at);
        transient["provider_error"]["error_kind"] = json!("capacity");
        assert_eq!(
            check.observe(&db_path, |_| Ok(transient)).unwrap()["state"],
            "awaiting_result"
        );
        assert_eq!(
            check
                .observe(&db_path, |_| Ok(proof(
                    value["created_at"].as_f64().unwrap() - 1.0
                )))
                .unwrap()["state"],
            "awaiting_result"
        );
        assert!(check
            .observe(&db_path, |_| anyhow::bail!("read unavailable"))
            .is_err());
        assert_eq!(session_revision(&db), before);
        let mut other_owner = owner.clone();
        other_owner["run_id"] = json!("wrong-run");
        assert!(Check {
            owner: &other_owner,
            ..check
        }
        .observe(&db_path, |_| unreachable!())
        .is_err());
        register(&mut db, input, &owner, "run-two");
        assert!(check.observe(&db_path, |_| unreachable!()).is_err());
    }

    fn session_revision(db: &Connection) -> Value {
        summary(
            db,
            &json!({"run_id":"run-one","conversation_id":"conversation-one"}),
        )
        .unwrap()["revision"]
            .clone()
    }
}
