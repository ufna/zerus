//! Approve exactly one visible Claude Bash request. Persistent grants and mode
//! changes remain in Terminal; this adapter never chooses them implicitly.
use super::*;
use sha2::{Digest,Sha256};
struct Panel { body:String, labels:Vec<String>, selected:usize }
fn parse(screen:&str)->Option<Panel> {
    let rows=screen.trim_end().lines().map(str::trim).collect::<Vec<_>>();
    let prompt=rows.iter().rposition(|r|*r=="Do you want to proceed?")?;
    let start=rows[..prompt].iter().rposition(|r|r.len()>20&&r.chars().all(|c|c=='─'))?;
    if rows.get(start+1)!=Some(&"Bash command") {return None;}
    if !rows.last()?.starts_with("Esc to cancel") {return None;}
    let pattern=regex::Regex::new(r"^(❯\s*)?([1-9])\. (.+)$").ok()?;
    let mut labels:Vec<String>=Vec::new();let mut selected=None;
    for row in &rows[prompt+1..rows.len()-1] {
        if row.is_empty(){continue;}
        let Some(caps)=pattern.captures(row) else {
            // Older Claude wraps the persistent grant's folder onto another
            // line. Only intermediate options may continue; Yes and No remain
            // exact single-line choices and are the only actions we expose.
            if labels.len()<2 || labels.last()?.as_str()=="No" || row.contains('❯') {return None;}
            labels.last_mut()?.push_str(&format!("\n{row}"));continue;
        };
        if caps[2].parse::<usize>().ok()?!=labels.len()+1{return None;}
        if caps.get(1).is_some()&&selected.replace(labels.len()).is_some(){return None;}
        labels.push(caps[3].to_owned());
    }
    if labels.len()<2||labels.len()>8||labels.first()?.as_str()!="Yes"||labels.last()?.as_str()!="No" {return None;}
    Some(Panel{body:rows[start+1..prompt].join("\n"),labels,selected:selected?})
}
fn eligible(r:&Value)->bool {
    r["agent"]=="claude"&&r["phase"]=="approval"&&!pause_active(r)
}
fn pending(record:&Value)->Option<Value> {
    if record["pending_approval"].is_object(){return Some(record["pending_approval"].clone());}
    // Existing sessions may be waiting on a hook written by an older HGS.
    // Recover only one exact, untruncated command and still verify the whole
    // native panel before every navigation key.
    let tools=record["active_tools"].as_object()?.iter().filter(|(_,v)|v["name"]=="Bash").collect::<Vec<_>>();
    if tools.len()!=1{return None;}
    Some(json!({"tool_name":"Bash","tool_use_id":tools[0].0,"tool_input":{"command":tools[0].1["detail"]}}))
}
fn normalized(text:&str)->String {text.split_whitespace().collect::<Vec<_>>().join(" ")}
fn card(record:&Value,panel:&Panel)->Option<Value> {
    if !eligible(record){return None;}
    let request=pending(record)?;
    if request["tool_name"]!="Bash"||string(&request,"tool_use_id").is_empty(){return None;}
    let command=string(&request["tool_input"],"command");
    if command.trim().is_empty()||command.len()>64000{return None;}
    let dividers=panel.body.lines().enumerate().filter(|(_,r)|r.chars().count()>20&&r.chars().all(|c|c=='╌')).map(|(i,_)|i).collect::<Vec<_>>();
    let rows=panel.body.lines().collect::<Vec<_>>();
    if dividers.len()==2 {
        if normalized(&rows[dividers[0]+1..dividers[1]].join("\n"))!=normalized(command){return None;}
    } else if dividers.is_empty() {
        // Claude 2.1.284 renders the command and its description without code
        // fences. Match the entire body, including the optional native tip.
        let mut content=rows[1..].join("\n");content=content.trim().to_owned();
        const TIP:&str="Tip: auto mode handles these prompts for you — choose \"switch to auto mode\" below";
        if content.starts_with(TIP){content=content[TIP.len()..].trim().to_owned();}
        let expected=format!("{}\n{}",command,string(&request["tool_input"],"description"));
        if normalized(&content)!=normalized(&expected){return None;}
    } else {return None;}
    let hash=format!("{:x}",Sha256::digest(json!([record["run_id"],record["conversation_id"],record["pane"],record["pid"],record["process_start"],request,panel.body,panel.labels]).to_string()));
    Some(json!({"question_id":format!("claude-approval:{hash}"),"question_hash":hash,"tool_call_id":request["tool_use_id"],
        "run_id":record["run_id"],"conversation_id":record["conversation_id"],"source":"claude_tool_approval",
        "can_answer":true,"approval":true,"answer_transport":"claude_tui","created_at":record["last_event_at"],
        "questions":[{"id":"approval","question":"Approve this Bash command?","body":command,"allow_other":false,
            "options":[{"id":"allow","label":"Approve once"},{"id":"deny","label":"Deny"}]}]}))
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn older_claude_body_and_wrapped_grant_remain_exact() {
        let screen="────────────────────────────────────────\n Bash command\n Tip: auto mode handles these prompts for you — choose \"switch to auto mode\" below\n\n   printf approved > approved.txt\n   Create a fixture file after approval\n\n Do you want to proceed?\n ❯ 1. Yes\n   2. Yes, and always allow access to\n      /a/long/project/folder from this project\n   3. Yes, and switch to auto mode\n   4. No\n\n Esc to cancel · Tab to amend\n";
        let mut record=json!({"agent":"claude","phase":"approval","pending_approval":{"tool_name":"Bash","tool_use_id":"one","tool_input":{"command":"printf approved > approved.txt","description":"Create a fixture file after approval"}}});
        let panel=parse(screen).unwrap();assert_eq!(panel.labels.len(),4);assert_eq!(panel.selected,0);
        assert!(card(&record,&panel).is_some());
        record["pending_approval"]["tool_input"]["command"]=json!("printf other > approved.txt");assert!(card(&record,&panel).is_none());
        assert!(parse(&screen.replace("   4. No","   4. No\n      hidden extra choice")).is_none());
    }
}
fn snapshot(record:&Value,screen:&str)->Option<(Value,usize)> {let panel=parse(screen)?;Some((card(record,&panel)?["question_hash"].clone(),panel.selected))}
pub(super) fn current(record:&Value)->Option<Value> {
    if !eligible(record){return None;}
    kimi_tui_choice::identity(record).ok()?;
    card(record,&parse(&kimi_tui_choice::capture(record).ok()?)?)
}
pub(super) fn available(record:&Value,question:&Value)->Result<()> {
    let current=current(record).ok_or("The native approval is no longer visible. Open Terminal")?;
    if current["question_hash"]!=question["question_hash"]{return Err("The command changed. Review its new approval request".into());}Ok(())
}
pub(super) fn answer(record:&Value,question:&Value,answers:&Value)->Result<()> {
    available(record,question)?;
    let panel=parse(&kimi_tui_choice::capture(record)?).ok_or("Approval disappeared")?;
    let chosen=match answers[0]["selected_option_ids"][0].as_str(){Some("allow")=>0,Some("deny")=>panel.labels.len()-1,_=>return Err("Choose Approve once or Deny".into())};
    kimi_tui_choice::answer(record,question,chosen,panel.labels.len(),snapshot,
        |screen|!screen.trim().is_empty()&&!screen.contains("Do you want to proceed?"),false)
}
