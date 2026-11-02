use super::*;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Wake, Waker};
use zbus::export::futures_core::Stream;
use zbus::{Connection, Executor, MessageStream};
use zerus_freedesktop_notify as native;

const BUS: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";
const RULES: [&str; 2] = [
    "type='signal',interface='org.freedesktop.Notifications',path='/org/freedesktop/Notifications'",
    "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.freedesktop.Notifications'",
];
const RAW: usize = 16;
struct Notify(mpsc::SyncSender<()>);
impl Wake for Notify {
    fn wake(self: Arc<Self>) {
        let _ = self.0.try_send(());
    }
    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.0.try_send(());
    }
}
struct Driver {
    wake: Waker,
    receive: mpsc::Receiver<()>,
    events: std::collections::VecDeque<Signal>,
    owner: Option<String>,
    broken: bool,
}
impl Driver {
    fn new() -> Self {
        let (send, receive) = mpsc::sync_channel(1);
        Self {
            wake: Waker::from(Arc::new(Notify(send))),
            receive,
            events: std::collections::VecDeque::new(),
            owner: None,
            broken: false,
        }
    }
    fn message(&mut self, message: zbus::Message) {
        let header = message.header();
        if header.message_type() == zbus::message::Type::Signal
            && header.sender().map(|s| s.as_str()) == Some(BUS)
            && header.path().map(|s| s.as_str()) == Some(BUS_PATH)
            && header.interface().map(|s| s.as_str()) == Some(BUS)
            && header.member().map(|s| s.as_str()) == Some("NameOwnerChanged")
        {
            match message.body().deserialize::<(String, String, String)>() {
                Ok((name, old, new)) if name == native::SERVICE => {
                    if self
                        .owner
                        .as_ref()
                        .is_some_and(|owner| old == *owner && new != *owner)
                    {
                        self.broken = true;
                    }
                }
                Ok(_) => (),
                Err(_) => self.broken = true,
            }
        }
        let Some(owner) = self
            .owner
            .as_deref()
            .and_then(|s| zbus::names::UniqueName::try_from(s).ok())
        else {
            return;
        };
        if let Some(signal) = native::decode_signal(&message, &owner) {
            if self.events.len() == CALLBACKS {
                self.broken = true;
            } else {
                self.events.push_back(signal);
            }
        }
    }
    fn drive<T>(
        &mut self,
        executor: Option<&Executor<'static>>,
        mut reader: Option<&mut MessageStream>,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        work: impl Future<Output = T>,
    ) -> std::result::Result<T, ()> {
        let mut work = pin!(work);
        loop {
            if cancelled() || Instant::now() >= deadline || self.broken && reader.is_some() {
                return Err(());
            }
            let wake = self.wake.clone();
            let mut context = Context::from_waker(&wake);
            if let Poll::Ready(result) = work.as_mut().poll(&mut context) {
                return if cancelled() || Instant::now() >= deadline {
                    Err(())
                } else {
                    Ok(result)
                };
            }
            if let Some(executor) = executor {
                for _ in 0..16 {
                    if pin!(executor.tick()).poll(&mut context).is_pending() {
                        break;
                    }
                }
            }
            if let Some(reader) = reader.as_mut() {
                for _ in 0..RAW {
                    match pin!(&mut **reader).poll_next(&mut context) {
                        Poll::Ready(Some(Ok(message))) => self.message(message),
                        Poll::Ready(_) => {
                            self.broken = true;
                            return Err(());
                        }
                        Poll::Pending => break,
                    }
                }
            }
            let _ = self.receive.recv_timeout(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(10)),
            );
        }
    }
}
struct Bus {
    connection: Connection,
    executor: Executor<'static>,
    reader: Option<MessageStream>,
    gate: Arc<ingress::Ingress>,
    driver: Driver,
    matched: usize,
}
impl Bus {
    fn connect(
        address: String,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> std::result::Result<Self, ()> {
        let mut driver = Driver::new();
        let (socket, gate) =
            driver.drive(None, None, deadline, &cancelled, ingress::connect(address))??;
        let connection = driver
            .drive(
                None,
                None,
                deadline,
                &cancelled,
                zbus::connection::Builder::socket(socket)
                    .internal_executor(false)
                    .max_queued(RAW)
                    .build(),
            )?
            .map_err(|_| ())?;
        let executor = connection.executor().clone();
        if connection.is_closed() || executor.is_empty() {
            return Err(());
        }
        let reader = Some(MessageStream::from(&connection));
        Ok(Self {
            connection,
            executor,
            reader,
            gate,
            driver,
            matched: 0,
        })
    }
    fn drive<T>(
        &mut self,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        work: impl Future<Output = T>,
    ) -> std::result::Result<T, ()> {
        self.driver.drive(
            Some(&self.executor),
            self.reader.as_mut(),
            deadline,
            cancelled,
            work,
        )
    }
    fn matches(&mut self, add: bool, deadline: Instant, cancelled: impl Fn() -> bool) -> bool {
        let count = if add { RULES.len() } else { self.matched };
        for rule in RULES.iter().take(count) {
            let connection = self.connection.clone();
            let method = if add { "AddMatch" } else { "RemoveMatch" };
            let call = async {
                let reply = connection
                    .call_method(Some(BUS), BUS_PATH, Some(BUS), method, &(rule,))
                    .await
                    .map_err(|_| ())?;
                if reply.header().sender().map(|s| s.as_str()) != Some(BUS) {
                    return Err(());
                }
                reply.body().deserialize::<()>().map_err(|_| ())
            };
            if !matches!(self.drive(deadline, &cancelled, call), Ok(Ok(()))) {
                return false;
            }
            if add {
                self.matched += 1;
            }
        }
        true
    }
    fn namespace(
        &mut self,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> std::result::Result<Namespace, ()> {
        let connection = self.connection.clone();
        let lookup = async {
            let bus = connection
                .call_method(Some(BUS), BUS_PATH, Some(BUS), "GetId", &())
                .await
                .map_err(|_| ())?;
            let owner = connection
                .call_method(
                    Some(BUS),
                    BUS_PATH,
                    Some(BUS),
                    "GetNameOwner",
                    &(native::SERVICE,),
                )
                .await
                .map_err(|_| ())?;
            if [bus.header(), owner.header()]
                .iter()
                .any(|h| h.sender().map(|s| s.as_str()) != Some(BUS))
            {
                return Err(());
            }
            let bus_id: String = bus.body().deserialize().map_err(|_| ())?;
            let owner: String = owner.body().deserialize().map_err(|_| ())?;
            if bus_id.len() != 32
                || !bus_id.bytes().all(|b| b.is_ascii_hexdigit())
                || zbus::names::UniqueName::try_from(owner.as_str()).is_err()
            {
                return Err(());
            }
            Ok(Namespace { bus_id, owner })
        };
        self.drive(deadline, cancelled, lookup)?
    }
    fn close(
        mut self,
        was_good: bool,
        mut mappings: Option<&mut Mappings>,
        endpoint: &Endpoint,
        deadline: Instant,
    ) -> bool {
        let mut good = was_good && self.matched == RULES.len();
        // Keep the original raw reader while RemoveMatch replies form the
        // ordered bus boundary. Drain exact Closed callbacks through that
        // boundary under the current writer; stopping never opens UI clicks.
        let removed = self.matches(false, deadline, || false);
        good &= removed && Instant::now() < deadline;
        if good {
            let wake = self.driver.wake.clone();
            let mut context = Context::from_waker(&wake);
            if let Some(reader) = &mut self.reader {
                for _ in 0..RAW {
                    match pin!(&mut *reader).poll_next(&mut context) {
                        Poll::Ready(Some(Ok(message))) => self.driver.message(message),
                        Poll::Pending => break,
                        Poll::Ready(_) => {
                            good = false;
                            break;
                        }
                    }
                }
            }
            while good && !self.driver.broken {
                let Some(signal) = self.driver.events.pop_front() else {
                    break;
                };
                if !matches!(signal, Signal::NotificationClosed { .. }) {
                    continue;
                }
                let Some(mappings) = mappings.as_deref_mut() else {
                    good = false;
                    break;
                };
                good &=
                    self.namespace(deadline, || false).ok().as_ref() == Some(&mappings.namespace);
                if good {
                    good &= mappings.signal(signal, endpoint);
                }
            }
        }
        // Every raw receiver and buffered event is actually destroyed before
        // arming the private local-close EOF exception. No public setter exists.
        drop(self.reader.take());
        self.driver.events.clear();
        good &= !self.driver.broken && Instant::now() < deadline;
        if good {
            self.gate.removals_acked.store(true, Ordering::Release);
        }
        let connection = self.connection.clone();
        good &= matches!(
            self.drive(deadline, || false, connection.close()),
            Ok(Ok(()))
        );
        let executor = self.executor.clone();
        let finished = std::future::poll_fn(|cx| {
            if executor.is_empty() {
                return Poll::Ready(());
            }
            let _ = pin!(executor.tick()).poll(cx);
            if executor.is_empty() {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        });
        good &= self.drive(deadline, || false, finished).is_ok();
        good && self.executor.is_empty() && !self.gate.refused()
        // This return destroys the SAME connection, executor, captures and
        // waker/channel. Runtime still requires the original thread join.
    }
}

#[cfg(test)]
pub(super) fn test_close_with_queued_callbacks(
    address: String,
    endpoint: Endpoint,
    value: Activation,
    emit: impl FnOnce(),
) -> bool {
    let setup = Instant::now() + WAIT;
    let mut bus = Bus::connect(address, setup, || false).unwrap();
    assert!(bus.matches(true, setup, || false));
    let namespace = bus.namespace(setup, || false).unwrap();
    bus.driver.owner = Some(namespace.owner.clone());
    let mut mappings = Mappings::new(namespace);
    assert!(mappings.restore(vec![value]));
    emit();
    bus.close(true, Some(&mut mappings), &endpoint, Instant::now() + WAIT)
}

pub(super) fn run(
    session: Session,
    endpoint: Endpoint,
    address: String,
    controls: mpsc::Receiver<Control>,
    shared: Arc<Shared>,
    setup: Instant,
) -> bool {
    let cancelled = || shared.stop.load(Ordering::Acquire) || endpoint.stopping();
    let Ok(mut bus) = Bus::connect(address, setup, cancelled) else {
        shared.failed.store(true, Ordering::Release);
        return false;
    };
    let mut good = bus.matches(true, setup, cancelled);
    let namespace = good
        .then(|| bus.namespace(setup, cancelled))
        .and_then(std::result::Result::ok);
    let Some(namespace) = namespace else {
        shared.failed.store(true, Ordering::Release);
        return bus.close(false, None, &endpoint, shared.cleanup_deadline());
    };
    bus.driver.owner = Some(namespace.owner.clone());
    let mut mappings = Mappings::new(namespace);
    let mut restoring = true;
    while !cancelled() && good {
        match controls.try_recv() {
            Ok(Control::Restore(values, reply)) => {
                let current = bus.namespace(Instant::now() + WAIT, cancelled).ok();
                let accepted = restoring
                    && current.as_ref() == Some(&mappings.namespace)
                    && mappings.restore(values);
                let _ = reply.try_send(accepted);
                good &= accepted;
            }
            Ok(Control::Finish(reply)) => {
                let current = bus.namespace(Instant::now() + WAIT, cancelled).ok();
                let accepted = restoring && current.as_ref() == Some(&mappings.namespace);
                if accepted {
                    restoring = false;
                    shared.ready.store(true, Ordering::Release);
                }
                let _ = reply.try_send(accepted);
                good &= accepted;
            }
            Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => (),
        }
        if let Some(signal) = bus.driver.events.pop_front() {
            // A signal's bus generation AND unique daemon owner are checked
            // freshly. Neither a numeric ID nor a restored token is authority.
            good &= bus
                .namespace(Instant::now() + WAIT, cancelled)
                .ok()
                .as_ref()
                == Some(&mappings.namespace);
            if good {
                good &= mappings.signal(signal, &endpoint);
            }
            continue;
        }
        if !restoring {
            match endpoint.try_next() {
                Ok(work) => {
                    let Some(dispatch) = work.begin() else {
                        continue;
                    };
                    let resources = dispatch.resources();
                    let command = dispatch.command().clone();
                    let until = dispatch.deadline().min(Instant::now() + WAIT);
                    let stop = || cancelled() || dispatch.cancelled();
                    let current = command.valid()
                        && command.binding.session == session
                        && bus.namespace(until, stop).ok().as_ref() == Some(&mappings.namespace);
                    let outcome = if !current {
                        Outcome::Refused {
                            code: Refusal::NotReady,
                        }
                    } else {
                        let connection = bus.connection.clone();
                        let owner =
                            zbus::names::UniqueName::try_from(mappings.namespace.owner.as_str())
                                .unwrap();
                        match &command.action {
                            Action::Post { title, body, .. } => {
                                if mappings.rows.len() == wire::MAPPINGS {
                                    Outcome::Refused {
                                        code: Refusal::Capacity,
                                    }
                                } else {
                                    let escaped =
                                        zerus_client_core::linux_notify::escape_body(body);
                                    match bus.drive(
                                        until,
                                        stop,
                                        native::notify_async(&connection, owner, title, &escaped),
                                    ) {
                                        Ok(Ok(reply))
                                            if reply_from(&reply, &mappings.namespace.owner) =>
                                        {
                                            match reply.body().deserialize::<u32>() {
                                                Ok(id) => {
                                                    let collision = mappings.rows.contains_key(&id);
                                                    let outcome = mappings.adopt(&command, id);
                                                    good &= !collision;
                                                    outcome
                                                }
                                                Err(_) => Outcome::Unknown {
                                                    code: Uncertainty::NativeError,
                                                },
                                            }
                                        }
                                        Ok(Ok(_)) => Outcome::Unknown {
                                            code: Uncertainty::Transport,
                                        },
                                        Ok(Err(_)) => Outcome::Unknown {
                                            code: Uncertainty::NativeError,
                                        },
                                        Err(()) => Outcome::Unknown {
                                            code: if Instant::now() >= until {
                                                Uncertainty::Deadline
                                            } else {
                                                Uncertainty::Transport
                                            },
                                        },
                                    }
                                }
                            }
                            Action::Withdraw { token, native_id } => {
                                if !mappings.owns(token, native_id) {
                                    Outcome::Refused {
                                        code: Refusal::NotReady,
                                    }
                                } else {
                                    let id = mappings.id(native_id).unwrap();
                                    // The actor already tombstoned this original withdrawal.
                                    mappings.rows.remove(&id);
                                    mappings.tokens.remove(&id);
                                    match bus.drive(
                                        until,
                                        stop,
                                        native::close_notification_async(&connection, owner, id),
                                    ) {
                                        Ok(Ok(reply))
                                            if reply_from(&reply, &mappings.namespace.owner)
                                                && reply.body().deserialize::<()>().is_ok() =>
                                        {
                                            Outcome::Delivered {
                                                native_id: native_id.clone(),
                                            }
                                        }
                                        Ok(Ok(_)) => Outcome::Unknown {
                                            code: Uncertainty::Transport,
                                        },
                                        Ok(Err(_)) => Outcome::Unknown {
                                            code: Uncertainty::NativeError,
                                        },
                                        Err(()) => Outcome::Unknown {
                                            code: if Instant::now() >= until {
                                                Uncertainty::Deadline
                                            } else {
                                                Uncertainty::Transport
                                            },
                                        },
                                    }
                                }
                            }
                        }
                    };
                    // Route callbacks already captured by this original call
                    // while its input/output/callback charge is still retained.
                    // The actor can defer an early event until this exact post
                    // commits; it cannot be turned into a new post or binding.
                    while good {
                        let Some(signal) = bus.driver.events.pop_front() else {
                            break;
                        };
                        good &=
                            bus.namespace(until, stop).ok().as_ref() == Some(&mappings.namespace);
                        if good {
                            good &= mappings.signal(signal, &endpoint);
                        }
                    }
                    // The original native future and its captures have returned
                    // or been destroyed before the per-command charge is freed.
                    drop(resources);
                    dispatch.finish(outcome);
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => break,
                Err(mpsc::TryRecvError::Empty) => (),
            }
        }
        let _ = bus.drive(
            Instant::now() + Duration::from_millis(10),
            cancelled,
            std::future::pending::<()>(),
        );
        good &= !bus.driver.broken && !bus.gate.refused();
    }
    shared.ready.store(false, Ordering::Release);
    let clean = bus.close(
        good,
        Some(&mut mappings),
        &endpoint,
        shared.cleanup_deadline(),
    );
    drop(mappings);
    shared.failed.store(!clean, Ordering::Release);
    clean
}

pub(super) fn reply_from(message: &zbus::Message, owner: &str) -> bool {
    let header = message.header();
    header.message_type() == zbus::message::Type::MethodReturn
        && header.sender().map(|sender| sender.as_str()) == Some(owner)
}
