//! Codex's startup trust panel precedes SessionStart and has no conversation ID.
//! Only a complete panel in a supervised startup run can be answered. Its path
//! and full permission disclosure are part of the identity, never an implicit approval.
use super::*;
use sha2::{Digest, Sha256};

const TITLE: &str = "Folder access";
const HINT: &str = "enter continue · esc quit";
const EXPLANATION: &str = "Trust this folder? Codex can read, edit, and run files here, subject to your permission settings. Folder settings can run code automatically, even without a model request. Continue only if you trust these files. Your trust decision will be saved.";
const LABELS: [&str; 2] = ["Trust and continue", "Quit"];
const DESCRIPTIONS: [&str; 2] = ["Trust this folder and remember the decision.", "Exit without trusting this folder."];

struct Panel {
    folder: String,
    disclosure: String,
    selected: usize,
}

fn parse(screen: &str) -> Option<Panel> {
    let rows: Vec<_> = screen.trim().lines().map(str::trim).filter(|r| !r.is_empty()).collect();
    if rows.len()<6 || rows[0]!=TITLE || !rows[1].starts_with('/') || *rows.last()?!=HINT {
        return None;
    }
    let options=rows.len()-3;
    let mut selected=None;
    for i in 0..2 {
        let row=rows[options+i];
        let label=if let Some(label)=row.strip_prefix("› ") {
            if selected.replace(i).is_some(){return None;} label
        } else {row};
        if label!=format!("{}. {}",i+1,LABELS[i]) {return None;}
    }
    let disclosure=rows[2..options].join(" ");
    if disclosure.split_whitespace().collect::<Vec<_>>().join(" ")!=EXPLANATION {return None;}
    Some(Panel{folder:rows[1].into(),disclosure,selected:selected?})
}

fn eligible(record: &Value) -> bool {
    string(record, "agent") == "codex"
        && record["run_identity_version"] == 1
        && record["supervisor"].is_object()
        && !string(record, "run_id").is_empty()
        && string(record, "conversation_id").is_empty()
        && string(record, "error").is_empty()
        && !pause_active(record)
        && record.get("session_end").is_none()
        && record["last_event_at"].as_f64().unwrap_or(0.) == 0.
        && record["input_pending_at"].as_f64().unwrap_or(0.) == 0.
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
                record["expected_id"],
                panel.folder,
                panel.disclosure,
                LABELS,
                DESCRIPTIONS
            ])
            .to_string()
        )
    );
    Some(
        json!({"question_id":format!("codex-trust:{hash}"),"question_hash":hash,
        "tool_call_id":"codex_folder_trust","agent_id":"main","run_id":record["run_id"],
        "conversation_id":record["conversation_id"],"created_at":0,"source":"codex_folder_trust",
        "can_answer":true,"answer_transport":"codex_tui","answer_unavailable_reason":"",
        "questions":[{"id":"trust","header":"Folder trust","question":"Trust this folder?",
            "body":format!("{}\n\n{}",panel.folder,panel.disclosure),"multi_select":false,"allow_other":false,
            "options":LABELS.iter().enumerate().map(|(i,label)|json!({"id":format!("trust_{i}"),"label":label,"description":DESCRIPTIONS[i]})).collect::<Vec<_>>()}]}),
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
    let screen = kimi_tui_choice::capture(&record)?;
    if snapshot(&record, &screen).map(|s| s.0) != Some(question["question_hash"].clone()) {
        return Err("Codex's folder trust request changed; refresh Activity".into());
    }
    Ok(())
}

pub(super) fn answer(record: &Value, question: &Value, answers: &Value) -> Result<()> {
    let chosen = match answers[0]["selected_option_ids"][0].as_str() {
        Some("trust_0") => 0,
        Some("trust_1") => 1,
        _ => return Err("Choose whether to trust this folder".into()),
    };
    kimi_tui_choice::answer(
        record,
        question,
        chosen,
        LABELS.len(),
        snapshot,
        |screen| !screen.trim().is_empty() && !screen.contains(TITLE) && !screen.contains(HINT),
        chosen == 1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const SCREEN: &str = include_str!("../../tests/fixtures/codex-folder-trust.txt");
    #[test]
    fn complete_native_panel_only() {
        assert_eq!(parse(SCREEN).unwrap().selected,0);
        for invalid in [SCREEN.replace(HINT,"enter continue"),SCREEN.replace("  2. Quit","› 2. Quit"),
            SCREEN.replace("Your trust decision will be saved.","…"),format!("Agent said:\n{SCREEN}"),format!("{SCREEN}\n› composer")] {
            assert!(parse(&invalid).is_none());
        }
    }
    #[test]
    fn startup_identity_pins_folder_and_run_but_not_highlight() {
        let record=json!({"agent":"codex","run_id":"run","run_identity_version":1,"supervisor":{},"launch_dir":"/work/example"});
        let question=card(&record,&parse(SCREEN).unwrap()).unwrap();
        let moved=SCREEN.replace("› 1.","  1.").replace("  2. Quit","› 2. Quit");
        assert_eq!(question["question_hash"],card(&record,&parse(&moved).unwrap()).unwrap()["question_hash"]);
        let mut other=record.clone();other["run_id"]=json!("other");
        assert_ne!(question["question_hash"],card(&other,&parse(SCREEN).unwrap()).unwrap()["question_hash"]);
        other=record.clone();other["conversation_id"]=json!("already-started");
        assert!(card(&other,&parse(SCREEN).unwrap()).is_none());
        assert!(card(&record,&parse(&SCREEN.replace("/work/example","/other/folder")).unwrap()).is_none());
    }
}
