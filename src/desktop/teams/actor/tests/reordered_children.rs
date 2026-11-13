//! A child's Start and Stop are written by two hook processes and can arrive
//! in either order (zerus-63rru.78). The rows are made by the product: the
//! supervisor and SessionStart hook build the record, the hook writer applies
//! the child events, and the listing's own steps make the row this test reads
//! with the actor's facts. A Start for a child the record already holds as
//! ended must not count as active again; the cases the writer cannot tell
//! apart are pinned as they are, with what bounds them.
use super::super::fleet::Facts;
use super::super::ports::Fact;
use serde_json::Value;

const CHILD: &str = "state::subagent_order::reordered_children_child";

fn produced() -> Value {
    let text = super::real_rows::isolated(CHILD);
    let line = text
        .lines()
        .find_map(|line| line.split_once("SUBAGENT-ORDER ").map(|(_, rows)| rows))
        .expect("the child printed its rows");
    let value: Value = serde_json::from_str(line).expect("the rows are one JSON object");
    value["rows"].clone()
}

/// (row, source, complete flag, count, unsettled, free, unknown fact, seen working)
#[allow(clippy::type_complexity)]
const EXPECTED: [(
    &str,
    &str,
    bool,
    Option<u64>,
    bool,
    bool,
    Option<Fact>,
    Option<bool>,
); 11] = [
    // Claude Code: both orders of one child's events leave it ended.
    (
        "claude_start_stop",
        "hooks",
        true,
        Some(0),
        false,
        true,
        None,
        Some(false),
    ),
    (
        "claude_stop_start",
        "hooks",
        true,
        Some(0),
        false,
        true,
        None,
        Some(false),
    ),
    (
        "claude_late_start",
        "hooks",
        true,
        Some(0),
        false,
        true,
        None,
        Some(false),
    ),
    // Codex: its events name only the child's ID; the same holds.
    (
        "codex_start_stop",
        "hooks",
        true,
        Some(0),
        false,
        true,
        None,
        Some(false),
    ),
    (
        "codex_stop_start",
        "hooks",
        true,
        Some(0),
        false,
        true,
        None,
        Some(false),
    ),
    // Kimi's profile hooks carry no child identity: a reordered Start cannot
    // be told from a new child of the same profile, so it stays counted and
    // the count stays incomplete (busy, never silently free).
    (
        "kimi_start_stop",
        "hook_profiles",
        true,
        Some(0),
        false,
        true,
        None,
        Some(false),
    ),
    (
        "kimi_stop_start",
        "hook_profiles",
        false,
        Some(1),
        true,
        false,
        None,
        Some(true),
    ),
    // One of two children reordered: the other one's history is unaffected.
    (
        "two_children_one_reordered",
        "hooks",
        true,
        Some(0),
        false,
        true,
        None,
        Some(false),
    ),
    // A Start for a child pruned above the cap has no ended entry to meet.
    (
        "claude_pruned_start",
        "hooks",
        true,
        Some(1),
        false,
        false,
        None,
        Some(true),
    ),
    // Nothing else changes: a real new child counts, a lone Stop stays history.
    (
        "claude_start_only",
        "hooks",
        true,
        Some(1),
        false,
        false,
        None,
        Some(true),
    ),
    (
        "claude_stop_only",
        "hooks",
        true,
        Some(0),
        false,
        true,
        None,
        Some(false),
    ),
];

#[test]
fn a_start_for_an_ended_child_never_counts_as_active_again() {
    let rows = produced();
    for (name, source, complete, count, unsettled, free, unknown, seen) in EXPECTED {
        let row = &rows[name];
        assert_eq!(row["subagent_source"], source, "{name}");
        assert_eq!(row["subagent_counts_complete"], complete, "{name}");
        let facts = Facts::of(row);
        assert_eq!(facts.children, count, "{name}: {row}");
        assert_eq!(facts.children_unsettled, unsettled, "{name}");
        assert_eq!(facts.free(), free, "{name}: {row}");
        assert_eq!(facts.unknown(), unknown, "{name}");
        assert_eq!(facts.seen().map(|seen| seen.working), seen, "{name}");
    }
    // The two orders of one child are indistinguishable to the member: both
    // leave it free, which is what hands it its next task.
    let rows = &rows;
    assert_eq!(
        Facts::of(&rows["claude_stop_start"]).free(),
        Facts::of(&rows["claude_start_stop"]).free(),
    );
}
