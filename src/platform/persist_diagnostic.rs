//! G1 diagnostic only: one actual atomic producer, its original source and
//! deadline. No publication backend, retry, sharing or source substitution.
#![cfg(all(test, windows))]

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    ffi::c_void,
    fs::File,
    io::{self, Seek, SeekFrom},
    os::windows::{fs::FileExt, io::AsRawHandle},
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY,
    },
    System::{
        LibraryLoader::{GetModuleHandleW, GetProcAddress},
        IO::IO_STATUS_BLOCK,
    },
};

const CONTENT_BYTES: usize = 4096;
const EVENTS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Object {
    pub volume: u64,
    pub file: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct HandleInfo {
    pub handle: u64,
    pub object: Object,
    pub length: u64,
    pub granted_access: u32,
    pub mode: u32,
    pub attributes: u32,
    pub directory: bool,
}

/// Queries only the supplied original handle. Share masks come from acquisition
/// records: NtQueryObject does not report them. No name is opened or duplicated.
pub(crate) fn handle_info(file: &File) -> io::Result<HandleInfo> {
    #[repr(C)]
    struct Basic {
        attributes: u32,
        access: u32,
        handles: u32,
        pointers: u32,
        reserved: [u32; 10],
    }
    type QueryObject = unsafe extern "system" fn(HANDLE, u32, *mut c_void, u32, *mut u32) -> i32;
    type QueryFile =
        unsafe extern "system" fn(HANDLE, *mut IO_STATUS_BLOCK, *mut c_void, u32, u32) -> i32;
    let name: Vec<u16> = "ntdll.dll\0".encode_utf16().collect();
    // SAFETY: borrowed loaded system module, fixed NUL-terminated symbol names.
    let (object_query, file_query) = unsafe {
        let module = GetModuleHandleW(name.as_ptr());
        if module.is_null() {
            return Err(io::Error::last_os_error());
        }
        let object = GetProcAddress(module, c"NtQueryObject".as_ptr().cast())
            .ok_or_else(|| io::Error::other("native object query unavailable"))?;
        let query = GetProcAddress(module, c"NtQueryInformationFile".as_ptr().cast())
            .ok_or_else(|| io::Error::other("native file query unavailable"))?;
        (
            std::mem::transmute::<unsafe extern "system" fn() -> isize, QueryObject>(object),
            std::mem::transmute::<unsafe extern "system" fn() -> isize, QueryFile>(query),
        )
    };
    let handle = file.as_raw_handle();
    let mut basic: Basic = unsafe { std::mem::zeroed() };
    let mut returned = 0;
    // SAFETY: existing handle and exactly sized writable native structures.
    let status = unsafe {
        object_query(
            handle,
            0,
            (&mut basic as *mut Basic).cast(),
            std::mem::size_of::<Basic>() as u32,
            &mut returned,
        )
    };
    if status != 0 || returned as usize != std::mem::size_of::<Basic>() {
        return Err(io::Error::other(format!(
            "incomplete native access query: status {status:#x}, bytes {returned}"
        )));
    }
    let mut mode = 0u32;
    let mut ios: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
    // FileModeInformation. A pending/short query cannot supply a verified
    // mode. These originals are synchronous; no async buffer is retained.
    let status = unsafe { file_query(handle, &mut ios, (&mut mode as *mut u32).cast(), 4, 16) };
    if status != 0 || unsafe { ios.Anonymous.Status } != 0 || ios.Information != 4 {
        return Err(io::Error::other(format!(
            "incomplete native mode query: status {status:#x}, bytes {}",
            ios.Information
        )));
    }
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(handle, &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(HandleInfo {
        handle: handle as usize as u64,
        object: Object {
            volume: info.dwVolumeSerialNumber.into(),
            file: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        },
        length: (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow),
        granted_access: basic.access,
        mode,
        attributes: info.dwFileAttributes,
        directory: info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0,
    })
}

#[derive(Clone, Debug, Serialize)]
struct Snapshot {
    handle: HandleInfo,
    cursor: u64,
    digest: String,
}
impl Snapshot {
    fn same_source(&self, other: &Self) -> bool {
        // tempfile legitimately changes TEMPORARY/NORMAL attributes. The
        // observer checks attributes within each callback, not across producer
        // phases; it never changes them itself.
        self.handle.handle == other.handle.handle
            && self.handle.object == other.handle.object
            && self.handle.length == other.handle.length
            && self.handle.mode == other.handle.mode
            && self.handle.granted_access == other.handle.granted_access
            && self.cursor == other.cursor
            && self.digest == other.digest
    }
}
struct Restore<'a> {
    file: &'a File,
    cursor: u64,
    armed: bool,
}
impl Restore<'_> {
    fn restore(&mut self) -> io::Result<()> {
        let mut borrowed = self.file;
        borrowed.seek(SeekFrom::Start(self.cursor))?;
        if borrowed.stream_position()? != self.cursor {
            return Err(io::Error::other("source cursor restoration failed"));
        }
        self.armed = false;
        Ok(())
    }
}
impl Drop for Restore<'_> {
    fn drop(&mut self) {
        if self.armed {
            let mut borrowed = self.file;
            let _ = borrowed.seek(SeekFrom::Start(self.cursor));
        }
    }
}
fn snapshot(file: &File, expected: &[u8]) -> io::Result<Snapshot> {
    let mut borrowed = file;
    let cursor = borrowed.stream_position()?;
    let mut restore = Restore {
        file,
        cursor,
        armed: true,
    };
    let before = handle_info(file)?;
    if before.directory
        || !matches!(before.mode & 0x30, 0x10 | 0x20) // synchronous alert/non-alert
        || expected.len() > CONTENT_BYTES
        || before.length != expected.len() as u64
    {
        return Err(io::Error::other("unexpected original source kind/length"));
    }
    let observed = (|| -> io::Result<Vec<u8>> {
        let mut bytes = vec![0; expected.len() + 1];
        let mut at = 0;
        while at < bytes.len() {
            // Windows seek_read DOES move this synchronous file's cursor.
            // The original borrowed handle is exclusively owned by persist;
            // restore before returning on success, error and unwind paths.
            let count = file.seek_read(&mut bytes[at..], at as u64)?;
            if count == 0 {
                break;
            }
            at += count;
        }
        bytes.truncate(at);
        Ok(bytes)
    })();
    let restored = restore.restore();
    restored?;
    let bytes = observed?;
    if bytes != expected || handle_info(file)? != before {
        return Err(io::Error::other(
            "source bytes/handle/object/mode changed during observation",
        ));
    }
    Ok(Snapshot {
        handle: before,
        cursor,
        digest: format!("{:x}", Sha256::digest(bytes)),
    })
}

#[derive(Clone, Debug, Serialize)]
struct Event {
    stage: &'static str,
    error: Option<i32>,
    snapshot: Snapshot,
}
// BEGIN PORTABLE STAGED ENTRY — exact module and its controls can be extracted.
mod staged_entry {
    use serde::Serialize;
    use std::{io, path::Path, time::Instant};
    pub(super) fn target_matches(expected: &Path, actual: &Path) -> bool {
        expected == actual
    }
    /// No source handle, bytes or mutable source is exposed to the entry callback.
    pub(crate) type EntryCallback = Box<dyn FnMut(Instant) -> io::Result<EntryDecision>>;
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum EntryDecision {
        Complete,
        Incomplete,
    }
    #[derive(Clone, Debug, Default, Serialize)]
    pub(super) struct EntryReport {
        pub(super) attempted: bool,
        pub(super) complete: bool,
        pub(super) failure: Option<&'static str>,
        pub(super) error_code: Option<i32>,
        pub(super) deadline_expired: bool,
    }
    // Pure callback driver: also extracted for controls that perform no Windows I/O.
    pub(super) fn invoke_entry(
        mut callback: Option<EntryCallback>,
        verified: bool,
        entries: usize,
        until: Instant,
    ) -> EntryReport {
        let mut entry = EntryReport {
            deadline_expired: Instant::now() >= until,
            ..EntryReport::default()
        };
        if !verified || entries != 1 {
            entry.failure = Some("source_not_verified_or_not_single_entry");
            return entry;
        }
        if Instant::now() >= until {
            entry.failure = Some("entry_deadline");
            entry.deadline_expired = true;
            return entry;
        }
        let Some(mut callback) = callback.take() else {
            entry.failure = Some("entry_callback_already_consumed");
            return entry;
        };
        entry.attempted = true;
        // An entry callback is diagnostic only. Its unwind must not replace the
        // result/control flow of the original ordinary producer.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(until)));
        entry.deadline_expired = Instant::now() >= until;
        match result {
            Ok(Ok(EntryDecision::Complete)) if !entry.deadline_expired => entry.complete = true,
            Ok(Ok(_)) => entry.failure = Some("entry_incomplete_or_expired"),
            Ok(Err(error)) => {
                entry.failure = Some("entry_error");
                entry.error_code = error.raw_os_error();
            }
            Err(_) => entry.failure = Some("entry_panicked"),
        }
        entry
    }

    #[cfg(test)]
    mod controls {
        use super::*;
        use std::{cell::Cell, rc::Rc, time::Duration};
        #[test]
        fn entry_target_is_exact_not_a_label_or_neighbor() {
            assert!(target_matches(
                Path::new("fixture/run.json"),
                Path::new("fixture/run.json")
            ));
            assert!(!target_matches(
                Path::new("fixture/run.json"),
                Path::new("neighbor/run.json")
            ));
        }
        #[test]
        fn verified_single_entry_runs_once_with_the_original_deadline() {
            let calls = Rc::new(Cell::new(0));
            let used = calls.clone();
            let until = Instant::now() + Duration::from_secs(1);
            let callback: EntryCallback = Box::new(move |same| {
                assert_eq!(same, until);
                used.set(used.get() + 1);
                Ok(EntryDecision::Complete)
            });
            let result = invoke_entry(Some(callback), true, 1, until);
            assert!(result.complete);
            assert_eq!(calls.get(), 1);
            assert!(!invoke_entry(None, true, 1, until).complete);
        }
        #[test]
        fn unverified_repeated_or_expired_entry_never_invokes_callback() {
            for (verified, entries, expired) in
                [(false, 1, false), (true, 2, false), (true, 1, true)]
            {
                let calls = Rc::new(Cell::new(0));
                let used = calls.clone();
                let until = if expired {
                    Instant::now() - Duration::from_secs(1)
                } else {
                    Instant::now() + Duration::from_secs(1)
                };
                let result = invoke_entry(
                    Some(Box::new(move |_| {
                        used.set(1);
                        Ok(EntryDecision::Complete)
                    })),
                    verified,
                    entries,
                    until,
                );
                assert!(!result.complete);
                assert_eq!(calls.get(), 0);
            }
        }
        #[test]
        fn errors_incomplete_and_panics_are_contained_as_incomplete() {
            for mode in 0..3 {
                let result = invoke_entry(
                    Some(Box::new(move |_| match mode {
                        0 => Err(io::Error::from_raw_os_error(5)),
                        1 => Ok(EntryDecision::Incomplete),
                        _ => panic!("private entry fixture"),
                    })),
                    true,
                    1,
                    Instant::now() + Duration::from_secs(1),
                );
                assert!(!result.complete);
                assert!(result.failure.is_some());
            }
        }
        #[test]
        fn expired_during_callback_never_completes() {
            let until = Instant::now() + Duration::from_millis(1);
            let result = invoke_entry(
                Some(Box::new(move |_| {
                    while Instant::now() < until {
                        std::hint::spin_loop();
                    }
                    Ok(EntryDecision::Complete)
                })),
                true,
                1,
                until,
            );
            assert!(!result.complete);
            assert!(result.deadline_expired);
        }
    }
}
// END PORTABLE STAGED ENTRY
use staged_entry::{invoke_entry, EntryReport};
pub(crate) use staged_entry::{EntryCallback, EntryDecision};

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct Report {
    entries: usize,
    events: Vec<Event>,
    invalid: Option<String>,
    first_error: Option<i32>,
    released: bool,
    success: bool,
    success_before_deadline: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    entry: Option<EntryReport>,
}
impl Report {
    pub fn positive(&self) -> bool {
        self.entries == 1
            && self.invalid.is_none()
            && self.first_error == Some(32)
            && self.released
            && self.success
            && self.success_before_deadline
            && self.entry.as_ref().is_none_or(|entry| entry.complete)
    }
}
struct Hook {
    target: PathBuf,
    expected: Vec<u8>,
    report: Report,
    release: Option<Box<dyn FnMut(Instant) -> io::Result<()>>>,
    entry: Option<EntryCallback>,
}
thread_local! { static ACTIVE: RefCell<Option<Rc<RefCell<Hook>>>> = const { RefCell::new(None) }; }
pub(crate) struct Scope {
    hook: Rc<RefCell<Hook>>,
}
impl Scope {
    pub fn install(
        target: PathBuf,
        expected: Vec<u8>,
        release: Option<Box<dyn FnMut(Instant) -> io::Result<()>>>,
    ) -> io::Result<Self> {
        if expected.len() > CONTENT_BYTES {
            return Err(io::Error::other("diagnostic source bound"));
        }
        let hook = Rc::new(RefCell::new(Hook {
            target,
            expected,
            report: Report::default(),
            release,
            entry: None,
        }));
        ACTIVE.with(|cell| {
            if cell.borrow().is_some() {
                return Err(io::Error::other("nested persist diagnostic scope"));
            }
            *cell.borrow_mut() = Some(hook.clone());
            Ok(())
        })?;
        Ok(Self { hook })
    }
    /// A distinct opt-in factory; the original install/default path has no
    /// entry callback, no entry queries and no extra serialized report field.
    pub fn install_staged(
        target: PathBuf,
        expected: Vec<u8>,
        release: Option<Box<dyn FnMut(Instant) -> io::Result<()>>>,
        entry: EntryCallback,
    ) -> io::Result<Self> {
        let scope = Self::install(target, expected, release)?;
        {
            let mut hook = scope.hook.borrow_mut();
            hook.entry = Some(entry);
            hook.report.entry = Some(EntryReport::default());
        }
        Ok(scope)
    }
    pub fn report(&self) -> Report {
        self.hook.borrow().report.clone()
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.with(|cell| {
            if cell
                .borrow()
                .as_ref()
                .is_some_and(|x| Rc::ptr_eq(x, &self.hook))
            {
                cell.borrow_mut().take();
            }
        });
    }
}
/// One invocation's observer. None does not inspect a file or native handle.
pub(crate) struct Session {
    hook: Option<Rc<RefCell<Hook>>>,
    baseline: Option<Snapshot>,
    until: Instant,
    source: PathBuf,
}
impl Session {
    pub fn enter(source: &tempfile::NamedTempFile, target: &Path, until: Instant) -> Self {
        let hook = ACTIVE.with(|cell| {
            cell.borrow()
                .as_ref()
                .filter(|x| staged_entry::target_matches(&x.borrow().target, target))
                .cloned()
        });
        let mut this = Self {
            hook,
            baseline: None,
            until,
            source: source.path().into(),
        };
        if let Some(hook) = &this.hook {
            let mut h = hook.borrow_mut();
            h.report.entries += 1;
            if h.report.entries != 1 {
                h.report.invalid = Some("more than one ordinary source".into());
            }
        }
        this.baseline = this.observe(source.as_file(), "entry", None, until);
        this.run_entry();
        this
    }
    fn run_entry(&mut self) {
        let Some(hook) = &self.hook else {
            return;
        };
        let (callback, entries, original_failure) = {
            let mut h = hook.borrow_mut();
            if h.report.entry.is_none() {
                return;
            }
            // Old install returns before any entry work. Latch incomplete
            // before callback; failed source observation never gets a rebase.
            let original_failure = h.report.invalid.clone();
            h.report
                .invalid
                .get_or_insert("staged entry incomplete".into());
            h.report.entry = Some(EntryReport::default());
            (h.entry.take(), h.report.entries, original_failure)
        };
        let entry = invoke_entry(
            callback,
            self.baseline.is_some() && original_failure.is_none(),
            entries,
            self.until,
        );
        let mut h = hook.borrow_mut();
        if entry.complete {
            h.report.invalid = None;
        } else {
            h.report.invalid = original_failure
                .or_else(|| Some(entry.failure.unwrap_or("staged entry incomplete").into()));
        }
        h.report.entry = Some(entry);
    }
    fn invalidate(&self, reason: impl Into<String>) {
        if let Some(hook) = &self.hook {
            hook.borrow_mut()
                .report
                .invalid
                .get_or_insert(reason.into());
        }
    }
    fn observe(
        &self,
        file: &File,
        stage: &'static str,
        error: Option<i32>,
        until: Instant,
    ) -> Option<Snapshot> {
        let hook = self.hook.as_ref()?;
        let expected = {
            let mut h = hook.borrow_mut();
            if h.report.invalid.is_some() {
                return None;
            }
            if h.report.entry.is_some() && (until != self.until || Instant::now() >= until) {
                h.report.invalid = Some("deadline before staged source observation".into());
                return None;
            }
            h.report.invalid = Some("observation did not complete".into());
            h.expected.clone()
        };
        let result = snapshot(file, &expected);
        let mut h = hook.borrow_mut();
        if h.report.entry.is_some() && (until != self.until || Instant::now() >= until) {
            h.report.invalid = Some("deadline after staged source restoration".into());
            return None;
        }
        match result {
            Ok(value)
                if until == self.until
                    && Instant::now() < until
                    && h.report.events.len() < EVENTS =>
            {
                if self
                    .baseline
                    .as_ref()
                    .is_some_and(|old| !old.same_source(&value))
                {
                    h.report.invalid =
                        Some("original source identity/content/cursor changed".into());
                    return None;
                }
                h.report.invalid = None;
                h.report.events.push(Event {
                    stage,
                    error,
                    snapshot: value.clone(),
                });
                Some(value)
            }
            Ok(_) => {
                h.report.invalid = Some("original deadline exhausted or event bound".into());
                None
            }
            Err(error) => {
                h.report.invalid = Some(format!("source observation failed: {error}"));
                None
            }
        }
    }
    pub fn failed(&mut self, error: &tempfile::PersistError, until: Instant) {
        if self.hook.is_none() {
            return;
        }
        if error.file.path() != self.source {
            self.invalidate("original source pathname changed");
            return;
        }
        let code = error.error.raw_os_error();
        if self
            .observe(error.file.as_file(), "persist_error", code, until)
            .is_none()
        {
            return;
        }
        let Some(hook) = &self.hook else {
            return;
        };
        let mut h = hook.borrow_mut();
        let first = h.report.first_error.is_none();
        if first {
            h.report.first_error = code;
        }
        if first && code == Some(32) {
            // All observation/restoration time is charged to the SAME deadline.
            if Instant::now() >= self.until {
                h.report.invalid = Some("deadline exhausted before R release".into());
                return;
            }
            if let Some(release) = h.release.as_mut() {
                match release(self.until) {
                    Ok(()) => h.report.released = true,
                    Err(error) => {
                        h.report.invalid = Some(format!("R inventory/release refused: {error}"))
                    }
                }
            }
        } else if first {
            h.report.invalid = Some("first persist failure was not expected error32".into());
        }
    }
    pub fn succeeded(&mut self, file: &File, until: Instant) {
        if self.observe(file, "persist_success", None, until).is_none() {
            return;
        }
        if let Some(hook) = &self.hook {
            let mut h = hook.borrow_mut();
            h.report.success = true;
            h.report.success_before_deadline = Instant::now() < self.until;
        }
    }
}

// Supplementary observer safety checks. These do NOT establish the ordinary
// atomic producer/cohort contrast; only that separate call site can do so.
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn observation_preserves_the_original_cursor_on_success_and_byte_error() {
        let mut original = tempfile::NamedTempFile::new().unwrap();
        original.write_all(b"fixture").unwrap();
        original.seek(SeekFrom::Start(3)).unwrap();
        let before = handle_info(original.as_file()).unwrap();
        assert!(snapshot(original.as_file(), b"fixture").is_ok());
        assert_eq!(original.stream_position().unwrap(), 3);
        assert!(snapshot(original.as_file(), b"changed").is_err());
        assert_eq!(original.stream_position().unwrap(), 3);
        assert_eq!(handle_info(original.as_file()).unwrap(), before);
    }

    #[test]
    fn observation_restores_after_an_actual_read_access_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut original = File::options()
            .write(true)
            .create_new(true)
            .open(dir.path().join("write-only"))
            .unwrap();
        original.write_all(b"fixture").unwrap();
        original.seek(SeekFrom::Start(2)).unwrap();
        let before = handle_info(&original).unwrap();
        assert!(snapshot(&original, b"fixture").is_err());
        assert_eq!(original.stream_position().unwrap(), 2);
        assert_eq!(handle_info(&original).unwrap(), before);
    }

    #[test]
    fn original_cursor_restoration_is_armed_during_unwind() {
        let mut original = tempfile::NamedTempFile::new().unwrap();
        original.write_all(b"fixture").unwrap();
        original.seek(SeekFrom::Start(2)).unwrap();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _restore = Restore {
                file: original.as_file(),
                cursor: 2,
                armed: true,
            };
            let mut borrowed = original.as_file();
            borrowed.seek(SeekFrom::Start(5)).unwrap();
            panic!("private observer unwind fixture");
        }));
        assert!(outcome.is_err());
        assert_eq!(original.stream_position().unwrap(), 2);
    }
}

#[cfg(test)]
mod staged_scope_compatibility {
    use super::*;
    #[test]
    fn default_scope_has_no_entry_work_or_serialized_entry_field() {
        let scope = Scope::install(PathBuf::from("fixture/report-only"), vec![], None).unwrap();
        assert!(scope.hook.borrow().entry.is_none());
        assert_eq!(
            serde_json::to_string(&scope.report()).unwrap(),
            r#"{"entries":0,"events":[],"invalid":null,"first_error":null,"released":false,"success":false,"success_before_deadline":false}"#
        );
    }
    #[test]
    fn legacy_positive_guard_keeps_its_previous_meaning_without_entry() {
        let report = Report {
            entries: 1,
            first_error: Some(32),
            released: true,
            success: true,
            success_before_deadline: true,
            ..Report::default()
        };
        assert!(report.positive());
        assert!(serde_json::to_value(&report)
            .unwrap()
            .get("entry")
            .is_none());
    }
}
