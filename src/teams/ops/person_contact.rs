//! Person messages for the lead, and the two receipts the person can open.
use super::*;
use store::person_contact as contacts;

fn admit(ctx: &Context<'_>, pattern: &str, body: &str) -> Result<()> {
    planning::ordinary_card(ctx, None)?;
    zerus_team_plan::lead_message(body)?;
    if contacts::sent_since(ctx.db, &ctx.team.id, ctx.now.saturating_sub(29_999))? {
        return Err(over("Wait 30 seconds between messages to the lead."));
    }
    if contacts::open_count(
        ctx.db,
        &ctx.team.id,
        pattern,
        ctx.team.settings.caps.max_open_questions,
    )? >= ctx.team.settings.caps.max_open_questions
    {
        return Err(over("Too many open messages of this kind for the lead."));
    }
    Ok(())
}

pub(super) fn open_note(
    ctx: &mut Context<'_>,
    text: &str,
    node: Option<u32>,
    decisions: Option<&[String]>,
    parameters: Option<&[(String, String)]>,
) -> Result<()> {
    if node.is_some() || decisions != Some(&[]) || parameters.is_some() {
        return Err(invalid("A lead note takes only its text."));
    }
    admit(ctx, "person_note_link:*", text)?;
    let card = talk::card(ctx, contacts::NOTE, text, None, None, &[], &[])?;
    // A pointer in the existing lead inbox drives its guarded, content-free
    // reminder. Only the card holds the note's body.
    add_inbox(
        ctx,
        &format!("person_note_link:{card}"),
        &format!("Read note card #{card} with hgs team cards."),
    )?;
    ctx.reply.item = Some(card);
    ctx.event = Some("person_left_lead_note");
    Ok(())
}

fn add_inbox(ctx: &Context<'_>, kind: &str, body: &str) -> Result<u64> {
    ctx.db.execute(
        "INSERT INTO inbox(team,kind,node,generation,actor,text,created_ms) VALUES(?1,?2,0,0,?3,?4,?5)",
        params![ctx.team.id, kind, encode(ctx.actor)?, body, ctx.now],
    ).map_err(storage)?;
    Ok(ctx.db.last_insert_rowid() as u64)
}

pub(super) fn ask(ctx: &mut Context<'_>, body: &str) -> Result<()> {
    admit(ctx, "person_question:*", body)?;
    let next: u64 = ctx
        .db
        .query_row("SELECT COALESCE(MAX(seq),0)+1 FROM inbox", [], |row| {
            row.get(0)
        })
        .map_err(storage)?;
    ctx.reply.item = Some(add_inbox(ctx, &format!("person_question:{next}"), body)?);
    Ok(())
}

pub(super) fn reply(ctx: &mut Context<'_>, item: u64, body: &str) -> Result<()> {
    planning::ordinary_card(ctx, None)?;
    zerus_team_plan::lead_message(body)?;
    let changed = ctx
        .db
        .execute(
            "UPDATE inbox SET reply=?3,reply_actor=?4,state='closed'
         WHERE team=?1 AND seq=?2 AND node=0 AND generation=0
         AND state='open' AND kind GLOB 'person_question:*'
         AND json_extract(actor,'$.kind')='person' AND json_extract(actor,'$.person')=?5",
            params![ctx.team.id, item, body, encode(ctx.actor)?, ctx.team.owner],
        )
        .map_err(storage)?;
    if changed != 1 {
        return Err(invalid(
            "The person's question is unavailable or already closed.",
        ));
    }
    ctx.reply.item = Some(talk::card(
        ctx,
        contacts::REPLY_NOTICE,
        body,
        None,
        None,
        &[],
        &[],
    )?);
    Ok(())
}

pub(super) fn acknowledge(ctx: &mut Context<'_>, card: u64) -> Result<()> {
    planning::ordinary_card(ctx, None)?;
    let note: Option<String> = ctx
        .db
        .query_row(
            "UPDATE cards SET state='closed',decision='acknowledged',decided_actor=?3
         WHERE team=?1 AND seq=?2 AND state='open' AND kind='person_note_to_lead'
         AND node IS NULL AND json_extract(actor,'$.kind')='person'
         AND json_extract(actor,'$.person')=?4 RETURNING text",
            params![ctx.team.id, card, encode(ctx.actor)?, ctx.team.owner],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage)?;
    let note =
        note.ok_or_else(|| invalid("The person's note is unavailable or already closed."))?;
    if ctx
        .db
        .execute(
            "UPDATE inbox SET state='closed' WHERE team=?1 AND node=0 AND generation=0
         AND kind=?2 AND state='open' AND json_extract(actor,'$.kind')='person'
         AND json_extract(actor,'$.person')=?3",
            params![
                ctx.team.id,
                format!("person_note_link:{card}"),
                ctx.team.owner
            ],
        )
        .map_err(storage)?
        != 1
    {
        return Err(invalid("The note's lead inbox entry is unavailable."));
    }
    ctx.reply.item = Some(talk::card(
        ctx,
        contacts::ACK_NOTICE,
        &note,
        None,
        None,
        &[],
        &[],
    )?);
    Ok(())
}

pub(super) fn read(ctx: &Context<'_>, card: u64) -> Result<()> {
    if ctx
        .db
        .execute(
            "UPDATE cards SET state='closed',decided_actor=?3
         WHERE team=?1 AND seq=?2 AND state='open' AND node IS NULL
         AND kind IN ('lead_reply_to_person','lead_note_ack') AND decisions='[]'
         AND json_extract(actor,'$.kind')='session' AND json_extract(actor,'$.team')=?1",
            params![ctx.team.id, card, encode(ctx.actor)?],
        )
        .map_err(storage)?
        != 1
    {
        return Err(invalid("The lead's notice is unavailable or already read."));
    }
    Ok(())
}

pub(super) fn close_pending(ctx: &Context<'_>) -> Result<()> {
    ctx.db.execute(
        "UPDATE inbox SET state='closed' WHERE team=?1 AND state='open' AND node=0 AND generation=0
         AND (kind GLOB 'person_question:*' OR kind GLOB 'person_note_link:*')",
        [&ctx.team.id],
    ).map_err(storage)?;
    ctx.db.execute(
        "UPDATE cards SET state='closed' WHERE team=?1 AND state='open' AND kind='person_note_to_lead'",
        [&ctx.team.id],
    ).map_err(storage)?;
    Ok(())
}
