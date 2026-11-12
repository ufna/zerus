//! Text that something outside the service wrote: a command's answer, Git's
//! error output, a file name, a member's report (zerus-63rru.59). It is
//! cleaned where the service takes it, before it can reach a card, an item
//! or a recorded reason: no control character reaches a person's or the
//! lead's eyes, and nothing but a report spans more than one line.

/// One line: control characters removed, every run of whitespace (a line
/// break among them) joined into one space, cut to `limit` bytes first. The
/// words are otherwise kept, so a stable sentence is recorded unchanged.
pub(crate) fn sentence(text: &str, limit: usize) -> String {
    let text: String = text
        .chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect();
    cut(&text, limit)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Several lines, as a report has: control characters removed except line
/// feeds and tabs; a carriage return is dropped, and a Unicode line or
/// paragraph separator becomes a line feed, the one line break kept.
pub(crate) fn lines(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            '\n' | '\t' => Some(c),
            '\u{2028}' | '\u{2029}' => Some('\n'),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

/// A path shown on one line, cut to `limit` bytes. A path that is blank or
/// holds a control character or a line separator is shown changed: quoted,
/// with each such character escaped, so two different names never read the
/// same and no name starts a line of its own.
pub(crate) fn path(name: &str, limit: usize) -> String {
    let name = cut(name, limit);
    if !name.trim().is_empty() && !name.chars().any(breaks) {
        return name.to_owned();
    }
    let mut shown = String::from("\"");
    for ch in name.chars() {
        let piece = match ch {
            '\u{2028}' => "\\u2028".to_owned(),
            '\u{2029}' => "\\u2029".to_owned(),
            ch => {
                let quoted =
                    serde_json::to_string(&ch.to_string()).expect("a character is JSON text");
                quoted[1..quoted.len() - 1].to_owned()
            }
        };
        if shown.len() + piece.len() + 1 > limit {
            break;
        }
        shown.push_str(&piece);
    }
    shown.push('"');
    shown
}

/// A character that would end a line or act on a terminal.
fn breaks(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
}

fn cut(text: &str, limit: usize) -> &str {
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOSTILE: &str = "a\u{7}b\u{1b}[31mc\u{0}d\u{9b}e\r\nf\u{2028}g\u{2029}h\u{85}i\tj  k";

    #[test]
    fn a_sentence_is_one_clean_bounded_line() {
        assert_eq!(sentence(HOSTILE, 4096), "ab[31mcde f g h i j k");
        assert_eq!(sentence("  The task left.  ", 512), "The task left.");
        assert_eq!(sentence("é".repeat(400).as_str(), 513).len(), 512);
        assert_eq!(sentence("\u{7}\u{1b}", 512), "");
    }

    #[test]
    fn a_report_keeps_its_line_feeds_and_tabs_and_nothing_else_of_control() {
        assert_eq!(lines(HOSTILE), "ab[31mcde\nf\ng\nhi\tj  k");
        assert_eq!(lines("one\r\ntwo\rthree"), "one\ntwothree");
    }

    #[test]
    fn a_path_is_shown_unchanged_or_quoted_on_one_line() {
        assert_eq!(path("src/main.rs", 4096), "src/main.rs");
        assert_eq!(path("éé", 4096), "éé");
        for (name, shown) in [
            ("   ", r#""   ""#),
            ("line\nbreak", r#""line\nbreak""#),
            ("bell\u{7}", r#""bell\u0007""#),
            ("esc\u{1b}[31m", r#""esc\u001b[31m""#),
            ("ls\u{2028}ps\u{2029}", r#""ls\u2028ps\u2029""#),
        ] {
            assert_eq!(path(name, 4096), shown, "{name:?}");
            assert!(!path(name, 4096).chars().any(breaks));
        }
        let long = format!("\n{}", "x".repeat(5000));
        let shown = path(&long, 4096);
        assert!(shown.len() <= 4096 && shown.starts_with('"') && shown.ends_with('"'));
    }
}
