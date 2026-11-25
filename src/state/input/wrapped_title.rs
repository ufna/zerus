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
    let Some(continuation) = next.strip_prefix("  ") else {
        return false;
    };
    // When even the separator cannot fit, Codex wraps it with the title.
    // Otherwise, the first row carries the separator and possibly title text.
    let separator_wrapped = rest.is_empty();
    let (title, continuation) = if separator_wrapped {
        let Some(continuation) = continuation.strip_prefix("· ") else {
            return false;
        };
        ("", continuation)
    } else {
        let Some(title) = rest
            .strip_prefix(" · ")
            .or_else(|| (rest == " ·").then_some(""))
        else {
            return false;
        };
        (title, continuation)
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
    let word_width = if separator_wrapped { 1 } else { word.width() };
    let separator_width = if separator_wrapped { 2 } else { 0 };
    first_width + 1 + word_width > columns
        && (first_width - 2 + 1 + separator_width + continuation.width()).div_ceil(columns - 2) == 2
}
