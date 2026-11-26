//! Narrow lifecycle admission for a live Linux Codex launch whose legacy
//! start stamp drifted. A published capability never carries a process handle.
use super::super::backend;
use super::super::{
    Result, codex_tasks, interrupt, native_launch::Witness, resume_input, string, supervisor,
    terminating,
};
use crate::platform::{self, PinnedProcess, ProcessState};
use crate::state::process_index::{Birth, Process};
use serde_json::Value;
use zerus_client_core::desktop::verified_send::Readiness;

const CHANGED: &str = "The exact original session process or terminal changed; nothing was stopped";

#[derive(Clone, Copy)]
pub(in crate::state) enum Operation {
    Pause,
    Stop,
    Terminate,
}

fn available(record: &Value, operation: Operation, pause_owner: Option<&Value>) -> bool {
    if record["agent"] != "codex"
        || !string(record, "expected_id").is_empty()
        || !string(record, "error").is_empty()
        || !record["last_event_at"].as_f64().is_some_and(|at| at > 0.0)
        || terminating::pending(record)
        || supervisor::owned_pending(record)
        || codex_tasks::multiple(record)
        || resume_input::pending(record)
        || resume_input::fork_pending(record)
        || ["paused", "starting", "clearing", "resuming"]
            .iter()
            .any(|key| !record[*key].is_null() && record[*key] != false)
        || [
            "resume_pending",
            "restore_archive_id",
            "restore_archive_cleanup",
            "session_end",
            "exited_at",
            "pending_settings",
        ]
        .iter()
        .any(|key| !record[*key].is_null())
        || pause_owner.map_or_else(
            || !record["pausing"].is_null(),
            |owner| &record["pausing"] != owner,
        )
    {
        return false;
    }
    match operation {
        Operation::Pause => super::safe_to_pause(record) && super::pause_recipe(record).is_ok(),
        Operation::Stop => {
            interrupt::eligible(record)
                && record["turn_started"].as_f64().is_some_and(|at| at > 0.0)
        }
        Operation::Terminate => true,
    }
}

/// Called only after this canonical row has passed the exact launch witness.
/// This publishes action eligibility without reserving or dispatching anything.
pub(in crate::state) fn add_capabilities(record: &Value, readiness: &mut Readiness) {
    readiness.pause = available(record, Operation::Pause, None);
    readiness.stop = available(record, Operation::Stop, None);
    readiness.terminate = available(record, Operation::Terminate, None);
}

pub(in crate::state) fn observe(record: &Value, operation: Operation) -> Option<Witness> {
    Witness::read_checked(record, |current| available(current, operation, None))
}

enum TerminalEffect {
    Interrupt,
    Terminate,
    CloseDeadPane,
}

pub(in crate::state) struct Original {
    terminal_server: u32,
    witness: Witness,
    child: PinnedProcess,
    supervisor: PinnedProcess,
}

pub(in crate::state) fn birth(process: &Process) -> Result<String> {
    match process.birth {
        Birth::Linux(ticks) => Ok(ticks.to_string()),
        _ => Err(CHANGED.into()),
    }
}

impl Original {
    pub(in crate::state) fn pin(witness: Witness) -> Result<Self> {
        let (child, supervisor) = witness.originals();
        let mut caller = std::process::id();
        for _ in 0..256 {
            if caller == child.pid || caller == supervisor.pid {
                return Err(
                    "Run this action from outside the original session process tree".into(),
                );
            }
            if caller <= 1 {
                let child = PinnedProcess::pin(child.pid, &birth(child)?)?;
                let supervisor = PinnedProcess::pin(supervisor.pid, &birth(supervisor)?)?;
                let terminal_server = backend::current()
                    .pane_info(
                        backend::Target::Session(witness.terminal_session_id()),
                        &[backend::PaneField::ServerPid],
                    )?
                    .value()
                    .parse::<u32>()
                    .map_err(|_| CHANGED)?;
                let original = Self {
                    terminal_server,
                    witness,
                    child,
                    supervisor,
                };
                original.check_terminal(ProcessState::Running)?;
                return Ok(original);
            }
            caller = platform::parent_and_name(caller)
                .map_err(|_| CHANGED)?
                .0
                .ok_or(CHANGED)?;
        }
        Err("Could not verify the action caller's original process ancestry".into())
    }

    pub(in crate::state) fn recheck(
        &self,
        record: &Value,
        operation: Operation,
        pause_owner: Option<&Value>,
    ) -> Result<()> {
        let current =
            Witness::read_checked(record, |current| available(current, operation, pause_owner));
        if current.as_ref() != Some(&self.witness) {
            return Err(CHANGED.into());
        }
        self.check_terminal(ProcessState::Running)
    }

    pub(in crate::state) fn check_terminal(&self, child_state: ProcessState) -> Result<()> {
        let (child, supervisor) = self.witness.originals();
        if !self.witness.terminal_is_current()
            || self.child.state()? != child_state
            || self.supervisor.state()? != ProcessState::Running
            || platform::process_identity(child.pid)
                .is_none_or(|identity| identity.parent != supervisor.pid)
        {
            return Err(CHANGED.into());
        }
        Ok(())
    }

    pub(in crate::state) fn child_birth(&self) -> Result<String> {
        birth(self.witness.originals().0)
    }

    pub(in crate::state) fn terminal_session_id(&self) -> &str {
        self.witness.terminal_session_id()
    }

    pub(in crate::state) fn interrupt_terminal(&self, record: &Value) -> Result<()> {
        self.terminal_effect(record, TerminalEffect::Interrupt)
    }

    pub(in crate::state) fn kill_terminal(&self, record: &Value) -> Result<()> {
        self.terminal_effect(record, TerminalEffect::Terminate)
    }

    pub(in crate::state) fn close_dead_pane(&self, record: &Value) -> Result<()> {
        self.terminal_effect(record, TerminalEffect::CloseDeadPane)
    }

    fn terminal_effect(&self, record: &Value, kind: TerminalEffect) -> Result<()> {
        let pane = string(record, "pane");
        let run = string(record, "run_id");
        let session = string(record, "session_id");
        if !pane
            .strip_prefix('%')
            .is_some_and(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
            || [run, session]
                .iter()
                .any(|id| !zerus_client_core::creation::canonical_uuid(id))
        {
            return Err(CHANGED.into());
        }
        let state = match kind {
            TerminalEffect::Interrupt => Some(ProcessState::Running),
            TerminalEffect::Terminate => Some(ProcessState::Stopped),
            TerminalEffect::CloseDeadPane => None,
        };
        if let Some(state) = state {
            self.check_terminal(state)?;
        }
        let dead = state.is_none();
        let supervisor = self.witness.originals().1.pid.to_string();
        let server = self.terminal_server.to_string();
        let terminal = self.terminal_session_id();
        let checks = [
            ("pane_id", pane),
            ("pane_pid", &supervisor),
            ("session_id", terminal),
            ("@hgs_run", run),
            ("HGS_SESSION_ID", session),
            ("pid", &server),
            ("pane_dead", if dead { "1" } else { "0" }),
        ];
        // Keep the final pane comparison and effect in the same tmux command
        // queue. An independent tmux client need not take the HGS lock.
        let mut condition = if matches!(kind, TerminalEffect::Terminate) {
            "#{&&:#{==:#{session_windows},1},#{==:#{window_panes},1}}".to_owned()
        } else {
            "1".to_owned()
        };
        for (key, value) in checks {
            condition = format!("#{{&&:#{{==:#{{{key}}},{value}}},{condition}}}");
        }
        let effect = match kind {
            TerminalEffect::Interrupt => format!("send-keys -t {pane} Escape"),
            TerminalEffect::Terminate => format!("kill-session -t {terminal}"),
            TerminalEffect::CloseDeadPane => format!("kill-pane -t {pane}"),
        };
        let output = platform::output_until(
            std::process::Command::new("tmux").args([
                "if-shell",
                "-F",
                "-t",
                pane,
                &condition,
                &effect,
                "display-message -p HGS_ORIGINAL_TERMINAL_CHANGED",
            ]),
            std::time::Duration::from_secs(3),
        )
        .map_err(|_| "Could not confirm the terminal result")?;
        if output.status.success() && output.stdout == b"HGS_ORIGINAL_TERMINAL_CHANGED\n" {
            // Pause already settled its original process. A different pane
            // must remain open even if it now occupies the same pane ID.
            return if dead { Ok(()) } else { Err(CHANGED.into()) };
        }
        if !output.status.success() || !output.stdout.is_empty() {
            return Err("Could not confirm the terminal result".into());
        }
        Ok(())
    }

    pub(in crate::state) fn request_exit(&self) -> Result<()> {
        self.child.terminate()
    }

    pub(in crate::state) fn child_exited(&self) -> Result<bool> {
        self.child.exit_observed().map_err(|e| e.to_string())
    }
}
