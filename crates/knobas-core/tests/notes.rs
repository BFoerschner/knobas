//! The note store, against a real PostgreSQL.
//!
//! A note is the first thing knobas **owns** rather than mirrors, so the
//! battery here is about what a reader observes -- a note is written, edited,
//! found and deleted; a `[[ref]]` in its body resolves; the entity on the other
//! end shows the backlink -- and never about how the body was parsed. The
//! parsing has its own unit tests in `knobas_core::note`.
//!
//! Same concurrency contract as `tests/stores.rs`: one database per test
//! binary, shared by every test in it, so each test seeds ids unique to itself
//! and nothing truncates.

use knobas_core::entity::EntityRef;
use knobas_core::{link, note};
use uuid::Uuid;

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A mirrored entity of this run's own, so a `[[ref]]` has something real to
/// point at.
///
/// Written straight into `knobas.entity` rather than through a sync run: what a
/// ref needs is an address, and going through the engine would make this
/// battery depend on the adapter registry.
async fn seed_entity(pool: &sqlx::PgPool, kind: &str, title: &str, withdrawn: bool) -> EntityRef {
    let entity = EntityRef::new("jira", &format!("NOTE-{}", Uuid::new_v4()));
    sqlx::query(
        "insert into knobas.entity (id, kind, title, deleted_at)
         values ($1, $2, $3, case when $4 then now() end)",
    )
    .bind(entity.to_string())
    .bind(kind)
    .bind(title)
    .bind(withdrawn)
    .execute(pool)
    .await
    .unwrap();
    entity
}

const ACTOR: &str = "user";

#[tokio::test]
async fn a_note_is_written_read_back_and_edited() {
    let pool = pool().await;

    let written = note::create(&pool, "Standup", "Blocker: none.", ACTOR)
        .await
        .unwrap();
    let id = EntityRef::parse(&written.id).expect("a note id is an entity id");
    assert_eq!(id.namespace, "note");
    assert_eq!(written.title, "Standup");
    assert_eq!(written.body_md, "Blocker: none.");

    // Read back through the store, not through the row the write returned: the
    // point of the write returning one is that it is the *stored* row.
    let read = note::get(&pool, &id)
        .await
        .unwrap()
        .expect("the note exists");
    assert_eq!(read.title, "Standup");
    assert_eq!(read.body_md, "Blocker: none.");
    assert_eq!(read.created_at, written.created_at);

    let edited = note::save(&pool, &id, "Standup", "Blocker: the SEPA retry.", ACTOR)
        .await
        .unwrap()
        .expect("editing a note that exists");
    assert_eq!(edited.body_md, "Blocker: the SEPA retry.");
    assert_eq!(
        edited.created_at, written.created_at,
        "editing a note does not make it a new one"
    );
    assert!(
        edited.updated_at >= written.updated_at,
        "an edit moves the note's own clock"
    );
    assert_eq!(
        note::get(&pool, &id).await.unwrap().unwrap().body_md,
        "Blocker: the SEPA retry."
    );
}

/// A note is an entity, which is the whole of what makes it linkable and
/// addressable -- and its title is the entity's, or a link chip pointing at it
/// would draw a name nobody chose.
#[tokio::test]
async fn a_note_is_an_entity_and_renaming_it_renames_that_entity() {
    let pool = pool().await;
    let written = note::create(&pool, "Retry investigation", "", ACTOR)
        .await
        .unwrap();
    let id = EntityRef::parse(&written.id).unwrap();

    let (kind, title): (String, String) =
        sqlx::query_as("select kind, title from knobas.entity where id = $1")
            .bind(&written.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(kind, "note");
    assert_eq!(title, "Retry investigation");

    note::save(&pool, &id, "SEPA retry investigation", "", ACTOR)
        .await
        .unwrap()
        .unwrap();
    let (title,): (String,) = sqlx::query_as("select title from knobas.entity where id = $1")
        .bind(&written.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title, "SEPA retry investigation");
}

/// A note with no title still has a name, because everything that draws it --
/// a search row, a link chip, a room tile -- draws the entity's title.
#[tokio::test]
async fn an_untitled_note_is_given_a_name_rather_than_none() {
    let pool = pool().await;
    let written = note::create(&pool, "   ", "a thought", ACTOR)
        .await
        .unwrap();
    assert_eq!(written.title, note::UNTITLED);

    let id = EntityRef::parse(&written.id).unwrap();
    let saved = note::save(&pool, &id, "", "a thought", ACTOR)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.title, note::UNTITLED);
}

/// Deleting a note removes what the user wrote and keeps the address it was
/// written at.
///
/// The same shape a withdrawn mirrored item gets (§5a, #53): the entity row
/// survives and is tombstoned, so a link somebody drew *to* the note stays
/// visible and marked instead of dangling. What goes is the body -- deleting a
/// scratch thought has to actually delete it -- and with it the links the body
/// was the only source of.
#[tokio::test]
async fn deleting_a_note_removes_its_body_and_tombstones_its_address() {
    let pool = pool().await;
    let target = seed_entity(&pool, "ticket", "SEPA retry", false).await;
    let written = note::create(&pool, "Scratch", &format!("see [[{target}]]"), ACTOR)
        .await
        .unwrap();
    let id = EntityRef::parse(&written.id).unwrap();
    assert_eq!(link::entries_of(&pool, &target).await.unwrap().len(), 1);

    assert!(note::delete(&pool, &id).await.unwrap());
    assert!(note::get(&pool, &id).await.unwrap().is_none());

    let (deleted_at,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("select deleted_at from knobas.entity where id = $1")
            .bind(&written.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        deleted_at.is_some(),
        "the address survives the body, tombstoned"
    );
    assert!(
        link::entries_of(&pool, &target).await.unwrap().is_empty(),
        "the body was the only source of that link, and the body is gone"
    );

    // Idempotent: a second delete deleted nothing, and says so.
    assert!(!note::delete(&pool, &id).await.unwrap());
    // And editing a note that is gone is not the same as editing one that
    // never existed -- both answer `None` rather than writing a new note.
    assert!(
        note::save(&pool, &id, "back?", "", ACTOR)
            .await
            .unwrap()
            .is_none()
    );
}

/// Reading or editing a note nobody wrote is an empty answer, not an error and
/// not a new note.
#[tokio::test]
async fn a_note_that_does_not_exist_reads_as_nothing() {
    let pool = pool().await;
    let nobody = EntityRef::new("note", &Uuid::new_v4().to_string());
    assert!(note::get(&pool, &nobody).await.unwrap().is_none());
    assert!(
        note::save(&pool, &nobody, "t", "b", ACTOR)
            .await
            .unwrap()
            .is_none()
    );
    assert!(!note::delete(&pool, &nobody).await.unwrap());
    assert!(note::refs_of(&pool, &nobody).await.unwrap().is_empty());
}

/// The lifecycle in both directions: what the body says is what the links say.
///
/// The observable is the links panel's own read (`link::entries_of`) from the
/// *target's* end -- the backlink the ticket shows -- because that is what a
/// reader of the ticket sees and it is the whole point of a ref. Asserting on
/// the note's own row set would prove only that the note remembers what it
/// wrote.
#[tokio::test]
async fn adding_a_ref_draws_a_link_and_removing_it_withdraws_the_link() {
    let pool = pool().await;
    let ticket = seed_entity(&pool, "ticket", "SEPA retry", false).await;
    let written = note::create(&pool, "Investigation", "no refs yet", ACTOR)
        .await
        .unwrap();
    let id = EntityRef::parse(&written.id).unwrap();
    assert!(link::entries_of(&pool, &ticket).await.unwrap().is_empty());

    note::save(
        &pool,
        &id,
        "Investigation",
        &format!("off-by-one in [[{ticket}]]"),
        ACTOR,
    )
    .await
    .unwrap()
    .unwrap();

    let backlinks = link::entries_of(&pool, &ticket).await.unwrap();
    assert_eq!(
        backlinks.len(),
        1,
        "the ticket shows the note that names it"
    );
    let entry = &backlinks[0];
    assert_eq!(entry.link.from_id, written.id);
    assert_eq!(entry.link.to_id, ticket.to_string());
    assert_eq!(entry.link.relation, note::REF_RELATION);
    assert_eq!(
        entry.link.origin,
        link::Origin::Implied,
        "a ref is knobas drawing a link as a consequence of what was typed"
    );
    // Hydrated: the backlink names the note, not a raw id (#53).
    assert_eq!(entry.other.entity_id, written.id);
    assert_eq!(entry.other.kind, "note");
    assert_eq!(entry.other.title, "Investigation");

    // Saving the same body again changes nothing -- an autosave fires on every
    // keystroke pause, and a reconciliation that churned would rewrite the
    // link's id and its created_at on each one.
    note::save(
        &pool,
        &id,
        "Investigation",
        &format!("off-by-one in [[{ticket}]]"),
        ACTOR,
    )
    .await
    .unwrap()
    .unwrap();
    let again = link::entries_of(&pool, &ticket).await.unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(
        again[0].link.id, entry.link.id,
        "the same link, not a new one"
    );

    // And taking the ref out of the body withdraws it.
    note::save(&pool, &id, "Investigation", "off-by-one somewhere", ACTOR)
        .await
        .unwrap()
        .unwrap();
    assert!(
        link::entries_of(&pool, &ticket).await.unwrap().is_empty(),
        "the body no longer names it, so the link is withdrawn"
    );
    // Withdrawn, not erased: the tombstone is what an export and the log read.
    let (rows,): (i64,) = sqlx::query_as(
        "select count(*) from knobas.link where from_id = $1 and deleted_at is not null",
    )
    .bind(&written.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rows, 1);
}

/// A ref to a thing that does not exist is visible as unresolved and creates
/// nothing.
///
/// The negative case is what stops a typo becoming a phantom link: a link row
/// to `jira:PAY-9999` would be a backlink on nothing, exportable, and
/// unexplainable. It is also the case a reconciliation written the obvious way
/// -- insert every name the body holds -- gets wrong.
#[tokio::test]
async fn an_unresolved_ref_creates_no_link_and_says_it_is_unresolved() {
    let pool = pool().await;
    let real = seed_entity(&pool, "ticket", "SEPA retry", false).await;
    let typo = format!("jira:NOSUCH-{}", Uuid::new_v4());
    let written = note::create(
        &pool,
        "Investigation",
        &format!("[[{real}]] and [[{typo}]] and [[not an id]]"),
        ACTOR,
    )
    .await
    .unwrap();
    let id = EntityRef::parse(&written.id).unwrap();

    let (links,): (i64,) = sqlx::query_as(
        "select count(*) from knobas.link where from_id = $1 and deleted_at is null",
    )
    .bind(&written.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(links, 1, "only the ref that resolves is a link");

    let refs = note::refs_of(&pool, &id).await.unwrap();
    assert_eq!(
        refs.iter()
            .map(|r| r.target_id.as_str())
            .collect::<Vec<_>>(),
        [real.to_string().as_str(), typo.as_str(), "not an id"],
        "every ref the body names is shown back, in the order it wrote them"
    );
    assert_eq!(
        refs[0].target.as_ref().map(|t| t.title.as_str()),
        Some("SEPA retry")
    );
    assert!(refs[1].target.is_none(), "a typo is unresolved, not silent");
    assert!(refs[2].target.is_none(), "and so is text that is not an id");

    // ...and the day the target exists, the same body resolves it. Nothing had
    // to be re-typed, because the body was always the truth.
    sqlx::query(
        "insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'Arrived late')",
    )
    .bind(&typo)
    .execute(&pool)
    .await
    .unwrap();
    note::save(
        &pool,
        &id,
        "Investigation",
        &format!("[[{real}]] and [[{typo}]] and [[not an id]]"),
        ACTOR,
    )
    .await
    .unwrap()
    .unwrap();
    let refs = note::refs_of(&pool, &id).await.unwrap();
    assert_eq!(
        refs[1].target.as_ref().map(|t| t.title.as_str()),
        Some("Arrived late")
    );
    let (links,): (i64,) = sqlx::query_as(
        "select count(*) from knobas.link where from_id = $1 and deleted_at is null",
    )
    .bind(&written.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(links, 2);
}

/// A ref whose target the source withdrew stays a ref, stays a link, and is
/// marked.
///
/// The same fixture shape #53 pinned its hydration against -- a genuinely
/// tombstoned `knobas.entity` row -- because "withdrawn upstream" and "never
/// existed" are different facts and only one of them is the user's mistake.
/// A resolution that read `sync.live_item` would collapse them.
#[tokio::test]
async fn a_ref_to_a_withdrawn_entity_still_resolves_and_is_marked() {
    let pool = pool().await;
    let withdrawn = seed_entity(&pool, "ticket", "Legacy payout (withdrawn)", true).await;
    let written = note::create(&pool, "Runbook", &format!("was [[{withdrawn}]]"), ACTOR)
        .await
        .unwrap();
    let id = EntityRef::parse(&written.id).unwrap();

    let refs = note::refs_of(&pool, &id).await.unwrap();
    assert_eq!(refs.len(), 1);
    let target = refs[0]
        .target
        .as_ref()
        .expect("a withdrawn entity still resolves");
    assert_eq!(target.title, "Legacy payout (withdrawn)");
    assert!(
        target.deleted_at.is_some(),
        "and carries what marks it withdrawn"
    );

    // From the other end, the backlink is there too: the withdrawn ticket is
    // still openable and still says what points at it.
    let back = link::entries_of(&pool, &withdrawn).await.unwrap();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].other.entity_id, written.id);
    assert!(
        back[0].other.deleted_at.is_none(),
        "the live end -- the note -- is not marked withdrawn"
    );
}

/// The body governs the links the body derived, and nothing else.
///
/// A link the user drew by hand out of a note is theirs; a reconciliation that
/// took every link on the note would delete it the next time the note was
/// typed in, which is a note editor that silently unlinks things.
#[tokio::test]
async fn a_hand_drawn_link_out_of_a_note_survives_the_body_changing() {
    let pool = pool().await;
    let by_hand = seed_entity(&pool, "pr", "Fix the retry counter", false).await;
    let by_ref = seed_entity(&pool, "ticket", "SEPA retry", false).await;
    let written = note::create(&pool, "Investigation", &format!("[[{by_ref}]]"), ACTOR)
        .await
        .unwrap();
    let id = EntityRef::parse(&written.id).unwrap();

    // Deliberately the *same relation* a ref carries. Scoping the
    // reconciliation to the relation alone would look right until the day a
    // user drew this link by hand, which is the day it would start deleting
    // their work: `origin` is the only thing that tells the two apart.
    let manual = link::create(
        &pool,
        &id,
        &by_hand,
        note::REF_RELATION,
        link::Origin::Manual,
        Some("drawn in the panel"),
        ACTOR,
    )
    .await
    .unwrap();

    // Rewrite the body until nothing of the original is left.
    note::save(&pool, &id, "Investigation", "", ACTOR)
        .await
        .unwrap()
        .unwrap();

    let remaining = link::entries_of(&pool, &id).await.unwrap();
    assert_eq!(
        remaining.iter().map(|e| e.link.id).collect::<Vec<_>>(),
        [manual.id],
        "the ref went with the text that made it; the hand-drawn link did not"
    );
    assert_eq!(
        remaining[0].link.note.as_deref(),
        Some("drawn in the panel")
    );
}
