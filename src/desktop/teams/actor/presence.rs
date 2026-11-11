//! Passive read of the existing service status exchange, with its own bound.
use crate::config::Config;
pub(crate) fn presence(config: &Config) -> Option<bool> {
    let location = crate::desktop::store::Location::new(config).ok()?;
    match crate::desktop::exchange(&location, "status", None).ok()? {
        None => Some(false),
        Some(value) => value["running"].as_bool(),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::desktop::{store::Location, transport::Listener};
    use crate::teams::ops::tests::Harness;
    use std::{
        io::{BufRead, BufReader, Write},
        time::{Duration, Instant},
    };

    fn config(h: &Harness) -> Config {
        crate::platform::private_dir(&h.temp.path().join("state")).unwrap();
        crate::platform::private_dir(&h.temp.path().join("config")).unwrap();
        Config {
            home: String::new(),
            dir: h.temp.path().join("config"),
            state: h.temp.path().join("state"),
            selves: vec![],
            peers: vec![],
            base_peers: vec![],
            peers_from_environment: false,
            machines: Default::default(),
            tab: false,
            tab_colors: String::new(),
        }
    }

    #[test]
    fn a_missing_socket_is_absent_and_the_read_creates_nothing() {
        let h = Harness::new();
        let config = config(&h);
        let desktop = config.state.join("desktop");
        assert_eq!(presence(&config), Some(false));
        assert!(!desktop.exists());
    }

    fn answering(bytes: Vec<u8>, wait: Duration) -> (Option<bool>, Duration) {
        let h = Harness::new();
        let config = config(&h);
        let location = Location::new(&config).unwrap();
        location.prepare().unwrap();
        let listener = Listener::bind(&location).unwrap();
        let server = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(2);
            let mut stream = loop {
                match listener.accept() {
                    Ok(stream) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < until =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    other => panic!("fixture accept: {other:?}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            let request: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["op"], "status");
            assert!(request["request"].is_null());
            std::thread::sleep(wait);
            let _ = stream.write_all(&bytes);
        });
        let at = Instant::now();
        let result = presence(&config);
        let elapsed = at.elapsed();
        server.join().unwrap();
        assert!(!location.root.join("service.sock").exists());
        (result, elapsed)
    }

    #[test]
    fn a_service_reply_reports_presence_and_an_unreadable_exchange_is_unknown() {
        let mut good = serde_json::to_vec(
            &serde_json::json!({"abi":crate::desktop::ABI,"t":"status","value":{"running":true}}),
        )
        .unwrap();
        good.push(b'\n');
        assert_eq!(answering(good, Duration::ZERO).0, Some(true));
        assert_eq!(answering(b"unreadable\n".to_vec(), Duration::ZERO).0, None);
    }

    #[test]
    fn a_slow_service_stays_unknown_with_the_existing_exchange_bound() {
        let (value, elapsed) = answering(vec![], Duration::from_millis(2200));
        assert_eq!(value, None);
        assert!(elapsed < Duration::from_secs(3), "{elapsed:?}");
    }
}
