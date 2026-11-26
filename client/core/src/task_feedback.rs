//! The person's explicit discussion mode, bound to the clicked task epoch and
//! pinned dispatch. Recovery always uses the existing outbox's original mode.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Delivery {
    pub mode: String,
    pub stage_at: i64,
    pub dispatch_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<crate::task_quick_review::ReportGuard>,
}

pub fn checked<'a>(detail: &Value, delivery: &'a Delivery) -> Result<&'a str, String> {
    let stage = detail["stage"].as_str().unwrap_or("");
    let valid = match delivery.mode.as_str() {
        "note" => matches!(stage, "todo" | "backlog" | "active" | "review" | "done"),
        "queue" => matches!(stage, "todo" | "backlog"),
        "send" => stage == "active",
        "reopen" => matches!(stage, "review" | "done")
            && !crate::task_quick_review::team_routed(detail)
            && (delivery.review.is_some() || !crate::task_quick_review::uses_report_review(detail)),
        _ => false,
    };
    if !valid
        || detail["writable"] != true
        || delivery.stage_at <= 0
        || detail["stage_at"].as_i64() != Some(delivery.stage_at)
        || detail["dispatch_id"].as_str().unwrap_or("") != delivery.dispatch_id
        || delivery.dispatch_id.len() > 512
        || delivery.dispatch_id.chars().any(char::is_control)
        || delivery.review.as_ref().is_some_and(|review| {
            delivery.mode != "reopen"
                || !review.matches(detail)
                || detail["quick_review_status"] != "rework"
                || recorded_defect(detail).is_none()
        })
    {
        return Err("The task changed. Your comment draft was kept. Refresh the task.".into());
    }
    Ok(&delivery.mode)
}

/// The exact stored rework text, after the caller has checked its report guard.
pub fn recorded_defect(detail: &Value) -> Option<String> {
    use crate::task_quick_review::Disposition;
    let value = serde_json::from_value::<Disposition>(detail["quick_review"].clone()).ok()?;
    if !value.valid() {
        return None;
    }
    match value {
        Disposition::Rework { defect } => Some(defect),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn each_stage_has_only_its_explicit_delivery_mode_and_note() {
        for (stage, mode) in [
            ("todo", "queue"),
            ("backlog", "queue"),
            ("active", "send"),
            ("review", "reopen"),
            ("done", "reopen"),
        ] {
            let detail =
                json!({"stage":stage,"stage_at":123,"dispatch_id":"original","writable":true});
            for choice in ["queue", "send", "reopen", "note"] {
                let delivery = Delivery {
                    mode: choice.into(),
                    stage_at: 123,
                    dispatch_id: "original".into(),
                    review: None,
                };
                assert_eq!(
                    checked(&detail, &delivery).is_ok(),
                    choice == mode || choice == "note"
                );
            }
        }
    }
    #[test]
    fn a_changed_epoch_dispatch_or_future_document_cannot_deliver() {
        let delivery = Delivery {
            mode: "send".into(),
            stage_at: 123,
            dispatch_id: "original".into(),
            review: None,
        };
        let detail =
            json!({"stage":"active","stage_at":123,"dispatch_id":"original","writable":true});
        for (key, value) in [
            ("stage_at", json!(124)),
            ("dispatch_id", json!("other")),
            ("writable", json!(false)),
            ("stage", json!("review")),
        ] {
            let mut changed = detail.clone();
            changed[key] = value;
            assert!(checked(&changed, &delivery).is_err());
        }
    }
}
