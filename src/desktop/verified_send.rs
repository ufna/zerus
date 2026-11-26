//! The original local native Desktop Send helper. A catalog row, command
//! argument or service-owned send cannot construct this frontend permit.
use super::{store::Location, ui_authority, Config};
use anyhow::{ensure, Result};
use std::io::Read;
use zerus_client_core::desktop::verified_send::{Intent, ENVELOPE_BYTES, VERSION};

pub(crate) struct Permit(ui_authority::Validated);

impl Permit {
    pub(crate) fn intent(&self) -> std::result::Result<&Intent, String> {
        self.0.verified_send().map_err(|err| err.to_string())
    }
}

pub(crate) fn dispatch() -> Result<i32> {
    let mut bytes = Vec::new();
    std::io::stdin().take((ENVELOPE_BYTES + 1) as u64).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= ENVELOPE_BYTES, "native verified send request exceeds its bound");
    let config = Config::load()?;
    let location = Location::new(&config)?;
    let scope = ui_authority::Scope::new(&config, &location);
    let permit = Permit(ui_authority::authenticate_verified_send(&scope, serde_json::from_slice(&bytes)?)?);
    let intent = permit.0.verified_send()?;
    let status = super::status(&location)?;
    ensure!(
        status["running"] == true && status["native_readiness"]["version"] == VERSION
            && status["native_readiness"]["send"] == true
            && status["epoch"] == intent.send.scope.service_epoch,
        "verified send service source changed"
    );
    let reply = crate::state::send_desktop_verified(&permit).map_err(anyhow::Error::msg)?;
    println!("{}", serde_json::to_string(&reply)?);
    Ok(0)
}
