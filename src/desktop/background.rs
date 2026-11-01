//! Service-owned upstream lifetime. The actor applies every original event.
use super::{feed, fleet, store::Store};
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
}

impl Runtime {
    pub fn start(config: &Config, epoch: &str, broker: &mut feed::Broker) -> Result<Self> {
        let producer = broker.claim_internal()?;
        match fleet::Runtime::start(config, epoch) {
            Ok(fleet) => Ok(Self { fleet, producer }),
            Err(error) => {
                producer.close();
                Err(error)
            }
        }
    }

    /// Each original permit is held through the transaction and observation
    /// point. At most sixteen packets are processed per actor turn.
    pub fn poll(&mut self, store: &mut Store, broker: &mut feed::Broker) -> Vec<Value> {
        let mut changes = Vec::new();
        for _ in 0..POLL_EVENTS {
            let Some(event) = self.fleet.poll() else {
                break;
            };
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
        changes
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
        value["ownership"] = if code == "drained" {
            "revoked"
        } else {
            "background_readonly"
        }
        .into();
        value["upstreams"] = status["computers"].clone();
        value["background_runtime"] = status.clone();
        value["components"]["fleet"] = code.into();
        value["lifecycle"]["background"]["fleet"] = json!({"ready":ready,
            "code":code,"status":status});
    }

    pub fn drain(&mut self, timeout: Duration) -> Result<()> {
        self.producer.close();
        let end = Instant::now() + timeout;
        while !self.fleet.stop() {
            ensure!(
                Instant::now() < end,
                "desktop fleet shutdown is incomplete; owned streams have not drained"
            );
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }
}
