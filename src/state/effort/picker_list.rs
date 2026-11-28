//! Rows inside a provider-validated picker panel. A row never supplies
//! authority to open a panel, navigate a section, or apply a setting.

#[derive(Clone, Copy, Debug)]
pub(super) enum Layout {
    Numbered,
    Claude,
    Kimi,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Position {
    number: Option<String>,
    label: Vec<String>,
    description: Vec<String>,
    pub section: Option<char>,
}

#[derive(Debug)]
pub(super) struct Row {
    #[cfg(test)]
    pub raw: String,
    pub position: Position,
    caption: String,
    description: String,
    layout: Layout,
    selected: bool,
}

fn after_marker<'a>(text: &'a str, markers: &[char]) -> Option<(char, &'a str)> {
    let marker = text.chars().next()?;
    let rest = &text[marker.len_utf8()..];
    (markers.contains(&marker) && rest.starts_with(char::is_whitespace))
        .then(|| (marker, rest.trim_start()))
}

/// A tab, a run of spaces, or a middle dot separates display columns.
/// A lone space can be part of a multiword model name.
fn columns(text: &str) -> (&str, &str) {
    let mut gap = None;
    for (index, ch) in text.char_indices() {
        if ch == '·' && gap.is_some() {
            return (
                text[..gap.unwrap_or(index)].trim_end(),
                text[index..].trim_start_matches('·').trim_start(),
            );
        }
        if ch.is_whitespace() {
            if ch == '\t' || gap.is_some() {
                return (&text[..gap.unwrap_or(index)], text[index..].trim_start());
            }
            gap = Some(index);
        } else {
            gap = None;
        }
    }
    (text, "")
}

fn tokens(text: &str) -> Vec<String> {
    let text = text
        .to_lowercase()
        .replace("(current)", " ")
        .replace("(default)", " ")
        .replace("← current", " ");
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

impl Row {
    pub(super) fn read(line: &str, layout: Layout) -> Option<Self> {
        let line = line.trim();
        let cursor = after_marker(line, &['❯', '›', '>', '▶']);
        let raw = cursor.map_or(line, |(_, row)| row);
        let section = matches!(layout, Layout::Claude)
            .then(|| after_marker(raw, &['▸', '▾']))
            .flatten();
        let (number, text) = if let Some((_, text)) = section {
            (None, text)
        } else if let Some((number, rest)) = raw.split_once('.').filter(|(number, rest)| {
            !number.is_empty()
                && number.bytes().all(|b| b.is_ascii_digit())
                && rest.starts_with(char::is_whitespace)
        }) {
            (Some(number), rest.trim_start())
        } else if matches!(layout, Layout::Kimi) {
            (None, raw)
        } else {
            return None;
        };
        let text = text.trim_start_matches(|ch: char| {
            ch.is_whitespace() || matches!(ch, '✔' | '✓' | '●' | '◉')
        });
        if !text.starts_with(char::is_alphanumeric) {
            return None;
        }
        let (caption, description) = columns(text);
        let label = tokens(caption);
        if label.is_empty() {
            return None;
        }
        Some(Self {
            #[cfg(test)]
            raw: raw.to_owned(),
            position: Position {
                number: number.map(str::to_owned),
                label,
                description: tokens(description),
                section: section.map(|(marker, _)| marker),
            },
            caption: caption.to_owned(),
            description: description.to_owned(),
            layout,
            selected: cursor.is_some(),
        })
    }

    fn matches(&self, name: &[String]) -> bool {
        let label = self.position.label.as_slice();
        let description = self.position.description.as_slice();
        let recommended_default = label == ["default", "recommended"]
            || (label == ["default"]
                && description
                    .first()
                    .is_some_and(|word| word == "recommended"));
        if self.position.section.is_some() || recommended_default || name.is_empty() {
            return false;
        }
        label == name
            || (matches!(self.layout, Layout::Claude)
                && !label.starts_with(name)
                && tokens(self.description.split('·').next().unwrap_or("")) == name)
    }
}

#[derive(Debug)]
pub(super) struct List {
    rows: Vec<Row>,
    selected: usize,
}

impl List {
    pub(super) fn read<'a>(lines: impl Iterator<Item = &'a str>, layout: Layout) -> Option<Self> {
        let rows: Vec<_> = lines.filter_map(|line| Row::read(line, layout)).collect();
        let mut marked = rows.iter().enumerate().filter(|(_, row)| row.selected);
        let (selected, _) = marked.next()?;
        marked.next().is_none().then_some(Self { rows, selected })
    }

    pub(super) fn selected(&self) -> &Row {
        &self.rows[self.selected]
    }

    pub(super) fn selected_matches(&self, name: &str) -> Result<bool, &'static str> {
        let name = tokens(name);
        let mut matches = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.matches(&name));
        let first = matches.next();
        if matches.next().is_some() {
            return Err("native model picker offers more than one matching row; inspect Terminal");
        }
        Ok(first.is_some_and(|(index, _)| index == self.selected))
    }

    pub(super) fn edges(&self) -> (bool, bool) {
        (self.selected == 0, self.selected + 1 == self.rows.len())
    }
}

#[cfg(test)]
pub(super) fn row_matches(line: &str, layout: Layout, name: &str) -> bool {
    Row::read(line, layout).is_some_and(|row| row.matches(&tokens(name)))
}

#[derive(Default)]
pub(super) struct Seen {
    labels: Vec<String>,
}

impl Seen {
    pub(super) fn observe(&mut self, list: &List) {
        for row in &list.rows {
            if self.labels.len() == 20 {
                break;
            }
            if row.position.section.is_some() {
                continue;
            }
            let label: String = row
                .caption
                .chars()
                .filter(|ch| !ch.is_control())
                .take(40)
                .collect();
            if !self.labels.contains(&label) {
                self.labels.push(label);
            }
        }
    }

    pub(super) fn refusal(&self, reason: &str) -> String {
        if self.labels.is_empty() {
            reason.to_owned()
        } else {
            format!("{reason}. Visible labels: {}", self.labels.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_choice_reports_a_bounded_sample_of_visible_labels() {
        let screen: Vec<_> = (1..=24)
            .map(|i| {
                format!(
                    "{cursor}{i}. Label {i:02} {}  Details",
                    "x".repeat(60),
                    cursor = if i == 1 { "› " } else { "" }
                )
            })
            .collect();
        let rows = List::read(screen.iter().map(String::as_str), Layout::Numbered).unwrap();
        assert_eq!(rows.selected_matches("Unavailable"), Ok(false));
        let mut seen = Seen::default();
        seen.observe(&rows);
        seen.observe(&rows);
        assert_eq!(seen.labels.len(), 20);
        assert!(seen.labels.iter().all(|label| label.chars().count() == 40));
        let message = seen.refusal("model is not offered");
        assert!(message.starts_with("model is not offered. Visible labels: Label 01"));
        assert!(message.contains("Label 20"));
        assert!(!message.contains("Label 21"));
    }
}
