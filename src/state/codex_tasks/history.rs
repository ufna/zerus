//! Account-relative proof for native Codex task histories.
use super::*;
use crate::platform;

const ACCOUNT_UNAVAILABLE: &str = "Codex account home is unavailable; no message was queued";
const HISTORY_OUTSIDE: &str =
    "Codex native task history path does not match the account sessions; no message was queued";

pub(in crate::state) struct HistoryRoot {
    configured: PathBuf,
    directory: repositories::directory::Directory,
}

impl HistoryRoot {
    pub(in crate::state) fn open(record: &Value) -> Result<Self> {
        let mut configured = recorded_agent_home(record);
        if !configured.is_absolute() && !string(record, "agent_home").is_empty() {
            configured = Path::new(string(record, "launch_dir")).join(configured);
        }
        if !configured.is_absolute() {
            return Err(ACCOUNT_UNAVAILABLE.into());
        }
        let resolved = platform::canonical(&configured).map_err(|_| ACCOUNT_UNAVAILABLE)?;
        let directory =
            repositories::directory::Directory::open(&resolved).map_err(|_| ACCOUNT_UNAVAILABLE)?;
        Ok(Self {
            configured,
            directory,
        })
    }

    pub(in crate::state) fn path(&self) -> &Path {
        &self.directory.path
    }

    pub(in crate::state) fn resolve_transcript(&self, path: &Path) -> Result<PathBuf> {
        let sessions = self.path().join("sessions");
        if !path.starts_with(&sessions) && !path.starts_with(self.configured.join("sessions")) {
            return Err(HISTORY_OUTSIDE.into());
        }
        let resolved = platform::canonical(path)
            .map_err(|_| "Codex native task history path is unavailable; no message was queued")?;
        if !resolved.starts_with(sessions) {
            return Err(HISTORY_OUTSIDE.into());
        }
        Ok(resolved)
    }

    pub(in crate::state) fn verify(&self) -> Result<()> {
        if platform::canonical(&self.configured).ok().as_deref() != Some(self.path())
            || self.directory.verify().is_err()
        {
            return Err("Codex account home changed during native task verification".into());
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn account_verification_survives_new_entries_and_refuses_a_replacement() {
        let fixture = tempfile::tempdir().unwrap();
        let home = fixture.path().join("codex");
        fs::create_dir(&home).unwrap();
        let account = HistoryRoot::open(&json!({"agent":"codex","agent_home":home})).unwrap();
        fs::create_dir(home.join("sessions")).unwrap();
        account.verify().unwrap();

        fs::rename(&home, fixture.path().join("original-codex")).unwrap();
        fs::create_dir(&home).unwrap();
        assert!(account.verify().is_err());
    }
}
