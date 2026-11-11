//! The optional replicated key adds cross-computer routing. The home
//! computer's intake and policy use the team store independently of it.
use super::*;

pub(super) struct Link {
    pub team: String,
    pub home: String,
}

pub(super) fn read(document: &TaskDoc) -> Result<Option<Link>> {
    Ok(document.team().map_err(storage)?.map(|link| Link {
        team: link.team,
        home: link.home,
    }))
}

pub(super) fn write(document: &mut TaskDoc, team: &Team, who: &TaskActor, now: i64) -> Result<()> {
    document
        .set_team(
            Some(&zerus_replica::TaskTeam {
                team: team.id.clone(),
                home: team.home.clone(),
            }),
            who,
            now,
        )
        .map_err(storage)
}
