use super::*;

#[test]
fn missing_store_is_lazy_and_preview_is_required() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(Store::open(temp.path()).unwrap_err().code, "teams_inactive");
    assert!(!temp.path().join("teams").exists());
    std::fs::create_dir(temp.path().join("teams")).unwrap();
    std::fs::write(temp.path().join("teams/preview"), "on").unwrap();
    let store = Store::open(temp.path()).unwrap();
    assert!(store.team("missing").unwrap().is_none());
    assert!(!temp.path().join("teams/teams-v1.sqlite3").exists());
}
