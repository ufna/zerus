//! Account setup uses the same catalog revision and physical-computer guard.
use super::{run_catalog, Binding, CHANGED, MAX_MEMBERS};
use crate::{actions::Invocation, terminal_launch::System, ComputerId};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};

pub const ANTIGRAVITY_LOGIN_MARKER: &str = "antigravity-native-login-v1";

fn antigravity_program(hgs: &Path) -> Option<String> {
    let selected = if hgs.file_name() == Some(hgs.as_os_str()) {
        System::find_path(hgs.to_str()?)?
    } else {
        hgs.to_path_buf()
    };
    // Configured terminals start in the user's home. Resolve a relative
    // override in the desktop's directory before the help probe and launch.
    let program = std::path::absolute(selected).ok()?;
    // Terminal commands are UTF-8. Never turn an unrepresentable path into
    // a different executable path by substituting replacement characters.
    program.into_os_string().into_string().ok()
}

/// Resolve and verify the binary that will forward the interactive command.
pub fn antigravity_forwarder(hgs: &Path) -> Option<String> {
    let program = antigravity_program(hgs)?;
    let call = Invocation {
        args: vec!["--help".into()],
        stdin: None,
        timeout: Duration::from_secs(5),
        request_id: None,
    };
    matches!(crate::run::run_limited(Path::new(&program), &call, &[], 64 * 1024),
        Ok(crate::Exit::Code { code: 0, stdout, .. })
            if String::from_utf8_lossy(&stdout).lines().any(|line| line.trim() == ANTIGRAVITY_LOGIN_MARKER))
    .then_some(program)
}

/// The single native Antigravity account advertised by this catalog.
pub fn antigravity(catalog: &Value) -> Option<&Value> {
    let mut rows = catalog["profiles"]
        .as_array()?
        .iter()
        .filter(|row| row["id"] == "native-agy");
    let row = rows.next()?;
    (rows.next().is_none() && row["provider"] == "agy" && row["native"] == true).then_some(row)
}

#[derive(Clone, Debug)]
pub struct Host {
    pub computer: ComputerId,
    pub machine_id: String,
    pub revision: String,
    pub setup_revision: bool,
}
impl Host {
    pub fn capture(
        computer: ComputerId,
        machine_id: String,
        catalog: &Value,
    ) -> Result<Self, String> {
        if (!computer.is_local() && !crate::fleet::valid_alias(&computer.0))
            || !crate::creation::canonical_uuid(&machine_id)
            || catalog["revision"]
                .as_str()
                .is_none_or(|r| r.is_empty() || r.len() > 256)
            || catalog["profiles"]
                .as_array()
                .is_none_or(|a| a.len() > MAX_MEMBERS)
            || catalog["removed_profiles"]
                .as_array()
                .is_some_and(|a| a.len() > MAX_MEMBERS)
        {
            return Err(CHANGED.into());
        }
        Ok(Self {
            computer,
            machine_id,
            revision: catalog["revision"].as_str().unwrap().into(),
            setup_revision: catalog["setup_revision"] == true,
        })
    }
    pub fn verify(&self, catalog: &Value) -> Result<(), String> {
        if catalog["revision"] != self.revision
            || (catalog["setup_revision"] == true) != self.setup_revision
        {
            return Err(CHANGED.into());
        }
        Ok(())
    }
    fn invocation(&self, args: &[&str]) -> Invocation {
        // Reuse the existing argument quoting/physical identity boundary.
        Binding {
            computer: self.computer.clone(),
            machine_id: self.machine_id.clone(),
            revision: self.revision.clone(),
            id: String::new(),
            provider: String::new(),
            home: String::new(),
            auth_revision: String::new(),
        }
        .invocation(args)
    }
    fn call(&self, op: &str, id: &str) -> Invocation {
        self.invocation(&[op, id, "--revision", &self.revision])
    }
    pub fn terminal_call(&self, op: &str, id: &str) -> Result<Invocation, String> {
        if !self.setup_revision {
            return Err("Update hgs on this machine to continue verified account setup.".into());
        }
        Ok(self.call(op, id))
    }
    pub fn antigravity_login(&self, catalog: &Value) -> Result<Invocation, String> {
        self.verify(catalog)?;
        let _ = antigravity(catalog)
            .filter(|row| row["installed"] == true && row["native_login_supported"] == true)
            .ok_or(CHANGED)?;
        self.terminal_call("login", "native-agy")
    }
    pub fn antigravity_terminal(
        &self,
        hgs: &Path,
        catalog: &Value,
    ) -> Result<(String, Invocation), String> {
        let call = self.antigravity_login(catalog)?;
        let program = if self.computer.is_local() {
            antigravity_program(hgs).ok_or("Could not resolve the local hgs executable.")?
        } else {
            antigravity_forwarder(hgs).ok_or(
                "Update the local hgs before opening Antigravity sign-in on another computer.",
            )?
        };
        Ok((program, call))
    }
    pub fn fresh(&self, hgs: &Path) -> Result<Value, String> {
        let catalog = self.read(hgs)?;
        self.verify(&catalog)?;
        Ok(catalog)
    }
    pub fn read(&self, hgs: &Path) -> Result<Value, String> {
        let call = self.invocation(&["ls"]);
        run_catalog(hgs, &call)
    }
    pub fn create(
        &self,
        hgs: &Path,
        id: &str,
        provider: &str,
        label: &str,
    ) -> Result<Value, String> {
        if !["codex", "claude", "kimi"].contains(&provider) || !valid_id(id) || !valid_label(label)
        {
            return Err("Enter a valid account name and agent.".into());
        }
        let catalog = self.fresh(hgs)?;
        if ["profiles", "removed_profiles"].iter().any(|f| {
            catalog[*f]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|r| r["id"] == id))
        }) {
            return Err(CHANGED.into());
        }
        let mut call = self.call("add", id);
        call.args.extend([
            "--provider".into(),
            provider.into(),
            "--label".into(),
            label.trim().into(),
        ]);
        run_catalog(hgs, &call)
    }
    pub fn restore(&self, hgs: &Path, saved: &Value) -> Result<Value, String> {
        let current = self.fresh(hgs)?;
        let id = saved["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or(CHANGED)?;
        let mut matches = current["removed_profiles"]
            .as_array()
            .ok_or(CHANGED)?
            .iter()
            .filter(|r| r["id"] == id);
        let row = matches.next().ok_or(CHANGED)?;
        if matches.next().is_some()
            || ["provider", "home", "auth_revision"]
                .iter()
                .any(|f| row[*f] != saved[*f])
        {
            return Err(CHANGED.into());
        }
        run_catalog(hgs, &self.call("restore", id))
    }
    pub fn start(&self, hgs: &Path, id: &str) -> Result<Value, String> {
        if id != "native-opencode" {
            return Err(CHANGED.into());
        }
        self.fresh(hgs)?;
        let exit =
            crate::run::run_limited(hgs, &self.terminal_call("start", id)?, &[], 4 * 1024 * 1024)
                .map_err(|_| CHANGED)?;
        match exit {
            crate::Exit::Code { code: 0, .. } => self.fresh(hgs),
            other => Err(crate::actions::failure_detail(other)),
        }
    }
    pub fn set_key(&self, hgs: &Path, label: Option<&str>, key: &str) -> Result<Value, String> {
        if key.is_empty()
            || key.len() > 4096
            || key.chars().any(|c| c.is_control() || c.is_whitespace())
            || label.is_some_and(|s| !valid_label(s))
        {
            return Err("Enter a non-empty API key without spaces or line breaks.".into());
        }
        let current = self.fresh(hgs)?;
        if !["profiles", "removed_profiles"].iter().any(|f| {
            current[*f].as_array().is_some_and(|rows| {
                rows.iter()
                    .any(|r| r["id"] == "native-dsh" && r["installed"] == true)
            })
        }) {
            return Err("Install DeepSeek on this machine before saving its API key.".into());
        }
        let mut call = self.call("set-key", "native-dsh");
        call.args.push("--json".into());
        let mut value = json!({"api_key":key});
        if let Some(label) = label {
            value["label"] = json!(label);
        }
        call.stdin = Some(serde_json::to_vec(&value).map_err(|_| CHANGED)?);
        run_catalog(hgs, &call)
    }
}
pub fn valid_label(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty() && s.chars().count() <= 160 && !s.chars().any(char::is_control)
}
pub fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 48
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        && s.as_bytes()[0].is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setup_keeps_physical_identity_and_revision_and_validates_inputs() {
        let h = Host::capture(
            ComputerId("studio".into()),
            "11111111-1111-4111-8111-111111111111".into(),
            &json!({"revision":"r1","profiles":[]}),
        )
        .unwrap();
        assert_eq!(
            h.call("login", "example").args,
            [
                "@studio",
                "--expect-machine",
                "11111111-1111-4111-8111-111111111111",
                "account",
                "login",
                "example",
                "--revision",
                "r1"
            ]
        );
        assert!(h.verify(&json!({"revision":"r2"})).is_err());
        assert!(h.terminal_call("login", "example").is_err());
        let mut supported = h.clone();
        supported.setup_revision = true;
        assert!(supported.terminal_call("login", "example").is_ok());
        assert!(valid_label(" Work "));
        assert!(!valid_label("bad\nname"));
        assert!(valid_id("codex-example"));
        assert!(!valid_id("--other"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn antigravity_remote_terminal_requires_fresh_local_transport_capability() {
        struct Scratch(std::path::PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let path = std::env::temp_dir().join(format!("agy-origin-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        let dir = Scratch(path);
        let hgs = dir.0.join("hgs");
        let help = |line: &str, status: u8| {
            crate::test_executable::write_executable(
                &hgs,
                format!("#!/bin/sh\n[ \"$*\" = --help ] || exit 9\nprintf '%s\\n' '{line}'\nexit {status}\n"),
            );
        };
        let machine = "11111111-1111-4111-8111-111111111111";
        let catalog = json!({"revision":"r1","setup_revision":true,"profiles":[{
            "id":"native-agy","provider":"agy","native":true,"installed":true,"native_login_supported":true
        }]});
        let peer =
            Host::capture(ComputerId("mac-fixture".into()), machine.into(), &catalog).unwrap();
        assert!(peer.antigravity_login(&catalog).is_ok());
        help("Account transfer ABI: revision-copy-v1", 0);
        assert!(peer.antigravity_terminal(&hgs, &catalog).is_err());
        help(ANTIGRAVITY_LOGIN_MARKER, 1);
        assert!(peer.antigravity_terminal(&hgs, &catalog).is_err());
        help(&format!("missing {ANTIGRAVITY_LOGIN_MARKER}"), 0);
        assert!(peer.antigravity_terminal(&hgs, &catalog).is_err());
        help(&format!("       {ANTIGRAVITY_LOGIN_MARKER}"), 0);
        let (program, _) = peer.antigravity_terminal(&hgs, &catalog).unwrap();
        assert!(Path::new(&program).is_absolute());
        assert_eq!(Path::new(&program), hgs);
        help("older executable", 0);
        assert!(peer.antigravity_terminal(&hgs, &catalog).is_err());
        let local = Host::capture(ComputerId::local(), machine.into(), &catalog).unwrap();
        assert!(local.antigravity_terminal(&hgs, &catalog).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn antigravity_terminal_refuses_an_executable_path_it_cannot_preserve() {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
        let hgs = Path::new(OsStr::from_bytes(b"/fixture/\xffhgs"));
        assert!(antigravity_program(hgs).is_none());
    }

    #[test]
    fn antigravity_login_requires_its_exact_native_account_and_current_capability() {
        let machine = "11111111-1111-4111-8111-111111111111";
        let raw = json!({"id":"native-agy","provider":"agy","native":true,"installed":true,
            "native_login_supported":true,"portable":false,"auth_status":"unknown"});
        let catalog = |row: Value| json!({"revision":"r1","setup_revision":true,"profiles":[row]});
        let supported = catalog(raw.clone());
        let host =
            Host::capture(ComputerId("mac-fixture".into()), machine.into(), &supported).unwrap();
        let call = host.antigravity_login(&supported).unwrap();
        assert_eq!(
            call.args,
            [
                "@mac-fixture",
                "--expect-machine",
                machine,
                "account",
                "login",
                "native-agy",
                "--revision",
                "r1"
            ]
        );
        assert!(call.stdin.is_none());
        for (key, value) in [
            ("installed", json!(false)),
            ("native_login_supported", Value::Null),
            ("native", json!(false)),
            ("provider", json!("codex")),
        ] {
            let mut next = raw.clone();
            next[key] = value;
            assert!(host.antigravity_login(&catalog(next)).is_err(), "{key}");
        }
        let mut changed = supported.clone();
        changed["revision"] = json!("r2");
        assert!(host.antigravity_login(&changed).is_err());
        changed = supported;
        changed["profiles"].as_array_mut().unwrap().push(raw);
        assert!(host.antigravity_login(&changed).is_err());
    }
}
