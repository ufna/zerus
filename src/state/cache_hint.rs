//! Native cold-cache footer hints, read only. Never run /clear automatically.
use super::*;
pub(super) fn parse(agent: &str, screen: &str) -> Option<Value> {
    if agent!="claude" {return None;}
    let rows=screen.lines().rev().filter(|s|!s.trim().is_empty()).take(8).collect::<Vec<_>>();
    let cold=regex::Regex::new(r"^~?([0-9]+(?:\.[0-9]+)?)([kKmM]?) uncached\s*[·•/]\s*/?clear to start fresh$").unwrap();
    let savings=regex::Regex::new(r"^new task\? /clear to save ([0-9]+(?:\.[0-9]+)?)([kKmM]?) tokens$").unwrap();
    for row in rows {
        let row=row.trim();
        let (matched,status)=if let Some(c)=cold.captures(row) {(c,"cold")}
            else if let Some(c)=savings.captures(row) {(c,"saving_hint")} else {continue;};
        let n=matched[1].parse::<f64>().ok()?*match &matched[2] {"k"|"K"=>1000.,"m"|"M"=>1000000.,_=>1.};
        return Some(json!({"status":status,"tokens":n,"source":"native_footer"}));
    }
    None
}
pub(super) fn enrich(record: &Value, output: &mut Value, live: bool) {
    if live && output["pending_questions"].as_array().into_iter().flatten().any(|q|q["source"]=="kimi_cache_hint") {
        output["cache_hint"]=json!({"status":"cold","source":"native_chooser","tokens":output["session_usage"]["context"]["used"]});return;
    }
    if !live || record["agent"]!="claude" || !process_alive(record) {return;}
    if let Ok(screen)=kimi_tui_choice::capture(record) {
        if let Some(hint)=parse("claude",&screen) {output["cache_hint"]=hint;}
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn distinguishes_saving_suggestion_from_known_cold_cache() {
        assert_eq!(parse("claude","  ~756k uncached · /clear to start fresh").unwrap()["status"],"cold");
        assert_eq!(parse("claude"," new task? /clear to save 484.8k tokens").unwrap()["status"],"saving_hint");
        assert!(parse("codex","~756k uncached · /clear to start fresh").is_none());
        assert!(parse("claude","The user wrote: ~756k uncached · /clear to start fresh").is_none());
    }
}
