//! Approval policy requests are issued by an original native UI child, never
//! by the generic CLI request lane or by a JSON role/confirmation flag.
#[cfg(unix)]
use super::owner;
use super::store::Location;
use crate::{config::Config, platform};
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use zerus_client_core::desktop::ui_requests::{Envelope, Reply, Request};

const ANCESTORS: usize = 32;
const WAIT: Duration = Duration::from_secs(2);

pub(super) struct Scope {
    location: Location,
    frontend_images: Vec<PathBuf>,
}
impl std::ops::Deref for Scope {
    type Target = Location;
    fn deref(&self) -> &Location {
        &self.location
    }
}
impl Scope {
    pub fn new(config: &Config, location: &Location) -> Self {
        let home = Path::new(&config.home);
        let mut frontend_images = vec![home.join(".local/bin/hgs-tray")];
        if let Ok(cli) = std::env::current_exe() {
            if let Some(folder) = cli.parent() {
                frontend_images.push(folder.join("hgs-tray"));
            }
            #[cfg(target_os = "macos")]
            if let Some(bundle) = cli
                .ancestors()
                .find(|p| p.extension().is_some_and(|e| e == "app"))
            {
                frontend_images.push(bundle.join("Contents/MacOS/hgs-tray"));
            }
        }
        #[cfg(target_os = "macos")]
        for bundle in [
            home.join("Applications/hgs-tray.app"),
            home.join("Applications/Zerus.app"),
            PathBuf::from("/Applications/Zerus.app"),
            PathBuf::from("/Applications/hgs-tray.app"),
        ] {
            frontend_images.push(bundle.join("Contents/MacOS/hgs-tray"));
        }
        #[cfg(target_os = "linux")]
        frontend_images.push(PathBuf::from("/usr/bin/hgs-tray"));
        Self {
            location: Location {
                root: location.root.clone(),
                configuration: location.configuration.clone(),
            },
            frontend_images,
        }
    }
    fn selected_frontend(&self, image: &Path) -> bool {
        let Some(identity) = platform::file_identity(image) else {
            return false;
        };
        self.frontend_images
            .iter()
            .any(|path| platform::file_identity(path) == Some(identity))
    }
}

#[derive(Clone)]
struct Process {
    pid: u32,
    identity: platform::ProcessIdentity,
    executable: String,
}
impl Process {
    fn read(pid: u32) -> Result<Self> {
        let identity =
            platform::process_identity(pid).context("frontend process identity unavailable")?;
        let executable = platform::describe(pid)
            .context("frontend process image unavailable")?
            .executable;
        ensure!(
            platform::process_identity(pid).as_ref() == Some(&identity),
            "frontend process changed"
        );
        Ok(Self {
            pid,
            identity,
            executable,
        })
    }
    fn current(&self) -> bool {
        platform::process_identity(self.pid).as_ref() == Some(&self.identity)
            && platform::describe(self.pid).is_some_and(|p| p.executable == self.executable)
    }
}

/// Opaque and one-use; only authenticate() can construct it. The actor consumes
/// it just before its durable mutation. No wire decoder can construct authority.
pub(super) struct Authorized {
    envelope: Envelope,
    caller: Process,
    frontend: Process,
    until: Instant,
    instance: String,
    serial: u64,
}
/// Actor-side permit after the original peer was revalidated. It deliberately
/// has no Clone/Deserialize or public constructor, even inside the request lane.
pub(super) struct Validated(Request, Freshness);
enum Freshness {
    Native {
        caller: Process,
        frontend: Process,
        until: Instant,
    },
    #[cfg(test)]
    Fixture,
}
impl Validated {
    pub fn id(&self) -> &str {
        self.0.id()
    }
    pub fn into_request(self) -> Result<Request> {
        match self.1 {
            Freshness::Native {
                caller,
                frontend,
                until,
            } => ensure!(
                Instant::now() < until && caller.current() && frontend.current(),
                "frontend permit expired"
            ),
            #[cfg(test)]
            Freshness::Fixture => (),
        }
        Ok(self.0)
    }
    #[cfg(test)]
    pub fn fixture(value: &Value) -> Self {
        Self(
            serde_json::from_value(value.clone()).expect("synthetic native-policy fixture"),
            Freshness::Fixture,
        )
    }
}
impl Authorized {
    pub fn into_request(self, order: &mut Order) -> Result<Validated> {
        ensure!(
            Instant::now() < self.until && self.caller.current() && self.frontend.current(),
            "frontend request identity expired"
        );
        order.observe(
            (
                self.frontend.pid,
                self.frontend.identity.birth.clone(),
                self.instance,
            ),
            self.serial,
            Instant::now(),
        )?;
        Ok(Validated(
            self.envelope.request,
            Freshness::Native {
                caller: self.caller,
                frontend: self.frontend,
                until: self.until,
            },
        ))
    }
}

pub(super) fn authenticate(location: &Scope, peer_pid: u32, value: &Value) -> Result<Authorized> {
    let envelope: Envelope =
        serde_json::from_value(value.clone()).context("invalid frontend envelope")?;
    ensure!(envelope.valid(), "invalid frontend envelope");
    let until = Instant::now() + WAIT;
    let caller = Process::read(peer_pid)?;
    let own_image = platform::describe(std::process::id()).context("service image unavailable")?;
    let selected = platform::file_identity(Path::new(&own_image.executable))
        .context("service image identity unavailable")?;
    ensure!(
        platform::file_identity(Path::new(&caller.executable)) == Some(selected),
        "frontend relay is not the selected hgs"
    );
    let argv = platform::describe(peer_pid)
        .and_then(|p| p.argv)
        .context("frontend relay argv unavailable")?;
    ensure!(
        argv.get(1..)
            .is_some_and(|a| a == ["desktop", "frontend-request", "--json"]),
        "unexpected frontend relay"
    );
    let (frontend_pid, reply) =
        probe(location, &envelope.attempt).context("canonical frontend evidence unavailable")?;
    let frontend = Process::read(frontend_pid)?;
    ensure!(
        location.selected_frontend(Path::new(&frontend.executable)),
        "canonical frontend is not the installed application"
    );
    validate(
        &envelope,
        &caller,
        &frontend,
        &reply,
        &location.configuration,
    )?;
    check_ancestry(peer_pid, until)?;
    ensure!(
        Instant::now() < until && caller.current() && frontend.current(),
        "frontend changed during authentication"
    );
    Ok(Authorized {
        envelope,
        caller,
        frontend,
        until,
        instance: reply.hello.instance_epoch,
        serial: reply.request.serial,
    })
}

fn validate(
    envelope: &Envelope,
    caller: &Process,
    frontend: &Process,
    reply: &Reply,
    configuration: &str,
) -> Result<()> {
    ensure!(
        reply
            .hello
            .identity_matches(&envelope.attempt, configuration),
        "frontend identity/configuration does not match"
    );
    ensure!(
        caller.identity.parent == frontend.pid,
        "relay is not a direct frontend child"
    );
    let image = Path::new(&frontend.executable)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    ensure!(
        matches!(
            image,
            "zerus-client"
                | "zerus-next"
                | "hgs-tray"
                | "Zerus"
                | "zerus-client.exe"
                | "zerus-next.exe"
                | "hgs-tray.exe"
                | "Zerus.exe"
        ),
        "canonical peer is not a native frontend"
    );
    ensure!(
        reply.request.serial != 0
            && reply.request.attempt == envelope.attempt
            && reply.request.child_pid == caller.pid
            && reply.request.child_birth == caller.identity.birth
            && reply.request.request_id == envelope.request.id()
            && reply.request.fingerprint == envelope.request.fingerprint(),
        "original frontend child/request does not match"
    );
    Ok(())
}

type Instance = (u32, String, String);
#[derive(Default)]
pub(super) struct Order {
    current: Option<(Instance, u64)>,
    retired: BTreeMap<Instance, Instant>,
}
impl Order {
    /// Intent order comes from the native cache, never a caller field. This
    /// prevents a slower earlier Enable from overtaking a later Disable.
    fn observe(&mut self, instance: Instance, serial: u64, now: Instant) -> Result<()> {
        self.retired.retain(|_, until| now < *until);
        ensure!(
            serial != 0 && !self.retired.contains_key(&instance),
            "stale frontend instance"
        );
        if let Some((old, previous)) = &self.current {
            if *old == instance {
                ensure!(serial > *previous, "stale frontend intent");
            } else {
                ensure!(
                    self.retired.len() < 16,
                    "frontend instance changes are busy"
                );
                self.retired.insert(old.clone(), now + WAIT);
            }
        }
        self.current = Some((instance, serial));
        Ok(())
    }
}

#[cfg(unix)]
fn probe(location: &Location, attempt: &str) -> Option<(u32, Reply)> {
    zerus_client_core::desktop::probe_ui_request_at(
        &owner::instance_path(),
        attempt,
        &location.configuration,
    )
}
#[cfg(not(unix))]
fn probe(_: &Location, _: &str) -> Option<(u32, Reply)> {
    // The Windows canonical activation pipe still only accepts show/sessions.
    // Until its bounded reply adapter lands, policy authentication is refused.
    None
}

fn check_ancestry(pid: u32, until: Instant) -> Result<()> {
    ancestry(pid, until, |pid| {
        if !same_user(pid)? {
            return Ok(None);
        }
        let process = Process::read(pid)?;
        #[cfg(target_os = "linux")]
        untraced(pid)?;
        let tagged = agent_environment(pid)?;
        let name = Path::new(&process.executable)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .trim_end_matches(".exe");
        let agent = tagged
            || matches!(
                name,
                "codex" | "claude" | "deepseek" | "gemini" | "opencode"
            );
        ensure!(process.current(), "frontend ancestry changed");
        Ok(Some((process.identity.parent, agent)))
    })
}

fn ancestry(
    mut pid: u32,
    until: Instant,
    mut inspect: impl FnMut(u32) -> Result<Option<(u32, bool)>>,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    while pid != 0 {
        ensure!(
            seen.len() < ANCESTORS && Instant::now() < until && seen.insert(pid),
            "frontend ancestry unavailable"
        );
        let Some((parent, agent)) = inspect(pid)? else {
            return Ok(());
        };
        ensure!(!agent, "agent-descended frontend request refused");
        pid = parent;
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn marked(bytes: &[u8]) -> bool {
    bytes.split(|b| *b == 0).any(|entry| {
        [
            b"HGS_SESSION=".as_slice(),
            b"HGS_RUN_ID=",
            b"LD_PRELOAD=",
            b"LD_AUDIT=",
            b"DYLD_INSERT_LIBRARIES=",
            b"DYLD_FORCE_FLAT_NAMESPACE=",
        ]
        .iter()
        .any(|key| entry.starts_with(key))
    })
}

#[cfg(any(target_os = "linux", test))]
fn tracer_absent(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let mut fields = text
        .lines()
        .filter_map(|line| line.strip_prefix("TracerPid:"));
    fields
        .next()
        .is_some_and(|v| v.trim().parse::<u32>() == Ok(0))
        && fields.next().is_none()
}
#[cfg(target_os = "linux")]
fn untraced(pid: u32) -> Result<()> {
    use std::{io::Read, os::unix::fs::OpenOptionsExt};
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(format!("/proc/{pid}/status"))?;
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 65536 && tracer_absent(&bytes),
        "traced frontend ancestry refused"
    );
    Ok(())
}
#[cfg(target_os = "linux")]
fn same_user(pid: u32) -> Result<bool> {
    use std::os::unix::fs::MetadataExt;
    Ok(std::fs::metadata(format!("/proc/{pid}"))?.uid() == unsafe { libc::geteuid() })
}
#[cfg(target_os = "linux")]
fn agent_environment(pid: u32) -> Result<bool> {
    use std::{io::Read, os::unix::fs::OpenOptionsExt};
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(format!("/proc/{pid}/environ"))?;
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 65536 && (bytes.is_empty() || bytes.last() == Some(&0)),
        "frontend environment unavailable"
    );
    Ok(marked(&bytes))
}

#[cfg(target_os = "macos")]
fn same_user(pid: u32) -> Result<bool> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
    let size = std::mem::size_of::<libc::proc_bsdinfo>();
    // SAFETY: native PID and exactly sized writable output.
    let used = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size as i32,
        )
    };
    ensure!(used == size as i32, "frontend user identity unavailable");
    // SAFETY: proc_pidinfo initialized the complete output structure.
    Ok(unsafe { info.assume_init().pbi_uid == libc::geteuid() })
}
#[cfg(target_os = "macos")]
fn agent_environment(pid: u32) -> Result<bool> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as i32];
    let mut bytes = vec![0u8; 65536];
    let mut size = bytes.len();
    // SAFETY: fixed MIB and initialized writable buffer with its capacity.
    let code = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    ensure!(
        code == 0 && (4..=bytes.len()).contains(&size),
        "frontend environment unavailable"
    );
    bytes.truncate(size);
    let argc = i32::from_ne_bytes(bytes[..4].try_into()?);
    ensure!((1..=4096).contains(&argc), "frontend argv unavailable");
    let mut rest = &bytes[4..];
    rest = &rest[rest
        .iter()
        .position(|b| *b == 0)
        .context("invalid executable")?
        + 1..];
    while rest.first() == Some(&0) {
        rest = &rest[1..];
    }
    for _ in 0..argc {
        rest = &rest[rest.iter().position(|b| *b == 0).context("invalid argv")? + 1..];
    }
    Ok(marked(rest))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn same_user(_: u32) -> Result<bool> {
    anyhow::bail!("frontend ancestry is unavailable on this platform")
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn agent_environment(_: u32) -> Result<bool> {
    anyhow::bail!("frontend ancestry is unavailable on this platform")
}

#[cfg(test)]
mod tests {
    use super::*;
    use zerus_client_core::desktop::{
        ui_requests::{Evidence, Request, ABI},
        Build, Hello,
    };

    fn id(n: usize) -> String {
        format!("00000000-0000-4000-8000-{n:012x}")
    }
    fn fixture() -> (Envelope, Process, Process, Reply) {
        let envelope = Envelope {
            abi: ABI.into(),
            attempt: id(1),
            request: Request::AutomaticApprovals {
                request_id: id(2),
                enabled: true,
            },
        };
        let frontend = Process {
            pid: 7,
            identity: platform::ProcessIdentity {
                parent: 1,
                group: None,
                birth: "100".into(),
                started: 100.0,
            },
            executable: "/opt/example/zerus-client".into(),
        };
        let caller = Process {
            pid: 8,
            identity: platform::ProcessIdentity {
                parent: 7,
                group: None,
                birth: "101".into(),
                started: 101.0,
            },
            executable: "/opt/example/hgs".into(),
        };
        let reply = Reply {
            hello: Hello {
                abi: zerus_client_core::desktop::ABI.into(),
                version: 1,
                nonce: id(1),
                application: "com.hgdev.hgs-tray".into(),
                gui: "tauri".into(),
                build: Build {
                    version: "1.0.0".into(),
                    source_commit: "a".repeat(40),
                    source_dirty: Some(false),
                },
                instance_epoch: id(3),
                writer_epoch: None,
                configuration: Some("a".repeat(64)),
                live: true,
                replay: false,
            },
            request: Evidence {
                attempt: id(1),
                request_id: id(2),
                fingerprint: envelope.request.fingerprint(),
                child_pid: 8,
                child_birth: "101".into(),
                serial: 1,
            },
        };
        (envelope, caller, frontend, reply)
    }

    #[test]
    fn original_child_and_request_join_is_closed() {
        let config = "a".repeat(64);
        let (request, caller, frontend, reply) = fixture();
        validate(&request, &caller, &frontend, &reply, &config).unwrap();
        for changed in 0..10 {
            let (request, mut caller, mut frontend, mut reply) = fixture();
            match changed {
                0 => reply.request.child_pid += 1,
                1 => reply.request.child_birth = "reused".into(),
                2 => reply.request.fingerprint = "b".repeat(64),
                3 => reply.request.request_id = id(4),
                4 => reply.request.attempt = id(4),
                5 => caller.identity.parent = 99,
                6 => frontend.executable = "/bin/sh".into(),
                7 => reply.hello.gui = "qt".into(),
                8 => reply.hello.configuration = Some("b".repeat(64)),
                _ => reply.request.serial = 0,
            }
            assert!(validate(&request, &caller, &frontend, &reply, &config).is_err());
        }
    }

    #[test]
    fn a_retained_permit_must_still_be_fresh_at_the_mutation_boundary() {
        let (envelope, caller, frontend, _) = fixture();
        let permit = Validated(
            envelope.request,
            Freshness::Native {
                caller,
                frontend,
                until: Instant::now(),
            },
        );
        // Expiration refuses before attempting any lookup of these fake PIDs.
        assert!(permit.into_request().is_err());
    }

    #[test]
    fn a_child_without_markers_cannot_hide_a_marked_parent() {
        let end = Instant::now() + WAIT;
        let error = ancestry(8, end, |pid| match pid {
            8 => Ok(Some((7, false))),
            7 => Ok(Some((1, true))),
            _ => panic!("must stop at the agent parent"),
        })
        .unwrap_err();
        assert!(error.to_string().contains("agent-descended"));
        assert!(ancestry(8, end, |_| anyhow::bail!("unknown parent")).is_err());
        assert!(ancestry(8, end, |pid| Ok(Some((pid, false)))).is_err());
        assert!(ancestry(8, Instant::now(), |_| panic!("expired before inspection")).is_err());
    }

    #[test]
    fn native_order_refuses_replay_and_slow_earlier_intents() {
        let mut order = Order::default();
        let now = Instant::now();
        let a = (7, "100".into(), id(1));
        let b = (7, "200".into(), id(2));
        order.observe(a.clone(), 2, now).unwrap();
        assert!(order.observe(a.clone(), 1, now).is_err());
        assert!(order.observe(a.clone(), 2, now).is_err());
        order.observe(a.clone(), 3, now).unwrap();
        order.observe(b.clone(), 1, now).unwrap();
        assert!(order.observe(a.clone(), 4, now).is_err());
        order.observe(b, 2, now).unwrap();
        // An already authenticated request cannot survive WAIT, so the old
        // instance entry may expire without retaining unbounded history.
        order.observe(a, 5, now + WAIT).unwrap();
    }

    #[test]
    fn agent_marker_names_are_exact_and_never_logged() {
        assert!(marked(b"PATH=/bin\0HGS_SESSION=example\0"));
        assert!(marked(b"HGS_RUN_ID=\0"));
        assert!(!marked(b"X_HGS_SESSION=example\0HGS_SESSION_EXTRA=x\0"));
        for key in [
            "LD_PRELOAD",
            "LD_AUDIT",
            "DYLD_INSERT_LIBRARIES",
            "DYLD_FORCE_FLAT_NAMESPACE",
        ] {
            assert!(marked(format!("{key}=\0").as_bytes()));
            assert!(!marked(format!("EXTRA_{key}=value\0").as_bytes()));
        }
    }

    #[test]
    fn trace_status_is_required_and_not_inferred_from_a_missing_field() {
        assert!(tracer_absent(b"Name:\texample\nTracerPid:\t0\n"));
        for bytes in [
            b"TracerPid: 123\n".as_slice(),
            b"Name: example\n",
            b"TracerPid: 0\nTracerPid: 0\n",
            b"TracerPid: unknown\n",
            b"TracerPid: 4294967296\n",
        ] {
            assert!(!tracer_absent(bytes));
        }
    }

    #[test]
    fn maintained_launcher_search_paths_are_not_injection_primitives() {
        let launcher=b"LD_LIBRARY_PATH=/opt/example/webkit/lib\0HGS_LINUX_WEBKIT_ORIGINAL_ENV={}\0DYLD_LIBRARY_PATH=/opt/example/lib\0DYLD_FRAMEWORK_PATH=/opt/example/Frameworks\0";
        assert!(!marked(launcher));
        let mut injected = launcher.to_vec();
        injected.extend_from_slice(b"LD_PRELOAD=/opt/example/inject.so\0");
        assert!(marked(&injected));
    }

    #[test]
    fn a_native_looking_filename_does_not_select_the_installed_frontend() {
        let temp = tempfile::tempdir().unwrap();
        let installed = temp.path().join("installed/hgs-tray");
        let other = temp.path().join("other/hgs-tray");
        std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        std::fs::write(&installed, b"installed fixture").unwrap();
        std::fs::write(&other, b"same name, another program").unwrap();
        let scope = Scope {
            location: Location {
                root: temp.path().join("state/desktop"),
                configuration: "a".repeat(64),
            },
            frontend_images: vec![installed.clone()],
        };
        assert!(scope.selected_frontend(&installed));
        assert!(!scope.selected_frontend(&other));
        assert!(!scope.selected_frontend(&temp.path().join("missing/hgs-tray")));
    }
}
