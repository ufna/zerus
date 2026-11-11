use super::*;
use rusqlite::params;

fn start(
    db: Option<&Connection>,
    scope: String,
    table: &str,
    team: &str,
    limit: u32,
    cursor: Option<&Cursor>,
) -> Result<Cursor> {
    if !(1..=MAX_PAGE).contains(&limit) {
        return Err(Problem::new(
            code::OVER_CAP,
            "Page limit must be between 1 and 200.",
        ));
    }
    if let Some(cursor) = cursor {
        if cursor.scope != scope || cursor.after > cursor.ceiling {
            return Err(Problem::new(
                code::INVALID_STATE,
                "Cursor belongs to a different list.",
            ));
        }
        return Ok(cursor.clone());
    }
    let ceiling = if let Some(db) = db {
        let sql=match table {
            "nodes"=>"SELECT COALESCE(MAX(id),0) FROM node_identity WHERE team=?1".to_owned(),
            "journal"=>"SELECT MAX(COALESCE((SELECT MAX(seq) FROM journal WHERE team=?1),0),COALESCE((SELECT MAX(seq) FROM journal_archive WHERE team=?1),0))".to_owned(),
            "members"=>"SELECT COALESCE(MAX(rowid),0) FROM members WHERE team=?1".to_owned(),
            _=>format!("SELECT COALESCE(MAX(seq),0) FROM {table} WHERE team=?1"),
        };
        db.query_row(&sql, [team], |r| r.get(0)).map_err(storage)?
    } else {
        0
    };
    Ok(Cursor {
        scope,
        ceiling,
        after: 0,
        rank: String::new(),
    })
}
fn finish<T>(
    rows: Vec<T>,
    mut cursor: Cursor,
    limit: u32,
    key: impl FnOnce(&T) -> (u64, String),
) -> Page<T> {
    let next = if rows.len() == limit as usize {
        if let Some(last) = rows.last() {
            let (after, rank) = key(last);
            cursor.after = after;
            cursor.rank = rank;
        }
        Some(cursor)
    } else {
        None
    };
    Page { rows, next }
}
impl Store {
    pub fn members(&self, team: &str, limit: u32, cursor: Option<&Cursor>) -> Result<Page<Member>> {
        self.active()?;
        let c = start(
            self.db.as_ref(),
            format!("{team}:members"),
            "members",
            team,
            limit,
            cursor,
        )?;
        let mut rows = Vec::new();
        if let Some(db) = &self.db {
            let mut s=db.prepare("SELECT data FROM members WHERE team=?1 AND rowid<=?2 AND id>?3 ORDER BY id LIMIT ?4").map_err(storage)?;
            rows = s
                .query_map(params![team, c.ceiling, c.rank, limit], |r| sql_json(r, 0))
                .map_err(storage)?
                .collect::<std::result::Result<Vec<Member>, _>>()
                .map_err(storage)?;
        }
        Ok(finish(rows, c, limit, |r| (0, r.id.clone())))
    }
    pub fn phases(&self, team: &str, limit: u32, cursor: Option<&Cursor>) -> Result<Page<Phase>> {
        self.active()?;
        if !(1..=MAX_PAGE).contains(&limit) {
            return Err(Problem::new(
                code::OVER_CAP,
                "Page limit must be between 1 and 200.",
            ));
        }
        let scope = format!("{team}:phases");
        let c = if let Some(c) = cursor {
            if c.scope != scope {
                return Err(storage("cursor belongs to another list"));
            }
            c.clone()
        } else {
            let ceiling = self.db.as_ref().map_or(Ok(0), |db| {
                db.query_row(
                    "SELECT COALESCE(MAX(n),0)+1 FROM phases WHERE team=?1",
                    [team],
                    |r| r.get(0),
                )
                .map_err(storage)
            })?;
            Cursor {
                scope,
                ceiling,
                after: 0,
                rank: String::new(),
            }
        };
        let mut rows = Vec::new();
        if let Some(db) = &self.db {
            let mut s = db
                .prepare(
                    "SELECT data FROM phases WHERE team=?1 AND n>=?2 AND n<?3 ORDER BY n LIMIT ?4",
                )
                .map_err(storage)?;
            rows = s
                .query_map(params![team, c.after, c.ceiling, limit], |r| sql_json(r, 0))
                .map_err(storage)?
                .collect::<std::result::Result<Vec<Phase>, _>>()
                .map_err(storage)?;
        }
        Ok(finish(rows, c, limit, |r| {
            (u64::from(r.n) + 1, String::new())
        }))
    }
    pub fn queue(&self, team: &str, limit: u32, cursor: Option<&Cursor>) -> Result<Page<Node>> {
        self.node_page(team, None, limit, cursor)
    }
    pub fn nodes_by_state(
        &self,
        team: &str,
        state: NodeState,
        limit: u32,
        cursor: Option<&Cursor>,
    ) -> Result<Page<Node>> {
        self.node_page(team, Some(state), limit, cursor)
    }
    fn node_page(
        &self,
        team: &str,
        state: Option<NodeState>,
        limit: u32,
        cursor: Option<&Cursor>,
    ) -> Result<Page<Node>> {
        self.active()?;
        let filter = state
            .map(|state| encode(&state))
            .transpose()?
            .map(|s| s.trim_matches('"').to_owned());
        let c = start(
            self.db.as_ref(),
            format!("{team}:nodes:{filter:?}"),
            "nodes",
            team,
            limit,
            cursor,
        )?;
        let mut rows = Vec::new();
        if let Some(db) = &self.db {
            let table = if matches!(state, Some(NodeState::Accepted | NodeState::Cancelled)) {
                "node_archive"
            } else {
                "nodes"
            };
            let sql = if filter.is_some() {
                format!("SELECT {} FROM {table} WHERE team=?1 AND node_state=?5 AND id>?2 AND id<=?3 ORDER BY id LIMIT ?4",rows::NODE_SELECT)
            } else {
                format!("SELECT {} FROM nodes WHERE team=?1 AND id<=?3 AND (rank>?5 OR (rank=?5 AND id>?2)) ORDER BY rank,id LIMIT ?4",rows::NODE_SELECT)
            };
            let mut s = db.prepare(&sql).map_err(storage)?;
            rows = s
                .query_map(
                    params![
                        team,
                        c.after,
                        c.ceiling,
                        limit,
                        filter.as_deref().unwrap_or(&c.rank)
                    ],
                    rows::node_row,
                )
                .map_err(storage)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(storage)?;
            for node in &mut rows {
                node.deps = rows::deps(db, team, node.id)?;
            }
        }
        Ok(finish(rows, c, limit, |r| {
            (
                u64::from(r.id),
                if state.is_none() {
                    r.rank.clone()
                } else {
                    String::new()
                },
            )
        }))
    }
    pub fn inbox(
        &self,
        team: &str,
        limit: u32,
        cursor: Option<&Cursor>,
    ) -> Result<Page<InboxItem>> {
        self.seq_page(
            team,
            "inbox",
            limit,
            cursor,
            |r| {
                Ok(InboxItem {
                    seq: r.get(0)?,
                    kind: r.get(1)?,
                    node: r.get(2)?,
                    generation: r.get(3)?,
                    actor: sql_json(r, 4)?,
                    text: r.get(5)?,
                    state: r.get(6)?,
                    reply: r.get(7)?,
                    reply_actor: r
                        .get::<_, Option<String>>(8)?
                        .map(|s| serde_json::from_str(&s))
                        .transpose()
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    created_ms: r.get(9)?,
                })
            },
            "seq,kind,node,generation,actor,text,state,reply,reply_actor,created_ms",
        )
    }
    pub fn cards(&self, team: &str, limit: u32, cursor: Option<&Cursor>) -> Result<Page<Card>> {
        self.seq_page(
            team,
            "cards",
            limit,
            cursor,
            |r| {
                Ok(Card {
                    seq: r.get(0)?,
                    kind: r.get(1)?,
                    node: r.get(2)?,
                    actor: sql_json(r, 3)?,
                    text: r.get(4)?,
                    state: r.get(5)?,
                    decision: r.get(6)?,
                    decided_actor: r
                        .get::<_, Option<String>>(7)?
                        .map(|s| serde_json::from_str(&s))
                        .transpose()
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    created_ms: r.get(8)?,
                })
            },
            "seq,kind,node,actor,text,state,decision,decided_actor,created_ms",
        )
    }
    pub fn journal(
        &self,
        team: &str,
        limit: u32,
        cursor: Option<&Cursor>,
    ) -> Result<Page<JournalEntry>> {
        self.seq_page(
            team,
            "journal",
            limit,
            cursor,
            |r| {
                Ok(JournalEntry {
                    seq: r.get(0)?,
                    at_ms: r.get(1)?,
                    actor: sql_json(r, 2)?,
                    kind: r.get(3)?,
                    request_id: r.get(4)?,
                    text: r.get(5)?,
                    refs: sql_json(r, 6)?,
                })
            },
            "seq,at_ms,actor,kind,request_id,text,refs",
        )
    }
    pub fn effects(
        &self,
        team: &str,
        limit: u32,
        cursor: Option<&Cursor>,
    ) -> Result<Page<EffectIntent>> {
        self.seq_page(
            team,
            "effects",
            limit,
            cursor,
            |r| {
                Ok(EffectIntent {
                    seq: r.get(0)?,
                    node: r.get(1)?,
                    member: r.get(2)?,
                    generation: r.get(3)?,
                    request_id: r.get(4)?,
                    actor: sql_json(r, 5)?,
                    state: r.get(6)?,
                    outcome: r
                        .get::<_, Option<String>>(7)?
                        .map(|s| serde_json::from_str(&s))
                        .transpose()
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    created_ms: r.get(8)?,
                })
            },
            "seq,node,member,generation,request_id,actor,state,outcome,created_ms",
        )
    }
    pub fn decisions(
        &self,
        team: &str,
        limit: u32,
        cursor: Option<&Cursor>,
    ) -> Result<Page<Decision>> {
        self.seq_page(
            team,
            "decisions",
            limit,
            cursor,
            |r| {
                Ok(Decision {
                    seq: r.get(0)?,
                    node: r.get(1)?,
                    generation: r.get(2)?,
                    actor: sql_json(r, 3)?,
                    kind: r.get(4)?,
                    text: r.get(5)?,
                })
            },
            "seq,node,generation,actor,kind,text",
        )
    }
    fn seq_page<T>(
        &self,
        team: &str,
        table: &str,
        limit: u32,
        cursor: Option<&Cursor>,
        read: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
        columns: &str,
    ) -> Result<Page<T>> {
        self.active()?;
        let c = start(
            self.db.as_ref(),
            format!("{team}:{table}"),
            table,
            team,
            limit,
            cursor,
        )?;
        let mut rows = Vec::new();
        if let Some(db) = &self.db {
            let sql = if table == "journal" {
                format!("SELECT {columns} FROM journal WHERE team=?1 AND seq>?2 AND seq<=?3 UNION ALL SELECT {columns} FROM journal_archive WHERE team=?1 AND seq>?2 AND seq<=?3 ORDER BY seq LIMIT ?4")
            } else {
                format!("SELECT {columns} FROM {table} WHERE team=?1 AND seq>?2 AND seq<=?3 ORDER BY seq LIMIT ?4")
            };
            let mut s = db.prepare(&sql).map_err(storage)?;
            // seq is read separately so the generic row itself need not expose it.
            rows = s
                .query_map(params![team, c.after, c.ceiling, limit], {
                    let mut read = read;
                    move |r| Ok((r.get::<_, u64>(0)?, read(r)?))
                })
                .map_err(storage)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(storage)?;
        }
        let page = finish(rows, c, limit, |r| (r.0, String::new()));
        Ok(Page {
            rows: page.rows.into_iter().map(|(_, r)| r).collect(),
            next: page.next,
        })
    }
}
