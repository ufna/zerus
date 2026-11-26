use super::*;

#[test]
fn fresh_activation_mints_ids_without_accepting_the_old_store_or_a_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path();
    private_dir(path).unwrap();
    let old_id = uuid::Uuid::new_v4().to_string();
    let original = Arc::new(Store::for_request(path, &old_id, true, true).unwrap());
    let fingerprint = "a".repeat(64);
    let old = fence_store(original.clone(), &old_id, &fingerprint).unwrap();
    assert!(
        path.join(STABLE_ANCHOR).is_file(),
        "test requires a private APFS root"
    );
    drop(original);

    // This private fixture stands in for an older binary's v1-only store
    // captured with a different device number before a remount. The real
    // writer above supplied the original receipt and its outside anchor.
    std::fs::rename(path.join(STABLE_ANCHOR), path.join("fixture-v2-anchor")).unwrap();
    let anchor = path.join(ANCHOR);
    let mut legacy: Value = serde_json::from_slice(&std::fs::read(&anchor).unwrap()).unwrap();
    legacy["directory_device"] = json!(legacy["directory_device"].as_u64().unwrap() + 1);
    let legacy = legacy.to_string();
    std::fs::write(&anchor, &legacy).unwrap();
    let old_path = path
        .join("worktree-requests")
        .join(format!("{old_id}.json"));
    let original_bytes = std::fs::read(&old_path).unwrap();
    assert!(Store::for_request(path, &old_id, false, false).is_err());

    // Test the recovery writer in isolation. The person-only CLI admission
    // stays outside this test and is never offered to an agent as a fallback.
    Store::activate_fresh(path).unwrap();
    assert!(Store::fresh_configured(path).unwrap());
    let issued = Store::issue(path).unwrap();
    assert_ne!(issued, old_id);
    for id in [&old_id, &uuid::Uuid::new_v4().to_string()] {
        assert!(Store::for_request(path, id, true, true).is_err());
        assert!(Store::for_request(path, id, false, false).is_err());
    }
    let fresh = Arc::new(Store::for_request(path, &issued, false, false).unwrap());
    assert!(fresh.read(&issued).unwrap().is_none());
    let fresh_receipt = fence_store(fresh.clone(), &issued, &fingerprint).unwrap();
    assert_eq!(fresh_receipt["past_outcome"], "unknown");
    assert_eq!(fresh_receipt["request_id"], issued);
    assert_eq!(fresh.read(&issued).unwrap(), Some(fresh_receipt));
    assert_ne!(old["request_id"], issued);
    assert_eq!(std::fs::read(&old_path).unwrap(), original_bytes);
    assert_eq!(std::fs::read_to_string(&anchor).unwrap(), legacy);

    let fresh_path = path.join("worktree-requests-fresh");
    let retired = path.join("retained-fresh");
    std::fs::rename(&fresh_path, &retired).unwrap();
    private_dir(&fresh_path).unwrap();
    assert!(fresh.verify().is_err());
    assert!(Store::for_request(path, &issued, true, true).is_err());
    assert!(Store::issue(path).is_err());
    assert!(Store::activate_fresh(path).is_err());
    assert_eq!(std::fs::read_dir(fresh_path).unwrap().count(), 0);
    assert!(retired.join(format!("{issued}.json")).is_file());
    assert_eq!(std::fs::read(&old_path).unwrap(), original_bytes);
}
