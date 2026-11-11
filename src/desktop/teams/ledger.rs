//! Per-member originals, stored separately from task hand-outs. Unique keys
//! and compare-and-swap transitions prevent a second service writer acquiring
//! an existing original. Reading one key never scans a team's journal.
use super::ports::{ObservedSession, Placement, Plan, Result, Worktree};
use crate::teams::{ops::Request, store::storage};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};
use zerus_team_plan::{model::Team, problem::code, Problem};

pub(super) const BRANCH: &str = "@branch";
const FILE: &str = "member-launch-v1.sqlite3";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum State {
    BranchReady,
    BranchIntent,
    BranchDone,
    WorktreeReady,
    WorktreeIntent,
    WorktreeDone,
    LaunchReady,
    LaunchIntent,
    Submitted,
    Bound,
    ParentIntent,
    Done,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BranchPlan {
    pub team: String,
    pub project: String,
    pub machine: String,
    pub branch: String,
    pub base: String,
    pub placement: Placement,
}
impl BranchPlan {
    pub fn of(team: &Team, placement: Placement) -> Self {
        Self {
            team: team.id.clone(),
            placement,
            project: team.project.clone(),
            machine: team.home.clone(),
            branch: team.integration_branch.clone(),
            base: team.base_commit.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Row {
    pub team: String,
    pub member: String,
    pub epoch: String,
    pub person: String,
    pub version: u64,
    pub original: String,
    pub observed: Option<ObservedSession>,
    pub parent_result: Option<String>,
    pub state: State,
    pub plan: Option<Plan>,
    pub branch: Option<BranchPlan>,
    pub worktree: Option<Worktree>,
    pub problem: Option<Problem>,
    pub card_request: Option<Request>,
    pub card_done: bool,
}
impl Row {
    pub fn new(team: &Team, member: &str, epoch: &str, state: State) -> Self {
        Self {
            team: team.id.clone(),
            member: member.into(),
            epoch: epoch.into(),
            person: team.owner.clone(),
            version: 0,
            original: uuid::Uuid::new_v4().to_string(),
            observed: None,
            parent_result: None,
            state,
            plan: None,
            branch: None,
            worktree: None,
            problem: None,
            card_request: None,
            card_done: false,
        }
    }
}

pub(super) struct Ledger(Connection);
impl Ledger {
    pub fn open(state: &Path) -> Result<Self> {
        if !crate::teams::enabled(state) {
            return Err(Problem::new(code::TEAMS_INACTIVE, "Teams preview is off."));
        }
        let dir = state.join("teams");
        if !crate::platform::verify_private(&dir, crate::platform::Private::Directory)
            .map_err(storage)?
        {
            return Err(storage("Teams directory is not private"));
        }
        let path = dir.join(FILE);
        for suffix in ["", "-wal", "-shm", "-journal"] {
            match crate::platform::verify_private(
                &dir.join(format!("{FILE}{suffix}")),
                crate::platform::Private::File,
            ) {
                Ok(true) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                _ => return Err(storage("Member launch ledger is not private")),
            }
        }
        #[cfg(unix)]
        let identity = {
            crate::sqlite_file::create(&path).map_err(storage)?;
            crate::sqlite_file::private(&path).map_err(storage)?
        };
        #[cfg(not(unix))]
        {
            let file = crate::platform::no_follow(crate::platform::private_open(
                std::fs::OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .read(true)
                    .write(true),
            ))
            .open(&path)
            .map_err(storage)?;
            if !crate::platform::verify_private_file(&file).map_err(storage)? {
                return Err(storage("Member launch ledger is not private"));
            }
        }
        let mut db = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(storage)?;
        #[cfg(unix)]
        crate::sqlite_file::bind_private(&db, &path, identity).map_err(storage)?;
        db.busy_timeout(Duration::from_secs(2)).map_err(storage)?;
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let version: u32 = tx
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(storage)?;
        match version {
            0 => tx
                .execute_batch(include_str!("schema.sql"))
                .map_err(storage)?,
            1 => (),
            _ => return Err(storage("Member launch schema is newer than this Zerus")),
        }
        tx.commit().map_err(storage)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")
            .map_err(storage)?;
        Ok(Self(db))
    }
    pub fn get(&self, team: &str, member: &str) -> Result<Option<Row>> {
        let data: Option<String> = self
            .0
            .query_row(
                "SELECT data FROM originals WHERE team=?1 AND member=?2",
                params![team, member],
                |r| r.get(0),
            )
            .optional()
            .map_err(storage)?;
        let row: Option<Row> = data
            .map(|s| serde_json::from_str(&s).map_err(storage))
            .transpose()?;
        if row
            .as_ref()
            .is_some_and(|r| r.team != team || r.member != member)
        {
            return Err(storage("Member launch key changed"));
        }
        Ok(row)
    }
    pub fn insert(&mut self, row: &Row, now_ms: i64) -> Result<bool> {
        let tx = self
            .0
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let data = encoded(row)?;
        let inserted = tx
            .execute(
                "INSERT OR IGNORE INTO originals(team,member,version,data) VALUES(?1,?2,0,?3)",
                params![row.team, row.member, data],
            )
            .map_err(storage)?
            == 1;
        if inserted {
            journal(&tx, row, now_ms)?;
        }
        tx.commit().map_err(storage)?;
        Ok(inserted)
    }
    pub fn save(&mut self, row: &mut Row, now_ms: i64) -> Result<()> {
        let old = row.version;
        row.version = old
            .checked_add(1)
            .ok_or_else(|| storage("Launch revision overflow"))?;
        let tx = self
            .0
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let changed = tx.execute(
            "UPDATE originals SET version=?3,data=?4 WHERE team=?1 AND member=?2 AND version=?5",
            params![row.team,row.member,row.version,encoded(row)?,old],
        ).map_err(storage)?;
        if changed != 1 {
            return Err(storage("Member launch original changed"));
        }
        journal(&tx, row, now_ms)?;
        tx.commit().map_err(storage)
    }
}
fn encoded(row: &Row) -> Result<String> {
    let data = serde_json::to_string(row).map_err(storage)?;
    if data.len() > 262_144 {
        return Err(storage("Member launch record exceeds its bound"));
    }
    Ok(data)
}
fn journal(tx: &rusqlite::Transaction<'_>, row: &Row, now_ms: i64) -> Result<()> {
    let actor = serde_json::json!({"kind":"service","person":row.person,"team":row.team});
    tx.execute(
        "INSERT INTO journal(team,member,version,at_ms,actor,state) VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            row.team,
            row.member,
            row.version,
            now_ms,
            actor.to_string(),
            serde_json::to_string(&row.state).map_err(storage)?
        ],
    )
    .map_err(storage)?;
    Ok(())
}
