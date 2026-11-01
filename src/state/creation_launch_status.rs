//! Dormant, bounded original launch observation, before generic CLI bootstrap.
//! No producer, listing, repair, writer, process, transport or current authority.
use crate::cli::{Error, Result};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::io::Write;

const COMMAND: &str = "creation-launch-status";
const CAPABILITY: &str = "creation-launch-status-v1";
#[cfg(any(target_os = "linux", target_os = "macos"))]
const UNAVAILABLE: &str =
    "The original launch observation is unavailable; review the original operation.";
const INVALID: &str = "Use --expect-machine M creation-launch-status --launch-id ID --json.";

fn at(args: &[String], index: usize, word: &str) -> bool {
    args.get(index).is_some_and(|arg| arg == word)
}
// Only command positions select this entry. An unrelated native argv or
// session tag containing the reserved word must retain ordinary routing.
fn reserved_by(at: impl Fn(usize, &str) -> bool, offset: usize) -> bool {
    at(offset, COMMAND)
        || (at(offset, "--help") && at(offset + 1, CAPABILITY))
        || (at(offset, "--expect-machine")
            && (at(offset + 1, COMMAND)
                || at(offset + 2, COMMAND)
                || (at(offset + 2, "--client")
                    && ((at(offset + 3, COMMAND) && at(offset + 4, "--launch-id"))
                        || at(offset + 4, COMMAND)))))
}
fn reserved(args: &[String], offset: usize) -> bool {
    reserved_by(|index, word| at(args, index, word), offset)
}
#[derive(Default)]
struct Argument {
    keyword: Option<&'static str>,
    remote: bool,
    text: Option<String>,
}
fn remote_refusal() -> Error {
    Error::new(
        2,
        "This observation requires a bounded target-side invocation.",
    )
}
// This bounds only our retained probe, NOT args_os construction, the runtime,
// loader or native process-argv storage. It is no original-scope witness.
fn os_arguments(args: impl IntoIterator<Item = std::ffi::OsString>) -> Option<Result<Vec<String>>> {
    let mut slots: [Argument; 10] = std::array::from_fn(|_| Argument::default());
    let mut count = 0;
    let mut retained = 0;
    let mut invalid = false;
    // argv0 is skipped as OS data, without Unicode conversion or identity use.
    for argument in args.into_iter().skip(1).take(10) {
        let slot = &mut slots[count];
        slot.keyword = [
            COMMAND,
            CAPABILITY,
            "--help",
            "--expect-machine",
            "--client",
            "--launch-id",
        ]
        .into_iter()
        .find(|word| argument == std::ffi::OsStr::new(word));
        slot.remote = argument.as_encoded_bytes().first() == Some(&b'@');
        let bytes = argument.as_encoded_bytes().len();
        if bytes <= 256 && bytes <= 1024 - retained {
            if let Some(text) = argument.to_str() {
                // Check both caps BEFORE making any retained String copy.
                slot.text = Some(text.to_owned());
                retained += bytes;
            } else {
                invalid = true;
            }
        } else {
            invalid = true;
        }
        count += 1;
    }
    let at = |index: usize, word: &str| {
        slots
            .get(index)
            .is_some_and(|slot| slot.keyword == Some(word))
    };
    let remote = slots[0].remote && reserved_by(at, 1);
    if !remote && !reserved_by(at, 0) {
        return None;
    }
    if remote {
        return Some(Err(remote_refusal()));
    }
    if invalid || count == slots.len() {
        return Some(Err(Error::new(2, INVALID)));
    }
    Some(Ok(slots
        .into_iter()
        .take(count)
        .map(|slot| slot.text.unwrap())
        .collect()))
}
/// First main entry, before existing Unicode argument conversion/preparation.
pub(crate) fn early_os(args: impl IntoIterator<Item = std::ffi::OsString>) -> Option<Result<i32>> {
    let selected = os_arguments(args)?;
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let _ = selected;
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return Some(Err(unsupported()));
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Some(selected.and_then(|args| {
        // Exactly the same parser/observation, once; never generic fallback.
        early(&args).unwrap_or_else(|| Err(Error::new(2, INVALID)))
    }))
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn unsupported() -> Error {
    Error::new(
        2,
        "Original launch observation is unsupported on this platform.",
    )
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn canonical(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil() && id.to_string() == value)
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
struct Request {
    machine: String,
    launch: String,
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn request(args: &[String]) -> Option<Request> {
    let (machine, launch) = match args {
        [guard, machine, command, flag, launch, json]
            if guard == "--expect-machine"
                && command == COMMAND
                && flag == "--launch-id"
                && json == "--json" =>
        {
            (machine, launch)
        }
        [guard, machine, client, origin, command, flag, launch, json]
            if guard == "--expect-machine"
                && client == "--client"
                && !origin.is_empty()
                && origin.len() <= 80
                && !origin.chars().any(|c| c.is_control() || c.is_whitespace())
                && command == COMMAND
                && flag == "--launch-id"
                && json == "--json" =>
        {
            (machine, launch)
        }
        _ => return None,
    };
    (canonical(machine) && canonical(launch)).then(|| Request {
        machine: machine.clone(),
        launch: launch.clone(),
    })
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn fail() -> Error {
    Error::new(1, UNAVAILABLE)
}
/// Reserved forms return before every generic loader and environment mutation.
pub(crate) fn early(args: &[String]) -> Option<Result<i32>> {
    let remote = args.first().is_some_and(|arg| arg.starts_with('@')) && reserved(args, 1);
    if !remote && !reserved(args, 0) {
        return None;
    }
    if remote {
        return Some(Err(remote_refusal()));
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Some(Err(unsupported()))
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Some((|| {
        if matches!(args, [help, capability] if help == "--help" && capability == CAPABILITY) {
            std::io::stdout()
                .lock()
                .write_all(b"creation-launch-status-v1\n")
                .map_err(|_| fail())?;
            return Ok(0);
        }
        let request = request(args).ok_or_else(|| Error::new(2, INVALID))?;
        let reply = unix::observe(&request).map_err(|_| fail())?;
        let bytes = unix::encode(&reply).map_err(|_| fail())?;
        std::io::stdout()
            .lock()
            .write_all(&bytes)
            .map_err(|_| fail())?;
        Ok(0)
    })())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix {
    use super::*;
    use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
    use serde::Serialize;
    use serde_json::Value;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeSet;
    use std::fs::{self, File, Metadata};
    use std::io::{self, Read};
    use std::os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::MetadataExt,
    };
    use std::path::{Component, Path, PathBuf};

    const RAW: usize = 1 << 20;
    // Conservative reservations, not measured syscall counts. Exact1.99.1 can
    // perform statx + availability probe + legacy stat, and audited libc statx
    // can add fstatat on ENOSYS. Further target paths withhold qualification.
    const META_COST: usize = 5;
    const SMALL: usize = 8 << 10;
    const ENCODED: usize = 16 << 10;
    #[derive(Clone, Copy, Debug)]
    pub(super) struct Fault;
    type Checked<T> = std::result::Result<T, Fault>;

    #[derive(Default)]
    struct Counts {
        walks: usize,
        opens: usize,
        metadata: usize,
    }
    impl Counts {
        fn walk(&mut self, path: &Path) -> Checked<()> {
            let cost = path
                .components()
                .filter(|c| matches!(c, Component::Normal(_)))
                .count()
                + 1;
            if cost > 33 || self.walks >= 16 {
                return Err(Fault);
            }
            self.open(cost)?;
            self.walks += 1;
            Ok(())
        }
        fn open(&mut self, cost: usize) -> Checked<()> {
            if cost > 1024 - self.opens {
                return Err(Fault);
            }
            self.opens += cost;
            Ok(())
        }
        fn meta(&mut self, cost: usize) -> Checked<()> {
            if cost > 4096 - self.metadata {
                return Err(Fault);
            }
            self.metadata += cost;
            Ok(())
        }
    }
    #[derive(PartialEq, Eq)]
    struct Stamp {
        device: u64,
        inode: u64,
        mode: u32,
        uid: u32,
        gid: u32,
        links: u64,
        length: u64,
        modified: (i64, i64),
        changed: (i64, i64),
    }
    impl Stamp {
        fn of(meta: &Metadata) -> Self {
            Self {
                device: meta.dev(),
                inode: meta.ino(),
                mode: meta.mode(),
                uid: meta.uid(),
                gid: meta.gid(),
                links: meta.nlink(),
                length: meta.len(),
                modified: (meta.mtime(), meta.mtime_nsec()),
                changed: (meta.ctime(), meta.ctime_nsec()),
            }
        }
    }
    fn path_stamp(path: &Path, counts: &mut Counts) -> Checked<Option<Stamp>> {
        valid_path(path)?;
        counts.meta(META_COST)?;
        match fs::symlink_metadata(path) {
            Ok(meta) => Ok(Some(Stamp::of(&meta))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(Fault),
        }
    }
    fn file_stamp(file: &File, counts: &mut Counts) -> Checked<Stamp> {
        counts.meta(META_COST)?;
        file.metadata()
            .map(|meta| Stamp::of(&meta))
            .map_err(|_| Fault)
    }
    // Existing Directory's read-only pattern, private to this command. Its
    // generic OpenOptions root open retries EINTR; no such wrapper is used here.
    struct ReadDirectory {
        path: PathBuf,
        file: File,
    }
    fn adopted(fd: libc::c_int) -> Checked<File> {
        if fd < 0 {
            return Err(Fault);
        }
        // SAFETY: every caller passes its one newly acquired owned descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    fn component(name: &std::ffi::OsStr) -> Checked<std::ffi::CString> {
        let bytes = name.as_encoded_bytes();
        if bytes.is_empty() || bytes.len() > 4096 || bytes.contains(&0) {
            return Err(Fault);
        }
        // Bounds/NUL were checked before this owned copy.
        std::ffi::CString::new(bytes).map_err(|_| Fault)
    }
    impl ReadDirectory {
        fn open(path: &Path, counts: &mut Counts) -> Checked<Self> {
            valid_path(path)?;
            // Reserve EVERY potential open before beginning this finite walk.
            counts.walk(path)?;
            let flags = libc::O_RDONLY
                | libc::O_DIRECTORY
                | libc::O_NOFOLLOW
                | libc::O_NONBLOCK
                | libc::O_NOCTTY
                | libc::O_CLOEXEC;
            // SAFETY: fixed absolute root, read-only flags, no variadic mode.
            let mut file = adopted(unsafe { libc::open(c"/".as_ptr(), flags) })?;
            for part in path.components() {
                match part {
                    Component::RootDir => (),
                    Component::Normal(name) => {
                        let name = component(name)?;
                        // SAFETY: owned live parent and one bounded NUL-free
                        // component. EINTR/error refuses, with NO retry/fallback.
                        file = adopted(unsafe {
                            libc::openat(file.as_raw_fd(), name.as_ptr(), flags)
                        })?;
                    }
                    _ => return Err(Fault),
                }
            }
            Ok(Self {
                path: path.into(),
                file,
            })
        }
        fn private(&self, counts: &mut Counts) -> Checked<bool> {
            // The existing predicate has one metadata API and a possible eUID
            // query. Charge both BEFORE calling, even if it short-circuits.
            counts.meta(META_COST + 1)?;
            crate::platform::verify_private_directory(&self.file).map_err(|_| Fault)
        }
        fn verify(&self, counts: &mut Counts) -> Checked<()> {
            let current = Self::open(&self.path, counts)?;
            let held = file_stamp(&self.file, counts)?;
            let now = file_stamp(&current.file, counts)?;
            if (held.device, held.inode) != (now.device, now.inode) {
                return Err(Fault);
            }
            Ok(())
        }
        fn read_private(&self, leaf: &str, counts: &mut Counts) -> Checked<File> {
            if leaf.is_empty() || leaf.contains(['/', '\\', ':']) || matches!(leaf, "." | "..") {
                return Err(Fault);
            }
            let name = component(std::ffi::OsStr::new(leaf))?;
            counts.open(1)?;
            // SAFETY: held parent plus one fixed bounded leaf. No retry, no
            // std open wrapper, no create, and immediate owned FD adoption.
            let file = adopted(unsafe {
                libc::openat(
                    self.file.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY
                        | libc::O_NOFOLLOW
                        | libc::O_NONBLOCK
                        | libc::O_NOCTTY
                        | libc::O_CLOEXEC,
                )
            })?;
            counts.meta(META_COST + 1)?;
            if !crate::platform::verify_private_file(&file).map_err(|_| Fault)? {
                return Err(Fault);
            }
            Ok(file)
        }
    }
    struct Root {
        directory: ReadDirectory,
        stamp: Stamp,
    }
    impl Root {
        fn pin(path: &Path, counts: &mut Counts) -> Checked<Self> {
            valid_path(path)?;
            let stamp = path_stamp(path, counts)?.ok_or(Fault)?;
            let directory = ReadDirectory::open(path, counts)?;
            if !directory.private(counts)? {
                return Err(Fault);
            }
            directory.verify(counts)?;
            if path_stamp(path, counts)?.as_ref() != Some(&stamp) {
                return Err(Fault);
            }
            Ok(Self { directory, stamp })
        }
        fn finish(&self, counts: &mut Counts) -> Checked<()> {
            if !self.directory.private(counts)? {
                return Err(Fault);
            }
            self.directory.verify(counts)?;
            if path_stamp(&self.directory.path, counts)?.as_ref() != Some(&self.stamp) {
                return Err(Fault);
            }
            Ok(())
        }
        fn path(&self, leaf: &str) -> PathBuf {
            self.directory.path.join(leaf)
        }
        fn open(&self, leaf: &str, counts: &mut Counts) -> Checked<File> {
            valid_path(&self.path(leaf))?;
            self.directory.read_private(leaf, counts)
        }
    }
    struct ByteBudget {
        remaining: usize,
    }
    impl ByteBudget {
        fn new() -> Self {
            Self { remaining: RAW }
        }
        fn consume(&mut self, bytes: usize) -> Checked<()> {
            if bytes > self.remaining {
                return Err(Fault);
            }
            self.remaining -= bytes;
            Ok(())
        }
    }
    // One stack buffer serves both bounded initial reads and final hash reads.
    // Never retain a sentinel, retry an interrupted read or parse a prefix.
    fn bounded(
        file: &mut File,
        length: usize,
        budget: &mut ByteBudget,
        retain: bool,
    ) -> Checked<(Vec<u8>, [u8; 32])> {
        if length > budget.remaining {
            return Err(Fault);
        }
        let mut bytes = if retain {
            Vec::with_capacity(length)
        } else {
            Vec::new()
        };
        let mut hash = Sha256::new();
        let mut read = 0usize;
        let mut scratch = [0u8; SMALL];
        loop {
            let maximum = scratch.len().min(length.saturating_sub(read) + 1);
            let count = file.read(&mut scratch[..maximum]).map_err(|_| Fault)?;
            if count == 0 {
                break;
            }
            if count > length - read {
                return Err(Fault);
            }
            budget.consume(count)?;
            hash.update(&scratch[..count]);
            if retain {
                bytes.extend_from_slice(&scratch[..count]);
            }
            read += count;
        }
        if read != length {
            return Err(Fault);
        }
        Ok((bytes, hash.finalize().into()))
    }
    struct Image {
        file: File,
        stamp: Stamp,
        bytes: Vec<u8>,
        hash: [u8; 32],
    }
    fn capture(
        root: &Root,
        leaf: &str,
        cap: usize,
        bytes: &mut ByteBudget,
        counts: &mut Counts,
    ) -> Checked<Option<Image>> {
        let Some(stamp) = path_stamp(&root.path(leaf), counts)? else {
            return Ok(None);
        };
        if stamp.length > cap as u64 || stamp.length > bytes.remaining as u64 {
            return Err(Fault);
        }
        let mut file = root.open(leaf, counts)?;
        if file_stamp(&file, counts)? != stamp {
            return Err(Fault);
        }
        let (raw, hash) = bounded(&mut file, stamp.length as usize, bytes, true)?;
        if file_stamp(&file, counts)? != stamp
            || path_stamp(&root.path(leaf), counts)?.as_ref() != Some(&stamp)
        {
            return Err(Fault);
        }
        Ok(Some(Image {
            file,
            stamp,
            bytes: raw,
            hash,
        }))
    }
    fn recheck(
        root: &Root,
        leaf: &str,
        image: Option<&Image>,
        bytes: &mut ByteBudget,
        counts: &mut Counts,
    ) -> Checked<()> {
        let stamp = path_stamp(&root.path(leaf), counts)?;
        let Some(image) = image else {
            return if stamp.is_none() { Ok(()) } else { Err(Fault) };
        };
        if stamp.as_ref() != Some(&image.stamp) || file_stamp(&image.file, counts)? != image.stamp {
            return Err(Fault);
        }
        let mut file = root.open(leaf, counts)?;
        if file_stamp(&file, counts)? != image.stamp {
            return Err(Fault);
        }
        let (_, hash) = bounded(&mut file, image.stamp.length as usize, bytes, false)?;
        if hash != image.hash
            || file_stamp(&file, counts)? != image.stamp
            || file_stamp(&image.file, counts)? != image.stamp
            || path_stamp(&root.path(leaf), counts)?.as_ref() != Some(&image.stamp)
        {
            return Err(Fault);
        }
        Ok(())
    }
    // Refuse duplicate JSON keys before any Value/Store can erase ambiguity.
    // This deliberately also refuses duplicate opaque fields in this reader.
    struct Unique;
    impl<'de> DeserializeSeed<'de> for Unique {
        type Value = ();
        fn deserialize<D: serde::Deserializer<'de>>(
            self,
            decoder: D,
        ) -> std::result::Result<(), D::Error> {
            decoder.deserialize_any(self)
        }
    }
    impl<'de> Visitor<'de> for Unique {
        type Value = ();
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("unambiguous JSON")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> std::result::Result<(), M::Error> {
            let mut keys = BTreeSet::new();
            while let Some(key) = map.next_key::<String>()? {
                if !keys.insert(key) {
                    return Err(serde::de::Error::custom("ambiguous JSON"));
                }
                map.next_value_seed(Unique)?;
            }
            Ok(())
        }
        fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> std::result::Result<(), S::Error> {
            while seq.next_element_seed(Unique)?.is_some() {}
            Ok(())
        }
        fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_str<E: serde::de::Error>(self, _: &str) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_none<E: serde::de::Error>(self) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<(), E> {
            Ok(())
        }
    }
    fn unique(bytes: &[u8]) -> Checked<()> {
        let mut decoder = serde_json::Deserializer::from_slice(bytes);
        Unique.deserialize(&mut decoder).map_err(|_| Fault)?;
        decoder.end().map_err(|_| Fault)
    }
    fn json(image: &Image) -> Checked<Value> {
        unique(&image.bytes)?;
        let value: Value = serde_json::from_slice(&image.bytes).map_err(|_| Fault)?;
        if !value.is_object()
            || ["schema", "version"]
                .iter()
                .any(|key| value.get(key).is_some_and(|v| v.as_u64() != Some(1)))
        {
            return Err(Fault);
        }
        Ok(value)
    }
    fn text<'a>(value: &'a Value, key: &str) -> Checked<&'a str> {
        value.get(key).and_then(Value::as_str).ok_or(Fault)
    }
    fn name(value: &str) -> bool {
        let parts: Vec<_> = value.split('/').collect();
        !value.is_empty()
            && value.len() <= 512
            && (2..=3).contains(&parts.len())
            && parts.iter().all(|part| {
                !part.is_empty()
                    && !part.contains([':', '.'])
                    && !part.chars().any(|c| c.is_control() || c.is_whitespace())
            })
    }
    fn finite(value: &Value, key: &str) -> Checked<f64> {
        value
            .get(key)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite() && *v >= 0.0)
            .ok_or(Fault)
    }
    #[derive(Serialize)]
    #[serde(tag = "status", rename_all = "snake_case")]
    pub(super) enum Reply {
        Unknown {
            schema: u32,
            machine_id: String,
            launch_id: String,
        },
        ObservedBinding {
            schema: u32,
            machine_id: String,
            launch_id: String,
            name: String,
            agent: String,
            run_id: String,
            conversation_id: String,
        },
        ObservedEnd {
            schema: u32,
            machine_id: String,
            launch_id: String,
            name: String,
            code: i32,
            ended_at: f64,
            binding_unresolved: bool,
        },
    }
    fn unknown(request: &Request) -> Reply {
        Reply::Unknown {
            schema: 1,
            machine_id: request.machine.clone(),
            launch_id: request.launch.clone(),
        }
    }
    struct Locations {
        identity: PathBuf,
        state: PathBuf,
    }
    fn valid_path(path: &Path) -> Checked<()> {
        if !path.is_absolute()
            || path.as_os_str().len() > 4096
            || path
                .components()
                .filter(|c| matches!(c, Component::Normal(_)))
                .count()
                > 32
            || path
                .components()
                .any(|c| !matches!(c, Component::RootDir | Component::Normal(_)))
        {
            return Err(Fault);
        }
        Ok(())
    }
    const CWD_BUFFER: usize = 4096;
    fn cwd_path(bytes: &[u8]) -> Checked<PathBuf> {
        if bytes.is_empty() || bytes.len() >= CWD_BUFFER || bytes.contains(&0) {
            return Err(Fault);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| Fault)?;
        let path = PathBuf::from(text);
        valid_path(&path)?;
        Ok(path)
    }
    #[cfg(any(target_os = "linux", test))]
    fn linux_cwd_reply(buffer: &[u8; CWD_BUFFER], written: libc::c_long) -> Checked<PathBuf> {
        let count = usize::try_from(written).map_err(|_| Fault)?;
        if count == 0 || count > buffer.len() || buffer[count - 1] != 0 {
            return Err(Fault);
        }
        // Linux's return includes the ONE terminal NUL. No prior NUL is valid.
        cwd_path(&buffer[..count - 1])
    }
    #[cfg(any(target_os = "macos", test))]
    fn mac_cwd_reply(buffer: &[u8; CWD_BUFFER], api_range: usize) -> Checked<PathBuf> {
        // F_GETPATH returns no byte count. Its SDK MAXPATHLEN, not our larger
        // allocation or zero initialization, bounds where a real NUL may occur.
        let written = buffer.get(..api_range).ok_or(Fault)?;
        let terminal = written.iter().position(|byte| *byte == 0).ok_or(Fault)?;
        cwd_path(&written[..terminal])
    }
    #[cfg(target_os = "linux")]
    fn bounded_cwd(counts: &mut Counts) -> Checked<PathBuf> {
        counts.meta(1)?;
        let mut buffer = [0xa5; CWD_BUFFER];
        // SAFETY: buffer is writable for exactly its supplied length. Calling
        // the kernel directly avoids libc getcwd's growing/path-walk fallback.
        let written = unsafe { libc::syscall(libc::SYS_getcwd, buffer.as_mut_ptr(), buffer.len()) };
        // Any error, including EINTR, refuses. Never retry or use another API.
        linux_cwd_reply(&buffer, written)
    }
    #[cfg(target_os = "macos")]
    fn bounded_cwd(counts: &mut Counts) -> Checked<PathBuf> {
        use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
        const _: () = assert!(libc::MAXPATHLEN > 0 && libc::MAXPATHLEN as usize <= CWD_BUFFER);
        counts.open(1)?;
        // SAFETY: fixed NUL-terminated dot; no caller path or ownership alias.
        let fd = unsafe {
            libc::open(
                c".".as_ptr(),
                libc::O_RDONLY
                    | libc::O_DIRECTORY
                    | libc::O_NOFOLLOW
                    | libc::O_NONBLOCK
                    | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(Fault);
        }
        // SAFETY: this is the one newly owned descriptor from successful open.
        let file = unsafe { File::from_raw_fd(fd) };
        let result = (|| {
            counts.meta(META_COST)?;
            let before = file.metadata().map_err(|_| Fault)?;
            if !before.is_dir() || before.nlink() == 0 {
                return Err(Fault);
            }
            let mut buffer = [0xa5; CWD_BUFFER];
            counts.meta(1)?;
            // SAFETY: SDK requires MAXPATHLEN writable bytes; our fixed buffer
            // satisfies that compile-time checked bound and stays alive here.
            if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETPATH, buffer.as_mut_ptr()) } != 0 {
                return Err(Fault);
            }
            let path = mac_cwd_reply(&buffer, libc::MAXPATHLEN as usize)?;
            counts.meta(META_COST)?;
            let after = file.metadata().map_err(|_| Fault)?;
            if !after.is_dir() || after.nlink() == 0 || Stamp::of(&before) != Stamp::of(&after) {
                return Err(Fault);
            }
            Ok(path)
        })();
        // Consume ownership BEFORE close: ambiguous close/EINTR is never retried
        // by us or File::drop after the descriptor number could have been reused.
        let fd = file.into_raw_fd();
        // SAFETY: fd is this still-owned transient descriptor; close exactly once.
        if unsafe { libc::close(fd) } != 0 {
            return Err(Fault);
        }
        // No source root can be opened until this exact close has succeeded.
        result
    }
    fn locations(counts: &mut Counts) -> Checked<Locations> {
        let mut aggregate = 0usize;
        let mut variable = |key| -> Checked<Option<String>> {
            match std::env::var(key) {
                Ok(value) => {
                    if value.is_empty() || value.len() > 4096 {
                        return Err(Fault);
                    }
                    aggregate += value.len();
                    if aggregate > 16 << 10 {
                        return Err(Fault);
                    }
                    Ok(Some(value))
                }
                Err(std::env::VarError::NotPresent) => Ok(None),
                Err(_) => Err(Fault),
            }
        };
        let home = variable("HOME")?.ok_or(Fault)?;
        let config = variable("HGS_CONFIG_DIR")?
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(&home).join(".config/hgs"));
        let state = variable("HGS_STATE_DIR")?;
        let xdg = variable("XDG_STATE_HOME")?;
        let cwd = bounded_cwd(counts)?;
        let current = cwd.to_str().ok_or(Fault)?;
        if current.len() > 4096 || aggregate + current.len() > 16 << 10 {
            return Err(Fault);
        }
        let state = state.map(PathBuf::from).unwrap_or_else(|| {
            xdg.map(PathBuf::from)
                .unwrap_or_else(|| Path::new(&home).join(".local/state"))
                .join("hgs/sessions")
        });
        let absolute = |path: PathBuf| {
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        };
        let config = absolute(config);
        let state = absolute(state);
        valid_path(&config)?;
        valid_path(&state)?;
        let identity = config.join("swarm");
        valid_path(&identity)?;
        Ok(Locations { identity, state })
    }
    fn selected(request: &Request, path: &Path, counts: &mut Counts) -> Checked<Reply> {
        let state = Root::pin(path, counts)?;
        let launches = Root::pin(&path.join("launches"), counts)?;
        let mut bytes = ByteBudget::new();
        let start_leaf = format!("{}.json", request.launch);
        let end_leaf = format!("{}.end.json", request.launch);
        let start = capture(&launches, &start_leaf, SMALL, &mut bytes, counts)?;
        let end = capture(&launches, &end_leaf, SMALL, &mut bytes, counts)?;
        let start_value = start.as_ref().map(json).transpose()?;
        let end_value = end.as_ref().map(json).transpose()?;
        let mut routes = None;
        let mut route_absent = None;
        let mut binding = None;
        let mut binding_leaf = None;
        if let Some(start) = &start_value {
            if text(start, "launch_id")? != request.launch || !name(text(start, "name")?) {
                return Err(Fault);
            }
            finite(start, "started_at")?;
            let leaf = format!("{:x}.json", Sha256::digest(text(start, "name")?.as_bytes()));
            let route_path = path.join("name_routes");
            match path_stamp(&route_path, counts)? {
                None => route_absent = Some(route_path),
                Some(_) => {
                    let root = Root::pin(&route_path, counts)?;
                    if path_stamp(&root.path(&leaf), counts)?.is_some() {
                        return Err(Fault);
                    }
                    routes = Some(root);
                }
            }
            binding = capture(&state, &leaf, RAW, &mut bytes, counts)?;
            binding_leaf = Some(leaf);
        }
        let result = if let Some(binding) = &binding {
            let value = json(binding)?;
            let original_name = text(start_value.as_ref().ok_or(Fault)?, "name")?;
            let agent = text(&value, "agent")?;
            let run = text(&value, "run_id")?;
            let conversation = text(&value, "conversation_id")?;
            if value.get("version").and_then(Value::as_u64) != Some(1)
                || text(&value, "launch_id")? != request.launch
                || text(&value, "name")? != original_name
                || !matches!(agent, "claude" | "codex")
                || original_name.split('/').next() != Some(agent)
                || !canonical(run)
                || !canonical(conversation)
                || [
                    "expected_id",
                    "creation_pending",
                    "rename_shadow",
                    "rename",
                    "renaming",
                ]
                .iter()
                .any(|key| value.get(key).is_some())
                || end.is_some()
            {
                return Err(Fault);
            }
            Reply::ObservedBinding {
                schema: 1,
                machine_id: request.machine.clone(),
                launch_id: request.launch.clone(),
                name: original_name.into(),
                agent: agent.into(),
                run_id: run.into(),
                conversation_id: conversation.into(),
            }
        } else if let Some(end) = &end_value {
            let observed_name = text(end, "name")?;
            if text(end, "launch_id")? != request.launch
                || !name(observed_name)
                || end.get("draining").is_some()
                || end.get("error").is_some()
                || end.get("output").is_some_and(|v| !v.is_string())
                || start_value.as_ref().is_some_and(|start| {
                    start.get("name").and_then(Value::as_str) != Some(observed_name)
                })
            {
                return Err(Fault);
            }
            let code = end
                .get("code")
                .and_then(Value::as_i64)
                .and_then(|n| i32::try_from(n).ok())
                .ok_or(Fault)?;
            Reply::ObservedEnd {
                schema: 1,
                machine_id: request.machine.clone(),
                launch_id: request.launch.clone(),
                name: observed_name.into(),
                code,
                ended_at: finite(end, "ended_at")?,
                binding_unresolved: true,
            }
        } else {
            unknown(request)
        };
        let mut rechecked = ByteBudget::new();
        recheck(
            &launches,
            &start_leaf,
            start.as_ref(),
            &mut rechecked,
            counts,
        )?;
        recheck(&launches, &end_leaf, end.as_ref(), &mut rechecked, counts)?;
        if let Some(leaf) = &binding_leaf {
            recheck(&state, leaf, binding.as_ref(), &mut rechecked, counts)?;
            if let Some(root) = &routes {
                if path_stamp(&root.path(leaf), counts)?.is_some() {
                    return Err(Fault);
                }
            }
        }
        if let Some(path) = route_absent {
            if path_stamp(&path, counts)?.is_some() {
                return Err(Fault);
            }
        }
        if let Some(root) = routes {
            root.finish(counts)?;
        }
        launches.finish(counts)?;
        state.finish(counts)?;
        Ok(result)
    }
    pub(super) fn observe(request: &Request) -> Checked<Reply> {
        let mut counts = Counts::default();
        let locations = locations(&mut counts)?;
        observe_at(request, &locations, &mut counts)
    }
    fn observe_at(request: &Request, locations: &Locations, counts: &mut Counts) -> Checked<Reply> {
        let root = Root::pin(&locations.identity, counts)?;
        let identity =
            capture(&root, "catalog.json", RAW, &mut ByteBudget::new(), counts)?.ok_or(Fault)?;
        unique(&identity.bytes)?;
        if crate::swarm::existing_node_id(&identity.bytes).map_err(|_| Fault)? != request.machine {
            return Err(Fault);
        }
        let reply =
            selected(request, &locations.state, counts).unwrap_or_else(|_| unknown(request));
        recheck(
            &root,
            "catalog.json",
            Some(&identity),
            &mut ByteBudget::new(),
            counts,
        )?;
        root.finish(counts)?;
        Ok(reply)
    }
    pub(super) fn encode(reply: &Reply) -> Checked<Vec<u8>> {
        struct Bounded(Vec<u8>);
        impl Write for Bounded {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if bytes.len() > ENCODED - 1 - self.0.len() {
                    return Err(io::Error::other("encoded bound"));
                }
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut out = Bounded(Vec::with_capacity(ENCODED));
        serde_json::to_writer(&mut out, reply).map_err(|_| Fault)?;
        out.0.push(b'\n');
        Ok(out.0)
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use serde_json::json;
        use std::os::unix::fs::{symlink, PermissionsExt};

        #[test]
        fn cwd_kernel_length_nul_unicode_and_component_bounds_are_strict() {
            let mut buffer = [0xa5; CWD_BUFFER];
            for written in [-1, 0, 4097] {
                assert!(linux_cwd_reply(&buffer, written).is_err());
            }
            buffer[..3].copy_from_slice(b"/x\0");
            assert_eq!(linux_cwd_reply(&buffer, 3).unwrap(), Path::new("/x"));
            assert!(linux_cwd_reply(&buffer, 4).is_err());
            buffer[..5].copy_from_slice(b"/x\0y\0");
            assert!(linux_cwd_reply(&buffer, 5).is_err());
            buffer[..3].copy_from_slice(&[b'/', 0xff, 0]);
            assert!(linux_cwd_reply(&buffer, 3).is_err());
            assert!(cwd_path(b"(unreachable)/x").is_err());
            assert!(cwd_path(b"relative").is_err());
            assert!(cwd_path(format!("/{}", vec!["x"; 33].join("/")).as_bytes()).is_err());
            buffer.fill(b'x');
            buffer[0] = b'/';
            buffer[CWD_BUFFER - 1] = 0;
            assert!(linux_cwd_reply(&buffer, CWD_BUFFER as libc::c_long).is_ok());
            buffer[CWD_BUFFER - 1] = b'x';
            assert!(linux_cwd_reply(&buffer, CWD_BUFFER as libc::c_long).is_err());
        }
        #[test]
        fn mac_cwd_requires_a_real_terminator_within_the_api_output_range() {
            let mut buffer = [0xa5; CWD_BUFFER];
            assert!(mac_cwd_reply(&buffer, 1024).is_err());
            assert!(mac_cwd_reply(&buffer, 0).is_err());
            assert!(mac_cwd_reply(&buffer, CWD_BUFFER + 1).is_err());
            buffer[..2].copy_from_slice(b"/x");
            buffer[1024] = 0; // Outside MAXPATHLEN cannot supply the terminator.
            assert!(mac_cwd_reply(&buffer, 1024).is_err());
            buffer[2] = 0;
            assert_eq!(mac_cwd_reply(&buffer, 1024).unwrap(), Path::new("/x"));
            buffer[1] = 0xff;
            assert!(mac_cwd_reply(&buffer, 1024).is_err());
        }
        fn id(n: u8) -> String {
            format!("00000000-1111-4111-8111-{n:012}")
        }
        fn args(words: &[&str]) -> Vec<String> {
            words.iter().map(|s| (*s).into()).collect()
        }
        fn private_dir(path: &Path) {
            fs::create_dir_all(path).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fn write(path: &Path, bytes: &[u8]) {
            fs::write(path, bytes).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        struct Fixture {
            _root: tempfile::TempDir,
            locations: Locations,
            request: Request,
        }
        impl Fixture {
            fn new() -> Self {
                let root = tempfile::tempdir().unwrap();
                let identity = root.path().join("config/swarm");
                let state = root.path().join("state");
                for path in [
                    &identity,
                    &state,
                    &state.join("launches"),
                    &state.join("name_routes"),
                ] {
                    private_dir(path);
                }
                write(
                    &identity.join("catalog.json"),
                    &serde_json::to_vec(&json!({
                        "node_id":id(1),"catalog":{"schema":1,"swarm_id":id(9),"operations":{}}
                    }))
                    .unwrap(),
                );
                Self {
                    _root: root,
                    locations: Locations { identity, state },
                    request: Request {
                        machine: id(1),
                        launch: id(2),
                    },
                }
            }
            fn start_path(&self) -> PathBuf {
                self.locations
                    .state
                    .join("launches")
                    .join(format!("{}.json", id(2)))
            }
            fn end_path(&self) -> PathBuf {
                self.locations
                    .state
                    .join("launches")
                    .join(format!("{}.end.json", id(2)))
            }
            fn leaf(&self) -> String {
                format!("{:x}.json", Sha256::digest(b"codex/fixture/original"))
            }
            fn binding_path(&self) -> PathBuf {
                self.locations.state.join(self.leaf())
            }
            fn start(&self) {
                write(
                    &self.start_path(),
                    &serde_json::to_vec(&json!({
                        "launch_id":id(2),"name":"codex/fixture/original","started_at":1.0
                    }))
                    .unwrap(),
                );
            }
            fn binding(&self) -> Value {
                json!({
                    "version":1,"name":"codex/fixture/original","agent":"codex",
                    "launch_id":id(2),"run_id":id(3),"conversation_id":id(4),
                    "opaque":{"secret":"PRIVATE BODY"}
                })
            }
            fn end(&self) -> Value {
                json!({
                    "launch_id":id(2),"name":"codex/fixture/original","code":0,
                    "ended_at":2.0,"output":"PRIVATE NATIVE OUTPUT"
                })
            }
            fn put_binding(&self, value: &Value) {
                write(&self.binding_path(), &serde_json::to_vec(value).unwrap());
            }
            fn put_end(&self, value: &Value) {
                write(&self.end_path(), &serde_json::to_vec(value).unwrap());
            }
            fn read(&self) -> (Value, Counts) {
                let mut counts = Counts::default();
                let reply = observe_at(&self.request, &self.locations, &mut counts).unwrap();
                let encoded = encode(&reply).unwrap();
                assert!(encoded.len() <= ENCODED);
                assert!(!String::from_utf8_lossy(&encoded).contains("PRIVATE"));
                (serde_json::from_slice(&encoded).unwrap(), counts)
            }
        }
        #[test]
        fn positional_recognition_does_not_hijack_native_arguments_or_origin_labels() {
            for value in [
                args(&["codex", "fixture", "-n", COMMAND]),
                args(&["sh", COMMAND]),
                args(&[
                    "--expect-machine",
                    &id(1),
                    "--client",
                    COMMAND,
                    "dsh",
                    "fixture",
                ]),
                args(&["--expect-machine", &id(1), "dsh", COMMAND]),
                args(&["--help", "unrelated", CAPABILITY]),
            ] {
                assert!(!reserved(&value, 0));
                assert!(early(&value).is_none());
            }
            for value in [
                args(&[COMMAND]),
                args(&["--expect-machine", COMMAND, "--launch-id", &id(2), "--json"]),
                args(&[
                    "--expect-machine",
                    &id(1),
                    "--client",
                    COMMAND,
                    "--launch-id",
                    &id(2),
                    "--json",
                ]),
                args(&["--help", CAPABILITY, "extra"]),
            ] {
                assert!(reserved(&value, 0));
                assert!(request(&value).is_none());
            }
            let remote = args(&[
                "@fixture",
                "--expect-machine",
                &id(1),
                COMMAND,
                "--launch-id",
                &id(2),
                "--json",
            ]);
            assert!(reserved(&remote, 1));
            assert!(early(&remote).unwrap().is_err());
        }
        #[test]
        fn exact_guard_grammar_canonical_ids_and_nonidentity_origin() {
            let plain = args(&[
                "--expect-machine",
                &id(1),
                COMMAND,
                "--launch-id",
                &id(2),
                "--json",
            ]);
            assert!(request(&plain).is_some());
            let forwarded = args(&[
                "--expect-machine",
                &id(1),
                "--client",
                COMMAND,
                COMMAND,
                "--launch-id",
                &id(2),
                "--json",
            ]);
            assert!(request(&forwarded).is_some());
            for (index, value) in [
                (1, "00000000-0000-0000-0000-000000000000"),
                (1, "ABCDEF00-1111-4111-8111-000000000001"),
                (4, "not-a-uuid"),
                (3, "--request-id"),
                (5, "--dry-run"),
            ] {
                let mut changed = plain.clone();
                changed[index] = value.into();
                assert!(request(&changed).is_none());
            }
            for origin in ["", "two words", "\n"] {
                let mut changed = forwarded.clone();
                changed[3] = origin.into();
                assert!(request(&changed).is_none());
            }
            let mut extra = plain;
            extra.push("--json".into());
            assert!(request(&extra).is_none());
        }
        #[test]
        fn actual_original_binding_is_closed_historical_body_free_evidence() {
            let f = Fixture::new();
            f.start();
            f.put_binding(&f.binding());
            let (reply, counts) = f.read();
            assert_eq!(reply["status"], "observed_binding");
            assert_eq!(reply.as_object().unwrap().len(), 8);
            assert_eq!(reply["machine_id"], id(1));
            assert_eq!(reply["launch_id"], id(2));
            assert_eq!(reply["run_id"], id(3));
            assert_eq!(reply["conversation_id"], id(4));
            assert!(counts.walks <= 16 && counts.opens <= 1024 && counts.metadata <= 4096);
        }
        #[test]
        fn missing_start_never_selects_a_current_or_independent_name_binding() {
            let f = Fixture::new();
            f.put_binding(&f.binding());
            assert_eq!(f.read().0["status"], "unknown");
            f.put_end(&f.end());
            let (reply, _) = f.read();
            assert_eq!(reply["status"], "observed_end");
            assert_eq!(reply["binding_unresolved"], true);
            assert_eq!(reply.as_object().unwrap().len(), 8);
            fs::remove_file(f.end_path()).unwrap();
            f.start();
            fs::remove_file(f.binding_path()).unwrap();
            assert_eq!(f.read().0["status"], "unknown");
        }
        #[test]
        fn matching_end_or_any_present_route_refuses_a_binding() {
            let f = Fixture::new();
            f.start();
            f.put_binding(&f.binding());
            f.put_end(&f.end());
            assert_eq!(f.read().0["status"], "unknown");
            fs::remove_file(f.end_path()).unwrap();
            write(
                &f.locations.state.join("name_routes").join(f.leaf()),
                b"not even parsed",
            );
            assert_eq!(f.read().0["status"], "unknown");
        }
        #[test]
        fn provider_identity_pending_and_future_binding_fields_refuse() {
            let f = Fixture::new();
            f.start();
            for (key, value) in [
                ("agent", json!("kimi")),
                ("name", json!("codex/fixture/reused")),
                ("launch_id", json!(id(8))),
                ("run_id", Value::Null),
                ("conversation_id", json!("ses_not_a_codex_id")),
                ("version", json!(2)),
                ("expected_id", Value::Null),
                ("creation_pending", json!(false)),
                ("rename_shadow", json!(false)),
                ("rename", json!({})),
            ] {
                let mut binding = f.binding();
                binding[key] = value;
                f.put_binding(&binding);
                assert_eq!(f.read().0["status"], "unknown", "{key}");
            }
        }
        #[test]
        fn end_requires_the_actual_completed_encoding_and_explicit_valid_fields() {
            let f = Fixture::new();
            for (key, value) in [
                ("name", Value::Null),
                ("name", json!("")),
                ("code", json!(2147483648_i64)),
                ("code", json!(1.0)),
                ("code", Value::Null),
                ("ended_at", json!(-1)),
                ("ended_at", Value::Null),
                ("launch_id", json!(id(8))),
                ("draining", json!(false)),
                ("draining", Value::Null),
                ("error", json!("")),
                ("error", Value::Null),
                ("output", json!({})),
            ] {
                let mut end = f.end();
                end[key] = value;
                f.put_end(&end);
                assert_eq!(f.read().0["status"], "unknown", "{key}");
            }
            f.start();
            let mut end = f.end();
            end["name"] = json!("codex/fixture/other");
            f.put_end(&end);
            assert_eq!(f.read().0["status"], "unknown");
        }
        #[test]
        fn duplicate_bootstrap_identity_refuses_before_selected_directory_walks() {
            let f = Fixture::new();
            f.start();
            f.put_binding(&f.binding());
            let raw = serde_json::to_string(
                &json!({"node_id":id(1),"catalog":{"schema":1,"swarm_id":id(9),"operations":{}}}),
            )
            .unwrap();
            write(
                &f.locations.identity.join("catalog.json"),
                raw.replacen("\"node_id\":", "\"node_id\":null,\"node_id\":", 1)
                    .as_bytes(),
            );
            let mut counts = Counts::default();
            assert!(observe_at(&f.request, &f.locations, &mut counts).is_err());
            assert_eq!(counts.walks, 2);
            write(
                &f.locations.identity.join("catalog.json"),
                &vec![b' '; RAW + 1],
            );
            let mut counts = Counts::default();
            assert!(observe_at(&f.request, &f.locations, &mut counts).is_err());
            assert_eq!(counts.walks, 2);
        }
        #[test]
        fn duplicate_sources_and_combined_selected_or_per_leaf_bounds_refuse() {
            let f = Fixture::new();
            f.start();
            let raw = serde_json::to_string(&f.binding()).unwrap().replacen(
                "\"run_id\":",
                "\"run_id\":null,\"run_id\":",
                1,
            );
            write(&f.binding_path(), raw.as_bytes());
            assert_eq!(f.read().0["status"], "unknown");
            let mut raw = serde_json::to_vec(&f.binding()).unwrap();
            raw.resize(RAW, b' ');
            write(&f.binding_path(), &raw);
            assert_eq!(f.read().0["status"], "unknown");
            fs::remove_file(f.binding_path()).unwrap();
            write(&f.end_path(), &vec![b' '; SMALL + 1]);
            assert_eq!(f.read().0["status"], "unknown");
            assert!(unique(br#"{"opaque":{"x":1,"x":2}}"#).is_err());
        }
        #[test]
        fn unsafe_link_directory_permissions_and_fifo_never_become_a_binding() {
            let f = Fixture::new();
            f.start();
            f.put_binding(&f.binding());
            fs::set_permissions(f.binding_path(), fs::Permissions::from_mode(0o644)).unwrap();
            assert_eq!(f.read().0["status"], "unknown");
            fs::set_permissions(f.binding_path(), fs::Permissions::from_mode(0o600)).unwrap();
            let other = f.locations.state.join("other");
            fs::hard_link(f.binding_path(), &other).unwrap();
            assert_eq!(f.read().0["status"], "unknown");
            fs::remove_file(&other).unwrap();
            fs::rename(f.binding_path(), &other).unwrap();
            symlink(&other, f.binding_path()).unwrap();
            assert_eq!(f.read().0["status"], "unknown");
            fs::remove_file(f.binding_path()).unwrap();
            private_dir(&f.binding_path());
            assert_eq!(f.read().0["status"], "unknown");
            fs::remove_dir(f.binding_path()).unwrap();
            use std::os::unix::ffi::OsStrExt;
            let fifo = std::ffi::CString::new(f.binding_path().as_os_str().as_bytes()).unwrap();
            // SAFETY: the fixture owns this NUL-terminated path; no existing entry.
            assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
            assert_eq!(f.read().0["status"], "unknown");
        }
        #[test]
        fn actual_file_and_held_root_replacements_refuse_recheck() {
            let f = Fixture::new();
            f.start();
            f.put_binding(&f.binding());
            let mut counts = Counts::default();
            let root = Root::pin(&f.locations.state, &mut counts).unwrap();
            let image = capture(&root, &f.leaf(), RAW, &mut ByteBudget::new(), &mut counts)
                .unwrap()
                .unwrap();
            let mut changed = f.binding();
            changed["opaque"] = json!("changed");
            f.put_binding(&changed);
            assert!(recheck(
                &root,
                &f.leaf(),
                Some(&image),
                &mut ByteBudget::new(),
                &mut counts
            )
            .is_err());
            let moved = f.locations.state.with_extension("moved");
            fs::rename(&f.locations.state, &moved).unwrap();
            private_dir(&f.locations.state);
            assert!(root.finish(&mut counts).is_err());
        }
        #[test]
        fn joined_leaf_and_path_limits_refuse_before_metadata_or_open() {
            let mut counts = Counts::default();
            let huge = PathBuf::from(format!("/{}", "x".repeat(4096)));
            assert!(path_stamp(&huge, &mut counts).is_err());
            assert_eq!(counts.metadata, 0);
            assert_eq!(counts.opens, 0);
            let deep = PathBuf::from(format!("/{}", vec!["x"; 33].join("/")));
            assert!(path_stamp(&deep, &mut counts).is_err());
            assert_eq!(counts.metadata, 0);
            assert_eq!(counts.opens, 0);
            let f = Fixture::new();
            let root = Root::pin(&f.locations.state, &mut counts).unwrap();
            let before = counts.opens;
            assert!(root.open(&"x".repeat(4096), &mut counts).is_err());
            assert_eq!(counts.opens, before);
        }
        #[test]
        fn large_siblings_do_not_grow_indexed_open_counts_and_hard_counters_refuse() {
            let f = Fixture::new();
            f.start();
            f.put_binding(&f.binding());
            let (_, before) = f.read();
            for n in 0..2048 {
                write(
                    &f.locations.state.join(format!("{n:064x}.json")),
                    b"unrelated",
                );
                write(
                    &f.locations
                        .state
                        .join("launches")
                        .join(format!("unrelated-{n}.json")),
                    b"unrelated",
                );
            }
            let (reply, after) = f.read();
            assert_eq!(reply["status"], "observed_binding");
            assert_eq!(
                (before.walks, before.opens, before.metadata),
                (after.walks, after.opens, after.metadata)
            );
            let mut counts = Counts::default();
            assert!(counts.open(1025).is_err());
            assert_eq!(counts.opens, 0);
            assert!(counts.meta(4097).is_err());
            assert_eq!(counts.metadata, 0);
            for _ in 0..16 {
                counts.walk(Path::new("/")).unwrap();
            }
            let old = counts.opens;
            assert!(counts.walk(Path::new("/")).is_err());
            assert_eq!(counts.opens, old);
            assert!(valid_path(Path::new("/safe/../other")).is_err());
            assert!(valid_path(Path::new("relative")).is_err());
            assert!(valid_path(&PathBuf::from(format!("/{}", vec!["x"; 33].join("/")))).is_err());
        }
    }
}

#[cfg(test)]
mod argument_tests {
    use super::*;
    use std::ffi::OsString;
    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }
    fn os(words: &[String]) -> Vec<OsString> {
        std::iter::once(OsString::from("arbitrary argv0"))
            .chain(words.iter().map(OsString::from))
            .collect()
    }
    #[test]
    fn os_probe_and_typed_reserved_positions_share_the_exact_predicate() {
        let rows = [
            vec![COMMAND],
            vec!["--help", CAPABILITY],
            vec!["--help", CAPABILITY, "extra"],
            vec![
                "--expect-machine",
                "M",
                COMMAND,
                "--launch-id",
                "ID",
                "--json",
            ],
            vec![
                "--expect-machine",
                "M",
                "--client",
                "origin",
                COMMAND,
                "--launch-id",
                "ID",
                "--json",
            ],
            vec!["--expect-machine", COMMAND],
            vec![
                "--expect-machine",
                "M",
                "--client",
                COMMAND,
                "--launch-id",
                "ID",
                "--json",
            ],
            vec![
                "@target",
                "--expect-machine",
                "M",
                "--client",
                "origin",
                COMMAND,
                "--launch-id",
                "ID",
                "--json",
            ],
            vec!["@target", "--help", CAPABILITY],
            vec!["dsh", COMMAND],
            vec!["a", COMMAND],
            vec!["--help", "other", COMMAND],
            vec![
                "--expect-machine",
                "M",
                "--client",
                COMMAND,
                "dsh",
                "/fixture",
            ],
            vec!["@target", "dsh", COMMAND],
            vec!["dsh", "--client", COMMAND],
        ];
        for row in rows {
            let args = strings(&row);
            let remote = args.first().is_some_and(|arg| arg.starts_with('@')) && reserved(&args, 1);
            let expected = remote || reserved(&args, 0);
            let selected = os_arguments(os(&args));
            assert_eq!(selected.is_some(), expected, "fixture index parity");
            if expected && !remote {
                assert_eq!(selected.unwrap().unwrap(), args);
            }
        }
    }
    #[cfg(unix)]
    #[test]
    fn invalid_os_identity_origin_launch_trailing_and_argv0_never_leak() {
        use std::os::unix::ffi::OsStringExt;
        for words in [
            strings(&[
                "--expect-machine",
                "M",
                COMMAND,
                "--launch-id",
                "ID",
                "--json",
            ]),
            strings(&[
                "--expect-machine",
                "M",
                "--client",
                "origin",
                COMMAND,
                "--launch-id",
                "ID",
                "--json",
            ]),
        ] {
            let indexes: &[usize] = if words.len() == 6 {
                &[1, 4]
            } else {
                &[1, 3, 6]
            };
            for index in indexes {
                let mut args = os(&words);
                args[index + 1] = OsString::from_vec(vec![0xff]);
                let err = os_arguments(args).unwrap().unwrap_err();
                assert_eq!(err.message, INVALID);
                assert!(err.message.len() + 6 <= 256);
            }
            let mut args = os(&words);
            args.push(OsString::from_vec(vec![0xff]));
            assert_eq!(os_arguments(args).unwrap().unwrap_err().message, INVALID);
            let mut args = os(&words);
            args[0] = OsString::from_vec(vec![0xff]);
            assert_eq!(os_arguments(args).unwrap().unwrap(), words);
        }
        let mut decoy = os(&strings(&["dsh", COMMAND]));
        decoy.push(OsString::from_vec(vec![0xff]));
        assert!(os_arguments(decoy).is_none());
    }
    #[test]
    fn overcap_and_tenth_item_refuse_without_yielding_an_eleventh_argument() {
        use std::{cell::Cell, rc::Rc};
        let words = strings(&[
            "--expect-machine",
            "M",
            COMMAND,
            "--launch-id",
            "ID",
            "--json",
        ]);
        for index in [1, 4] {
            let mut args = os(&words);
            args[index + 1] = OsString::from("X".repeat(257));
            assert_eq!(os_arguments(args).unwrap().unwrap_err().message, INVALID);
        }
        let mut aggregate = os(&strings(&[
            "--expect-machine",
            &"M".repeat(256),
            "--client",
            &"O".repeat(256),
            COMMAND,
            "--launch-id",
            &"I".repeat(256),
            "--json",
        ]));
        assert!(os_arguments(aggregate.clone()).unwrap().is_ok());
        aggregate.push(OsString::from("T".repeat(256)));
        assert_eq!(aggregate.len(), 10); // argv0 plus nine arguments, below ten.
        assert_eq!(
            os_arguments(aggregate).unwrap().unwrap_err().message,
            INVALID
        );
        let mut args = os(&words);
        args.extend((0..4).map(|_| OsString::from("extra")));
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        let mut iter = args.into_iter();
        let bounded = std::iter::from_fn(move || {
            observed.set(observed.get() + 1);
            assert!(
                observed.get() <= 11,
                "eleventh argument after argv0 must stay unread"
            );
            iter.next()
        });
        assert_eq!(os_arguments(bounded).unwrap().unwrap_err().message, INVALID);
        assert_eq!(calls.get(), 11); // argv0 plus the ten-item probe.
        let args = os(&strings(&["--expect-machine", &"M".repeat(257), COMMAND]));
        assert!(os_arguments(args).unwrap().is_err());
    }
}
