//! Review labels use bounded native evidence from the same projection on all
//! desktop task reads. These fields do not authorize any mutation.
use super::execution::Execution;
use serde_json::{json, Map, Value};

/// Take the raw replica fields out before publishing to the renderer. A
/// present but unreadable execution is not evidence of a clean review.
pub(crate) fn present_review(task: &mut Map<String, Value>) {
    task.remove("execution_unavailable");
    if let Some(raw) = task.remove("execution").filter(|value| !value.is_null()) {
        if let Some(execution) = Execution::from_value(&raw) {
            task.insert("execution".into(), json!(execution));
        } else {
            task.insert("execution_unavailable".into(), json!(true));
        }
    }

    let invalid = task.remove("team_invalid") == Some(json!(true));
    let team = task.remove("team").filter(|value| !value.is_null());
    if let Some(team) = &team {
        if let (Some(id), Some(home)) = (team["team"].as_str(), team["home"].as_str()) {
            if crate::creation::canonical_uuid(id) && crate::creation::canonical_uuid(home) {
                task.insert("team".into(), json!({"team": id, "home": home}));
            }
        }
    }
    if invalid || (team.is_some() && !task.contains_key("team")) {
        task.insert("team_invalid".into(), json!(true));
    }

    if task.get("review_editable") != Some(&Value::Bool(true))
        || task.get("stage").and_then(Value::as_str) != Some("review")
        || task.get("writable") == Some(&Value::Bool(false))
        || team.is_some()
        || invalid
        || task.contains_key("execution_unavailable")
        || !task
            .get("history_revision")
            .and_then(Value::as_str)
            .is_some_and(crate::tasks_write::valid_revision)
    {
        task.remove("review_editable");
    }

    let team_routed = team.is_some()
        || invalid
        || task.get("review_lane").and_then(Value::as_str) == Some("team")
        || task.get("quick_review_status").and_then(Value::as_str) == Some("team");
    let ordinary_report = task.get("stage").and_then(Value::as_str) == Some("review")
        && !team_routed
        && !task.contains_key("execution_unavailable")
        && task
            .get("history_revision")
            .and_then(Value::as_str)
            .is_some_and(crate::tasks_write::valid_revision);
    crate::task_quick_review::present(task, ordinary_report, team_routed);

    let assignment = task.remove("review_assignment");
    if team.is_some() || invalid {
        return;
    }
    if let Some(assignment) = assignment {
        if let (Some(owner @ ("person" | "technical")), Some(criterion)) = (
            assignment["owner"].as_str(),
            assignment["criterion"].as_str(),
        ) {
            if !criterion.trim().is_empty()
                && criterion.len() <= 512
                && !criterion.chars().any(|ch| {
                    ch.is_control()
                        || matches!(ch, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
                })
            {
                task.insert(
                    "review_assignment".into(),
                    json!({"owner": owner, "criterion": criterion}),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_execution_stays_unavailable_and_bad_team_routing_hides_ordinary_assignment() {
        let mut task = json!({
            "execution": {"current": {"native": "unprojectable"}},
            "team": {"team": "malformed", "home": "wrong"},
            "review_assignment": {"owner": "person", "criterion": "Inspect output"}
        });
        present_review(task.as_object_mut().unwrap());
        assert_eq!(
            task,
            json!({"execution_unavailable": true, "team_invalid": true})
        );

        let mut legacy = json!({"execution": null});
        present_review(legacy.as_object_mut().unwrap());
        assert_eq!(legacy, json!({}));
    }

    #[test]
    fn review_assignment_retains_only_a_known_owner_and_bounded_criterion() {
        let mut task = json!({"review_assignment": {
            "owner": "technical", "criterion": "Check the result", "private": "ignored"
        }});
        present_review(task.as_object_mut().unwrap());
        assert_eq!(
            task["review_assignment"],
            json!({
                "owner": "technical", "criterion": "Check the result"
            })
        );
        for criterion in ["é".repeat(257), "bad\nline".into(), "x\u{2066}y".into()] {
            task["review_assignment"] = json!({"owner": "person", "criterion": criterion});
            present_review(task.as_object_mut().unwrap());
            assert!(task.get("review_assignment").is_none());
        }
    }
}
