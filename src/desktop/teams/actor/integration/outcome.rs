//! Reserve the entire possible receipt before Git; a certain large conflict
//! must fit both the private ledger row and the store's 200-entry fact.
use super::*;
const MAX_ROW: usize = 16_384;
fn size(row: &MergeRow) -> Result<usize> {
    serde_json::to_vec(row)
        .map(|v| v.len())
        .map_err(crate::teams::store::storage)
}
fn future(row: &MergeRow, outcome: MergeResult) -> MergeRow {
    let mut next = row.clone();
    next.padding.clear();
    next.outcome = Some(outcome);
    next.fact_id = Some("00000000-0000-4000-8000-000000000001".into());
    next.fact_revision = Some(u64::MAX);
    next
}
pub(super) fn reserve(row: &mut MergeRow) -> Result<()> {
    let worst = future(
        row,
        MergeResult::Unknown {
            reason: "x".repeat(2048),
        },
    );
    if size(&worst)? > MAX_ROW {
        return Err(problem("Integration plan leaves no room for its outcome"));
    }
    // Charge the complete future receipt, not only the small initial intent.
    row.padding = "x".into();
    row.padding = " ".repeat(MAX_ROW - size(row)? + 1);
    Ok(())
}
fn conflicts(row: &MergeRow, files: Vec<String>) -> Result<Vec<String>> {
    let total = files.len();
    let mut kept = Vec::new();
    for name in files.into_iter().take(199) {
        kept.push(cut(&name, 4096));
        let mut candidate = kept.clone();
        if total > kept.len() {
            candidate.push(format!("and {} more files", total - kept.len()));
        }
        if size(&future(row, MergeResult::Conflict { files: candidate }))? > MAX_ROW {
            kept.pop();
            break;
        }
    }
    if total > kept.len() {
        kept.push(format!("and {} more files", total - kept.len()));
    }
    Ok(kept)
}
pub(super) fn save(row: &mut MergeRow, reply: MergeResult) -> Result<()> {
    row.padding.clear();
    row.outcome = Some(match reply {
        MergeResult::Conflict { files } => MergeResult::Conflict {
            files: conflicts(row, files)?,
        },
        MergeResult::Unknown { reason } => MergeResult::Unknown {
            reason: cut(&reason, 2048),
        },
        MergeResult::Unavailable { reason } => MergeResult::Unavailable {
            reason: cut(&reason, 2048),
        },
        merged => merged,
    });
    Ok(())
}
