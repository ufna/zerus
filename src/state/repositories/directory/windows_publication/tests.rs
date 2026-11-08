use super::*;
use std::{io::Read, os::windows::fs::OpenOptionsExt};
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ};

fn private_root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    crate::platform::private_dir(root.path()).unwrap();
    root
}

#[test]
fn original_handle_publication_keeps_pins_and_the_original_file() {
    let temp = private_root();
    let path = crate::platform::canonical(temp.path()).unwrap();
    let directory = Directory::open(&path).unwrap();
    let other_pin = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .share_mode(FILE_SHARE_READ)
        .open(&path)
        .unwrap();
    let mut old = tempfile::NamedTempFile::new_in(&path).unwrap();
    old.write_all(b"path publication").unwrap();
    let refused = old.persist_noclobber(path.join("path-based")).unwrap_err();
    assert!(matches!(refused.error.raw_os_error(), Some(5) | Some(32)));

    let mut original = directory
        .write_new_file("receipt", b"native publication")
        .unwrap();
    let named = directory.read_private("receipt").unwrap();
    assert_eq!(
        crate::platform::FileIdentity::of(&original).unwrap(),
        crate::platform::FileIdentity::of(&named).unwrap()
    );
    use std::io::Seek;
    original.rewind().unwrap();
    let mut bytes = Vec::new();
    original.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"native publication");
    directory.verify().unwrap();
    drop(named);
    drop(original);
    drop(other_pin);
    drop(directory);
    // Causal control: the SAME refused file publishes once the parent pins go.
    refused
        .file
        .persist_noclobber(path.join("path-based"))
        .unwrap();
    assert_eq!(
        std::fs::read(path.join("path-based")).unwrap(),
        b"path publication"
    );
}

#[test]
fn no_replace_and_original_reader_refusal_preserve_the_previous_bytes() {
    let temp = private_root();
    let path = crate::platform::canonical(temp.path()).unwrap();
    let directory = Directory::open(&path).unwrap();
    directory.write_new("receipt", b"original").unwrap();
    assert!(directory.write_new("receipt", b"must not replace").is_err());
    assert_eq!(std::fs::read(path.join("receipt")).unwrap(), b"original");
    let reader = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path.join("receipt"))
        .unwrap();
    assert!(directory.replace_file("receipt", b"reader held").is_err());
    assert_eq!(std::fs::read(path.join("receipt")).unwrap(), b"original");
    directory.verify().unwrap();
    drop(reader);
    directory
        .replace_file("receipt", b"replaced explicitly")
        .unwrap();
    assert_eq!(
        std::fs::read(path.join("receipt")).unwrap(),
        b"replaced explicitly"
    );
    directory.verify().unwrap();
}

#[test]
fn invalid_names_refuse_before_allocating_a_staging_file() {
    let temp = private_root();
    let path = crate::platform::canonical(temp.path()).unwrap();
    let directory = Directory::open(&path).unwrap();
    for name in [
        "",
        ".",
        "..",
        "../foreign",
        "a/b",
        "a\\b",
        "a:b",
        "a\0b",
        "a.",
        "a ",
    ] {
        assert!(directory.write_new(name, b"never published").is_err());
    }
    assert!(directory.write_new(&"x".repeat(256), b"too long").is_err());
    assert_eq!(std::fs::read_dir(&path).unwrap().count(), 0);
    directory.verify().unwrap();
}

#[test]
fn retained_reader_replacement_reports_the_actual_native_boundary() {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_WRITE,
        READ_CONTROL, SYNCHRONIZE,
    };
    // Observation only: no status here stands in for the six original Stop
    // assertions. Keep every reader alive during replacement and observation.
    for role in [
        "actual_private_reader",
        "attributes_only_reader",
        "no_delete_reader",
    ] {
        let temp = private_root();
        let path = crate::platform::canonical(temp.path()).unwrap();
        let parent = Directory::open(&path).unwrap();
        // Match Publisher::open_enabled's ORIGINAL created directory handle,
        // not merely a later reopened GENERIC_READ handle.
        let directory = parent.create("task-stop-facts", "unused").unwrap();
        let name = format!("{}.json", "f".repeat(64));
        directory.write_new(&name, b"original snapshot").unwrap();
        assert_eq!(
            std::fs::read(directory.path.join(&name)).unwrap(),
            b"original snapshot"
        );
        let reader = match role {
            "actual_private_reader" => directory.read_private(&name).unwrap(),
            "attributes_only_reader" => OpenOptions::new()
                .access_mode(FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .open(directory.path.join(&name))
                .unwrap(),
            "no_delete_reader" => OpenOptions::new()
                .read(true)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .share_mode(FILE_SHARE_READ)
                .open(directory.path.join(&name))
                .unwrap(),
            _ => unreachable!(),
        };
        assert!(crate::platform::verify_private_file(&reader).unwrap());
        let before = crate::platform::FileIdentity::of(&reader).unwrap();
        let result = directory.replace_file_retained(&name, b"new snapshot");
        let phase = PHASE.with(std::cell::Cell::get);
        let after = crate::platform::FileIdentity::of(&reader).unwrap();
        assert_eq!((before.device, before.inode), (after.device, after.inode));
        directory.verify().unwrap();
        parent.verify().unwrap();
        let record = serde_json::json!({
            "role": role, "phase": phase, "success": result.is_ok(),
            "error": result.as_ref().err().map(|value| value.chars().take(256).collect::<String>()),
            "original_reader_still_bound": true,
        });
        // Direct bounded stderr output survives libtest's success capture.
        writeln!(std::io::stderr().lock(), "K4WDJ_READER_BOUNDARY {record}").unwrap();
        let expected = if result.is_ok() {
            b"new snapshot".as_slice()
        } else {
            b"original snapshot".as_slice()
        };
        assert_eq!(std::fs::read(directory.path.join(&name)).unwrap(), expected);
        if role == "no_delete_reader" {
            assert!(result.is_err());
        }
        // Explicitly later than the replacement and every observation above.
        drop(result);
        drop(reader);
    }
}
