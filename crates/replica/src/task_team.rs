//! Optional team routing metadata. This field deliberately has no schema
//! upgrade: an older reader preserves unknown field-map entries on edits.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskTeam {
    pub team: String,
    pub home: String,
}

/// Forward-compatible metadata is parsed only within this UTF-8 byte bound.
const MAX_TEAM_FIELD_BYTES: usize = 2048;

impl TaskTeam {
    fn validate(&self) -> Result<()> {
        if !ids::is_uuid(&self.team) || !ids::is_uuid(&self.home) {
            return Err(Error::new("a task team needs team and home computer UUIDs"));
        }
        Ok(())
    }
}

impl TaskDoc {
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_raw_team_for_test(&mut self, raw: &str) {
        self.fields().insert("team", raw).unwrap();
        self.doc.commit();
    }
    fn team_field(&self) -> (Option<TaskTeam>, bool) {
        let Some(value) = self.fields().get("team") else {
            return (None, false);
        };
        let Ok(LoroValue::String(raw)) = value.into_value() else {
            return (None, true);
        };
        if raw.len() > MAX_TEAM_FIELD_BYTES {
            return (None, true);
        }
        match serde_json::from_str::<TaskTeam>(&raw) {
            Ok(team) if team.validate().is_ok() => (Some(team), false),
            _ => (None, true),
        }
    }

    pub fn team(&self) -> Result<Option<TaskTeam>> {
        Ok(self.team_field().0)
    }

    /// An invalid marker is preserved and exposed, never treated as damage
    /// to the task or as permission to bypass a team.
    pub fn team_invalid(&self) -> bool {
        self.team_field().1
    }

    /// Changes only the optional field and its history. Target and schema
    /// stay independent; a hand-out still uses the ordinary task writer.
    pub fn set_team(&mut self, team: Option<&TaskTeam>, actor: &Actor, now: i64) -> Result<()> {
        let name = team
            .cloned()
            .or_else(|| self.team_field().0)
            .map_or_else(|| "unknown team".into(), |t| t.team);
        self.set_team_named(team, &name, actor, now)
    }

    pub fn set_team_named(
        &mut self,
        team: Option<&TaskTeam>,
        name: &str,
        actor: &Actor,
        now: i64,
    ) -> Result<()> {
        self.writable()?;
        if let Some(team) = team {
            team.validate()?;
        }
        if name.trim().is_empty() || name.len() > 512 || name.chars().any(char::is_control) {
            return Err(Error::new("invalid team display name"));
        }
        if !self.team_invalid() && self.team()?.as_ref() == team {
            return Ok(());
        }
        if self.content_len() + 1024 > MAX_DOCUMENT {
            return Err(Error::new("task history is full"));
        }
        let raw = team
            .map(serde_json::to_string)
            .transpose()?
            .unwrap_or_default();
        if team.is_some() {
            self.fields().insert("team", raw.as_str())?;
        } else {
            self.fields().delete("team")?;
        }
        let text = format!(
            "{} team {}",
            if team.is_some() {
                "Sent to"
            } else {
                "Removed from"
            },
            name.trim()
        );
        self.log("team", actor, now, &text, None)?;
        self.commit()
    }
}
