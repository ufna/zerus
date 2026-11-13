//! Run only with a source-built HGS_TEST_BIN, in an explicitly chosen process
//! placement. Calls the real Tauri command, including its native runner and
//! reply decoder; no application, resident host or user state is opened.
use super::teams_preview;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;

fn process_placement() -> serde_json::Value {
    let mut pid = std::process::id();
    let owner = unsafe { libc::geteuid() };
    let mut rows = Vec::new();
    for _ in 0..64 {
        let root = format!("/proc/{pid}");
        let status = std::fs::read_to_string(format!("{root}/status")).unwrap();
        let uid = status
            .lines()
            .find_map(|line| line.strip_prefix("Uid:"))
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u32>()
            .unwrap();
        let stat = std::fs::read_to_string(format!("{root}/stat")).unwrap();
        let (name, tail) = stat.rsplit_once(')').unwrap();
        let parent = tail
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u32>()
            .unwrap();
        let readable = uid == owner && std::fs::File::open(format!("{root}/environ")).is_ok();
        rows.push(
            json!({"pid":pid,"parent":parent,"name":name.split_once('(').unwrap().1,
            "same_user":uid==owner,"environment_readable":readable}),
        );
        if uid != owner || parent == 0 || parent == pid {
            break;
        }
        pid = parent;
    }
    json!(rows)
}

#[test]
#[ignore = "requires a private source-built CLI and an explicit systemd/agent process placement"]
fn the_real_preview_command_records_its_caller_result() {
    for marker in ["HGS_SESSION", "HGS_RUN_ID", "HGS_SESSION_ID"] {
        assert!(
            std::env::var_os(marker).is_none(),
            "the GUI child has no session markers"
        );
    }
    let binary =
        std::path::PathBuf::from(std::env::var_os("HGS_TEST_BIN").expect("source-built CLI"));
    assert!(binary.is_absolute() && binary.is_file());
    let mode = std::env::var("ZERUS_TEST_PREVIEW_CALLER").expect("person or agent placement");
    assert!(matches!(mode.as_str(), "person" | "agent"));
    let home = tempfile::tempdir().unwrap();
    for dir in ["config", "state", "tmux"] {
        std::fs::create_dir(home.path().join(dir)).unwrap();
        std::fs::set_permissions(
            home.path().join(dir),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
    }
    let quote =
        |path: &std::path::Path| format!("'{}'", path.display().to_string().replace('\'', "'\\''"));
    let private = home.path().join("hgs-private");
    let script = format!(
        "#!/bin/sh\nexec env -i PATH=\"$PATH\" HOME={} HGS_CONFIG_DIR={} HGS_STATE_DIR={} TMUX_TMPDIR={} HGS_SELF=example-local HGS_PEERS='' {} \"$@\"\n",
        quote(home.path()), quote(&home.path().join("config")), quote(&home.path().join("state")),
        quote(&home.path().join("tmux")), quote(&binary));
    std::fs::write(&private, script).unwrap();
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::env::set_var("ZERUS_CLIENT_HGS", &private);
    println!("PREVIEW_ANCESTRY={}", process_placement());
    let result = tauri::async_runtime::block_on(teams_preview(true));
    println!(
        "PREVIEW_REPLY={}",
        json!({"placement":mode,"result":result})
    );
    match mode.as_str() {
        "person" => {
            let reply = result.unwrap();
            assert_eq!(reply["ok"], true);
            assert_eq!(reply["preview"], "on");
            let off = tauri::async_runtime::block_on(teams_preview(false)).unwrap();
            assert_eq!(off["preview"], "off");
        }
        "agent" => assert!(result.is_err(), "an agent must not turn on the preview"),
        _ => unreachable!(),
    }
    std::env::remove_var("ZERUS_CLIENT_HGS");
}
