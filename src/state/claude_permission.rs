//! Claude's native auto-mode onboarding has no question hook, even though
//! SessionStart may already have bound a conversation. Mirror only its complete
//! startup panel; changing the account default always needs an explicit answer.
use super::*;
use sha2::{Digest, Sha256};

const TITLE: &str = "Make auto mode your default permission mode?";
const BODY: &str = "Auto mode lets Claude handle permission prompts automatically. Claude checks each tool call for risky actions and prompt injection before executing, runs the ones it assesses as lower-risk, and blocks the rest.";
const ACCEPT: &str = "Yes, set auto mode as my default permission mode";

struct Panel {
    folder: String,
    body: String,
    labels: [String; 2],
    selected: usize,
}

fn parse(screen: &str) -> Option<Panel> {
    let rows: Vec<_> = screen.trim().lines().map(str::trim).collect();
    if rows.len() < 10 || !rows[0].contains("Claude Code v") {
        return None;
    }
    let folder = rows[2].get(rows[2].find('/')?..)?.to_owned();
    let divider = rows
        .iter()
        .position(|row| row.chars().count() >= 40 && row.chars().all(|c| c == '─'))?;
    let end = rows.len().checked_sub(3)?;
    if divider < 3
        || end <= divider + 3
        || rows[divider + 1] != TITLE
        || !rows[divider + 2].is_empty()
        || !rows[end].is_empty()
    {
        return None;
    }
    let body = rows[divider + 3..end].join("\n");
    if body.split_whitespace().collect::<Vec<_>>().join(" ") != BODY {
        return None;
    }
    let mut labels = [String::new(), String::new()];
    let mut selected = None;
    for (i, row) in rows[end + 1..].iter().enumerate() {
        let label = if let Some(value) = row.strip_prefix('❯') {
            if selected.replace(i).is_some() {
                return None;
            }
            value.trim_start()
        } else {
            row
        };
        labels[i] = label.to_string();
    }
    // The native decline label contains the current mode. Preserve it verbatim
    // and include it in the question identity instead of assuming bypass.
    if labels[0] != ACCEPT
        || !labels[1].starts_with("No, keep ")
        || labels[1].len() <= "No, keep ".len()
        || labels[1].len() > 100
    {
        return None;
    }
    Some(Panel {
        folder,
        body,
        labels,
        selected: selected?,
    })
}

fn eligible(record: &Value) -> bool {
    string(record, "agent") == "claude"
        && record["run_identity_version"] == 1
        && record["supervisor"].is_object()
        && !string(record, "run_id").is_empty()
        && string(record, "error").is_empty()
        && !pause_active(record)
        && record.get("exited_at").is_none()
        && record.get("session_end").is_none()
        && record["turn_started"].as_f64().unwrap_or(0.) == 0.
        && record["input_pending_at"].as_f64().unwrap_or(0.) == 0.
        && record["active_tools"]
            .as_object()
            .is_none_or(|tools| tools.is_empty())
        && ["", "unknown", "idle", "input", "approval"].contains(&string(record, "phase"))
}

fn card(record: &Value, panel: &Panel) -> Option<Value> {
    if !eligible(record) || panel.folder != string(record, "launch_dir") {
        return None;
    }
    let hash = format!(
        "{:x}",
        Sha256::digest(
            json!([
                record["run_id"],
                record["pane"],
                record["pid"],
                record["process_start"],
                record["conversation_id"],
                record["expected_id"],
                panel.folder,
                TITLE,
                panel.body,
                panel.labels
            ])
            .to_string()
        )
    );
    Some(
        json!({"question_id":format!("claude-permissions:{hash}"),"question_hash":hash,
        "tool_call_id":"claude_permission_mode","agent_id":"main","run_id":record["run_id"],
        "conversation_id":record["conversation_id"],"created_at":0,"source":"claude_permission_mode",
        "can_answer":true,"answer_transport":"claude_tui","answer_unavailable_reason":"",
        "questions":[{"id":"permission_mode","header":"Default permission mode","question":TITLE,
            "body":panel.body,"multi_select":false,"allow_other":false,
            "options":panel.labels.iter().enumerate().map(|(i,label)|json!({"id":format!("mode_{i}"),"label":label})).collect::<Vec<_>>()}]}),
    )
}

fn snapshot(record: &Value, screen: &str) -> Option<(Value, usize)> {
    let panel = parse(screen)?;
    Some((
        card(record, &panel)?["question_hash"].clone(),
        panel.selected,
    ))
}

pub(super) fn current(record: &Value) -> Option<Value> {
    if !eligible(record) {
        return None;
    }
    let panel = parse(&kimi_tui_choice::capture(record).ok()?)?;
    if !process_alive(record) || !matches(record, live().ok()?.get(string(record, "name"))) {
        return None;
    }
    input::checked_terminal(record, false).ok()?;
    card(record, &panel)
}

pub(super) fn available(record: &Value, question: &Value) -> Result<()> {
    let record = kimi_tui_choice::identity(record)?;
    if snapshot(&record, &kimi_tui_choice::capture(&record)?).map(|s| s.0)
        != Some(question["question_hash"].clone())
    {
        return Err("Claude's permission mode question changed; refresh Activity".into());
    }
    Ok(())
}

pub(super) fn answer(record: &Value, question: &Value, answers: &Value) -> Result<()> {
    let chosen = match answers[0]["selected_option_ids"][0].as_str() {
        Some("mode_0") => 0,
        Some("mode_1") => 1,
        _ => return Err("Choose whether to use auto mode or keep the current mode".into()),
    };
    kimi_tui_choice::answer(
        record,
        question,
        chosen,
        2,
        snapshot,
        |screen| {
            !screen.trim().is_empty()
                && !screen.contains(TITLE)
                && !screen.contains(ACCEPT)
                && !screen.contains("No, keep ")
        },
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const SCREEN: &str = include_str!("../../tests/fixtures/claude-auto-mode.txt");
    fn record() -> Value {
        json!({"agent":"claude","run_id":"run","run_identity_version":1,
        "supervisor":{},"launch_dir":"/work/example","conversation_id":"bound","phase":"idle","last_event_at":100})
    }
    #[test]
    fn recognizes_complete_native_question_after_session_start() {
        let panel = parse(SCREEN).unwrap();
        let q = card(&record(), &panel).unwrap();
        assert_eq!(
            q["questions"][0]["options"][1]["label"],
            "No, keep bypass permissions"
        );
        assert_eq!(q["conversation_id"], "bound");
        assert!(q["questions"][0]["options"][0].get("selected").is_none());
        for invalid in [
            SCREEN.replace("and blocks the rest.", ""),
            SCREEN.replace(ACCEPT, "Yes, continue"),
            format!("User quoted:\n{SCREEN}"),
            format!("{SCREEN}\n❯ "),
            SCREEN.replace("No, keep", "❯ No, keep"),
            SCREEN.replace("❯ Yes", "Yes"),
        ] {
            assert!(parse(&invalid).is_none());
        }
    }
    #[test]
    fn highlights_are_not_identity_but_modes_runs_and_conversations_are() {
        let r = record();
        let q = card(&r, &parse(SCREEN).unwrap()).unwrap();
        let moved = SCREEN
            .replace("❯ Yes", "  Yes")
            .replace("  No, keep", "❯ No, keep");
        assert_eq!(
            q["question_hash"],
            card(&r, &parse(&moved).unwrap()).unwrap()["question_hash"]
        );
        let changed = SCREEN.replace("bypass permissions", "default permissions");
        assert_ne!(
            q["question_hash"],
            card(&r, &parse(&changed).unwrap()).unwrap()["question_hash"]
        );
        let mut changed = r.clone();
        changed["conversation_id"] = json!("other");
        assert_ne!(
            q["question_hash"],
            card(&changed, &parse(SCREEN).unwrap()).unwrap()["question_hash"]
        );
        changed["turn_started"] = json!(101);
        assert!(card(&changed, &parse(SCREEN).unwrap()).is_none());
        changed = r;
        changed["launch_dir"] = json!("/other");
        assert!(card(&changed, &parse(SCREEN).unwrap()).is_none());
    }
}
