//! Explicit native /clear, never a prompt or an automatic cache reaction.
use super::*;
use serde::Deserialize;
use std::time::{Duration, Instant};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request { request_id: String, expected_run_id: String, expected_conversation_id: String }

pub(super) fn available(record: &Value, live: bool) -> bool {
    live && AGENTS.contains(&string(record,"agent"))
        && !string(record,"conversation_id").is_empty()
        && record["activity"]=="idle" && record["phase"]=="idle"
        && !pause_active(record) && string(record,"error").is_empty()
        && string(record,"expected_id").is_empty()
        && !record["clear_context_request"].is_object()
        && !compact_context::pending(record)
}
fn command_row(agent: &str, row: &str, x: usize, command: &str) -> bool {
    let before=row.chars().take(x).collect::<String>();
    let after=row.chars().skip(x).collect::<String>();
    match agent {
        "codex" => before.trim()==format!("› {command}") && after.trim().is_empty(),
        "claude" => [format!("❯ {command}"),format!("> {command}")].iter().any(|prompt|prompt==before.replace('\u{a0}'," ").trim()) && after.trim().is_empty(),
        "kimi" => before.trim()==format!("│ > {command}") && after.trim()=="│",
        _ => false,
    }
}
pub(super) fn ready(record: &Value, command: &str) -> Result<bool> {
    let output=tmux(&["display-message","-p","-t",string(record,"pane"),"#{cursor_x}\t#{cursor_y}"].map(str::to_owned),true)?;
    let cursor=String::from_utf8_lossy(&output.stdout);
    let Some((x,y))=cursor.trim().split_once('\t') else {return Ok(false)};
    let (Ok(x),Ok(y))=(x.parse::<usize>(),y.parse::<usize>()) else {return Ok(false)};
    let screen=kimi_tui_choice::capture(record)?;
    Ok(screen.lines().nth(y).is_some_and(|row|command_row(string(record,"agent"),row,x,command)))
}
pub(super) fn dispatch(args: &[String]) -> Result<i32> {
    if args.len()!=2 || args[1]!="--json" {return Err("usage: hgs clear-context <session> --json".into());}
    let mut bytes=Vec::new();io::stdin().take(8193).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    if bytes.len()>8192 {return Err("Clear request is too large".into());}
    let request:Request=serde_json::from_slice(&bytes).map_err(|e|e.to_string())?;
    let uuid=uuid::Uuid::parse_str(&request.request_id).map_err(|_|"Invalid request identity")?;
    let name=&args[0];
    if dsh::exists(name) {return Err("This DeepSeek harness does not expose a native context reset. Use its native UI.".into());}
    let guard=lock(None)?;
    let mut record=input::checked_ready(name,&request.expected_run_id,&request.expected_conversation_id)?;
    if !available(&record,true) {return Err("Wait for the agent to become idle before clearing context".into());}
    input::checked_terminal(&record,true)?;
    if record["pending_settings"].is_object() {return Err("Wait for the pending model settings before clearing context".into());}
    let path=absolute_root()?.join("clear_receipts").join(format!("{uuid}.json"));
    if path.exists() {return Err("This clear was already attempted. Check Terminal before trying again".into());}
    recovery::cancel_for_input(name)?;
    record["clear_context_request"]=json!({"request_id":request.request_id,"conversation_id":request.expected_conversation_id,"at":now()});
    write(&mut record)?;
    atomic(&path,&json!({"status":"in_progress","run_id":request.expected_run_id,"conversation_id":request.expected_conversation_id}).to_string())?;
    // No Ctrl-U/Escape: never discard a native draft or dismiss a live approval.
    let result=(|| {
        tmux(&["send-keys","-t",string(&record,"pane"),"-l","/clear"].map(str::to_owned),true)?;
        let deadline=Instant::now()+Duration::from_secs(3);
        loop {
            let current=kimi_tui_choice::identity(&record)?;
            if ready(&current,"/clear")? {break;}
            if Instant::now()>=deadline {return Err("Native /clear input could not be verified".into());}
            std::thread::sleep(Duration::from_millis(40));
        }
        let current=input::checked_ready(name,&request.expected_run_id,&request.expected_conversation_id)?;
        kimi_tui_choice::identity(&record)?;
        input::checked_terminal(&current,false)?;
        if !ready(&current,"/clear")? {return Err("The native input changed before /clear".into());}
        tmux(&["send-keys","-t",string(&current,"pane"),"Enter"].map(str::to_owned),true)?;
        Ok(())
    })();
    drop(guard); // SessionStart hooks must be able to commit the new identity.
    result.map_err(|e:String|format!("Clear delivery uncertain: {e}. Check Terminal before retrying"))?;
    let deadline=Instant::now()+Duration::from_secs(4);
    let mut status="submitted";
    loop {
        let current=read(name)?;
        if current["run_id"]!=record["run_id"] || current["pid"]!=record["pid"] {break;}
        if !string(&current,"conversation_id").is_empty() && current["conversation_id"]!=record["conversation_id"] {
            status="confirmed";break;
        }
        if Instant::now()>=deadline {break;}
        std::thread::sleep(Duration::from_millis(80));
    }
    let receipt=json!({"request_id":request.request_id,"name":name,"run_id":request.expected_run_id,
        "conversation_id":request.expected_conversation_id,"status":status});
    atomic(&path,&receipt.to_string())?;println!("{receipt}");Ok(0)
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn exact_command_only() {
        assert!(command_row("codex","› /clear",8,"/clear"));
        assert!(command_row("claude","❯\u{a0}/clear",8,"/clear"));
        assert!(command_row("kimi"," │ > /clear       │",11,"/clear"));
        for row in ["› /clear extra","› draft /clear","shell$ /clear"] {assert!(!command_row("codex",row,8,"/clear"));}
    }
}
