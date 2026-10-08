//! One explicit native interrupt, pinned to the displayed turn. Never kills
//! the session or retries a keystroke whose delivery might have succeeded.
use super::*;
use serde::Deserialize;

fn eligible(record: &Value) -> bool {
    ["codex", "claude", "kimi"].contains(&string(record, "agent"))
        && record["run_identity_version"] == 1 && record["supervisor"].is_object()
        && record["activity"] == "busy"
        && ["working", "tool", "compacting"].contains(&string(record, "phase"))
        && !pause_active(record) && string(record, "error").is_empty()
}
pub(super) fn available(record: &Value, live: bool) -> bool { live && eligible(record) }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request { request_id: String, expected_run_id: String, expected_conversation_id: String, expected_turn_started: f64 }

pub(super) fn dispatch(args: &[String]) -> Result<i32> {
    if args.len()!=2 || args[1]!="--json" { return Err("usage: hgs interrupt <session> --json".into()); }
    let mut bytes=Vec::new(); io::stdin().take(8193).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    if bytes.len()>8192 {return Err("Interrupt request is too large".into());}
    let request:Request=serde_json::from_slice(&bytes).map_err(|e|e.to_string())?;
    let uuid=uuid::Uuid::parse_str(&request.request_id).map_err(|_|"Invalid request identity")?;
    let guard=lock(None)?;
    let name=&args[0];let native=dsh::exists(name);
    let record=if native {dsh::inspect(name,None)?} else {read(name)?};
    if request.expected_run_id.is_empty() || record["run_id"]!=request.expected_run_id
        || record["conversation_id"]!=request.expected_conversation_id
        || record["turn_started"].as_f64().unwrap_or(0.0)!=request.expected_turn_started {
        return Err("The session or turn changed. Refresh Activity before stopping it".into());
    }
    let path=absolute_root()?.join("interrupt_receipts").join(format!("{uuid}.json"));
    if path.exists() {return Err("This interrupt was already attempted. Check Terminal before trying again".into());}
    if native {
        if record["interrupt_supported"]!=true {return Err("This DeepSeek turn cannot be interrupted from Activity. Open the native UI".into());}
    } else {
        if !eligible(&record) {return Err("This turn is no longer working".into());}
        kimi_tui_choice::identity(&record)?;
    }
    recovery::cancel_for_input(name)?;
    atomic(&path,&json!({"status":"in_progress"}).to_string())?;
    let mut seen=None;
    if native { dsh::interrupt(&record)?; }
    else {
        let mut current=kimi_tui_choice::identity(&record)?;
        if !eligible(&current) || current["turn_started"]!=record["turn_started"] {return Err("The turn changed before interruption".into());}
        if current["agent"]=="claude" {seen=kimi_tui_choice::capture(&current).ok().map(|screen|interruptions(&screen));}
        compact_context::observe(&mut current,&json!({"hook_event_name":"Interrupt"}));
        write(&mut current)?;
        tmux(&["send-keys","-t",string(&current,"pane"),"Escape"].map(str::to_owned),true)
            .map_err(|e|format!("Interrupt delivery uncertain: {e}. Check Terminal before retrying"))?;
    }
    let mut receipt=json!({"request_id":request.request_id,"name":name,"run_id":request.expected_run_id,
        "conversation_id":request.expected_conversation_id,"status":"submitted"});
    atomic(&path,&receipt.to_string())?;
    if let Some(seen)=seen {
        // Hooks must not wait while Claude redraws.
        drop(guard);
        receipt["confirmed"]=json!(settle_claude(&record,seen));
    }
    println!("{receipt}");Ok(0)
}

// Claude prints this line when Escape stops a turn, and sends no hook.
const CLAUDE_INTERRUPTED:&str="Interrupted · What should Claude do instead?";
fn interruptions(screen:&str)->usize {screen.matches(CLAUDE_INTERRUPTED).count()}

/// Record the interruption once Claude shows one more interruption line than
/// before Escape, so the session leaves Working and the prompt can return.
fn settle_claude(original:&Value,seen:usize)->bool {
    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(3);
    while std::time::Instant::now()<deadline {
        std::thread::sleep(std::time::Duration::from_millis(60));
        if !kimi_tui_choice::capture(original).is_ok_and(|screen|interruptions(&screen)>seen) {continue;}
        let settled=(||->Result<bool>{
            let _guard=lock(None)?;
            let Some(mut record)=read_run(string(original,"name"),string(original,"run_id"))? else {return Ok(false)};
            if record["turn_started"]!=original["turn_started"] || !eligible(&record) {return Ok(false);}
            let event=json!({"hook_event_name":"Interrupt"});
            journal::update_activity(&mut record,&event);
            questions::observe(&mut record,&event);
            journal::log_event(&record,&event)?;
            write(&mut record)?;Ok(true)
        })();
        return settled.unwrap_or(false);
    }
    false
}
