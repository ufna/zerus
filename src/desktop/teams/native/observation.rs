use super::*;
fn bytes(path: &Path, cap: u64) -> Result<Vec<u8>> {
    crate::swarm::read_regular(path, cap)
        .map_err(|_| invalid("Bounded native metadata unavailable"))
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
pub(super) fn ownership(config: &Config, p: &Placement) -> Result<Worktree> {
    match std::fs::metadata(&p.destination) {
        Ok(m) if m.is_dir() => (),
        Ok(_) => return Err(uncertain("Generated worktree is not a directory")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(uncertain("Generated worktree is missing"))
        }
        Err(_) => {
            return Err(invalid(
                "Generated worktree verification is temporarily unavailable",
            ))
        }
    }
    let owner = crate::state::team_worktree_ownership(
        &config.state,
        Path::new(&p.git_dir),
        Path::new(&p.destination),
    )
    .map_err(|_| invalid("Generated worktree verification is temporarily unavailable"))?
    .ok_or_else(|| uncertain("Generated worktree ownership is unconfirmed"))?;
    if owner["branch"] != p.branch {
        return Err(uncertain("Generated worktree branch changed"));
    }
    Ok(Worktree {
        path: p.destination.clone(),
        ownership_nonce: owner["id"]
            .as_str()
            .ok_or_else(|| uncertain("Worktree nonce unavailable"))?
            .into(),
    })
}
pub(super) fn session(native: &Native, plan: &Plan, deadline: Instant) -> Result<LaunchRead> {
    use crate::state::TeamObservation;
    match crate::state::observe_team_launch(&native.config.state, &plan.launch_id, deadline) {
        TeamObservation::Unavailable => {
            Err(invalid("Launch observation is temporarily unavailable"))
        }
        TeamObservation::Absent { legacy } => Ok(LaunchRead::Absent { legacy }),
        TeamObservation::Ended { code } => Ok(LaunchRead::Ended { code }),
        TeamObservation::Mismatch { field } => {
            Err(uncertain(&format!("Original launch {field} changed")))
        }
        TeamObservation::Record(record) => {
            if record.launch_id != plan.launch_id
                || record.agent != plan.agent
                || record.account != plan.account
                || record.cwd != plan.placement.destination
            {
                return Err(uncertain("Original launch identity or cwd changed"));
            }
            Ok(LaunchRead::Session(Box::new(ObservedSession {
                machine: native.machine.clone(),
                launch_id: record.launch_id,
                agent: record.agent,
                account: record.account,
                worktree: ownership(&native.config, &plan.placement)?,
                parent: None,
                name: record.name,
                session_id: record.session_id,
                run_id: record.run_id,
                conversation_id: record.conversation_id,
            })))
        }
    }
}
