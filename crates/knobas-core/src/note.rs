//! Notes: the first thing knobas **owns** rather than mirrors.
//!
//! A note is a markdown document knobas holds outright -- `CONTEXT.md`: *"a
//! knobas-owned markdown document with `[[refs]]`; a first-class searchable
//! kind, not an annotation on something else"*. It is an [entity], so it has
//! one stable address, it is linkable from both ends, and it is in the same id
//! space as everything the mirror holds.
//!
//! Two tables, one row each, always written together:
//!
//! * `knobas.entity` -- the address, the kind and the title. This is what a
//!   link chip, a room tile and a search row read, and what survives a delete.
//! * `knobas.note` -- the body, and the `fts` generated from title and body.
//!   This is the corpus the launcher searches (`knobas_search::corpus::NOTE`).
//!
//! Migration `0006` is what makes the pair a pair: `note_entity_fk` means a
//! note cannot exist without its entity, and `note_id_ns_chk` means its id is
//! in the `note:` namespace -- which no source may ever write into
//! (`RESERVED_NAMESPACES`, and `0006`'s `item_entity_reserved_chk`).
//!
//! # `[[refs]]` are links, and the body is the only thing that decides them
//!
//! A `[[ref]]` in the body is a row in `knobas.link` like any other -- one link
//! table, so the panel #53 built draws a note's references and an entity's
//! backlinks with the same code and no second concept. Their [`Origin`] is
//! `implied`: knobas drew them as a consequence of what the user typed, which
//! is what tells them apart from a link drawn by hand and from a machine
//! suggestion.
//!
//! The body is the source of truth and the links are derived from it, so the
//! two cannot disagree: [`save`] reconciles the whole set on every write --
//! adding a ref creates the link, removing it withdraws the link -- and a ref
//! that resolves to nothing creates none at all. What the reconciliation
//! touches is scoped to `(this note, relation [`REF_RELATION`], origin
//! `implied`)`, so a link the user drew *by hand* out of a note is not governed
//! by the note's text and does not vanish when the text changes.
//!
//! ## Two notes naming each other share one row
//!
//! Since #70 one *pair* carries one active link per relation whichever way round
//! it was drawn, so `A` saying `[[B]]` and `B` saying `[[A]]` cannot each have a
//! row of their own -- they are one edge, and the panels drew it as one already,
//! `entries_of` being undirected. What that costs is a rule the reconciliation
//! has to state: the row belongs to whichever note wrote it, and when *that*
//! note drops its ref while the other still names it, the row is **handed over**
//! rather than withdrawn ([`reconcile_refs`]). Withdrawing it would leave the
//! second note's body saying `[[A]]` with no link behind it until that note
//! happened to be saved again, which is exactly the disagreement this section
//! promises cannot happen.
//!
//! Handing over is display-neutral: [`refs_of`] reads the *body*, and backlinks
//! read the pair undirected, so no panel changes. Only the reconciliation cares
//! which end owns the row.
//!
//! # A birth and a death are lines; an edit is not
//!
//! [`create`] writes one `created` line and [`delete`] one `deleted` line, each
//! inside the transaction it describes, and nothing else in this module writes
//! any (#524). The asymmetry is #409's, and it is about how often the thing
//! happens rather than about how important it is: `knobas.activity` is
//! append-only, so a line the editor's 700 ms autosave writes can never be
//! collapsed afterwards, and an afternoon on one note would be forty lines
//! about one thought. A birth and a death happen once each, so the flood
//! argument does not reach them.
//!
//! The birth line is also what pairs the log's two halves. A link a note is
//! born with is `manual`, so the panel lets a reader withdraw one and
//! `commands::entity`'s `unlink_inner` writes an `unlinked` line for it; the
//! `created` line names every withdrawable born link by the id that `unlinked`
//! line carries ([`DrawnLink`], whose doc says which links those are and why
//! that is all of them), so the reader who withdraws one is looking at both
//! ends of the same story.
//!
//! [entity]: crate::entity

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::CoreError;
use crate::activity;
use crate::entity::EntityRef;
use crate::link::{LinkEnd, Origin};

/// The relation a `[[ref]]` link carries.
///
/// One relation for every ref, and a *key* rather than prose (the same rule
/// `commands::entity::relation_of` folds to): the panel groups by this value
/// and the reconciliation below matches on it verbatim, so a second spelling
/// would be a second group and an unreconcilable set.
pub const REF_RELATION: &str = "references";

/// What a note with no title is called.
///
/// Everything that draws a note draws its *entity's* title -- a search row, a
/// link chip, a room tile -- so "" would be a note nobody can point at in a
/// list. The blank title is not preserved anywhere: it is a note the user has
/// not named yet, not a note named nothing.
pub const UNTITLED: &str = "Untitled note";

/// Longest `[[…]]` this will read as a reference.
///
/// An entity id is `namespace:key` and a key can be long
/// (`confluence:ENG:SEPA design`), so the cap is generous. It exists for the
/// unclosed `[[` -- without it, one stray bracket makes the rest of the
/// document a single reference.
const MAX_REF_CHARS: usize = 512;

/// One note.
#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct NoteRow {
    /// `note:<uuid>`, and also the id of this note's `knobas.entity` row.
    pub id: String,
    /// The note's name. Never blank -- see [`UNTITLED`].
    pub title: String,
    /// Markdown the user typed. **Untrusted text**: render it as text, never as
    /// markup (roadmap §4 gotcha 7). It is the user's own here rather than a
    /// source's, which changes who is at fault and nothing else.
    pub body_md: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One `[[ref]]` the body names, and what it points at -- if anything.
#[derive(Clone, Debug, Serialize)]
pub struct NoteRef {
    /// The text between the brackets, exactly as the body wrote it.
    ///
    /// Not necessarily a well-formed entity id: a typo is a ref that resolves
    /// to nothing, and showing it back is how the user finds the typo (#46
    /// story 10).
    pub target_id: String,
    /// The entity it names, when one exists.
    ///
    /// [`None`] is an **unresolved** ref: it is visible as unresolved and it
    /// creates no link. `Some` with `deleted_at` set is a ref whose target was
    /// withdrawn upstream -- still a ref, still a link, and marked (story 9).
    pub target: Option<LinkEnd>,
}

/// Every `[[…]]` the body names, in first-appearance order, without repeats.
///
/// The text between the brackets is returned **verbatim after trimming** and
/// nothing here decides whether it addresses anything: resolution is a database
/// question and lives in [`refs_of`] and in [`save`]'s reconciliation. A ref
/// that is empty, that spans a line break, or that is longer than
/// [`MAX_REF_CHARS`] is not read as a reference at all -- each of those is a
/// stray bracket rather than something a person meant to point at.
#[must_use]
pub fn parse_refs(body_md: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = body_md;
    while let Some(open) = rest.find("[[") {
        rest = &rest[open + 2..];
        let Some(close) = rest.find("]]") else {
            break;
        };
        let inner = &rest[..close];
        rest = &rest[close + 2..];
        let named = inner.trim();
        if named.is_empty()
            || named.contains('\n')
            || named.chars().count() > MAX_REF_CHARS
            || out.iter().any(|seen| seen == named)
        {
            continue;
        }
        out.push(named.to_owned());
    }
    out
}

/// One link a note is **born with**: what it points at, and the word it
/// carries.
///
/// The note is always the `from` end. A capture says *this thought was
/// captured in that context* and *captured from that ticket* (`CONTEXT.md`,
/// **Capture**), and both sentences run outwards from the note, which is what
/// makes `captured in` and `captured here` the two readings of one row.
///
/// Deliberately not a `[[ref]]`: a ref is *the body's own list* and is
/// reconciled against the text on every save ([`reconcile_refs`]), so a link
/// the body does not name would be withdrawn by the first autosave. These are
/// ordinary links of origin [`Origin::Manual`], governed by nothing but the
/// reader, and [`crate::link::unlink`] withdraws one exactly as it withdraws a
/// link drawn from the panel by hand.
#[derive(Clone, Debug)]
pub struct BornLink {
    /// The other end.
    pub target: EntityRef,
    /// The stored relation, already folded to the one spelling that groups it.
    pub relation: String,
}

/// One link a note really **was** born with, in the three words the activity
/// log uses for a link.
///
/// The same three keys `commands::entity`'s `link_detail` writes on a `linked`
/// or an `unlinked` line, and deliberately so: they are what lets a reader of
/// the note's history pair the birth with a later withdrawal. A born link is
/// [`Origin::Manual`], so the links panel does let a reader withdraw one, and
/// until #524 that wrote an `unlinked` line whose partner never existed.
///
/// **What was drawn, not what was asked for.** [`draw_born_links`] takes these
/// out of the insert's own `returning`, so a target no `knobas.entity` row
/// carries contributes nothing here -- there is no link, so there is no id a
/// withdrawal could ever name. A caller naming the same target twice under one
/// relation likewise contributes one, because one is what the `on conflict`
/// drew.
///
/// The third such case, and the reason that is the right rule rather than a
/// shortfall: a born link under [`REF_RELATION`] whose target the body *also*
/// names is drawn by [`reconcile_refs`] a statement earlier, so the born insert
/// conflicts and this list omits it. The row that exists is then `implied`, and
/// `implied` is the one origin the links panel refuses to withdraw -- so it can
/// never write the `unlinked` line this list exists to give a partner to. Every
/// link that *can* be withdrawn from a new note is in here.
#[derive(Clone, Debug, Serialize)]
struct DrawnLink {
    /// `knobas.link.id` -- the id an `unlinked` line will carry.
    link_id: Uuid,
    /// The other end. The note is always the `from` end, so this is the half
    /// the reader does not already know.
    to_id: String,
    /// The stored relation, as it was folded.
    relation: String,
}

/// Write a new note, the links its body already names, and the links it is
/// **born with**.
///
/// The id is knobas': a `note:<uuid>` nobody has to choose and nobody can
/// collide with. Both rows, every ref and every [`BornLink`] land in one
/// transaction, so a note is never half-written and its links never describe a
/// body that was rolled back. That atomicity is the whole of #502's title: a
/// note that existed for a moment without its `captured-in` link would be a
/// note that was, for that moment, in no context.
///
/// **A target with no `knobas.entity` row draws no link, and the note is
/// written anyway** -- expressed as the same `select ... from knobas.entity`
/// [`reconcile_refs`] uses, though the precedent is `commands::time`'s
/// `heartbeat` rather than that one: *"losing the attribution is honest,
/// losing the observation is not"*. The note is the observation and a born
/// link is the attribution, and refusing the note would make *New note* a
/// button that stays broken while the reader can do nothing about it. Nothing
/// is hidden by this: the note's links panel draws the links that exist, and
/// there is no field claiming otherwise (`CONTEXT.md`, **Capture**: *"Never a
/// field on the note"*).
///
/// **Which case that is, exactly**, because the obvious guess is wrong: an id
/// *no row ever carried*. An entity a source dropped still **has** its row --
/// a purge tombstones and never deletes (`knobas_sync::config`'s
/// `PURGE_ITEMS` is an `update ... set deleted_at`; `CONTEXT.md`'s **Purge**
/// carries *delete* on its *Avoid* list for this reason), and a context is
/// never deleted at all -- so a born link to one **is** drawn, and the panel
/// shows it marked ([`crate::link::LinkEnd`]'s `deleted_at`). What reaches this
/// clause is a caller handing an id it did not read from a row, or one
/// remembered across a database that changed under it. Ruled by the deputy on
/// 2026-09-08 (#502); ADR-0011 is why the earlier wording here was corrected
/// rather than left standing.
///
/// # One line, for the birth
///
/// The last thing in the transaction is a single `created` line on the note's
/// own entity, actor the author, detail `{ title, born: [{ link_id, to_id,
/// relation }] }` (#524). One line and not three: a birth is **one event**, so
/// it does not reach the argument #409 refused a line per note edit with --
/// `knobas.activity` is append-only and a 700 ms-debounced autosave would
/// flood it -- and announcing each born link separately would say a note's
/// links were worth a line while the note's own birth stayed silent.
///
/// `born` carries [`DrawnLink`]s, which is what makes the line the partner a
/// withdrawal needs: the same three keys `link_detail` writes, so an
/// `unlinked` line's `link_id` is one of these. It is **`[]` and never
/// absent** when nothing was drawn -- a key that is missing reads as a line
/// written before this existed, and a reader of the log can act on the
/// difference.
///
/// Inside the transaction, after the links, for both of the obvious reasons:
/// the ids it names do not exist until the inserts have run, and a line
/// describing a note that rolled back would be a birth the log records and the
/// database never had.
///
/// [`save`] writes no line at all, and neither does [`reconcile_refs`] -- see
/// [`save`].
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn create(
    pool: &PgPool,
    title: &str,
    body_md: &str,
    born_with: &[BornLink],
    author: &str,
) -> Result<NoteRow, CoreError> {
    let entity = EntityRef::new("note", &Uuid::new_v4().to_string());
    let id = entity.to_string();
    let mut tx = pool.begin().await?;

    sqlx::query(
        "insert into knobas.entity (id, kind, title, updated_at) values ($1, 'note', $2, now())",
    )
    .bind(&id)
    .bind(named(title))
    .execute(&mut *tx)
    .await?;

    let row = sqlx::query_as::<_, NoteRow>(
        "insert into knobas.note (id, title, body_md) values ($1, $2, $3)
         returning id, title, body_md, created_at, updated_at",
    )
    .bind(&id)
    .bind(named(title))
    .bind(body_md)
    .fetch_one(&mut *tx)
    .await?;

    reconcile_refs(&mut tx, &id, body_md, author).await?;
    let born = draw_born_links(&mut tx, &id, born_with, author).await?;
    activity::record_with(
        &mut *tx,
        author,
        "created",
        Some(&entity),
        serde_json::json!({ "title": row.title, "born": born }),
    )
    .await?;
    tx.commit().await?;
    Ok(row)
}

/// Draw the links a note is born with, inside the transaction that wrote it.
///
/// One statement per link rather than one over an array: there are two of them
/// at most, they carry *different relations*, and an `unnest` of two parallel
/// arrays would trade a legible statement for a round trip nobody is counting.
///
/// `on conflict do nothing` because the pair's uniqueness is the database's
/// (`link_pair_active_idx`) and a caller naming the same target twice under one
/// relation is asking for one link, not for a failure. `e.id <> $1` is the
/// self-link guard [`reconcile_refs`] carries; no caller can reach it today,
/// since the id was minted three statements ago and nobody else has seen it,
/// and it is written all the same so that the two inserts in this module state
/// the same rule rather than one of them relying on its caller.
///
/// **No line here either, and this one is not a gap.** The links a note is born
/// with are announced by the note's own `created` line, which [`create`] writes
/// once for the birth and names every one of them in -- that is what pairs a
/// later `unlinked` with something, and announcing each link *here* would say a
/// note's links were events while the note itself was not (#524, closing the
/// gap this comment used to name).
///
/// Which is why the drawn rows come back rather than being dropped: the
/// `returning` is the only place the ids exist, and the line is composed from
/// them. `fetch_optional`, because both of this statement's silences are real
/// ones -- a target no row carries matches nothing in the `select`, and a
/// conflict inserts nothing -- and in both the honest answer is that no link
/// was drawn for that input.
async fn draw_born_links(
    tx: &mut Transaction<'_, Postgres>,
    note_id: &str,
    born_with: &[BornLink],
    author: &str,
) -> Result<Vec<DrawnLink>, CoreError> {
    let mut drawn = Vec::with_capacity(born_with.len());
    for born in born_with {
        let row: Option<(Uuid, String, String)> = sqlx::query_as(
            "insert into knobas.link (from_id, to_id, relation, origin, created_by)
             select $1, e.id, $2, $3, $4
               from knobas.entity e
              where e.id = $5 and e.id <> $1
             on conflict do nothing
             returning id, to_id, relation",
        )
        .bind(note_id)
        .bind(&born.relation)
        .bind(Origin::Manual.as_str())
        .bind(author)
        .bind(born.target.to_string())
        .fetch_optional(&mut **tx)
        .await?;
        if let Some((link_id, to_id, relation)) = row {
            drawn.push(DrawnLink {
                link_id,
                to_id,
                relation,
            });
        }
    }
    Ok(drawn)
}

/// Rewrite a note's title and body, and bring its `[[refs]]` back into step.
///
/// This is what an autosave calls, so it is one round trip and it is idempotent
/// -- saving the same body twice writes the same links and withdraws nothing.
///
/// `Ok(None)` when no note carries `id`: it was deleted, or it never existed.
/// Neither is an error and neither writes a note -- a save is an edit of
/// something that is there, and resurrecting a deleted note from a stale editor
/// would be the opposite of what *delete* meant.
///
/// **No activity line, and none per `[[ref]]`** -- Björn's ruling of
/// 2026-09-05 (#409), which [`create`]'s birth line does not reopen.
/// `knobas.activity` is append-only, the editor's autosave calls this every
/// 700 ms of pause, and a line per save could not be collapsed after the fact;
/// `crate::time::worklog` reads `knobas.note.updated_at` instead, which one
/// afternoon overwrites rather than accumulates. `reconcile_refs` is silent for
/// the same reason twice over: it runs on every one of those saves.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn save(
    pool: &PgPool,
    id: &EntityRef,
    title: &str,
    body_md: &str,
    author: &str,
) -> Result<Option<NoteRow>, CoreError> {
    let id = id.to_string();
    let mut tx = pool.begin().await?;

    let row = sqlx::query_as::<_, NoteRow>(
        "update knobas.note set title = $2, body_md = $3, updated_at = now()
          where id = $1
         returning id, title, body_md, created_at, updated_at",
    )
    .bind(&id)
    .bind(named(title))
    .bind(body_md)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        // Nothing to roll back, but the transaction is closed explicitly for
        // the reason `knobas_sync::run_inner` closes its own: dropping one
        // queues the rollback rather than sending it.
        tx.rollback().await?;
        return Ok(None);
    };

    // The entity carries the name everything else draws, so a rename that
    // stopped here would rename the note everywhere except where it is read.
    sqlx::query("update knobas.entity set title = $2, updated_at = now() where id = $1")
        .bind(&id)
        .bind(named(title))
        .execute(&mut *tx)
        .await?;

    reconcile_refs(&mut tx, &id, body_md, author).await?;
    tx.commit().await?;
    Ok(Some(row))
}

/// One note, or nothing.
///
/// # Errors
///
/// [`CoreError::Db`] if the read fails.
pub async fn get(pool: &PgPool, id: &EntityRef) -> Result<Option<NoteRow>, CoreError> {
    let row = sqlx::query_as::<_, NoteRow>(
        "select id, title, body_md, created_at, updated_at from knobas.note where id = $1",
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Delete a note: the body goes, the address stays, tombstoned.
///
/// The asymmetry is deliberate and is the same one a withdrawn mirrored item
/// gets (§5a, and #53's hydration). What the user asked to delete is what they
/// wrote, and that is really deleted -- there is no source to re-fetch it from,
/// which is why notes are in the backup. The `knobas.entity` row survives so
/// that a link somebody drew *to* this note stays visible and marked instead of
/// dangling, and so the id can never be handed out again.
///
/// The note's own `[[ref]]` links are withdrawn with the body they were derived
/// from. Links drawn by hand out of the note are not: they are nobody's
/// derivation, and #53's panel is what shows them, marked.
///
/// `Ok(false)` when there was no note to delete -- idempotent, like
/// [`crate::link::unlink`], and a call that mutated nothing announces nothing.
///
/// # One line, for the death
///
/// A `deleted` line on the note's own entity, inside this transaction, detail
/// `{ title }` (#524). It is the other half of [`create`]'s birth line: one
/// event, and a history panel that showed a birth and no death would be the
/// orphan in the other direction. The address survives the body -- that is
/// what the paragraph above is about -- so the line is named on something a
/// reader can still open.
///
/// The title comes out of the `delete`'s own `returning`, which is the last
/// moment it exists to be read. It is also what answers "was there a note",
/// so this is one statement rather than a read and a write that could disagree.
///
/// `actor` and not `author`, which is [`create`]'s word: `create` binds it to
/// `knobas.link.created_by` as well as to the line, and a note's author is a
/// fact about the note. Here it reaches nothing but
/// [`crate::activity::ActivityRow::actor`], and the person who deletes a note
/// is not the person who wrote it.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn delete(pool: &PgPool, id: &EntityRef, actor: &str) -> Result<bool, CoreError> {
    let note_id = id.to_string();
    let mut tx = pool.begin().await?;

    let deleted: Option<(String,)> =
        sqlx::query_as("delete from knobas.note where id = $1 returning title")
            .bind(&note_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((title,)) = deleted else {
        tx.rollback().await?;
        return Ok(false);
    };

    // No body, so no refs: the empty set is the whole reconciliation.
    withdraw_refs_other_than(&mut tx, &note_id, &[]).await?;

    // `deleted_at is null` keeps the *first* deletion's timestamp, exactly as
    // the sweep and `ENTITY_UPSERT` do.
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1 and deleted_at is null")
        .bind(&note_id)
        .execute(&mut *tx)
        .await?;

    activity::record_with(
        &mut *tx,
        actor,
        "deleted",
        Some(id),
        serde_json::json!({ "title": title }),
    )
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// Every `[[ref]]` this note's body names, resolved where it resolves.
///
/// In body order, so the panel that lists them reads in the order the note
/// does. A ref whose target has no `knobas.entity` row comes back with
/// [`NoteRef::target`] as `None` -- unresolved, and visible as such.
///
/// Resolved against `knobas.entity` and **not** `sync.live_item`, for the
/// reason [`crate::link::entries_of`] gives: a ref to something the source
/// withdrew must stay visible and marked rather than becoming unresolved. Those
/// are different facts and the user can act on only one of them.
///
/// # Errors
///
/// [`CoreError::Db`] if the read fails.
pub async fn refs_of(pool: &PgPool, id: &EntityRef) -> Result<Vec<NoteRef>, CoreError> {
    let Some(note) = get(pool, id).await? else {
        return Ok(Vec::new());
    };
    let named = parse_refs(&note.body_md);
    if named.is_empty() {
        return Ok(Vec::new());
    }

    let ends = sqlx::query_as::<_, LinkEnd>(
        "select e.id as entity_id, e.kind, e.title, e.deleted_at
           from knobas.entity e
          where e.id = any($1)",
    )
    .bind(&named)
    .fetch_all(pool)
    .await?;

    Ok(named
        .into_iter()
        .map(|target_id| NoteRef {
            target: ends.iter().find(|end| end.entity_id == target_id).cloned(),
            target_id,
        })
        .collect())
}

/// A title that is a name, not an absence.
fn named(title: &str) -> &str {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        UNTITLED
    } else {
        trimmed
    }
}

/// Make the note's `implied` links say exactly what its body says.
///
/// Three steps, in this order, all scoped to
/// `(from_id, REF_RELATION, origin implied)`:
///
/// 0. hand over every ref link whose target the body no longer names **but
///    whose target is a note that names this one** -- see the module header;
/// 1. withdraw every ref link whose target the body no longer names;
/// 2. draw a link for every named target that has an entity row.
///
/// Step 2 is what resolves: the join to `knobas.entity` is the only thing that
/// decides whether a ref is a link, so an unresolved ref -- a typo, or
/// something that has not synced -- creates nothing, silently and correctly.
/// It cannot become a phantom link later either: the next save reconciles
/// again, and if the target has appeared by then the link appears with it.
///
/// Written here rather than through [`crate::link::create`] because this is a
/// *set* reconciliation and has to be atomic with the body it is derived from:
/// one statement per ref would be one round trip per ref on an autosave, and
/// the two halves could not share the transaction the body is written in. The
/// rows are ordinary link rows -- one link table, whatever wrote them.
///
/// The `on conflict` does nothing where an **active** link for the same pair and
/// relation already exists, which is the partial unique index
/// `link_pair_active_idx` read from the other side. That covers the same body
/// saved twice, and it covers a link the user happened to draw by hand between
/// the same pair under the same relation: the row that is there stays, with the
/// origin it was made with.
///
/// **Unarbitrated since #70**, and it has to be. The index is now on
/// `(least(from_id, to_id), greatest(from_id, to_id), relation)`, so a conflict
/// target naming the three columns infers no index at all and the statement
/// fails outright; naming the expression instead would put the normalisation in
/// two places, and the one here is the copy that would go stale. Nothing else on
/// this table can raise a conflict for `do nothing` to swallow -- the endpoints
/// come out of `knobas.entity` in the `select` itself, so neither foreign key
/// can be the fault, and a foreign-key violation is not a conflict `do nothing`
/// covers in any case.
///
/// A ref to an entity that already links *back* to this note under
/// `REF_RELATION` is skipped rather than written: one active edge per unordered
/// pair per relation is the rule the migration states, and two rows for "these
/// two reference each other" is the duplicate #40's story 14 exists to prevent.
/// [`hand_over_refs_the_other_note_still_names`] is what keeps that from costing
/// the second note its link when the first drops its ref.
async fn reconcile_refs(
    tx: &mut Transaction<'_, Postgres>,
    note_id: &str,
    body_md: &str,
    author: &str,
) -> Result<(), CoreError> {
    let named = parse_refs(body_md);
    hand_over_refs_the_other_note_still_names(tx, note_id, &named).await?;
    withdraw_refs_other_than(tx, note_id, &named).await?;

    sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, created_by)
         select $1, e.id, $2, $3, $4
           from knobas.entity e
          where e.id = any($5) and e.id <> $1
         on conflict do nothing",
    )
    .bind(note_id)
    .bind(REF_RELATION)
    .bind(Origin::Implied.as_str())
    .bind(author)
    .bind(&named)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Re-point, rather than withdraw, the ref links the *other* note still names.
///
/// Runs before [`withdraw_refs_other_than`], which then does not see them: what
/// this leaves behind is a row whose `from_id` is the note that still justifies
/// it. See the module header for why one row is all a mutually-referencing pair
/// gets since #70.
///
/// Scoped to targets that are **notes**, because a note is the only thing whose
/// body can name anything back. The bodies are parsed here rather than matched
/// in SQL: [`parse_refs`] is what decides what a ref *is* -- the length limit,
/// the newline rule, the de-duplication -- and a `like` pattern beside it would
/// be a second, looser answer to the same question.
///
/// The swap is one statement per handed-over row, and there is at most one per
/// dropped ref. `set from_id = to_id, to_id = from_id` is a genuine swap:
/// PostgreSQL evaluates every right-hand side against the row as it stood.
///
/// Reads `knobas.confirmed_link`, not the base table (#161): a ref link is
/// written by [`reconcile_refs`] with no `confirmed_at`, which the column's
/// default makes *now*, so every row this could hand over is a confirmed one --
/// and a machine *proposal* between two notes is the suggestion tray's to answer,
/// never a side effect of saving a body. The view's own predicate is also where
/// the `deleted_at is null` clause went.
async fn hand_over_refs_the_other_note_still_names(
    tx: &mut Transaction<'_, Postgres>,
    note_id: &str,
    keep: &[String],
) -> Result<(), CoreError> {
    let candidates: Vec<(Uuid, String)> = sqlx::query_as(
        "select l.id, n.body_md
           from knobas.confirmed_link l
           join knobas.note n on n.id = l.to_id
          where l.from_id = $1
            and l.relation = $2
            and l.origin = $3
            and not (l.to_id = any($4))",
    )
    .bind(note_id)
    .bind(REF_RELATION)
    .bind(Origin::Implied.as_str())
    .bind(keep)
    .fetch_all(&mut **tx)
    .await?;

    for (id, body_md) in candidates {
        if !parse_refs(&body_md).iter().any(|named| named == note_id) {
            continue;
        }
        sqlx::query("update knobas.link set from_id = to_id, to_id = from_id where id = $1")
            .bind(id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// Withdraw the note's ref links whose targets are not in `keep`.
///
/// Scoped to `origin = 'implied'` so that a link the user drew by hand out of
/// this note is not governed by the note's text: the body is the source of
/// truth for what the body derived, and for nothing else.
async fn withdraw_refs_other_than(
    tx: &mut Transaction<'_, Postgres>,
    note_id: &str,
    keep: &[String],
) -> Result<(), CoreError> {
    sqlx::query(
        "update knobas.link set deleted_at = now()
          where from_id = $1
            and relation = $2
            and origin = $3
            and deleted_at is null
            and not (to_id = any($4))",
    )
    .bind(note_id)
    .bind(REF_RELATION)
    .bind(Origin::Implied.as_str())
    .bind(keep)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a person means by `[[…]]`, and what they do not.
    ///
    /// The expectations are literal rather than computed: a body walked the way
    /// the parser walks it would agree with the parser by construction.
    #[test]
    fn a_body_names_the_things_between_its_double_brackets() {
        assert_eq!(
            parse_refs("see [[jira:PAY-231]] and [[gitea:tidewater/payout#142]]"),
            ["jira:PAY-231", "gitea:tidewater/payout#142"]
        );
        // Trimmed, because a person typing `[[ PAY-231 ]]` means the ticket.
        assert_eq!(parse_refs("[[  jira:PAY-231  ]]"), ["jira:PAY-231"]);
        // A key may itself contain ':' and '#' -- `EntityRef` splits on the
        // first ':' only.
        assert_eq!(
            parse_refs("[[confluence:ENG:SEPA design]]"),
            ["confluence:ENG:SEPA design"]
        );
        // Named twice is one reference: the set is what becomes links.
        assert_eq!(parse_refs("[[a:b]] then [[a:b]] again"), ["a:b"]);
        // First-appearance order, so the panel reads in the order the note does.
        assert_eq!(parse_refs("[[b:2]] [[a:1]]"), ["b:2", "a:1"]);
    }

    /// Every one of these is a stray bracket rather than a reference, and
    /// reading any of them as one would put something in the panel that the
    /// writer cannot see the cause of.
    #[test]
    fn a_stray_bracket_is_not_a_reference() {
        assert!(parse_refs("").is_empty());
        assert!(parse_refs("no brackets at all").is_empty());
        assert!(parse_refs("[[]]").is_empty());
        assert!(parse_refs("[[   ]]").is_empty());
        // Unclosed: the rest of the document is not one enormous reference.
        assert!(parse_refs("an unclosed [[jira:PAY-231").is_empty());
        // A line break inside is the same mistake seen from further away.
        assert!(parse_refs("[[jira:PAY-231\nand more]]").is_empty());
        // Single brackets are markdown's own, not ours.
        assert!(parse_refs("[a link](https://example.invalid)").is_empty());
        // Longer than any id anybody addresses.
        let huge = format!("[[{}]]", "x".repeat(MAX_REF_CHARS + 1));
        assert!(parse_refs(&huge).is_empty());
        assert_eq!(
            parse_refs(&format!("[[{}]]", "x".repeat(MAX_REF_CHARS))).len(),
            1,
            "exactly at the cap is still a reference"
        );
    }

    /// One unclosed bracket does not swallow the references after it.
    #[test]
    fn an_unclosed_bracket_ends_the_scan_where_it_starts() {
        // `[[a:1]]` closes; `[[b` opens and never closes, so `c:3` -- which is
        // inside no brackets at all -- is not read as one.
        assert_eq!(parse_refs("[[a:1]] then [[b and c:3"), ["a:1"]);
        // But a `]]` that arrives late still closes what opened, which is the
        // honest reading of what was typed.
        assert_eq!(parse_refs("[[a:1]] then [[b:2 ]]"), ["a:1", "b:2"]);
    }

    #[test]
    fn a_blank_title_becomes_a_name() {
        assert_eq!(named(""), UNTITLED);
        assert_eq!(named("   \n "), UNTITLED);
        assert_eq!(named("  Standup  "), "Standup");
    }
}
