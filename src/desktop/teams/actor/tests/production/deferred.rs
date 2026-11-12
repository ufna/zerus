use super::*;
use std::{cell::Cell, rc::Rc};

fn submitted(id: &str) {
    crate::desktop::teams::child::FIXTURE_EXIT.with(|s| {
        *s.borrow_mut() = Some(
            serde_json::json!({"schema":1,"id":id,"state":"submitted","error":""})
                .to_string()
                .into_bytes(),
        )
    });
}
fn as_person(state: &Path, op: Operation) -> String {
    let mut store = Store::open(state).unwrap();
    let rev = store.team(TEAM).unwrap().unwrap().revision;
    match ops::apply(
        &mut store,
        &person(),
        &crate::teams::ops::tests::request(rev, op),
        100,
    ) {
        Ok(r) => format!("ok rev {}", r.revision),
        Err(e) => format!("refused {}: {}", e.code, e.message),
    }
}
type Change = Box<dyn FnMut(usize, &native::Native, &Team, &Node) -> String>;
struct W {
    native: native::Native,
    calls: usize,
    admissions: Rc<Cell<usize>>,
    busy_on: Vec<usize>,
    change: Change,
    log: Vec<String>,
}
impl ports::Ports for W {
    fn changes(&mut self, _: &str, _: &str, _: u64) -> ports::Result<zerus_replica::Changes> {
        Ok(zerus_replica::Changes {
            generation: "journal".into(),
            offset: 0,
            documents: Some(Default::default()),
        })
    }
    fn observe(&mut self, _: &mut Store, _: &TaskRef) -> ports::Result<()> {
        Ok(())
    }
    fn free(&mut self, _: &Team, m: &Member) -> ports::Result<bool> {
        Ok(m.id == "m1")
    }
    fn deliver(&mut self, t: &Team, n: &Node, m: &Member, id: &str) -> ports::DeliveryResult {
        self.calls += 1;
        let note = (self.change)(self.calls, &self.native, t, n);
        let held = if self.busy_on.contains(&self.calls) {
            let db =
                rusqlite::Connection::open(self.native.config.state.join("teams/teams-v1.sqlite3"))
                    .unwrap();
            db.execute_batch("BEGIN IMMEDIATE").unwrap();
            Some(db)
        } else {
            None
        };
        let count = self.admissions.clone();
        crate::desktop::original_child::AFTER_FINAL_CHECK
            .with(|s| *s.borrow_mut() = Some(Box::new(move || count.set(count.get() + 1))));
        submitted(id);
        let started = std::time::Instant::now();
        let reply = ports::Ports::deliver(&mut self.native, t, n, m, id);
        crate::desktop::original_child::AFTER_FINAL_CHECK.with(|s| s.borrow_mut().take());
        crate::desktop::teams::child::FIXTURE_EXIT.with(|s| s.borrow_mut().take());
        drop(held);
        self.log.push(format!(
            "call {} node {} member {} change [{note}] -> {:?} in {:?}",
            self.calls,
            n.id,
            m.id,
            reply,
            started.elapsed()
        ));
        reply
    }
}
fn w(h: &Harness, busy_on: Vec<usize>, change: Change) -> W {
    W {
        native: native(h),
        calls: 0,
        admissions: Default::default(),
        busy_on,
        change,
        log: vec![],
    }
}
fn none() -> Change {
    Box::new(|_, _, _, _| String::new())
}

#[test]
fn a_busy_not_started_delivery_is_refused_before_terminal_cancellation() {
    for archived in [false, true] {
        let mut h = Harness::new();
        h.bind("m1");
        let node = h.add(&person(), 1);
        let machine = h.store.team(TEAM).unwrap().unwrap().home;
        let mut a = Runtime::open(h.temp.path(), &machine).unwrap();
        let mut p = w(&h, vec![1], none());
        a.tick(&mut h.store, &mut p, TEAM, 100, false).unwrap();
        assert_eq!(p.calls, 1);
        let row = a
            .ledger
            .prefix_page(TEAM, "delivery:", "")
            .unwrap()
            .remove(0);
        assert_eq!(row.data["not_started"], true);
        assert_eq!(h.store.open_effects(TEAM, 200, None).unwrap().rows.len(), 1);
        h.run(&person(), Operation::Stop).unwrap();
        if archived {
            h.run(&person(), Operation::Archive).unwrap();
        }
        drop(a);
        let mut a = Runtime::open(h.temp.path(), &machine).unwrap();
        // The real terminal page must settle the proof before asking for
        // cancel work. No live-team tick is needed after Stop.
        a.terminal_cancels(&mut h.store, &mut p, 101).unwrap();
        assert_eq!(p.calls, 1);
        assert!(h
            .store
            .open_effects(TEAM, 200, None)
            .unwrap()
            .rows
            .is_empty());
        assert!(a
            .ledger
            .prefix_page(TEAM, "delivery:", "")
            .unwrap()
            .is_empty());
        let effect = h.store.effects(TEAM, 200, None).unwrap().rows.remove(0);
        assert_eq!(effect.state, "refused");
        assert!(h.node(node).assignee.is_none());
        a.tick(&mut h.store, &mut p, TEAM, 60_100, false).unwrap();
        assert_eq!(p.calls, 1);
    }
}
