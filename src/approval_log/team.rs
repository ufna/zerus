//! Team delegations have their own entry point and categories. The ordinary
//! automatic-tool admission never admits these service kinds.
use super::*;

#[derive(Clone, Copy)]
#[derive(PartialEq)]
pub(crate) enum TeamKind {
    FolderTrust,
    /// Codex hook trust Zerus records before a team member's Codex starts
    /// (zerus-f67qp).
    HookTrust,
    Resume,
    /// Launch context through the guarded send, before a conversation exists.
    Brief,
}

pub(crate) fn begin_team(db: &mut Connection, payload: &Value, kind: TeamKind) -> Result<Value> {
    let mut payload = payload.clone();
    let answers = match kind {
        TeamKind::FolderTrust => {
            let Some(answers) = folder_answers(&payload) else {
                return Ok(json!({"eligible":false}));
            };
            payload["categories"] = json!(["team_folder_trust"]);
            answers
        }
        TeamKind::HookTrust => {
            let Some(answers) = hook_answers(&payload) else {
                return Ok(json!({"eligible":false}));
            };
            payload["categories"] = json!(["team_hook_trust"]);
            answers
        }
        TeamKind::Resume | TeamKind::Brief => {
            if payload["question"]["source"] != "team_resume" {
                return Ok(json!({"eligible":false}));
            }
            if kind == TeamKind::Brief
                && (payload["question"]["can_answer"] != false
                    || !payload["question"]["question_id"].as_str()
                        .is_some_and(|id| id.starts_with("brief:")))
            {
                return Ok(json!({"eligible":false}));
            }
            payload["categories"] = json!(["team_resume"]);
            json!([])
        }
    };
    reserve(
        db,
        &payload,
        answers,
        matches!(
            kind,
            TeamKind::FolderTrust | TeamKind::HookTrust | TeamKind::Brief
        ),
    )
}

/// Codex hook trust Zerus recorded before a team member's Codex started
/// (zerus-f67qp): no native card is answered, so the question is Zerus's own
/// record of the exact hook keys and hashes it trusted, and it has no answer.
fn hook_answers(payload: &Value) -> Option<Value> {
    let q = &payload["question"];
    if payload["provider"] != "codex"
        || q["source"] != "team_hook_trust"
        || q["can_answer"] != false
        || !q["question_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("hooks:"))
    {
        return None;
    }
    let hooks = q["hooks"].as_array()?;
    if hooks.is_empty()
        || !hooks.iter().all(|hook| {
            hook.as_object().is_some_and(|o| o.len() == 2)
                && hook["key"].as_str().is_some_and(|key| !key.is_empty())
                && hook["hash"]
                    .as_str()
                    .is_some_and(|hash| hash.starts_with("sha256:"))
        })
    {
        return None;
    }
    let items = q["questions"].as_array()?;
    if items.len() != 1 || items[0]["id"] != "hooks_trust" {
        return None;
    }
    Some(json!([]))
}

fn folder_answers(payload: &Value) -> Option<Value> {
    let q = &payload["question"];
    let (source, transport, id, label) = match payload["provider"].as_str()? {
        "claude" => (
            "claude_folder_trust",
            "claude_tui",
            "trust_1",
            "Yes, I trust this folder",
        ),
        "codex" => (
            "codex_folder_trust",
            "codex_tui",
            "trust_0",
            "Trust and continue",
        ),
        "kimi" => (
            "kimi_folder_trust",
            "kimi_tui",
            "trust_0",
            "Trust this folder",
        ),
        _ => return None,
    };
    if q["source"] != source
        || q["answer_transport"] != transport
        || q["can_answer"] != true
        // A startup card has no conversation yet: the producers copy the
        // record's null, which is the same identity as "" (zerus-63rru.60).
        || crate::state::native_identity(q, "conversation_id") != Some("")
    {
        return None;
    }
    let items = q["questions"].as_array()?;
    if items.len() != 1
        || items[0]["id"] != "trust"
        || (items[0]["question"] != "Trust this folder?"
            && !(q["source"] == "codex_folder_trust"
                && items[0]["question"] == "Trust this Git repository?"))
        || items[0]["multi_select"] == true
        || items[0]["allow_other"] == true
    {
        return None;
    }
    let options = items[0]["options"].as_array()?;
    if options.iter().filter(|o| o["id"] == id).count() != 1
        || options.len() != 2
        || options
            .iter()
            .filter(|o| {
                o["id"] == id
                    && (o["label"] == label
                        || source == "kimi_folder_trust" && o["label"] == "Trust and continue")
            })
            .count()
            != 1
    {
        return None;
    }
    Some(json!([{"question_id":"trust","selected_option_ids":[id],"text":""}]))
}

#[cfg(test)]
mod tests {
    use super::*;
    mod lead151;
    #[test]
    fn team_kinds_are_distinct_and_ordinary_admission_stays_closed() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = open(&dir.path().join("audit")).unwrap();
        // The card Claude's producer writes for a new run (zerus-63rru.60).
        let q = crate::state::trust_fixture::startup_card("claude");
        let mut p = json!({"host":"machine-1","session":"claude/example/member","provider":"claude","question":q});
        assert_eq!(begin(&mut db, &p).unwrap()["eligible"], false);
        // The hooks kind admits only Zerus's own hook-trust record (zerus-f67qp).
        assert_eq!(
            begin_team(&mut db, &p, TeamKind::HookTrust).unwrap()["eligible"],
            false
        );
        let trust = begin_team(&mut db, &p, TeamKind::FolderTrust).unwrap();
        assert_eq!(trust["fresh"], true);
        assert_eq!(
            begin_team(&mut db, &p, TeamKind::FolderTrust).unwrap()["fresh"],
            false
        );
        p["question"]["source"] = json!("team_resume");
        p["question"]["question_id"] = json!("resume:run-2");
        p["question"]["conversation_id"] = json!("conversation-1");
        assert_eq!(begin(&mut db, &p).unwrap()["eligible"], false);
        assert_eq!(
            begin_team(&mut db, &p, TeamKind::Resume).unwrap()["fresh"],
            true
        );
        let rows = list(&db, &json!({})).unwrap();
        assert_eq!(rows["entries"][0]["categories"], json!(["team_resume"]));
        assert_eq!(
            rows["entries"][1]["categories"],
            json!(["team_folder_trust"])
        );
        finish(
            &db,
            &json!({"id":trust["id"],"request_id":trust["request_id"],"status":"unknown"}),
        )
        .unwrap();
        drop(db);
        let mut db = open(&dir.path().join("audit")).unwrap();
        p["question"] = q;
        assert_eq!(
            begin_team(&mut db, &p, TeamKind::FolderTrust).unwrap()["fresh"],
            false
        );
    }

    /// Codex's own hooks-trust card, as its producer writes it at a new run's
    /// startup and at a resumed one's, is answered by no kind (zerus-f67qp
    /// delta): Zerus never presses Trust all and continue. The hook kind
    /// admits only Zerus's own record of trust it wrote before the start
    /// (`confirm::hook_trust` tests bind that producer).
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn codex_hooks_trust_cards_are_answered_by_no_kind() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = open(&dir.path().join("audit")).unwrap();
        for q in [
            crate::state::trust_fixture::hooks_trust_prompt(),
            crate::state::trust_fixture::resumed_hooks_trust_prompt("conversation-1"),
        ] {
            let p = payload("codex", &q);
            assert_eq!(begin(&mut db, &p).unwrap()["eligible"], false);
            for kind in [TeamKind::FolderTrust, TeamKind::HookTrust] {
                assert_eq!(begin_team(&mut db, &p, kind).unwrap()["eligible"], false);
            }
        }
        assert!(list(&db, &json!({})).unwrap()["entries"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    fn payload(agent: &str, q: &Value) -> Value {
        json!({"host":"machine-1","session":format!("{agent}/example/member"),"provider":agent,"question":q})
    }

    /// The startup card each producer writes is the shape this audit reads
    /// (zerus-63rru.60): a changed shape fails here before a real member is
    /// refused. The conversation is null, as every producer copies it.
    #[test]
    fn each_producers_startup_card_is_the_shape_the_audit_reads() {
        for (agent, option) in [
            ("claude", "trust_1"),
            ("codex", "trust_0"),
            ("kimi", "trust_0"),
        ] {
            let q = crate::state::trust_fixture::startup_card(agent);
            assert!(q["conversation_id"].is_null(), "{agent}");
            let mut keys: Vec<&String> = q.as_object().unwrap().keys().collect();
            keys.sort();
            assert_eq!(
                keys,
                [
                    "agent_id",
                    "answer_transport",
                    "answer_unavailable_reason",
                    "can_answer",
                    "conversation_id",
                    "created_at",
                    "question_hash",
                    "question_id",
                    "questions",
                    "run_id",
                    "source",
                    "tool_call_id"
                ],
                "{agent}"
            );
            assert_eq!(
                folder_answers(&payload(agent, &q)),
                Some(json!([{"question_id":"trust","selected_option_ids":[option],"text":""}])),
                "{agent}"
            );
        }
    }

    /// One question, one reservation: a startup card's null and "" are one
    /// identity. The row keeps the card as it was seen.
    #[test]
    fn a_startup_card_is_reserved_once_and_saved_as_it_was_seen() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = open(&dir.path().join("audit")).unwrap();
        for agent in ["claude", "codex", "kimi"] {
            let q = crate::state::trust_fixture::startup_card(agent);
            assert_eq!(
                begin(&mut db, &payload(agent, &q)).unwrap()["eligible"],
                false
            );
            let first = begin_team(&mut db, &payload(agent, &q), TeamKind::FolderTrust).unwrap();
            assert_eq!(
                (first["eligible"].clone(), first["fresh"].clone()),
                (json!(true), json!(true)),
                "{agent}"
            );
            let mut empty = q.clone();
            empty["conversation_id"] = json!("");
            let second =
                begin_team(&mut db, &payload(agent, &empty), TeamKind::FolderTrust).unwrap();
            assert_eq!(second["fresh"], false, "{agent}");
            assert_eq!(second["id"], first["id"], "{agent}");
            let saved: String = db
                .query_row(
                    "SELECT question FROM attempts WHERE id=?1",
                    [first["id"].as_i64().unwrap()],
                    |r| r.get(0),
                )
                .unwrap();
            let saved: Value = serde_json::from_str(&saved).unwrap();
            assert_eq!(
                saved["question"], q,
                "{agent}: the row keeps the card as seen"
            );
            assert!(saved["question"]["conversation_id"].is_null(), "{agent}");
        }
    }

    /// Another conversation, or one of another type, is no startup card.
    #[test]
    fn a_card_with_a_conversation_or_a_bad_identity_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = open(&dir.path().join("audit")).unwrap();
        for conversation in [json!("conversation-1"), json!(7), json!({})] {
            let mut q = crate::state::trust_fixture::startup_card("claude");
            q["conversation_id"] = conversation.clone();
            assert_eq!(
                begin_team(&mut db, &payload("claude", &q), TeamKind::FolderTrust).unwrap()
                    ["eligible"],
                false,
                "{conversation}"
            );
        }
        let mut q = crate::state::trust_fixture::startup_card("claude");
        q.as_object_mut().unwrap().remove("conversation_id");
        assert_eq!(
            begin_team(&mut db, &payload("claude", &q), TeamKind::FolderTrust).unwrap()["eligible"],
            false
        );
    }
}
