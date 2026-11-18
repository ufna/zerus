//! Teams Add member and Swap (zerus-efeo3 G16): one local account's model
//! catalog, the one Accounts > Default model and effort reads. Read only:
//! bound to the accounts revision the hub holds, kept per revision so a
//! dialog that opens again reads nothing, and bounded. A failure is the
//! caller's cue to fall back to the account's own defaults.
use super::*;
use std::collections::VecDeque;
use zerus_client_core::account_actions::{self, models::Catalog as ModelCatalog, Binding};

/// The refusal when no local account of the agent was signed in when last
/// inspected; the window shows its account-defaults hint for it.
pub const NO_SIGNED_IN: &str = "No signed-in account of this agent here.";

/// Catalogs kept: one per (accounts revision, account, agent), oldest out first.
pub(super) const KEPT: usize = 16;

#[derive(Default)]
pub(super) struct Cache(Mutex<VecDeque<(String, String, String, ModelCatalog)>>);

/// The catalog and the account and revision it belongs to.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct MemberModels {
    pub account: String,
    pub revision: String,
    pub catalog: ModelCatalog,
}

/// The freest account's stand-in: the first installed account of `agent` in
/// this computer's `account ls` that was signed in when last inspected, as
/// `account ls` keeps it. No provider is asked again for this choice.
fn stand_in(accounts: &Value, agent: &str) -> Option<String> {
    let signed_in = |status: &Value| {
        matches!(status["status"].as_str(), Some("ok" | "expired"))
            || status["auth_status"] == "signed_in"
    };
    accounts["profiles"]
        .as_array()?
        .iter()
        .take(account_actions::MAX_MEMBERS)
        .find(|row| {
            row["provider"] == agent
                && row["installed"] == true
                && signed_in(&row["account_status"])
        })
        .and_then(|row| row["id"].as_str())
        .map(Into::into)
}

impl Screens {
    /// `agent`'s local `account` catalog ("" for the freest account's
    /// stand-in), from the cache while the hub's accounts revision stays the
    /// same.
    pub(crate) fn member_models(
        &self,
        machine: Option<String>,
        agent: &str,
        account: &str,
    ) -> Result<MemberModels, String> {
        if !self.0.allowed {
            return Err(REPLAY_NEEDS_HGS.into());
        }
        let local = ComputerId::local();
        let accounts = self
            .lock()
            .accounts
            .catalogs
            .get(&local)
            .and_then(|slot| slot.data.clone())
            .ok_or(account_actions::CHANGED)?;
        let revision = accounts["revision"].as_str().unwrap_or("").to_owned();
        let account = if account.is_empty() {
            stand_in(&accounts, agent).ok_or(NO_SIGNED_IN)?
        } else {
            account.to_owned()
        };
        let account = account.as_str();
        if let Some(kept) = self.kept(&revision, account, agent) {
            return Ok(kept);
        }
        let machine = machine.ok_or(account_actions::CHANGED)?;
        let binding = Binding::capture(local, machine, &accounts, account)?;
        if binding.provider != agent {
            return Err(account_actions::CHANGED.into());
        }
        // The same read and parse as the Accounts dialog: `account ls` proves
        // the revision, `model-settings` answers for this account only.
        let catalog = binding.model_settings(&self.0.hgs)?;
        let mut kept = self
            .0
            .member_models
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        kept.retain(|(r, a, g, _)| !(r == &revision && a == account && g == agent));
        while kept.len() >= KEPT {
            kept.pop_front();
        }
        kept.push_back((
            revision.clone(),
            account.to_owned(),
            agent.to_owned(),
            catalog.clone(),
        ));
        Ok(MemberModels {
            account: account.to_owned(),
            revision,
            catalog,
        })
    }

    fn kept(&self, revision: &str, account: &str, agent: &str) -> Option<MemberModels> {
        let kept = self
            .0
            .member_models
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        kept.iter()
            .find(|(r, a, g, _)| r == revision && a == account && g == agent)
            .map(|(r, a, _, catalog)| MemberModels {
                account: a.clone(),
                revision: r.clone(),
                catalog: catalog.clone(),
            })
    }
}

#[cfg(feature = "live")]
#[tauri::command(async)]
pub fn team_member_models(
    state: State<'_, Screens>,
    source: State<'_, crate::commands::AppState>,
    agent: String,
    account: String,
) -> Result<MemberModels, String> {
    let machine = source
        .pump()
        .machine(&ComputerId::local())
        .and_then(|m| m.machine_id);
    state.member_models(machine, &agent, &account)
}
#[cfg(not(feature = "live"))]
#[tauri::command]
pub fn team_member_models() -> Result<MemberModels, String> {
    Err(REPLAY_NEEDS_HGS.into())
}

// The fake `hgs` is the Accounts dialog tests' Python script, as theirs: Unix only.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;
    const MACHINE: &str = "11111111-1111-4111-8111-111111111111";

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    /// The Accounts dialog's own fake `account ls` and `model-settings`
    /// (client/core/tests/support/fake_hgs_account_edits.py), which logs
    /// every call, and a hub holding the same accounts catalog.
    /// One `account ls` row, with the status it cached at its last inspection.
    fn row(id: &str, provider: &str, status: Option<(&str, &str)>, installed: bool) -> Value {
        let mut row = json!({"id": id, "provider": provider, "label": id, "home": format!("/accounts/{id}"), "auth_revision": "a1", "installed": installed});
        if let Some((status, auth)) = status {
            row["account_status"] =
                json!({"status": status, "auth_status": auth, "checked_at": 100.0});
        }
        row
    }
    fn fixture(profiles: &[(&str, &str)]) -> (Scratch, Screens) {
        fixture_rows(
            profiles
                .iter()
                .map(|(id, provider)| row(id, provider, Some(("ok", "signed_in")), true))
                .collect(),
        )
    }
    fn fixture_rows(rows: Vec<Value>) -> (Scratch, Screens) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let scratch = Scratch(std::env::temp_dir().join(format!(
            "zerus-member-models-{}-{stamp}",
            std::process::id()
        )));
        std::fs::create_dir(&scratch.0).unwrap();
        let hgs = scratch.0.join("hgs");
        std::fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../core/tests/support/fake_hgs_account_edits.py"),
            &hgs,
        )
        .unwrap();
        std::fs::set_permissions(&hgs, std::fs::Permissions::from_mode(0o700)).unwrap();
        let catalog = json!({"revision": "r1", "setup_revision": true, "profiles": rows});
        std::fs::write(scratch.0.join("catalog.json"), catalog.to_string()).unwrap();
        let screens = Screens::new(
            hgs,
            true,
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(Vec::new),
            Arc::new(|| 0),
        );
        screens.lock().accounts.catalogs.insert(
            ComputerId::local(),
            accounts::CatalogSlot {
                requested: Some(0),
                data: Some(catalog),
                ..Default::default()
            },
        );
        (scratch, screens)
    }
    fn calls(scratch: &Scratch) -> Vec<Vec<String>> {
        std::fs::read_to_string(scratch.0.join("calls.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
    fn revise(scratch: &Scratch, screens: &Screens, revision: &str) {
        let mut hub = screens.lock();
        let data = hub
            .accounts
            .catalogs
            .get_mut(&ComputerId::local())
            .unwrap()
            .data
            .as_mut()
            .unwrap();
        data["revision"] = json!(revision);
        std::fs::write(scratch.0.join("catalog.json"), data.to_string()).unwrap();
    }

    #[test]
    fn reads_an_account_catalog_once_per_accounts_revision() {
        let (scratch, screens) = fixture(&[("work", "codex")]);
        let first = screens
            .member_models(Some(MACHINE.into()), "codex", "work")
            .unwrap();
        assert_eq!(
            (first.account.as_str(), first.revision.as_str()),
            ("work", "r1")
        );
        assert_eq!(first.catalog.models[0].effort_options, ["low", "high"]);
        let read = calls(&scratch);
        let verbs: Vec<_> = read.iter().map(|c| c[3..].join(" ")).collect();
        assert_eq!(
            verbs,
            ["ls", "model-settings work --json"],
            "the Accounts dialog's two reads"
        );
        assert!(read.iter().all(|c| c[..2] == ["--expect-machine", MACHINE]));
        // The dialog opens again: nothing is read.
        assert_eq!(
            screens
                .member_models(Some(MACHINE.into()), "codex", "work")
                .unwrap(),
            first
        );
        assert_eq!(calls(&scratch).len(), 2);
        // Another revision is read again, and bound to it.
        revise(&scratch, &screens, "r2");
        let again = screens
            .member_models(Some(MACHINE.into()), "codex", "work")
            .unwrap();
        assert_eq!(again.revision, "r2");
        assert_eq!(calls(&scratch).len(), 4);
    }

    #[test]
    fn refuses_another_agent_an_unknown_account_and_no_machine_without_reading() {
        let (scratch, screens) = fixture(&[("work", "codex")]);
        for (machine, agent, account) in [
            (Some(MACHINE), "claude", "work"),
            (Some(MACHINE), "codex", "missing"),
            (None, "codex", "work"),
        ] {
            assert_eq!(
                screens.member_models(machine.map(Into::into), agent, account),
                Err(account_actions::CHANGED.into()),
                "{agent} {account}"
            );
        }
        assert!(calls(&scratch).is_empty());
        // A kept catalog answers only its own agent.
        screens
            .member_models(Some(MACHINE.into()), "codex", "work")
            .unwrap();
        let read = calls(&scratch).len();
        assert_eq!(
            screens.member_models(Some(MACHINE.into()), "claude", "work"),
            Err(account_actions::CHANGED.into())
        );
        assert_eq!(calls(&scratch).len(), read);
        let replay = Screens::new(
            PathBuf::from("unused-hgs"),
            false,
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(Vec::new),
            Arc::new(|| 0),
        );
        assert_eq!(
            replay.member_models(Some(MACHINE.into()), "codex", "work"),
            Err(REPLAY_NEEDS_HGS.into())
        );
    }

    #[test]
    fn keeps_a_bounded_number_of_catalogs() {
        let ids: Vec<String> = (0..=KEPT).map(|i| format!("a{i}")).collect();
        let profiles: Vec<(&str, &str)> = ids.iter().map(|id| (id.as_str(), "claude")).collect();
        let (scratch, screens) = fixture(&profiles);
        for id in &ids {
            screens
                .member_models(Some(MACHINE.into()), "claude", id)
                .unwrap();
        }
        assert_eq!(screens.0.member_models.0.lock().unwrap().len(), KEPT);
        let before = calls(&scratch).len();
        // The newest is kept; the oldest went out first and is read again.
        screens
            .member_models(Some(MACHINE.into()), "claude", &ids[KEPT])
            .unwrap();
        assert_eq!(calls(&scratch).len(), before);
        screens
            .member_models(Some(MACHINE.into()), "claude", &ids[0])
            .unwrap();
        assert_eq!(calls(&scratch).len(), before + 2);
    }

    #[test]
    fn the_freest_account_reads_the_first_account_signed_in_when_last_inspected() {
        let (scratch, screens) = fixture_rows(vec![
            row("other", "codex", Some(("ok", "signed_in")), true),
            row("absent", "claude", Some(("ok", "signed_in")), false),
            row("spare", "claude", Some(("signed_out", "signed_out")), true),
            row("never", "claude", None, true),
            row(
                "limited",
                "claude",
                Some(("rate_limited", "signed_in")),
                true,
            ),
            row("work", "claude", Some(("ok", "signed_in")), true),
        ]);
        // Not installed, signed out, never inspected and other agents are
        // passed over; a rate-limited account is still signed in.
        let freest = screens
            .member_models(Some(MACHINE.into()), "claude", "")
            .unwrap();
        assert_eq!(freest.account, "limited");
        let verbs: Vec<_> = calls(&scratch).iter().map(|c| c[3..].join(" ")).collect();
        assert_eq!(verbs, ["ls", "model-settings limited --json"]);
        // Its catalog is kept like a named account's.
        assert_eq!(
            screens.member_models(Some(MACHINE.into()), "claude", ""),
            Ok(freest)
        );
        assert_eq!(calls(&scratch).len(), 2);
        // No kimi account at all: refused before any read, with the text
        // the window shows its hint for.
        assert_eq!(
            screens.member_models(Some(MACHINE.into()), "kimi", ""),
            Err(NO_SIGNED_IN.into())
        );
        assert_eq!(calls(&scratch).len(), 2);
        // No claude account signed in when last inspected: the same.
        let (unsigned, screens) = fixture_rows(vec![row(
            "spare",
            "claude",
            Some(("signed_out", "signed_out")),
            true,
        )]);
        assert_eq!(
            screens.member_models(Some(MACHINE.into()), "claude", ""),
            Err(NO_SIGNED_IN.into())
        );
        assert!(calls(&unsigned).is_empty());
    }

    /// The frontend's MemberChoices tests read this command's own replies;
    /// set ZERUS_UPDATE_MEMBER_MODELS_FIXTURE=1 to write them again.
    #[test]
    fn the_member_models_fixture_is_this_command_reply() {
        let (_scratch, screens) = fixture(&[
            ("claude-work", "claude"),
            ("kimi-work", "kimi"),
            ("codex-work", "codex"),
        ]);
        let mut replies = serde_json::Map::new();
        for (agent, account) in [("claude", ""), ("kimi", ""), ("codex", "codex-work")] {
            let reply = screens
                .member_models(Some(MACHINE.into()), agent, account)
                .unwrap();
            replies.insert(agent.into(), serde_json::to_value(reply).unwrap());
        }
        assert_eq!(
            replies["claude"]["account"], "claude-work",
            "the freest stand-in"
        );
        assert_eq!(
            replies["kimi"]["catalog"]["effort_supported"], false,
            "Kimi keeps effort native"
        );
        replies.insert(
            "refusals".into(),
            json!({"no_signed_in": NO_SIGNED_IN, "changed": account_actions::CHANGED}),
        );
        assert_eq!(replies["claude"]["catalog"]["effort_supported"], true);
        let written = serde_json::to_string_pretty(&replies).unwrap() + "\n";
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../src/ui/teams/fixtures/member-models.json");
        if std::env::var_os("ZERUS_UPDATE_MEMBER_MODELS_FIXTURE").is_some() {
            std::fs::write(&path, &written).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_or_default(),
            written,
            "regenerate with ZERUS_UPDATE_MEMBER_MODELS_FIXTURE=1"
        );
    }
}
