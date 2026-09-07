//! Contexts: a working set of entities, and the rule that decides who is in it.
//!
//! A context (spec §7) is knobas' own object -- `knobas.context` plus a
//! `knobas.entity` row of kind `ctx`, which is what makes it linkable, and
//! being linkable is what *Add to context* is: an ordinary confirmed link with
//! the context as one end. It comes in three kinds ([`ContextKind`]): a
//! promoted **epic**, a promoted **ticket**, and an **ad-hoc** label that
//! never needed a source system at all.
//!
//! Membership is not stored anywhere; it is [`member_ids`]'s answer, computed
//! from the confirmed link graph by the fixed rule §16.11 ratified and
//! ADR-0008 records: **seed + direct links + one hop out**. The seed is the
//! explicit adds, the anchor, and -- for a promoted epic -- the tickets whose
//! source-recorded parent it is; the walk then takes each seed's links and one
//! further hop ("a member ticket's PRs, their builds"), and stops. The rule is
//! fixed and not configurable in v1.
//!
//! Since #434 the same statement carries §16.11's other sentence, *"asset
//! membership counts through ancestors"*: an asset is a member when it or any
//! of its ancestors is one of those three, expanded over
//! `knobas.asset.parent_id` (ADR-0014) and never over links. That expansion is
//! the walk's last layer and lives in [`MEMBER_IDS`] with the rest -- one
//! statement, so the room's tiles, the per-context inbox filter and the tray's
//! proposal scope cannot come to different answers about who is here.
//!
//! Every step of the walk reads `knobas.confirmed_link`, never `knobas.link`:
//! the table also holds proposals (#41), the tray *scopes proposals by
//! membership*, and a membership built from proposals would make the two
//! circular. `tests/link_reads.rs` is the scan that keeps this true.

use serde::Serialize;
use sqlx::PgPool;

use crate::CoreError;
use crate::entity::EntityRef;

crate::closed_vocabulary! {
    /// What kind of working set this is.
    ///
    /// Stored as lowercase text in `knobas.context.kind`, whose
    /// `context_kind_chk` (migration 0010) allows exactly these spellings --
    /// and `ALL` is what the test that pins the two together walks.
    pub enum ContextKind {
        /// A promoted epic: members start from its tickets.
        Epic => "epic",
        /// A promoted ticket: focused on the one piece of work.
        Ticket => "ticket",
        /// A label the user minted; no source system behind it.
        Adhoc => "adhoc",
    }
}

impl std::fmt::Display for ContextKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A `kind` column value that is not one of the three known kinds.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown context kind {0:?}")]
pub struct UnknownContextKind(pub String);

impl std::str::FromStr for ContextKind {
    type Err = UnknownContextKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Read off `ALL` rather than a second hand-written match: the point of
        // declaring the enum as a closed vocabulary is that there is one list.
        ContextKind::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == s)
            .ok_or_else(|| UnknownContextKind(s.to_owned()))
    }
}

// The column is plain `text`, so the codec borrows `str`'s rather than
// declaring a PostgreSQL enum type that does not exist.
impl sqlx::Type<sqlx::Postgres> for ContextKind {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <str as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <&str as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for ContextKind {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let text = <&str as sqlx::Decode<'r, sqlx::Postgres>>::decode(value)?;
        Ok(text.parse()?)
    }
}

/// One context, as `knobas.context` holds it.
#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct ContextRow {
    /// `ctx:<uuid>` -- a local id (`RESERVED_NAMESPACES`), and also the id of
    /// this context's `knobas.entity` row.
    pub id: String,
    pub kind: ContextKind,
    pub title: String,
    /// The promoted entity this context is about; `None` for an ad-hoc label.
    pub anchor_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub archived_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Every column of `knobas.context` that leaves this module, in one place --
/// the discipline `link_columns!` records, for the same reason.
macro_rules! context_columns {
    () => {
        "id, kind, title, anchor_id, created_at, archived_at"
    };
}

/// Mint a fresh context id.
///
/// A uuid rather than something derived from a title: titles are neither
/// unique nor stable, and the id outlives every rename. The anchor tie lives
/// in `anchor_id`, not in the id's spelling.
fn mint_id() -> String {
    EntityRef::new("ctx", &uuid::Uuid::new_v4().to_string()).to_string()
}

/// Write one context and its entity row, in one transaction.
///
/// The entity row is what makes the context linkable at all -- an *Add to
/// context* is a link, and `knobas.link` has foreign keys on both ends -- so a
/// context without one would exist and be unusable. One transaction, because
/// the two rows are one fact.
async fn insert(
    pool: &PgPool,
    id: &str,
    kind: ContextKind,
    title: &str,
    anchor_id: Option<&str>,
) -> Result<ContextRow, CoreError> {
    let mut tx = pool.begin().await?;
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ctx', $2)")
        .bind(id)
        .bind(title)
        .execute(&mut *tx)
        .await?;
    let row = sqlx::query_as::<_, ContextRow>(concat!(
        "insert into knobas.context (id, kind, title, anchor_id)
         values ($1, $2, $3, $4)
         returning ",
        context_columns!()
    ))
    .bind(id)
    .bind(kind.as_str())
    .bind(title)
    .bind(anchor_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row)
}

/// Create an ad-hoc context -- a label, no anchor, no source system.
///
/// The title is stored as given (trimmed); validating that a label says
/// anything is the caller's, because "what is worth refusing" is an interface
/// question and this store would answer it invisibly.
///
/// # Errors
///
/// [`CoreError::Db`] if a write fails.
pub async fn create_adhoc(pool: &PgPool, title: &str) -> Result<ContextRow, CoreError> {
    insert(pool, &mint_id(), ContextKind::Adhoc, title.trim(), None).await
}

/// What the promote path needs to know about its anchor, in one read.
///
/// `epic` is a **payload read outside an adapter**, governed by ADR-0007: the
/// issue type lives only in the source's own shape
/// (`fields.issuetype.name`), this statement is the one place that shape is
/// spelled, and its failure direction is pinned by
/// `an_unrecognized_issue_type_shape_misses_toward_ticket` -- an absent or
/// unrecognized shape reads `false`, so a miss promotes the *narrower*
/// ticket-kind context, never a wrong epic. A second source's spelling of
/// "this is an epic" is one more `or` here and nothing anywhere else.
///
/// The join is `left`, so an entity that was never mirrored (a note, say)
/// still answers -- with no payload to read, it misses toward ticket like
/// everything else.
const ANCHOR_SHAPE: &str = "
    select e.title,
           coalesce(i.payload->'fields'->'issuetype'->>'name' ilike 'epic', false) as epic
      from knobas.entity e
      left join sync.live_item i on i.entity_id = e.id
     where e.id = $1";

/// A promotion's answer: the context, and whether this call made it.
///
/// `fresh` is decided by **which statement ran**, never by comparing lists
/// before and after -- a diff of two reads is a representation of the fact
/// and is racy about it, while "my insert succeeded" is the fact itself. The
/// caller that writes one activity line and one event per *mutation* is what
/// the flag exists for.
#[derive(Clone, Debug)]
pub struct Promoted {
    pub context: ContextRow,
    /// True when this call inserted the context; false when it answered with
    /// one that already existed.
    pub fresh: bool,
}

/// Promote an entity to a context of its own (spec §7: "any ticket can be
/// promoted").
///
/// Idempotent: promoting an entity that already anchors an unarchived context
/// answers with that context and `fresh: false`. The check-then-insert race is
/// closed by `context_anchor_idx` (migration 0010), whose refusal classifies
/// as [`CoreError::Duplicate`] and is answered by re-reading -- so two racing
/// promotes both get the one context and exactly one of them hears
/// `fresh: true`.
///
/// `None` if `anchor` has no `knobas.entity` row: promoting something that
/// never synced is a miss the caller turns into its own not-found, not a
/// context about nothing.
///
/// # Errors
///
/// [`CoreError::AnchorIsAContext`] if `anchor` is itself a context: a context
/// about a context would put a `ctx` node at the walk's root, which is the
/// exact traversal [`MEMBER_IDS`]'s kind filter exists to refuse -- see the
/// module note. [`CoreError::Db`] if a statement fails.
pub async fn promote(pool: &PgPool, anchor: &EntityRef) -> Result<Option<Promoted>, CoreError> {
    if anchor.namespace == "ctx" {
        return Err(CoreError::AnchorIsAContext);
    }
    let anchor_id = anchor.to_string();
    let Some((title, epic)) = sqlx::query_as::<_, (String, bool)>(ANCHOR_SHAPE)
        .bind(&anchor_id)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };

    if let Some(existing) = anchored(pool, &anchor_id).await? {
        return Ok(Some(Promoted {
            context: existing,
            fresh: false,
        }));
    }

    let kind = if epic {
        ContextKind::Epic
    } else {
        ContextKind::Ticket
    };
    // A mirrored title can be blank; the anchor's key is the one name that is
    // always there to fall back on.
    let title = if title.trim().is_empty() {
        anchor.key.clone()
    } else {
        title
    };

    match insert(pool, &mint_id(), kind, &title, Some(&anchor_id)).await {
        Ok(row) => Ok(Some(Promoted {
            context: row,
            fresh: true,
        })),
        // The race the index exists for: someone promoted between the read
        // and the write. The context they made is the answer; this call
        // mutated nothing, so it is not fresh.
        Err(CoreError::Duplicate) => {
            Ok(anchored(pool, &anchor_id).await?.map(|context| Promoted {
                context,
                fresh: false,
            }))
        }
        Err(other) => Err(other),
    }
}

/// The unarchived context anchored on `anchor_id`, if there is one.
async fn anchored(pool: &PgPool, anchor_id: &str) -> Result<Option<ContextRow>, CoreError> {
    Ok(sqlx::query_as::<_, ContextRow>(concat!(
        "select ",
        context_columns!(),
        " from knobas.context where anchor_id = $1 and archived_at is null"
    ))
    .bind(anchor_id)
    .fetch_optional(pool)
    .await?)
}

/// Every unarchived context, newest first -- the switcher's list.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn list(pool: &PgPool) -> Result<Vec<ContextRow>, CoreError> {
    Ok(sqlx::query_as::<_, ContextRow>(concat!(
        "select ",
        context_columns!(),
        " from knobas.context where archived_at is null
          order by created_at desc, id desc"
    ))
    .fetch_all(pool)
    .await?)
}

/// Where a source records an item's **parent**, as one named path.
///
/// A **payload read outside an adapter**, governed by ADR-0007, and this is
/// requirement 2 discharged: two seeds read it now — [`MEMBER_IDS`]' one
/// context and [`held_by_any_context!`]'s every context — and a second
/// source's spelling has to be one more `coalesce` *here* rather than an edit
/// in each. `fields.parent.key` is Jira's, widened by #32.
///
/// **Its failure direction, stated (requirement 3): it misses.** A record
/// whose payload does not carry this path yields no row, so an epic seeds
/// fewer children rather than the wrong ones, and the walk is short rather
/// than wrong — an absent member, never a wrong one. Pinned in both seeds:
/// `a_foreign_or_misshapen_parent_contributes_nothing` for `member_ids`, and
/// `a_misshapen_parent_seeds_no_context_in_the_merged_walk` for the merged
/// one, both in `tests/contexts.rs`.
///
/// Exported for [`held_by_any_context!`]'s sake — a macro's body resolves at
/// its call site — which is [`context_expansion!`]'s reason too.
#[doc(hidden)]
#[macro_export]
macro_rules! recorded_parent_key {
    () => {
        "i.payload->'fields'->'parent'->>'key'"
    };
}

/// The three layers every membership walk shares, whatever seeded it.
///
/// Written once because there are two seeds -- [`MEMBER_IDS`]' one context and
/// [`held_by_any_context!`]'s every context at once -- and the expansion over
/// them is the *same rule*: a seed's links, one hop further, and everything
/// held under whatever that reached. Two copies of it would be two answers to
/// "who is in a context" the day one was amended, which is the failure the
/// module header's *one statement* sentence exists to prevent.
///
/// It expects a CTE named `seed(id)` before it and defines `direct`, `hop` and
/// `held` after it, ending without a trailing comma so a caller appends its
/// own `select`. Exported only because [`held_by_any_context!`] is a macro too
/// and a macro's body resolves at its call site -- `declared_candidates`'
/// arrangement, and its reason.
#[doc(hidden)]
#[macro_export]
macro_rules! context_expansion {
    () => {
        "    direct(id) as (
        select id from seed
        union
        select o.id
          from seed s
          join knobas.confirmed_link l on l.from_id = s.id or l.to_id = s.id
          join knobas.entity o
            on o.id = case when l.from_id = s.id then l.to_id else l.from_id end
         where o.kind <> 'ctx'
    ),
    hop(id) as (
        select id from direct
        union
        select o.id
          from direct d
          join knobas.confirmed_link l on l.from_id = d.id or l.to_id = d.id
          join knobas.entity o
            on o.id = case when l.from_id = d.id then l.to_id else l.from_id end
         where o.kind <> 'ctx'
    ),
    -- ADR-0008's asset clause, over ADR-0014's column: everything held by
    -- something the walk reached, at any depth, and nothing that merely links
    -- to it.
    held(id) as (
        select id from hop
        union
        select c.id
          from held h
          join knobas.asset c on c.parent_id = h.id
    )"
    };
}

/// The membership rule, as one statement (§16.11, ADR-0008).
///
/// Four terms. The three **link** layers are plain CTEs rather than a
/// recursion, because the link rule is *fixed at* seed + direct + one hop and
/// a recursive walk there would be a knob this statement exists not to have;
/// the fourth, `held`, is a recursion because containment is transitive
/// without limit and that asymmetry is the rule (see below, and #434):
///
/// * **`seed`** -- the explicit adds (every confirmed link touching the
///   context's own entity), the anchor, and the epic's children: mirrored
///   items whose source-recorded parent (`fields.parent.key`, widened by #32)
///   is the anchor, in the anchor's own source -- two Jiras are two
///   namespaces, and matching bare keys across them would seed `jira:PAY-1`'s
///   epic with `jira-eu:PAY-1`.
/// * **`direct`** -- the seeds plus everything they link to.
/// * **`hop`** -- those plus one hop further, and no further.
/// * **`held`** -- the assets under everything the walk reached, at any depth
///   (#434). ADR-0008's ratified sentence is *"asset membership counts through
///   ancestors"*, so an asset is a member when **it or any of its ancestors**
///   is a seed, a direct link or a one-hop reach.
///
/// `held` is a `with recursive` term and the other three are not, and the
/// asymmetry is the rule rather than an inconsistency: the link walk is *fixed
/// at* three layers (§16.11, "not configurable in v1"), while containment is
/// transitive without limit -- spec §12.1's "any asset can hold assets without
/// limit" -- and ADR-0014 is the decision that keeps the two apart by making
/// containment a column instead of a relation. It expands over
/// `knobas.asset.parent_id` and **never over links**, and it is the walk's
/// **last** layer: the assets it brings in contribute no seeds, no direct
/// links and no hops of their own, so a page linked to a brought-in container
/// is not a member. Only assets carry a parent, so nothing else can enter
/// through it, and the column's one writer refuses cycles before it writes
/// (`knobas_app::assets::move_to`, with `asset_no_self_parent_chk` under it).
///
/// **No `implied` row is written anywhere for this**, which is spec #427's own
/// ruling -- *"the implied membership of an asset linked to a member ticket is
/// computed, not stored, as the ADR requires"* -- and ADR-0008's first
/// subsidiary decision. Spec §5a's *"linking an asset to a ticket auto-adds
/// the asset to that ticket's contexts"* is what the `direct` layer already
/// says; what makes it *removable* is that removing the link removes the
/// membership, with nothing left behind.
///
/// The parent match is a **payload read outside an adapter**, governed by
/// ADR-0007. Since #446 there are two seeds that make it, so the *path* is
/// confined to one named statement of its own -- [`recorded_parent_key!`],
/// which both read -- rather than to this one; its failure direction is stated
/// there and pinned here by `a_foreign_or_misshapen_parent_contributes_nothing`
/// -- a shape the path does not fit contributes no seed, so the failure is an
/// absent member, never a wrong one.
///
/// Every **link** expansion joins `knobas.entity` to refuse `ctx`-kind
/// neighbours: a ticket shared by two contexts would otherwise walk *through*
/// the second context and union the two memberships. `held` needs no such
/// guard and has none -- it joins `knobas.asset`, and only an asset has a
/// parent, so a context cannot enter through it. Those three steps read
/// `knobas.confirmed_link` at every step -- see the module note for why that
/// is load-bearing and not a style choice.
const MEMBER_IDS: &str = concat!(
    "
    with recursive seed(id) as (
        select o.id
          from knobas.confirmed_link l
          join knobas.entity o
            on o.id = case when l.from_id = $1 then l.to_id else l.from_id end
         where (l.from_id = $1 or l.to_id = $1)
           and o.kind <> 'ctx'
        union
        -- The anchor, checked like every other entrant: `promote` refuses a
        -- ctx anchor, but a row written by import or by hand must not put a
        -- context at the walk's root either.
        select e.id
          from knobas.entity e
         where $2::text is not null and e.id = $2::text and e.kind <> 'ctx'
        union
        select i.entity_id
          from sync.live_item i
         where $3::text is not null
           and i.source_id = $3::text
           and ",
    crate::recorded_parent_key!(),
    " = $4::text
    ),
",
    crate::context_expansion!(),
    "
    select id from held where id <> $1"
);

/// Every entity **some** context holds, as a parenthesised subquery -- the same
/// walk [`MEMBER_IDS`] makes, seeded from every context at once (#446).
///
/// The inbox's alert rule needs one question answered — *is this asset a member
/// of some context, directly or through an ancestor?* — inside a statement that
/// binds no context id, because the inbox is one derivation over the whole
/// mirror and not a read per context. Asking [`member_ids`] once per context
/// from Rust would be a second walk beside this one, which #434's third
/// criterion forbids in as many words.
///
/// **Merging the seeds is exact, not an approximation**, and that is worth
/// stating because it looks like one. Every layer after the seed is a
/// *neighbour* expansion, and taking neighbours distributes over union:
/// `N(A ∪ B) = N(A) ∪ N(B)`. So the walk from every context's seeds at once
/// reaches exactly the union of the walks from each context's seeds — the same
/// three layers, the same depth, the same `held` recursion. It is checked
/// rather than argued: `the_merged_walk_is_the_union_of_every_contexts_members`
/// in `tests/contexts.rs` compares this against [`member_ids`] over
/// [`list`]'s own contexts on a fixture that has several.
///
/// **Archived contexts are left out**, which is the one place this and
/// [`member_ids`] deliberately differ: `member_ids` is asked about a context by
/// name and answers about *that* context whatever its state, while this asks
/// "is anyone still working on this?" — and an archived context is one the
/// reader put away. [`list`] draws the same line for the switcher, and an alert
/// that went on interrupting somebody because of a context they archived last
/// spring would be the inbox failing its own promise. It is why the comparison
/// above is against `list`'s contexts rather than every row.
///
/// The epic-children seed makes the same **payload read outside an adapter**
/// [`MEMBER_IDS`] does, through the same [`recorded_parent_key!`], and is bound
/// by ADR-0007's three requirements through it: it misses, the path is one
/// named statement both seeds read, and its failure direction is stated there
/// and pinned here by `a_misshapen_parent_seeds_no_context_in_the_merged_walk`.
///
/// A subquery and not a `const`, because the one caller `concat!`s it into a
/// compile-time statement; `declared_list!` is the same shape for the same
/// reason.
#[macro_export]
macro_rules! held_by_any_context {
    () => {
        concat!(
            "(with recursive seed(id) as (
        select o.id
          from knobas.context c
          join knobas.confirmed_link l on l.from_id = c.id or l.to_id = c.id
          join knobas.entity o
            on o.id = case when l.from_id = c.id then l.to_id else l.from_id end
         where c.archived_at is null and o.kind <> 'ctx'
        union
        select e.id
          from knobas.context c
          join knobas.entity e on e.id = c.anchor_id
         where c.archived_at is null and e.kind <> 'ctx'
        union
        -- A promoted epic's children, by the source-recorded parent key, in
        -- the anchor's own namespace -- `member_ids` binds the two halves and
        -- here they are split out of the anchor id, which is the same
        -- `<namespace>:<key>` split `knobas_core::entity::EntityRef` makes.
        select i.entity_id
          from knobas.context c
          join sync.live_item i
            on i.source_id = split_part(c.anchor_id, ':', 1)
           and ",
            $crate::recorded_parent_key!(),
            " = substr(c.anchor_id, strpos(c.anchor_id, ':') + 1)
         where c.archived_at is null and c.kind = 'epic'
    ),
",
            $crate::context_expansion!(),
            "
    select id from held)"
        )
    };
}

/// Every entity some context holds, as a list -- [`held_by_any_context!`] run.
///
/// Nothing in the app reads this: it exists so the merged walk has a name a
/// test can call, and `the_merged_walk_is_the_union_of_every_contexts_members`
/// is the test. A subquery nobody can run on its own is a rule nobody can write
/// a control for, which is [`RULES`](crate::inbox::RULES)' own argument.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn held_by_any_context(pool: &PgPool) -> Result<Vec<String>, CoreError> {
    let rows: Vec<(String,)> = sqlx::query_as(concat!(
        "select id from ",
        crate::held_by_any_context!(),
        " as m"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Who is in this context, by the fixed rule -- computed, never stored.
///
/// An unknown or archived-and-forgotten `ctx_id` answers with no members
/// rather than an error: an address can outlive the thing it names, and every
/// caller of this is a reader scoping a view, for which "nothing here" is the
/// honest answer. Withdrawn entities are *not* filtered -- a member the source
/// deleted is still a member (§5a), and each reading surface decides what to
/// show about it.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn member_ids(pool: &PgPool, ctx_id: &str) -> Result<Vec<String>, CoreError> {
    let row = sqlx::query_as::<_, (Option<String>, ContextKind)>(
        "select anchor_id, kind from knobas.context where id = $1",
    )
    .bind(ctx_id)
    .fetch_optional(pool)
    .await?;
    let (anchor, kind) = match row {
        Some((anchor_id, kind)) => (
            anchor_id.and_then(|id| EntityRef::parse(&id).ok()),
            Some(kind),
        ),
        None => (None, None),
    };

    // The parent seed is the **epic's**: spec §7 gives the two promoted kinds
    // different member rules ("epic: members = its tickets and everything
    // linked" vs "ticket: focused"), and a plain story's sub-tasks name it in
    // the same `fields.parent` -- seeding them into a ticket-kind context
    // would make every promoted story an epic in all but name. A misdetected
    // epic therefore loses its children (the issue-type read misses toward
    // ticket), which is ADR-0007's direction: an absent member, never a wrong
    // one.
    let seeds_children = kind == Some(ContextKind::Epic);
    let (anchor_id, anchor_source, anchor_key) = match &anchor {
        Some(entity) => (
            Some(entity.to_string()),
            seeds_children.then(|| entity.namespace.clone()),
            seeds_children.then(|| entity.key.clone()),
        ),
        None => (None, None, None),
    };

    let rows: Vec<(String,)> = sqlx::query_as(MEMBER_IDS)
        .bind(ctx_id)
        .bind(anchor_id)
        .bind(anchor_source)
        .bind(anchor_key)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_roundtrips_through_its_column_value() {
        for kind in [ContextKind::Epic, ContextKind::Ticket, ContextKind::Adhoc] {
            assert_eq!(kind.as_str().parse(), Ok(kind));
        }
        assert!("label".parse::<ContextKind>().is_err());
    }

    /// The enum and migration 0010's CHECK constraint are one list written in
    /// two places, and neither may grow without the other: a variant the
    /// constraint does not allow is an `INSERT` that fails at runtime, and a
    /// spelling the enum does not know is a row [`ContextKind`]'s decoder
    /// refuses, so the context cannot be read back at all.
    ///
    /// Driven by `ALL`, which is generated from the same variant list as the
    /// enum, so a new kind necessarily reaches this assertion. The other half
    /// of the pin -- the constraint as the live catalog reports it -- is in
    /// `crates/knobas-db/tests/schema.rs`.
    #[test]
    fn the_kinds_are_exactly_what_the_migration_allows() {
        let migration = include_str!("../../knobas-db/migrations/0010_contexts.sql");
        let line = migration
            .lines()
            .find(|line| line.contains("check (kind in ("))
            .expect("context_kind_chk is missing from 0010");

        for kind in ContextKind::ALL {
            assert!(
                line.contains(&format!("'{}'", kind.as_str())),
                "{kind:?} is a variant the constraint does not allow: {line}"
            );
        }
        assert_eq!(
            line.matches('\'').count() / 2,
            ContextKind::ALL.len(),
            "the constraint and the enum list different numbers of kinds: {line}"
        );
    }

    #[test]
    fn kind_serializes_as_its_column_value() {
        assert_eq!(
            serde_json::to_string(&ContextKind::Adhoc).unwrap(),
            "\"adhoc\""
        );
    }

    #[test]
    fn a_minted_id_is_a_parseable_local_ref() {
        let id = mint_id();
        let parsed = EntityRef::parse(&id).expect("a minted id parses");
        assert_eq!(parsed.namespace, "ctx");
    }
}
