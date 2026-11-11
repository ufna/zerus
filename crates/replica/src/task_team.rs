//! Optional team routing metadata. This field deliberately has no schema
//! upgrade: an older reader preserves unknown field-map entries on edits.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskTeam {
    pub team: String,
    pub home: String,
}

impl TaskTeam {
    fn validate(&self) -> Result<()> {
        if !ids::is_uuid(&self.team) || !ids::is_uuid(&self.home) {
            return Err(Error::new("a task team needs team and home computer UUIDs"));
        }
        Ok(())
    }
}

impl TaskDoc {
    pub fn team(&self) -> Result<Option<TaskTeam>> {
        let Some(raw) = string(&self.fields(), "team") else {
            if self.fields().get("team").is_some() {
                return Err(Error::failed("invalid task team field"));
            }
            return Ok(None);
        };
        let team: TaskTeam = serde_json::from_str(&raw)?;
        team.validate()?;
        Ok(Some(team))
    }

    /// Changes only the optional field and its history. Target and schema
    /// stay independent; a hand-out still uses the ordinary task writer.
    pub fn set_team(&mut self, team: Option<&TaskTeam>, actor: &Actor, now: i64) -> Result<()> {
        self.writable()?;
        if let Some(team) = team {
            team.validate()?;
        }
        if self.team()?.as_ref() == team {
            return Ok(());
        }
        if self.content_len() + 1024 > MAX_DOCUMENT {
            return Err(Error::new("task history is full"));
        }
        let text = team
            .map(serde_json::to_string)
            .transpose()?
            .unwrap_or_default();
        if team.is_some() {
            self.fields().insert("team", text.as_str())?;
        } else {
            self.fields().delete("team")?;
        }
        self.log("team", actor, now, &text, None)?;
        self.commit()
    }
}
