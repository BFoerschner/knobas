//! The detail view: one entity, its links, activity and builds.

use super::*;

// -- the detail view --------------------------------------------------------

pub use knobas_core::detail::SourceRef;

/// Everything the slide-over draws for one entity (interfaces §2.5).
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
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
    /// Whether the item's adapter reads one item by id (#2107, ADR-0031):
    /// its descriptor declares `Capability::Refresh`. The detail draws
    /// *Refresh* where it does and *Sync now* where it does not, both only
    /// while [`SourceRef::enabled`]. Off the adapter's template, so the read
    /// builds no source; `false` for a source with no configuration row or
    /// an adapter not installed. An instance that answers *not offered* all
    /// the same gets the run in its place.
    pub refreshes: bool,
    /// Untrusted source text. Rendered as text, never as markup (gotcha 7).
    pub body_text: String,
    pub author: Option<String>,
    /// The source record **verbatim** (§3a): what the generic detail view
    /// projects. It is untrusted text all the way down.
    ///
    /// `unknown` in the shell, per §2.6's `serde_json::Value` mapping: project
    /// it as text, never as markup (interfaces §2.5, gotcha 7).
    #[ts(type = "unknown")]
    pub payload: serde_json::Value,
    /// Where the item lives in its own system, or `None` when the adapter
    /// reported no page -- in which case *Open in browser* is absent (P5).
    pub web_url: Option<String>,
    /// Set when the source withdrew the entity. The mirror row survives, so
    /// links and notes still resolve (§5a).
    ///
    /// RFC 3339 on the wire, `None` while the source still has it.
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

/// Everything the slide-over needs for `entity_id`: the source listing the
/// declarations come from, then [`knobas_core::detail::read`], completed with
/// what the adapter declares about its kind ([`kind_info_for`]).
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `entity_id` is not an entity
/// id -- which is how a mistyped deep link reports itself rather than as a
/// 500; [`NotFound`](crate::IpcErrorCode::NotFound) if nothing carries it;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn get_entity_inner(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    entity_id: &str,
) -> Result<EntityDetail, IpcError> {
    // First, and before any query: `#/ticket/not-an-id` is a bad address, not
    // a missing entity, and the two want different words on screen.
    let entity = EntityRef::parse(entity_id).map_err(IpcError::invalid)?;
    let declarations = declared_paths(pool, registry).await?;
    let detail = knobas_core::detail::read(pool, &declarations, &entity)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("{entity} is not in the mirror")))?;
    Ok(EntityDetail {
        kind_info: kind_info_for(
            registry,
            detail.configured_adapter_kind.as_deref(),
            &detail.row.kind,
        ),
        refreshes: refreshes(registry, detail.configured_adapter_kind.as_deref()),
        row: detail.row,
        source: detail.source,
        body_text: detail.body_text,
        author: detail.author,
        payload: detail.payload,
        web_url: detail.web_url,
        deleted_at: detail.deleted_at,
        links: detail.links,
        activity: detail.activity,
        builds: detail.builds,
        status: detail.status,
        issue_type: detail.issue_type,
        priority: detail.priority,
        estimate_seconds: detail.estimate_seconds,
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
/// **First declaration wins**, deterministically: `registry` lists its
/// adapters in a fixed order, so two adapters declaring one kind id resolve
/// the same way on every call rather than by whichever the iterator reached
/// first this time.
fn kind_info_for(
    registry: &dyn AdapterRegistry,
    adapter_kind: Option<&str>,
    kind: &str,
) -> Option<knobas_source::KindInfo> {
    let adapter_kind = adapter_kind?;
    registry
        .descriptors()
        .into_iter()
        .find(|template| template.adapter_kind == adapter_kind)?
        .entity_kinds
        .into_iter()
        .find(|info| info.id == kind)
}

/// Whether `adapter_kind`'s template declares the read of one item: the
/// detail's [`EntityDetail::refreshes`].
fn refreshes(registry: &dyn AdapterRegistry, adapter_kind: Option<&str>) -> bool {
    adapter_kind.is_some_and(|kind| {
        registry
            .descriptors()
            .iter()
            .any(|template| template.adapter_kind == kind && template.refreshes())
    })
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
    get_entity_inner(&pool, lifecycle.registry(), &entity_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> crate::sources::Registry {
        crate::sources::Registry::for_process()
    }

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
        let mock = crate::sources::Registry::for_process()
            .templates()
            .into_iter()
            .find(|template| template.adapter_kind == "mock")
            .expect("the mock adapter is compiled in");
        let declared = mock
            .entity_kinds
            .first()
            .expect("the mock declares at least one kind")
            .clone();

        let resolved = kind_info_for(&registry(), Some("mock"), &declared.id)
            .expect("a kind the mock declares resolves");
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
        assert!(kind_info_for(&registry(), Some("mock"), "no-such-kind").is_none());
        assert!(kind_info_for(&registry(), None, "ticket").is_none());
        assert!(kind_info_for(&registry(), Some("not-a-compiled-in-adapter"), "ticket").is_none());
    }

    /// The detail's *can refresh* is the adapter's own declaration, read off
    /// its template: on for an adapter that declares `Capability::Refresh`,
    /// off for one that does not, and off with no adapter to ask.
    #[test]
    fn can_refresh_is_the_adapters_declaration() {
        let templates = registry().templates();
        assert!(refreshes(&registry(), Some("mock")), "the mock reads one item");
        let quiet = templates.iter().find(|template| !template.refreshes());
        if let Some(quiet) = quiet {
            assert!(
                !refreshes(&registry(), Some(&quiet.adapter_kind)),
                "{} does not declare the read",
                quiet.adapter_kind
            );
        }
        assert!(!refreshes(&registry(), None));
        assert!(!refreshes(&registry(), Some("not-a-compiled-in-adapter")));
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
        let templates = crate::sources::Registry::for_process().templates();
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
            let resolved = kind_info_for(&registry(), Some(&template.adapter_kind), "ticket")
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
