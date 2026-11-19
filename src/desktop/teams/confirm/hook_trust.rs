//! Codex hook trust under a team's trust setting (zerus-f67qp, review round 2).
//! Zerus never answers Codex's "Hooks need review" panel: "Trust all and
//! continue" trusts whatever inventory the panel captured. Instead, just before
//! the service starts or resumes a team member's Codex, Zerus brings its own
//! hook entries in the member's account up to date (the same writer a session
//! start uses), asks that account's Codex for its pending inventory (app server
//! `hooks/list` for the member's folder, each hook's key and current hash), and
//! records trust for exactly the covered entries through Codex's own
//! `config/batchWrite` of `hooks.state` (what the panel itself writes), at the
//! user configuration's version it just read. Covered: Zerus's own state hook,
//! its matcher group in the account's `hooks.json` exactly as this installation
//! writes it; and a project hook whose `.codex` file is byte for byte the team's
//! base commit's. The write is verified: those keys now hold those hashes,
//! nothing else in the configuration changed, and Codex lists them as trusted.
//! Codex then starts without a panel for them; any panel that still appears is
//! the person's.
use super::types::Grant;
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

/// The bound of the app server's output.
const LIMIT: u64 = 4 << 20;
/// The bound of one app-server session.
const DEADLINE: Duration = Duration::from_secs(20);
/// The bound of the account's configuration read for the comparison.
const CONFIG_LIMIT: u64 = 1 << 20;

/// Zerus's own Codex hook entry: the matcher group this installation writes
/// for each Codex event, in the account's `hooks.json`.
pub(in crate::desktop::teams) struct Expected {
    pub hooks_json: PathBuf,
    pub command: String,
    pub timeout: u64,
    /// The events in Codex's own spelling (`sessionStart`).
    pub events: Vec<String>,
    /// The exact matcher group, every field Codex hashes.
    pub group: Value,
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
            group: crate::state::codex_hook_group()?,
        })
    }
}

fn camel(event: &str) -> String {
    let mut chars = event.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(chars).collect()
    })
}

fn pascal(event: &str) -> String {
    let mut chars = event.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

/// One app-server session of the member's account, in the member's folder.
struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: mpsc::Receiver<Value>,
    reader: Option<std::thread::JoinHandle<()>>,
    next: u64,
    deadline: Instant,
}

impl Session {
    fn open(home: &Path, folder: &Path) -> Result<Self, String> {
        let unavailable = |why: &str| format!("Codex's hook inventory is unavailable: {why}");
        let mut cmd = crate::platform::agent_command(&["codex".to_owned()])
            .map_err(|_| unavailable("Codex was not found"))?;
        cmd.args(["app-server", "--listen", "stdio://"])
            .env("CODEX_HOME", home)
            .current_dir(folder)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = crate::platform::spawn_group(&mut cmd)
            .map_err(|_| unavailable("Codex did not start"))?;
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
        let stdin = child.stdin.take();
        let mut session = Self {
            child,
            stdin,
            rx,
            reader: Some(reader),
            next: 1,
            deadline: Instant::now() + DEADLINE,
        };
        session.request(
            "initialize",
            json!({"clientInfo":{"name":"hgs_zerus","title":"HGS Zerus","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}),
        )?;
        session.notify("initialized")?;
        Ok(session)
    }

    fn notify(&mut self, method: &str) -> Result<(), String> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or("Codex's app server closed its input")?;
        writeln!(stdin, "{}", json!({ "method": method }))
            .map_err(|_| "Codex's app server closed its input".to_owned())
    }

    /// One request and its own answer, within the session's bound.
    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next;
        self.next += 1;
        let stdin = self
            .stdin
            .as_mut()
            .ok_or("Codex's app server closed its input")?;
        writeln!(
            stdin,
            "{}",
            json!({ "id": id, "method": method, "params": params })
        )
        .map_err(|_| "Codex's app server closed its input".to_owned())?;
        loop {
            let left = self.deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(format!("Codex did not answer {method} in time"));
            }
            match self.rx.recv_timeout(left.min(Duration::from_millis(200))) {
                Ok(v) if v["id"] == id => {
                    return match v.get("result") {
                        Some(result) => Ok(result.clone()),
                        None => Err(format!(
                            "Codex refused {method}: {}",
                            v["error"]["message"].as_str().unwrap_or("no reason")
                        )),
                    };
                }
                Ok(_) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if self.child.try_wait().ok().flatten().is_some() {
                        return Err(format!(
                            "Codex's app server ended before it answered {method}"
                        ));
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(format!(
                        "Codex's app server ended before it answered {method}"
                    ))
                }
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        drop(self.stdin.take());
        crate::platform::kill_process_group(&mut self.child);
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

/// What the member's Codex would ask about at its next start, read before it
/// starts: the covered entries with their keys and hashes, and the others,
/// named.
pub(in crate::desktop::teams) struct Prepared {
    session: Session,
    home: PathBuf,
    folder: PathBuf,
    /// The user configuration's version Codex reported, for the write.
    version: String,
    /// The user configuration as it was, for the comparison.
    before: Value,
    pub grant: Grant,
    pub uncovered: Vec<String>,
}

/// Reads the member's pending inventory and classifies it. An inventory with
/// an error, a warning other than Codex's clamp of Zerus's own timeouts, or no
/// answer, is not read: nothing is trusted.
pub(in crate::desktop::teams) fn prepare(
    home: &Path,
    folder: &Path,
    main: Option<&Path>,
    base: &str,
) -> Result<Prepared, String> {
    let expected = Expected::current(home)?;
    let mut session = Session::open(home, folder)?;
    // The account's file Codex hashed is the one compared: the same before
    // and after Codex lists it.
    let account = read_bounded(&expected.hooks_json)?;
    let answer = session.request("hooks/list", json!({ "cwds": [folder] }))?;
    if read_bounded(&expected.hooks_json)? != account {
        return Err("The account's hooks.json changed while Codex listed it".into());
    }
    let (hooks, warnings) = listed(&answer, folder)?;
    benign(&warnings, &expected)?;
    let account: Value = if account.is_empty() {
        json!({})
    } else {
        serde_json::from_str(&account).map_err(|_| "The account's hooks.json cannot be parsed")?
    };
    let (grant, uncovered) = classify(&hooks, &expected, &account, &mut |path| {
        committed(path, folder, main, base)
    })?;
    let read = session.request("config/read", json!({ "includeLayers": true }))?;
    let version = user_version(&read)?;
    let before = config(home)?;
    Ok(Prepared {
        session,
        home: home.to_owned(),
        folder: folder.to_owned(),
        version,
        before,
        grant,
        uncovered,
    })
}

impl Prepared {
    /// Records trust for exactly the granted keys and hashes through Codex's
    /// own configuration write, then verifies it. `Ok(true)` only when the
    /// configuration changed in exactly those entries and Codex lists each as
    /// trusted at its hash.
    pub fn write(mut self) -> Result<bool, String> {
        if self.grant.is_empty() {
            return Ok(true);
        }
        let value: serde_json::Map<String, Value> = self
            .grant
            .iter()
            .map(|(key, hash)| (key.clone(), json!({ "trusted_hash": hash })))
            .collect();
        self.session.request(
            "config/batchWrite",
            json!({"edits":[{"keyPath":"hooks.state","value":value,"mergeStrategy":"upsert"}],
                "expectedVersion":self.version,"reloadUserConfig":false}),
        )?;
        let after = config(&self.home)?;
        if !unchanged_except(&self.before, &after, &self.grant) {
            return Ok(false);
        }
        let answer = self
            .session
            .request("hooks/list", json!({ "cwds": [self.folder] }))?;
        let (hooks, _) = listed(&answer, &self.folder)?;
        Ok(self.grant.iter().all(|(key, hash)| {
            hooks.iter().any(|hook| {
                hook["key"] == key.as_str()
                    && hook["currentHash"] == hash.as_str()
                    && hook["trustStatus"] == "trusted"
            })
        }))
    }
}

fn read_bounded(path: &Path) -> Result<String, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(_) => return Err(format!("{} cannot be read", path.display())),
    };
    let mut text = String::new();
    file.take(CONFIG_LIMIT + 1)
        .read_to_string(&mut text)
        .map_err(|_| format!("{} cannot be read", path.display()))?;
    if text.len() as u64 > CONFIG_LIMIT {
        return Err(format!("{} exceeds its bound", path.display()));
    }
    Ok(text)
}

/// The user configuration's version in Codex's `config/read` answer.
pub(in crate::desktop::teams) fn user_version(read: &Value) -> Result<String, String> {
    read["layers"]
        .as_array()
        .and_then(|layers| {
            layers
                .iter()
                .find(|layer| layer["name"]["type"] == "user")
                .and_then(|layer| layer["version"].as_str())
        })
        .map(str::to_owned)
        .ok_or_else(|| "Codex's configuration has no user layer version".into())
}

/// The account's `config.toml` as a value, an absent file as an empty table.
fn config(home: &Path) -> Result<Value, String> {
    toml_json(&read_bounded(&home.join("config.toml"))?)
}

/// A TOML document as a JSON value, to compare two readings of it whatever
/// its formatting.
pub(in crate::desktop::teams) fn toml_json(text: &str) -> Result<Value, String> {
    let document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| "Codex's configuration cannot be parsed".to_owned())?;
    Ok(table(document.as_table()))
}

fn table(table: &dyn toml_edit::TableLike) -> Value {
    Value::Object(
        table
            .iter()
            .map(|(key, item)| (key.to_owned(), item_json(item)))
            .collect(),
    )
}

fn item_json(item: &toml_edit::Item) -> Value {
    match item {
        toml_edit::Item::None => Value::Null,
        toml_edit::Item::Value(value) => value_json(value),
        toml_edit::Item::Table(t) => table(t),
        toml_edit::Item::ArrayOfTables(array) => {
            Value::Array(array.iter().map(|t| table(t)).collect())
        }
    }
}

fn value_json(value: &toml_edit::Value) -> Value {
    match value {
        toml_edit::Value::String(s) => json!(s.value()),
        toml_edit::Value::Integer(i) => json!(i.value()),
        toml_edit::Value::Float(f) => json!(f.value().to_string()),
        toml_edit::Value::Boolean(b) => json!(b.value()),
        toml_edit::Value::Datetime(d) => json!(d.value().to_string()),
        toml_edit::Value::Array(array) => Value::Array(array.iter().map(value_json).collect()),
        toml_edit::Value::InlineTable(t) => table(t),
    }
}

/// Whether `after` is `before` with only the granted keys' `trusted_hash`
/// set to their hashes: every other value, including the rest of each granted
/// key's own entry, is unchanged.
pub(in crate::desktop::teams) fn unchanged_except(
    before: &Value,
    after: &Value,
    grant: &Grant,
) -> bool {
    if !grant
        .iter()
        .all(|(key, hash)| after["hooks"]["state"][key.as_str()]["trusted_hash"] == hash.as_str())
    {
        return false;
    }
    let strip = |config: &Value| {
        let mut config = config.clone();
        if let Some(state) = config
            .get_mut("hooks")
            .and_then(|hooks| hooks.get_mut("state"))
            .and_then(Value::as_object_mut)
        {
            for (key, _) in grant {
                if let Some(entry) = state.get_mut(key).and_then(Value::as_object_mut) {
                    entry.remove("trusted_hash");
                    if entry.is_empty() {
                        state.remove(key);
                    }
                }
            }
        }
        if let Some(hooks) = config.get_mut("hooks").and_then(Value::as_object_mut) {
            if hooks
                .get("state")
                .and_then(Value::as_object)
                .is_some_and(|s| s.is_empty())
            {
                hooks.remove("state");
            }
        }
        if let Some(root) = config.as_object_mut() {
            if root
                .get("hooks")
                .and_then(Value::as_object)
                .is_some_and(|h| h.is_empty())
            {
                root.remove("hooks");
            }
        }
        config
    };
    strip(before) == strip(after)
}

/// The entries of `answer`, Codex's `hooks/list` result, for `folder`, and
/// its warnings. A result with an error, for another folder, or without its
/// lists is not an inventory.
pub(in crate::desktop::teams) fn listed(
    answer: &Value,
    folder: &Path,
) -> Result<(Vec<Value>, Vec<String>), String> {
    let data = answer["data"]
        .as_array()
        .or_else(|| answer["result"]["data"].as_array())
        .ok_or("Codex's hook inventory has no result")?;
    if data.len() != 1
        || canonical(Path::new(data[0]["cwd"].as_str().unwrap_or(""))) != canonical(folder)
    {
        return Err("Codex's hook inventory names another folder".into());
    }
    let entry = &data[0];
    let errors = entry["errors"]
        .as_array()
        .ok_or("Codex's hook inventory is incomplete")?;
    if let Some(first) = errors.first() {
        let text = first["message"].as_str().unwrap_or("");
        return Err(format!(
            "Codex's hook inventory has an error: {}",
            text.chars().take(200).collect::<String>()
        ));
    }
    let warnings = entry["warnings"]
        .as_array()
        .ok_or("Codex's hook inventory is incomplete")?
        .iter()
        .map(|warning| warning.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or("Codex's hook inventory is incomplete")?;
    let hooks = entry["hooks"]
        .as_array()
        .cloned()
        .ok_or("Codex's hook inventory lists no hooks")?;
    Ok((hooks, warnings))
}

/// Codex 0.170.0 caps SessionEnd and Interrupt hook timeouts at three seconds
/// (`normalize_command_hook`) and warns that it did; Zerus writes fifteen for
/// every event, so its own `hooks.json` always gets those two warnings, with
/// the hooks still loaded. Any other warning, such as a hook Codex skipped,
/// means the inventory is not complete.
pub(in crate::desktop::teams) fn benign(
    warnings: &[String],
    expected: &Expected,
) -> Result<(), String> {
    for warning in warnings {
        let clamped = ["SessionEnd", "Interrupt"].iter().any(|event| {
            warning
                .strip_prefix(&format!("clamping {event} hook timeout to {CLAMP}s in "))
                .is_some_and(|path| canonical(Path::new(path)) == expected.hooks_json)
        });
        if !clamped {
            return Err(format!(
                "Codex's hook inventory has a warning: {}",
                warning.chars().take(200).collect::<String>()
            ));
        }
    }
    Ok(())
}

/// Codex's cap of SessionEnd and Interrupt hook timeouts, in seconds.
const CLAMP: u64 = 3;

/// The timeout Codex lists for Zerus's own entry of `event`: what Zerus
/// writes, as Codex normalizes it.
fn timeout(event: &str, written: u64) -> u64 {
    if matches!(event, "sessionEnd" | "interrupt") {
        written.clamp(1, CLAMP)
    } else {
        written.max(1)
    }
}

/// The pending entries Codex lists as untrusted or modified: the covered
/// ones, each Zerus's own entry in `account` (the account's `hooks.json`) or a
/// committed project hook (`committed`), with Codex's key and current hash;
/// and the others, named.
pub(in crate::desktop::teams) fn classify(
    hooks: &[Value],
    expected: &Expected,
    account: &Value,
    committed: &mut dyn FnMut(&Path) -> Result<bool, String>,
) -> Result<(Grant, Vec<String>), String> {
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
                Some("user") => zerus(hook, expected, account),
                Some("project") => committed(Path::new(hook["sourcePath"].as_str().unwrap_or("")))?,
                _ => false,
            };
        if covered {
            grant.push((key.to_owned(), hash.to_owned()));
        } else {
            other.push(describe(hook));
        }
    }
    Ok((grant, other))
}

/// The person's notice of what Codex asks about that Zerus did not trust.
pub(in crate::desktop::teams) fn named(uncovered: &[String]) -> String {
    let more = uncovered.len().saturating_sub(5);
    let mut list = uncovered
        .iter()
        .take(5)
        .cloned()
        .collect::<Vec<_>>()
        .join("; ");
    if more > 0 {
        list.push_str(&format!("; and {more} more"));
    }
    list
}

/// The person's card for a hooks panel that still appears: what the member's
/// Codex lists that Zerus did not trust, read again now.
pub(in crate::desktop::teams) fn listing(home: Option<&Path>, folder: &str, base: &str) -> String {
    let main = super::policy::main_checkout(folder);
    let prepared = home
        .ok_or_else(|| "The member's Codex account is unknown".to_owned())
        .and_then(|home| {
            prepare(
                home,
                Path::new(folder),
                main.as_deref().map(Path::new),
                base,
            )
        });
    match prepared {
        Ok(prepared) if !prepared.uncovered.is_empty() => format!(
            "Codex asks you to review hooks it lists: {}",
            named(&prepared.uncovered)
        ),
        Ok(prepared) if prepared.grant.is_empty() => {
            "Codex asks you to review hooks; it now lists none as new or changed".into()
        }
        Ok(prepared) => format!(
            "Codex asks you to review {} hooks, each Zerus's own or the team's committed one, whose trust Zerus could not record before Codex started",
            prepared.grant.len()
        ),
        Err(why) => format!("Codex asks you to review its hooks; their list is unavailable: {why}"),
    }
}

/// An ordinary command hook: no plugin, not managed by policy, no context
/// limit of its own.
fn plain(hook: &Value) -> bool {
    hook["handlerType"] == "command"
        && hook["pluginId"].is_null()
        && hook["isManaged"] == false
        && hook["environmentId"].is_null()
}

/// Zerus's own entry, exactly as this installation writes it: Codex's listing
/// agrees, and the matcher group Codex read it from in the account's
/// `hooks.json` is this installation's group, every field Codex hashes.
fn zerus(hook: &Value, expected: &Expected, account: &Value) -> bool {
    let event = hook["eventName"].as_str().unwrap_or("");
    let listed = canonical(Path::new(hook["sourcePath"].as_str().unwrap_or("")))
        == expected.hooks_json
        && hook["command"] == expected.command.as_str()
        && hook["async"] != true
        && hook["timeoutSec"] == timeout(event, expected.timeout)
        && hook["matcher"].is_null()
        && hook["statusMessage"].is_null()
        && hook["additionalContextLimit"].is_null()
        && expected.events.iter().any(|e| e == event);
    // The key is `<path>:<event>:<group>:<handler>`; Zerus writes one handler
    // per group.
    let key = hook["key"].as_str().unwrap_or("");
    let mut parts = key.rsplitn(3, ':');
    let (handler, group) = (parts.next(), parts.next());
    let group = match (handler, group.and_then(|g| g.parse::<usize>().ok())) {
        (Some("0"), Some(group)) => group,
        _ => return false,
    };
    listed && account["hooks"][pascal(event)][group] == expected.group
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
pub(in crate::desktop::teams) fn committed(
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

/// The keys and hashes as one value for the journal and its identity.
pub(in crate::desktop::teams) fn grant_json(grant: &Grant) -> Value {
    Value::Array(
        grant
            .iter()
            .map(|(key, hash)| json!({ "key": key, "hash": hash }))
            .collect(),
    )
}

/// A team member whose Codex the service is about to start or resume.
pub(in crate::desktop::teams) struct Start<'a> {
    pub team: &'a str,
    pub member: &'a str,
    pub person: &'a str,
    pub machine: &'a str,
    /// The member's launch: the feed line's member binding.
    pub launch: &'a str,
    /// The session's name, for the approval journal.
    pub session: &'a str,
    /// The run the journal row names: the run being resumed, or the launch.
    pub run: &'a str,
    pub conversation: &'a str,
    /// The member's worktree, which Zerus created for the team.
    pub folder: &'a str,
    /// The team's base commit.
    pub base: &'a str,
    /// The member's Codex account.
    pub home: &'a Path,
}

/// What the pre-trust did.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::desktop::teams) enum Pretrust {
    /// Nothing pending was covered: Codex asks the person, if at all.
    Nothing,
    /// Codex now trusts exactly these entries.
    Trusted(usize),
    /// The write's outcome was not confirmed; it is never repeated.
    Unknown(String),
    /// The journal already holds this exact grant: never repeated.
    Repeated,
    /// Nothing was written.
    Refused(String),
}

/// Just before the service starts or resumes a team member's Codex, under
/// the team's trust setting (the caller checked the setting, the member's
/// binding and the worktree's ownership): Zerus's own entries are brought up
/// to date in the account, the covered pending entries are trusted through
/// Codex's own write, journaled in the approval journal before the write, and
/// a confirmed write owes one feed line.
pub(in crate::desktop::teams) fn before_start(
    config: &crate::config::Config,
    start: &Start,
) -> Pretrust {
    if let Err(why) = crate::state::install_codex_hooks(start.home, "codex") {
        return Pretrust::Refused(why);
    }
    let main = super::policy::main_checkout(start.folder);
    let prepared = match prepare(
        start.home,
        Path::new(start.folder),
        main.as_deref().map(Path::new),
        start.base,
    ) {
        Ok(prepared) => prepared,
        Err(why) => return Pretrust::Refused(why),
    };
    if prepared.grant.is_empty() {
        return Pretrust::Nothing;
    }
    let count = prepared.grant.len();
    let question = journal_question(start, &prepared.grant);
    let payload = json!({"host":start.machine,"session":start.session,"provider":"codex","question":question});
    let audit = (|| {
        let mut db = crate::approval_log::open(&config.state.join("approval-audit"))
            .map_err(|e| e.to_string())?;
        let reserved = crate::approval_log::begin_team(
            &mut db,
            &payload,
            crate::approval_log::TeamKind::HookTrust,
        )
        .map_err(|e| e.to_string())?;
        if reserved["eligible"] != true {
            return Err("The team audit refused the hook trust".to_owned());
        }
        Ok((db, reserved))
    })();
    let (db, reserved) = match audit {
        Ok(audit) => audit,
        Err(why) => return Pretrust::Refused(why),
    };
    if reserved["fresh"] != true {
        return Pretrust::Repeated;
    }
    let outcome = prepared.write();
    let (status, detail) = match &outcome {
        Ok(true) => ("approved", SAVED.to_owned()),
        Ok(false) => ("unknown", NOT_SAVED.to_owned()),
        Err(why) => ("unknown", why.clone()),
    };
    let _ = crate::approval_log::finish(
        &db,
        &json!({"id":reserved["id"],"request_id":reserved["request_id"],"status":status,"detail":detail}),
    );
    if status != "approved" {
        return Pretrust::Unknown(detail);
    }
    if let Err(why) = owe_line(
        config,
        start,
        reserved["request_id"].as_str().unwrap_or(""),
        count,
    ) {
        return Pretrust::Unknown(why);
    }
    Pretrust::Trusted(count)
}

/// The one feed line a confirmed pre-trust owes the member's launch, keyed by
/// its journal request; the confirmation engine writes it.
pub(in crate::desktop::teams) fn owe_line(
    config: &crate::config::Config,
    start: &Start,
    request: &str,
    count: usize,
) -> Result<(), String> {
    let text = format!(
        "Zerus trusted {count} of its own and the team's committed Codex hooks for member {} before its agent started.",
        start.member
    );
    super::ledger::Ledger::open(&config.state)?.owe_note(
        &format!("hooks:{request}"),
        start.team,
        start.member,
        start.launch,
        start.person,
        &text,
    )
}

/// The journal's outcome of a confirmed write, and of a write whose saved
/// trust is not exactly the grant.
pub(in crate::desktop::teams) const SAVED: &str = "Codex saved trust for exactly these hooks";
pub(in crate::desktop::teams) const NOT_SAVED: &str =
    "Codex's saved trust is not exactly these hooks";

/// The approval journal's question for a grant: the trusted keys and hashes,
/// with no native answer (Zerus wrote the trust through Codex's own request).
pub(in crate::desktop::teams) fn journal_question(start: &Start, grant: &Grant) -> Value {
    use sha2::{Digest, Sha256};
    let hooks = grant_json(grant);
    let hash = format!(
        "{:x}",
        Sha256::digest(json!([start.launch, start.run, hooks]).to_string())
    );
    json!({"source":"team_hook_trust","question_id":format!("hooks:{}:{}", start.launch, &hash[..16]),
        "question_hash":hash,"run_id":start.run,"conversation_id":start.conversation,"can_answer":false,
        "hooks":hooks,"questions":[{"id":"hooks_trust","header":"Hook trust",
        "question":"Trust Zerus's own and the team's committed Codex hooks",
        "body":format!("Hooks to trust: {}", grant.len()),"options":[]}]})
}

#[cfg(test)]
mod tests;
