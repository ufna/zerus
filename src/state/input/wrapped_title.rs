//! One untrusted native thread-title field, word-wrapped after an exact cwd.
//! The caller owns model, styles, two-row count, cursor and native-width proof.
use super::shown_path;
use unicode_width::UnicodeWidthStr;

pub(super) fn matches(
    folder: &str,
    first: &str,
    next: &str,
    columns: usize,
    windows: bool,
) -> bool {
    if columns <= 2 {
        return false;
    }
    let Some((_, body)) = first.split_once(" · ") else {
        return false;
    };
    let Some(rest) = shown_path::strip_prefix(body, folder, windows) else {
        return false;
    };
    // Codex may wrap the first word of the title. The native capture then
    // ends the model/folder row at the separator, without its trailing space.
    let Some(title) = rest
        .strip_prefix(" · ")
        .or_else(|| (rest == " ·").then_some(""))
    else {
        return false;
    };
    let Some(continuation) = next.strip_prefix("  ") else {
        return false;
    };
    if title.trim() != title
        || continuation.trim() != continuation
        || continuation.width() == 0
        || first.width() > columns
        || next.width() > columns
    {
        return false;
    }
    let joined = format!("{title} {continuation}");
    // With another agent in the thread roster, Codex appends the native
    // primary-thread label to its status line. The title remains one field
    // even when the last word of that label is on the continuation row.
    let title = joined.strip_suffix(" · Main [default]").unwrap_or(&joined);
    if title.is_empty() || title.contains(" · ") || title.chars().any(char::is_control) {
        return false;
    }
    let Some(word) = continuation.split_whitespace().next() else {
        return false;
    };
    // It must be a needed native word wrap, not an arbitrary extra paragraph.
    let first_width = first.trim_end_matches(' ').width();
    first_width + 1 + word.width() > columns
        && (first_width - 2 + 1 + continuation.width()).div_ceil(columns - 2) == 2
}
