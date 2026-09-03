//! The standup protocol: a note per date, published to Confluence (issue
//! #289, spec #272 stories 64-69).
//!
//! `CONTEXT.md`'s **standup protocol**: *"a note -- one per date, opened by
//! its own address -- holding attendees, per-person notes and action items.
//! Publish creates a Confluence page from it under a configured parent and
//! links note and page; the note stays the editable original. Not a kind of
//! its own."*
//!
//! Every sentence of that is load-bearing here, and the last one most of all:
//! there is **no protocol table, no protocol kind and no protocol id space**.
//! A protocol is a `knobas.note` row like any other, which is what makes it
//! searchable, exportable and linkable for free -- and what means this module
//! owns no storage. It owns three rules over storage that already exists.
//!
//! # The three rules
//!
//! 1. **One note per date, found by its title.** [`title_of`] is the whole of
//!    the identity: opening the standup for a date twice must land in the same
//!    note, not in a second one with the same name. [`get_or_create`] reads
//!    before it writes.
//! 2. **One publication per date, found by the page's title.** The page is
//!    titled with the date ([`page_title_of`]), and *that title is what
//!    identifies the publication* -- see the section below, which is the
//!    ruling this ticket was asked to make.
//! 3. **The link is the record.** "The page's entity id is recorded on the
//!    note" and "a knobas link is drawn between them" are one act, not two:
//!    the link row *is* the page id recorded against the note, so the two
//!    cannot disagree and no migration is owed. Spec #272's schema paragraph
//!    lists migrations for the timer, the blocks and the worklog and none for
//!    this, and the reason is that a `knobas.link` row already says exactly
//!    what would have gone in one.
//!
//! # Publishing twice for one date -- the ruling
//!
//! Delivery is **at-least-once** (ADR-0012) and, unlike `UpdatePage`, a
//! re-sent `CreatePage` carries no version for the server to check. A daily
//! standup page is exactly the shape that collides: same parent, same title,
//! every morning. Nothing before this exercised a second create with one title
//! in one space, so the answer is stated here rather than left to be
//! discovered.
//!
//! **Three layers, and the first is knobas'.**
//!
//! * **knobas refuses to queue a second one.** [`publication_of`] asks whether
//!   a `create_page` write for this date's page title already exists in a
//!   state that is not [`WriteState::Refused`] or [`WriteState::Discarded`],
//!   and [`publish`] answers with that publication instead of composing a new
//!   op. So *Publish* pressed twice, or pressed again next to a write that is
//!   still waiting for an offline Confluence, queues one write and points at
//!   it. A **refused** or **discarded** write is the one case a fresh publish
//!   is right: nothing landed, and the reader has been told so.
//! * **Confluence refuses the redelivery knobas cannot see.** A `POST` whose
//!   response was lost is a page that exists with knobas none the wiser, and
//!   the queue will send it again. Confluence Data Center requires a page
//!   title to be **unique within its space**, so the second delivery comes
//!   back a refusal in Confluence's own words (ADR-0004) rather than a second
//!   page. That is a fact about the product and therefore checked against the
//!   real product: `atlassian_live.rs`'s
//!   `a_second_page_with_one_title_in_one_space_is_refused` is the witness,
//!   because a mock asserting it would be an assumption checked against itself
//!   (ADR-0013).
//! * **Drawing the link twice is a no-op.** `create_link_inner` answers
//!   `Conflict` for a pair that already carries the relation, which
//!   [`draw_link`] reads as *already drawn* -- the same reading
//!   `start_work::queue`'s `Linked::Already` makes, and for the same reason: a
//!   step whose job is that the relationship exists has succeeded.
//!
//! What is **not** claimed: that a page can never be created twice. Only an
//! idempotency key could promise that and the SPI has none (#122 is open). The
//! claim is that knobas never *asks* twice, and that the one ask it cannot see
//! is refused by the server rather than duplicated.
//!
//! # Where the page id comes from after a restart
//!
//! Not from the receipt. `WriteReceipt` lives for the length of one flush; the
//! settle writes what it carried into `knobas.write_queue.remote_id`
//! (`knobas_core::write_queue::sent`), and that column is the durable handle.
//! So [`reconcile`] -- which every read of the protocol runs, not only the one
//! that pressed the button -- reads the queue row, not a receipt, and can
//! therefore draw the link for a write that settled while the app was shut, or
//! while the reader was looking at something else.
//!
//! It needs one more thing the settle cannot give it: the page has to be in
//! the mirror before a link can point at it, because `knobas.link`'s endpoints
//! are `knobas.entity` rows. `sources::write_queue::submit` already re-syncs a
//! source whose write landed, so the ordinary path has the page by the time
//! [`publish`] returns; a write that settled without that -- a scheduler tick,
//! a relaunch -- gets its link at the next read instead. [`reconcile`] is
//! therefore written to be run any number of times and to be right when the
//! mirror is still behind: no page row, no link, no error, and it tries again
//! next time.

use chrono::NaiveDate;
use knobas_core::entity::EntityRef;
use knobas_core::note::{self, NoteRow};
use knobas_core::write_queue::WriteState;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::{IpcError, IpcErrorCode};

/// The `knobas.setting` key holding where protocols are published.
///
/// `knobas.setting`, the key/value store migration `0002` exists for, so this
/// needs no migration of its own -- the reasoning `backup::SCHEDULE_KEY` and
/// `time::passive::SETTING_KEY` both record. Spec #272's sub-milestone map
/// names this key as one of M3.3's two frozen-surface touches.
pub const PUBLISH_TARGET_KEY: &str = "standup.publish_target";

/// The relation a published protocol carries.
///
/// A *key*, lower case, the shape `note::REF_RELATION` and the link dialog's
/// vocabulary both take -- `app/src/lib/detail/relations.ts` is where it reads
/// as a sentence, "published as" from the note and "published from" from the
/// page. That is what makes story 66 -- *either detail shows the other* --
/// true from both ends: one row, read undirected by `link::entries_of`, worded
/// per end by the frontend's table.
pub const PUBLISHED_RELATION: &str = "published-as";

/// The relation a ticket filed from an action item carries.
///
/// The vocabulary's default. Nothing in the curated list says "came out of a
/// meeting", and inventing a word for it would put language on screen that
/// `relations.ts` has no inverse for -- an unknown relation reads the same
/// from both ends, which is worse than the honest general one.
pub const ACTION_ITEM_RELATION: &str = "related";

/// The op identifier a publication is queued under.
const CREATE_PAGE: &str = "create_page";

/// Who knobas records as the author of a note it made on the reader's behalf.
///
/// The same actor `commands::entity` writes for everything a person did
/// through the interface.
const ACTOR: &str = "user";

/// Where protocols are published: which Confluence, and under which page.
///
/// Both halves, because neither is enough. The source id is what routes the
/// write (interfaces §4.1 makes it the entity namespace), and with two
/// Confluence sources configured a parent id alone would name a page in an
/// instance nobody chose -- story 68 in one struct field.
///
/// `parent` is a **mirrored page entity id** (`confluence:98400`), not a raw
/// content id and not a space: `WriteOp::CreatePage` says so, and it is what a
/// reader picks out of the mirror. The space key the op also needs is *not*
/// stored here -- it is read off the parent's own record at publish time, so
/// the pair can never disagree about which space the parent is in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishTarget {
    /// The Confluence source's instance id.
    pub source_id: String,
    /// The parent page, as an entity id.
    pub parent: String,
}

/// What became of a date's publication, as far as knobas can tell.
#[derive(Clone, Debug, Serialize)]
pub struct Publication {
    /// The write queue row that carries it.
    pub write_id: i64,
    /// The queue's own word for where the write stands.
    pub state: WriteState,
    /// The source's sentence, when it had one. Untrusted source text.
    pub detail: Option<String>,
    /// The page, as an entity id, once the source has named it.
    ///
    /// `None` while the write is still in flight -- and *not* the same fact as
    /// [`linked`](Self::linked) being false, which additionally means the
    /// mirror has not caught up.
    pub page_entity_id: Option<String>,
    /// Whether note and page are linked yet.
    pub linked: bool,
}

/// A date's protocol: the note, and what became of publishing it.
#[derive(Clone, Debug, Serialize)]
pub struct Protocol {
    /// The date this protocol is for, as the reader's own calendar spells it.
    pub day: NaiveDate,
    /// The note's entity id.
    pub note_id: String,
    /// The title the published page carries -- what a second publish for this
    /// date would collide with, shown so the reader knows what they are about
    /// to put in the wiki.
    pub page_title: String,
    /// The publication, when there has been one.
    pub publication: Option<Publication>,
}

/// What a date's protocol note is called.
///
/// `Standup 2026-09-03`. The word is there because this title is what a reader
/// meets in the launcher and in a link chip, where a bare date is a note about
/// nothing; the date is there because it is the identity [`get_or_create`]
/// matches on.
#[must_use]
pub fn title_of(day: NaiveDate) -> String {
    format!("Standup {day}")
}

/// What the published **page** is called.
///
/// The date and nothing else, which is story 65's own wording -- *a page under
/// a configured parent titled with the date*. Under a parent called *Standup
/// protocols* the word would be noise, and the wiki's own page tree is where a
/// reader reads it.
///
/// It is also the publication's **identity**: see the module header's ruling.
#[must_use]
pub fn page_title_of(day: NaiveDate) -> String {
    day.to_string()
}

/// The body a protocol starts life with.
///
/// Three headings, in the order a standup runs, and each with an empty bullet
/// under it so the first keystroke goes somewhere. `CONTEXT.md` names exactly
/// these three -- attendees, per-person notes, action items -- and this is the
/// only place that list is spelled out.
///
/// The action items are **task list items**, `- [ ]`, because that is the
/// syntax [`action_items`] reads and the one a person types without being
/// told. Nothing here is required to survive: it is a note, and a reader who
/// deletes a heading has deleted a heading.
#[must_use]
pub fn template_of(day: NaiveDate) -> String {
    format!(
        "# Standup {day}\n\n\
         ## Attendees\n\n\
         - \n\n\
         ## Notes\n\n\
         - \n\n\
         ## Action items\n\n\
         - [ ] \n"
    )
}

/// The date's protocol note, made if it is not there yet.
///
/// **Read before write, and the read is by title.** A second call for one date
/// must answer the note the first call made -- opening the standup view twice
/// is the ordinary way this happens, and a view that made a fresh note on
/// every open would leave a week of empty duplicates behind and lose the one
/// the reader typed into.
///
/// Deleted notes are skipped (`knobas.note` loses its row on delete and keeps
/// its tombstoned entity), so a protocol the reader deleted can be started
/// again for the same date rather than resurrecting a body they threw away --
/// the same direction `note::save` takes for a stale editor.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read or the write fails.
pub async fn get_or_create(pool: &PgPool, day: NaiveDate) -> Result<NoteRow, IpcError> {
    let title = title_of(day);
    // Ordered by `created_at`, so a database that somehow holds two answers
    // with the oldest -- the one links have been drawn to and the one a
    // publication names. Newest-first would make a duplicate the winner.
    let existing = sqlx::query_as::<_, NoteRow>(
        "select id, title, body_md, created_at, updated_at
           from knobas.note
          where title = $1
          order by created_at, id
          limit 1",
    )
    .bind(&title)
    .fetch_optional(pool)
    .await
    .map_err(IpcError::internal)?;
    if let Some(note) = existing {
        return Ok(note);
    }
    Ok(note::create(pool, &title, &template_of(day), ACTOR).await?)
}

/// One action item a protocol's body names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ActionItem {
    /// The words after the bullet and the checkbox, trimmed.
    pub text: String,
    /// Whether the box is ticked.
    pub done: bool,
}

/// The action items in a protocol body.
///
/// Everything bulleted under the **Action items** heading, until the next
/// heading of any level. A `- [ ]` or `- [x]` prefix is read as a checkbox and
/// stripped; a plain `- ` bullet is an action item that has no box, which is
/// what somebody who deleted the template's brackets meant.
///
/// A bullet with no words is not an item: the template ships one empty bullet
/// on purpose, and offering *Create ticket* on it would be a button that files
/// a ticket called nothing.
///
/// The heading is matched **case-insensitively on its text**, so `## Action
/// Items` and `### action items` both count -- a person editing a note is not
/// keeping a parser in mind. Nothing else about the body is interpreted: this
/// is a reader over one section, not a markdown implementation.
#[must_use]
pub fn action_items(body_md: &str) -> Vec<ActionItem> {
    const HEADING: &str = "action items";
    let mut inside = false;
    let mut out = Vec::new();
    for line in body_md.lines() {
        let trimmed = line.trim();
        if let Some(text) = trimmed.strip_prefix('#') {
            let text = text.trim_start_matches('#').trim();
            inside = text.eq_ignore_ascii_case(HEADING);
            continue;
        }
        if !inside {
            continue;
        }
        let Some(rest) = bullet(trimmed) else {
            continue;
        };
        let (done, text) = checkbox(rest);
        if text.is_empty() {
            continue;
        }
        out.push(ActionItem {
            text: text.to_owned(),
            done,
        });
    }
    out
}

/// What follows a list bullet, or `None` for a line that is not one.
fn bullet(line: &str) -> Option<&str> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return Some(rest.trim());
        }
    }
    // A bullet with nothing after it -- the template's own empty item.
    if matches!(line, "-" | "*" | "+") {
        return Some("");
    }
    None
}

/// A leading `[ ]` / `[x]`, read and removed.
fn checkbox(rest: &str) -> (bool, &str) {
    for (marker, done) in [("[ ]", false), ("[x]", true), ("[X]", true)] {
        if let Some(text) = rest.strip_prefix(marker) {
            return (done, text.trim());
        }
    }
    (false, rest)
}

/// Markdown in, Confluence **storage format** out.
///
/// The third implementation of "our text, their dialect", and the reason it is
/// a third rather than a reuse is the input: `storage::from_text` and
/// `page-sections.ts`'s `toStorage` both take *plain text*, because a comment
/// and a section edit are plain text. A note is **markdown**, and a page whose
/// body read `## Attendees` and `- [ ] ship it` as literal prose would be a
/// publication nobody on the team would accept.
///
/// What is translated is exactly what [`template_of`] writes and what a person
/// types into it:
///
/// * `#` to `######` become `<h1>`…`<h6>`;
/// * `-`, `*` and `+` bullets become one `<ul>` per run of them, with `[ ]` and
///   `[x]` rendered as `☐` and `☑` -- Confluence's own task list is an `ac:`
///   macro, and knobas does not write macros it cannot read back;
/// * every other non-blank run of lines becomes a `<p>`, with single newlines
///   inside it as `<br/>`;
/// * blank lines separate blocks and produce nothing of their own.
///
/// **Everything else is text.** Inline emphasis, links and code are *not*
/// interpreted: `**bold**` reaches the page as the characters `**bold**`. That
/// is the stated failure direction -- markup knobas does not translate arrives
/// visibly untranslated rather than half-translated -- and it is the safe one,
/// because the alternative is a partial markdown implementation whose gaps
/// nobody can predict from the outside.
///
/// `&`, `<` and `>` are escaped, `&` **first**, the rule both twins state:
/// escaping `<` first would then escape the `&` of the `&lt;` it just wrote.
/// A body that is only whitespace produces the empty string.
#[must_use]
pub fn to_storage(body_md: &str) -> String {
    let mut out = String::with_capacity(body_md.len() + body_md.len() / 2);
    let mut paragraph: Vec<&str> = Vec::new();
    let mut list: Vec<String> = Vec::new();

    for line in body_md.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush_paragraph(&mut out, &mut paragraph);
            flush_list(&mut out, &mut list);
            continue;
        }
        if let Some((level, text)) = heading(trimmed) {
            flush_paragraph(&mut out, &mut paragraph);
            flush_list(&mut out, &mut list);
            out.push_str(&format!("<h{level}>{}</h{level}>", escape(text)));
            continue;
        }
        if let Some(item) = bullet(trimmed) {
            flush_paragraph(&mut out, &mut paragraph);
            list.push(list_item(item));
            continue;
        }
        flush_list(&mut out, &mut list);
        paragraph.push(trimmed);
    }
    flush_paragraph(&mut out, &mut paragraph);
    flush_list(&mut out, &mut list);
    out
}

/// A heading's level and its words, or `None`.
///
/// Capped at six, because there is no `<h7>`: `#######` is prose that starts
/// with hashes, and is left as prose.
fn heading(line: &str) -> Option<(usize, &str)> {
    let hashes = line.len() - line.trim_start_matches('#').len();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = line[hashes..].strip_prefix(' ')?;
    Some((hashes, rest.trim()))
}

/// One `<li>`, with a task box rendered as a character.
fn list_item(item: &str) -> String {
    let (done, text) = checkbox(item);
    let box_ = if item.len() == text.len() {
        // No checkbox was there at all -- an ordinary bullet.
        String::new()
    } else if done {
        "\u{2611} ".to_owned()
    } else {
        "\u{2610} ".to_owned()
    };
    format!("<li>{box_}{}</li>", escape(text))
}

fn flush_paragraph(out: &mut String, lines: &mut Vec<&str>) {
    if lines.is_empty() {
        return;
    }
    let joined = lines
        .iter()
        .map(|line| escape(line))
        .collect::<Vec<_>>()
        .join("<br/>");
    out.push_str(&format!("<p>{joined}</p>"));
    lines.clear();
}

fn flush_list(out: &mut String, items: &mut Vec<String>) {
    if items.is_empty() {
        return;
    }
    out.push_str("<ul>");
    for item in items.iter() {
        out.push_str(item);
    }
    out.push_str("</ul>");
    items.clear();
}

/// `&`, `<` and `>`, in that order.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The publication for a date, if there is one.
///
/// **Found by the page's title**, which is the module header's ruling put into
/// a statement: a `create_page` write whose `title` is this date's page title
/// is this date's publication, whatever the reader has since done to the
/// publish target. Matching the parent too would strand a write queued under
/// yesterday's target and let a second one be composed beside it, which is
/// exactly the duplicate this rule exists to prevent.
///
/// A **refused** or **discarded** write is not a publication: the first is the
/// source saying no in its own words and the second is the reader withdrawing
/// it, and in both cases nothing landed and a fresh publish is right. Every
/// other state counts, `pending` and `held` included -- an offline Confluence
/// owes this page, and a second press must not queue a second one.
///
/// The newest matching row wins, so a date published again after a refusal
/// reports the attempt that is live rather than the one that failed.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn publication_of(
    pool: &PgPool,
    day: NaiveDate,
) -> Result<Option<Publication>, IpcError> {
    let row: Option<(i64, String, WriteState, Option<String>, Option<String>)> = sqlx::query_as(
        "select id, source_id, state, detail, remote_id
           from knobas.write_queue
          where op = $1
            and payload->>'title' = $2
            and state <> 'refused'
            and state <> 'discarded'
          order by id desc
          limit 1",
    )
    .bind(CREATE_PAGE)
    .bind(page_title_of(day))
    .fetch_optional(pool)
    .await
    .map_err(IpcError::internal)?;

    let Some((write_id, source_id, state, detail, remote_id)) = row else {
        return Ok(None);
    };
    // The id the source gave the page, in knobas' address space. §4.1 makes
    // the instance id and the entity namespace one string, so the source id on
    // the queue row is the namespace -- read from the row rather than from the
    // setting, because the setting may have moved since the write was queued.
    let page_entity_id = remote_id.map(|id| EntityRef::new(&source_id, &id).to_string());
    Ok(Some(Publication {
        write_id,
        state,
        detail,
        page_entity_id,
        linked: false,
    }))
}

/// Draw the link a settled publication owes, if it can be drawn yet.
///
/// Idempotent and total: every read of a protocol runs it, and it is right at
/// each of the three moments it can be run at.
///
/// * the write has not settled -- nothing to link to, and it says so;
/// * the write settled but the page is not in the mirror yet -- **no link and
///   no error**, because `knobas.link`'s endpoints are `knobas.entity` rows
///   and the page has one only once a sync has seen it. The next read tries
///   again;
/// * the page is there -- the link is drawn, or was already, and both are
///   *linked*.
///
/// This is the whole of the after-a-restart path: the id comes from
/// `knobas.write_queue.remote_id`, which the settle wrote and which outlives
/// every process, and not from the `WriteReceipt`, which does not.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if a read or the link write
/// fails.
pub async fn reconcile(
    pool: &PgPool,
    note_id: &str,
    mut publication: Publication,
) -> Result<Publication, IpcError> {
    let Some(page) = publication.page_entity_id.clone() else {
        return Ok(publication);
    };
    if !mirrored(pool, &page).await? {
        return Ok(publication);
    }
    draw_link(pool, note_id, &page, PUBLISHED_RELATION).await?;
    publication.linked = true;
    Ok(publication)
}

/// Whether `knobas.entity` holds this id -- the precondition a link has.
///
/// `knobas.entity` rather than `sync.live_item`, deliberately: a page the
/// source has since withdrawn still has its address, and a link to it stays
/// visible and marked, which is the reading `link::entries_of` and
/// `note::refs_of` both take.
async fn mirrored(pool: &PgPool, entity_id: &str) -> Result<bool, IpcError> {
    let found: Option<String> = sqlx::query_scalar("select id from knobas.entity where id = $1")
        .bind(entity_id)
        .fetch_optional(pool)
        .await
        .map_err(IpcError::internal)?;
    Ok(found.is_some())
}

/// Draw a link, treating one that is already there as drawn.
///
/// `start_work::queue::Queue::link`'s reading, for its reason: a caller whose
/// job is that the relationship *exists* has succeeded when it exists, and a
/// `Conflict` is the pair-unique index saying so.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails for any
/// other reason.
pub async fn draw_link(
    pool: &PgPool,
    from: &str,
    to: &str,
    relation: &str,
) -> Result<(), IpcError> {
    match crate::commands::entity::create_link_inner(pool, from, to, Some(relation), None).await {
        Ok(_) => Ok(()),
        Err(error) if error.code == IpcErrorCode::Conflict => Ok(()),
        Err(error) => Err(error),
    }
}

/// Where protocols are published, or `None` until somebody has said.
///
/// A stored value that no longer decodes reads as `None` -- the same discipline
/// `backup::read_setting` and `time::passive::enabled` record, resolved the
/// safe way for this key: a target knobas cannot read is a target it must ask
/// for again, never one it guesses at and publishes into.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn publish_target(pool: &PgPool) -> Result<Option<PublishTarget>, IpcError> {
    let stored: Option<serde_json::Value> =
        sqlx::query_scalar("select value from knobas.setting where key = $1")
            .bind(PUBLISH_TARGET_KEY)
            .fetch_optional(pool)
            .await
            .map_err(IpcError::internal)?;
    Ok(stored.and_then(|value| serde_json::from_value(value).ok()))
}

/// Record where protocols are published.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if the parent is not an entity id
/// or the source id is blank -- a target that could never route a write is a
/// bad request, not something to store and discover at publish time.
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn set_publish_target(
    pool: &PgPool,
    target: &PublishTarget,
) -> Result<PublishTarget, IpcError> {
    if target.source_id.trim().is_empty() {
        return Err(IpcError::invalid(
            "a publish target needs the Confluence source to publish into",
        ));
    }
    let parent = EntityRef::parse(&target.parent).map_err(IpcError::invalid)?;
    // §4.1: the instance id *is* the namespace, so a parent in another
    // source's address space would queue the write at a source that cannot see
    // the page. One answer, so it is checked rather than trusted.
    if parent.namespace != target.source_id {
        return Err(IpcError::invalid(format!(
            "the parent page {} is not in {}'s address space",
            target.parent, target.source_id
        )));
    }
    let value = serde_json::to_value(target).map_err(IpcError::internal)?;
    sqlx::query(
        "insert into knobas.setting (key, value, updated_at) values ($1, $2, now())
         on conflict (key) do update set value = excluded.value, updated_at = now()",
    )
    .bind(PUBLISH_TARGET_KEY)
    .bind(value)
    .execute(pool)
    .await
    .map_err(IpcError::internal)?;
    Ok(target.clone())
}

/// The space key a parent page sits in, read off the parent's own record.
///
/// `WriteOp::CreatePage` needs both the parent and the space key and they must
/// agree; storing the key beside the parent in the setting would be two
/// answers to one question, and the wrong one would be a page that landed
/// somewhere nobody chose. So it is read here, from the mirror, at the moment
/// the op is composed.
///
/// A **payload read outside an adapter**, so ADR-0007's interim discipline
/// applies: one named read, gated on nothing but the record being a Confluence
/// page's, missing to a refusal rather than to a guess. #277's `KindPaths` has
/// a project slot and this is that fact -- a space is Confluence's project
/// (ADR-0010) -- but the declared project read answers for an *item*, and what
/// is wanted here is the key of the space the item is in; when a slot for it
/// exists this read expires into it.
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) when the parent is not in the
/// mirror -- a target the reader configured and the source has since removed
/// -- and [`Invalid`](crate::IpcErrorCode::Invalid) when its record names no
/// space, which is a page record Confluence does not produce and which nothing
/// downstream could recover from.
pub async fn space_of(pool: &PgPool, parent: &str) -> Result<String, IpcError> {
    let payload: Option<serde_json::Value> =
        sqlx::query_scalar("select payload from sync.live_item where entity_id = $1")
            .bind(parent)
            .fetch_optional(pool)
            .await
            .map_err(IpcError::internal)?;
    let Some(payload) = payload else {
        return Err(IpcError::not_found(format!(
            "the parent page {parent} is not in the mirror, so knobas cannot tell which space to \
             publish into. Pick the parent again in settings."
        )));
    };
    payload
        .get("space")
        .and_then(|space| space.get("key"))
        .and_then(serde_json::Value::as_str)
        .filter(|key| !key.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            IpcError::invalid(format!(
                "the record for {parent} does not say which space it is in, and a page cannot be \
                 created without one"
            ))
        })
}

/// The `WriteOp::CreatePage` payload for a protocol.
///
/// Composed here rather than in the command for the reason
/// `start_work::plan` composes its own: the op is the *decision* -- which
/// parent, which space, which title, which dialect the body is in -- and a
/// decision that only exists inside a `#[tauri::command]` is one no test can
/// reach.
///
/// Serialized rather than typed, the shape `submit_write` takes and for #42's
/// ratified reason: `WriteOp` grows per milestone (ADR-0006) and naming the
/// enum here would put the SPI on this module's surface.
#[must_use]
pub fn create_page_payload(
    target: &PublishTarget,
    space: &str,
    day: NaiveDate,
    body_md: &str,
) -> serde_json::Value {
    serde_json::json!({
        "CreatePage": {
            "parent": target.parent,
            "space": space,
            "title": page_title_of(day),
            "body": to_storage(body_md),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 3).unwrap()
    }

    /// The two titles, and the difference between them, which is the whole of
    /// the identity story: the note is named for a person reading a list, the
    /// page is named for story 65's own sentence.
    #[test]
    fn a_protocol_is_named_for_its_date_and_its_page_is_the_date() {
        assert_eq!(title_of(day()), "Standup 2026-09-03");
        assert_eq!(page_title_of(day()), "2026-09-03");
    }

    /// The template carries the three sections `CONTEXT.md` names, and its
    /// action item is in the syntax [`action_items`] reads -- otherwise the
    /// first thing a reader types would be invisible to *Create ticket*.
    #[test]
    fn the_template_carries_the_three_sections_and_a_readable_action_item() {
        let body = template_of(day());
        assert!(body.contains("## Attendees"), "{body}");
        assert!(body.contains("## Notes"), "{body}");
        assert!(body.contains("## Action items"), "{body}");
        assert_eq!(action_items(&body), Vec::new(), "an empty bullet is no item");
        let typed = body.replace("- [ ] \n", "- [ ] Ask Ines about the SEPA retry\n");
        assert_eq!(
            action_items(&typed),
            [ActionItem {
                text: "Ask Ines about the SEPA retry".to_owned(),
                done: false,
            }]
        );
    }

    /// What is and is not an action item. The negative half is the load-bearing
    /// one: a bullet under *Notes* is somebody's note, and filing a ticket from
    /// it would be knobas acting on a sentence nobody marked.
    #[test]
    fn action_items_are_the_bullets_under_their_own_heading() {
        let body = "\
# Standup 2026-09-03

## Notes

- Jonas is on the payout retry
- [ ] this looks like an action item and is not one

## Action items

- [ ] Ask Ines about the SEPA retry
- [x] Book the postmortem
* Star bullets count
-
- [ ]

## Attendees

- Mara
";
        assert_eq!(
            action_items(body),
            [
                ActionItem {
                    text: "Ask Ines about the SEPA retry".to_owned(),
                    done: false
                },
                ActionItem {
                    text: "Book the postmortem".to_owned(),
                    done: true
                },
                ActionItem {
                    text: "Star bullets count".to_owned(),
                    done: false
                },
            ],
            "only the bullets under the action-items heading, and none of the empty ones"
        );
        // The heading ends the section, so *Attendees* is not action items.
        assert!(!action_items(body).iter().any(|item| item.text == "Mara"));
    }

    /// A heading is a heading however it is capitalised: a person editing a
    /// note is not keeping a parser in mind.
    #[test]
    fn the_action_items_heading_is_matched_case_insensitively() {
        assert_eq!(
            action_items("### action ITEMS\n- [ ] ship it\n"),
            [ActionItem {
                text: "ship it".to_owned(),
                done: false
            }]
        );
    }

    /// The markdown vocabulary the template writes, and what a body of it
    /// becomes on the wire.
    #[test]
    fn a_protocol_body_becomes_the_storage_format_a_page_is_stored_in() {
        let storage = to_storage(
            "# Standup 2026-09-03\n\n## Attendees\n\n- Mara\n- Jonas\n\n## Action items\n\n\
             - [ ] Ask Ines\n- [x] Book it\n",
        );
        assert_eq!(
            storage,
            "<h1>Standup 2026-09-03</h1><h2>Attendees</h2><ul><li>Mara</li><li>Jonas</li></ul>\
             <h2>Action items</h2><ul><li>\u{2610} Ask Ines</li><li>\u{2611} Book it</li></ul>"
        );
    }

    /// Prose is paragraphs, a single newline inside one is a break, and a run
    /// of bullets is one list rather than one list each.
    #[test]
    fn prose_is_paragraphs_and_a_run_of_bullets_is_one_list() {
        assert_eq!(
            to_storage("one\ntwo\n\nthree\n"),
            "<p>one<br/>two</p><p>three</p>"
        );
        assert_eq!(to_storage("- a\n- b\n"), "<ul><li>a</li><li>b</li></ul>");
        assert_eq!(
            to_storage("- a\n\n- b\n"),
            "<ul><li>a</li></ul><ul><li>b</li></ul>",
            "a blank line ends the list, because it ends the block"
        );
        assert_eq!(to_storage("   \n\n  \n"), "", "whitespace is not a page");
    }

    /// Somebody's prose is never markup. `&` first, or the `&lt;` this writes
    /// would be escaped by the pass that follows it.
    #[test]
    fn a_bracket_in_somebodys_words_reaches_the_wiki_as_a_bracket() {
        assert_eq!(
            to_storage("a < b && <script>alert(1)</script>\n"),
            "<p>a &lt; b &amp;&amp; &lt;script&gt;alert(1)&lt;/script&gt;</p>"
        );
        assert_eq!(
            to_storage("## a & <b>\n"),
            "<h2>a &amp; &lt;b&gt;</h2>",
            "a heading's words are words too"
        );
    }

    /// What this deliberately does **not** translate, pinned so that a later
    /// reader meets the decision rather than the bug: inline markup arrives
    /// visibly untranslated, never half-translated.
    #[test]
    fn inline_markdown_is_left_as_the_characters_somebody_typed() {
        assert_eq!(to_storage("**bold**\n"), "<p>**bold**</p>");
        assert_eq!(
            to_storage("####### seven hashes\n"),
            "<p>####### seven hashes</p>",
            "there is no h7, so it is prose that starts with hashes"
        );
    }
}
