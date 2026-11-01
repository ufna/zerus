//! The existing Zerus Freedesktop wire protocol, independent of its UI or actor.
//!
//! Callers supply their connection and pinned unique daemon name. They retain
//! notification ownership, durable reservations, ID mapping, retries and worker
//! lifetime. This module creates no connection, thread, store or notification
//! consumer. Bodies are already escaped markup, as in the existing UI adapter.
#![cfg(target_os = "linux")]

use std::collections::BTreeMap;
use zbus::blocking::{Connection, Proxy};
use zbus::names::UniqueName;
use zbus::zvariant::Value;

pub const SERVICE: &str = "org.freedesktop.Notifications";
pub const PATH: &str = "/org/freedesktop/Notifications";

/// Issue one call to the selected daemon. A zero ID or an error is returned
/// unchanged: only the caller's original reservation can settle its outcome.
pub fn notify(
    connection: &Connection,
    owner: UniqueName<'_>,
    title: &str,
    body_markup: &str,
) -> zbus::Result<u32> {
    let proxy = Proxy::new(connection, owner, PATH, SERVICE)?;
    let hints = BTreeMap::from([
        ("desktop-entry", Value::from("hgs-tray")),
        ("urgency", Value::from(1_u8)),
        ("category", Value::from("im.received")),
    ]);
    proxy.call(
        "Notify",
        &(
            "Zerus",
            0_u32,
            "hgs-zerus-swarm-symbolic",
            title,
            body_markup,
            vec!["default", "Open session", "open-session", "Open session"],
            hints,
            -1_i32,
        ),
    )
}

/// Close only on the supplied unique daemon. Namespace/currentness checks and
/// the disposition of an uncertain reply belong to the original caller.
pub fn close_notification(
    connection: &Connection,
    owner: UniqueName<'_>,
    id: u32,
) -> zbus::Result<()> {
    Proxy::new(connection, owner, PATH, SERVICE)?.call("CloseNotification", &(id,))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Signal {
    ActivationToken { id: u32, token: String },
    ActionInvoked { id: u32, action: String },
    NotificationClosed { id: u32, reason: u32 },
}

/// Decode only this daemon's protocol signals. Numeric IDs remain scoped to
/// the caller's bus/owner mapping; decoding grants no activation permission.
pub fn decode_signal(message: &zbus::Message, owner: &UniqueName<'_>) -> Option<Signal> {
    let header = message.header();
    if header.message_type() != zbus::message::Type::Signal
        || header.sender().map(|sender| sender.as_str()) != Some(owner.as_str())
        || header.interface().map(|interface| interface.as_str()) != Some(SERVICE)
        || header.path().map(|path| path.as_str()) != Some(PATH)
    {
        return None;
    }
    match header.member()?.as_str() {
        "ActivationToken" => {
            let (id, token) = message.body().deserialize::<(u32, String)>().ok()?;
            Some(Signal::ActivationToken { id, token })
        }
        "ActionInvoked" => {
            let (id, action) = message.body().deserialize::<(u32, String)>().ok()?;
            Some(Signal::ActionInvoked { id, action })
        }
        "NotificationClosed" => {
            let (id, reason) = message.body().deserialize::<(u32, u32)>().ok()?;
            Some(Signal::NotificationClosed { id, reason })
        }
        _ => None,
    }
}
