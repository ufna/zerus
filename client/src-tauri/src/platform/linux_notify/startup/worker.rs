use super::*;
use std::future::Future;
use std::pin::{pin, Pin};
use std::task::{Context, Poll, Wake, Waker};
use zbus::export::futures_core::Stream;

pub(super) enum Exit {
    Clean,
    Unknown,
}
struct Notify(mpsc::SyncSender<()>);
impl Wake for Notify {
    fn wake(self: Arc<Self>) {
        let _ = self.0.try_send(());
    }
    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.0.try_send(());
    }
}
struct Driver<'a> {
    scope: &'a Scope,
    receiver: &'a mpsc::Receiver<()>,
    waker: Waker,
}
impl Driver<'_> {
    fn drive<T>(
        &self,
        executor: Option<&zbus::Executor<'static>>,
        deadline: Instant,
        cancellable: bool,
        work: impl Future<Output = T>,
    ) -> Result<T, ()> {
        let mut work = pin!(work);
        loop {
            if Instant::now() >= deadline
                || (cancellable && self.scope.cancel.load(Ordering::Acquire))
            {
                return Err(());
            }
            let mut context = Context::from_waker(&self.waker);
            if let Poll::Ready(value) = work.as_mut().poll(&mut context) {
                if Instant::now() >= deadline
                    || (cancellable && self.scope.cancel.load(Ordering::Acquire))
                {
                    return Err(());
                }
                return Ok(value);
            }
            if let Some(executor) = executor {
                for _ in 0..16 {
                    let tick = executor.tick();
                    if pin!(tick).poll(&mut context).is_pending() {
                        break;
                    }
                }
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let _ = self
                .receiver
                .recv_timeout(remaining.min(Duration::from_millis(10)));
        }
    }
}
async fn match_call(connection: &zbus::Connection, method: &str, rule: &str) -> Result<(), ()> {
    let reply = connection
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            method,
            &(rule,),
        )
        .await
        .map_err(|_| ())?;
    if reply.header().sender().map(|s| s.as_str()) != Some("org.freedesktop.DBus") {
        return Err(());
    }
    reply.body().deserialize::<()>().map_err(|_| ())
}
fn rules() -> [String; 2] {
    ["type='signal',interface='org.freedesktop.Notifications',path='/org/freedesktop/Notifications'".into(),
     "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.freedesktop.Notifications'".into()]
}
pub(super) fn run(
    address: String,
    scope: Arc<Scope>,
    receiver: mpsc::Receiver<()>,
    setup: Instant,
    hooks: Hooks,
) -> Exit {
    hooks.at(Hook::BeforeNative);
    if Instant::now() >= setup || tokio::runtime::Handle::try_current().is_ok() {
        return Exit::Unknown;
    }
    // The narrow known-no-native-work path still needs the actual thread join.
    if scope.cancel.load(Ordering::Acquire) {
        return Exit::Clean;
    }
    let notify = Arc::new(Notify(scope.wake.clone()));
    let probe = Arc::downgrade(&notify);
    let driver = Driver {
        scope: &scope,
        receiver: &receiver,
        waker: Waker::from(notify),
    };
    let result = session(&address, &driver, setup, &hooks);
    let deadline = scope.cleanup().ok();
    drop(driver);
    // Only test builds carry callback captures; production Hooks is empty.
    #[cfg(test)]
    drop(hooks);
    // Reactor readiness callbacks may briefly retain a waker after its task
    // completed. Its exact last capture must disappear, not just stop waking.
    while probe.strong_count() != 0 {
        if deadline.is_none_or(|d| Instant::now() >= d) {
            return Exit::Unknown;
        }
        let _ = receiver.recv_timeout(Duration::from_millis(1));
    }
    if result.is_ok() && deadline.is_some_and(|d| Instant::now() < d) {
        Exit::Clean
    } else {
        Exit::Unknown
    }
}
fn session(address: &str, driver: &Driver<'_>, setup: Instant, hooks: &Hooks) -> Result<(), ()> {
    let builder = zbus::connection::Builder::address(address)
        .map_err(|_| ())?
        .internal_executor(false)
        .max_queued(RAW);
    let connection = driver
        .drive(None, setup, true, builder.build())?
        .map_err(|_| ())?;
    let executor = connection.executor().clone();
    // A live async-io socket reader is unfinished. Tokio's always-empty
    // executor cannot pass this check; construction was outside Tokio too.
    if connection.is_closed() || executor.is_empty() {
        return Err(());
    }
    #[cfg(test)]
    {
        driver.scope.state.lock().map_err(|_| ())?.peer =
            connection.unique_name().map(|name| name.to_string());
    }
    hooks.at(Hook::Connected);
    let rules = rules();
    let mut setup_ok = true;
    for rule in &rules {
        if driver
            .drive(
                Some(&executor),
                setup,
                true,
                match_call(&connection, "AddMatch", rule),
            )
            .and_then(|r| r)
            .is_err()
        {
            setup_ok = false;
            break;
        }
        hooks.at(Hook::Matched);
    }
    let mut good =
        setup_ok && Instant::now() < setup && !driver.scope.cancel.load(Ordering::Acquire);
    if good {
        let mut readers = [
            zbus::MessageStream::from(&connection),
            zbus::MessageStream::from(&connection),
        ];
        good &= readers.iter().all(|r| r.max_queued() == RAW);
        if good && driver.scope.ready(setup).is_ok() {
            hooks.at(Hook::Raw);
            while !driver.scope.cancel.load(Ordering::Acquire) {
                let mut context = Context::from_waker(&driver.waker);
                for reader in &mut readers {
                    if driver.scope.cancel.load(Ordering::Acquire) {
                        break;
                    }
                    match Pin::new(reader).poll_next(&mut context) {
                        Poll::Ready(Some(Ok(message))) => {
                            // No body decoding, copying, retention or routing.
                            hooks.at(Hook::Message);
                            drop(message);
                        }
                        Poll::Ready(_) => {
                            good = false;
                            let _ = driver.scope.close();
                        }
                        Poll::Pending => (),
                    }
                }
                if driver.scope.cancel.load(Ordering::Acquire) {
                    break;
                }
                let tick = executor.tick();
                let _ = pin!(tick).poll(&mut context);
                let _ = driver.receiver.recv_timeout(Duration::from_millis(10));
            }
        } else {
            good = false;
        }
        hooks.at(Hook::DropReaders);
        drop(readers); // BOTH receivers before ANY RemoveMatch.
    }
    let cleanup = driver.scope.cleanup()?;
    if setup_ok {
        for rule in &rules {
            good &= driver
                .drive(
                    Some(&executor),
                    cleanup,
                    false,
                    match_call(&connection, "RemoveMatch", rule),
                )
                .and_then(|r| r)
                .is_ok();
        }
    }
    hooks.at(Hook::DropConnection);
    good &= driver
        .drive(Some(&executor), cleanup, false, connection.clone().close())
        .and_then(|r| r.map_err(|_| ()))
        .is_ok();
    drop(connection);
    let finished = std::future::poll_fn(|context| {
        if executor.is_empty() {
            return Poll::Ready(());
        }
        let tick = executor.tick();
        let _ = pin!(tick).poll(context);
        if executor.is_empty() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    });
    good &= driver
        .drive(Some(&executor), cleanup, false, finished)
        .is_ok();
    good &= executor.is_empty();
    drop(executor);
    if good {
        Ok(())
    } else {
        Err(())
    }
}
