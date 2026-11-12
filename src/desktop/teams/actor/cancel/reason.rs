//! Preserve the command's answer separately from the store or retry cause.
use super::*;
pub(super) fn refused(row: &Entry, why: &str) -> String {
    format!(
        "{} The team store refused this outcome: {}.",
        answered(row),
        sentence(why, 1300)
    )
}
pub(super) fn stalled(row: &Entry) -> String {
    format!("{} The service stopped retrying after three stale answers whose observations made no progress.",
        answered(row))
}
fn answered(row: &Entry) -> String {
    if row.pending.task.is_none() {
        return "No task command was needed.".into();
    }
    let (state, fallback) = match &row.reply {
        Some(TaskReply::Cancelled) => ("cancelled", ""),
        Some(TaskReply::Released) => ("released", ""),
        Some(TaskReply::Stale) => ("stale", ""),
        Some(TaskReply::Refused { reason }) => ("refused", reason.as_str()),
        Some(TaskReply::CuttingOver { reason }) => ("cutting over", reason.as_str()),
        _ => ("without a complete reply", ""),
    };
    let own = if row.sentence.is_empty() {
        fallback
    } else {
        &row.sentence
    };
    // The command's own words: its control characters are removed here,
    // and whitespace is normalized below.
    let own: String = own
        .chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect();
    let own = sentence(&own, 512);
    if own.is_empty() {
        format!("The cancel command answered {state}.")
    } else {
        format!("The cancel command answered {state}: {own}.")
    }
}
fn sentence(text: &str, limit: usize) -> String {
    cut(text, limit)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches('.')
        .to_owned()
}
