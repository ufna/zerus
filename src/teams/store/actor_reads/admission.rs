//! A read admission owns the store's writer fence only across effect start.
use super::*;
pub struct Admission(Store);
impl std::ops::Deref for Admission {
    type Target = Store;
    fn deref(&self) -> &Store {
        &self.0
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        if let Some(db) = &self.0.db {
            let _ = db.execute_batch("ROLLBACK");
        }
    }
}
impl Store {
    /// Starts no store and writes no facts. Dropping the owning guard releases
    /// its immediate transaction, including on refusal or a failed spawn.
    pub fn admission(self) -> Result<Admission> {
        self.active()?;
        self.db
            .as_ref()
            .ok_or_else(|| storage("Admission store is missing"))?
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(storage)?;
        Ok(Admission(self))
    }
}
impl Admission {
    pub fn handout_admissible(&self, team: &Team, node: u32, member: &str) -> Result<()> {
        self.active()?;
        crate::teams::ops::handout_admissible(
            self.0.db.as_ref().expect("admitted store"),
            team,
            node,
            member,
        )
    }

    /// The enclosing admission already supplies the snapshot/archival fence;
    /// unlike the standalone read, this must not start a nested transaction.
    pub fn node(&self, team: &str, node: NodeId) -> Result<Option<Node>> {
        self.active()?;
        rows::node(self.0.db.as_ref().expect("admitted store"), team, node)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::teams::ops::tests::*;
    #[test]
    fn handout_predicate_uses_the_snapshot_and_allows_its_own_reserved_effect() {
        let mut h = Harness::new();
        h.bind("m1");
        let node = h.add(&person(), 1);
        h.intent(node, "m1");
        let guard = Store::open(h.temp.path()).unwrap().admission().unwrap();
        let team = guard.team(TEAM).unwrap().unwrap();
        guard.handout_admissible(&team, node, "m1").unwrap();
        drop(guard);
        h.run(&person(), crate::teams::ops::Operation::Hold { node })
            .unwrap();
        let guard = Store::open(h.temp.path()).unwrap().admission().unwrap();
        assert!(guard.handout_admissible(&team, node, "m1").is_err());
    }
    #[test]
    fn admission_keeps_facts_and_excludes_writers_until_dropped() {
        let mut h = Harness::new();
        h.add(&person(), 1);
        let guard = Store::open(h.temp.path()).unwrap().admission().unwrap();
        assert!(guard.node(TEAM, 1).unwrap().is_some());
        assert_eq!(
            guard.team(TEAM).unwrap().unwrap().status,
            TeamStatus::Running
        );
        let db = h.store.db.as_ref().unwrap();
        db.busy_timeout(std::time::Duration::ZERO).unwrap();
        assert!(db.execute_batch("BEGIN IMMEDIATE").is_err());
        drop(guard);
        db.execute_batch("BEGIN IMMEDIATE; ROLLBACK").unwrap();
    }
    #[test]
    fn an_absent_store_cannot_grant_admission_or_create_one() {
        let h = Harness::empty();
        assert!(Store::open(h.temp.path()).unwrap().admission().is_err());
        assert!(!h.temp.path().join("teams/teams-v1.sqlite3").exists());
    }
}
