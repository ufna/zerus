//! Bounded prompt context, read only from the team's stored model.
use super::*;
use zerus_team_plan::model::{DepKind, Node};

const LIMIT: usize = 12_000;
const CUT: &str = "\n[Team context was cut.]\n";

/// A stage task can pass its own node/member here as pipeline support grows.
/// Missing model fields are omitted; a task's prompt already carries its text.
pub(crate) fn build(store: &Store, team: &Team, node: &Node, member: &Member) -> Result<String> {
    let mut out = Context {
        text: String::new(),
        cut: false,
    };
    out.push("\n\nTeam context\nTeam: ", &team.name);
    out.push("Goal: ", &team.goal);
    if let Some(role) = team.bundle.roles.iter().find(|r| r.id == member.role) {
        out.push("Role: ", &role.name);
        out.push("Role instructions: ", &role.prompt);
    }
    if node.task.is_none() {
        out.push("Node text: ", &node.text);
    }
    out.push("Base commit: ", &team.base_commit);
    out.push("Integration branch: ", &team.integration_branch);
    for path in node.writes.iter().take(200) {
        out.push("Write set: ", path);
    }
    let Some(db) = &store.db else {
        return Ok(out.finish());
    };
    let mut remaining = 6000;
    for dep in node
        .deps
        .iter()
        .filter(|d| d.kind == DepKind::Hard)
        .take(200)
    {
        if remaining == 0 || out.cut {
            break;
        }
        let report:Option<String> = db.query_row("SELECT text FROM reports WHERE team=?1 AND node=?2 ORDER BY generation DESC LIMIT 1",params![team.id,dep.node],|r|r.get(0)).optional().map_err(storage)?;
        if let Some(report) = report {
            let text: String = report.chars().take(2000.min(remaining)).collect();
            remaining -= text.chars().count();
            out.push(&format!("Dependency {} report: ", dep.node), &text);
        }
    }
    let mut query = db
        .prepare(
            "SELECT kind,text FROM decisions WHERE team=?1 AND node=?2 ORDER BY seq DESC LIMIT 32",
        )
        .map_err(storage)?;
    let decisions = query
        .query_map(params![team.id, node.id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(storage)?;
    for decision in decisions {
        if out.cut {
            break;
        }
        let (kind, text) = decision.map_err(storage)?;
        out.push(&format!("Binding decision ({kind}): "), &text);
    }
    Ok(out.finish())
}

struct Context {
    text: String,
    cut: bool,
}
impl Context {
    fn push(&mut self, label: &str, value: &str) {
        if value.is_empty() || self.cut {
            return;
        }
        for part in [label, value, "\n"] {
            let mut end = part.len().min(LIMIT.saturating_sub(self.text.len()));
            while !part.is_char_boundary(end) {
                end -= 1;
            }
            self.text.push_str(&part[..end]);
            if end < part.len() {
                self.cut = true;
                break;
            }
        }
    }
    fn finish(mut self) -> String {
        if self.cut {
            let mut end = (LIMIT - CUT.len()).min(self.text.len());
            while !self.text.is_char_boundary(end) {
                end -= 1;
            }
            self.text.truncate(end);
            self.text.push_str(CUT);
        }
        self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::teams::ops::tests::{fixture, person, Harness, TEAM};

    #[test]
    fn context_names_stored_role_goal_base_and_writes() {
        let mut h = Harness::new();
        let id = h.add(&person(), 1);
        let mut node = h.node(id);
        node.writes = vec!["src/example/".into()];
        node.text = "This is not task acceptance criteria".into();
        let f = fixture();
        let member = &f.members[1];
        let text = build(&h.store, &f.team, &node, member).unwrap();
        assert!(text.contains(&f.team.name));
        assert!(text.contains(&f.team.goal));
        assert!(text.contains(&f.team.base_commit));
        assert!(text.contains("src/example/"));
        let role = f
            .team
            .bundle
            .roles
            .iter()
            .find(|r| r.id == member.role)
            .unwrap();
        assert!(text.contains(&role.name));
        assert!(text.contains(&role.prompt));
        assert!(!text.contains(&node.text));
        assert!(!text.contains("Member branch:"));
        assert_eq!(h.store.team(TEAM).unwrap().unwrap().revision, 3);
    }

    #[test]
    fn context_caps_dependency_reports_and_marks_a_utf8_cut() {
        use zerus_team_plan::model::{Dep, DepKind};
        let mut h = Harness::new();
        let mut deps = Vec::new();
        for n in 1..=5 {
            let id = h.add(&person(), n);
            h.store.db.as_ref().unwrap().execute("INSERT INTO reports(team,node,generation,actor,text,commit_id) VALUES(?1,?2,1,'{}',?3,'')", rusqlite::params![TEAM,id,"界".repeat(3000)]).unwrap();
            deps.push(Dep {
                node: id,
                kind: DepKind::Hard,
            });
        }
        let f = fixture();
        let mut node = h.node(1);
        node.deps = deps;
        let text = build(&h.store, &f.team, &node, &f.members[1]).unwrap();
        assert!(text.len() <= 12000);
        assert!(text.ends_with("[Team context was cut.]\n"));
        let mut team = f.team;
        team.goal = "é".repeat(20000);
        let cut = build(&h.store, &team, &node, &f.members[1]).unwrap();
        assert!(cut.len() <= 12000);
        assert!(cut.ends_with("[Team context was cut.]\n"));
        assert!(std::str::from_utf8(cut.as_bytes()).is_ok());
    }
}
