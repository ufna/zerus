//! Invoked by the root Teams contract while its product-written off/on
//! fixtures are alive. Calls the command's production follow/pump path without
//! constructing an application or reaching a running desktop service.
use super::*;
use std::time::{Duration, Instant};

fn list(pump: &crate::pump::Pump) -> zerus_client_core::Batch {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let batch = pump.take(Duration::from_millis(50));
        if batch.teams.iter().any(|entry| entry.key == "list") {
            return batch;
        }
        assert!(
            Instant::now() < until,
            "a list answer reaches next_batch's pump"
        );
    }
}

#[test]
#[ignore = "run from teams::preview_client_tests with its product-written private fixtures"]
fn the_real_cli_off_and_on_reach_the_tauri_pump() {
    for on in [false, true] {
        let name = if on {
            "HGS_TEST_TEAMS_ON"
        } else {
            "HGS_TEST_TEAMS_OFF"
        };
        let hgs =
            std::path::PathBuf::from(std::env::var_os(name).expect("private real CLI fixture"));
        let teams = Teams::new();
        let pump = crate::pump::Pump::default();
        let follow = TeamsFollow {
            list: true,
            ..TeamsFollow::default()
        };
        teams
            .follow(hgs.clone(), pump.clone(), follow.clone(), false)
            .unwrap();
        let first = list(&pump);
        let entry = &first.teams[0];
        assert_eq!(entry.kind, if on { "list" } else { "failed" });
        if !on {
            assert_eq!(entry.payload["code"], "teams_inactive");
        }
        println!(
            "TAURI_REAL_PREVIEW_BATCH={}",
            serde_json::to_string(&first).unwrap()
        );
        teams.follow(hgs, pump.clone(), follow, true).unwrap();
        assert_eq!(
            serde_json::to_value(first).unwrap(),
            serde_json::to_value(list(&pump)).unwrap()
        );
    }
}
