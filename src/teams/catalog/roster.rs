//! Milestone policy shares the caller proof's resident-host classification.
use zerus_team_plan::{problem::code, Problem, TemplateBundle};

pub(crate) fn validate_roster(bundle: &TemplateBundle) -> Result<(), Problem> {
    let agents = std::iter::once(bundle.template.lead.agent.as_str()).chain(
        bundle.template.roster.iter().filter_map(|seat| {
            bundle
                .roles
                .iter()
                .find(|role| role.id == seat.role)
                .map(|role| role.agent.as_str())
        }),
    );
    for agent in agents {
        if crate::teams::caller::host_served_agent(agent) {
            return Err(Problem::new(
                code::INVALID_STATE,
                format!("A session of agent kind '{agent}' is served by a resident agent host and cannot be a team lead or member in this milestone."),
            ).with_refs([agent]));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::*;
    fn catalog_with(agent: &str, lead: bool) -> Catalog {
        let mut catalog = builtin();
        if lead {
            for template in &mut catalog.templates {
                template.lead.agent = agent.into();
            }
        } else {
            catalog
                .roles
                .iter_mut()
                .find(|r| r.id == "developer")
                .unwrap()
                .agent = agent.into();
        }
        catalog
    }
    #[test]
    fn saving_a_template_refuses_host_served_leads_and_members() {
        for agent in ["dsh", "opencode"] {
            for lead in [true, false] {
                let dir = tempfile::tempdir().unwrap();
                let problem = save(dir.path(), &catalog_with(agent, lead)).unwrap_err();
                assert_eq!(problem.code, code::INVALID_STATE);
                assert!(problem.message.contains("served by a resident agent host"));
                assert!(!path(dir.path()).exists());
            }
        }
    }
    #[test]
    fn using_an_older_saved_template_refuses_a_host_served_roster() {
        for agent in ["dsh", "opencode"] {
            for lead in [true, false] {
                let dir = tempfile::tempdir().unwrap();
                let file = path(dir.path());
                std::fs::create_dir_all(file.parent().unwrap()).unwrap();
                let bytes = serde_json::to_vec(&catalog_with(agent, lead)).unwrap();
                std::fs::write(&file, &bytes).unwrap();
                let catalog = load(dir.path()).unwrap();
                let problem = export_bundle(&catalog, "bugfix").unwrap_err();
                assert_eq!(problem.code, code::INVALID_STATE);
                assert_eq!(std::fs::read(file).unwrap(), bytes, "no repair on read");
            }
        }
    }
    #[test]
    fn every_builtin_roster_uses_agents_the_caller_proof_can_bind() {
        let catalog = builtin();
        for template in &catalog.templates {
            assert!(!crate::teams::caller::host_served_agent(
                &template.lead.agent
            ));
            for seat in &template.roster {
                let role = catalog.roles.iter().find(|r| r.id == seat.role).unwrap();
                assert!(!crate::teams::caller::host_served_agent(&role.agent));
            }
            export_bundle(&catalog, &template.id).unwrap();
        }
    }
}
