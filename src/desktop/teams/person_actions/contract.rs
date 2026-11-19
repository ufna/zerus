//! The Person steps of the real-agent Teams contracts (zerus-63rru.182), for
//! the hosted workflow and for private local runs: test-only, absent from
//! every `cargo build` executable, and run only as an ignored test of the
//! unit-test executable. Each step is the native Person action the window
//! would confirm (`models::team_action`, `models::intake`); only the window's
//! proof is modeled. It acts only on a private fixture root that the contract
//! harness created and marked, never on an account's own Zerus state.
use super::models;
use crate::{config::Config, platform};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use zerus_client_core::desktop::team_person_actions as wire;

/// Printed with every answer, and checked absent from shipped executables
/// (`scripts/ci/check-no-person-fixture.py`).
pub(crate) const MARKER: &str = "zerus-teams-contract-person-fixture/v1";
/// The file the contract harness writes into its private root.
const ROOT_FILE: &str = ".zerus-teams-contract-root";
/// The bound of one step file.
const STEP_LIMIT: u64 = 1 << 20;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    root: PathBuf,
    nonce: String,
    action: Action,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    /// A team action as the window builds it (Create, Stop).
    Team { plan: wire::Plan },
    /// A task sent to the team as the window builds it.
    Task { draft: wire::TaskDraft },
}

/// The run's one step: `ZERUS_TEAMS_CONTRACT_STEP` names its file. The answer
/// is one line, the marker and a JSON value; a refused step also fails.
#[test]
#[ignore = "run by tests/teams_native_fixture.py for its Person steps (zerus-63rru.182)"]
fn contract_person_step() {
    let path = std::env::var_os("ZERUS_TEAMS_CONTRACT_STEP").expect("ZERUS_TEAMS_CONTRACT_STEP");
    let answer = Config::load()
        .map_err(|error| error.to_string())
        .and_then(|config| {
            let home = platform::home_dir().ok_or("HOME is not set")?;
            step(Path::new(&path), &config, &home, &account_home()?)
        });
    match answer {
        Ok(reply) => println!("{MARKER} {}", json!({ "reply": reply })),
        Err(why) => {
            println!("{MARKER} {}", json!({ "refused": why }));
            panic!("the contract Person step was refused: {why}");
        }
    }
}

/// One step for `config`, after the root is proven to be the harness's.
fn step(path: &Path, config: &Config, home: &Path, person_home: &Path) -> Result<Value, String> {
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(STEP_LIMIT + 1).read_to_string(&mut text))
        .map_err(|_| "The step file cannot be read".to_owned())?;
    if text.len() as u64 > STEP_LIMIT {
        return Err("The step file exceeds its bound".into());
    }
    let step: Step = serde_json::from_str(&text).map_err(|e| format!("Invalid step: {e}"))?;
    admit(&step.root, &step.nonce, config, home, person_home)?;
    match step.action {
        Action::Team { plan } => models::team_action(config, &plan),
        Action::Task { draft } => models::intake(config, draft).map(|value| value["reply"].clone()),
    }
    .map_err(|problem| problem.to_string())
}

/// The account's own home, from the account database, never `HOME`.
fn account_home() -> Result<PathBuf, String> {
    let mut bytes = vec![0u8; 64 * 1024];
    let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut found = std::ptr::null_mut();
    // SAFETY: the buffers are caller-owned and outlive the call; the entry is
    // read only when the call reports it found one.
    let home = unsafe {
        if libc::getpwuid_r(
            libc::geteuid(),
            entry.as_mut_ptr(),
            bytes.as_mut_ptr().cast(),
            bytes.len(),
            &mut found,
        ) != 0
            || found.is_null()
        {
            return Err("The account's own home is unknown".into());
        }
        let entry = entry.assume_init();
        if entry.pw_dir.is_null() {
            return Err("The account's own home is unknown".into());
        }
        std::ffi::CStr::from_ptr(entry.pw_dir).to_owned()
    };
    use std::os::unix::ffi::OsStringExt;
    Ok(PathBuf::from(std::ffi::OsString::from_vec(
        home.into_bytes(),
    )))
}

/// Admits only the private root the contract harness created: a private
/// folder of this account, holding the harness's marker with this step's
/// nonce, around the state, configuration and home `config` uses; never one
/// that holds the account's own home or lies in its default Zerus folders.
fn admit(
    root: &Path,
    nonce: &str,
    config: &Config,
    home: &Path,
    person_home: &Path,
) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let canonical = |path: &Path| {
        platform::canonical(path).map_err(|_| format!("{} is missing", path.display()))
    };
    if !root.is_absolute() || canonical(root)? != root {
        return Err("The root is not a canonical absolute folder".into());
    }
    let uid = unsafe { libc::geteuid() };
    let folder = std::fs::symlink_metadata(root).map_err(|_| "The root is missing")?;
    if !folder.is_dir() || folder.uid() != uid || folder.permissions().mode() & 0o077 != 0 {
        return Err("The root is not a private folder of this account".into());
    }
    if nonce.len() < 16
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("The step's nonce is not the harness's".into());
    }
    let marker = root.join(ROOT_FILE);
    let file = std::fs::symlink_metadata(&marker).map_err(|_| "The root has no harness marker")?;
    if !file.file_type().is_file() || file.uid() != uid || file.len() > 4096 {
        return Err("The root's marker is not the harness's file".into());
    }
    if std::fs::read_to_string(&marker).ok().as_deref() != Some(&format!("{MARKER}\n{nonce}\n")) {
        return Err("The root's marker does not name this step".into());
    }
    let person_home = canonical(person_home)?;
    if person_home.starts_with(root) {
        return Err("The root holds the account's own home".into());
    }
    for default in [
        person_home.join(".local/state/hgs"),
        person_home.join(".config/hgs"),
    ] {
        if root.starts_with(&default) {
            return Err("The root lies in the account's own Zerus folders".into());
        }
    }
    for (what, path) in [
        ("state", config.state.as_path()),
        ("configuration", config.dir.as_path()),
        ("home", home),
    ] {
        let path = canonical(path)?;
        if path == root || !path.starts_with(root) {
            return Err(format!("The {what} folder is not inside the root"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
