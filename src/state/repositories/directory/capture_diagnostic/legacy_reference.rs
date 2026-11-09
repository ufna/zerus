//! Closed, test-only legacy observations. Neither data nor a checked negative
//! is a current-name, no-effect, terminal-content or compatibility permit.
#![cfg(all(test, windows))]
use super::*;
use crate::platform::{
    self,
    persist_diagnostic::{LegacyTrace, Report},
};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    io::{Read, Seek, SeekFrom},
};

const FILE_BYTES: usize = 256 << 10;
const MAX_HELD: usize = 32;
const REPARSE: u32 = 0x400;
const TOTAL_ROWS: usize = 128;
const TOTAL_BYTES: usize = 64 << 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::state) enum LegacyCase {
    Fresh,
    Reopened,
    ExtraAliases,
    NoDrop,
}
impl LegacyCase {
    fn roots(self) -> usize {
        match self {
            Self::Fresh => 1,
            Self::Reopened | Self::NoDrop => 2,
            Self::ExtraAliases => 4,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::state) enum LegacyAuditPoint {
    TimelyFirst32,
    Teardown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Reason {
    Bound,
    Clock,
    Query,
    Privacy,
    Shape,
    Acquisition,
    Slot,
    Identity,
    Data,
    Snapshot,
    Cursor,
    Panic,
    Partition,
    OldOracle,
    Output,
    Incomplete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    Root,
    Facts,
    Ancestor,
    Fact,
    Writer,
}

/// Original copied fields are DATA, never an admission receipt.
pub(in crate::state) struct OriginalCapturedData {
    fact: platform::FileIdentity,
    writer: platform::FileIdentity,
    bytes: usize,
    digest: [u8; 32],
}
impl OriginalCapturedData {
    pub fn new(
        fact: platform::FileIdentity,
        writer: platform::FileIdentity,
        bytes: usize,
        digest: [u8; 32],
    ) -> Self {
        Self {
            fact,
            writer,
            bytes,
            digest,
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
pub(in crate::state) struct OriginalCapturedBinding {
    case: LegacyCase,
    root: Object,
    directory: Object,
    fact: platform::FileIdentity,
    writer: platform::FileIdentity,
    bytes: usize,
    digest: [u8; 32],
    writer_bytes: usize,
    writer_digest: [u8; 32],
    capture_operations: usize,
}
#[derive(Serialize)]
pub(in crate::state) struct OriginalDataSummary {
    case: LegacyCase,
    phase: &'static str,
    logical_rows: usize,
    fallible_operations_attempted: usize,
    fact_bytes: usize,
    fact_digest: String,
    writer_bytes: usize,
    writer_digest: String,
}
impl OriginalCapturedBinding {
    pub fn logical_rows(&self) -> usize {
        5
    }
    pub fn summary(&self) -> OriginalDataSummary {
        OriginalDataSummary {
            case: self.case,
            phase: "original_data_capture",
            logical_rows: self.logical_rows(),
            fallible_operations_attempted: self.capture_operations,
            fact_bytes: self.bytes,
            fact_digest: hex(&self.digest),
            writer_bytes: self.writer_bytes,
            writer_digest: hex(&self.writer_digest),
        }
    }
    pub fn capture(
        case: LegacyCase,
        root: &Directory,
        directory: &Directory,
        files: (&File, &File),
        original: OriginalCapturedData,
        validate_snapshot: impl Fn(&[u8]) -> io::Result<()>,
    ) -> Result<Self, AuditFailure> {
        let clock = Clock {
            point: None,
            until: Instant::now(),
            queries: Cell::new(0),
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for d in [root, directory] {
                require(
                    clock.query(|| platform::verify_private_directory(&d.file))?,
                    Reason::Privacy,
                )?;
            }
            let r = clock.query(|| handle_info(&root.file))?;
            let d = clock.query(|| handle_info(&directory.file))?;
            require(
                r.directory
                    && d.directory
                    && r.object != d.object
                    && r.attributes & REPARSE == 0
                    && d.attributes & REPARSE == 0,
                Reason::Shape,
            )?;
            let (fact, writer) = files;
            for file in [fact, writer] {
                require(
                    clock.query(|| platform::verify_private_file(file))?,
                    Reason::Privacy,
                )?;
            }
            require(
                identity_matches(
                    clock.query(|| platform::FileIdentity::of(fact))?,
                    original.fact,
                ) && identity_matches(
                    clock.query(|| platform::FileIdentity::of(writer))?,
                    original.writer,
                ),
                Reason::Identity,
            )?;
            let bytes = read_bounded(fact, &clock)?;
            require(
                bytes_match(bytes.len(), digest(&bytes), original.bytes, original.digest),
                Reason::Data,
            )?;
            clock.query(|| validate_snapshot(&bytes)).map_err(|r| {
                if r == Reason::Query {
                    Reason::Snapshot
                } else {
                    r
                }
            })?;
            let writer_bytes = read_bounded(writer, &clock)?;
            require(
                identity_matches(
                    clock.query(|| platform::FileIdentity::of(fact))?,
                    original.fact,
                ) && identity_matches(
                    clock.query(|| platform::FileIdentity::of(writer))?,
                    original.writer,
                ),
                Reason::Identity,
            )?;
            for file in [fact, writer] {
                require(
                    clock.query(|| platform::verify_private_file(file))?,
                    Reason::Privacy,
                )?;
            }
            require(
                clock.query(|| handle_info(&root.file))? == r
                    && clock.query(|| handle_info(&directory.file))? == d,
                Reason::Identity,
            )?;
            Ok(Self {
                case,
                root: r.object,
                directory: d.object,
                fact: original.fact,
                writer: original.writer,
                bytes: original.bytes,
                digest: original.digest,
                writer_bytes: writer_bytes.len(),
                writer_digest: digest(&writer_bytes),
                capture_operations: clock.queries.get(),
            })
        }))
        .unwrap_or(Err(Reason::Panic));
        result.map_err(|reason| AuditFailure::empty(None, reason))
    }
}

struct Clock {
    point: Option<LegacyAuditPoint>,
    until: Instant,
    queries: Cell<usize>,
}
impl Clock {
    fn check(&self) -> Result<(), Reason> {
        require(
            self.point != Some(LegacyAuditPoint::TimelyFirst32) || Instant::now() < self.until,
            Reason::Clock,
        )
    }
    fn query<T>(&self, operation: impl FnOnce() -> io::Result<T>) -> Result<T, Reason> {
        self.check()?;
        self.queries.set(self.queries.get().saturating_add(1));
        let value = operation();
        self.check()?;
        value.map_err(|_| Reason::Query)
    }
}
fn require(value: bool, reason: Reason) -> Result<(), Reason> {
    if value {
        Ok(())
    } else {
        Err(reason)
    }
}
fn identity_matches(actual: platform::FileIdentity, original: platform::FileIdentity) -> bool {
    actual == original
}
fn bytes_match(
    actual_bytes: usize,
    actual_digest: [u8; 32],
    expected_bytes: usize,
    expected_digest: [u8; 32],
) -> bool {
    actual_bytes == expected_bytes && actual_digest == expected_digest
}
fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
struct Restore<'a> {
    file: &'a File,
    position: u64,
    armed: bool,
}
impl Restore<'_> {
    fn finish(&mut self, clock: &Clock) -> Result<(), Reason> {
        // Restoration is attempted even if reading failed or time expired.
        let mut file = self.file;
        let before = clock.check();
        clock.queries.set(clock.queries.get().saturating_add(1));
        let seek = file.seek(SeekFrom::Start(self.position));
        let between = clock.check();
        let restored = seek
            .and_then(|_| {
                clock.queries.set(clock.queries.get().saturating_add(1));
                file.stream_position()
            })
            .map(|at| at == self.position);
        let after = clock.check();
        match restored {
            Ok(true) => self.armed = false,
            _ => return Err(Reason::Cursor),
        }
        before?;
        between?;
        after
    }
}
impl Drop for Restore<'_> {
    fn drop(&mut self) {
        if self.armed {
            let mut file = self.file;
            let _ = file.seek(SeekFrom::Start(self.position));
        }
    }
}
fn read_bounded(file: &File, clock: &Clock) -> Result<Vec<u8>, Reason> {
    let mut borrowed = file;
    let position = clock.query(|| borrowed.stream_position())?;
    let mut restore = Restore {
        file,
        position,
        armed: true,
    };
    let value = (|| {
        clock.query(|| borrowed.seek(SeekFrom::Start(0)))?;
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let remaining = (FILE_BYTES + 1).saturating_sub(bytes.len());
            require(remaining > 0, Reason::Bound)?;
            let count = clock.query(|| borrowed.read(&mut buffer[..remaining.min(4096)]))?;
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..count]);
            require(bytes.len() <= FILE_BYTES, Reason::Bound)?;
        }
        Ok(bytes)
    })();
    let restored = restore.finish(clock);
    restored?;
    value
}

// BEGIN PORTABLE LEGACY POLICY (metadata/data policy only, no Windows I/O).
fn compare(
    role: Role,
    before: &HandleInfo,
    after: &HandleInfo,
    private: bool,
    acquisition: bool,
) -> Result<(), Reason> {
    require(acquisition, Reason::Acquisition)?;
    require(
        before.attributes & REPARSE == 0 && after.attributes & REPARSE == 0,
        Reason::Shape,
    )?;
    require(
        before.handle == after.handle
            && before.object == after.object
            && before.granted_access == after.granted_access
            && before.mode == after.mode
            && before.directory == after.directory,
        Reason::Identity,
    )?;
    require(
        after.directory == matches!(role, Role::Root | Role::Facts | Role::Ancestor)
            && before.directory == (before.attributes & 0x10 != 0)
            && after.directory == (after.attributes & 0x10 != 0),
        Reason::Shape,
    )?;
    require(role == Role::Ancestor || private, Reason::Privacy)?;
    if role == Role::Root {
        // This computes a separate NEGATIVE-purpose guard. The old complete
        // comparison remains false and every length/attribute difference stays.
        let changed = differences(before, after);
        require(
            changed
                .iter()
                .all(|f| matches!(*f, "length" | "attributes")),
            Reason::Identity,
        )
    } else {
        require(before == after, Reason::OldOracle)
    }
}
#[derive(Clone, Copy)]
struct PurposeChecks {
    context: bool,
    audits: bool,
    old: bool,
    trace: bool,
    producer: bool,
    source: bool,
    coverage: bool,
    budget: bool,
}
fn accounting(required: usize, reported: usize) -> bool {
    required <= reported && reported <= TOTAL_ROWS
}
fn all(c: PurposeChecks) -> bool {
    c.context && c.audits && c.old && c.trace && c.producer && c.source && c.coverage && c.budget
}
// END PORTABLE LEGACY POLICY

#[derive(Clone, Serialize)]
struct ClosureView {
    length: u64,
    attributes: u32,
    granted_access: u32,
    mode: u32,
    directory: bool,
    changed: Vec<&'static str>,
}
#[derive(Clone, Serialize)]
struct View {
    // Slot is the ORIGINAL inventory ordinal, never an encoding/hash of an OS handle.
    slot: usize,
    complete: bool,
    closure: Option<ClosureView>,
    length: u64,
    attributes: u32,
    granted_access: u32,
    mode: u32,
    directory: bool,
    changed: Vec<&'static str>,
    private_checked: bool,
    privacy_allowed: Option<bool>,
    data_bytes: Option<usize>,
    data_digest: Option<String>,
}
#[derive(Serialize)]
pub(in crate::state) struct AuditFailure {
    point: Option<LegacyAuditPoint>,
    reason: Reason,
    expected: usize,
    checked: usize,
    not_checked: usize,
    failed_slot: Option<usize>,
    fallible_operations_attempted: usize,
    rows: Vec<View>,
    started_before_deadline: bool,
    finished_before_deadline: bool,
}
impl std::fmt::Debug for AuditFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditFailure")
            .field("point", &self.point)
            .field("reason", &self.reason)
            .finish()
    }
}
impl AuditFailure {
    fn empty(point: Option<LegacyAuditPoint>, reason: Reason) -> Self {
        Self {
            point,
            reason,
            expected: 0,
            checked: 0,
            not_checked: 0,
            failed_slot: None,
            fallible_operations_attempted: 0,
            rows: vec![],
            started_before_deadline: false,
            finished_before_deadline: false,
        }
    }
    pub fn unavailable(point: LegacyAuditPoint) -> Self {
        Self::empty(Some(point), Reason::Incomplete)
    }
    pub fn io(&self) -> io::Error {
        io::Error::other("legacy held-object audit not qualified")
    }
    pub fn logical_rows(&self) -> usize {
        self.rows.len() + 1
    }
}
#[derive(Serialize)]
pub(in crate::state) struct LegacyAudit {
    point: LegacyAuditPoint,
    expected: usize,
    checked: usize,
    rows: Vec<View>,
    r_held: usize,
    retained_held: usize,
    physically_released: bool,
    fallible_operations_attempted: usize,
    started_before_deadline: bool,
    finished_before_deadline: bool,
    #[serde(skip)]
    until: Instant,
    #[serde(skip)]
    original: OriginalCapturedBinding,
    #[serde(skip)]
    inventory: Vec<Inventory>,
}
impl LegacyAudit {
    pub fn logical_rows(&self) -> usize {
        self.rows.len() + 1
    }
    pub fn point(&self) -> LegacyAuditPoint {
        self.point
    }
}

fn role(held: &Held, binding: &OriginalCapturedBinding) -> Result<Role, Reason> {
    let object = held.inventory.native.object;
    if object == binding.root {
        return Ok(Role::Root);
    }
    if object == binding.directory {
        return Ok(Role::Facts);
    }
    if object.volume == binding.fact.device && object.file == binding.fact.inode {
        return Ok(Role::Fact);
    }
    if object.volume == binding.writer.device && object.file == binding.writer.inode {
        return Ok(Role::Writer);
    }
    require(held.inventory.native.directory, Reason::Shape)?;
    Ok(Role::Ancestor)
}
fn provenance(i: &Inventory, role: Role, case: LegacyCase) -> bool {
    let source = if matches!(role, Role::Fact | Role::Writer) {
        let origin = if role == Role::Fact {
            "capture-fact-file"
        } else {
            "capture-independent-writer-file"
        };
        if i.origin != origin || i.duplicated_original {
            return false;
        }
        Acquisition::PrivateReader
    } else if i.origin == "capture-facts/primary" {
        if role != Role::Facts || i.duplicated_original {
            return false;
        }
        if case == LegacyCase::Fresh {
            Acquisition::CreatedDirectory
        } else {
            Acquisition::OpenedDirectory
        }
    } else if i.origin == "extra-duplicate-R" {
        if role != Role::Root || case != LegacyCase::ExtraAliases || !i.duplicated_original {
            return false;
        }
        Acquisition::OpenedDirectory
    } else {
        if i.duplicated_original {
            return false;
        }
        let primary = i.origin == "capture-root/primary" && role == Role::Root;
        let extra = i.origin == "extra-root-open/primary"
            && role == Role::Root
            && case == LegacyCase::ExtraAliases;
        let ancestor = [
            "capture-root/ancestor-",
            "capture-facts/ancestor-",
            "extra-root-open/ancestor-",
        ]
        .iter()
        .any(|prefix| {
            i.origin.strip_prefix(prefix).is_some_and(|n| {
                !n.is_empty() && n.len() <= 3 && n.bytes().all(|b| b.is_ascii_digit())
            }) && (!prefix.starts_with("extra") || case == LegacyCase::ExtraAliases)
                && matches!(role, Role::Ancestor | Role::Root)
        });
        if !primary && !extra && !ancestor {
            return false;
        }
        Acquisition::OpenedDirectory
    };
    let (access, share, name) = source.masks();
    i.ownership == "synthetic-test-owned"
        && i.requested_access == access
        && i.requested_share == share
        && i.acquisition == name
}
fn same_inventory(a: &Inventory, b: &Inventory) -> bool {
    a.origin == b.origin
        && a.ownership == b.ownership
        && a.acquisition == b.acquisition
        && a.requested_access == b.requested_access
        && a.requested_share == b.requested_share
        && a.duplicated_original == b.duplicated_original
        && a.native == b.native
}
fn slot_for_inventory(
    inventory: &[Inventory],
    held: &Inventory,
    seen: &[usize],
) -> Result<usize, Reason> {
    require(
        !inventory.is_empty() && inventory.len() <= MAX_HELD,
        Reason::Bound,
    )?;
    let matches: Vec<_> = inventory
        .iter()
        .enumerate()
        .filter(|(_, i)| i.native.handle == held.native.handle)
        .collect();
    require(matches.len() == 1, Reason::Slot)?;
    let (slot, original) = matches[0];
    require(
        !seen.contains(&slot) && same_inventory(original, held),
        Reason::Slot,
    )?;
    Ok(slot)
}
fn private(held: &Held, role: Role, clock: &Clock) -> Result<bool, Reason> {
    if role == Role::Ancestor {
        return Ok(false);
    }
    clock.query(|| {
        if matches!(role, Role::Root | Role::Facts) {
            platform::verify_private_directory(&held.file)
        } else {
            platform::verify_private_file(&held.file)
        }
    })
}
fn file_data(
    held: &Held,
    role: Role,
    clock: &Clock,
    binding: &OriginalCapturedBinding,
    validate: &impl Fn(&[u8]) -> io::Result<()>,
    row: &mut View,
) -> Result<(), Reason> {
    if !matches!(role, Role::Fact | Role::Writer) {
        return Ok(());
    }
    let identity = if role == Role::Fact {
        binding.fact
    } else {
        binding.writer
    };
    require(
        identity_matches(
            clock.query(|| platform::FileIdentity::of(&held.file))?,
            identity,
        ),
        Reason::Identity,
    )?;
    let bytes = read_bounded(&held.file, clock)?;
    let hash = digest(&bytes);
    row.data_bytes = Some(bytes.len());
    row.data_digest = Some(hex(&hash));
    let expected = if role == Role::Fact {
        (binding.bytes, binding.digest)
    } else {
        (binding.writer_bytes, binding.writer_digest)
    };
    require(
        bytes_match(bytes.len(), hash, expected.0, expected.1),
        Reason::Data,
    )?;
    if role == Role::Fact {
        clock.query(|| validate(&bytes)).map_err(|r| {
            if r == Reason::Query {
                Reason::Snapshot
            } else {
                r
            }
        })?;
    }
    require(
        identity_matches(
            clock.query(|| platform::FileIdentity::of(&held.file))?,
            identity,
        ),
        Reason::Identity,
    )?;
    Ok(())
}

impl Cohort {
    pub(in crate::state) fn audit_legacy_failure(
        &self,
        point: LegacyAuditPoint,
        original_until: Instant,
        binding: &OriginalCapturedBinding,
        validate_snapshot: impl Fn(&[u8]) -> io::Result<()>,
    ) -> Result<LegacyAudit, AuditFailure> {
        let mut failure = AuditFailure::empty(Some(point), Reason::Incomplete);
        failure.started_before_deadline = Instant::now() < original_until;
        failure.expected = self.roots.len() + self.retained.len();
        failure.not_checked = failure.expected;
        let clock = Clock {
            point: Some(point),
            until: original_until,
            queries: Cell::new(0),
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            clock.check()?;
            require(
                self.root == binding.root && self.expected_roots == binding.case.roots(),
                Reason::Partition,
            )?;
            require(
                !self.inventory.is_empty() && self.inventory.len() <= MAX_HELD,
                Reason::Bound,
            )?;
            require(
                self.roots.len()
                    + self.retained.len()
                    + if self.released {
                        self.expected_roots
                    } else {
                        0
                    }
                    == self.inventory.len(),
                Reason::Partition,
            )?;
            require(
                if self.released {
                    point == LegacyAuditPoint::Teardown && self.roots.is_empty()
                } else {
                    self.roots.len() == self.expected_roots
                },
                Reason::Partition,
            )?;
            let held: Vec<_> = self.roots.iter().chain(&self.retained).collect();
            failure.expected = held.len();
            failure.not_checked = failure.expected;
            let mut slots = Vec::new();
            let mut roles = Vec::new();
            for h in &held {
                let slot = slot_for_inventory(&self.inventory, &h.inventory, &slots)?;
                let r = role(h, binding)?;
                require(
                    provenance(&h.inventory, r, binding.case),
                    Reason::Acquisition,
                )?;
                slots.push(slot);
                roles.push(r);
            }
            require(
                roles.iter().filter(|r| **r == Role::Root).count() == self.roots.len()
                    && roles.iter().filter(|r| **r == Role::Facts).count() == 1
                    && roles.iter().filter(|r| **r == Role::Fact).count() == 1
                    && roles.iter().filter(|r| **r == Role::Writer).count() == 1,
                Reason::Partition,
            )?;
            // One complete same-held audit at THIS phase, with fresh before/
            // after identity, metadata, privacy and bounded data/read-restoration
            // guards for each object. Sequential checks are not an atomic
            // filesystem snapshot or continuous ACL freeze. No historical row
            // supplies this phase or its closure.
            {
                for ((h, r), slot) in held.iter().zip(&roles).zip(&slots) {
                    failure.failed_slot = Some(*slot);
                    clock.check()?;
                    let actual = clock.query(|| handle_info(&h.file))?;
                    failure.rows.push(View {
                        slot: *slot,
                        complete: false,
                        closure: None,
                        length: actual.length,
                        attributes: actual.attributes,
                        granted_access: actual.granted_access,
                        mode: actual.mode,
                        directory: actual.directory,
                        changed: differences(&h.inventory.native, &actual),
                        private_checked: false,
                        privacy_allowed: None,
                        data_bytes: None,
                        data_digest: None,
                    });
                    let row = failure.rows.last_mut().ok_or(Reason::Incomplete)?;
                    let privacy = private(h, *r, &clock)?;
                    row.private_checked = *r != Role::Ancestor;
                    row.privacy_allowed = if *r == Role::Ancestor {
                        None
                    } else {
                        Some(privacy)
                    };
                    file_data(h, *r, &clock, binding, &validate_snapshot, row)?;
                    let after = clock.query(|| handle_info(&h.file))?;
                    row.closure = Some(ClosureView {
                        length: after.length,
                        attributes: after.attributes,
                        granted_access: after.granted_access,
                        mode: after.mode,
                        directory: after.directory,
                        changed: differences(&h.inventory.native, &after),
                    });
                    require(actual == after, Reason::Identity)?;
                    let final_privacy = private(h, *r, &clock)?;
                    row.privacy_allowed = if *r == Role::Ancestor {
                        None
                    } else {
                        Some(privacy && final_privacy)
                    };
                    compare(
                        *r,
                        &h.inventory.native,
                        &after,
                        privacy && final_privacy,
                        provenance(&h.inventory, *r, binding.case),
                    )?;
                    clock.check()?;
                    row.complete = true;
                    failure.checked += 1;
                    failure.not_checked = failure.expected - failure.checked;
                }
                clock.check()?;
            }
            clock.check()
        }))
        .unwrap_or(Err(Reason::Panic));
        failure.finished_before_deadline = Instant::now() < original_until;
        failure.fallible_operations_attempted = clock.queries.get();
        if let Err(reason) = result {
            failure.reason = reason;
            return Err(failure);
        }
        Ok(LegacyAudit {
            point,
            expected: failure.expected,
            checked: failure.checked,
            rows: failure.rows,
            r_held: self.roots.len(),
            retained_held: self.retained.len(),
            physically_released: self.released,
            fallible_operations_attempted: clock.queries.get(),
            started_before_deadline: failure.started_before_deadline,
            finished_before_deadline: failure.finished_before_deadline,
            until: original_until,
            original: binding.clone(),
            inventory: self.inventory.clone(),
        })
    }
}
fn hex(digest: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(64);
    for byte in digest {
        text.push(HEX[(byte >> 4) as usize] as char);
        text.push(HEX[(byte & 15) as usize] as char);
    }
    text
}

fn old_reference(
    case: LegacyCase,
    root: Object,
    log: &VerificationLog,
    inventory: &[Inventory],
) -> bool {
    if log.row_overflow
        || log.phase_overflow
        || log.seen_rows != log.rows.len()
        || log.released_by_diagnostic
        || log.r_held != case.roots()
        || log.r_held + log.retained_held != inventory.len()
    {
        return false;
    }
    let expected = if case == LegacyCase::NoDrop {
        vec![Phase::Initial, Phase::AfterProducer]
    } else {
        vec![Phase::Initial, Phase::BeforeRelease, Phase::AfterProducer]
    };
    if log.phases.len() != expected.len() {
        return false;
    }
    for (p, phase) in log.phases.iter().zip(expected) {
        if std::mem::discriminant(&p.phase) != std::mem::discriminant(&phase) {
            return false;
        }
        let rows: Vec<_> = log
            .rows
            .iter()
            .filter(|r| std::mem::discriminant(&r.phase) == std::mem::discriminant(&phase))
            .collect();
        if rows.len() != p.checked
            || p.checked + p.not_checked != p.expected
            || p.expected != inventory.len()
        {
            return false;
        }
        for row in &rows {
            let matched: Vec<_> = inventory
                .iter()
                .filter(|i| i.origin == row.origin && i.native == row.before)
                .collect();
            if matched.len() != 1
                || row.cohort
                    != if row.before.object == root {
                        "R"
                    } else {
                        "retained"
                    }
            {
                return false;
            }
        }
        if matches!(phase, Phase::Initial) {
            if p.ok != Some(true)
                || p.error.is_some()
                || p.not_checked != 0
                || rows
                    .iter()
                    .any(|r| r.query_error.is_some() || r.after.as_ref() != Some(&r.before))
            {
                return false;
            }
        } else {
            if p.ok != Some(false)
                || p.error.is_none()
                || rows.iter().filter(|r| !r.changed.is_empty()).count() != 1
                || rows.last().is_none_or(|r| r.changed.is_empty())
            {
                return false;
            }
            for row in rows {
                let Some(actual) = row.after.as_ref() else {
                    return false;
                };
                if row.query_error.is_some() || differences(&row.before, actual) != row.changed {
                    return false;
                }
                if row.cohort == "R" {
                    if compare(Role::Root, &row.before, actual, true, true).is_err() {
                        return false;
                    }
                } else if actual != &row.before {
                    return false;
                }
            }
        }
    }
    true
}

pub(in crate::state) struct LegacyOutcomeInput<'a> {
    pub binding: &'a OriginalCapturedBinding,
    pub inventory: &'a [Inventory],
    pub first: &'a Result<LegacyAudit, AuditFailure>,
    pub teardown: &'a Result<LegacyAudit, AuditFailure>,
    pub trace: Option<&'a LegacyTrace>,
    pub report: &'a Report,
    pub verification: &'a VerificationLog,
    pub post: &'a io::Result<()>,
    pub producer: &'a Result<(), String>,
    pub readback: Option<&'a [u8]>,
    pub expected_content: &'a [u8],
    pub total_rows: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::state) enum DiagnosticPurpose {
    FullLegacyPositive,
    CheckedLegacyFailureReference,
    NotQualified,
}
#[derive(Serialize)]
pub(in crate::state) struct DiagnosticOutcome {
    purpose: DiagnosticPurpose,
    terminal_source_bytes: &'static str,
    terminal_source_cursor: &'static str,
    reason: Option<Reason>,
}
impl DiagnosticOutcome {
    fn new(purpose: DiagnosticPurpose, reason: Option<Reason>) -> Self {
        let content = if purpose == DiagnosticPurpose::FullLegacyPositive {
            "qualified_original_success_snapshot"
        } else {
            "unqualified_not_retained"
        };
        Self {
            purpose,
            terminal_source_bytes: content,
            terminal_source_cursor: content,
            reason,
        }
    }
    pub fn after_output(mut self, written: &io::Result<()>, rows: usize, bytes: usize) -> Self {
        if written.is_err() || rows > TOTAL_ROWS || bytes > TOTAL_BYTES {
            self.purpose = DiagnosticPurpose::NotQualified;
            self.reason = Some(Reason::Output);
            self.terminal_source_bytes = "unqualified_not_retained";
            self.terminal_source_cursor = "unqualified_not_retained";
        }
        self
    }
    pub fn purpose(&self) -> DiagnosticPurpose {
        self.purpose
    }
    pub fn qualified(&self) -> bool {
        self.purpose != DiagnosticPurpose::NotQualified
    }
}
/// Count actual borrowed old rows and sealed new receipts, including failure
/// and purpose-summary rows. References do not erase logical observations.
pub(in crate::state) fn required_rows(input: &LegacyOutcomeInput<'_>) -> usize {
    input.binding.logical_rows()
        + input.inventory.len()
        + input.verification.rows.len()
        + input.verification.phases.len()
        + input.report.logical_rows()
        + input.trace.map_or(1, LegacyTrace::logical_rows)
        + input
            .first
            .as_ref()
            .map_or_else(AuditFailure::logical_rows, LegacyAudit::logical_rows)
        + input
            .teardown
            .as_ref()
            .map_or_else(AuditFailure::logical_rows, LegacyAudit::logical_rows)
        + 1
}
pub(in crate::state) fn classify(input: LegacyOutcomeInput<'_>) -> DiagnosticOutcome {
    let (Ok(first), Ok(final_audit), Some(trace)) = (input.first, input.teardown, input.trace)
    else {
        return DiagnosticOutcome::new(DiagnosticPurpose::NotQualified, Some(Reason::Incomplete));
    };
    let same = input.inventory.len() == first.inventory.len()
        && input
            .inventory
            .iter()
            .zip(&first.inventory)
            .all(|(a, b)| same_inventory(a, b))
        && first.original == *input.binding
        && final_audit.original == *input.binding
        && first.inventory.len() == final_audit.inventory.len()
        && first
            .inventory
            .iter()
            .zip(&final_audit.inventory)
            .all(|(a, b)| same_inventory(a, b))
        && first.until == final_audit.until
        && trace.original_until() == Some(first.until);
    let audit = first.point() == LegacyAuditPoint::TimelyFirst32
        && final_audit.point() == LegacyAuditPoint::Teardown
        && first.started_before_deadline
        && first.finished_before_deadline
        && first.expected == first.checked
        && final_audit.expected == final_audit.checked;
    let required_rows = required_rows(&input);
    let coverage = accounting(required_rows, input.total_rows)
        && first.rows.len() == first.checked
        && final_audit.rows.len() == final_audit.checked
        && first.rows.iter().all(|r| r.complete && r.closure.is_some())
        && final_audit
            .rows
            .iter()
            .all(|r| r.complete && r.closure.is_some());
    let no_drop = !first.physically_released
        && !final_audit.physically_released
        && first.r_held == input.binding.case.roots()
        && final_audit.r_held == input.binding.case.roots()
        && first.retained_held == final_audit.retained_held;
    let negative = PurposeChecks {
        context: same,
        audits: audit && no_drop,
        old: input.post.is_err()
            && old_reference(
                input.binding.case,
                input.binding.root,
                input.verification,
                &first.inventory,
            ),
        trace: trace.check_failure_reference(first.until).is_ok(),
        producer: input.producer.is_err(),
        source: !input.report.positive(),
        coverage,
        budget: input.total_rows <= TOTAL_ROWS,
    };
    if all(negative) {
        return DiagnosticOutcome::new(DiagnosticPurpose::CheckedLegacyFailureReference, None);
    }
    let positive = PurposeChecks {
        context: same,
        audits: audit
            && !first.physically_released
            && first.r_held == input.binding.case.roots()
            && final_audit.physically_released
            && final_audit.r_held == 0
            && first.retained_held == final_audit.retained_held
            && final_audit.started_before_deadline
            && final_audit.finished_before_deadline,
        old: input.post.is_ok()
            && input.verification.complete()
            && first.rows.iter().all(|r| r.changed.is_empty())
            && final_audit.rows.iter().all(|r| r.changed.is_empty()),
        trace: trace.check_full_positive(first.until).is_ok(),
        producer: input.producer.is_ok()
            && input.readback == Some(input.expected_content)
            && input.binding.case != LegacyCase::NoDrop,
        source: input.report.positive(),
        coverage,
        budget: input.total_rows <= TOTAL_ROWS,
    };
    if all(positive) {
        DiagnosticOutcome::new(DiagnosticPurpose::FullLegacyPositive, None)
    } else {
        DiagnosticOutcome::new(DiagnosticPurpose::NotQualified, Some(Reason::Incomplete))
    }
}

// These controls instantiate policy DATA only. They cannot construct the
// sibling's sealed LegacyTrace or turn a JSON packet into a native receipt.
#[cfg(test)]
mod policy_controls {
    use super::*;
    fn native(directory: bool) -> HandleInfo {
        HandleInfo {
            handle: 1,
            object: Object { volume: 2, file: 3 },
            length: 4,
            granted_access: 5,
            mode: 6,
            attributes: if directory { 16 } else { 32 },
            directory,
        }
    }
    #[test]
    fn every_non_root_compared_field_remains_mandatory() {
        type Change = fn(&mut HandleInfo);
        let changes: [Change; 8] = [
            |x| x.handle += 1,
            |x| x.object.volume += 1,
            |x| x.object.file += 1,
            |x| x.length += 1,
            |x| x.granted_access += 1,
            |x| x.mode += 1,
            |x| x.attributes ^= 1,
            |x| x.directory = !x.directory,
        ];
        for role in [Role::Facts, Role::Ancestor, Role::Fact, Role::Writer] {
            let before = native(matches!(role, Role::Facts | Role::Ancestor));
            assert!(compare(role, &before, &before, true, true).is_ok());
            for change in changes {
                let mut after = before.clone();
                change(&mut after);
                assert!(compare(role, &before, &after, true, true).is_err());
            }
        }
    }
    #[test]
    fn root_temporal_difference_is_a_negative_guard_not_full_equality() {
        let before = native(true);
        let mut after = before.clone();
        after.length += 1;
        after.attributes ^= 1;
        assert!(compare(Role::Root, &before, &after, true, true).is_ok());
        assert!(verify_value(&before, &after).is_err());
        assert_eq!(differences(&before, &after), ["length", "attributes"]);
        let changes: [fn(&mut HandleInfo); 7] = [
            |x: &mut HandleInfo| x.handle += 1,
            |x: &mut HandleInfo| x.object.file += 1,
            |x: &mut HandleInfo| x.object.volume += 1,
            |x: &mut HandleInfo| x.granted_access += 1,
            |x: &mut HandleInfo| x.mode += 1,
            |x: &mut HandleInfo| x.directory = false,
            |x: &mut HandleInfo| x.attributes |= REPARSE,
        ];
        for change in changes {
            let mut after = before.clone();
            change(&mut after);
            assert!(compare(Role::Root, &before, &after, true, true).is_err());
        }
    }
    #[test]
    fn privacy_acquisition_and_reparse_never_become_temporal_exceptions() {
        for role in [Role::Root, Role::Facts, Role::Fact, Role::Writer] {
            let before = native(matches!(role, Role::Root | Role::Facts));
            assert!(compare(role, &before, &before, false, true).is_err());
            assert!(compare(role, &before, &before, true, false).is_err());
            let mut reparse = before.clone();
            reparse.attributes |= REPARSE;
            assert!(compare(role, &reparse, &reparse, true, true).is_err());
        }
        let ancestor = native(true);
        assert!(compare(Role::Ancestor, &ancestor, &ancestor, false, true).is_ok());
    }
    #[test]
    fn full_original_file_identity_includes_device_inode_and_both_time_fields() {
        let original = platform::FileIdentity {
            device: 1,
            inode: 2,
            modified: 3,
            modified_nanos: 4,
        };
        assert!(identity_matches(original, original));
        let values = [
            platform::FileIdentity {
                device: 9,
                ..original
            },
            platform::FileIdentity {
                inode: 9,
                ..original
            },
            platform::FileIdentity {
                modified: 9,
                ..original
            },
            platform::FileIdentity {
                modified_nanos: 9,
                ..original
            },
        ];
        for value in values {
            assert!(!identity_matches(value, original));
        }
    }
    #[test]
    fn original_slot_map_rejects_missing_duplicate_changed_and_reused_slots() {
        let roster = vec![inventory(0), inventory(1)];
        assert_eq!(slot_for_inventory(&roster, &roster[0], &[]).unwrap(), 0);
        assert!(slot_for_inventory(&roster, &roster[0], &[0]).is_err());
        let duplicate = vec![roster[0].clone(), roster[0].clone()];
        assert!(slot_for_inventory(&duplicate, &roster[0], &[]).is_err());
        let mut missing = roster[0].clone();
        missing.native.handle = 99;
        assert!(slot_for_inventory(&roster, &missing, &[]).is_err());
        let mut changed = roster[0].clone();
        changed.requested_share ^= 2;
        assert!(slot_for_inventory(&roster, &changed, &[]).is_err());
        let large = vec![roster[0].clone(); MAX_HELD + 1];
        assert!(slot_for_inventory(&large, &roster[0], &[]).is_err());
    }
    #[test]
    fn complete_byte_count_and_sha_are_independent_guards() {
        let bytes = b"bounded original data";
        let hash = digest(bytes);
        assert!(bytes_match(bytes.len(), hash, bytes.len(), hash));
        assert!(!bytes_match(bytes.len() + 1, hash, bytes.len(), hash));
        let mut changed = hash;
        changed[0] ^= 1;
        assert!(!bytes_match(bytes.len(), changed, bytes.len(), hash));
    }
    #[test]
    fn every_actual_purpose_guard_is_sensitive_and_incomplete_never_passes() {
        let complete = PurposeChecks {
            context: true,
            audits: true,
            old: true,
            trace: true,
            producer: true,
            source: true,
            coverage: true,
            budget: true,
        };
        type Guard = fn(&mut PurposeChecks);
        let guards: [Guard; 8] = [
            |c| c.context = false,
            |c| c.audits = false,
            |c| c.old = false,
            |c| c.trace = false,
            |c| c.producer = false,
            |c| c.source = false,
            |c| c.coverage = false,
            |c| c.budget = false,
        ];
        assert!(all(complete));
        for remove in guards {
            let mut incomplete = complete;
            remove(&mut incomplete);
            assert!(!all(incomplete));
        }
    }
    fn inventory(slot: usize) -> Inventory {
        let mut info = native(true);
        info.handle += slot as u64;
        info.object.file += slot as u64;
        let (requested_access, requested_share, acquisition) = Acquisition::OpenedDirectory.masks();
        Inventory {
            origin: format!("model-slot-{slot}"),
            ownership: "synthetic-test-owned",
            acquisition,
            requested_access,
            requested_share,
            duplicated_original: false,
            native: info,
        }
    }
    fn old_model(case: LegacyCase) -> (VerificationLog, Vec<Inventory>) {
        let mut inventory: Vec<_> = (0..case.roots() + 3).map(inventory).collect();
        let root = inventory[0].native.object;
        for row in inventory.iter_mut().take(case.roots()) {
            row.native.object = root;
        }
        let mut log = VerificationLog {
            r_held: case.roots(),
            retained_held: 3,
            ..VerificationLog::default()
        };
        let i = log.begin(Phase::Initial, inventory.len());
        for row in &inventory {
            log.record(
                i,
                Phase::Initial,
                &row.origin,
                if row.native.object == root {
                    "R"
                } else {
                    "retained"
                },
                &row.native,
                &Ok(row.native.clone()),
            );
        }
        log.finish(i, &Ok(()));
        let phases = if case == LegacyCase::NoDrop {
            vec![Phase::AfterProducer]
        } else {
            vec![Phase::BeforeRelease, Phase::AfterProducer]
        };
        for phase in phases {
            let i = log.begin(phase, inventory.len());
            let row = &inventory[0];
            let mut after = row.native.clone();
            after.length += 1;
            log.record(i, phase, &row.origin, "R", &row.native, &Ok(after));
            log.finish(i, &Err(io::Error::other("model full comparison refusal")));
        }
        (log, inventory)
    }
    #[test]
    fn full_false_oracle_keeps_short_circuit_unknowns_and_exact_case_phases() {
        for case in [
            LegacyCase::Fresh,
            LegacyCase::Reopened,
            LegacyCase::ExtraAliases,
            LegacyCase::NoDrop,
        ] {
            let (log, inventory) = old_model(case);
            assert!(old_reference(
                case,
                inventory[0].native.object,
                &log,
                &inventory
            ));
            assert!(!log.complete());
            assert!(log.phases.last().unwrap().not_checked > 0);
            let mut changed = log.clone();
            changed.phases.pop();
            assert!(!old_reference(
                case,
                inventory[0].native.object,
                &changed,
                &inventory
            ));
            let mut changed = log.clone();
            changed.phases.last_mut().unwrap().ok = Some(true);
            assert!(!old_reference(
                case,
                inventory[0].native.object,
                &changed,
                &inventory
            ));
            let mut changed = log.clone();
            changed.rows.last_mut().unwrap().changed.clear();
            assert!(!old_reference(
                case,
                inventory[0].native.object,
                &changed,
                &inventory
            ));
            let mut changed = log.clone();
            changed
                .rows
                .last_mut()
                .unwrap()
                .after
                .as_mut()
                .unwrap()
                .mode += 1;
            assert!(!old_reference(
                case,
                inventory[0].native.object,
                &changed,
                &inventory
            ));
            let mut changed = log.clone();
            changed.rows.last_mut().unwrap().cohort = "unknown";
            assert!(!old_reference(
                case,
                inventory[0].native.object,
                &changed,
                &inventory
            ));
            let mut changed = log.clone();
            changed.rows.last_mut().unwrap().origin = "missing-slot".into();
            assert!(!old_reference(
                case,
                inventory[0].native.object,
                &changed,
                &inventory
            ));
            let mut changed = log.clone();
            changed.rows.last_mut().unwrap().after = None;
            assert!(!old_reference(
                case,
                inventory[0].native.object,
                &changed,
                &inventory
            ));
            let mut changed = log.clone();
            changed.row_overflow = true;
            assert!(!old_reference(
                case,
                inventory[0].native.object,
                &changed,
                &inventory
            ));
            let mut changed = log.clone();
            changed.phase_overflow = true;
            assert!(!old_reference(
                case,
                inventory[0].native.object,
                &changed,
                &inventory
            ));
            let mut changed = log.clone();
            changed.seen_rows += 1;
            assert!(!old_reference(
                case,
                inventory[0].native.object,
                &changed,
                &inventory
            ));
            let mut changed = log.clone();
            changed.r_held += 1;
            assert!(!old_reference(
                case,
                inventory[0].native.object,
                &changed,
                &inventory
            ));
            let mut changed = log.clone();
            changed.released_by_diagnostic = true;
            assert!(!old_reference(
                case,
                inventory[0].native.object,
                &changed,
                &inventory
            ));
        }
    }
    #[test]
    fn unknown_slot_acquisition_masks_and_duplication_refuse() {
        let mut row = inventory(0);
        row.origin = "capture-root/primary".into();
        assert!(provenance(&row, Role::Root, LegacyCase::Fresh));
        let mut bad = row.clone();
        bad.requested_share ^= 2;
        assert!(!provenance(&bad, Role::Root, LegacyCase::Fresh));
        let mut bad = row.clone();
        bad.requested_access ^= 1;
        assert!(!provenance(&bad, Role::Root, LegacyCase::Fresh));
        let mut bad = row.clone();
        bad.origin = "unknown".into();
        assert!(!provenance(&bad, Role::Root, LegacyCase::Fresh));
        let mut bad = row.clone();
        bad.duplicated_original = true;
        assert!(!provenance(&bad, Role::Root, LegacyCase::Fresh));
        let mut bad = row.clone();
        bad.acquisition = "unknown";
        assert!(!provenance(&bad, Role::Root, LegacyCase::Fresh));
        assert!(!provenance(&row, Role::Ancestor, LegacyCase::Fresh));
    }
    #[test]
    fn first32_expiry_is_not_teardown_and_does_not_invoke_io() {
        let until = Instant::now() - std::time::Duration::from_secs(1);
        let calls = std::cell::Cell::new(0);
        let timely = Clock {
            point: Some(LegacyAuditPoint::TimelyFirst32),
            until,
            queries: Cell::new(0),
        };
        assert!(timely
            .query(|| {
                calls.set(1);
                Ok(())
            })
            .is_err());
        assert_eq!(calls.get(), 0);
        let teardown = Clock {
            point: Some(LegacyAuditPoint::Teardown),
            until,
            queries: Cell::new(0),
        };
        assert!(teardown
            .query(|| {
                calls.set(1);
                Ok(())
            })
            .is_ok());
        assert_eq!(calls.get(), 1);
        assert!(Instant::now() >= until); // actual late teardown, never relabelled timely
    }
    #[test]
    fn actual_row_undercount_and_cap_are_not_repaired_by_references() {
        assert!(accounting(127, 128));
        assert!(!accounting(128, 127));
        assert!(!accounting(129, 129));
    }
    #[test]
    fn expiry_or_query_failure_after_entry_never_returns_a_timely_guard() {
        let until = Instant::now() + std::time::Duration::from_millis(1);
        let clock = Clock {
            point: Some(LegacyAuditPoint::TimelyFirst32),
            until,
            queries: Cell::new(0),
        };
        let result = clock.query(|| {
            while Instant::now() < until {
                std::hint::spin_loop();
            }
            Ok(())
        });
        assert!(result.is_err());
        let clock = Clock {
            point: None,
            until,
            queries: Cell::new(0),
        };
        assert!(clock
            .query(|| Err::<(), _>(io::Error::from_raw_os_error(5)))
            .is_err());
    }
    #[test]
    fn terminal_content_is_not_claimed_by_negative_or_failed_output() {
        let good = DiagnosticOutcome::new(DiagnosticPurpose::CheckedLegacyFailureReference, None);
        let row = serde_json::to_value(&good).unwrap();
        assert_eq!(row["terminal_source_bytes"], "unqualified_not_retained");
        assert_eq!(row["terminal_source_cursor"], "unqualified_not_retained");
        assert!(good
            .after_output(&Ok(()), TOTAL_ROWS, TOTAL_BYTES)
            .qualified());
        for (rows, bytes, error) in [
            (TOTAL_ROWS + 1, 0, false),
            (0, TOTAL_BYTES + 1, false),
            (0, 0, true),
        ] {
            let output = if error {
                Err(io::Error::other("model output error"))
            } else {
                Ok(())
            };
            assert!(!DiagnosticOutcome::new(
                DiagnosticPurpose::CheckedLegacyFailureReference,
                None
            )
            .after_output(&output, rows, bytes)
            .qualified());
        }
    }
    #[test]
    fn historical_saved_packets_without_sealed_audits_and_terminal_trace_are_unqualified() {
        // All four OLD reports have precisely this missing-certificate state.
        // Their false/inconclusive measurements remain literal; no JSON mint.
        for case in [
            LegacyCase::Fresh,
            LegacyCase::Reopened,
            LegacyCase::ExtraAliases,
            LegacyCase::NoDrop,
        ] {
            let (log, inventory) = old_model(case);
            assert!(!log.complete());
            let complete = PurposeChecks {
                context: true,
                audits: false,
                old: old_reference(case, inventory[0].native.object, &log, &inventory),
                trace: false,
                producer: true,
                source: true,
                coverage: false,
                budget: true,
            };
            assert!(!all(complete));
        }
    }
}

// Native reader-position safety controls are unrun by the component author.
// They are not the ordinary atomic producer or a compatibility experiment.
#[cfg(test)]
mod reader_controls {
    use super::*;
    use std::io::Write;
    #[test]
    fn same_original_reader_is_restored_on_success_bound_and_read_error() {
        let mut original = tempfile::NamedTempFile::new().unwrap();
        original.write_all(b"fact").unwrap();
        original.seek(SeekFrom::Start(2)).unwrap();
        let clock = Clock {
            point: None,
            until: Instant::now(),
            queries: Cell::new(0),
        };
        assert_eq!(read_bounded(original.as_file(), &clock).unwrap(), b"fact");
        assert_eq!(original.stream_position().unwrap(), 2);
        original.as_file().set_len((FILE_BYTES + 1) as u64).unwrap();
        assert!(read_bounded(original.as_file(), &clock).is_err());
        assert_eq!(original.stream_position().unwrap(), 2);
        let dir = tempfile::tempdir().unwrap();
        let mut write_only = File::options()
            .write(true)
            .create_new(true)
            .open(dir.path().join("write-only"))
            .unwrap();
        write_only.write_all(b"original").unwrap();
        write_only.seek(SeekFrom::Start(3)).unwrap();
        assert!(read_bounded(&write_only, &clock).is_err());
        assert_eq!(write_only.stream_position().unwrap(), 3);
    }
    #[test]
    fn original_reader_restore_guard_is_armed_during_unwind() {
        let mut original = tempfile::NamedTempFile::new().unwrap();
        original.write_all(b"fact").unwrap();
        original.seek(SeekFrom::Start(2)).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = Restore {
                file: original.as_file(),
                position: 2,
                armed: true,
            };
            let mut file = original.as_file();
            file.seek(SeekFrom::Start(1)).unwrap();
            panic!("model unwind");
        }));
        assert!(result.is_err());
        assert_eq!(original.stream_position().unwrap(), 2);
    }
}
