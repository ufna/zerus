use super::*;
use serde_json::json;
use std::{fs, os::unix::fs::PermissionsExt, time::Duration};
fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}
struct Fixture {
    root: tempfile::TempDir,
    state: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");
        fs::create_dir(&state).unwrap();
        fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
        Self { root, state }
    }
    fn put(&self, path: &str, value: Value) {
        let p = self.state.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::set_permissions(p.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(&p, value.to_string()).unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(0o600)).unwrap();
    }
    fn start(&self, allocated: bool) {
        let mut v = json!({"launch_id":id(1),"name":"kimi/example/original","started_at":1.0});
        if allocated {
            v["session_id"] = json!(id(2));
        }
        self.put(&format!("launches/{}.json", id(1)), v);
    }
    fn value(&self) -> Value {
        json!({"version":1,"layout":"id","session_id":id(2),"launch_id":id(1),"name":"kimi/example/original","agent":"kimi","account_id":"native-kimi","run_id":id(3),"conversation_id":"","cwd":"/workspace/example"})
    }
    fn read(&self) -> TeamObservation {
        observe(&self.state, &id(1), Instant::now() + Duration::from_secs(3))
    }
    fn legacy(&self, v: Value) {
        self.put(
            &format!("{:x}.json", Sha256::digest(b"kimi/example/original")),
            v,
        );
    }
}
#[test]
fn teams_reads_a_kimi_record_without_a_conversation_and_after_rename() {
    let f = Fixture::new();
    let _ = &f.root;
    f.start(true);
    let mut v = f.value();
    v["name"] = json!("kimi/example/renamed");
    f.put(&format!("sessions/{}.json", id(2)), v);
    let TeamObservation::Record(r) = f.read() else {
        panic!("renamed trust record was hidden")
    };
    assert_eq!(r.name, "kimi/example/renamed");
    assert!(r.conversation_id.is_empty());
    assert_eq!(r.launch_id, id(1));
}
#[test]
fn an_older_start_record_uses_the_legacy_label_route() {
    let f = Fixture::new();
    f.start(false);
    let mut v = f.value();
    v.as_object_mut().unwrap().remove("layout");
    f.legacy(v);
    assert!(matches!(f.read(), TeamObservation::Record(_)));
}
#[test]
fn a_record_from_another_launch_in_the_same_folder_is_positive_mismatch() {
    let f = Fixture::new();
    f.start(true);
    let mut v = f.value();
    v["launch_id"] = json!(id(9));
    f.put(&format!("sessions/{}.json", id(2)), v);
    assert_eq!(
        f.read(),
        TeamObservation::Mismatch {
            field: "launch_id".into()
        }
    );
}
#[test]
fn ended_missing_unreadable_and_expired_observations_remain_distinct() {
    let f = Fixture::new();
    f.start(false);
    assert_eq!(f.read(), TeamObservation::Absent { legacy: true });
    f.put(
        &format!("launches/{}.end.json", id(1)),
        json!({"launch_id":id(1),"name":"kimi/example/original","code":3,"ended_at":2.0}),
    );
    assert_eq!(f.read(), TeamObservation::Ended { code: 3 });
    assert_eq!(
        observe(&f.state, &id(1), Instant::now()),
        TeamObservation::Unavailable
    );
}
#[test]
fn a_legacy_rename_shadow_routes_but_does_not_authorize_a_binding() {
    let f = Fixture::new();
    f.start(false);
    let mut shadow = f.value();
    shadow["rename_shadow"] = json!(true);
    shadow["rename_target"] = json!("kimi/example/new");
    f.legacy(shadow);
    let mut v = f.value();
    v["name"] = json!("kimi/example/new");
    f.put(
        &format!("{:x}.json", Sha256::digest(b"kimi/example/new")),
        v,
    );
    assert!(matches!(f.read(),TeamObservation::Record(r) if r.name=="kimi/example/new"));
}

#[test]
fn native_providers_read_their_own_legacy_bindings_and_id_layout() {
    for agent in ["dsh", "opencode"] {
        for allocated in [false, true] {
            for by_id in [false, true] {
                let f = Fixture::new();
                let label = format!("{agent}/example/original");
                let mut start = json!({"launch_id":id(1),"name":label,"started_at":1.0});
                if allocated {
                    start["session_id"] = json!(id(2));
                }
                f.put(&format!("launches/{}.json", id(1)), start);
                let mut value = f.value();
                value["name"] = json!(label);
                value["agent"] = json!(agent);
                value["backend"] = json!(agent);
                value.as_object_mut().unwrap().remove("account_id");
                if by_id {
                    value["name"] = json!(format!("{agent}/example/renamed"));
                    f.put(&format!("{agent}/sessions/{}.json", id(2)), value);
                    let leaf = format!("{:x}.json", Sha256::digest(label.as_bytes()));
                    f.put(
                        &format!("labels/{leaf}"),
                        json!({"label":label,"ids":[id(2)]}),
                    );
                } else {
                    value.as_object_mut().unwrap().remove("layout");
                    let leaf = format!("{:x}.json", Sha256::digest(label.as_bytes()));
                    f.put(&format!("{agent}/bindings/{leaf}"), value);
                    f.put(&format!("ids/{}.json", id(2)), json!({"name":label}));
                }
                let TeamObservation::Record(record) = f.read() else {
                    panic!("{agent}: allocated={allocated}, by_id={by_id}")
                };
                assert_eq!(record.agent, agent);
                assert_eq!(record.account, format!("native-{agent}"));
                assert!(record.conversation_id.is_empty());
            }
        }
    }
}
