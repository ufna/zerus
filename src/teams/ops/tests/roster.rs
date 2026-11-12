use super::*;

fn refused_host(agent: &str, lead: bool) {
    let mut h = Harness::empty();
    let mut op = create();
    if let Operation::Create { bundle, .. } = &mut op {
        if lead {
            bundle.template.lead.agent = agent.into();
        } else {
            let role = &bundle.template.roster[0].role;
            bundle
                .roles
                .iter_mut()
                .find(|r| &r.id == role)
                .unwrap()
                .agent = agent.into();
        }
    }
    let problem = h.run(&person(), op).unwrap_err();
    assert_eq!(problem.code, code::INVALID_STATE);
    assert!(problem.message.contains(agent), "{problem:?}");
    assert!(problem.message.contains("served by a resident agent host"));
    assert!(problem
        .message
        .contains("cannot be a team lead or member in this milestone"));
    assert!(h.store.db.is_none(), "creation writes nothing");
}
#[test]
fn a_host_served_lead_is_refused_at_creation() {
    for agent in ["dsh", "opencode"] {
        refused_host(agent, true);
    }
}
#[test]
fn a_host_served_member_is_refused_at_creation() {
    for agent in ["dsh", "opencode"] {
        refused_host(agent, false);
    }
}
