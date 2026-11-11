//! The reviewer's real admission-window probes, now asserted regressions.
use super::*;

fn submitted(id: &str) {
    crate::desktop::teams::child::FIXTURE_EXIT.with(|s| {
        *s.borrow_mut() = Some(
            serde_json::json!({"schema":1,"id":id,"state":"submitted","error":""})
                .to_string()
                .into_bytes(),
        )
    });
}

/// The person holds the node after HandoutIntent reserved its effect and
/// before the spawn. The production admission decides.
struct HoldAtDelivery {
    native: native::Native,
    calls: usize,
    mode: &'static str,
    dep: u32,
}
impl ports::Ports for HoldAtDelivery {
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
    fn deliver(&mut self, t: &Team, n: &Node, m: &Member, id: &str) -> ops::HandoutOutcome {
        self.calls += 1;
        if self.mode.starts_with("fleet ") {
            super::fleet::change(&self.native, self.mode);
        }
        let operation = match self.mode {
            "hold" => Some(Operation::Hold { node: n.id }),
            "new hard dependency" => {
                let mut op = crate::teams::ops::tests::route(n.id);
                if let Operation::Route { deps, .. } = &mut op {
                    deps.push(Dep {
                        node: self.dep,
                        kind: DepKind::Hard,
                    });
                }
                Some(op)
            }
            "reassign" => Some(Operation::Reassign {
                node: n.id,
                member: "m2".into(),
                reason: "Change the executor".into(),
            }),
            "cancel" => Some(Operation::Cancel {
                node: n.id,
                reason: "Cancel before the spawn".into(),
            }),
            "assignee" => {
                let mut op = crate::teams::ops::tests::route(n.id);
                if let Operation::Route { assignee, .. } = &mut op {
                    *assignee = Some("m2".into());
                }
                Some(op)
            }
            "role" => {
                let mut op = crate::teams::ops::tests::route(n.id);
                if let Operation::Route { role, pipeline, .. } = &mut op {
                    *role = "reviewer".into();
                    *pipeline = "reviewer-work".into();
                }
                Some(op)
            }
            _ => None,
        };
        if let Some(operation) = operation {
            let mut store = Store::open(&self.native.config.state).unwrap();
            let current = store.team(&t.id).unwrap().unwrap();
            let r = ops::apply(
                &mut store,
                &person(),
                &crate::teams::ops::tests::request(current.revision, operation),
                100,
            );
            r.unwrap();
        }
        submitted(id);
        ports::Ports::deliver(&mut self.native, t, n, m, id)
    }
}
#[test]
fn review08_r2_hold_between_handout_intent_and_spawn() {
    for mode in [
        "control",
        "hold",
        "new hard dependency",
        "reassign",
        "cancel",
        "assignee",
        "role",
        "fleet busy",
        "fleet pending",
        "fleet background",
        "fleet unknown",
        "fleet epoch",
        "fleet run",
    ] {
        let mut h = Harness::empty();
        let mut create = crate::teams::ops::tests::create();
        if let Operation::Create { bundle, .. } = &mut create {
            bundle.pipelines[0].stages.clear();
            let mut reviewer = bundle.pipelines[0].clone();
            reviewer.id = "reviewer-work".into();
            reviewer.implementer = "reviewer".into();
            bundle.pipelines.push(reviewer);
        }
        h.run(&person(), create).unwrap();
        h.bind("m1");
        h.bind("m2");
        h.bind("lead");
        let node = h.add(&person(), 1);
        let dep = h.add(&person(), 2);
        // Only node 1 is offered: node 2 is held from the start.
        h.run(&person(), Operation::Hold { node: dep }).unwrap();
        let machine = h.store.team(TEAM).unwrap().unwrap().home;
        let mut a = Runtime::open(h.temp.path(), &machine).unwrap();
        let mut p = HoldAtDelivery {
            native: native(&h),
            calls: 0,
            mode,
            dep,
        };
        a.tick(&mut h.store, &mut p, TEAM, 100, false).unwrap();
        let effects = h.store.effects(TEAM, 200, None).unwrap().rows;
        let n = h.store.node(TEAM, node).unwrap().unwrap();
        assert_eq!(p.calls, 1, "{mode}");
        assert_eq!(effects.len(), 1, "{mode}");
        if mode == "control" {
            assert_eq!(effects[0].state, "submitted");
        } else {
            assert_eq!(
                effects[0].outcome.as_ref().unwrap()["status"],
                "refused",
                "{mode}: {:?}",
                effects[0]
            );
        }
        assert!(n.assignee.as_deref() != Some("m1"), "{mode}");
    }
}
