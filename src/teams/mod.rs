//! Agent Teams: a lead session, member sessions with roles, and work that
//! runs through a pipeline. See
//! `docs/superpowers/specs/2026-11-11-agent-teams-design.md`.
//!
//! Teams are a preview and are off unless the person turned them on for this
//! computer. With them off nothing here creates a file, and every `hgs team`
//! command except `preview` answers `teams_inactive`.
use crate::cli::Result;
use crate::config::Config;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use zerus_team_plan::problem::code;
use zerus_team_plan::Problem;

pub mod catalog;

/// The folder of everything Teams keep under the `hgs` state directory.
const DIR: &str = "teams";
/// Its presence turns the preview on for this computer.
const PREVIEW: &str = "preview";

const INACTIVE: &str =
    "Agent Teams are off on this computer. Nothing was attempted. A person turns them on with: hgs team preview on";

fn preview_file(state: &Path) -> PathBuf {
    state.join(DIR).join(PREVIEW)
}

/// Whether the person turned Teams on for this computer.
pub(crate) fn enabled(state: &Path) -> bool {
    preview_file(state).is_file()
}

pub(crate) fn dispatch(config: &Config, args: &[String], _dry: bool) -> Result<i32> {
    let (reply, exit) = answer(&config.state, args, from_session());
    println!("{reply}");
    Ok(exit)
}

/// An agent's shell carries both variables; a person's terminal does not.
/// This is the interim person rule of the specification, not proof.
fn from_session() -> bool {
    let set = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    set("HGS_SESSION") && set("HGS_RUN_ID")
}

fn refusal(problem: Problem) -> (Value, i32) {
    (json!({"ok": false, "error": problem}), 1)
}

fn answer(state: &Path, args: &[String], from_session: bool) -> (Value, i32) {
    let words: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|word| *word != "--json")
        .collect();
    if words.first() == Some(&"preview") {
        return preview(
            state,
            words.get(1).copied().unwrap_or("status"),
            from_session,
        );
    }
    if !enabled(state) {
        return refusal(Problem::new(code::TEAMS_INACTIVE, INACTIVE));
    }
    let command = words.first().copied().unwrap_or("");
    refusal(
        Problem::new(code::NOT_FOUND, format!("Unknown team command: {command}"))
            .with_refs([command]),
    )
}

fn preview(state: &Path, action: &str, from_session: bool) -> (Value, i32) {
    let shown =
        |state: &Path| json!({"ok": true, "preview": if enabled(state) { "on" } else { "off" }});
    match action {
        "status" => (shown(state), 0),
        "on" | "off" if from_session => refusal(Problem::new(
            code::NOT_ALLOWED,
            "Only a person turns the Teams preview on or off, from a terminal outside any session.",
        )),
        "on" => match turn_on(state) {
            Ok(()) => (shown(state), 0),
            Err(error) => refusal(Problem::new(
                code::INVALID_STATE,
                format!("Cannot turn the preview on: {error}"),
            )),
        },
        "off" => match turn_off(state) {
            Ok(()) => (shown(state), 0),
            Err(error) => refusal(Problem::new(
                code::INVALID_STATE,
                format!("Cannot turn the preview off: {error}"),
            )),
        },
        other => refusal(
            Problem::new(code::NOT_FOUND, "Usage: hgs team preview on|off|status")
                .with_refs([other]),
        ),
    }
}

fn turn_on(state: &Path) -> std::io::Result<()> {
    crate::platform::private_dir(&state.join(DIR))?;
    std::fs::write(preview_file(state), b"on\n")
}

/// Removes the marker, and the folder too when nothing else is in it.
fn turn_off(state: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(preview_file(state)) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let _ = std::fs::remove_dir(state.join(DIR));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh empty folder, removed when the value is dropped.
    struct State(PathBuf);

    impl State {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("hgs-teams-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn entries(&self) -> usize {
            std::fs::read_dir(&self.0).unwrap().count()
        }
    }

    impl Drop for State {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    fn code_of(reply: &Value) -> &str {
        reply["error"]["code"].as_str().unwrap_or("")
    }

    #[test]
    fn off_by_default_and_nothing_is_created() {
        let state = State::new("default");
        assert!(!enabled(&state.0));
        for command in [
            "status --json",
            "create --json",
            "",
            "inbox",
            "preview status",
        ] {
            let (reply, exit) = answer(&state.0, &words(command), false);
            if command == "preview status" {
                assert_eq!(
                    (reply["ok"].as_bool(), reply["preview"].as_str(), exit),
                    (Some(true), Some("off"), 0)
                );
            } else {
                assert_eq!((code_of(&reply), exit), ("teams_inactive", 1), "{command}");
            }
        }
        assert_eq!(state.entries(), 0);
    }

    #[test]
    fn a_person_turns_the_preview_on_and_off() {
        let state = State::new("toggle");
        let (reply, exit) = answer(&state.0, &words("preview on"), false);
        assert_eq!((reply["preview"].as_str(), exit), (Some("on"), 0));
        assert!(enabled(&state.0));
        let (reply, _) = answer(&state.0, &words("preview status --json"), false);
        assert_eq!(reply["preview"], "on");
        let (reply, exit) = answer(&state.0, &words("preview off"), false);
        assert_eq!((reply["preview"].as_str(), exit), (Some("off"), 0));
        assert!(!enabled(&state.0));
        assert_eq!(state.entries(), 0, "the empty folder goes with the marker");
        let (reply, exit) = answer(&state.0, &words("preview off"), false);
        assert_eq!(
            (reply["preview"].as_str(), exit),
            (Some("off"), 0),
            "off twice is still off"
        );
    }

    #[test]
    fn a_session_cannot_change_the_preview() {
        let state = State::new("session");
        let (reply, exit) = answer(&state.0, &words("preview on"), true);
        assert_eq!((code_of(&reply), exit), ("not_allowed", 1));
        assert!(!enabled(&state.0));
        assert_eq!(state.entries(), 0);
        answer(&state.0, &words("preview on"), false);
        let (reply, exit) = answer(&state.0, &words("preview off"), true);
        assert_eq!((code_of(&reply), exit), ("not_allowed", 1));
        assert!(enabled(&state.0));
        let (reply, exit) = answer(&state.0, &words("preview status"), true);
        assert_eq!(
            (reply["preview"].as_str(), exit),
            (Some("on"), 0),
            "a session may read it"
        );
    }

    #[test]
    fn turning_off_keeps_a_folder_that_holds_something() {
        let state = State::new("keep");
        answer(&state.0, &words("preview on"), false);
        std::fs::write(state.0.join(DIR).join("other"), b"x").unwrap();
        answer(&state.0, &words("preview off"), false);
        assert!(!enabled(&state.0));
        assert!(state.0.join(DIR).join("other").is_file());
    }

    #[test]
    fn an_unknown_command_is_named_once_the_preview_is_on() {
        let state = State::new("unknown");
        answer(&state.0, &words("preview on"), false);
        let (reply, exit) = answer(&state.0, &words("frobnicate --json"), false);
        assert_eq!((code_of(&reply), exit), ("not_found", 1));
        assert_eq!(reply["error"]["refs"][0], "frobnicate");
        let (reply, exit) = answer(&state.0, &words("preview maybe"), false);
        assert_eq!((code_of(&reply), exit), ("not_found", 1));
    }
}
