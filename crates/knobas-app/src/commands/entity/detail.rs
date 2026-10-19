//! The detail view: one entity, its links, activity and builds.

use super::*;

// -- the detail view --------------------------------------------------------

/// Which source an entity came from, as the detail view names it.
///
/// Every field falls back to the source id, because `run_once` syncs sources
/// that have no `knobas.source_config` row (tests, ad-hoc imports, interfaces
/// §1). An entity whose source was deleted still has to render.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceRef {
    pub id: String,
    pub display_name: String,
    /// The *adapter* kind (`jira`, `mock`), not the instance id.
    pub adapter_kind: String,
    /// Whether the user has this source turned on, **as of this read** --
    /// derived in the [`DETAIL`] statement, never stored (issue #204).
    ///
    /// The detail reaches past `sync.live_item` on purpose (§5a), so it opens
    /// entities every other reader hides -- and since migration `0012` there
    /// are two reasons a reader hides one. `deleted_at` marks the first
    /// (withdrawn upstream); this marks the second (turned off by the user),
    /// and the banner needs both because the remedies differ: nothing undoes
    /// a withdrawal, one click undoes a disable -- which is also why it is
    /// derived: a stored marker would still say "off" after that click.
    ///
    /// `true` for a source with no configuration row, matching `0012`'s
    /// `coalesce(enabled, true)`: absence of configuration is not a decision
    /// the user made.
    pub enabled: bool,
}

/// Everything the slide-over draws for one entity (interfaces §2.5).
#[derive(Debug, Clone, serde::Serialize)]
pub struct EntityDetail {
    pub row: EntityRow,
    pub source: SourceRef,
    /// The adapter's own label and monogram for this kind.
    ///
    /// Resolved from the adapter registry by the row's `adapter_kind` -- see
    /// [`kind_info_for`]. `None` for a kind the adapter does not declare, and
    /// for a source with no configuration row at all (which `run_once`
    /// produces). Both cases fall through to the frontend's §3a humaniser,
    /// which is what §3a asks for when nothing declares anything.
    pub kind_info: Option<knobas_source::KindInfo>,
    /// Untrusted source text. Rendered as text, never as markup (gotcha 7).
    pub body_text: String,
    pub author: Option<String>,
    /// The source record **verbatim** (§3a): what the generic detail view
    /// projects. It is untrusted text all the way down.
    pub payload: serde_json::Value,
    /// Where the item lives in its own system, or `None` when the adapter
    /// reported no page -- in which case *Open in browser* is absent (P5).
    pub web_url: Option<String>,
    /// Set when the source withdrew the entity. The mirror row survives, so
    /// links and notes still resolve (§5a).
    pub deleted_at: Option<DateTime<Utc>>,
    /// Every link this entity takes part in, newest first, each with the end
    /// the reader is *not* on already resolved.
    ///
    /// Read undirected by [`knobas_core::link::entries_of`], so a link drawn
    /// from either end is on both ends' detail, and hydrated there, so the
    /// panel draws a kind and a title rather than a raw id -- including for a
    /// target the source withdrew. [`create_link`] and [`unlink`] are what
    /// move it.
    pub links: Vec<LinkEntry>,
    /// This entity's own history, newest first (spec §12.1).
    pub activity: Vec<ActivityRow>,
    /// The mirrored builds that ran on the commit this entity stands at
    /// (#710): for a pull request its head commit, for a branch its tip, for
    /// a commit itself -- whatever its kind declares as `commit` -- the rows
    /// whose declared `revision` names that sha. Failing first, then running,
    /// then the rest by tone, newest first within one; at most
    /// [`DETAIL_BUILDS`]. Empty for a kind that declares no commit, which is
    /// every kind but code, and for a commit nothing mirrored ran on.
    pub builds: Vec<EntityRow>,
    /// A ticket's status as its source declares it (#760), read once beside
    /// the row by the rule the mini board reads it by
    /// ([`declared_string!`](knobas_core::declared_string)); the ticket detail's
    /// select shows it as the ticket's current one.
    ///
    /// `None` for anything that is not a ticket, for a ticket whose source
    /// declares no status or whose record does not carry it (the board's
    /// terminal group, "No status"), and for a withdrawn ticket or a disabled
    /// source's, which the board does not draw either.
    pub status: Option<String>,
    /// A ticket's type in its source's own word (#816, `Story`, `Bug`), read
    /// through the declared `issue_type` path; the job card draws it. `None`
    /// where nothing is declared or the record misses it (ADR-0007).
    pub issue_type: Option<String>,
    /// A ticket's priority in its source's own word (#816), through the
    /// declared `priority` path. `None` on a miss.
    pub priority: Option<String>,
    /// A ticket's original estimate in seconds (#816), through the declared
    /// `estimate` path: a whole JSON number or nothing, never zero for a
    /// miss. The job card writes it as *est. 3h*.
    pub estimate_seconds: Option<i64>,
}

/// How many history lines the detail view is given up front.
///
/// Enough to fill the panel without a second round trip; the full history is a
/// later milestone's view, not a scroll in a slide-over.
const DETAIL_ACTIVITY: i64 = 20;

/// How many builds the detail draws for a commit (#710): enough for every
/// configuration a commit is commonly built by, failing first, so a cut
/// leaves out passed ones.
const DETAIL_BUILDS: i64 = 20;

/// The builds that ran on the commit `$1` (#710), a full object id.
///
/// The commit is the entity's declared `commit`, read by [`DETAIL`] beside
/// the row ([`declared_string!`](knobas_core::declared_string)); a build ran
/// on it where its kind's declared `revision` lists it. That walk is stored
/// (#732, migration `0030`, ADR-0019's pattern): `sync.item_revision` holds
/// one row per commit each item declares, filled by the database as items
/// are written, and this statement asks its index on `revision` and touches
/// only the builds it names -- under a millisecond at 50,000 builds, where the
/// walk over every live row took 437-442 ms. No adapter is named here, no
/// payload key: the rows are matched to the source's configured adapter on
/// `adapter_kind`, so a source whose adapter declares no revision contributes
/// nothing whatever its payloads say. Matched exactly: both sides are full
/// object ids as git spells them.
///
/// **Asked only for an entity that stands at a commit**: the detail of a
/// ticket or a page declares none, so there is no sha to bind, and skipping
/// the round trip costs nothing.
///
/// The builds come from `sync.live_item`, so a withdrawn build or a disabled
/// source's is not drawn as if it stood. Ordered as the Builds tile weighs a
/// state (#680): `err`, `warn`, `info`, `ok`, `muted`, then none; newest
/// first within one. `$2` is the declarations, for the path line and the
/// state chip.
///
/// Public so `tests/it/builds_of_a_commit_perf.rs` can read its plan.
pub const DETAIL_BUILDS_AT: &str = concat!(
    r#"
select b.* from (
  select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.synced_at,
         "#,
    knobas_core::path_line_read!("$2"),
    r#" as path,
         "#,
    knobas_core::declared_state!("$2"),
    r#" as state
    from sync.item_revision r
    join sync.live_item i on i.entity_id = r.entity_id
    join knobas.source_config s on s.id = i.source_id and s.kind = r.adapter_kind
   where r.revision = $1
) b
 order by coalesce(array_position(array['err', 'warn', 'info', 'ok', 'muted'], b.state->>'tone'), 6),
          b.item_updated_at desc nulls last, b.entity_id
 limit $3
"#
);

/// One entity, deleted or not.
///
/// The join is `sync.live_item`'s minus **both** of its filters, deliberately:
/// §5a says a withdrawn entity must still open, and since migration `0012` a
/// disabled source's must too -- links and notes point at either. Each dropped
/// filter leaves its marker instead: `e.deleted_at` for the withdrawn banner,
/// `coalesce(c.enabled, true)` for the turned-off one (issue #204).
/// `source_config` is a **left** join because `run_once` syncs unconfigured
/// sources.
const DETAIL: &str = concat!(
    r#"
select i.entity_id, i.source_id, i.kind, i.title, "#,
    knobas_core::body_text_shown!("i.body_text"),
    r#" as body_text, i.author,
       i.item_updated_at, i.synced_at, i.payload, i.web_url,
       "#,
    knobas_core::path_line_read!("$2"),
    r#" as path,
       "#,
    knobas_core::declared_state!("$2"),
    r#" as state,
       "#,
    knobas_core::declared_string!("$2", "commit"),
    r#" as commit,
       "#,
    knobas_core::declared_string!("$2", "status_name"),
    r#" as status,
       "#,
    knobas_core::declared_string!("$2", "issue_type"),
    r#" as issue_type,
       "#,
    knobas_core::declared_string!("$2", "priority"),
    r#" as priority,
       "#,
    knobas_core::declared_integer!("$2", "estimate"),
    r#" as estimate_seconds,
       e.deleted_at,
       c.display_name, c.kind as adapter_kind,
       coalesce(c.enabled, true) as source_enabled
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
  left join knobas.source_config c on c.id = i.source_id
 where i.entity_id = $1
"#
);

/// Everything the slide-over needs for `entity_id`, in four round trips and,
/// for an entity that stands at a commit, a fifth: the source listing the
/// declarations come from, the row (with its declared commit), its links, its
/// history and the builds of that commit (#710).
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `entity_id` is not an entity
/// id -- which is how a mistyped deep link reports itself rather than as a
/// 500; [`NotFound`](crate::IpcErrorCode::NotFound) if nothing carries it;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn get_entity_inner(pool: &PgPool, entity_id: &str) -> Result<EntityDetail, IpcError> {
    // First, and before any query: `#/ticket/not-an-id` is a bad address, not
    // a missing entity, and the two want different words on screen.
    let entity = EntityRef::parse(entity_id).map_err(IpcError::invalid)?;

    // The declarations the path line reads the parent through (#596), as a
    // room's line reads them: the panel and the row draw one path.
    let declarations = declared_paths(pool).await?;
    let row = sqlx::query(DETAIL)
        .bind(entity.to_string())
        .bind(declarations.as_param())
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("{entity} is not in the mirror")))?;

    let source_id: String = row.get("source_id");
    let display_name: Option<String> = row.get("display_name");
    let adapter_kind: Option<String> = row.get("adapter_kind");

    let kind: String = row.get("kind");
    // The board draws live tickets only (`sync.live_item`); the detail opens
    // withdrawn ones too, and says nothing of a status the board would not.
    let status: Option<String> = row.get("status");
    let status = status.filter(|_| {
        kind == "ticket"
            && row.get::<Option<DateTime<Utc>>, _>("deleted_at").is_none()
            && row.get::<bool, _>("source_enabled")
    });
    let kind_info = kind_info_for(adapter_kind.as_deref(), &kind);

    // The builds of the commit this entity stands at, asked only when it
    // stands at one (see [`DETAIL_BUILDS_AT`] for why).
    let commit: Option<String> = row.get("commit");
    let builds = match commit {
        Some(sha) => sqlx::query(DETAIL_BUILDS_AT)
            .bind(sha)
            .bind(declarations.as_param())
            .bind(DETAIL_BUILDS)
            .fetch_all(pool)
            .await?
            .iter()
            .map(entity_row)
            .collect::<Result<_, _>>()?,
        None => Vec::new(),
    };

    Ok(EntityDetail {
        row: EntityRow {
            entity_id: row.get("entity_id"),
            kind,
            source_id: source_id.clone(),
            title: row.get("title"),
            updated_at: row.get("item_updated_at"),
            synced_at: row.get("synced_at"),
            // The detail panel draws the same path the launcher row does, out
            // of the same statement -- the panel *has* the payload, but
            // reading it here would be a second spelling of "where is this",
            // and ADR-0007 requirement 2 exists to stop exactly that.
            path: row.get("path"),
            state: state_of(&row)?,
        },
        source: SourceRef {
            display_name: display_name.unwrap_or_else(|| source_id.clone()),
            adapter_kind: adapter_kind.unwrap_or_else(|| source_id.clone()),
            id: source_id,
            enabled: row.get("source_enabled"),
        },
        kind_info,
        body_text: row.get("body_text"),
        author: row.get("author"),
        payload: row.get("payload"),
        web_url: row.get("web_url"),
        deleted_at: row.get("deleted_at"),
        links: knobas_core::link::entries_of(pool, &entity).await?,
        activity: knobas_core::activity::recent(pool, DETAIL_ACTIVITY, Some(&entity)).await?,
        builds,
        status,
        issue_type: row.get("issue_type"),
        priority: row.get("priority"),
        estimate_seconds: row.get("estimate_seconds"),
    })
}

/// What the adapter declares about one kind, if anything does.
///
/// §3a: *"entity kinds with display metadata drive grouping, chips and
/// labels"*. Looked up by the **adapter** kind and not the instance id, because
/// `jira` and `jira-eu` are two instances of one adapter that declare the same
/// kinds (P10).
///
/// `None` in three cases, and all three are the same answer to the frontend:
/// no configuration row for the source (`run_once` produces exactly that), no
/// adapter of that kind compiled in, and an adapter that simply does not
/// declare this kind. §3a's whole point is that an undeclared kind still
/// renders, so the humaniser on the other side is the designed path rather
/// than a degradation.
///
/// **First declaration wins**, deterministically: `ADAPTERS` is a fixed table
/// in a fixed order, so two adapters declaring one kind id resolve the same
/// way on every call rather than by whichever the iterator reached first this
/// time.
fn kind_info_for(adapter_kind: Option<&str>, kind: &str) -> Option<knobas_source::KindInfo> {
    let adapter_kind = adapter_kind?;
    crate::sources::Registry::builtin()
        .templates()
        .into_iter()
        .find(|template| template.adapter_kind == adapter_kind)?
        .entity_kinds
        .into_iter()
        .find(|info| info.id == kind)
}

/// One entity, for the detail slide-over.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, and whatever [`get_entity_inner`] refuses
/// with.
#[tauri::command]
pub async fn get_entity(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<EntityDetail, IpcError> {
    let pool = lifecycle.pool()?;
    get_entity_inner(&pool, &entity_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §3a, as a resolution rather than as a claim.
    ///
    /// The whole promise is *"a new source's items get grouped, chipped and
    /// labeled without touching core"*, and it holds only if this lookup
    /// actually reaches the adapter's own declaration. The mock declares
    /// `ticket`, so a mock ticket must come back with the mock's words --
    /// which the frontend's humaniser could not produce (it has no `plural`
    /// and no monogram to invent).
    #[test]
    fn a_declared_kind_resolves_to_the_adapters_own_metadata() {
        let mock = crate::sources::Registry::builtin()
            .templates()
            .into_iter()
            .find(|template| template.adapter_kind == "mock")
            .expect("the mock adapter is compiled in");
        let declared = mock
            .entity_kinds
            .first()
            .expect("the mock declares at least one kind")
            .clone();

        let resolved =
            kind_info_for(Some("mock"), &declared.id).expect("a kind the mock declares resolves");
        assert_eq!(resolved.id, declared.id);
        assert_eq!(resolved.label, declared.label);
        assert_eq!(resolved.plural, declared.plural);
        assert_eq!(resolved.monogram, declared.monogram);
    }

    /// Three ways to have nothing to say, and all three say nothing.
    ///
    /// The middle one is the case that matters operationally: `run_once`
    /// writes mirror rows for a source with **no configuration row**, so
    /// `adapter_kind` is genuinely `None` there and a lookup that unwrapped it
    /// would panic on a perfectly ordinary corpus.
    #[test]
    fn nothing_declared_resolves_to_none_rather_than_to_a_guess() {
        assert!(kind_info_for(Some("mock"), "no-such-kind").is_none());
        assert!(kind_info_for(None, "ticket").is_none());
        assert!(kind_info_for(Some("not-a-compiled-in-adapter"), "ticket").is_none());
    }

    /// One kind id declared by two adapters resolves by **adapter**, not by
    /// kind.
    ///
    /// `jira` and `mock` both declare `ticket` today. A lookup that searched
    /// every template for the kind would hand a Jira ticket the mock's words
    /// (or the reverse) depending only on table order -- which is exactly the
    /// per-adapter table §3a exists to avoid, built by accident.
    #[test]
    fn two_adapters_declaring_one_kind_do_not_fight() {
        let templates = crate::sources::Registry::builtin().templates();
        let declaring: Vec<&knobas_source::SourceDescriptor> = templates
            .iter()
            .filter(|template| template.entity_kinds.iter().any(|info| info.id == "ticket"))
            .collect();
        assert!(
            declaring.len() >= 2,
            "only {} adapter(s) declare `ticket`, so this test proves nothing",
            declaring.len()
        );

        for template in declaring {
            let resolved = kind_info_for(Some(&template.adapter_kind), "ticket")
                .expect("the adapter declares `ticket`");
            let expected = template
                .entity_kinds
                .iter()
                .find(|info| info.id == "ticket")
                .expect("it is in the list this loop filtered on");
            // Field by field: `KindInfo` has no `PartialEq`, and adding one
            // would be an edit to the frozen SPI for a test's convenience.
            assert_eq!(
                (
                    resolved.label.as_str(),
                    resolved.plural.as_str(),
                    resolved.monogram.as_str(),
                    resolved.full_sync_exhaustive
                ),
                (
                    expected.label.as_str(),
                    expected.plural.as_str(),
                    expected.monogram.as_str(),
                    expected.full_sync_exhaustive
                ),
                "`ticket` from {} resolved to another adapter's declaration",
                template.adapter_kind
            );
        }
    }
}
