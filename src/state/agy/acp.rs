//! One original Antigravity ACP v1 child. The client offers no file, terminal,
//! permission, or authentication capability.
use super::*;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::fd::AsRawFd,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
};

const MAX_FRAME: usize = 512 << 10;

pub(super) enum Reply {
    Result(Value),
    AuthRequired,
    Refusal,
}

pub(super) struct Client {
    child: Child,
    input: Option<ChildStdin>,
    frames: Receiver<Result<Value>>,
    request: u64,
}

impl Client {
    pub(super) fn start(binary: &Path, cwd: &Path) -> Result<Self> {
        let mut child = Command::new(binary)
            .args(["acp", "--disable-telemetry"])
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Could not start Antigravity: {e}"))?;
        let (tx, frames) = mpsc::sync_channel(32);
        let stdout = child.stdout.take().expect("ACP stdout is piped");
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                let frame = match Read::by_ref(&mut reader)
                    .take(MAX_FRAME as u64 + 1)
                    .read_until(b'\n', &mut line)
                {
                    Ok(0) => break,
                    Ok(_) if line.len() <= MAX_FRAME && line.ends_with(b"\n") => {
                        serde_json::from_slice(&line)
                            .map_err(|_| "Malformed Antigravity ACP frame".into())
                    }
                    _ => Err("Antigravity ACP frame is unavailable or too large".into()),
                };
                let failed = frame.is_err();
                if tx.send(frame).is_err() || failed {
                    break;
                }
            }
        });
        let input = child.stdin.take().expect("ACP stdin is piped");
        Ok(Self {
            child,
            input: Some(input),
            frames,
            request: 0,
        })
    }

    pub(super) fn prepare_input(&self) -> Result<()> {
        let fd = self.input.as_ref().expect("ACP input exists").as_raw_fd();
        // This is our private pipe; another ACP process never shares this end.
        if unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
            return Err("Could not bound Antigravity ACP writes".into());
        }
        Ok(())
    }

    fn write(&mut self, frame: Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(&frame).map_err(|_| "Invalid ACP request")?;
        bytes.push(b'\n');
        let input = self.input.as_mut().ok_or("Antigravity input is closed")?;
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut pending = bytes.as_slice();
        while !pending.is_empty() {
            if Instant::now() >= deadline {
                return Err("Antigravity did not read its input in time".into());
            }
            match input.write(pending) {
                Ok(0) => return Err("Antigravity input closed unexpectedly".into()),
                Ok(n) => pending = &pending[n..],
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return Err("Antigravity input closed unexpectedly".into()),
            }
        }
        Ok(())
    }

    pub(super) fn request(&mut self, method: &str, params: Value) -> Result<u64> {
        self.request += 1;
        self.write(json!({"jsonrpc":"2.0","id":self.request,"method":method,"params":params}))?;
        Ok(self.request)
    }

    pub(super) fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.write(json!({"jsonrpc":"2.0","method":method,"params":params}))
    }

    pub(super) fn receive(&mut self, wait: Duration) -> Result<Option<Value>> {
        match self.frames.recv_timeout(wait) {
            Ok(frame) => {
                let frame = frame?;
                if frame["jsonrpc"] != "2.0" {
                    return Err("Unsupported Antigravity RPC envelope".into());
                }
                if let Some(method) = frame["method"].as_str() {
                    if let Some(id) = frame.get("id") {
                        let response = if method == "session/request_permission" {
                            json!({"jsonrpc":"2.0","id":id,"result":{"outcome":{"outcome":"cancelled"}}})
                        } else {
                            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Client capability unavailable"}})
                        };
                        self.write(response)?;
                        return Ok(None);
                    }
                }
                Ok(Some(frame))
            }
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err("Antigravity ACP disconnected".into()),
        }
    }

    pub(super) fn reply(frame: Value, id: u64) -> Result<Option<Reply>> {
        if frame.get("method").is_some() || frame["id"] != id {
            return Ok(None);
        }
        match (frame.get("result"), frame.get("error")) {
            (Some(value), None) => Ok(Some(Reply::Result(value.clone()))),
            (None, Some(error)) => match error["code"].as_i64() {
                Some(-32000) if error["message"] == "Authentication required" => {
                    Ok(Some(Reply::AuthRequired))
                }
                Some(_) => Ok(Some(Reply::Refusal)),
                None => Err("Invalid Antigravity refusal".into()),
            },
            _ => Err("Invalid Antigravity ACP response".into()),
        }
    }

    pub(super) fn call(&mut self, method: &str, params: Value, until: Instant) -> Result<Reply> {
        let id = self.request(method, params)?;
        while Instant::now() < until {
            if let Some(frame) = self.receive(until.saturating_duration_since(Instant::now()))? {
                if let Some(result) = Self::reply(frame, id)? {
                    return Ok(result);
                }
            }
        }
        Err("Antigravity did not confirm the request in time".into())
    }

    pub(super) fn pid(&self) -> u32 {
        self.child.id()
    }

    pub(super) fn finish(&mut self, until: Instant) -> Result<()> {
        self.input.take();
        while Instant::now() < until {
            if let Some(exit) = self
                .child
                .try_wait()
                .map_err(|_| "Could not verify Antigravity exit")?
            {
                return if exit.success() {
                    Ok(())
                } else {
                    Err("Antigravity exited with an error".into())
                };
            }
            thread::sleep(Duration::from_millis(15));
        }
        Err("Antigravity exit is unconfirmed".into())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.input.take();
        // An unconfirmed normal close conveys no authority to force-stop.
        let _ = self.child.try_wait();
    }
}
