//! Existing local machine-profile writes, bound to the exact displayed list.
//! No SSH, installation, terminal, source discovery or retry policy lives here.
use crate::actions::{Exit, Invocation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::Path, time::Duration};

pub const RESPONSE_BYTES: usize = 16 * 1024;
pub const CHANGED: &str = "Machine settings changed elsewhere. Reload the page before saving.";
pub const UNCERTAIN: &str = "Could not confirm the connection change. Refresh to check the saved connection before trying again.";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub hostname: String,
    pub user: String,
    pub port: Option<u16>,
    pub identity_file: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    Set {
        connection: Connection,
        create: bool,
    },
    Reset,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub alias: String,
    pub revision: String,
    pub change: Change,
}

fn ascii_field(value: &str, max: usize, allowed: &[u8]) -> bool {
    value.len() <= max
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || allowed.contains(&c))
}

/// Match the machine CLI's local profile fields; blanks keep SSH inheritance.
fn validate(connection: &Connection) -> Result<(), String> {
    if !ascii_field(&connection.hostname, 253, b"._:%-") {
        return Err("Hostname must be a DNS name or IPv4/IPv6 address, without spaces.".into());
    }
    if !ascii_field(&connection.user, 128, b"_.-") {
        return Err("SSH user contains unsupported characters.".into());
    }
    if connection.port == Some(0) {
        return Err("SSH port must be between 1 and 65535.".into());
    }
    let key = &connection.identity_file;
    if key.len() > 4096
        || key.chars().any(char::is_control)
        || (!key.is_empty()
            && !key.starts_with('/')
            && !key.starts_with("~/")
            && !(cfg!(windows) && (Path::new(key).is_absolute() || key.starts_with("~\\"))))
    {
        return Err("Choose an absolute SSH key path or a path starting with ~/ .".into());
    }
    Ok(())
}

/// Prepare only from this hub's original snapshot. hgs repeats the revision
/// comparison under its existing file lock; no write follows a stale snapshot.
pub fn prepare(list: &Value, request: &Request) -> Result<Invocation, String> {
    if request.alias.is_empty() || !ascii_field(&request.alias, 80, b"_.-") {
        return Err(
            "Machine alias: use letters, digits, dots, underscores and hyphens; no leading hyphen."
                .into(),
        );
    }
    if request.revision.len() != 64
        || !request
            .revision
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        || list.get("revision").and_then(Value::as_str) != Some(request.revision.as_str())
    {
        return Err(CHANGED.into());
    }
    if list.get("local").and_then(Value::as_str) == Some(request.alias.as_str()) {
        return Err("This is the local machine; SSH profiles are for remote machines.".into());
    }
    let rows = list
        .get("machines")
        .and_then(Value::as_array)
        .ok_or(CHANGED)?;
    let row = rows
        .iter()
        .find(|r| r.get("alias").and_then(Value::as_str) == Some(&request.alias));
    let (verb, connection, create) = match &request.change {
        Change::Set { connection, create } => {
            validate(connection)?;
            if *create && row.is_some() {
                return Err(
                    "This name already exists. Select the machine in the list to edit it.".into(),
                );
            }
            ("set", Some(connection), *create)
        }
        Change::Reset => {
            let original = row.ok_or(CHANGED)?;
            if original.get("inherited").and_then(Value::as_bool) != Some(true)
                || original.get("source").and_then(Value::as_str) != Some("local")
            {
                return Err(
                    "Restore the SSH settings of a machine from the shared configuration.".into(),
                );
            }
            ("reset", None, false)
        }
        Change::Remove => ("remove", None, false),
    };
    if !create && row.is_none() {
        return Err(CHANGED.into());
    }
    let mut args = vec!["machine".into(), verb.into(), request.alias.clone()];
    if let Some(connection) = connection {
        args.extend([
            "--json".into(),
            serde_json::to_string(connection).map_err(|_| CHANGED.to_owned())?,
        ]);
    }
    args.extend(["--revision".into(), request.revision.clone()]);
    if create {
        args.push("--create".into());
    }
    Ok(Invocation {
        args,
        stdin: None,
        timeout: Duration::from_secs(10),
        request_id: None,
    })
}

/// A lost/invalid reply is explicitly uncertain and never starts another call.
pub fn checked(exit: Exit, alias: &str) -> Result<(), String> {
    match exit {
        Exit::Code {
            code: 0, stdout, ..
        } => {
            if stdout.len() > RESPONSE_BYTES {
                return Err(UNCERTAIN.into());
            }
            let reply: Value = serde_json::from_slice(&stdout).map_err(|_| UNCERTAIN.to_owned())?;
            if reply.get("ok").and_then(Value::as_bool) == Some(true)
                && reply.get("alias").and_then(Value::as_str) == Some(alias)
                && reply.get("dry_run").and_then(Value::as_bool) == Some(false)
            {
                Ok(())
            } else {
                Err(UNCERTAIN.into())
            }
        }
        Exit::Code { stderr, .. } => {
            let detail: String = stderr.trim().chars().take(2000).collect();
            Err(if detail.is_empty() {
                UNCERTAIN.into()
            } else {
                detail
            })
        }
        Exit::StartFailed(_) => Err("Could not start HGS. The connection was not changed.".into()),
        Exit::Abnormal { .. } | Exit::TimedOut | Exit::WriteFailed => Err(UNCERTAIN.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn list() -> Value {
        json!({"local":"fixture-local","revision":"a".repeat(64),"machines":[{"alias":"fixture-peer","source":"local","inherited":true}]})
    }
    fn request(change: Change) -> Request {
        Request {
            alias: "fixture-peer".into(),
            revision: "a".repeat(64),
            change,
        }
    }
    fn connection() -> Connection {
        Connection {
            hostname: "peer.invalid".into(),
            user: String::new(),
            port: None,
            identity_file: String::new(),
            enabled: false,
        }
    }
    #[test]
    fn uses_original_revision_and_exact_local_args_for_all_three_writes() {
        for change in [
            Change::Set {
                connection: connection(),
                create: false,
            },
            Change::Reset,
            Change::Remove,
        ] {
            let call = prepare(&list(), &request(change)).unwrap();
            assert_eq!(&call.args[..1], ["machine"]);
            assert_eq!(call.args[2], "fixture-peer");
            assert_eq!(
                &call.args[call.args.len() - 2..],
                ["--revision", "a".repeat(64).as_str()]
            );
            assert!(call.stdin.is_none());
        }
        let mut req = request(Change::Set {
            connection: connection(),
            create: true,
        });
        assert!(prepare(&list(), &req).is_err());
        req.alias = "new-peer".into();
        assert_eq!(
            prepare(&list(), &req).unwrap().args.last().unwrap(),
            "--create"
        );
    }
    #[test]
    fn stale_local_missing_and_unresettable_rows_cannot_write() {
        let mut req = request(Change::Remove);
        req.revision = "b".repeat(64);
        assert_eq!(prepare(&list(), &req), Err(CHANGED.into()));
        req.revision = "a".repeat(64);
        for alias in [
            "fixture-local",
            "absent",
            "-oProxyCommand=x",
            "@peer",
            "peer\nnext",
        ] {
            req.alias = alias.into();
            assert!(prepare(&list(), &req).is_err());
        }
        let mut snapshot = list();
        snapshot["machines"][0]["inherited"] = json!(false);
        assert!(prepare(&snapshot, &request(Change::Reset)).is_err());
    }
    #[test]
    fn invalid_fields_never_reach_the_cli_and_blanks_preserve_inheritance() {
        let good = connection();
        assert!(validate(&good).is_ok());
        for bad in [
            Connection {
                hostname: "-oProxyCommand=x".into(),
                ..good.clone()
            },
            Connection {
                user: "user name".into(),
                ..good.clone()
            },
            Connection {
                port: Some(0),
                ..good.clone()
            },
            Connection {
                identity_file: "relative/key".into(),
                ..good.clone()
            },
            Connection {
                identity_file: "/tmp/key\nnext".into(),
                ..good.clone()
            },
        ] {
            assert!(validate(&bad).is_err());
        }
    }
    #[test]
    fn only_an_exact_positive_non_dry_reply_settles_and_unknown_never_replays() {
        let exit = |v: Value| Exit::Code {
            code: 0,
            stdout: serde_json::to_vec(&v).unwrap(),
            stderr: String::new(),
        };
        assert_eq!(
            checked(
                exit(json!({"ok":true,"alias":"fixture-peer","dry_run":false})),
                "fixture-peer"
            ),
            Ok(())
        );
        for value in [
            json!({"ok":true,"alias":"other","dry_run":false}),
            json!({"ok":true,"alias":"fixture-peer","dry_run":true}),
            json!({}),
        ] {
            assert_eq!(checked(exit(value), "fixture-peer"), Err(UNCERTAIN.into()));
        }
        assert_eq!(
            checked(Exit::TimedOut, "fixture-peer"),
            Err(UNCERTAIN.into())
        );
    }
}
