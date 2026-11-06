//! Test-only Windows sharing diagnostics. No stop/restart API is called.
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
};
thread_local! {
    static OPERATION: RefCell<Option<(&'static str, Vec<PathBuf>)>> = const { RefCell::new(None) };
}
pub(super) fn begin() {
    if !matches!(
        std::env::var("HGS_TEST_ARCHIVE_REFERENCES").as_deref(),
        Ok("writers" | "sharing-self-holder")
    ) {
        return;
    }
    OPERATION.with(|slot| *slot.borrow_mut() = Some(("normalize_record", Vec::new())));
}
pub(super) fn end() {
    OPERATION.with(|slot| *slot.borrow_mut() = None);
}
pub(super) fn stage(operation: &'static str, paths: &[&Path]) {
    OPERATION.with(|slot| {
        if let Some(current) = slot.borrow_mut().as_mut() {
            *current = (
                operation,
                paths.iter().map(|path| path.to_path_buf()).collect(),
            );
        }
    });
}
pub(super) fn failure(error: &str) {
    OPERATION.with(|slot| {
        let Some((operation, paths)) = slot.borrow_mut().take() else {
            return;
        };
        eprintln!(
            "archive_writer_failure pid={} operation={operation} paths={paths:?} error={error}",
            std::process::id()
        );
        #[cfg(windows)]
        if error.contains("os error 32") {
            for path in paths.iter().take(2) {
                let _ = owners(path);
            }
        }
    });
}
#[cfg(windows)]
fn owners(path: &Path) -> Option<Vec<u32>> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::RestartManager::{
        RmEndSession, RmGetList, RmRegisterResources, RmStartSession, RM_PROCESS_INFO,
    };
    let mut session = 0;
    let mut key = [0u16; 33];
    let result = unsafe { RmStartSession(&mut session, 0, key.as_mut_ptr()) };
    if result != 0 {
        eprintln!("archive_writer_owners path={path:?} start_error={result}");
        return None;
    }
    struct Session(u32);
    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                RmEndSession(self.0);
            }
        }
    }
    let _session = Session(session);
    let name = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let names = [name.as_ptr()];
    let result = unsafe {
        RmRegisterResources(
            session,
            1,
            names.as_ptr(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
        )
    };
    if result != 0 {
        eprintln!("archive_writer_owners path={path:?} register_error={result}");
        return None;
    }
    // One fixed-size observation. ERROR_MORE_DATA is unavailable, never empty.
    let mut rows: [RM_PROCESS_INFO; 32] = unsafe { std::mem::zeroed() };
    let (mut needed, mut count, mut reasons) = (0, rows.len() as u32, 0);
    let result = unsafe {
        RmGetList(
            session,
            &mut needed,
            &mut count,
            rows.as_mut_ptr(),
            &mut reasons,
        )
    };
    eprintln!("archive_writer_owners path={path:?} result={result} needed={needed} count={count}");
    if result == 0 && count <= rows.len() as u32 {
        let mut pids = Vec::new();
        for row in rows.iter().take(count as usize) {
            pids.push(row.Process.dwProcessId);
            let length = row
                .strAppName
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(row.strAppName.len());
            let birth = (u64::from(row.Process.ProcessStartTime.dwHighDateTime) << 32)
                | u64::from(row.Process.ProcessStartTime.dwLowDateTime);
            eprintln!(
                "archive_writer_owner pid={} birth={birth} app={:?} kind={}",
                row.Process.dwProcessId,
                String::from_utf16_lossy(&row.strAppName[..length]),
                row.ApplicationType
            );
        }
        return Some(pids);
    }
    None
}

#[cfg(windows)]
pub(super) fn known_holder(path: &Path) {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
    let _held = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .unwrap();
    let blocked = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
        .unwrap_err();
    assert_eq!(blocked.raw_os_error(), Some(32));
    let pids = owners(path).expect("the known-holder observation must be available");
    assert!(
        pids.contains(&std::process::id()),
        "the native query must name this exact original holder: {pids:?}"
    );
}
