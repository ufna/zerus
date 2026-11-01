//! Bounded desired subscriptions on the one existing service watch lease.
//! This worker sends read controls only; it never opens a host watch or polls.
use super::*;
use zerus_client_core::{
    desktop::service::details::{self as protocol, Key, Lease, Route, Version},
    CatalogState, OrganizationSnapshot,
};
use zerus_watch_protocol::{topic, Body, Subscription};

#[cfg(all(test, unix))]
mod tests;

#[derive(Default)]
pub(super) struct Hub {
    lease: Option<Lease>,
    routes: BTreeMap<Key, Entry>,
    retiring: BTreeSet<String>,
    sources: BTreeMap<String, Source>,
    next_source: u64,
}
struct Entry {
    route: Route,
    pending: bool,
    sent: bool,
    refresh: bool,
}
struct Source {
    generation: u64,
    serial: u64,
    state: CatalogState,
    pending: BTreeSet<String>,
    seen: BTreeSet<String>,
}
impl Hub {
    fn follow(&mut self, computer: &str, spec: Subscription) -> Result<(), String> {
        let key = Key::new(computer, &spec).map_err(str::to_owned)?;
        if self
            .routes
            .get(&key)
            .is_some_and(|old| old.route.subscription == spec)
        {
            return Ok(());
        }
        if self.routes.len() + self.retiring.len() >= protocol::DEMANDS {
            return Err("The desktop service detail subscription limit was reached.".into());
        }
        self.unfollow(&key);
        let route =
            Route::new(computer, uuid::Uuid::new_v4().to_string(), spec).map_err(str::to_owned)?;
        self.routes.insert(
            key,
            Entry {
                route,
                pending: true,
                sent: false,
                refresh: false,
            },
        );
        Ok(())
    }
    fn unfollow(&mut self, key: &Key) {
        if let Some(old) = self
            .routes
            .remove(key)
            .filter(|old| old.sent && self.lease.is_some())
        {
            self.retiring.insert(old.route.id);
        }
    }
    pub(super) fn disconnect(&mut self) -> Vec<Change> {
        self.lease = None;
        self.retiring.clear();
        self.sources.clear();
        let mut changes = Vec::new();
        for entry in self.routes.values_mut() {
            // Old queued deliveries cannot acquire the next lease's identity.
            entry.route = Route::new(
                &entry.route.key.computer,
                uuid::Uuid::new_v4().to_string(),
                entry.route.subscription.clone(),
            )
            .unwrap();
            entry.pending = true;
            entry.sent = false;
            entry.refresh = false;
            changes.push(entry.route.connection(false, "service_unavailable"));
        }
        changes
    }
    pub(super) fn source(&mut self, computer: &str, generation: u64) -> Result<(), &'static str> {
        if !service::valid_computer(computer) || generation == 0 {
            return Ok(());
        }
        if self
            .sources
            .get(computer)
            .is_some_and(|old| old.generation >= generation)
        {
            return Ok(());
        }
        if !self.sources.contains_key(computer) && self.sources.len() >= SOURCES {
            return Err("detail_source_capacity");
        }
        self.next_source = self
            .next_source
            .checked_add(1)
            .ok_or("detail_source_exhausted")?;
        self.sources.insert(
            computer.into(),
            Source {
                generation,
                serial: self.next_source,
                state: CatalogState::new(),
                pending: BTreeSet::from(["hello".into(), "meta".into()]),
                seen: BTreeSet::new(),
            },
        );
        Ok(())
    }
    fn next(&mut self) -> Option<(Lease, Work)> {
        let lease = self.lease.clone()?;
        if let Some(id) = self.retiring.pop_first() {
            return Some((lease, Work::Unfollow(id)));
        }
        if let Some(entry) = self.routes.values_mut().find(|entry| entry.pending) {
            entry.pending = false;
            entry.sent = true;
            return Some((
                lease,
                Work::Follow {
                    key: entry.route.key.clone(),
                    id: entry.route.id.clone(),
                    spec: Box::new(entry.route.subscription.clone()),
                    refresh: std::mem::take(&mut entry.refresh),
                },
            ));
        }
        for (computer, source) in &mut self.sources {
            if let Some(part) = source.pending.pop_first() {
                return Some((
                    lease,
                    Work::Source {
                        computer: computer.clone(),
                        generation: source.generation,
                        serial: source.serial,
                        part,
                    },
                ));
            }
        }
        None
    }
}
enum Work {
    Unfollow(String),
    Follow {
        key: Key,
        id: String,
        spec: Box<Subscription>,
        refresh: bool,
    },
    Source {
        computer: String,
        generation: u64,
        serial: u64,
        part: String,
    },
}

pub struct Delivery {
    shared: Weak<Shared>,
    stopped: Arc<AtomicBool>,
    lease: Lease,
    gate: Target,
    changes: Vec<Change>,
}
enum Target {
    Route {
        key: Key,
        id: String,
        version: Option<Version>,
        stamp: u64,
    },
    Source {
        computer: String,
        generation: u64,
        serial: u64,
    },
}
impl Delivery {
    pub fn apply(self, pump: &crate::pump::Pump, observe: &impl Fn(&Change)) {
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        let hub = shared
            .details
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.stopped.load(Ordering::Acquire)
            || shared.stopped.load(Ordering::Acquire)
            || hub.lease.as_ref() != Some(&self.lease)
        {
            return;
        }
        let valid = match &self.gate {
            Target::Route {
                key,
                id,
                version,
                stamp,
            } => hub.routes.get(key).is_some_and(|entry| {
                entry.route.id == *id
                    && entry.route.version() == *version
                    && entry.route.stamp() == *stamp
            }),
            Target::Source {
                computer,
                generation,
                serial,
            } => hub
                .sources
                .get(computer)
                .is_some_and(|source| source.generation == *generation && source.serial == *serial),
        };
        if valid {
            if let Target::Route { key, .. } = &self.gate {
                if key.computer.is_empty() && key.topic == topic::ORGANIZATION {
                    *shared
                        .organization
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = hub
                        .routes
                        .get(key)
                        .and_then(|entry| entry.route.organization_snapshot());
                }
            }
            for change in self.changes {
                observe(&change);
                pump.push(change);
            }
        }
    }
}
async fn deliver(
    shared: &Arc<Shared>,
    stop: &Arc<AtomicBool>,
    lease: Lease,
    gate: Target,
    changes: Vec<Change>,
) -> Result<(), &'static str> {
    if !changes.is_empty() {
        emit_data(
            shared,
            Data::Detail(Delivery {
                shared: Arc::downgrade(shared),
                stopped: stop.clone(),
                lease,
                gate,
                changes,
            }),
            stop,
        )
        .await?;
    }
    Ok(())
}

impl Service {
    pub fn follow_detail(&self, computer: &str, spec: Subscription) -> Result<(), String> {
        if self.shared.stopped.load(Ordering::Acquire) {
            return Err("Background service is unavailable.".into());
        }
        let change = {
            let mut hub = self
                .shared
                .details
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let key = Key::new(computer, &spec).map_err(str::to_owned)?;
            hub.follow(computer, spec)?;
            let entry = &hub.routes[&key];
            (!entry.sent).then(|| {
                if hub.lease.is_some() {
                    entry.route.waiting()
                } else {
                    entry.route.connection(false, "details_unavailable")
                }
            })
        };
        if let Some(change) = change {
            self.shared.pump.push(change);
        }
        self.shared.details_changed.notify_one();
        Ok(())
    }
    pub fn unfollow_detail(&self, computer: &str, topic: &str, key: &str) {
        self.shared
            .details
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .unfollow(&Key {
                computer: computer.into(),
                topic: topic.into(),
                key: key.into(),
            });
        self.shared.details_changed.notify_one();
    }
    pub fn processes_update(
        &self,
        computer: &str,
        key: &str,
        interval: Option<u64>,
    ) -> Result<(), String> {
        let mut hub = self
            .shared
            .details
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let target = Key {
            computer: computer.into(),
            topic: topic::PROCESSES.into(),
            key: key.into(),
        };
        let entry = hub
            .routes
            .get_mut(&target)
            .ok_or("The processes subscription is unavailable.")?;
        if let Some(ms) = interval {
            if !(1000..=600_000).contains(&ms) {
                return Err("Invalid processes interval.".into());
            }
            let mut next = entry.route.subscription.clone();
            next.interval_ms = Some(ms);
            hub.follow(computer, next)?;
        } else {
            entry.refresh = true;
            entry.pending = true;
        }
        drop(hub);
        self.shared.details_changed.notify_one();
        Ok(())
    }
    pub fn organization_snapshot(&self) -> Option<OrganizationSnapshot> {
        self.shared.organization.lock().ok()?.clone()
    }
}

pub(super) async fn frame(
    shared: &Arc<Shared>,
    frame: &Value,
    stop: &Arc<AtomicBool>,
) -> Result<(), &'static str> {
    let kind = frame["t"].as_str().ok_or("invalid_service_frame")?;
    if kind == "hello" {
        let mut hub = shared
            .details
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        hub.lease = frame["epoch"]
            .as_str()
            .and_then(|epoch| Lease::parse(&frame["value"]["details"], epoch));
        shared.details_changed.notify_one();
        return Ok(());
    }
    if kind == "catalog_changed" {
        let value = &frame["value"];
        let computer = value["computer"]
            .as_str()
            .filter(|s| service::valid_computer(s))
            .ok_or("invalid_detail_source")?;
        let mut hub = shared
            .details
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if value["removed"] == true {
            hub.sources.remove(computer);
        } else {
            hub.source(computer, value["generation"].as_u64().unwrap_or(0))?;
        }
        shared.details_changed.notify_one();
        return Ok(());
    }
    if !matches!(kind, "detail" | "detail_status" | "source") {
        return Ok(());
    }
    let value = &frame["value"];
    let delivery = {
        let mut hub = shared
            .details
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(lease) = hub
            .lease
            .clone()
            .filter(|lease| frame["epoch"] == lease.epoch && value["lease"] == lease.lease)
        else {
            return Ok(());
        };
        if kind == "source" {
            let computer = value["computer"]
                .as_str()
                .filter(|c| service::valid_computer(c))
                .ok_or("invalid_detail_source")?;
            let generation = value["generation"]
                .as_u64()
                .filter(|n| *n > 0)
                .ok_or("invalid_detail_source")?;
            let native = protocol::native_frame(&value["frame"])?;
            let part = match &native.body {
                Body::Hello(_) => "hello",
                Body::Snapshot(s) if s.topic == topic::CATALOG_META && s.last => "meta",
                _ => return Err("invalid_detail_source"),
            };
            if let Body::Hello(hello) = &native.body {
                if hub.sources.get(computer).is_some_and(|s| {
                    s.generation == generation
                        && s.state
                            .hello()
                            .is_some_and(|old| old.stream != hello.stream)
                }) {
                    hub.sources.remove(computer);
                }
            }
            hub.source(computer, generation)?;
            let source = hub
                .sources
                .get_mut(computer)
                .ok_or("invalid_detail_source")?;
            if source.generation != generation {
                return Ok(());
            }
            // Physical identity comes from the original Hello. Bootstrap it
            // first if a late subscriber saw meta before the source greeting.
            if part == "meta" && source.state.hello().is_none() {
                return Ok(());
            }
            source.pending.remove(part);
            source.seen.insert(part.into());
            let changes = source.state.apply(&ComputerId(computer.into()), &native);
            Some((
                lease,
                Target::Source {
                    computer: computer.into(),
                    generation,
                    serial: source.serial,
                },
                changes,
            ))
        } else {
            let Some(entry) = hub
                .routes
                .values_mut()
                .find(|entry| value["id"] == entry.route.id)
            else {
                return Ok(());
            };
            let changes = entry.route.apply(kind, value)?;
            let generation = entry.route.version().map_or(0, |v| v.generation);
            let computer = entry.route.key.computer.clone();
            let gate = Target::Route {
                key: entry.route.key.clone(),
                id: entry.route.id.clone(),
                version: entry.route.version(),
                stamp: entry.route.stamp(),
            };
            hub.source(&computer, generation)?;
            shared.details_changed.notify_one();
            Some((lease, gate, changes))
        }
    };
    if let Some((lease, gate, changes)) = delivery {
        deliver(shared, stop, lease, gate, changes).await?;
    }
    Ok(())
}

pub(super) async fn run(shared: Arc<Shared>, stop: Arc<AtomicBool>) -> Result<(), &'static str> {
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        let work = shared
            .details
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .next();
        let Some((lease, work)) = work else {
            shared.details_changed.notified().await;
            continue;
        };
        let id = uuid::Uuid::new_v4().to_string();
        let mut value = json!({"request_id":id,"epoch":lease.epoch,"lease":lease.lease});
        match &work {
            Work::Unfollow(route) => {
                value["op"] = json!("detail_unfollow");
                value["id"] = json!(route);
            }
            Work::Follow {
                key,
                id,
                spec,
                refresh,
            } => {
                value["op"] = json!("detail_follow");
                value["id"] = json!(id);
                value["computer"] = json!(key.computer);
                value["subscription"] =
                    serde_json::to_value(spec).map_err(|_| "invalid_detail_subscription")?;
                if *refresh {
                    value["subscription"]["refresh"] = json!(true);
                }
            }
            Work::Source {
                computer,
                generation,
                part,
                ..
            } => {
                value["op"] = json!("detail_source");
                value["computer"] = json!(computer);
                value["generation"] = json!(generation);
                value["part"] = json!(part);
            }
        }
        let response = request(&shared, value, stop.clone()).await?;
        if response["request_id"] != id {
            return Err("invalid_detail_response");
        }
        if response["accepted"] != true {
            // Refused metadata does not revoke unrelated native detail routes.
            if matches!(work, Work::Source { .. }) {
                continue;
            }
            if let Work::Follow { key, id, .. } = &work {
                let delivery = {
                    let hub = shared
                        .details
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    hub.routes
                        .get(key)
                        .filter(|entry| entry.route.id == *id)
                        .map(|entry| {
                            let code = response["code"]
                                .as_str()
                                .filter(|s| s.len() <= 128 && !s.chars().any(char::is_control))
                                .unwrap_or("detail_unavailable");
                            (
                                Target::Route {
                                    key: key.clone(),
                                    id: id.clone(),
                                    version: entry.route.version(),
                                    stamp: entry.route.stamp(),
                                },
                                vec![entry.route.connection(false, code)],
                            )
                        })
                };
                if let Some((gate, changes)) = delivery {
                    deliver(&shared, &stop, lease, gate, changes).await?;
                }
                continue;
            }
            // Failed retirement closes this lease so abandoned demand cannot
            // consume an unbounded succession of server-side slots.
            return Err("detail_retirement_unconfirmed");
        }
        if response["lease"] != lease.lease || response["durable"] != false {
            return Err("invalid_detail_response");
        }
        match work {
            Work::Source {
                computer,
                generation,
                serial,
                part,
            } => {
                if response["computer"] != computer
                    || response["generation"] != generation
                    || response["part"] != part
                {
                    return Err("invalid_detail_response");
                }
                if response["native"] != true
                    || response["online"] != true
                    || response["value"].is_null()
                {
                    continue;
                }
                let changes = {
                    let mut hub = shared
                        .details
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    if hub.lease.as_ref() != Some(&lease) {
                        continue;
                    }
                    let Some(source) = hub.sources.get_mut(&computer).filter(|s| {
                        s.generation == generation && s.serial == serial && !s.seen.contains(&part)
                    }) else {
                        continue;
                    };
                    if part == "meta" && source.state.hello().is_none() {
                        continue;
                    }
                    source.seen.insert(part.clone());
                    source.state.source_metadata(
                        &ComputerId(computer.clone()),
                        &part,
                        response["value"].clone(),
                    )?
                };
                deliver(
                    &shared,
                    &stop,
                    lease,
                    Target::Source {
                        computer,
                        generation,
                        serial,
                    },
                    changes,
                )
                .await?;
            }
            Work::Follow { id, .. } | Work::Unfollow(id) => {
                if response["id"] != id {
                    return Err("invalid_detail_response");
                }
                // Watch status is authoritative for this route. A delayed
                // follow response must never downgrade a newer ready status.
            }
        }
    }
}
