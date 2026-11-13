//! The witness of zerus-63rru.23: what the actor does today over a hand-out
//! refused by the project's `agents_other` or `agent_limit` setting. The real
//! command's side is pinned by `project_limits_refuse_with_distinct_stable_codes`
//! in the task command's own tests. These facts were measured for the survey;
//! a later shape for the gap may change them, so the test names the behaviour
//! it pins and nothing more.
use super::super::ports::{DeliveryResult, Ports};
use super::super::Runtime;
use super::*;
use crate::teams::ops::{self, HandoutOutcome, Operation};

struct Refused {
    reason: String,
    calls: u32,
}
impl Ports for Refused {
    fn changes(&mut self, _: &str, _: &str, _: u64) -> super::super::ports::Result<zerus_replica::Changes> {
        Ok(zerus_replica::Changes {
            generation: "journal".into(),
            offset: 0,
            documents: Some(Default::default()),
        })
    }
    fn observe(&mut self, _: &mut Store, _: &TaskRef) -> super::super::ports::Result<()> {
        Ok(())
    }
    fn free(&mut self, _: &Team, m: &Member) -> super::super::ports::Result<bool> {
        Ok(m.id == "m1" || m.id == "m2")
    }
    fn deliver(&mut self, _: &Team, _: &Node, _: &Member, _: &str) -> DeliveryResult {
        self.calls += 1;
        HandoutOutcome::Refused {
            reason: self.reason.clone(),
        }
        .into()
    }
}

/// Today's behaviour, pinned as the witness: the refusal is recorded on the
/// effect with its time row; the hand-out is retried on the 1/2/4/8/15-minute
/// backoff; the person's card opens after three failures and its
/// "acknowledged" decision closes it without stopping the retries; another
/// member is refused the same (the limit is the project's).
#[test]
fn a_project_limits_refusal_is_recorded_retried_and_card_opened() {
    for reason in [
        "project_agents_other: This project allows agents to start tasks only in their own session.",
        "project_agent_limit: The project's hourly agent start limit is reached.",
    ] {
        let mut h = Harness::new();
        h.bind("m1");
        h.bind("m2");
        let node = h.add(&person(), 1);
        let machine = h.store.team(TEAM).unwrap().unwrap().home;
        let mut a = Runtime::open(h.temp.path(), &machine).unwrap();
        let mut p = Refused {
            reason: reason.into(),
            calls: 0,
        };
        // The first pass refuses and records it; no assignee.
        a.tick(&mut h.store, &mut p, TEAM, 1000, false).unwrap();
        assert_eq!(p.calls, 1, "{reason}");
        let effect = h.store.effects(TEAM, 200, None).unwrap().rows.remove(0);
        assert_eq!(effect.state, "refused");
        let outcome: HandoutOutcome = serde_json::from_value(effect.outcome.unwrap()).unwrap();
        assert!(matches!(outcome, HandoutOutcome::Refused { .. }), "{reason}");
        assert!(h.store.node(TEAM, node).unwrap().unwrap().assignee.is_none());
        // Retried at +60s and +180s; the card opens on the third failure.
        a.tick(&mut h.store, &mut p, TEAM, 1000 + 60_000, false).unwrap();
        a.tick(&mut h.store, &mut p, TEAM, 1000 + 180_000, false).unwrap();
        assert_eq!(p.calls, 3, "{reason}");
        let cards = h.store.cards(TEAM, 200, None).unwrap().rows;
        let open: Vec<_> = cards.iter().filter(|c| c.state == "open").collect();
        assert_eq!(open.len(), 1, "{reason}");
        assert_eq!(open[0].kind, "team_actor_blocked");
        assert!(open[0].text.contains(reason), "{:?}", open[0].text);
        assert_eq!(open[0].node, None, "the card is not bound to the task");
        // "Acknowledged" closes the card; the retries continue.
        h.run(
            &person(),
            Operation::DecideCard {
                card: open[0].seq,
                decision: "acknowledged".into(),
            },
        )
        .unwrap();
        a.tick(&mut h.store, &mut p, TEAM, 1000 + 900_000, false).unwrap();
        assert_eq!(p.calls, 4, "{reason}");
        // Another member is refused the same.
        let revision = h.store.team(TEAM).unwrap().unwrap().revision;
        ops::apply(
            &mut h.store,
            &person(),
            &crate::teams::ops::tests::request(
                revision,
                Operation::PauseMember { member: "m1".into() },
            ),
            1000,
        )
        .unwrap();
        a.tick(&mut h.store, &mut p, TEAM, 1000 + 1_801_000, false).unwrap();
        let members: Vec<String> = h
            .store
            .effects(TEAM, 200, None)
            .unwrap()
            .rows
            .iter()
            .map(|e| e.member.clone())
            .collect();
        assert!(members.contains(&"m2".into()), "{members:?} {reason}");
    }
}
