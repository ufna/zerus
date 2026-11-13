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
    refused_host("opencode", true);
}
#[test]
fn a_host_served_member_is_refused_at_creation() {
    refused_host("opencode", false);
}

#[test]
fn deepseek_seats_are_created_without_contacting_or_starting_a_host() {
    for lead in [false, true] {
        let mut h = Harness::empty();
        let mut op = create();
        if let Operation::Create { bundle, .. } = &mut op {
            if lead { bundle.template.lead.agent = "dsh".into(); }
            else {
                let role = &bundle.template.roster[0].role;
                bundle.roles.iter_mut().find(|r| &r.id == role).unwrap().agent = "dsh".into();
            }
        }
        h.run(&person(), op).unwrap();
        assert!(!h.temp.path().join("dsh").exists());
    }
}
