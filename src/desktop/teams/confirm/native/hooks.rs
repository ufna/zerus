//! What a standing confirmation of Codex's hook review may trust
//! (zerus-f67qp review). "Trust all and continue" trusts every new or changed
//! hook Codex found: account hooks (Zerus's own and any other), project hook
//! files and inline configuration, enabled plugin hooks. The team's trust
//! setting covers two kinds only: Zerus's own state hook exactly as this
//! installation writes it into the account's `hooks.json`, and a project hook
//! whose file is byte for byte the team's base commit's. Codex itself lists
//! the pending entries with their keys and current hashes (its app server's
//! `hooks/list` for the member's folder), and only a `hooks.state` write of
//! exactly those keys and hashes in the account's `config.toml` confirms the
//! answer. Anything else, or an inventory that cannot be read completely,
//! stays with the person.
use super::super::types::Grant;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::mpsc,
    time::{Duration, Instant},
};

/// The bound of the app server's output.
const LIMIT: u64 = 4 << 20;
/// The bound of one inventory read.
const DEADLINE: Duration = Duration::from_secs(20);

/// Zerus's own Codex hook entry: the command line and timeout this
/// installation writes for each Codex event, in the account's `hooks.json`.
pub(in crate::desktop::teams::confirm) struct Expected {
    pub hooks_json: PathBuf,
    pub command: String,
    pub timeout: u64,
    /// The events in Codex's own spelling (`sessionStart`).
    pub events: Vec<String>,
}

impl Expected {
    /// This installation's entry for the account at `home`.
    pub fn current(home: &Path) -> Result<Self, String> {
        let (command, timeout, events) = crate::state::codex_hook_definition()?;
        Ok(Self {
            hooks_json: canonical(&home.join("hooks.json")),
            command,
            timeout,
            events: events.iter().map(|event| camel(event)).collect(),
        })
    }
}

fn camel(event: &str) -> String {
    let mut chars = event.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(chars).collect()
    })
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

/// What the answer may trust for the member's `folder` with the account at
/// `home`, `main` the repository's main checkout and `base` the team's base
/// commit, with Codex's saved trust before the answer.
pub(in crate::desktop::teams::confirm) fn check(
    home: &Path,
    folder: &Path,
    main: Option<&Path>,
    base: &str,
) -> Result<(Grant, BTreeMap<String, String>), String> {
    let expected = Expected::current(home)?;
    let listed = inventory(home, folder)?;
    let grant = grant(&listed, &expected, &mut |path| committed(path, folder, main, base))?;
    Ok((grant, trusted(home)?))
}

/// After the answer: whether Codex saved exactly `grant` from `before` in the
/// account at `home`. Codex writes the trust before it leaves the panel; the
/// file gets a short, bounded moment.
pub(in crate::desktop::teams::confirm) fn confirmed(
    grant: &Grant,
    before: &BTreeMap<String, String>,
    home: &Path,
) -> Result<bool, String> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if saved(grant, before, &trusted(home)?) {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Codex's hook inventory for `folder` with the account at `home`: the
/// entries of the one `hooks/list` result for that folder. A result with an
/// error or a warning (a hook Codex skipped or could not read), for another
/// folder, or not answered in time is not an inventory.
pub(super) fn inventory(home: &Path, folder: &Path) -> Result<Vec<Value>, String> {
    let unavailable = |why: &str| format!("Codex's hook inventory is unavailable: {why}");
    let mut cmd = crate::platform::agent_command(&["codex".to_owned()])
        .map_err(|_| unavailable("Codex was not found"))?;
    cmd.args(["app-server", "--listen", "stdio://"])
        .env("CODEX_HOME", home)
        .current_dir(folder)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child =
        crate::platform::spawn_group(&mut cmd).map_err(|_| unavailable("Codex did not start"))?;
    let (tx, rx) = mpsc::channel();
    let out = child
        .stdout
        .take()
        .ok_or_else(|| unavailable("no output"))?;
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(out.take(LIMIT))
            .lines()
            .map_while(Result::ok)
        {
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                if tx.send(value).is_err() {
                    break;
                }
            }
        }
    });
    let mut stdin = child.stdin.take().ok_or_else(|| unavailable("no input"))?;
    let send = |stdin: &mut std::process::ChildStdin, v: Value| writeln!(stdin, "{v}");
    let _ = send(
        &mut stdin,
        json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"hgs_zerus","title":"HGS Zerus","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}}),
    );
    let start = Instant::now();
    let mut answer = None;
    while start.elapsed() < DEADLINE && answer.is_none() {
        let Ok(v) = rx.recv_timeout(Duration::from_millis(200)) else {
            if child.try_wait().ok().flatten().is_some() {
                break;
            }
            continue;
        };
        match v["id"].as_u64() {
            Some(1) if v.get("result").is_some() => {
                let _ = send(&mut stdin, json!({"method":"initialized"}));
                let _ = send(
                    &mut stdin,
                    json!({"id":2,"method":"hooks/list","params":{"cwds":[folder]}}),
                );
            }
            Some(1) => break,
            Some(2) => answer = Some(v),
            _ => {}
        }
    }
    crate::platform::kill_process_group(&mut child);
    let _ = child.wait();
    drop(stdin);
    let _ = reader.join();
    let answer = answer.ok_or_else(|| unavailable("Codex did not answer in time"))?;
    listed(&answer, folder)
}

/// The entries of `answer`, Codex's `hooks/list` response, for `folder`.
pub(in crate::desktop::teams::confirm) fn listed(
    answer: &Value,
    folder: &Path,
) -> Result<Vec<Value>, String> {
    let data = answer["result"]["data"]
        .as_array()
        .ok_or("Codex's hook inventory has no result")?;
    if data.len() != 1
        || canonical(Path::new(data[0]["cwd"].as_str().unwrap_or(""))) != canonical(folder)
    {
        return Err("Codex's hook inventory names another folder".into());
    }
    let entry = &data[0];
    for (key, what) in [("errors", "an error"), ("warnings", "a warning")] {
        let list = entry[key]
            .as_array()
            .ok_or("Codex's hook inventory is incomplete")?;
        if let Some(first) = list.first() {
            let text = first["message"].as_str().or(first.as_str()).unwrap_or("");
            return Err(format!(
                "Codex's hook inventory has {what}: {}",
                text.chars().take(200).collect::<String>()
            ));
        }
    }
    entry["hooks"]
        .as_array()
        .cloned()
        .ok_or_else(|| "Codex's hook inventory lists no hooks".into())
}

/// The keys and hashes the answer would trust: every entry Codex lists as
/// untrusted or modified, each Zerus's own entry or a committed project hook
/// (`committed`). Otherwise the refusal names the entries the setting does
/// not cover.
pub(in crate::desktop::teams::confirm) fn grant(
    hooks: &[Value],
    expected: &Expected,
    committed: &mut dyn FnMut(&Path) -> Result<bool, String>,
) -> Result<Grant, String> {
    let mut grant = Vec::new();
    let mut other = Vec::new();
    for hook in hooks {
        let status = hook["trustStatus"].as_str().unwrap_or("");
        if !matches!(status, "untrusted" | "modified") {
            continue;
        }
        let key = hook["key"].as_str().filter(|key| !key.is_empty());
        let hash = hook["currentHash"].as_str().filter(|hash| !hash.is_empty());
        let (Some(key), Some(hash)) = (key, hash) else {
            return Err("Codex's hook inventory lists a hook without its key or hash".into());
        };
        let covered = plain(hook)
            && match hook["source"].as_str() {
                Some("user") => zerus(hook, expected),
                Some("project") => committed(Path::new(hook["sourcePath"].as_str().unwrap_or("")))?,
                _ => false,
            };
        if covered {
            grant.push((key.to_owned(), hash.to_owned()));
        } else {
            other.push(describe(hook));
        }
    }
    if !other.is_empty() {
        let more = other.len().saturating_sub(5);
        let mut list = other.into_iter().take(5).collect::<Vec<_>>().join("; ");
        if more > 0 {
            list.push_str(&format!("; and {more} more"));
        }
        return Err(format!(
            "Codex asks to trust hooks that the team's trust setting does not cover: {list}"
        ));
    }
    Ok(grant)
}

/// An ordinary command hook: no plugin, not managed by policy.
fn plain(hook: &Value) -> bool {
    hook["handlerType"] == "command"
        && hook["pluginId"].is_null()
        && hook["isManaged"] == false
        && hook["environmentId"].is_null()
}

/// Zerus's own entry, exactly as this installation writes it.
fn zerus(hook: &Value, expected: &Expected) -> bool {
    canonical(Path::new(hook["sourcePath"].as_str().unwrap_or(""))) == expected.hooks_json
        && hook["command"] == expected.command.as_str()
        && hook["async"] != true
        && hook["timeoutSec"] == expected.timeout
        && hook["matcher"].is_null()
        && hook["statusMessage"].is_null()
        && hook["eventName"]
            .as_str()
            .is_some_and(|event| expected.events.iter().any(|e| e == event))
}

fn describe(hook: &Value) -> String {
    let text = |key: &str| hook[key].as_str().unwrap_or("").to_owned();
    let command = text("command").chars().take(80).collect::<String>();
    format!(
        "{} {} {} ({command})",
        text("source"),
        text("sourcePath"),
        text("eventName")
    )
}

/// Whether the project hook file at `path` is the team's: under `.codex` in
/// the member's worktree `folder` or in the repository's main checkout `main`
/// (Codex reads a linked worktree's hooks there), and byte for byte the file
/// at the same path in the team's base commit `base`.
pub(in crate::desktop::teams::confirm) fn committed(
    path: &Path,
    folder: &Path,
    main: Option<&Path>,
    base: &str,
) -> Result<bool, String> {
    let path = canonical(path);
    for root in std::iter::once(folder).chain(main) {
        let root = canonical(root);
        let Ok(relative) = path.strip_prefix(&root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        if !matches!(
            relative.as_str(),
            ".codex/hooks.json" | ".codex/config.toml"
        ) {
            return Ok(false);
        }
        let Ok(bytes) = std::fs::read(&path) else {
            return Ok(false);
        };
        let blob = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["-c", "core.hooksPath=/dev/null", "show"])
            .arg(format!("{base}:{relative}"))
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|_| "Git is unavailable to compare the project hooks".to_owned())?;
        return Ok(blob.status.success() && blob.stdout == bytes);
    }
    Ok(false)
}

/// Codex's saved hook trust in the account's `config.toml`: each
/// `hooks.state` key with its `trusted_hash`. A missing file has none; a file
/// that cannot be read or parsed is an error.
pub(in crate::desktop::teams::confirm) fn trusted(
    home: &Path,
) -> Result<BTreeMap<String, String>, String> {
    let text = match std::fs::read_to_string(home.join("config.toml")) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(_) => return Err("Codex's configuration cannot be read".into()),
    };
    state(&text)
}

pub(in crate::desktop::teams::confirm) fn state(
    text: &str,
) -> Result<BTreeMap<String, String>, String> {
    let document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| "Codex's configuration cannot be parsed".to_owned())?;
    let mut trusted = BTreeMap::new();
    let Some(state) = document
        .get("hooks")
        .and_then(|hooks| hooks.get("state"))
        .and_then(|state| state.as_table_like())
    else {
        return Ok(trusted);
    };
    for (key, item) in state.iter() {
        if let Some(hash) = item.get("trusted_hash").and_then(|hash| hash.as_str()) {
            trusted.insert(key.to_owned(), hash.to_owned());
        }
    }
    Ok(trusted)
}

/// Whether Codex saved exactly the reviewed trust: every granted key now has
/// its granted hash, and no other key's trust changed from `before`.
pub(in crate::desktop::teams::confirm) fn saved(
    grant: &Grant,
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> bool {
    let granted: BTreeMap<&str, &str> = grant
        .iter()
        .map(|(key, hash)| (key.as_str(), hash.as_str()))
        .collect();
    granted
        .iter()
        .all(|(key, hash)| after.get(*key).map(String::as_str) == Some(*hash))
        && after
            .iter()
            .filter(|(key, _)| !granted.contains_key(key.as_str()))
            .all(|(key, hash)| before.get(key) == Some(hash))
        && before
            .keys()
            .filter(|key| !granted.contains_key(key.as_str()))
            .all(|key| after.contains_key(key))
}

#[cfg(test)]
mod tests;
