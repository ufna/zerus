//! Team delegations have their own entry point and categories. The ordinary
//! automatic-tool admission never admits either of these kinds.
use super::*;

#[derive(Clone, Copy)]
pub(crate) enum TeamKind {
    FolderTrust,
    Resume,
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
        TeamKind::Resume => {
            if payload["question"]["source"] != "team_resume" {
                return Ok(json!({"eligible":false}));
            }
            payload["categories"] = json!(["team_resume"]);
            json!([])
        }
    };
    reserve(db, &payload, answers, matches!(kind, TeamKind::FolderTrust))
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
        || q["conversation_id"] != ""
    {
        return None;
    }
    let items = q["questions"].as_array()?;
    if items.len() != 1
        || items[0]["id"] != "trust"
        || items[0]["question"] != "Trust this folder?"
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
    #[test]
    fn team_kinds_are_distinct_and_ordinary_admission_stays_closed() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = open(&dir.path().join("audit")).unwrap();
        let q = json!({"source":"claude_folder_trust","answer_transport":"claude_tui","can_answer":true,
            "question_id":"trust-1","question_hash":"a".repeat(64),"run_id":"run-1","conversation_id":"",
            "questions":[{"id":"trust","question":"Trust this folder?","options":[{"id":"trust_0","label":"No, exit"},{"id":"trust_1","label":"Yes, I trust this folder"}]}]});
        let mut p = json!({"host":"machine-1","session":"claude/example/member","provider":"claude","question":q});
        assert_eq!(begin(&mut db, &p).unwrap()["eligible"], false);
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
}
