mod accounts;
mod agent_guide;
mod analytics;
mod approval_log;
mod cli;
mod config;
mod desktop;
mod download;
mod host;
mod machines;
mod macos_session;
mod metrics;
#[cfg(unix)]
mod mobile_peers;
mod native_ui;
mod phones;
mod platform;
mod presence;
mod screen;
mod session_reference;
mod speech;
mod state;
mod status;
mod swarm;
mod tasks;
mod watch;

fn main() {
    if let Some(result) = state::creation_launch_status::early_os(std::env::args_os()) {
        let code = match result {
            Ok(code) => code,
            Err(error) => status_result_code(Err(error), &mut std::io::stderr().lock()),
        };
        std::process::exit(code);
    }
    if std::env::args().nth(1).as_deref() == Some("--macos-session-service") {
        std::process::exit(macos_session::serve());
    }
    // The Windows session host and its tools, before `cli::dispatch` puts
    // hgs.exe into its own kill-on-close job (spec D1, D4).
    #[cfg(windows)]
    if std::env::args().nth(1).as_deref() == Some("__host") {
        let args: Vec<String> = std::env::args().skip(2).collect();
        std::process::exit(host::run(&args));
    }
    let code = result_code(cli::dispatch(std::env::args().skip(1).collect()));
    std::process::exit(code);
}

fn status_result_code(result: cli::Result<i32>, output: &mut impl std::io::Write) -> i32 {
    match result {
        Ok(code) => code,
        Err(err) => {
            // Reserved status errors have fixed messages. Bound even an
            // unexpected message without clipping it or panicking on stderr I/O.
            let message = if err.message.len() <= 250 {
                err.message.as_str()
            } else {
                "The original launch observation is unavailable; review the original operation."
            };
            let line = format!("hgs: {message}\n");
            let _ = output.write_all(line.as_bytes());
            err.code
        }
    }
}

fn result_code(result: cli::Result<i32>) -> i32 {
    match result {
        Ok(code) => code,
        Err(err) => {
            // One write, preserving the existing error format on every path.
            let line = format!("hgs: {}\n", err.message);
            eprint!("{line}");
            err.code
        }
    }
}

#[cfg(test)]
mod status_output_tests {
    use super::*;

    #[test]
    fn status_output_is_bounded_without_clipping_unexpected_messages() {
        let mut output = Vec::new();
        assert_eq!(status_result_code(Ok(0), &mut output), 0);
        assert!(output.is_empty());
        assert_eq!(
            status_result_code(Err(cli::Error::new(2, "Fixed refusal.")), &mut output),
            2
        );
        assert_eq!(output, b"hgs: Fixed refusal.\n");
        output.clear();
        let private = "PRIVATE-FIXTURE".repeat(30);
        assert_eq!(
            status_result_code(Err(cli::Error::new(1, private)), &mut output),
            1
        );
        assert!(output.len() <= 256);
        assert!(!String::from_utf8(output)
            .unwrap()
            .contains("PRIVATE-FIXTURE"));
    }

    #[test]
    fn partial_status_diagnostic_failure_keeps_the_original_exit_without_panicking() {
        struct Partial(usize);
        impl std::io::Write for Partial {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.0 == 0 {
                    return Err(std::io::ErrorKind::BrokenPipe.into());
                }
                let written = self.0.min(bytes.len());
                self.0 -= written;
                Ok(written)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut output = Partial(3);
        assert_eq!(
            status_result_code(Err(cli::Error::new(2, "Fixed refusal.")), &mut output),
            2
        );
        assert_eq!(output.0, 0);
    }
}
