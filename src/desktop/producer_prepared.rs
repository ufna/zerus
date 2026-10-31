//! Durable negative preparation only. No producer/controller calls this yet.
//! Nothing here grants startup, Frozen/Drained evidence or writer authority.
use super::{
    presence_admission::Guard,
    producer_session::Session,
    store::{HandoffLock, Location, Store},
};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zerus_client_core::desktop::{profile::BACKGROUND_V1, Hello};

const BYTES: usize = 8192;
const DOMAIN: &[u8] = b"hgs-desktop-prepared-v1\0";

pub(super) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE desktop_producer_prepared (
        id INTEGER PRIMARY KEY CHECK(id=1),
        binding TEXT NOT NULL, digest TEXT NOT NULL,
        phase TEXT NOT NULL CHECK(phase IN('prepared','abort_requested')));",
    )?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    version: u32,
    request_id: String,
    configuration: String,
    service_epoch: String,
    producer_epoch: String,
    frontend: Hello,
    writer_epoch: String,
    drain_id: String,
    import_id: String,
    profile_id: String,
    profile_hash: String,
}
fn canonical_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil() && id.to_string() == value)
}
impl Binding {
    fn validate(&self, configuration: &str) -> Result<()> {
        ensure!(
            self.version == 1 && self.configuration == configuration,
            "prepared scope mismatch"
        );
        ensure!(
            [
                &self.request_id,
                &self.service_epoch,
                &self.producer_epoch,
                &self.frontend.nonce,
                &self.frontend.instance_epoch,
                &self.writer_epoch,
                &self.drain_id,
                &self.import_id,
            ]
            .into_iter()
            .all(|id| canonical_id(id)),
            "invalid prepared identity"
        );
        ensure!(
            self.frontend
                .identity_matches(&self.frontend.nonce, configuration)
                && self.frontend.writer_epoch.is_none()
                && BACKGROUND_V1.matches(1, &self.profile_id, &self.profile_hash),
            "invalid prepared binding"
        );
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Prepared,
    AbortRequested,
}
pub(super) struct Snapshot {
    binding: Binding,
    digest: String,
    phase: Phase,
}
impl Snapshot {
    pub fn request_id(&self) -> &str {
        &self.binding.request_id
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
}
/// Minted only after the original commit and directory sync. This is an exact
/// mutation key for further negative journal intent, never execution permission.
pub(super) struct Ticket {
    request_id: String,
    digest: String,
}
pub(super) enum Saved {
    Created(Ticket),
    Existing(Box<Snapshot>),
}
fn digest(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(DOMAIN);
    hash.update(bytes);
    format!("{:x}", hash.finalize())
}

/// At most two bounded rows: an unexpected second/foreign row is unknown,
/// never absence. The supplied Connection must already have private existing
/// file, schema and configuration checks; this creates or migrates nothing.
pub(super) fn inspect_db(db: &Connection, configuration: &str) -> Result<Option<Snapshot>> {
    let kind: Option<String> = db
        .query_row(
            "SELECT substr(type,1,17) FROM sqlite_schema WHERE name='desktop_producer_prepared'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    ensure!(
        kind.as_deref() == Some("table"),
        "prepared journal table unavailable"
    );
    let mut stmt = db.prepare("SELECT id,substr(CAST(binding AS BLOB),1,8193),substr(digest,1,65),substr(phase,1,33) FROM desktop_producer_prepared ORDER BY id LIMIT 2")?;
    let mut rows = stmt.query([])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let id: i64 = row.get(0)?;
    let bytes: Vec<u8> = row.get(1)?;
    let reported: String = row.get(2)?;
    let phase: String = row.get(3)?;
    ensure!(
        id == 1 && rows.next()?.is_none(),
        "invalid prepared row membership"
    );
    ensure!(
        bytes.len() <= BYTES
            && zerus_client_core::desktop::digest(&reported)
            && digest(&bytes) == reported,
        "invalid prepared bytes"
    );
    let binding: Binding = serde_json::from_slice(&bytes).context("invalid prepared record")?;
    binding.validate(configuration)?;
    let phase = match phase.as_str() {
        "prepared" => Phase::Prepared,
        "abort_requested" => Phase::AbortRequested,
        _ => anyhow::bail!("unsupported prepared phase"),
    };
    Ok(Some(Snapshot {
        binding,
        digest: reported,
        phase,
    }))
}
fn current(store: &Store, location: &Location, epoch: &str) -> Result<()> {
    let (configuration, current): (String, String) = store.db.query_row(
        "SELECT substr(configuration,1,65),substr(epoch,1,37) FROM service WHERE id=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    ensure!(
        canonical_id(epoch) && current == epoch && configuration == location.configuration,
        "prepared service epoch changed"
    );
    Ok(())
}

pub(super) fn prepare(
    location: &Location,
    lock: &HandoffLock,
    presence: &Guard,
    store: &mut Store,
    session: &Session,
    request_id: &str,
) -> Result<Saved> {
    let identity = session.identity();
    let binding = Binding {
        version: 1,
        request_id: request_id.into(),
        configuration: identity.configuration().into(),
        service_epoch: identity.challenge().service_epoch.clone(),
        producer_epoch: identity.challenge().producer_epoch.clone(),
        frontend: identity.hello().clone(),
        writer_epoch: uuid::Uuid::new_v4().to_string(),
        drain_id: uuid::Uuid::new_v4().to_string(),
        import_id: uuid::Uuid::new_v4().to_string(),
        profile_id: BACKGROUND_V1.id().into(),
        profile_hash: BACKGROUND_V1.hash(),
    };
    save(location, lock, presence, store, binding)
}

fn save(
    location: &Location,
    lock: &HandoffLock,
    presence: &Guard,
    store: &mut Store,
    binding: Binding,
) -> Result<Saved> {
    ensure!(cfg!(unix), "producer preparation durability is unsupported");
    lock.verify(location)?;
    presence.verify(location)?;
    location.check_existing()?;
    binding.validate(&location.configuration)?;
    current(store, location, &binding.service_epoch)?;
    let tx = store
        .db
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(existing) = inspect_db(&tx, &location.configuration)? {
        ensure!(
            existing.request_id() == binding.request_id,
            "another original preparation is retained"
        );
        ensure!(
            existing.binding.service_epoch == binding.service_epoch
                && existing.binding.producer_epoch == binding.producer_epoch
                && existing.binding.frontend == binding.frontend
                && existing.binding.profile_id == binding.profile_id
                && existing.binding.profile_hash == binding.profile_hash,
            "prepared request identity changed"
        );
        // Status-only replay: do not rebind the original epochs or mint a key.
        return Ok(Saved::Existing(Box::new(existing)));
    }
    let bytes = serde_json::to_vec(&binding)?;
    ensure!(bytes.len() <= BYTES, "prepared record exceeds its limit");
    let digest = digest(&bytes);
    tx.execute(
        "INSERT INTO desktop_producer_prepared VALUES(1,?1,?2,'prepared')",
        params![std::str::from_utf8(&bytes)?, digest],
    )?;
    tx.commit()?;
    // A failed sync may follow commit: keep the original ID, never retry with a
    // new preparation or treat missing delivery as permission to reopen writers.
    crate::platform::sync_dir(&location.root)?;
    Ok(Saved::Created(Ticket {
        request_id: binding.request_id,
        digest,
    }))
}

/// Record only a request for reconciliation. Both phases block replacement and
/// legacy writers. No API clears the row or claims abort/Frozen/Drained/Complete.
pub(super) fn request_abort(
    location: &Location,
    lock: &HandoffLock,
    presence: &Guard,
    store: &mut Store,
    ticket: &Ticket,
) -> Result<Snapshot> {
    ensure!(cfg!(unix), "producer preparation durability is unsupported");
    lock.verify(location)?;
    presence.verify(location)?;
    location.check_existing()?;
    let snapshot =
        inspect_db(&store.db, &location.configuration)?.context("prepared record missing")?;
    ensure!(
        snapshot.request_id() == ticket.request_id && snapshot.digest == ticket.digest,
        "prepared original changed"
    );
    current(store, location, &snapshot.binding.service_epoch)?;
    if snapshot.phase == Phase::AbortRequested {
        return Ok(snapshot);
    }
    let tx = store
        .db
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    ensure!(tx.execute("UPDATE desktop_producer_prepared SET phase='abort_requested' WHERE id=1 AND digest=?1 AND phase='prepared'", [&ticket.digest])? == 1, "prepared original changed");
    tx.commit()?;
    crate::platform::sync_dir(&location.root)?;
    inspect_db(&store.db, &location.configuration)?.context("prepared record missing")
}

macro_rules! redacted { ($($t:ty),+) => { $(impl std::fmt::Debug for $t {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { f.write_str(concat!(stringify!($t),"([prepared fence])")) }
})+ }; }
redacted!(Snapshot, Ticket, Saved);

#[cfg(all(test, unix))]
mod tests;
