//! Teams Add member and Swap (zerus-efeo3 G16; #2533 review): the model
//! choices of a member's account. A named account offers its own catalog,
//! the one Accounts > Default model and effort reads. The freest account
//! offers only the models every account of the agent lists, because the
//! launch planner may pick any of them (team-plan `pick_account` over the
//! agent's `account ls` rows, signed in or not as the launch's inspection
//! finds them); a model only some list comes with those accounts, to be
//! chosen together with one of them. Read only: one `account ls --provider`
//! per call (the agent's rows alone, never OpenCode or another provider's
//! host) and each account's `model-settings`, kept per (accounts revision,
//! account, agent) and bounded. A failure is the caller's cue to fall back
//! to the account's own defaults.
use super::*;
use std::collections::VecDeque;
use zerus_client_core::account_actions::{
    self,
    models::{Catalog as ModelCatalog, Defaults, Model},
    Binding,
};

/// The refusal when this computer has no account of the agent; the window
/// shows its account-defaults hint for it.
pub const NO_ACCOUNT: &str = "No account of this agent is on this computer.";
/// The refusal when the freest account would compare more accounts than
/// [`COMPARED`]; a named account is still read.
pub const TOO_MANY: &str =
    "This agent has too many accounts to compare their models; choose an account.";
/// The agents whose model the member choices offer: DeepSeek keeps its model
/// native and OpenCode is not a member agent.
const AGENTS: [&str; 3] = ["claude", "codex", "kimi"];
/// Catalogs kept: one per (accounts revision, account, agent), oldest out first.
pub(super) const KEPT: usize = 32;
/// Accounts the freest account compares at most.
pub(super) const COMPARED: usize = 16;

#[derive(Default)]
pub(super) struct Cache(Mutex<VecDeque<(String, String, String, ModelCatalog)>>);

/// An account the reply is about.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Named {
    pub id: String,
    pub label: String,
}

/// A model that only some of the agent's accounts list, and which.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct OnSome {
    pub id: String,
    pub label: String,
    pub accounts: Vec<String>,
}

/// The choices and the accounts and revision they belong to.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct MemberModels {
    /// The named account, or "" for the freest.
    pub account: String,
    pub revision: String,
    /// The named account's catalog, or the models every account lists.
    pub catalog: ModelCatalog,
    /// The named account, or every account of the agent.
    pub accounts: Vec<Named>,
    /// The freest account's models that only some accounts list.
    pub on_some: Vec<OnSome>,
}

/// The freest account's choices from every account's catalog: the models
/// all list (with the efforts all offer for them), and apart the models only
/// some list, with those accounts. `catalogs` is not empty.
fn compose(catalogs: &[(Named, ModelCatalog)]) -> (ModelCatalog, Vec<OnSome>) {
    let lists = |catalog: &ModelCatalog, id: &str| catalog.models.iter().any(|m| m.id == id);
    let every = |id: &str| catalogs.iter().all(|(_, catalog)| lists(catalog, id));
    let offers = |id: &str, effort: &String| {
        catalogs.iter().all(|(_, catalog)| {
            catalog
                .models
                .iter()
                .any(|m| m.id == id && m.effort_options.contains(effort))
        })
    };
    let models = catalogs[0]
        .1
        .models
        .iter()
        .filter(|model| every(&model.id))
        .map(|model| Model {
            id: model.id.clone(),
            label: model.label.clone(),
            effort_options: model
                .effort_options
                .iter()
                .filter(|effort| offers(&model.id, effort))
                .cloned()
                .collect(),
        })
        .collect();
    let mut on_some: Vec<OnSome> = Vec::new();
    for (named, catalog) in catalogs {
        for model in catalog.models.iter().filter(|model| !every(&model.id)) {
            if let Some(found) = on_some.iter_mut().find(|o| o.id == model.id) {
                found.accounts.push(named.id.clone());
            } else if on_some.len() < 256 {
                on_some.push(OnSome {
                    id: model.id.clone(),
                    label: model.label.clone(),
                    accounts: vec![named.id.clone()],
                });
            }
        }
    }
    let catalog = ModelCatalog {
        models,
        defaults: Defaults::default(),
        provider_defaults: Defaults::default(),
        effort_supported: catalogs.iter().all(|(_, catalog)| catalog.effort_supported),
    };
    (catalog, on_some)
}

impl Screens {
    /// `agent`'s choices for a local `account` ("" for the freest).
    pub(crate) fn member_models(
        &self,
        machine: Option<String>,
        agent: &str,
        account: &str,
    ) -> Result<MemberModels, String> {
        if !self.0.allowed {
            return Err(REPLAY_NEEDS_HGS.into());
        }
        let machine = machine.ok_or(account_actions::CHANGED)?;
        if !AGENTS.contains(&agent) {
            return Err(account_actions::CHANGED.into());
        }
        // The agent's own rows, read now: a dialog that opened before the
        // window read any accounts reads them too, and no other provider is
        // listed. An older `hgs` lists every row; only the agent's count.
        let listed = account_actions::provider_catalog(&self.0.hgs, &machine, agent)?;
        let revision = listed["revision"].as_str().unwrap_or("").to_owned();
        let rows: Vec<Named> = listed["profiles"]
            .as_array()
            .into_iter()
            .flatten()
            .take(account_actions::MAX_MEMBERS)
            .filter(|row| row["provider"] == agent)
            .filter_map(|row| {
                let id = row["id"].as_str()?;
                let label = row["label"].as_str().filter(|label| !label.is_empty());
                Some(Named {
                    id: id.into(),
                    label: label.unwrap_or(id).into(),
                })
            })
            .collect();
        if !account.is_empty() {
            let named = rows
                .into_iter()
                .find(|row| row.id == account)
                .ok_or(account_actions::CHANGED)?;
            let catalog = self.catalog_of(&machine, &listed, &revision, agent, &named.id)?;
            return Ok(MemberModels {
                account: named.id.clone(),
                revision,
                catalog,
                accounts: vec![named],
                on_some: Vec::new(),
            });
        }
        if rows.is_empty() {
            return Err(NO_ACCOUNT.into());
        }
        if rows.len() > COMPARED {
            return Err(TOO_MANY.into());
        }
        let mut catalogs = Vec::with_capacity(rows.len());
        for named in rows {
            let catalog = self.catalog_of(&machine, &listed, &revision, agent, &named.id)?;
            catalogs.push((named, catalog));
        }
        let (catalog, on_some) = compose(&catalogs);
        Ok(MemberModels {
            account: String::new(),
            revision,
            catalog,
            accounts: catalogs.into_iter().map(|(named, _)| named).collect(),
            on_some,
        })
    }

    /// One account's catalog: kept for this revision, or read with the
    /// Accounts dialog's parse, bound to the listing just read.
    fn catalog_of(
        &self,
        machine: &str,
        listed: &Value,
        revision: &str,
        agent: &str,
        id: &str,
    ) -> Result<ModelCatalog, String> {
        if let Some(kept) = self.kept(revision, id, agent) {
            return Ok(kept);
        }
        let binding = Binding::capture(ComputerId::local(), machine.into(), listed, id)?;
        if binding.provider != agent {
            return Err(account_actions::CHANGED.into());
        }
        let catalog = binding.read_model_settings(&self.0.hgs)?;
        let mut kept = self
            .0
            .member_models
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        kept.retain(|(r, a, g, _)| !(r == revision && a == id && g == agent));
        while kept.len() >= KEPT {
            kept.pop_front();
        }
        kept.push_back((revision.into(), id.into(), agent.into(), catalog.clone()));
        Ok(catalog)
    }

    fn kept(&self, revision: &str, account: &str, agent: &str) -> Option<ModelCatalog> {
        let kept = self
            .0
            .member_models
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        kept.iter()
            .find(|(r, a, g, _)| r == revision && a == account && g == agent)
            .map(|(_, _, _, catalog)| catalog.clone())
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
        assert_eq!(
            first.accounts,
            [Named {
                id: "work".into(),
                label: "work".into()
            }]
        );
        let read = calls(&scratch);
        let verbs: Vec<_> = read.iter().map(|c| c[3..].join(" ")).collect();
        assert_eq!(
            verbs,
            ["ls --provider codex", "model-settings work --json"],
            "the agent's listing, then the account's catalog"
        );
        assert!(read.iter().all(|c| c[..2] == ["--expect-machine", MACHINE]));
        // The dialog opens again: only the listing is read.
        assert_eq!(
            screens
                .member_models(Some(MACHINE.into()), "codex", "work")
                .unwrap(),
            first
        );
        assert_eq!(calls(&scratch).len(), 3);
        // Another revision is read again, and bound to it.
        revise(&scratch, &screens, "r2");
        let again = screens
            .member_models(Some(MACHINE.into()), "codex", "work")
            .unwrap();
        assert_eq!(again.revision, "r2");
        assert_eq!(calls(&scratch).len(), 5);
    }

    #[test]
    fn refuses_another_agent_an_unknown_account_and_no_machine_without_reading_a_catalog() {
        let (scratch, screens) = fixture(&[("work", "codex")]);
        for (machine, agent, account) in [
            (Some(MACHINE), "claude", "work"),
            (Some(MACHINE), "codex", "missing"),
            (None, "codex", "work"),
            (Some(MACHINE), "dsh", ""),
            (Some(MACHINE), "opencode", ""),
        ] {
            assert_eq!(
                screens.member_models(machine.map(Into::into), agent, account),
                Err(account_actions::CHANGED.into()),
                "{agent} {account}"
            );
        }
        let verbs: Vec<_> = calls(&scratch).iter().map(|c| c[3..].join(" ")).collect();
        assert_eq!(verbs, ["ls --provider claude", "ls --provider codex"]);
        // A kept catalog answers only its own agent.
        screens
            .member_models(Some(MACHINE.into()), "codex", "work")
            .unwrap();
        assert_eq!(
            screens.member_models(Some(MACHINE.into()), "claude", "work"),
            Err(account_actions::CHANGED.into())
        );
        let verbs: Vec<_> = calls(&scratch).iter().map(|c| c[3..].join(" ")).collect();
        assert_eq!(
            verbs[2..],
            [
                "ls --provider codex",
                "model-settings work --json",
                "ls --provider claude"
            ],
            "the kept codex catalog never answers claude"
        );
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
        // The newest is kept (the listing alone is read); the oldest went
        // out first and its catalog is read again.
        screens
            .member_models(Some(MACHINE.into()), "claude", &ids[KEPT])
            .unwrap();
        assert_eq!(calls(&scratch).len(), before + 1);
        screens
            .member_models(Some(MACHINE.into()), "claude", &ids[0])
            .unwrap();
        assert_eq!(calls(&scratch).len(), before + 3);
    }

    #[test]
    fn no_account_of_the_agent_and_too_many_to_compare_are_refused_with_their_reasons() {
        let (scratch, screens) = fixture(&[("work", "codex")]);
        assert_eq!(
            screens.member_models(Some(MACHINE.into()), "kimi", ""),
            Err(NO_ACCOUNT.into())
        );
        let verbs: Vec<_> = calls(&scratch).iter().map(|c| c[3..].join(" ")).collect();
        assert_eq!(verbs, ["ls --provider kimi"], "no catalog is read");
        let ids: Vec<String> = (0..=COMPARED).map(|i| format!("k{i}")).collect();
        let profiles: Vec<(&str, &str)> = ids.iter().map(|id| (id.as_str(), "kimi")).collect();
        let (many, screens) = fixture(&profiles);
        assert_eq!(
            screens.member_models(Some(MACHINE.into()), "kimi", ""),
            Err(TOO_MANY.into())
        );
        assert_eq!(calls(&many).len(), 1, "no catalog is read");
        // A named account of the same agent is still read.
        assert!(screens
            .member_models(Some(MACHINE.into()), "kimi", "k3")
            .is_ok());
    }

    /// One `account ls` row with its own synthetic model list, which the
    /// fake's `model-settings` answers for it.
    fn listing(id: &str, provider: &str, models: &[&str]) -> Value {
        let mut row = row(id, provider, Some(("ok", "signed_in")), true);
        row["models"] = json!(models
            .iter()
            .map(|m| json!({"id": m, "label": format!("{m} label"), "effort_options": ["low", "high"]}))
            .collect::<Vec<_>>());
        row
    }

    /// #2533 review (P2; review2533's launch-choice.py adopted): the launch
    /// planner may pick any account of the agent that `account ls` lists (its
    /// own inputs; the launch's inspection decides sign-in), so the freest
    /// account offers only the models every one of them lists.
    #[test]
    fn the_freest_account_offers_only_the_models_every_account_of_the_agent_lists() {
        let mut spare = listing("spare", "kimi", &["shared", "first-only"]);
        spare["account_status"] = json!({"status": "signed_out", "auth_status": "signed_out"});
        spare["installed"] = json!(false);
        let (scratch, screens) = fixture_rows(vec![
            listing("first", "kimi", &["shared", "first-only"]),
            listing("freer", "kimi", &["freer-only", "shared"]),
            spare,
            listing("other", "codex", &["codex-only"]),
        ]);
        let freest = screens
            .member_models(Some(MACHINE.into()), "kimi", "")
            .unwrap();
        let ids: Vec<_> = freest
            .catalog
            .models
            .iter()
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(
            ids,
            ["shared"],
            "a model only some accounts list is not the freest's"
        );
        assert_eq!(freest.account, "", "no stand-in account is named");
        let verbs: Vec<_> = calls(&scratch).iter().map(|c| c[3..].join(" ")).collect();
        assert_eq!(
            verbs,
            [
                "ls --provider kimi",
                "model-settings first --json",
                "model-settings freer --json",
                "model-settings spare --json"
            ]
        );
        let accounts: Vec<_> = freest.accounts.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            accounts,
            ["first", "freer", "spare"],
            "the planner's inputs, in listing order"
        );
        assert_eq!(
            freest.on_some,
            [
                OnSome {
                    id: "first-only".into(),
                    label: "first-only label".into(),
                    accounts: vec!["first".into(), "spare".into()]
                },
                OnSome {
                    id: "freer-only".into(),
                    label: "freer-only label".into(),
                    accounts: vec!["freer".into()]
                },
            ]
        );
        // Each account's catalog is kept: the next open reads the listing alone.
        screens
            .member_models(Some(MACHINE.into()), "kimi", "")
            .unwrap();
        assert_eq!(calls(&scratch).len(), 5);
    }

    #[test]
    fn the_freest_account_offers_only_the_efforts_every_account_offers_for_a_model() {
        let mut narrow = listing("narrow", "claude", &["shared"]);
        narrow["models"][0]["effort_options"] = json!(["high"]);
        let (_scratch, screens) =
            fixture_rows(vec![listing("wide", "claude", &["shared"]), narrow]);
        let freest = screens
            .member_models(Some(MACHINE.into()), "claude", "")
            .unwrap();
        assert_eq!(freest.catalog.models[0].effort_options, ["high"]);
        assert!(freest.catalog.effort_supported);
        assert!(freest.on_some.is_empty());
    }

    /// #2533 review (P2 refresh, P3): a dialog opened before the window read
    /// any accounts still reads its agent's catalog, and only that agent's.
    #[test]
    fn a_cold_hub_reads_the_agents_own_listing_and_asks_no_other_provider() {
        let (scratch, screens) = fixture(&[("work", "kimi"), ("code", "codex")]);
        screens.lock().accounts.catalogs.clear();
        let named = screens
            .member_models(Some(MACHINE.into()), "kimi", "work")
            .unwrap();
        assert_eq!(
            (named.account.as_str(), named.revision.as_str()),
            ("work", "r1")
        );
        let verbs: Vec<_> = calls(&scratch).iter().map(|c| c[3..].join(" ")).collect();
        assert_eq!(verbs, ["ls --provider kimi", "model-settings work --json"]);
    }

    /// The frontend's MemberChoices tests read this command's own replies;
    /// set ZERUS_UPDATE_MEMBER_MODELS_FIXTURE=1 to write them again.
    #[test]
    fn the_member_models_fixture_is_this_command_reply() {
        let mut spare = row(
            "claude-spare",
            "claude",
            Some(("signed_out", "signed_out")),
            true,
        );
        spare["label"] = json!("Spare");
        spare["models"] = json!([
            {"id": "model-a", "label": "Example model", "effort_options": ["low", "high"]},
            {"id": "model-b", "label": "Second model", "effort_options": ["low"]}
        ]);
        let mut work = row("claude-work", "claude", Some(("ok", "signed_in")), true);
        work["label"] = json!("Work");
        let (_scratch, screens) = fixture_rows(vec![
            work,
            spare,
            row("kimi-work", "kimi", Some(("ok", "signed_in")), true),
            row("codex-work", "codex", Some(("ok", "signed_in")), true),
        ]);
        // The freest account's reply per agent, a named account's per account.
        let reply = |agent: &str, account: &str| {
            serde_json::to_value(
                screens
                    .member_models(Some(MACHINE.into()), agent, account)
                    .unwrap(),
            )
            .unwrap()
        };
        let freest: serde_json::Map<String, Value> = ["claude", "codex", "kimi"]
            .into_iter()
            .map(|agent| (agent.to_owned(), reply(agent, "")))
            .collect();
        let named: serde_json::Map<String, Value> = [
            ("claude", "claude-work"),
            ("claude", "claude-spare"),
            ("codex", "codex-work"),
            ("kimi", "kimi-work"),
        ]
        .into_iter()
        .map(|(agent, account)| (account.to_owned(), reply(agent, account)))
        .collect();
        assert_eq!(
            freest["claude"]["account"], "",
            "the freest names no account"
        );
        assert_eq!(
            freest["claude"]["on_some"][0]["accounts"],
            json!(["claude-spare"])
        );
        assert_eq!(
            freest["kimi"]["catalog"]["effort_supported"], false,
            "Kimi keeps effort native"
        );
        assert_eq!(freest["claude"]["catalog"]["effort_supported"], true);
        let replies = json!({"freest": freest, "named": named,
            "refusals": {"no_account": NO_ACCOUNT, "too_many": TOO_MANY, "changed": account_actions::CHANGED}});
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
