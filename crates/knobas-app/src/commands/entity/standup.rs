//! The standup digest and the standup protocol.

use super::*;

// -- the standup digest (#288) ----------------------------------------------
//
// Here for the reason the two sections above give, and the reason spec #272
// gives in as many words: §10.8 freezes the `commands/` + `ipc/` layout, and
// "standup and Confluence reads go into the entity module as usual" -- the
// `time` module pair was the one ratified exception and this is not a second
// one. What the read decides lives in `crate::standup`; this seam adds the
// pool, the identity and the declarations, the same three things
// `inbox_items_inner` adds, and nothing else.

/// The standup digest for one day (issue #288, spec #272 stories 58-63).
///
/// Three lists -- yesterday, today, blockers -- every line carrying the item
/// it came from, which source said so and which verb it was. `CONTEXT.md`'s
/// **digest**; [`crate::standup`] holds the rules and the reasoning.
///
/// **The webview computes the days**, each as a date and the two instants it
/// spans: `today` is the day being asked about and `earlier` the days before
/// it, oldest first. The rule is [`day_blocks`](crate::commands::time::day_blocks)'s
/// in full -- the machine's timezone is a fact only that side holds, and a UTC
/// offset would be the wrong shape as well as the wrong owner for a day
/// containing a daylight-saving change. How far back the *yesterday* rule may
/// reach is **not** the webview's to say: at most
/// [`standup::LOOKBACK_DAYS`](crate::standup::LOOKBACK_DAYS) of `earlier` are
/// consulted however many are sent.
///
/// **The registry is the injected one**, the shape [`inbox_items`] has and for
/// the same reason: the blockers list is read through the descriptors'
/// declared paths (#277), and a test that could not hand over a descriptor
/// could only witness the declarations the shipped adapters happen to carry --
/// which is a battery that passes just as well against a hardcoded list of
/// English status words. That is the one mistake this read must not be able to
/// make.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up,
/// [`Internal`](crate::IpcErrorCode::Internal) for a read failure.
#[tauri::command]
pub async fn standup_digest(
    lifecycle: State<'_, Lifecycle>,
    today: crate::time::week::DayWindow,
    earlier: Vec<crate::time::week::DayWindow>,
) -> Result<crate::standup::StandupDigest, IpcError> {
    let pool = lifecycle.pool()?;
    standup_digest_inner(&pool, lifecycle.registry(), Utc::now(), today, &earlier).await
}

/// [`standup_digest`] with the pool, the registry and the clock handed in, so
/// a test can reach it -- the split every read in this module has.
///
/// # Errors
///
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a read
/// failure or a source listing that fails.
pub async fn standup_digest_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    now: DateTime<Utc>,
    today: crate::time::week::DayWindow,
    earlier: &[crate::time::week::DayWindow],
) -> Result<crate::standup::StandupDigest, IpcError> {
    let identity = identity_of(pool).await?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    crate::standup::digest(pool, &identity, &declarations, now, today, earlier).await
}

// -- the standup protocol (#289) ---------------------------------------------
//
// Here for the reason the digest above it is here, and the same sentence of
// spec #272: §10.8 freezes the `commands/` + `ipc/` layout, "standup and
// Confluence reads go into the entity module as usual", and the `time` module
// pair was the one ratified exception. What these decide lives in
// `crate::protocol`; this seam adds the pool, the sources state and the
// announcement, and nothing else.

/// The protocol for a date, made if it is not there yet (issue #289, spec
/// #272 stories 64-67).
///
/// **Get-or-create, and this is the read the standup view opens with**, so it
/// is also where a publication settled in the reader's absence catches up:
/// [`protocol::reconcile`] runs on every read, and it is the whole of the path
/// by which a write that landed after a restart comes to have its link. The
/// page id it needs comes from `knobas.write_queue.remote_id`, which the
/// settle wrote and which outlives the `WriteReceipt` that carried it.
///
/// The **date is the webview's**, exactly as it is for the digest and the day
/// review: which day it is where the reader sits is a fact only that side
/// holds.
///
/// # Errors
///
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a read or
/// write failure.
pub async fn standup_protocol_inner(
    pool: &PgPool,
    day: chrono::NaiveDate,
) -> Result<crate::protocol::Protocol, IpcError> {
    let note = crate::protocol::get_or_create(pool, day).await?;
    // A publication is per date **and per Confluence**, so the read needs to
    // know which one -- and the stored target is the only thing that says.
    // With none stored nothing has been published from here, which is what
    // `None` says: `publish` refuses before it queues anything when the target
    // is missing, so there is no publication for a read to have missed.
    let publication = match crate::protocol::publish_target(pool).await? {
        None => None,
        Some(target) => {
            match crate::protocol::publication_of(pool, day, &target.source_id).await? {
                Some(publication) => {
                    Some(crate::protocol::reconcile(pool, &note.id, publication).await?)
                }
                None => None,
            }
        }
    };
    Ok(crate::protocol::Protocol {
        day,
        note_id: note.id,
        page_title: crate::protocol::page_title_of(day),
        publication,
    })
}

/// [`standup_protocol_inner`] behind the lifecycle.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up; otherwise as [`standup_protocol_inner`].
#[tauri::command]
pub async fn standup_protocol(
    lifecycle: State<'_, Lifecycle>,
    day: chrono::NaiveDate,
) -> Result<crate::protocol::Protocol, IpcError> {
    standup_protocol_inner(&lifecycle.pool()?, day).await
}

/// Publish a date's protocol to Confluence (stories 65-68).
///
/// `target` is the answer to the dialog the **first** publish shows: given, it
/// is recorded as the setting and used; omitted, the stored one is used and a
/// missing one is a refusal rather than a guess. That is both directions of
/// story 67 in one command -- *asked for the first time, and changeable in
/// settings* -- with `set_standup_publish_target` as the other door onto the
/// same key.
///
/// **Which source is the caller's to decide, not this command's.** With two
/// Confluence sources configured there is no defensible default and story 68
/// says so in as many words, so the dialog asks and sends the answer here. A
/// command that picked one would be the mistake the story exists to prevent.
///
/// **A second publish for one date queues nothing.** See
/// `crate::protocol`'s ruling: the publication already on the queue is
/// answered with, so *Publish* pressed twice is one page. The one case that
/// composes a fresh op is a previous write that was refused or discarded,
/// where nothing landed.
///
/// The write goes through `sources::write_queue::submit` like every other, so
/// it queues when Confluence is down and it shows in the pending-writes panel.
/// `submit` also re-syncs the source when the write lands, which is what puts
/// the new page in the mirror in time for the link this call then draws --
/// `knobas.link`'s endpoints are `knobas.entity` rows, so the link cannot
/// precede the sync.
///
/// The publication is **re-read from the queue** rather than taken from what
/// `submit` handed back: the row is the durable record, the read is the same
/// one every later read makes, and taking the two answers from one place is
/// what stops them disagreeing.
///
/// # Errors
///
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) when no publish
/// target is stored and none was given, or when the one given is not a
/// coherent target; [`NotFound`](crate::IpcErrorCode::NotFound) when the
/// configured parent page is not in the mirror; otherwise whatever the write
/// queue refuses with.
pub async fn publish_standup_protocol_inner(
    state: &crate::sources::SourcesState,
    day: chrono::NaiveDate,
    target: Option<crate::protocol::PublishTarget>,
) -> Result<crate::protocol::Protocol, IpcError> {
    let pool = &state.pool;
    let target = target_for(pool, target).await?;
    if let Some(payload) = page_to_make(pool, day, &target).await? {
        crate::sources::write_queue::submit(state, payload).await?;
        // Wait for the re-read `submit` set off. The link this call is about
        // to draw points at a page that has an address only once a sync has
        // seen it, so triggering and hoping would leave every ordinary publish
        // unlinked until the reader happened to open the view again.
        crate::sources::write_queue::sync_after_write(state, &target.source_id).await;
    }
    standup_protocol_inner(pool, day).await
}

/// [`publish_standup_protocol_inner`], **answering once the page's write is
/// queued** (#779): what the `publish_standup_protocol` command does.
///
/// Every refusal the awaited path makes is made here too, before the answer:
/// a missing or incoherent target, a parent the mirror does not hold. The
/// answer's publication is the queued write -- `pending`, with no page id --
/// which the panel draws as on its way. The send, the sync that brings the
/// page into the mirror and the link ([`protocol::reconcile`], run by
/// [`standup_protocol_inner`]) follow in a spawned task, and every later read
/// of the protocol names the page and links it as well, so a panel that
/// re-reads when the write settles finds the address there.
///
/// The awaited path stays for the battery and `atlassian_live.rs`.
///
/// [`protocol::reconcile`]: crate::protocol::reconcile
///
/// # Errors
///
/// As [`publish_standup_protocol_inner`], all of them before anything is sent.
pub async fn publish_standup_protocol_in_background(
    state: &crate::sources::SourcesState,
    day: chrono::NaiveDate,
    target: Option<crate::protocol::PublishTarget>,
) -> Result<crate::protocol::Protocol, IpcError> {
    let pool = &state.pool;
    let target = target_for(pool, target).await?;
    if let Some(payload) = page_to_make(pool, day, &target).await? {
        let (queued, source_id) = crate::sources::write_queue::queue(state, payload).await?;
        crate::sources::write_queue::send_in_background_then(
            state,
            source_id,
            queued.id,
            target.source_id.clone(),
            move |state| async move {
                if let Err(error) = standup_protocol_inner(&state.pool, day).await {
                    tracing::warn!(%error, "a published protocol could not be linked yet");
                }
            },
        );
    }
    standup_protocol_inner(pool, day).await
}

/// The `CreatePage` a publish for `day` owes, or `None` when the date already
/// has a publication in the target's Confluence -- the module's one-page rule.
async fn page_to_make(
    pool: &PgPool,
    day: chrono::NaiveDate,
    target: &crate::protocol::PublishTarget,
) -> Result<Option<serde_json::Value>, IpcError> {
    let note = crate::protocol::get_or_create(pool, day).await?;
    if crate::protocol::publication_of(pool, day, &target.source_id)
        .await?
        .is_some()
    {
        return Ok(None);
    }
    let space = crate::protocol::space_of(pool, &target.parent).await?;
    Ok(Some(crate::protocol::create_page_payload(
        target,
        &space,
        day,
        &note.body_md,
    )))
}

/// The target a publish aims at: the one given, recorded as the setting, or
/// the stored one, or a refusal rather than a guess.
async fn target_for(
    pool: &PgPool,
    target: Option<crate::protocol::PublishTarget>,
) -> Result<crate::protocol::PublishTarget, IpcError> {
    Ok(match target {
        Some(chosen) => crate::protocol::set_publish_target(pool, &chosen).await?,
        None => crate::protocol::publish_target(pool)
            .await?
            .ok_or_else(|| {
                IpcError::invalid(
                    "knobas does not know where to publish standup protocols yet. Choose the \
                 Confluence source and the parent page first.",
                )
            })?,
    })
}

/// [`publish_standup_protocol_in_background`] behind the app handle: answers
/// once the page's write is queued (#779).
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up;
/// otherwise as [`publish_standup_protocol_inner`].
#[tauri::command]
pub async fn publish_standup_protocol<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    day: chrono::NaiveDate,
    target: Option<crate::protocol::PublishTarget>,
) -> Result<crate::protocol::Protocol, IpcError> {
    let state = crate::sources::state(&app)?;
    publish_standup_protocol_in_background(&state, day, target).await
}

/// Where standup protocols are published, or `null` until somebody has said
/// (story 67).
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a read failure.
#[tauri::command]
pub async fn standup_publish_target(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Option<crate::protocol::PublishTarget>, IpcError> {
    crate::protocol::publish_target(&lifecycle.pool()?).await
}

/// Change where standup protocols are published -- settings' half of story 67.
///
/// The same key [`publish_standup_protocol`] writes when it is given a target,
/// so the dialog and the settings panel cannot disagree about where the next
/// page lands. A protocol **already published** is not moved: it has a page and
/// a link, and the setting is about the next one.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a target that could not route
/// a write.
#[tauri::command]
pub async fn set_standup_publish_target(
    lifecycle: State<'_, Lifecycle>,
    target: crate::protocol::PublishTarget,
) -> Result<crate::protocol::PublishTarget, IpcError> {
    crate::protocol::set_publish_target(&lifecycle.pool()?, &target).await
}

/// What became of filing a ticket from an action item.
#[derive(Debug, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct ActionItemTicket {
    /// The write queue row carrying the create.
    pub write_id: i64,
    /// The queue's own word for where that write stands (#779): `pending`
    /// while it is on its way, `sent` once it landed, `refused` or `held`
    /// when it will not go on its own, `discarded` once withdrawn.
    pub state: knobas_core::write_queue::WriteState,
    /// The queue's or the source's sentence about the write. Untrusted text.
    pub detail: Option<String>,
    /// The ticket, as an entity id, once the mirror has it.
    ///
    /// `None` for a create that is still queued, and for one that landed at a
    /// source whose re-read has not brought the ticket back yet. Both are "not
    /// yet", and the note is unlinked in both -- there is nothing to link to.
    pub ticket_entity_id: Option<String>,
    /// Whether the note and the ticket are linked.
    pub linked: bool,
}

/// File a ticket from an action item and link it to the protocol (story 69).
///
/// **The existing op, unchanged.** `WriteOp::CreateTicket` is what the ticket
/// is made with and `submit` is what carries it, so this adds no write path --
/// it is the start-work flow's shape in miniature: dispatch an op, find what it
/// made by *reading the mirror*, draw the link.
///
/// The ticket is the one the create's receipt named (#1754), taken once the
/// mirror holds it; a create whose source named no key is found by its title.
/// `submit` re-syncs the source when the write lands, so the ticket is
/// normally there by the time this looks.
///
/// A ticket that is **not** there yet is not an error and files nothing twice:
/// the write is queued, the caller is told the id is not known, and the link is
/// the one thing left undone. Pressing *Create ticket* again would file a
/// second ticket, which is why the answer says plainly that the create is on
/// the queue.
///
/// `project` is the project's entity id (`jira:PAY`), which is what
/// `WriteOp::CreateTicket` calls its `entity` and what `list_projects` hands
/// the picker.
///
/// # Errors
///
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for a blank title
/// or a project that is not an entity id,
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no note carries `note_id`;
/// otherwise whatever the write queue refuses with.
pub async fn create_action_item_ticket_inner(
    state: &crate::sources::SourcesState,
    note_id: &str,
    project: &str,
    ticket_type: &str,
    title: &str,
    body: &str,
) -> Result<ActionItemTicket, IpcError> {
    let pool = &state.pool;
    let (project_ref, payload) =
        action_item_payload(pool, note_id, project, ticket_type, title, body).await?;
    let queued = crate::sources::write_queue::submit(state, payload).await?;
    // The same wait, for the same reason: a created ticket has no address
    // until the mirror has it, and this is the call that goes looking.
    crate::sources::write_queue::sync_after_write(state, &project_ref.namespace).await;
    action_item_ticket_inner(pool, note_id, queued.id).await
}

/// [`create_action_item_ticket_inner`], **answering once the create is
/// queued** (#779): what the `create_action_item_ticket` command does.
///
/// Every refusal the awaited path makes is made before the answer. The answer
/// is the queued write with no ticket yet; the send, the sync that brings the
/// ticket into the mirror and the link follow in a spawned task, and
/// [`action_item_ticket_inner`] -- which the panel asks when the write settles
/// -- names the ticket and draws the link as well, whichever comes first.
///
/// The awaited path stays for the battery.
///
/// # Errors
///
/// As [`create_action_item_ticket_inner`], all of them before anything is sent.
pub async fn create_action_item_ticket_in_background(
    state: &crate::sources::SourcesState,
    note_id: &str,
    project: &str,
    ticket_type: &str,
    title: &str,
    body: &str,
) -> Result<ActionItemTicket, IpcError> {
    let (project_ref, payload) =
        action_item_payload(&state.pool, note_id, project, ticket_type, title, body).await?;
    let (queued, source_id) = crate::sources::write_queue::queue(state, payload).await?;
    let (note, id) = (note_id.to_owned(), queued.id);
    crate::sources::write_queue::send_in_background_then(
        state,
        source_id,
        id,
        project_ref.namespace,
        move |state| async move {
            if let Err(error) = action_item_ticket_inner(&state.pool, &note, id).await {
                tracing::warn!(%error, write_id = id, "an action item's ticket could not be linked yet");
            }
        },
    );
    Ok(ActionItemTicket {
        write_id: queued.id,
        state: queued.state,
        detail: queued.detail,
        ticket_entity_id: None,
        linked: false,
    })
}

/// The checks and the `CreateTicket` payload both paths share.
async fn action_item_payload(
    pool: &PgPool,
    note_id: &str,
    project: &str,
    ticket_type: &str,
    title: &str,
    body: &str,
) -> Result<(EntityRef, serde_json::Value), IpcError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(IpcError::invalid(
            "an action item with no words is not a ticket anybody could act on",
        ));
    }
    let project_ref = EntityRef::parse(project).map_err(IpcError::invalid)?;
    // The note has to be there before the write is sent: the whole point is a
    // ticket linked to a protocol, and discovering the note was deleted after
    // Jira has the ticket leaves a ticket nothing points at.
    get_note_inner(pool, note_id).await?;
    Ok((
        project_ref,
        serde_json::to_value(knobas_source::WriteOp::CreateTicket {
            entity: project.to_owned(),
            title: title.to_owned(),
            body: body.to_owned(),
            ticket_type: ticket_type.to_owned(),
        })
        .map_err(IpcError::internal)?,
    ))
}

/// What became of the ticket an action item's write `write_id` files
/// (#779): where the write stands, and -- once it was sent and the mirror has
/// the ticket -- the ticket, linked to the note.
///
/// **Idempotent, like `protocol::reconcile`**: the panel asks it each time the
/// queue moves until the ticket is named, and the background task behind
/// [`create_action_item_ticket_in_background`] asks it once after its sync.
/// Whichever finds the ticket first draws the link; the other finds it drawn.
///
/// The ticket is looked for **only once the write was sent**: before that, a
/// ticket of the same title in the project is somebody else's, never this one.
/// It is the one the create's receipt named (`sources::write_queue::ticket_named`,
/// #1754); only a create the source answered without a key is looked for by its
/// title (`protocol::ticket_titled`).
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no write carries
/// `write_id`; [`Invalid`](crate::IpcErrorCode::Invalid) if it is not a
/// ticket's create; [`Internal`](crate::IpcErrorCode::Internal) for a read or
/// link failure.
pub async fn action_item_ticket_inner(
    pool: &PgPool,
    note_id: &str,
    write_id: i64,
) -> Result<ActionItemTicket, IpcError> {
    let write = knobas_core::write_queue::get(pool, write_id)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("no queued write with id {write_id}")))?;
    // ADR-0007: the queued payload is knobas's own `WriteOp`, so it is read as
    // one. Fail-closed and pinned: a payload that is not a ticket's create
    // (or not a `WriteOp` at all) is refused, never read as a title-less create.
    let not_a_ticket = || IpcError::invalid(format!("write {write_id} does not file a ticket"));
    let title = match serde_json::from_value::<knobas_source::WriteOp>(write.payload.clone()) {
        Ok(knobas_source::WriteOp::CreateTicket { title, .. }) => title,
        _ => return Err(not_a_ticket()),
    };
    let title = title.as_str();
    let mut ticket_entity_id = None;
    let mut linked = false;
    if write.state == knobas_core::write_queue::WriteState::Sent {
        // The key the create's receipt named (#1754); the title only where
        // the source named none.
        use crate::sources::write_queue::{Named, ticket_named};
        ticket_entity_id = match ticket_named(pool, write_id).await? {
            Named::Ticket(named) => named,
            Named::Nothing => crate::protocol::ticket_titled(pool, &write.source_id, title).await?,
        };
        if let Some(ticket) = ticket_entity_id.as_deref() {
            crate::protocol::draw_link(
                pool,
                note_id,
                ticket,
                crate::protocol::ACTION_ITEM_RELATION,
            )
            .await?;
            linked = true;
        }
    }
    Ok(ActionItemTicket {
        write_id,
        state: write.state,
        detail: write.detail,
        ticket_entity_id,
        linked,
    })
}

/// [`create_action_item_ticket_in_background`] behind the app handle:
/// answers once the create is queued (#779).
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up;
/// otherwise as [`create_action_item_ticket_inner`].
#[tauri::command]
pub async fn create_action_item_ticket<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    note_id: String,
    project: String,
    ticket_type: String,
    title: String,
    body: String,
) -> Result<ActionItemTicket, IpcError> {
    let state = crate::sources::state(&app)?;
    create_action_item_ticket_in_background(&state, &note_id, &project, &ticket_type, &title, &body)
        .await
}

/// Where an action item's ticket stands: its write, and the ticket once it is
/// filed and mirrored (#779). The panel asks it as the queue moves, to fill in
/// the address of a ticket it drew as on its way.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up; otherwise as [`action_item_ticket_inner`].
#[tauri::command]
pub async fn action_item_ticket(
    lifecycle: State<'_, Lifecycle>,
    note_id: String,
    write_id: i64,
) -> Result<ActionItemTicket, IpcError> {
    action_item_ticket_inner(&lifecycle.pool()?, &note_id, write_id).await
}
