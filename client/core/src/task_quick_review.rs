//! Report-bound ordinary task review commands and bounded renderer data.
use crate::{creation::canonical_uuid, tasks::valid_project, tasks_write::TaskCall};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

const REPORT_BYTES: usize = 512;
const DETAIL_BYTES: usize = 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Assignee {
    pub machine: String,
    pub session_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Disposition {
    Ready {
        evidence: String,
    },
    Check {
        criterion: String,
        assignee: Assignee,
    },
    Rework {
        defect: String,
    },
    Parked {
        reopen_when: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReportGuard {
    pub history_revision: String,
    pub report_id: String,
}

impl ReportGuard {
    pub fn valid(&self) -> bool {
        crate::tasks_write::valid_revision(&self.history_revision) && valid_report(&self.report_id)
    }

    pub fn matches(&self, task: &Value) -> bool {
        self.valid()
            && task["stage"] == "review"
            && task["review_report_id"] == self.report_id
            && task["history_revision"] == self.history_revision
            && !team_routed(task)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    Set {
        history_revision: String,
        report_id: String,
        disposition: Disposition,
    },
    Accept {
        history_revision: String,
        report_id: String,
    },
}

fn visible(text: &str) -> bool {
    !text.chars().any(|ch| {
        ch.is_control() || matches!(ch, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    })
}
fn detail(text: &str) -> bool {
    !text.trim().is_empty()
        && text.len() <= DETAIL_BYTES
        && !text.chars().any(|ch| {
            (ch.is_control() && !matches!(ch, '\n' | '\t'))
                || matches!(ch, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
}
pub(crate) fn valid_report(report: &str) -> bool {
    !report.is_empty()
        && report.len() <= REPORT_BYTES
        && visible(report)
        && !report.chars().any(char::is_whitespace)
}

impl Disposition {
    pub fn valid(&self) -> bool {
        match self {
            Self::Ready { evidence } => detail(evidence),
            Self::Check {
                criterion,
                assignee,
            } => {
                detail(criterion)
                    && canonical_uuid(&assignee.machine)
                    && assignee.session_id.len() <= 200
                    && assignee
                        .session_id
                        .split_once(':')
                        .is_some_and(|(kind, id)| {
                            valid_report(id)
                                && (kind == "run_id" || kind == "launch_id" && canonical_uuid(id))
                        })
            }
            Self::Rework { defect } => detail(defect),
            Self::Parked { reopen_when } => detail(reopen_when),
        }
    }
}

impl Change {
    fn report(&self) -> (&str, &str) {
        match self {
            Self::Set {
                history_revision,
                report_id,
                ..
            }
            | Self::Accept {
                history_revision,
                report_id,
            } => (history_revision, report_id),
        }
    }

    pub fn call(&self, project: &str, task: &str) -> Result<TaskCall, String> {
        let (revision, report) = self.report();
        if !valid_project(project)
            || !canonical_uuid(task)
            || !crate::tasks_write::valid_revision(revision)
            || !valid_report(report)
            || matches!(self, Self::Set { disposition, .. } if !disposition.valid())
        {
            return Err("Invalid quick review request.".into());
        }
        Ok(TaskCall {
            args: ["quick-review", task, "--project", project, "--json"]
                .map(str::to_owned)
                .into(),
            input: serde_json::to_string(self).map_err(|err| err.to_string())?,
        })
    }

    pub fn check_reply(
        &self,
        reply: &Map<String, Value>,
        project: &str,
        task: &str,
    ) -> Result<(), String> {
        let saved = reply.get("task").unwrap_or(&Value::Null);
        let matches = match self {
            Self::Set {
                disposition,
                report_id,
                ..
            } => {
                reply.get("op") == Some(&json!("set"))
                    && saved["stage"] == "review"
                    && saved["review_report_id"] == report_id.as_str()
                    && read_disposition(&saved["quick_review"]).as_ref() == Some(disposition)
            }
            Self::Accept { .. } => {
                reply.get("op") == Some(&json!("accept")) && saved["stage"] == "done"
            }
        };
        if !matches
            || reply.get("report_id").and_then(Value::as_str) != Some(self.report().1)
            || reply.get("task_uuid").and_then(Value::as_str) != Some(task)
            || reply.get("project_uuid").and_then(Value::as_str) != Some(project)
            || saved["uuid"] != task
            || saved["project"] != project
        {
            return Err("The review outcome could not be verified. Refresh the task.".into());
        }
        Ok(())
    }
}

pub(crate) fn team_routed(task: &Value) -> bool {
    !task["team"].is_null()
        || task["team_invalid"] == true
        || task["review_lane"] == "team"
        || task["quick_review_status"] == "team"
}

pub(crate) fn uses_report_review(task: &Value) -> bool {
    task["stage"] == "review"
        && (team_routed(task)
            || task["quick_review_status"]
                .as_str()
                .is_some_and(ordinary_status))
}

pub(crate) fn ordinary_status(status: &str) -> bool {
    matches!(
        status,
        "unresolved" | "unclassified" | "ready" | "check" | "rework" | "parked"
    )
}

/// Native reads may resolve the assignee's display name and host. Keep these
/// hints out of the typed write request and compare only the immutable key.
fn read_disposition(value: &Value) -> Option<Disposition> {
    let mut raw = value.clone();
    if let Some(assignee) = raw.get_mut("assignee").and_then(Value::as_object_mut) {
        assignee.remove("session");
        assignee.remove("host");
        assignee.remove("resolved");
    }
    serde_json::from_value::<Disposition>(raw)
        .ok()
        .filter(Disposition::valid)
}

fn present_disposition(raw: &Value) -> Option<Value> {
    let mut shown = serde_json::to_value(read_disposition(raw)?).ok()?;
    if raw["kind"] == "check" {
        let good_hint = |key| {
            raw["assignee"][key]
                .as_str()
                .filter(|hint| !hint.is_empty() && hint.len() <= 200 && visible(hint))
        };
        if let Some(session) = good_hint("session") {
            shown["assignee"]["session"] = json!(session);
            if let Some(host) = good_hint("host") {
                shown["assignee"]["host"] = json!(host);
            }
        }
    }
    Some(shown)
}

pub(crate) fn present_history(entry: &mut Map<String, Value>) {
    let Some(raw) = entry.remove("quick_review") else {
        return;
    };
    if entry.get("kind").and_then(Value::as_str) != Some("quick_review") {
        return;
    }
    let Some(report) = raw["report_id"].as_str().filter(|id| valid_report(id)) else {
        return;
    };
    if let Some(disposition) = present_disposition(&raw["disposition"]) {
        entry.insert(
            "quick_review".into(),
            json!({ "report_id": report, "disposition": disposition }),
        );
    }
}

pub(crate) fn present(task: &mut Map<String, Value>, eligible: bool, team: bool) {
    let status = task.remove("quick_review_status");
    let report = task.remove("review_report_id");
    let raw_disposition = task.remove("quick_review").filter(|v| !v.is_null());
    let Some(status) = status.as_ref().and_then(Value::as_str) else {
        return;
    };
    if team {
        if status == "team" {
            task.insert("quick_review_status".into(), json!(status));
        }
        return;
    }
    if !ordinary_status(status) {
        return;
    }
    task.insert("quick_review_status".into(), json!(status));
    if !eligible || status == "unresolved" {
        return;
    }
    let Some(report) = report.filter(|id| id.as_str().is_some_and(valid_report)) else {
        return;
    };
    let disposition = raw_disposition.as_ref().and_then(present_disposition);
    if status == "unclassified" {
        if raw_disposition.is_some() {
            return;
        }
    } else if disposition
        .as_ref()
        .is_none_or(|value| value["kind"] != status)
    {
        return;
    }
    task.insert("review_report_id".into(), report);
    if let Some(disposition) = disposition {
        task.insert("quick_review".into(), disposition);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = "pilot";
    const TASK: &str = "11111111-1111-4111-8111-111111111111";
    const REPORT: &str = "pilot:report@2";

    #[test]
    fn a_named_assignee_is_presented_but_never_becomes_write_authority() {
        let disposition = Disposition::Check {
            criterion: "Verify this response\nagainst the logs.".into(),
            assignee: Assignee {
                machine: TASK.into(),
                session_id: "run_id:candidate".into(),
            },
        };
        let change = Change::Set {
            history_revision: "a".repeat(64),
            report_id: REPORT.into(),
            disposition: disposition.clone(),
        };
        let call = change.call(PROJECT, TASK).unwrap();
        assert_eq!(
            call.args,
            ["quick-review", TASK, "--project", PROJECT, "--json"]
        );
        let input: Value = serde_json::from_str(&call.input).unwrap();
        assert_eq!(
            input["disposition"]["assignee"],
            json!({
                "machine": TASK, "session_id": "run_id:candidate"
            })
        );
        let mut result = json!({
            "task_uuid": TASK, "project_uuid": PROJECT, "report_id": REPORT, "op": "set",
            "task": { "uuid": TASK, "project": PROJECT, "stage": "review",
                "review_report_id": REPORT, "quick_review_status": "check", "quick_review": disposition }
        });
        result["task"]["quick_review"]["assignee"]["session"] = json!("codex/reviewer");
        result["task"]["quick_review"]["assignee"]["host"] = json!("arch");
        assert!(
            change
                .check_reply(result.as_object().unwrap(), PROJECT, TASK)
                .is_ok()
        );
        assert!(
            serde_json::from_value::<Change>(json!({
                "op": "set", "history_revision": "a".repeat(64), "report_id": REPORT,
                "disposition": result["task"]["quick_review"]
            }))
            .is_err()
        );
        present(result["task"].as_object_mut().unwrap(), true, false);
        assert_eq!(
            result["task"]["quick_review"]["assignee"]["session"],
            "codex/reviewer"
        );
        result["task"]["quick_review"]["assignee"]["session_id"] = json!("run_id:different");
        assert!(
            change
                .check_reply(result.as_object().unwrap(), PROJECT, TASK)
                .is_err()
        );
    }

    #[test]
    fn exact_reports_are_only_actionable_with_matching_native_status() {
        for (status, disposition, team, actionable) in [
            (
                "ready",
                json!({"kind":"ready","evidence":"Existing proof"}),
                false,
                true,
            ),
            ("unclassified", Value::Null, false, true),
            (
                "unclassified",
                json!({"kind":"ready","evidence":"Prior report"}),
                false,
                false,
            ),
            ("unresolved", Value::Null, false, false),
            (
                "ready",
                json!({"kind":"parked","reopen_when":"Fresh repro"}),
                false,
                false,
            ),
            (
                "team",
                json!({"kind":"ready","evidence":"Team proof"}),
                true,
                false,
            ),
        ] {
            let mut task = json!({"quick_review_status":status,"quick_review":disposition,"review_report_id":REPORT});
            present(task.as_object_mut().unwrap(), !team, team);
            assert_eq!(task["review_report_id"] == REPORT, actionable);
        }
        let accept = Change::Accept {
            history_revision: "a".repeat(64),
            report_id: REPORT.into(),
        };
        let mut reply = json!({
            "task_uuid":TASK, "project_uuid":PROJECT, "report_id":REPORT, "op":"accept",
            "task":{"uuid":TASK,"project":PROJECT,"stage":"done"}
        });
        assert!(
            accept
                .check_reply(reply.as_object().unwrap(), PROJECT, TASK)
                .is_ok()
        );
        reply["report_id"] = json!("different-report");
        assert!(
            accept
                .check_reply(reply.as_object().unwrap(), PROJECT, TASK)
                .is_err()
        );
    }

    #[test]
    fn history_outcomes_keep_their_exact_report_and_only_bounded_public_fields() {
        let mut entry = json!({"kind":"quick_review","quick_review":{
            "report_id":REPORT,"supersedes":["old"],"detail":"unprojected",
            "disposition":{"kind":"parked","reopen_when":"Review the next report."}
        }});
        present_history(entry.as_object_mut().unwrap());
        assert_eq!(
            entry["quick_review"],
            json!({
                "report_id":REPORT,"disposition":{"kind":"parked","reopen_when":"Review the next report."}
            })
        );
        entry["kind"] = json!("report");
        present_history(entry.as_object_mut().unwrap());
        assert!(entry.get("quick_review").is_none());
    }
}
