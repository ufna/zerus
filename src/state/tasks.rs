//! Bounded task snapshots from successful, identity-checked provider hooks.
//! These are reported plans, not inferred completion from an agent stopping.
use super::*;

const MAX_LISTS: usize = 65;
const MAX_TASKS: usize = 100;

fn tool(event: &Value) -> &str {
    string(event, "tool_name").rsplit('.').next().unwrap_or("")
}

fn supported(name: &str) -> bool {
    matches!(
        name,
        "TodoWrite"
            | "TodoList"
            | "SetTodoList"
            | "update_plan"
            | "TaskCreate"
            | "TaskUpdate"
            | "TaskList"
    )
}

fn owner(record: &Value, event: &Value) -> String {
    let id = journal::clipped(&event["agent_id"], 160);
    if id.is_empty() || (record["agent"] == "kimi" && id == "main") {
        "main".into()
    } else {
        format!("agent:{id}")
    }
}

fn state(value: &Value) -> &'static str {
    match value.as_str().unwrap_or("") {
        "pending" => "pending",
        "in_progress" => "in_progress",
        "completed" | "done" => "completed",
        _ => "unknown",
    }
}

fn item(value: &Value, index: usize) -> Option<Value> {
    let title = ["content", "title", "step", "subject"]
        .iter()
        .map(|key| journal::clipped(&value[key], 600))
        .find(|text| !text.trim().is_empty())?;
    let id = if let Some(id) = value["id"].as_u64() {
        id.to_string()
    } else {
        journal::clipped(&value["id"], 100)
    };
    Some(
        json!({"id":if id.is_empty() { (index + 1).to_string() } else { id },
        "title":title, "status":state(&value["status"]),
        "owner":journal::clipped(&value["owner"], 160)}),
    )
}

pub(super) fn observe(record: &mut Value, event: &Value) {
    let name = tool(event);
    if !supported(name) {
        return;
    }
    // Kimi's TaskList enumerates background processes, not the Claude plan.
    if record["agent"] != "claude" && matches!(name, "TaskCreate" | "TaskUpdate" | "TaskList") {
        return;
    }
    let kind = string(event, "hook_event_name");
    let agent_owner = owner(record, event);
    let call = ["tool_use_id", "tool_call_id"]
        .iter()
        .map(|key| string(event, key))
        .find(|id| !id.is_empty());
    let key = call.map(|id| format!("{agent_owner}:{}", journal::clipped(&json!(id), 160)));
    // Claude's Task tools operate on the session/team's shared list, whereas
    // TodoWrite and update_plan belong to the agent which reported them.
    let owner = if record["agent"] == "claude"
        && matches!(name, "TaskCreate" | "TaskUpdate" | "TaskList")
    {
        "main".to_owned()
    } else {
        agent_owner
    };
    // Some providers omit arguments in PostToolUse. Retain only bounded task
    // input, matched by exact call and agent; never apply an attempted update.
    if kind == "PreToolUse" {
        if let Some(key) = key {
            let input = &event["tool_input"];
            if input.is_object() && input.to_string().len() <= 128 * 1024 {
                if !record["pending_task_updates"].is_object() {
                    record["pending_task_updates"] = json!({});
                }
                let run_id = record["run_id"].clone();
                let pending = record["pending_task_updates"].as_object_mut().unwrap();
                if pending.len() < 64 {
                    pending.insert(key, json!({"tool":name,"input":input,"run_id":run_id}));
                }
            }
        }
        return;
    }
    if !matches!(kind, "PostToolUse" | "PostToolUseFailure") {
        return;
    }
    let pending = key.and_then(|key| record["pending_task_updates"].as_object_mut()?.remove(&key));
    let response = &event["tool_response"];
    if kind != "PostToolUse"
        || response["is_error"] == true
        || response["isError"] == true
        || response["success"] == false
        || event["is_error"] == true
    {
        return;
    }
    let input = if event["tool_input"].is_object() {
        event["tool_input"].clone()
    } else if let Some(pending) =
        pending.filter(|p| p["tool"] == name && p["run_id"] == record["run_id"])
    {
        pending["input"].clone()
    } else {
        Value::Null
    };
    let snapshot = match name {
        "TodoWrite" | "TodoList" | "SetTodoList" => input["todos"].as_array(),
        "update_plan" => input["plan"].as_array(),
        "TaskList" => response["tasks"].as_array(),
        _ => None,
    };
    let mut lists = record["task_lists"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    if !lists.contains_key(&owner) && lists.len() >= MAX_LISTS {
        return;
    }
    let previous = lists.get(&owner).cloned().unwrap_or(Value::Null);
    let (items, truncated) = if let Some(snapshot) = snapshot {
        // Invalid input is not evidence that a previous plan was cleared.
        let items: Vec<_> = snapshot
            .iter()
            .take(MAX_TASKS)
            .enumerate()
            .filter_map(|(i, v)| item(v, i))
            .collect();
        if !snapshot.is_empty() && items.is_empty() {
            return;
        }
        (items, snapshot.len() > MAX_TASKS)
    } else if matches!(name, "TaskCreate" | "TaskUpdate") {
        let same_family = matches!(
            string(&previous, "source"),
            "TaskCreate" | "TaskUpdate" | "TaskList"
        );
        let mut items = if same_family {
            previous["items"].as_array().cloned().unwrap_or_default()
        } else {
            vec![]
        };
        let mut truncated = same_family && previous["truncated"] == true;
        if name == "TaskCreate" {
            // Creation needs the provider-assigned ID; never invent one which
            // could collide with subsequent TaskUpdate calls.
            let task = if response["task"].is_object() {
                &response["task"]
            } else {
                response
            };
            if !task["id"].as_str().is_some_and(|id| {
                !id.trim().is_empty() && id.len() <= 100 && !id.chars().any(char::is_control)
            }) && task["id"].as_u64().is_none()
            {
                return;
            }
            let mut task = task.clone();
            if task["subject"].is_null() {
                task["subject"] = input["subject"].clone();
            }
            if task["status"].is_null() {
                task["status"] = json!("pending");
            }
            let Some(task) = item(&task, 0) else {
                return;
            };
            items.retain(|existing| existing["id"] != task["id"]);
            if items.len() >= MAX_TASKS {
                truncated = true;
            } else {
                items.push(task);
            }
        } else {
            if !same_family {
                return;
            }
            let id = input["taskId"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| input["taskId"].as_u64().map(|id| id.to_string()))
                .unwrap_or_default();
            if id.is_empty() {
                return;
            }
            if input["status"] == "deleted" {
                items.retain(|task| task["id"] != id);
            } else {
                let Some(task) = items.iter_mut().find(|task| task["id"] == id) else {
                    return;
                };
                if input["status"].is_string() {
                    task["status"] = json!(state(&input["status"]));
                }
                if input["subject"].is_string() {
                    task["title"] = json!(journal::clipped(&input["subject"], 600));
                }
                if input["owner"].is_string() {
                    task["owner"] = json!(journal::clipped(&input["owner"], 160));
                }
            }
        }
        (items, truncated)
    } else {
        return;
    };
    lists.insert(
        owner,
        json!({"source":name,"items":items,"truncated":truncated,
        "updated":now(),"run_id":record["run_id"]}),
    );
    record["task_lists"] = json!(lists);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kimi_current_todos_are_not_overwritten_by_background_task_list() {
        let mut r = json!({"agent":"kimi","run_id":"run-1"});
        observe(
            &mut r,
            &event(
                "PostToolUse",
                "SetTodoList",
                json!({"todos":[{"title":"Verify restore","status":"done"}]}),
            ),
        );
        assert_eq!(r["task_lists"]["main"]["items"][0]["status"], "completed");
        let mut background = event("PostToolUse", "TaskList", json!({}));
        background["tool_response"] =
            json!({"tasks":[{"id":"bg-1","title":"Build","status":"running"}]});
        observe(&mut r, &background);
        assert_eq!(
            r["task_lists"]["main"]["items"][0]["title"],
            "Verify restore"
        );
        observe(&mut r, &event("PostToolUse", "SetTodoList", json!({})));
        assert_eq!(
            r["task_lists"]["main"]["items"].as_array().unwrap().len(),
            1
        );
        observe(
            &mut r,
            &event("PostToolUse", "SetTodoList", json!({"todos":[]})),
        );
        assert!(r["task_lists"]["main"]["items"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    fn event(kind: &str, name: &str, input: Value) -> Value {
        json!({"hook_event_name":kind,"tool_name":name,"tool_input":input,"tool_use_id":"call-1"})
    }
    #[test]
    fn successful_snapshots_only_and_exact_agent_pairing() {
        let mut r = json!({"agent":"codex","run_id":"run-1"});
        let pre = event(
            "PreToolUse",
            "functions.update_plan",
            json!({"plan":[{"step":"Review", "status":"in_progress"}]}),
        );
        observe(&mut r, &pre);
        assert!(r["task_lists"].is_null());
        let mut post = event("PostToolUse", "functions.update_plan", Value::Null);
        post["agent_id"] = json!("child");
        observe(&mut r, &post);
        assert!(r["task_lists"].is_null());
        post["agent_id"] = Value::Null;
        observe(&mut r, &post);
        assert_eq!(r["task_lists"]["main"]["items"][0]["title"], "Review");
        observe(
            &mut r,
            &event("PostToolUseFailure", "update_plan", json!({"plan":[]})),
        );
        assert_eq!(
            r["task_lists"]["main"]["items"].as_array().unwrap().len(),
            1
        );
        let mut failed = event("PostToolUse", "update_plan", json!({"plan":[]}));
        failed["tool_response"] = json!({"isError":true});
        observe(&mut r, &failed);
        assert_eq!(
            r["task_lists"]["main"]["items"].as_array().unwrap().len(),
            1
        );
        observe(
            &mut r,
            &event("PostToolUse", "update_plan", json!({"plan":[]})),
        );
        assert!(r["task_lists"]["main"]["items"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn kimi_and_child_lists_stay_separate_and_bounded() {
        let mut r = json!({"agent":"kimi","run_id":"run-1"});
        let mut e = event(
            "PostToolUse",
            "TodoList",
            json!({"todos":[{"title":"Deploy","status":"done"}]}),
        );
        e["agent_id"] = json!("main");
        observe(&mut r, &e);
        e["agent_id"] = json!("reviewer");
        e["tool_input"]["todos"][0]["status"] = json!("pending");
        observe(&mut r, &e);
        assert_eq!(r["task_lists"]["main"]["items"][0]["status"], "completed");
        assert_eq!(
            r["task_lists"]["agent:reviewer"]["items"][0]["status"],
            "pending"
        );
        e["tool_input"]["todos"] = json!((0..150)
            .map(|_| json!({"content":"a".repeat(900),"status":"future"}))
            .collect::<Vec<_>>());
        observe(&mut r, &e);
        let list = &r["task_lists"]["agent:reviewer"];
        assert_eq!(list["items"].as_array().unwrap().len(), MAX_TASKS);
        assert_eq!(list["truncated"], true);
        assert_eq!(list["items"][0]["status"], "unknown");
        assert!(string(&list["items"][0], "title").chars().count() <= 601);
    }
    #[test]
    fn claude_task_lifecycle_uses_real_id() {
        let mut r = json!({"agent":"claude","run_id":"run-1"});
        let mut create = event(
            "PostToolUse",
            "TaskCreate",
            json!({"subject":"Inspect logs"}),
        );
        observe(&mut r, &create);
        assert!(r["task_lists"].is_null());
        create["tool_response"] = json!({"task":{"id":"7","subject":"Inspect logs"}});
        observe(&mut r, &create);
        observe(
            &mut r,
            &event(
                "PostToolUse",
                "TaskUpdate",
                json!({"taskId":"7","status":"in_progress","owner":"reviewer"}),
            ),
        );
        assert_eq!(r["task_lists"]["main"]["items"][0]["status"], "in_progress");
        assert_eq!(r["task_lists"]["main"]["items"][0]["owner"], "reviewer");
        let mut child_update = event(
            "PostToolUse",
            "TaskUpdate",
            json!({"taskId":"7","status":"completed"}),
        );
        child_update["agent_id"] = json!("reviewer");
        observe(&mut r, &child_update);
        assert_eq!(r["task_lists"]["main"]["items"][0]["status"], "completed");
        assert!(r["task_lists"]["agent:reviewer"].is_null());
        observe(
            &mut r,
            &event(
                "PostToolUse",
                "TaskUpdate",
                json!({"taskId":"7","status":"deleted"}),
            ),
        );
        assert!(r["task_lists"]["main"]["items"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn incremental_task_lists_do_not_confuse_todo_ids_and_mark_truncation() {
        let mut r = json!({"agent":"claude","run_id":"run-1"});
        observe(
            &mut r,
            &event(
                "PostToolUse",
                "TodoWrite",
                json!({"todos":[{"content":"Old plan","status":"pending"}]}),
            ),
        );
        observe(
            &mut r,
            &event(
                "PostToolUse",
                "TaskUpdate",
                json!({"taskId":"1","status":"completed"}),
            ),
        );
        assert_eq!(r["task_lists"]["main"]["source"], "TodoWrite");
        assert_eq!(r["task_lists"]["main"]["items"][0]["status"], "pending");
        for i in 1..=101 {
            let mut create = event(
                "PostToolUse",
                "TaskCreate",
                json!({"subject":format!("Task {i}")}),
            );
            create["tool_response"] = json!({"task":{"id":i}});
            observe(&mut r, &create);
        }
        let list = &r["task_lists"]["main"];
        assert_eq!(list["items"][0]["title"], "Task 1");
        assert_eq!(list["items"].as_array().unwrap().len(), MAX_TASKS);
        assert_eq!(list["truncated"], true);
        let mut empty_id = event("PostToolUse", "TaskCreate", json!({"subject":"Missing ID"}));
        empty_id["tool_response"] = json!({"task":{"id":""}});
        let before = r.clone();
        observe(&mut r, &empty_id);
        assert_eq!(r, before);
    }
}
