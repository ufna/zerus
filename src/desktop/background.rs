//! Service-owned upstream lifetime. The actor applies every original event.
use super::{
    feed, fleet, presence_worker,
    store::{Location, Store},
};
use crate::config::Config;
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

const POLL_EVENTS: usize = 16;

pub(super) struct Runtime {
    fleet: fleet::Runtime,
    producer: Arc<feed::Producer>,
    presence: Option<presence_worker::Runtime>,
}

impl Runtime {
    pub fn start(config: &Config, epoch: &str, broker: &mut feed::Broker) -> Result<Self> {
        let producer = broker.claim_internal()?;
        match fleet::Runtime::start(config, epoch) {
            Ok(fleet) => Ok(Self {
                fleet,
                producer,
                presence: None,
            }),
            Err(error) => {
                producer.close();
                Err(error)
            }
        }
    }

    pub fn start_presence(&mut self, location: &Location) -> Result<()> {
        self.presence = Some(presence_worker::Runtime::start(location)?);
        Ok(())
    }

    /// Each original permit is held through the transaction and observation
    /// point. At most sixteen packets are processed per actor turn.
    pub fn poll(&mut self, store: &mut Store, broker: &mut feed::Broker) -> (Vec<Value>, bool) {
        let mut changes = Vec::new();
        let mut consumed = 0;
        for _ in 0..POLL_EVENTS {
            let Some(event) = self.fleet.poll() else {
                break;
            };
            consumed += 1;
            match broker.apply(store, &self.producer, &event.packet) {
                Ok(change) => {
                    // .29 delivery.observe(store, &event): after commit and
                    // before dropping this service-owned event/permit.
                    if let Some(change) = change {
                        changes.push(change);
                    }
                }
                Err(_) => {
                    broker.invalidate(&self.producer, &event.packet);
                    self.fleet.reject(&event);
                }
            }
        }
        (changes, consumed == POLL_EVENTS)
    }

    pub fn decorate(&self, value: &mut Value) {
        let status = self.fleet.status();
        let ready = status["transport_ready"] == true;
        let code = match status["state"].as_str() {
            Some("drained") => "drained",
            Some("stopping") => "draining",
            _ if ready => "running",
            _ => "starting_or_degraded",
        };
        let presence = self.presence.as_ref().map(presence_worker::Runtime::status);
        value["ownership"] =
            if code == "drained" && presence.as_ref().is_none_or(|p| p["drained"] == true) {
                "revoked"
            } else if presence.is_some() {
                "background_service"
            } else {
                "background_readonly"
            }
            .into();
        value["upstreams"] = status["computers"].clone();
        value["background_runtime"] = status.clone();
        value["components"]["fleet"] = code.into();
        value["lifecycle"]["background"]["fleet"] = json!({"ready":ready,
            "code":code,"status":status});
        if let Some(presence) = presence {
            value["writers_enabled"] = presence["writer_active"].clone();
            value["components"]["presence"] = presence["code"].clone();
            value["lifecycle"]["background"]["presence"] = presence;
        }
    }

    pub fn drain(&mut self, timeout: Duration) -> Result<()> {
        self.producer.close();
        if let Some(presence) = &self.presence {
            presence.request_stop();
        }
        let end = Instant::now() + timeout;
        let mut failure = None;
        loop {
            let fleet_done = self.fleet.stop();
            let presence_done = match self.presence.as_mut().map(presence_worker::Runtime::stop) {
                None | Some(Ok(true)) => true,
                Some(Ok(false)) => false,
                Some(Err(error)) => {
                    failure = Some(error);
                    true // join failed, but still finish draining the fleet
                }
            };
            if fleet_done && presence_done {
                return failure.map_or(Ok(()), Err);
            }
            ensure!(
                Instant::now() < end,
                "desktop background shutdown is incomplete; owned workers have not drained"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}
