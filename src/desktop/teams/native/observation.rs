use super::*;
use sha2::{Digest, Sha256};
fn bytes(path: &Path, cap: u64) -> Result<Vec<u8>> {
    crate::swarm::read_regular(path, cap)
        .map_err(|_| invalid("Bounded native metadata unavailable"))
}
fn object(path: &Path) -> Result<Value> {
    serde_json::from_slice(&bytes(path, 16384)?).map_err(|_| invalid("Native metadata is invalid"))
}
pub(super) fn common_dir(source: &Path) -> Result<PathBuf> {
    let entry = source.join(".git");
    let git_dir = if entry.is_dir() {
        crate::platform::canonical(&entry).map_err(|_| invalid("Git directory unavailable"))?
    } else {
        let raw =
            String::from_utf8(bytes(&entry, 4096)?).map_err(|_| invalid("Git entry invalid"))?;
        let target = raw
            .trim()
            .strip_prefix("gitdir: ")
            .ok_or_else(|| invalid("An existing Git checkout is required"))?;
        crate::platform::canonical(source.join(target))
            .map_err(|_| invalid("Git directory unavailable"))?
    };
    let common = git_dir.join("commondir");
    let common = match std::fs::symlink_metadata(&common) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => git_dir,
        _ => {
            let raw = String::from_utf8(bytes(&common, 4096)?)
                .map_err(|_| invalid("Git common directory invalid"))?;
            crate::platform::canonical(git_dir.join(raw.trim()))
                .map_err(|_| invalid("Git common directory unavailable"))?
        }
    };
    if !common.join("objects").is_dir() {
        return Err(invalid("An existing repository is required"));
    }
    Ok(common)
}
pub(super) fn branch_exists(plan: &BranchPlan) -> Result<bool> {
    // Validate before constructing any metadata pathname. integrate performs
    // the full Git ref validation again before its compare-and-swap write.
    let b = &plan.branch;
    if b.is_empty()
        || b.starts_with('/')
        || b.split('/')
            .any(|p| p.is_empty() || p == "." || p == ".." || p.contains('\\'))
    {
        return Err(invalid("Integration branch is invalid"));
    }
    let common = Path::new(&plan.placement.git_dir);
    let reference = format!("refs/heads/{b}");
    let path = common.join(&reference);
    match std::fs::symlink_metadata(&path) {
        Ok(_) => {
            bytes(&path, 256)?;
            return Ok(true);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(_) => return Err(invalid("Integration ref is unavailable")),
    }
    let packed = common.join("packed-refs");
    match std::fs::symlink_metadata(&packed) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        _ => {
            let raw = String::from_utf8(bytes(&packed, 4 << 20)?)
                .map_err(|_| invalid("Packed refs are invalid"))?;
            Ok(raw.lines().any(|line| {
                line.split_once(' ')
                    .is_some_and(|(_, name)| name == reference)
            }))
        }
    }
}
pub(super) fn ownership(config: &Config, placement: &Placement) -> Result<Worktree> {
    let common = Path::new(&placement.git_dir);
    if common_dir(Path::new(&placement.destination))? != common {
        return Err(uncertain("Worktree repository identity changed"));
    }
    let raw = String::from_utf8(bytes(
        &Path::new(&placement.destination).join(".git"),
        4096,
    )?)
    .map_err(|_| uncertain("Worktree entry unreadable"))?;
    let metadata = raw
        .trim()
        .strip_prefix("gitdir: ")
        .ok_or_else(|| uncertain("Worktree entry is invalid"))?;
    let metadata = crate::platform::canonical(Path::new(&placement.destination).join(metadata))
        .map_err(|_| uncertain("Worktree metadata unavailable"))?;
    if !metadata.starts_with(common.join("worktrees")) {
        return Err(uncertain("Worktree metadata is outside its repository"));
    }
    let owner = object(&metadata.join("zerus-owner.json"))?;
    let hash = format!(
        "{:x}",
        Sha256::digest(format!("{}\n{}", common.display(), placement.destination).as_bytes())
    );
    let receipt = object(
        &config
            .state
            .join("worktree-cache")
            .join(format!("owned-{hash}.json")),
    )?;
    if owner != receipt
        || owner["version"] != 1
        || owner["path"] != placement.destination
        || owner["common_dir"] != placement.git_dir
        || owner["branch"] != placement.branch
        || owner["metadata"] != metadata.to_string_lossy().as_ref()
    {
        return Err(uncertain("Generated worktree ownership changed"));
    }
    let nonce = owner["id"]
        .as_str()
        .filter(|s| uuid::Uuid::parse_str(s).is_ok())
        .ok_or_else(|| uncertain("Generated worktree nonce unavailable"))?;
    Ok(Worktree {
        path: placement.destination.clone(),
        ownership_nonce: nonce.into(),
    })
}
pub(super) fn inspect(native: &Native, name: &str) -> Result<Value> {
    native.read(
        &[
            "inspect".into(),
            name.into(),
            "--after".into(),
            i64::MAX.to_string(),
            "--skip-processes".into(),
        ],
        None,
    )
}
pub(super) fn session(
    native: &Native,
    plan: &Plan,
    deadline: Instant,
) -> Result<Option<ObservedSession>> {
    let hint = arguments::hint(&plan.agent, &plan.launch_id);
    let args = [
        "inspect".into(),
        hint,
        "--after".into(),
        i64::MAX.to_string(),
        "--skip-processes".into(),
    ];
    let value = match native.read_until(&args, None, deadline) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if value["agent"] != plan.agent
        || value["launch_id"] != plan.launch_id
        || value["account_id"] != plan.account
        || value["cwd"] != plan.placement.destination
    {
        return Ok(None);
    }
    let get = |key| {
        value[key]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 1024)
            .map(str::to_owned)
            .ok_or_else(|| invalid("Session identity is incomplete"))
    };
    Ok(Some(ObservedSession {
        machine: native.machine.clone(),
        launch_id: plan.launch_id.clone(),
        agent: plan.agent.clone(),
        account: plan.account.clone(),
        worktree: ownership(&native.config, &plan.placement)?,
        parent: None,
        name: get("name")?,
        session_id: get("session_id")?,
        run_id: get("run_id")?,
        conversation_id: get("conversation_id")?,
    }))
}
