//! One `hgs team <op> --json` read: the request on standard input, the output
//! bounded, and the shape the projection relies on checked before a view sees
//! it.
//!
//! The desktop follows the teams of this computer only, one at a time (M1);
//! the reads are the store's pages and never a whole team.
use crate::actions::{Exit, Invocation};
use crate::reads::READ_LIMIT;
use crate::run::{run_limited, TooLarge};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;

/// Rows one read asks for: the store's own page bound.
pub const PAGE: u64 = 200;
/// The longest team id a read names.
pub const TEAM_ID_BYTES: usize = 64;
/// How long a read waits for `hgs` (it is this computer's hgs).
const TIMEOUT: Duration = Duration::from_secs(15);
/// How much of a failure or refusal is kept for the view.
const MESSAGE_CHARS: usize = 2000;

/// One read of the team store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeamRead {
    /// `team list`: the teams of this computer, newest first.
    List,
    /// `team status`: one team, its members and its counts, without a bundle.
    Status { team: String },
    /// `team queue`: one page of the team's queue and plan nodes, by rank.
    Queue { team: String, cursor: Option<Value> },
    /// `team waiting`: one page of what waits for the person, across teams.
    Waiting { before: Option<Value> },
}

impl TeamRead {
    /// The operation's word: `hgs team <op> --json`.
    pub fn op(&self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Status { .. } => "status",
            Self::Queue { .. } => "queue",
            Self::Waiting { .. } => "waiting",
        }
    }

    /// The team the read is about, if it is about one.
    pub fn team(&self) -> Option<&str> {
        match self {
            Self::Status { team } | Self::Queue { team, .. } => Some(team),
            Self::List | Self::Waiting { .. } => None,
        }
    }

    /// The JSON object the read sends on standard input.
    pub fn request(&self) -> Value {
        match self {
            Self::List => json!({"limit": PAGE}),
            Self::Status { team } => json!({"team": team}),
            Self::Queue { team, cursor } => {
                let mut request = json!({"team": team, "limit": PAGE});
                if let Some(cursor) = cursor {
                    request["cursor"] = cursor.clone();
                }
                request
            }
            Self::Waiting { before } => {
                let mut request = json!({"limit": PAGE});
                if let Some(before) = before {
                    request["before"] = before.clone();
                }
                request
            }
        }
    }

    /// The top-level keys the reply must carry, with whether each is an array.
    /// A `list` or `status` reply about one team is accepted too (a read that
    /// is about one team only), which is the shape of the shared fixture.
    fn required(&self) -> &'static [(&'static str, bool)] {
        match self {
            Self::Status { .. } => &[("team", false), ("members", true)],
            _ => &[],
        }
    }
}

/// A team id as a read sends it: nonempty, short, no option and no control or
/// space characters.
pub fn valid_team(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= TEAM_ID_BYTES
        && !id.starts_with('-')
        && !id.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// Why a read gave no view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeamError {
    /// `hgs` refused it: its stable code (`teams_inactive`, `not_found`, ...)
    /// and one sentence.
    Refused { code: String, message: String },
    /// `hgs` failed, timed out or gave no reply.
    Failed { message: String },
    /// The reply is not the JSON the read expects.
    Invalid { message: String },
    /// The reply passed the read's limit; the command was ended.
    TooLarge { limit: usize },
}

impl TeamError {
    /// The batch's failure code: a refusal's own code, else a stable one.
    pub fn code(&self) -> &str {
        match self {
            Self::Refused { code, .. } => code,
            Self::Failed { .. } => "failed",
            Self::Invalid { .. } => "invalid_reply",
            Self::TooLarge { .. } => "too_large",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Refused { message, .. }
            | Self::Failed { message }
            | Self::Invalid { message } => message,
            Self::TooLarge { .. } => "the reply passed the read's limit",
        }
    }
}

impl std::fmt::Display for TeamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for TeamError {}

fn text_of(stderr: &str) -> String {
    let text: String = stderr.trim().chars().take(MESSAGE_CHARS).collect();
    if text.is_empty() {
        "hgs gave no reply".to_owned()
    } else {
        text
    }
}

/// The refusal an `hgs` reply carries, when it carries one:
/// `{"ok": false, "error": {"code", "message"}}`.
fn refusal(stdout: &[u8]) -> Option<(String, String)> {
    let reply: Value = serde_json::from_slice(stdout).ok()?;
    if reply.get("ok").and_then(Value::as_bool) != Some(false) {
        return None;
    }
    let error = reply.get("error")?;
    let code = error
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    (!code.is_empty()).then(|| {
        (
            code.chars().take(64).collect(),
            message.chars().take(MESSAGE_CHARS).collect(),
        )
    })
}

/// The standard output of a read, or why it gave none.
fn output(exit: Exit) -> Result<Vec<u8>, TeamError> {
    match exit {
        Exit::Code {
            code: 0, stdout, ..
        } => Ok(stdout),
        Exit::Code { stdout, stderr, .. } => match refusal(&stdout) {
            Some((code, message)) => Err(TeamError::Refused { code, message }),
            None => Err(TeamError::Failed {
                message: text_of(&stderr),
            }),
        },
        Exit::Abnormal { stderr } => Err(TeamError::Failed {
            message: text_of(&stderr),
        }),
        Exit::TimedOut => Err(TeamError::Failed {
            message: "hgs did not answer in time".into(),
        }),
        Exit::StartFailed(message) => Err(TeamError::Failed {
            message: format!("hgs did not start: {message}"),
        }),
        Exit::WriteFailed => Err(TeamError::Failed {
            message: "hgs did not take the request".into(),
        }),
    }
}

/// The command of a read: `hgs team <op> --json` with the request on standard
/// input.
pub fn invocation(read: &TeamRead) -> Result<Invocation, TeamError> {
    if read.team().is_some_and(|team| !valid_team(team)) {
        return Err(TeamError::Failed {
            message: "Not a team id.".into(),
        });
    }
    Ok(Invocation {
        args: vec!["team".to_owned(), read.op().to_owned(), "--json".to_owned()],
        stdin: Some(read.request().to_string().into_bytes()),
        timeout: TIMEOUT,
        request_id: None,
    })
}

/// Checks a reply: its exit, then the shape the read relies on.
pub fn decode(read: &TeamRead, exit: Exit) -> Result<Value, TeamError> {
    let stdout = output(exit)?;
    let value: Value = serde_json::from_slice(&stdout).map_err(|error| TeamError::Invalid {
        message: error.to_string(),
    })?;
    if !value.is_object() {
        return Err(TeamError::Invalid {
            message: "not a JSON object".into(),
        });
    }
    // Every reply says whether the store took the read.
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        let (code, message) = refusal(&stdout).unwrap_or_else(|| {
            (
                "failed".to_owned(),
                "hgs refused the read without a reason".to_owned(),
            )
        });
        return Err(TeamError::Refused { code, message });
    }
    for (key, array) in read.required() {
        let fits = match value.get(key) {
            Some(found) if *array => found.is_array(),
            Some(found) => found.is_object(),
            None => false,
        };
        if !fits {
            return Err(TeamError::Invalid {
                message: format!("team {} gives no {key}", read.op()),
            });
        }
    }
    match read {
        TeamRead::Status { team } => {
            let found = &value["team"];
            if found.get("id").and_then(Value::as_str) != Some(team) {
                return Err(TeamError::Invalid {
                    message: "team status gives another team".into(),
                });
            }
        }
        TeamRead::Queue { .. } => {
            let rows = value.get("nodes").and_then(Value::as_array).or_else(|| {
                value
                    .get("page")
                    .and_then(|page| page.get("rows"))
                    .and_then(Value::as_array)
            });
            if rows.is_none() {
                return Err(TeamError::Invalid {
                    message: "team queue gives no rows".into(),
                });
            }
        }
        TeamRead::List => {
            let listed = value.get("items").and_then(Value::as_array).is_some();
            let single = value
                .get("team")
                .and_then(|team| team.get("id"))
                .and_then(Value::as_str)
                .is_some();
            if !listed && !single {
                return Err(TeamError::Invalid {
                    message: "team list gives no teams".into(),
                });
            }
        }
        TeamRead::Waiting { .. } => {
            if value.get("items").and_then(Value::as_array).is_none() {
                return Err(TeamError::Invalid {
                    message: "team waiting gives no items".into(),
                });
            }
        }
    }
    Ok(value)
}

/// Runs one read through `hgs` and checks its reply. Blocks for at most the
/// read's timeout plus a few seconds; callers run it off the UI thread.
pub fn read(hgs: &Path, read: &TeamRead) -> Result<Value, TeamError> {
    let invocation = invocation(read)?;
    let exit = run_limited(hgs, &invocation, &[], READ_LIMIT)
        .map_err(|TooLarge { cap, .. }| TeamError::TooLarge { limit: cap })?;
    decode(read, exit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/team-plan/fixtures/team-queue.json"
        )))
        .expect("the shared team fixture parses")
    }

    #[test]
    fn every_read_asks_for_one_page() {
        let list = TeamRead::List.request();
        assert_eq!(list["limit"], PAGE);
        let waiting = TeamRead::Waiting { before: None }.request();
        assert_eq!(waiting["limit"], PAGE);
        let queue = TeamRead::Queue {
            team: "t".into(),
            cursor: None,
        }
        .request();
        assert_eq!(queue["limit"], PAGE);
        assert_eq!(queue["team"], "t");
        let older = TeamRead::Queue {
            team: "t".into(),
            cursor: Some(serde_json::json!({"scope": "queue", "after": 20})),
        }
        .request();
        assert_eq!(older["cursor"]["after"], 20);
        assert_eq!(
            TeamRead::Waiting { before: None }.request().get("after"),
            None
        );
    }

    #[test]
    fn a_read_names_its_operation_and_no_host() {
        let invocation = invocation(&TeamRead::Status {
            team: "00000000-0000-4000-8000-00000000beef".into(),
        })
        .unwrap();
        assert_eq!(invocation.args, ["team", "status", "--json"]);
        assert_eq!(
            invocation.stdin,
            Some(br#"{"team":"00000000-0000-4000-8000-00000000beef"}"#.to_vec())
        );
        assert_eq!(invocation.timeout, TIMEOUT);
        assert_eq!(invocation.request_id, None);
    }

    #[test]
    fn an_unknown_team_id_is_refused() {
        for team in ["", "has space", "-option", "with\nnewline", &"x".repeat(65)] {
            assert!(!valid_team(team), "{team:?}");
            assert!(invocation(&TeamRead::Status {
                team: team.to_owned()
            })
            .is_err());
        }
        assert!(valid_team("00000000-0000-4000-8000-00000000beef"));
    }

    #[test]
    fn the_shapes_the_views_rely_on_are_checked() {
        let ok = |stdout: Value| Exit::Code {
            code: 0,
            stdout: serde_json::to_vec(&stdout).unwrap(),
            stderr: String::new(),
        };
        let team = "00000000-0000-4000-8000-00000000beef";
        let status = TeamRead::Status { team: team.into() };
        let reply = ok(serde_json::json!({
            "ok": true, "revision": 1,
            "team": {"id": team, "name": "Fixes"},
            "members": [],
        }));
        assert!(decode(&status, reply.clone()).is_ok());
        // The fixture is a status reply about one team.
        assert!(decode(&status, ok(fixture())).is_ok());
        // Another team, or no members, is not this read's reply.
        let other = ok(serde_json::json!({
            "ok": true, "team": {"id": "other"}, "members": [],
        }));
        assert!(matches!(
            decode(&status, other),
            Err(TeamError::Invalid { .. })
        ));
        let no_members = ok(serde_json::json!({"ok": true, "team": {"id": team}}));
        assert!(matches!(
            decode(&status, no_members),
            Err(TeamError::Invalid { .. })
        ));

        let queue = TeamRead::Queue {
            team: team.into(),
            cursor: None,
        };
        assert!(decode(
            &queue,
            ok(serde_json::json!({"ok": true, "page": {"rows": [], "next": null}}))
        )
        .is_ok());
        assert!(decode(&queue, ok(serde_json::json!({"ok": true}))).is_err());

        assert!(decode(&TeamRead::List, ok(fixture())).is_ok());
        assert!(decode(&TeamRead::List, ok(serde_json::json!({"ok": true}))).is_err());
        assert!(decode(
            &TeamRead::Waiting { before: None },
            ok(serde_json::json!({"ok": true, "items": []}))
        )
        .is_ok());
        assert!(decode(&TeamRead::Waiting { before: None }, ok(fixture())).is_err());
    }

    #[test]
    fn a_refusal_keeps_its_code() {
        let refused = json!({
            "ok": false,
            "error": {"code": "teams_inactive", "message": "Teams are off."},
        });
        let exit = Exit::Code {
            code: 1,
            stdout: serde_json::to_vec(&refused).unwrap(),
            stderr: String::new(),
        };
        let error = decode(&TeamRead::List, exit).unwrap_err();
        assert_eq!(error.code(), "teams_inactive");
        assert_eq!(error.message(), "Teams are off.");
    }

    #[test]
    fn a_failure_or_a_bad_reply_is_worded() {
        let failed = decode(
            &TeamRead::List,
            Exit::Code {
                code: 2,
                stdout: Vec::new(),
                stderr: " hgs is not installed\n".into(),
            },
        )
        .unwrap_err();
        assert_eq!(failed.code(), "failed");
        assert_eq!(failed.message(), "hgs is not installed");
        let timed_out = decode(&TeamRead::List, Exit::TimedOut).unwrap_err();
        assert_eq!(timed_out.code(), "failed");
        assert!(timed_out.message().contains("in time"));
        let invalid = decode(
            &TeamRead::List,
            Exit::Code {
                code: 0,
                stdout: b"not json".to_vec(),
                stderr: String::new(),
            },
        )
        .unwrap_err();
        assert_eq!(invalid.code(), "invalid_reply");
    }
}
