//! Claude AskUserQuestion: verify each native panel before a small input, then
//! acknowledge only the exact tool result in the pinned conversation transcript.
use super::question_terminal::{Native, Transport};
use super::*;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::time::{Duration, Instant};

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
#[derive(Debug, PartialEq)]
enum Panel {
    Question {
        index: usize,
        focused: usize,
        checked: BTreeSet<usize>,
        other: String,
    },
    Review {
        answers: Vec<String>,
        focused: usize,
    },
}
impl Panel {
    fn index(&self, count: usize) -> usize {
        match self {
            Self::Question { index, .. } => *index,
            Self::Review { .. } => count,
        }
    }
}
fn numbered(row: &str) -> Option<(usize, bool, &str)> {
    let row = row.trim();
    let focused = row.starts_with('❯');
    let row = row.strip_prefix('❯').unwrap_or(row).trim_start();
    let (number, text) = row.split_once(". ")?;
    Some((number.parse::<usize>().ok()?.checked_sub(1)?, focused, text))
}
fn divider(row: &str) -> bool {
    row.chars().count() > 10 && row.chars().all(|c| c == '─')
}
pub(super) fn same_panel(first: &str, second: &str, card: &Value) -> bool {
    matches!((parse(first,card),parse(second,card)),(Ok(a),Ok(b)) if a==b)
}
fn parse(screen: &str, card: &Value) -> Result<Panel> {
    let plain = question_terminal::plain(screen)?;
    let rows: Vec<_> = plain.trim().lines().map(str::trim).collect();
    let items = card["questions"]
        .as_array()
        .ok_or("Missing Claude question schema")?;
    if items.is_empty() {
        return Err("Missing Claude questions".into());
    }
    let single_header = if string(&items[0], "header").is_empty() {
        "Q1"
    } else {
        string(&items[0], "header")
    };
    let start = rows
        .iter()
        .rposition(|r| {
            (r.starts_with('←') && r.ends_with('→') && r.contains("✔ Submit"))
                || (items.len() == 1
                    && [format!("☐ {single_header}"), format!("☒ {single_header}")]
                        .contains(&r.to_string()))
        })
        .ok_or("Claude's question is not fully visible. Open Terminal to check it.")?;
    if start == 0 || !divider(rows[start - 1]) {
        return Err("Unrecognized Claude question panel".into());
    }
    let rows = &rows[start + 1..];
    let title = rows
        .iter()
        .position(|r| !r.is_empty())
        .ok_or("Empty Claude question panel")?;
    if rows[title] == "Review your answers" {
        let end = rows
            .iter()
            .position(|r| *r == "Ready to submit your answers?")
            .ok_or("Claude's answer review is incomplete")?;
        let mut seen: Vec<(String, String)> = Vec::new();
        let mut in_answer = false;
        let gutter = rows[title + 1..end]
            .iter()
            .find(|row| !row.is_empty())
            .is_some_and(|row| row.strip_prefix("│ ").is_some_and(|text| text.trim_start().starts_with("● ")));
        for row in &rows[title + 1..end] {
            // The same gutter appears on review question headings and their
            // wrapped lines. Answer text is not decorated: a literal bar in a
            // wrapped custom answer must survive the final exact comparison.
            let unframed = row.strip_prefix("│ ").unwrap_or(row).trim_start();
            let row = if gutter && (!in_answer || unframed.starts_with("● ")) {
                unframed
            } else {
                row
            };
            if let Some(text) = row.strip_prefix("● ") {
                seen.push((text.into(), String::new()));
                in_answer = false;
            } else if let Some(text) = row.strip_prefix("→ ") {
                seen.last_mut().ok_or("Unexpected review answer")?.1 = text.into();
                in_answer = true;
            } else if !row.is_empty() {
                let item = seen
                    .last_mut()
                    .ok_or("Not all Claude questions have answers")?;
                let text = if in_answer { &mut item.1 } else { &mut item.0 };
                text.push(' ');
                text.push_str(row);
            }
        }
        if seen.len() != items.len()
            || seen
                .iter()
                .zip(items)
                .any(|((q, _), item)| norm(q) != norm(string(item, "question")))
        {
            return Err("Claude's review does not match this question request".into());
        }
        let options: Vec<_> = rows[end + 1..].iter().filter(|r| !r.is_empty()).collect();
        if options.len() != 2 {
            return Err("Unrecognized Claude review controls".into());
        }
        let mut focus = None;
        for (index, label) in ["Submit answers", "Cancel"].iter().enumerate() {
            let (number, selected, text) =
                numbered(options[index]).ok_or("Unknown review choice")?;
            if number != index || text != *label || (selected && focus.replace(index).is_some()) {
                return Err("Unknown review choice".into());
            }
        }
        return Ok(Panel::Review {
            answers: seen.into_iter().map(|(_, a)| norm(&a)).collect(),
            focused: focus.ok_or("Missing review selection")?,
        });
    }
    let footer = rows.last().ok_or("Missing Claude controls")?;
    if !footer.contains("Enter to select")
        || !footer.contains("Esc to cancel")
        || !footer.contains("navigate")
    {
        return Err("Claude's question controls are not fully visible. Enlarge Terminal.".into());
    }
    let first = rows
        .iter()
        .position(|r| numbered(r).is_some())
        .ok_or("Missing Claude choices")?;
    // Claude also renders prompts with a dim vertical gutter. It is terminal
    // decoration, not part of the tool's question. Strip it only at the start
    // of prompt lines; the question and every option must still match exactly.
    let raw_text = norm(&rows[title..first].join(" "));
    let text = norm(
        &rows[title..first]
            .iter()
            .map(|row| row.strip_prefix("│ ").unwrap_or(row))
            .collect::<Vec<_>>()
            .join(" "),
    );
    let matching: Vec<_> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            let expected = norm(string(item, "question"));
            expected == text || expected == raw_text
        })
        .collect();
    if matching.len() != 1 {
        return Err("The visible Claude question differs from this request".into());
    }
    let (index, item) = matching[0];
    let options = item["options"].as_array().ok_or("Missing options")?;
    let end = rows
        .iter()
        .enumerate()
        .skip(first)
        .find(|(_, r)| divider(r))
        .map(|(i, _)| i)
        .ok_or("Claude choices are clipped")?;
    let mut labels: Vec<(String, String)> = Vec::new();
    let mut focused = None;
    let mut checked = BTreeSet::new();
    for row in &rows[first..end] {
        if let Some((number, selected, text)) = numbered(row) {
            if number != labels.len()
                || number > options.len()
                || (selected && focused.replace(number).is_some())
            {
                return Err("Unknown Claude option order".into());
            }
            let text = if item["multi_select"] == true {
                if let Some(text) = text.strip_prefix("[✔] ") {
                    checked.insert(number);
                    text
                } else {
                    text.strip_prefix("[ ] ")
                        .ok_or("Unknown Claude checkbox state")?
                }
            } else {
                text
            };
            labels.push((text.into(), String::new()));
        } else if !row.is_empty() && !["Next", "Submit", "❯ Next", "❯ Submit"].contains(row) {
            // Descriptions wrap below their labels. Reject missing/clipped labels
            // rather than applying an option index to a partial menu.
            let last = labels.last_mut().ok_or("Unexpected option description")?;
            last.1.push(' ');
            last.1.push_str(row);
        }
    }
    if labels.len() != options.len() + 1 {
        return Err("Claude choices are clipped. Enlarge Terminal.".into());
    }
    for (number, ((label, description), option)) in labels.iter().zip(options).enumerate() {
        let expected = norm(string(option, "label"));
        let committed = item["multi_select"] != true
            && label.strip_suffix(" ✔").is_some_and(|text| norm(text) == expected);
        if (norm(label) != expected && !committed)
            || norm(description) != norm(string(option, "description")) {
            return Err("Claude's visible options differ from this request".into());
        }
        if committed { checked.insert(number); }
    }
    if item["multi_select"] != true && checked.len() > 1 {
        return Err("Claude shows multiple selected answers for a single choice".into());
    }
    let other = &labels[options.len()].0;
    let other = if ["Type something", "Type something."].contains(&other.as_str()) {
        String::new()
    } else {
        norm(other)
    };
    Ok(Panel::Question {
        index,
        focused: focused.ok_or("Select an answer option in Terminal first")?,
        checked,
        other,
    })
}
fn wait<T: Transport>(
    io: &mut T,
    card: &Value,
    expected: impl Fn(&Panel) -> bool,
) -> Result<Panel> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let panel = parse(&io.screen()?, card)?;
        if expected(&panel) {
            return Ok(panel);
        }
        if Instant::now() >= deadline {
            return Err("Claude did not confirm question navigation".into());
        }
        io.pause();
    }
}
fn goto<T: Transport>(io: &mut T, card: &Value, index: usize) -> Result<Panel> {
    let items = card["questions"].as_array().unwrap();
    for _ in 0..=items.len() + 1 {
        let panel = parse(&io.screen()?, card)?;
        let current = panel.index(items.len());
        if current == index {
            return Ok(panel);
        }
        if let Panel::Question { focused, other, .. } = &panel {
            if *focused == items[current]["options"].as_array().unwrap().len() {
                if !other.is_empty() {
                    return Err(
                        "Finish the existing Other draft in Terminal before answering".into(),
                    );
                }
                io.key("Up")?;
                wait(
                    io,
                    card,
                    |p| matches!(p,Panel::Question{focused:f,..} if *f<*focused),
                )?;
            }
        }
        // Tab does not wrap and Other owns horizontal arrows while editing.
        let next = if current < index {
            current + 1
        } else {
            current - 1
        };
        io.key(if current < index { "Tab" } else { "Left" })?;
        wait(io, card, |p| p.index(items.len()) == next)?;
    }
    Err("Could not reach the requested Claude question".into())
}
fn selected_options(item: &Value, answer: &Value) -> BTreeSet<usize> {
    item["options"].as_array().unwrap().iter().enumerate()
        .filter(|(_, option)| answer["selected_option_ids"].as_array().unwrap().contains(&option["id"]))
        .map(|(index, _)| index).collect()
}
fn drive<T: Transport>(io: &mut T, card: &Value, answers: &Value) -> Result<()> {
    let items = card["questions"].as_array().ok_or("Missing questions")?;
    let answers = answers.as_array().ok_or("Missing answers")?;
    let wanted: Vec<_> = items.iter().zip(answers).map(|(item, answer)| {
        let mut labels: Vec<_> = selected_options(item, answer).iter()
            .map(|index| string(&item["options"][*index], "label").to_owned()).collect();
        if !string(answer, "text").is_empty() { labels.push(string(answer, "text").into()); }
        norm(&labels.join(", "))
    }).collect();
    // A failed pre-submit attempt may already have populated the exact review.
    // Confirm that state directly instead of revisiting every committed choice.
    if matches!(parse(&io.screen()?, card)?, Panel::Review { answers, .. } if answers == wanted) {
        return submit_review(io, card, &wanted);
    }
    for (index, (item, answer)) in items
        .iter()
        .zip(answers)
        .enumerate()
    {
        let options = item["options"].as_array().unwrap();
        let count = options.len();
        let selected = selected_options(item, answer);
        let text = string(answer, "text");
        let multi = item["multi_select"] == true;
        let panel = goto(io, card, index)?;
        if let Panel::Question { other, .. } = &panel {
            if !other.is_empty() && norm(other) != norm(text) {
                return Err("Another Other draft exists in Terminal; it was preserved".into());
            }
        }
        if multi {
            for pass in 0..2 {
                for option in 0..=count {
                    let panel = parse(&io.screen()?, card)?;
                    let Panel::Question {
                        index: current,
                        checked,
                        focused,
                        ..
                    } = panel
                    else {
                        return Err("Claude left the current question".into());
                    };
                    if current != index {
                        return Err("Claude changed question during selection".into());
                    }
                    // Leave a previously focused input before sending numeric keys.
                    if focused == count {
                        io.key("Up")?;
                        wait(
                            io,
                            card,
                            |p| matches!(p,Panel::Question{focused:f,..} if *f<count),
                        )?;
                    }
                    let desired = pass == 1 && option < count && selected.contains(&option);
                    if checked.contains(&option) != desired {
                        io.key(&(option + 1).to_string())?;
                        wait(
                            io,
                            card,
                            |p| matches!(p,Panel::Question{index:i,checked:c,..} if *i==index && c.contains(&option)==desired),
                        )?;
                    }
                }
            }
        }
        if !text.is_empty() {
            let panel = parse(&io.screen()?, card)?;
            let Panel::Question { focused, other, .. } = panel else {
                return Err("Claude left the current question".into());
            };
            if focused != count {
                // Numeric input chooses Other for a single-select; on a multi-
                // select it toggles, so focus Other with verified Down presses.
                if multi {
                    for next in focused + 1..=count {
                        io.key("Down")?;
                        wait(
                            io,
                            card,
                            |p| matches!(p,Panel::Question{focused:f,..} if *f==next),
                        )?;
                    }
                } else {
                    io.key(&(count + 1).to_string())?;
                    wait(
                        io,
                        card,
                        |p| matches!(p,Panel::Question{focused:f,..} if *f==count),
                    )?;
                }
            }
            if other.is_empty() {
                io.paste(text)?;
                wait(
                    io,
                    card,
                    |p| matches!(p,Panel::Question{other,..} if norm(other)==norm(text)),
                )?;
            }
            if multi {
                // Typing selects Other automatically; Enter in this text
                // field is a no-op. Preserve its checked state when leaving.
                io.key("Up")?;
                wait(
                    io,
                    card,
                    |p| matches!(p,Panel::Question{focused:f,..} if *f<count),
                )?;
                let p = parse(&io.screen()?, card)?;
                if matches!(&p,Panel::Question{checked,..} if !checked.contains(&count)) {
                    io.key(&(count + 1).to_string())?;
                    wait(
                        io,
                        card,
                        |p| matches!(p,Panel::Question{other,checked,..} if norm(other)==norm(text) && checked.contains(&count)),
                    )?;
                }
                let p = parse(&io.screen()?, card)?;
                if matches!(p,Panel::Question{focused:f,..} if f==count) {
                    io.key("Up")?;
                    wait(
                        io,
                        card,
                        |p| matches!(p,Panel::Question{focused:f,..} if *f<count),
                    )?;
                }
                io.key("Tab")?;
            } else {
                io.key("Enter")?;
                if items.len() == 1 {
                    return Ok(());
                }
            }
        } else if multi {
            io.key("Tab")?;
        } else {
            let panel = parse(&io.screen()?, card)?;
            if matches!(panel,Panel::Question{focused:f,..} if f==count) {
                io.key("Up")?;
                wait(
                    io,
                    card,
                    |p| matches!(p,Panel::Question{focused:f,..} if *f<count),
                )?;
            }
            io.key(&(selected.iter().next().ok_or("Missing selected answer")? + 1).to_string())?;
            if items.len() == 1 {
                return Ok(());
            }
        }
        wait(io, card, |p| p.index(items.len()) == index + 1)?;
    }
    goto(io, card, items.len())?;
    submit_review(io, card, &wanted)
}
fn submit_review<T: Transport>(io: &mut T, card: &Value, wanted: &[String]) -> Result<()> {
    let panel = parse(&io.screen()?, card)?;
    let Panel::Review {
        answers: actual,
        focused,
    } = panel
    else {
        return Err("Claude's final review is missing".into());
    };
    if actual != wanted {
        return Err(
            "Claude's final review differs from your answers; nothing was submitted".into(),
        );
    }
    if focused != 0 {
        io.key("Up")?;
        wait(
            io,
            card,
            |p| matches!(p,Panel::Review{answers,focused:0} if answers.as_slice()==wanted),
        )?;
    }
    if !matches!(parse(&io.screen()?,card)?,Panel::Review{answers,focused:0} if answers.as_slice()==wanted) {
        return Err("Claude's final review changed".into());
    }
    io.key("Enter")
}

pub(super) fn available(record: &Value, card: &Value) -> Result<()> {
    if string(record, "agent") != "claude" || string(card, "source") != "hook" {
        return Err("Not a Claude question".into());
    }
    let native = Native::new(record, card);
    let current = native.identity()?;
    if response(&current, card)?.is_some() {
        return Err("This Claude question was already answered; refresh Activity".into());
    }
    let panel = parse(&Native::capture(&current)?, card)?;
    if matches!(panel,Panel::Question{other,..} if !other.is_empty()) {
        return Err("Finish the existing Other draft in Terminal before answering".into());
    }
    Ok(())
}
pub(super) fn answer(record: &Value, card: &Value, answers: &Value) -> Result<()> {
    available(record, card)?;
    let mut native = Native::new(record, card);
    drive(&mut native, card, answers).map_err(|e| {
        if native.touched() {
            format!("delivery uncertain: {e}; inspect Terminal before retrying")
        } else {
            e
        }
    })
}
pub(super) fn response(record: &Value, card: &Value) -> Result<Option<Value>> {
    let id = string(record, "conversation_id");
    let path = PathBuf::from(string(record, "transcript"));
    if uuid::Uuid::parse_str(id).is_err()
        || !path.is_absolute()
        || !path
            .file_name()
            .is_some_and(|n| n.to_string_lossy() == format!("{id}.jsonl"))
    {
        return Err("Claude transcript identity is unavailable".into());
    }
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let offset = file
        .metadata()
        .map_err(|e| e.to_string())?
        .len()
        .saturating_sub(8 * 1024 * 1024);
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    for (index, line) in bytes.split_inclusive(|b| *b == b'\n').enumerate() {
        if (index == 0 && offset > 0) || !line.ends_with(b"\n") || line.len() > 1024 * 1024 {
            continue;
        }
        let Ok(event) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if string(&event, "sessionId") != id || event["isSidechain"] == true {
            continue;
        }
        let Some(parts) = event["message"]["content"].as_array() else {
            continue;
        };
        let Some(result) = parts
            .iter()
            .find(|p| p["type"] == "tool_result" && p["tool_use_id"] == card["tool_call_id"])
        else {
            continue;
        };
        if result["is_error"] == true {
            return Err("Claude rejected the question response".into());
        }
        let result = &event["toolUseResult"];
        if questions::normalize(&result["questions"])? != card["questions"]
            || !result["answers"].is_object()
        {
            return Err(
                "Claude's question acknowledgement differs from the pending request".into(),
            );
        }
        return Ok(Some(
            json!({"type":"interaction.resolved","time":now()*1000.,"response":{"answers":result["answers"]}}),
        ));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn items() -> Value {
        json!([
            {"question":"Which release should we prepare?","header":"Release","options":[{"label":"Stable","description":"Use stable build"},{"label":"Preview","description":"Use preview build"}]},
            {"question":"Which checks should we run?","header":"Checks","multiSelect":true,"options":[{"label":"Unit","description":"Unit checks"},{"label":"Integration","description":"Integration checks"},{"label":"UI","description":"UI checks"}]},
            {"question":"What else should we know?","header":"Notes","options":[{"label":"Nothing","description":"No extra notes"},{"label":"Later","description":"Discuss later"}]}
        ])
    }
    fn card() -> Value {
        json!({"questions":questions::normalize(&items()).unwrap(),"tool_call_id":"toolu_native_fixture"})
    }
    #[test]
    fn recognizes_recorded_claude_2_1_289_panels() {
        let card = card();
        assert!(matches!(
            parse(
                include_str!("../../tests/fixtures/claude-question-single.txt"),
                &card
            )
            .unwrap(),
            Panel::Question {
                index: 0,
                focused: 0,
                ..
            }
        ));
        assert!(
            matches!(parse(include_str!("../../tests/fixtures/claude-question-multi-selected.txt"),&card).unwrap(),Panel::Question{index:1,checked,..} if checked==BTreeSet::from([0,2]))
        );
        assert!(
            matches!(parse(include_str!("../../tests/fixtures/claude-question-other.txt"),&card).unwrap(),Panel::Question{index:2,focused:2,other,..} if other.is_empty())
        );
        assert!(
            matches!(parse(include_str!("../../tests/fixtures/claude-question-other-filled.txt"),&card).unwrap(),Panel::Question{index:2,focused:2,other,..} if other=="Привет from Zerus")
        );
        assert!(
            matches!(parse(include_str!("../../tests/fixtures/claude-question-review.txt"),&card).unwrap(),Panel::Review{focused:0,answers} if answers==["Preview","Unit, UI","Привет from Zerus"])
        );
    }
    #[test]
    fn refuses_changed_clipped_or_unrecognized_questions() {
        let screen = include_str!("../../tests/fixtures/claude-question-single.txt");
        for changed in [
            screen.replace("Which release should we prepare?", "Another question?"),
            screen.replace("Use preview build", "Different description"),
            screen.replace("2. Preview", "2. Other choice"),
            screen.replace("1. Stable", "2. Stable"),
            screen.replace("Esc to cancel", "Esc to leave"),
            screen.replace("❯ 1.", "  1."),
        ] {
            assert!(parse(&changed, &card()).is_err(), "{changed}");
        }
    }
    #[test]
    fn prompt_gutter_preserves_exact_question_and_option_matching() {
        let screen = include_str!("../../tests/fixtures/claude-question-single.txt");
        let card = card();
        for prompt in [
            "│ Which release should we prepare?",
            "\x1b[2m│\x1b[0m \x1b[1mWhich release should we prepare?\x1b[0m",
            "│ Which release should\n│ we prepare?",
            "│ Which release should\n  we prepare?",
        ] {
            let decorated = screen.replace("Which release should we prepare?", prompt);
            assert_eq!(parse(&decorated, &card).unwrap(), parse(screen, &card).unwrap());
            assert!(same_panel(screen, &decorated, &card));
            for changed in [
                decorated.replace("should", "must"),
                decorated.replace("2. Preview", "2. Other choice"),
                decorated.replace("Use preview build", "Different description"),
                decorated.replace("❯ 1. Stable", "❯ 1. │ Stable"),
            ] {
                assert!(parse(&changed, &card).is_err(), "{changed}");
            }
        }
        // A literal leading bar in the request remains valid too.
        let mut literal = card.clone();
        literal["questions"][0]["question"] = json!("│ Which release should we prepare?");
        let screen = screen.replace("Which release should we prepare?", "│ Which release should we prepare?");
        assert!(parse(&screen, &literal).is_ok());
    }
    #[test]
    fn recognizes_gutter_review_and_preserves_literal_answer_text() {
        let card = card();
        let plain = include_str!("../../tests/fixtures/claude-question-review.txt");
        let screen = include_str!("../../tests/fixtures/claude-question-review-gutter.txt");
        assert_eq!(parse(screen, &card).unwrap(), parse(plain, &card).unwrap());
        assert!(same_panel(plain, screen, &card));
        let styled = screen.replace("│", "\x1b[2m│\x1b[0m");
        assert!(same_panel(&styled, screen, &card));
        let literal = screen.replace("Привет from Zerus", "Привет from\n       │ Zerus");
        assert!(matches!(parse(&literal, &card).unwrap(), Panel::Review { answers, .. }
            if answers[2] == "Привет from │ Zerus"));
        assert!(!same_panel(screen, &literal, &card));
        for changed in [
            screen.replace("we prepare?", "we publish?"),
            screen.replace("│ ● Which checks should we run?\n   → Unit, UI\n", ""),
            screen.replace("Submit answers", "Run command"),
            screen.replace("❯ 1.", "  1."),
        ] {
            assert!(parse(&changed, &card).is_err(), "{changed}");
        }
    }
    #[test]
    fn recognizes_committed_single_choices_without_changing_labels() {
        let card = card();
        let screen = include_str!("../../tests/fixtures/claude-question-single.txt");
        let chosen = screen.replace("2. Preview", "2. Preview ✔");
        assert!(matches!(parse(&chosen, &card).unwrap(), Panel::Question { checked, .. }
            if checked == BTreeSet::from([1])));
        assert!(!same_panel(screen, &chosen, &card));
        assert!(parse(&chosen.replace("1. Stable", "1. Stable ✔"), &card).is_err());
        assert!(parse(&chosen.replace("2. Preview", "2. Different"), &card).is_err());
        let mut literal = card.clone();
        literal["questions"][0]["options"][1]["label"] = json!("Preview ✔");
        assert!(parse(&chosen, &literal).is_ok());
        assert!(matches!(parse(&chosen.replace("Preview ✔", "Preview ✔ ✔"), &literal).unwrap(),
            Panel::Question { checked, .. } if checked == BTreeSet::from([1])));
    }
    #[test]
    fn acknowledgement_requires_exact_tool_conversation_and_schema() {
        let dir = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let path = dir.path().join(format!("{id}.jsonl"));
        let record = json!({"conversation_id":id,"transcript":path});
        let card = card();
        let event = json!({"sessionId":id,"message":{"content":[{"type":"tool_result","tool_use_id":"toolu_native_fixture"}]},"toolUseResult":{"questions":items(),"answers":{"Which release should we prepare?":"Preview"}}});
        for field in ["sessionId", "isSidechain", "message"] {
            let mut stale = event.clone();
            stale[field] = if field == "isSidechain" {
                json!(true)
            } else {
                json!("another")
            };
            fs::write(&path, format!("{stale}\n")).unwrap();
            assert!(response(&record, &card).unwrap().is_none());
        }
        fs::write(&path, format!("{event}\n")).unwrap();
        assert!(response(&record, &card).unwrap().is_some());
        let mut changed = event;
        changed["toolUseResult"]["questions"][0]["question"] = json!("Changed request");
        fs::write(&path, format!("{changed}\n")).unwrap();
        assert!(response(&record, &card).is_err());
    }
}
