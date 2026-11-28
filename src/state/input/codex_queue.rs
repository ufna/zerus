//! Exact-thread native queue for a TUI that owns multiple independent tasks.
//! It never types into whichever task happens to be visible in the terminal.
use super::*;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::platform::codex_image;
use crate::state::codex_tasks::history::HistoryRoot;
use std::io::{BufRead, BufReader, Read};
use std::time::Instant;

pub(super) struct Queue {
    command: std::process::Command,
    executable: PathBuf,
    held: fs::File,
    file: crate::platform::FileIdentity,
    account: HistoryRoot,
    transcript: PathBuf,
    resolved_transcript: PathBuf,
    _history: fs::File,
    history_object: (u64, u64),
    pid: u32,
    process: crate::platform::ProcessIdentity,
    conversation: String,
    until: Instant,
    busy_outbox: bool,
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    mac_queue: Option<(codex_image::Image, codex_image::Pending)>,
}

fn reply(stdout: &[u8], conversation: &str) -> Option<String> {
    let line = std::str::from_utf8(stdout).ok()?.strip_suffix('\n')?;
    let (id, target) = line
        .strip_prefix("Queued message ")?
        .split_once(" for thread ")?;
    if target != format!("{conversation}.")
        || !uuid::Uuid::parse_str(id).is_ok_and(|value| !value.is_nil() && value.to_string() == id)
    {
        return None;
    }
    Some(id.into())
}

#[cfg(target_os = "linux")]
fn original_image(pid: u32, expected: crate::platform::FileIdentity) -> bool {
    crate::platform::file_identity(Path::new(&format!("/proc/{pid}/exe"))) == Some(expected)
}

#[cfg(not(target_os = "linux"))]
fn original_image(_: u32, _: crate::platform::FileIdentity) -> bool {
    // These platforms retain the original matched-version and executable-path checks.
    true
}

/// Read the exact native thread's queue before selecting whether a fresh
/// Desktop Send may steer. The input writer serializes this choice with
/// previous Desktop admissions, including a head whose ACK has settled.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) fn has_earlier(record: &Value) -> Result<bool> {
    let account = HistoryRoot::open(record)?;
    let transcript = account.resolve_transcript(Path::new(string(record,"transcript")))?;
    let pid = record["pid"].as_u64().and_then(|p| u32::try_from(p).ok()).ok_or("Codex process identity unavailable")?;
    if !process_alive(record) || !crate::platform::holds_open(pid, &transcript).unwrap_or(false) {
        return Err("Could not verify the account of this Codex session".into());
    }
    let mut file = crate::platform::no_follow(OpenOptions::new().read(true)).open(&transcript)
        .map_err(|_| "The Codex thread transcript is unavailable")?;
    provider_messages::first_line(&mut file, "codex", string(record, "conversation_id"))?;
    let earlier = codex_sqlite::has_queued_input(account.path(), string(record,"conversation_id"),
        Instant::now() + Duration::from_millis(350))?;
    account.verify()?;
    Ok(earlier)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) fn has_earlier(_: &Value) -> Result<bool> {
    Err("Ordered Codex Send is unavailable on this host".into())
}

impl Queue {
    pub(super) fn prepare(record: &Value, text: &str, busy_outbox: bool) -> Result<Self> {
        let until = Instant::now() + Duration::from_secs(10);
        let conversation = string(record, "conversation_id");
        if !uuid::Uuid::parse_str(conversation)
            .is_ok_and(|id| !id.is_nil() && id.to_string() == conversation)
            || !process_alive(record)
        {
            return Err("Codex task identity is unavailable; no message was queued".into());
        }
        let pid = record["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
            .ok_or("Codex process identity is unavailable")?;
        let process = crate::platform::process_identity(pid).ok_or("Codex process exited")?;
        let image = crate::platform::describe(pid).ok_or("Codex executable is unavailable")?;
        let executable = PathBuf::from(image.executable);
        if !executable.is_absolute()
            || !matches!(
                crate::platform::program_name(&executable).as_deref(),
                Some("codex" | "codex.exe")
            )
        {
            return Err(
                "This Codex task needs its native queue; no Terminal input was sent".into(),
            );
        }
        let held = crate::platform::no_follow(OpenOptions::new().read(true))
            .open(&executable)
            .map_err(|_| "Codex executable could not be opened; no message was queued")?;
        let file = crate::platform::FileIdentity::with_metadata(&held)
            .map_err(|_| "Codex executable metadata is unavailable; no message was queued")?
            .0;
        // A rollout header records the version that CREATED the thread. A
        // resumed TUI may run a newer binary. Linux runs the held mapped file.
        // Apple Silicon verifies the kernel's loaded code before any helper runs.
        #[cfg(target_os = "linux")]
        let pinned_image = busy_outbox;
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let mac_image = if busy_outbox {
            Some(codex_image::Image::read(pid).ok_or("The running Codex image could not be verified")?)
        } else {
            None
        };
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let pinned_image = mac_image.is_some();
        #[cfg(not(any(target_os = "linux", all(target_os = "macos", target_arch = "aarch64"))))]
        let pinned_image = false;
        if pinned_image && !original_image(pid, file) {
            return Err("The queue executable differs from the running Codex image".into());
        }
        let run = || {
            #[cfg(target_os = "linux")]
            if pinned_image {
                use std::os::{fd::AsRawFd, unix::process::CommandExt};
                let mut command = std::process::Command::new(format!("/proc/self/fd/{}", held.as_raw_fd()));
                command.arg0(&executable);
                return command;
            }
            std::process::Command::new(&executable)
        };
        let probe_queue = |command: &mut std::process::Command, limits| {
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            if let Some(image) = &mac_image {
                return codex_image::Pending::prepare(command, image, until)?
                    .run(image, limits, until);
            }
            crate::platform::run_bounded(command, limits, until, &|| false)
        };
        let account = HistoryRoot::open(record)?;
        let transcript = PathBuf::from(string(record, "transcript"));
        let resolved_transcript = account.resolve_transcript(&transcript)?;
        let mut history = crate::platform::no_follow(OpenOptions::new().read(true))
            .open(&transcript)
            .map_err(|_| "Codex native task history could not be opened; no message was queued")?;
        let (identity, metadata) =
            crate::platform::FileIdentity::with_metadata(&history).map_err(|_| {
                "Codex native task history metadata is unavailable; no message was queued"
            })?;
        if !metadata.is_file()
            || !crate::platform::holds_open(pid, &resolved_transcript)
                .map_err(|_| "Codex native task file ownership is unavailable".to_owned())?
        {
            return Err("The original Codex process does not hold this task history".into());
        }
        let mut first = Vec::new();
        BufReader::new((&mut history).take((64 << 10) + 1))
            .read_until(b'\n', &mut first)
            .map_err(|_| "Codex native task history could not be read; no message was queued")?;
        if first.len() > 64 << 10 || !first.ends_with(b"\n") {
            return Err("Codex history header exceeds its bound".into());
        }
        let header: Value = serde_json::from_slice(&first)
            .map_err(|_| "Codex native task history header is invalid; no message was queued")?;
        if header["type"] != "session_meta" || header["payload"]["id"] != conversation {
            return Err("Codex native task history identity changed".into());
        }
        let version = string(&header["payload"], "cli_version");
        if version.is_empty() || version.len() > 32 {
            return Err("Codex native task version is unavailable".into());
        }
        account.verify()?;
        let mut probe = run();
        for (key, _) in
            std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("HGS_"))
        {
            probe.env_remove(key);
        }
        probe.env("CODEX_HOME", account.path()).arg("--version");
        let (status, stdout, _) =
            probe_queue(&mut probe, [1024, 4096])
                .map_err(|_| "Codex queue version is unavailable".to_owned())?;
        let runtime = std::str::from_utf8(&stdout).ok().and_then(|line| line.trim().strip_prefix("codex-cli "));
        let valid = runtime.is_some_and(|v| !v.is_empty() && v.len() <= 32 && v.as_bytes()[0].is_ascii_digit()
            && v.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'+')));
        if !status.success() || !valid || (!pinned_image && runtime != Some(version)) {
            return Err("The native queue version differs from the running Codex task".into());
        }
        account.verify()?;
        let mut help = run();
        for (key, _) in
            std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("HGS_"))
        {
            help.env_remove(key);
        }
        help.env("CODEX_HOME", account.path())
            .args(["queue", "--help"]);
        let (status, output, _) =
            probe_queue(&mut help, [16 << 10, 4096])
                .map_err(|_| "Codex queue capability is unavailable".to_owned())?;
        let capability =
            std::str::from_utf8(&output).map_err(|_| "Codex queue help is unavailable")?;
        if !status.success()
            || !["--thread", "--message", "--resume"]
                .iter()
                .all(|flag| capability.split_whitespace().any(|word| word == *flag))
        {
            return Err("This native Codex does not support exact-thread queued input".into());
        }
        if text.len() > 96 << 10 || (cfg!(windows) && text.encode_utf16().count() > 24_000) {
            return Err(
                "This message exceeds the native Codex queue argument bound; nothing was queued"
                    .into(),
            );
        }
        let mut command = run();
        for (key, _) in
            std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("HGS_"))
        {
            command.env_remove(key);
        }
        command
            .env("CODEX_HOME", account.path())
            .current_dir(string(record, "cwd"));
        // No --resume: this operation cannot launch a daemon or a new model task.
        command.args(["queue", "--thread", conversation, "--message", text]);
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let mac_queue = match mac_image {
            Some(image) => {
                // Check the actual offer's loaded image before the caller
                // reserves the receipt. It stays suspended until offer().
                let pending = codex_image::Pending::prepare(&command, &image, until)
                    .map_err(|_| "The native queue helper image could not be verified")?;
                Some((image, pending))
            }
            None => None,
        };
        let queue = Self {
            command,
            executable,
            held,
            file,
            account,
            transcript,
            resolved_transcript,
            _history: history,
            history_object: (identity.device, identity.inode),
            pid,
            process,
            conversation: conversation.into(),
            until,
            busy_outbox,
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            mac_queue,
        };
        queue.verify()?;
        Ok(queue)
    }

    fn verify(&self) -> Result<()> {
        self.account.verify()?;
        if self.account.resolve_transcript(&self.transcript)? != self.resolved_transcript
            || crate::platform::file_identity(&self.transcript).map(|id| (id.device, id.inode))
                != Some(self.history_object)
        {
            return Err("Codex native task history changed during queue preparation".into());
        }
        if self.busy_outbox && (!original_image(self.pid, self.file)
            || crate::platform::holds_open(self.pid, &self.resolved_transcript).ok() != Some(true))
        {
            return Err("The original Codex process no longer owns the queue image or thread".into());
        }
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        if self.mac_queue.as_ref().is_some_and(|(image, _)| !image.current()) {
            return Err("The running Codex image changed during queue preparation".into());
        }
        if crate::platform::process_identity(self.pid).as_ref() != Some(&self.process)
            || crate::platform::file_identity(&self.executable) != Some(self.file)
            || crate::platform::FileIdentity::with_metadata(&self.held)
                .ok()
                .map(|v| v.0)
                != Some(self.file)
        {
            return Err("Codex original process or native queue executable changed".into());
        }
        Ok(())
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(super) fn client_id(&self, queue_id: &str) -> Option<String> {
        let ids = [queue_id.to_owned()];
        let mut clients = codex_sqlite::queued_clients(self.account.path(), &self.conversation, &ids,
            Instant::now() + Duration::from_millis(120)).ok()?;
        self.account.verify().ok()?;
        clients.remove(queue_id)
    }

    /// Once invoked, every failure is delivery-unknown. The caller reserved the
    /// original request first, so no caller can replay this native queue write.
    pub(super) fn offer(&mut self) -> Result<String> {
        self.verify()?;
        let limits = [4096, 8192];
        let mut run = || crate::platform::run_bounded(&mut self.command, limits, self.until, &|| false);
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let queued = if self.busy_outbox {
            let (image, pending) = self.mac_queue.take().ok_or("The original Codex queue helper is unavailable")?;
            let result = pending.run(&image, limits, self.until);
            if !image.current() {
                return Err("The running Codex image changed during the native queue offer".into());
            }
            result
        } else {
            run()
        };
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        let queued = run();
        let (status, stdout, _) =
            queued.map_err(|_| "Codex native queue outcome is unknown".to_owned())?;
        if !status.success() {
            return Err("Codex native queue did not confirm submission; do not replay it".into());
        }
        self.verify()?;
        reply(&stdout, &self.conversation)
            .ok_or_else(|| "Codex native queue returned an unknown receipt".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_one_exact_native_queue_identity_is_accepted() {
        let target = uuid::Uuid::new_v4().to_string();
        let queued = uuid::Uuid::new_v4().to_string();
        let body = format!("Queued message {queued} for thread {target}.\n");
        assert_eq!(reply(body.as_bytes(), &target), Some(queued));
        assert!(reply(body.as_bytes(), &uuid::Uuid::new_v4().to_string()).is_none());
        assert!(reply(format!("{body}extra\n").as_bytes(), &target).is_none());
        assert!(reply(b"Queued successfully\n", &target).is_none());
    }
}
