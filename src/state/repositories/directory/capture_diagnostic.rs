//! Owned synthetic diagnostic parts only. Consuming a Directory here never
//! yields replacement authority or a reconstructible Captured proof.
#![cfg(all(test, windows))]
#[cfg(all(test, windows))]
#[path = "capture_diagnostic/staged_probe.rs"]
pub(in crate::state) mod staged_probe;

#[cfg(all(test, windows))]
#[path = "capture_diagnostic/legacy_reference.rs"]
pub(in crate::state) mod legacy_reference;

use super::Directory;
use crate::platform::persist_diagnostic::{handle_info, HandleInfo, Object};
use serde::Serialize;
use std::{cell::RefCell, fs::File, io, time::Instant};
use windows_sys::Win32::{
    Foundation::GENERIC_READ,
    Storage::FileSystem::{
        FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_TRAVERSE, READ_CONTROL,
        SYNCHRONIZE,
    },
};

const ROWS: usize = 128;
const PHASES: usize = 8;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Initial,
    BeforeRelease,
    AfterRelease,
    AfterProducer,
}

#[derive(Clone, Debug, Serialize)]
pub(in crate::state) struct QueryError {
    kind: String,
    code: Option<i32>,
    message: String,
}
impl QueryError {
    pub fn of(error: &io::Error) -> Self {
        let mut message = error.to_string();
        if message.len() > 256 {
            let mut end = 240;
            while !message.is_char_boundary(end) {
                end -= 1;
            }
            message.truncate(end);
            message.push_str(" [truncated]");
        }
        Self {
            kind: format!("{:?}", error.kind()),
            code: error.raw_os_error(),
            message,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
struct CheckRow {
    phase: Phase,
    origin: String,
    cohort: &'static str,
    before: HandleInfo,
    after: Option<HandleInfo>,
    changed: Vec<&'static str>,
    query_error: Option<QueryError>,
}
#[derive(Clone, Debug, Serialize)]
struct PhaseRow {
    phase: Phase,
    expected: usize,
    checked: usize,
    not_checked: usize,
    ok: Option<bool>,
    error: Option<QueryError>,
}
#[derive(Clone, Debug, Default, Serialize)]
pub(in crate::state) struct VerificationLog {
    rows: Vec<CheckRow>,
    phases: Vec<PhaseRow>,
    seen_rows: usize,
    row_overflow: bool,
    phase_overflow: bool,
    r_held: usize,
    retained_held: usize,
    released_by_diagnostic: bool,
}
impl VerificationLog {
    fn begin(&mut self, phase: Phase, expected: usize) -> Option<usize> {
        if self.phases.len() == PHASES || self.phases.len() + self.rows.len() >= ROWS {
            self.phase_overflow = true;
            return None;
        }
        self.phases.push(PhaseRow {
            phase,
            expected,
            checked: 0,
            not_checked: expected,
            ok: None,
            error: None,
        });
        Some(self.phases.len() - 1)
    }
    fn record(
        &mut self,
        index: Option<usize>,
        phase: Phase,
        origin: &str,
        cohort: &'static str,
        before: &HandleInfo,
        actual: &io::Result<HandleInfo>,
    ) {
        self.seen_rows = self.seen_rows.saturating_add(1);
        if let Some(index) = index {
            let row = &mut self.phases[index];
            row.checked += 1;
            row.not_checked = row.expected.saturating_sub(row.checked);
        }
        if self.rows.len() + self.phases.len() >= ROWS {
            self.row_overflow = true;
            return;
        }
        self.rows.push(CheckRow {
            phase,
            origin: origin.into(),
            cohort,
            before: before.clone(),
            after: actual.as_ref().ok().cloned(),
            changed: actual
                .as_ref()
                .map(|after| differences(before, after))
                .unwrap_or_default(),
            query_error: actual.as_ref().err().map(QueryError::of),
        });
    }
    fn finish(&mut self, index: Option<usize>, result: &io::Result<()>) {
        if let Some(index) = index {
            self.phases[index].ok = Some(result.is_ok());
            self.phases[index].error = result.as_ref().err().map(QueryError::of);
        }
    }
    pub fn complete(&self) -> bool {
        !self.phases.is_empty()
            && !self.row_overflow
            && !self.phase_overflow
            && self.seen_rows == self.rows.len()
            && self
                .phases
                .iter()
                .all(|p| p.ok == Some(true) && p.checked == p.expected)
    }
}
fn differences(before: &HandleInfo, after: &HandleInfo) -> Vec<&'static str> {
    let mut fields = Vec::new();
    macro_rules! field {
        ($name:literal, $old:expr, $new:expr) => {
            if $old != $new {
                fields.push($name);
            }
        };
    }
    field!("handle", before.handle, after.handle);
    field!("object.volume", before.object.volume, after.object.volume);
    field!("object.file", before.object.file, after.object.file);
    field!("length", before.length, after.length);
    field!(
        "granted_access",
        before.granted_access,
        after.granted_access
    );
    field!("mode", before.mode, after.mode);
    field!("attributes", before.attributes, after.attributes);
    field!("directory", before.directory, after.directory);
    if before != after && fields.is_empty() {
        fields.push("unreported_compared_field");
    }
    fields
}
fn verify_value(before: &HandleInfo, after: &HandleInfo) -> io::Result<()> {
    // The ORIGINAL complete comparison and refusal, with no field exemption.
    if after != before {
        return Err(io::Error::other(
            "retained diagnostic object/access/mode changed",
        ));
    }
    Ok(())
}

const HANDLES: usize = 256;
#[derive(Clone, Copy)]
pub(in crate::state) enum Acquisition {
    OpenedDirectory,
    CreatedDirectory,
    PrivateReader,
}
impl Acquisition {
    fn masks(self) -> (u32, u32, &'static str) {
        match self {
            Self::OpenedDirectory => (GENERIC_READ, FILE_SHARE_READ, "Directory::open"),
            Self::CreatedDirectory => (
                FILE_LIST_DIRECTORY
                    | FILE_TRAVERSE
                    | FILE_READ_ATTRIBUTES
                    | READ_CONTROL
                    | SYNCHRONIZE,
                FILE_SHARE_READ,
                "Directory::create",
            ),
            // std OpenOptions' default R/W/DELETE sharing; acquisition is
            // read_private on the original fact/writer, never a new open here.
            Self::PrivateReader => (GENERIC_READ, 7, "Directory::read_private"),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub(in crate::state) struct Inventory {
    pub origin: String,
    pub ownership: &'static str,
    pub acquisition: &'static str,
    pub requested_access: u32,
    pub requested_share: u32,
    pub duplicated_original: bool,
    pub native: HandleInfo,
}
pub(in crate::state) struct Held {
    file: File,
    pub inventory: Inventory,
}
impl Held {
    pub fn new(file: File, origin: String, acquisition: Acquisition) -> io::Result<Self> {
        if origin.len() > 128 {
            return Err(io::Error::other("diagnostic origin bound"));
        }
        let (requested_access, requested_share, acquisition) = acquisition.masks();
        let native = handle_info(&file)?;
        Ok(Self {
            file,
            inventory: Inventory {
                origin,
                ownership: "synthetic-test-owned",
                acquisition,
                requested_access,
                requested_share,
                duplicated_original: false,
                native,
            },
        })
    }
    pub fn verify(&self) -> io::Result<()> {
        if handle_info(&self.file)? != self.inventory.native {
            return Err(io::Error::other(
                "retained diagnostic object/access/mode changed",
            ));
        }
        Ok(())
    }
    /// Extra directory-handle alias only. Never duplicates a persist source,
    /// fact, writer or locked open-file description.
    pub fn duplicate_root_alias(&self, root: Object) -> io::Result<Self> {
        self.verify()?;
        if self.inventory.native.object != root || !self.inventory.native.directory {
            return Err(io::Error::other(
                "only the synthetic R directory may be aliased",
            ));
        }
        let file = self.file.try_clone()?;
        let mut inventory = self.inventory.clone();
        inventory.origin = "extra-duplicate-R".into();
        inventory.duplicated_original = true;
        inventory.native = handle_info(&file)?;
        Ok(Self { file, inventory })
    }
}
impl Directory {
    /// Moves ALL original handles, including implicit R ancestor aliases.
    /// No reopen/duplicate/drop or production field/shape change occurs here.
    pub(in crate::state) fn into_capture_diagnostic_parts(
        self,
        origin: &str,
        acquisition: Acquisition,
    ) -> io::Result<Vec<Held>> {
        // Captured was verified before decomposition. Inventory never walks
        // names or opens a replacement handle; privacy uses this original.
        if !self.private().map_err(io::Error::other)? {
            return Err(io::Error::other("diagnostic directory privacy changed"));
        }
        let Directory {
            path: _,
            file,
            _ancestors,
        } = self;
        if _ancestors.len() + 1 > HANDLES {
            return Err(io::Error::other("diagnostic directory inventory bound"));
        }
        let mut parts = vec![Held::new(file, format!("{origin}/primary"), acquisition)?];
        for (n, file) in _ancestors.into_iter().enumerate() {
            parts.push(Held::new(
                file,
                format!("{origin}/ancestor-{n}"),
                Acquisition::OpenedDirectory,
            )?);
        }
        Ok(parts)
    }
}

/// The complete known, disposable test-owned R cohort. All other original
/// objects remain owned here through the actual producer and read-back.
pub(in crate::state) struct Cohort {
    root: Object,
    roots: Vec<Held>,
    retained: Vec<Held>,
    inventory: Vec<Inventory>,
    expected_roots: usize,
    released: bool,
    verification: RefCell<VerificationLog>,
}
impl Cohort {
    pub fn new(root: Object, handles: Vec<Held>, expected_roots: usize) -> io::Result<Self> {
        if handles.len() > HANDLES || expected_roots == 0 {
            return Err(io::Error::other("diagnostic cohort bound"));
        }
        let inventory = handles.iter().map(|h| h.inventory.clone()).collect();
        let (roots, retained): (Vec<_>, Vec<_>) = handles
            .into_iter()
            .partition(|h| h.inventory.native.object == root);
        let this = Self {
            root,
            roots,
            retained,
            inventory,
            expected_roots,
            released: false,
            verification: RefCell::new(VerificationLog::default()),
        };
        this.verify_at(Phase::Initial)?;
        Ok(this)
    }
    pub fn inventory(&self) -> &[Inventory] {
        &self.inventory
    }
    pub fn verification_log(&self) -> VerificationLog {
        let mut log = self.verification.borrow().clone();
        log.r_held = self.roots.len();
        log.retained_held = self.retained.len();
        log.released_by_diagnostic = self.released;
        log
    }
    pub fn verify_inventory(&self) -> io::Result<()> {
        self.verify_at(Phase::AfterProducer)
    }
    fn verify_at(&self, phase: Phase) -> io::Result<()> {
        let index = self
            .verification
            .borrow_mut()
            .begin(phase, self.roots.len() + self.retained.len());
        let result =
            (|| {
                if !self.released && self.roots.len() != self.expected_roots {
                    return Err(io::Error::other(
                        "incomplete or unexpected same-native-R inventory",
                    ));
                }
                if self.released && !self.roots.is_empty() {
                    return Err(io::Error::other("partial R release"));
                }
                for held in self.roots.iter().chain(&self.retained) {
                    // Reuse this SAME existing query result for both evidence and the
                    // unchanged full comparison. No second query or callback output.
                    let actual = handle_info(&held.file);
                    let role = if held.inventory.native.object == self.root {
                        "R"
                    } else {
                        "retained"
                    };
                    self.verification.borrow_mut().record(
                        index,
                        phase,
                        &held.inventory.origin,
                        role,
                        &held.inventory.native,
                        &actual,
                    );
                    verify_value(&held.inventory.native, &actual?)?;
                }
                if self.roots.iter().any(|h| {
                    !h.inventory.native.directory || h.inventory.native.object != self.root
                }) || self
                    .retained
                    .iter()
                    .any(|h| h.inventory.native.object == self.root)
                {
                    return Err(io::Error::other("same-native-R partition changed"));
                }
                Ok(())
            })();
        self.verification.borrow_mut().finish(index, &result);
        result
    }
    pub fn release_all_r(&mut self, until: Instant) -> io::Result<()> {
        if self.released {
            return Err(io::Error::other("diagnostic R cohort already released"));
        }
        self.verify_at(Phase::BeforeRelease)?;
        // Inventory work is charged to the original producer deadline too.
        // Do not discard an R object after that budget has expired.
        if Instant::now() >= until {
            return Err(io::Error::other(
                "original deadline exhausted before R release",
            ));
        }
        self.roots.clear(); // ONLY these synthetic owned R handles.
        self.released = true;
        self.verify_at(Phase::AfterRelease)
    }
}

#[cfg(test)]
mod report_tests {
    use super::*;
    fn native() -> HandleInfo {
        HandleInfo {
            handle: 1,
            object: Object { volume: 2, file: 3 },
            length: 4,
            granted_access: 5,
            mode: 6,
            attributes: 7,
            directory: false,
        }
    }
    #[test]
    fn every_compared_field_retains_before_after_and_the_original_refusal() {
        type FieldChange = (&'static str, fn(&mut HandleInfo));
        let changes: [FieldChange; 8] = [
            ("handle", |x| x.handle += 1),
            ("object.volume", |x| x.object.volume += 1),
            ("object.file", |x| x.object.file += 1),
            ("length", |x| x.length += 1),
            ("granted_access", |x| x.granted_access += 1),
            ("mode", |x| x.mode += 1),
            ("attributes", |x| x.attributes += 1),
            ("directory", |x| x.directory = true),
        ];
        for (field, change) in changes {
            let before = native();
            let mut after = before.clone();
            change(&mut after);
            let mut log = VerificationLog::default();
            let index = log.begin(Phase::BeforeRelease, 2);
            log.record(
                index,
                Phase::BeforeRelease,
                "fixture-R",
                "R",
                &before,
                &Ok(after.clone()),
            );
            let checked = verify_value(&before, &after);
            log.finish(index, &checked);
            assert!(checked.is_err());
            assert!(!log.complete());
            assert_eq!(log.rows[0].before, before);
            assert_eq!(log.rows[0].after.as_ref(), Some(&after));
            assert_eq!(log.rows[0].changed, [field]);
            assert_eq!(log.phases[0].not_checked, 1);
        }
    }
    #[test]
    fn callback_query_failure_keeps_its_stage_without_a_fabricated_after() {
        let mut log = VerificationLog::default();
        let index = log.begin(Phase::BeforeRelease, 2);
        let actual = Err(io::Error::from_raw_os_error(5));
        log.record(
            index,
            Phase::BeforeRelease,
            "fixture-ancestor",
            "retained",
            &native(),
            &actual,
        );
        let checked = actual.map(|_| ());
        log.finish(index, &checked);
        assert!(checked.is_err());
        assert!(!log.complete());
        assert!(log.rows[0].after.is_none());
        assert!(log.rows[0].changed.is_empty());
        assert_eq!(log.rows[0].query_error.as_ref().unwrap().code, Some(5));
        assert_eq!(log.phases[0].not_checked, 1);
        assert_eq!(
            serde_json::to_value(&log).unwrap()["rows"][0]["phase"],
            "before_release"
        );
    }
    #[test]
    fn row_and_stage_overflow_are_bounded_and_incomplete() {
        let mut log = VerificationLog::default();
        let index = log.begin(Phase::Initial, 256);
        for _ in 0..256 {
            log.record(
                index,
                Phase::Initial,
                "fixture",
                "R",
                &native(),
                &Ok(native()),
            );
        }
        log.finish(index, &Ok(()));
        assert!(log.row_overflow);
        assert!(!log.complete());
        assert!(log.rows.len() + log.phases.len() <= ROWS);
        assert_eq!(log.seen_rows, 256);
        for _ in 0..16 {
            let i = log.begin(Phase::AfterProducer, 0);
            log.finish(i, &Ok(()));
        }
        assert!(log.phase_overflow);
        assert!(log.rows.len() + log.phases.len() <= ROWS);
    }
    #[test]
    fn complete_check_keeps_all_fields_and_empty_journal_is_not_complete() {
        let mut log = VerificationLog::default();
        assert!(!log.complete());
        let index = log.begin(Phase::AfterProducer, 1);
        log.record(
            index,
            Phase::AfterProducer,
            "fixture",
            "retained",
            &native(),
            &Ok(native()),
        );
        log.finish(index, &verify_value(&native(), &native()));
        assert!(log.complete());
        assert!(log.rows[0].changed.is_empty());
        let row = serde_json::to_value(&log).unwrap();
        assert_eq!(row["rows"][0]["before"], row["rows"][0]["after"]);
    }
}
