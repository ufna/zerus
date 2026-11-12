//! Reads for the lead's unstaffed items (zerus-63rru.45). Open items are read
//! through the partial index `inbox_unstaffed`, so their cost does not grow
//! with the team's settled history.
use super::*;
use std::collections::BTreeSet;

/// The open unstaffed items of a team, one per node, by node.
pub(crate) const OPEN_ITEMS: &str = "SELECT seq,node FROM inbox WHERE team=?1 AND state='open' AND kind LIKE 'unstaffed:%' ORDER BY node LIMIT 501";
/// The nodes with an open effect (the store's own [`OPEN_EFFECT_STATES`]):
/// each is in somebody's hands or on its way.
pub(crate) fn open_effect_nodes_sql() -> String {
    format!("SELECT node FROM effects WHERE team=?1 AND state IN {OPEN_EFFECT_STATES} LIMIT 601")
}
/// At most this many open unstaffed items: one per ready node, and the
/// admitted cap bounds the ready nodes. One more fails closed.
pub(crate) const MAX_OPEN: usize = 500;

/// The open unstaffed items, `(seq, node)`; more than [`MAX_OPEN`] fails
/// closed with a named refusal rather than a cut list.
pub(crate) fn open_items(db: &Connection, team: &str) -> Result<Vec<(u64, u32)>> {
    let rows = db
        .prepare(OPEN_ITEMS)
        .and_then(|mut s| {
            s.query_map([team], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()
        })
        .map_err(storage)?;
    if rows.len() > MAX_OPEN {
        return Err(Problem::new(
            code::OVER_CAP,
            "More than 500 open unstaffed items: the reconcile fails closed.",
        ));
    }
    Ok(rows)
}
/// The nodes with an open effect, bounded by the live nodes.
pub(crate) fn open_effect_nodes(db: &Connection, team: &str) -> Result<BTreeSet<u32>> {
    let rows = db
        .prepare(&open_effect_nodes_sql())
        .and_then(|mut s| {
            s.query_map([team], |r| r.get(0))?
                .collect::<std::result::Result<Vec<u32>, _>>()
        })
        .map_err(storage)?;
    if rows.len() > 600 {
        return Err(Problem::new(
            code::OVER_CAP,
            "More open effects than live nodes: the reconcile fails closed.",
        ));
    }
    Ok(rows.into_iter().collect())
}

impl Store {
    /// The nodes whose unstaffed item is open, for the service's comparison.
    pub fn open_unstaffed(&self, team: &str) -> Result<BTreeSet<u32>> {
        self.active()?;
        let Some(db) = &self.db else {
            return Ok(BTreeSet::new());
        };
        Ok(open_items(db, team)?.into_iter().map(|(_, n)| n).collect())
    }
}
