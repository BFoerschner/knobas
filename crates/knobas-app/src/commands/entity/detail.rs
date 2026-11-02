//! The detail view: one entity, its links, activity and builds.

use super::*;
use crate::sources::refresh::template_refreshes as refreshes;

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
    /// Whether the item's adapter reads one item **of its kind** by id
    /// (#2107, #2146, ADR-0031): its descriptor declares `Capability::Refresh`
    /// and the kind's `KindInfo::refreshes`. The detail draws
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
    /// miss. The ticket page's card draws it again on the Logged row (#2403).
    pub estimate_seconds: Option<i64>,
    /// A ticket's labels through the declared `labels` path (#2400), the
    /// ticket page card's *Labels* row; the mini board reads them by the same
    /// rule. `None`, absent on the wire, where there are none, so the row is
    /// not drawn (ADR-0007); never an empty list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub labels: Option<Vec<String>>,
    /// A ticket's attachment list through the declared `attachments` path
    /// (#2404): the ticket page's Attachments section. The list, never the
    /// bytes. `None`, absent on the wire, where there are none, so no section
    /// is drawn (ADR-0007); never an empty list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub attachments: Option<Vec<knobas_core::payload::Attachment>>,
    /// A repository's default branch as its source declares it (#2307),
    /// through the kind's `default_branch` path: what the repository page
    /// draws its tabs on before the branch list answers. Absent on the wire
    /// where nothing is declared or the record misses it (ADR-0007).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default_branch: Option<String>,
    /// How the tree of this kind's view names its sections by path (#2298),
    /// resolved from the adapter registry by the row's `adapter_kind` as
    /// [`kind_info`](Self::kind_info) is: what a deep file link reads before
    /// any listing names its sections (#2244). Absent on the wire for a kind
    /// that draws no tree or an adapter that declares none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tree: Option<knobas_source::view::TreeSections>,
    /// A ticket's description as its source wrote it (#2308), through the
    /// declared `description` path; untrusted text. `None` where nothing is
    /// declared or the record misses it: the reader then draws no
    /// description. Not [`body_text`](Self::body_text), the search text.
    pub description: Option<String>,
    /// How the source writes this kind's prose (#2308), as its kind declares:
    /// the shell draws `markdown` and `jira_wiki` with its Markdown component
    /// and `text` as text. `text` where nothing is declared.
    pub markup: knobas_source::Markup,
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
        tree: detail
            .configured_adapter_kind
            .as_deref()
            .and_then(|adapter| registry.tree_sections(adapter, &detail.row.kind)),
        refreshes: refreshes(
            registry,
            detail.configured_adapter_kind.as_deref(),
            &detail.row.kind,
        ),
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
        labels: Some(detail.labels).filter(|labels| !labels.is_empty()),
        attachments: Some(detail.attachments).filter(|files| !files.is_empty()),
        default_branch: detail.default_branch,
        description: detail.description,
        markup: detail.markup,
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

// -- an attachment's bytes (spec #2396, #2405) -------------------------------

/// What [`attachment_bytes`] answers: the file's media type and its bytes in
/// base64, which the shell draws as a `data:` image (the webview loads
/// nothing from the source itself).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct AttachmentBytes {
    /// The media type the mirror's list gives the file, else
    /// `application/octet-stream`.
    pub mime: String,
    /// The whole file, base64 (standard alphabet, padded).
    pub data: String,
}

/// The attachment `attachment_id` of `entity_id` as the mirror lists it, and
/// the entity's live source. Only a file the list holds is ever fetched, so
/// the address the source is asked for is one it emitted itself.
async fn listed_attachment(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    attachment_id: &str,
) -> Result<
    (
        knobas_core::payload::Attachment,
        std::sync::Arc<dyn knobas_source::Source>,
        SourceRef,
    ),
    IpcError,
> {
    let detail = get_entity_inner(pool, registry.as_ref(), entity_id).await?;
    let file = detail
        .attachments
        .unwrap_or_default()
        .into_iter()
        .find(|file| file.id == attachment_id)
        .ok_or_else(|| {
            IpcError::not_found(format!("{entity_id} lists no attachment {attachment_id}"))
        })?;
    let source = crate::sources::crud::run_source(pool, secrets, registry, live, entity_id)
        .await
        .map_err(|error| crate::sources::to_ipc(&error, Some(&detail.source.id)))?;
    Ok((file, source, detail.source))
}

/// A source's failure on an attachment read. A 403 says the account may not
/// read this one file, which is not what a dead credential says, so its
/// message names the status for the page's *not permitted* (the code stays
/// `unauthorized`, as every 401 and 403 is).
fn attachment_failed(
    source: &SourceRef,
    file: &knobas_core::payload::Attachment,
    error: knobas_source::SourceError,
) -> IpcError {
    match error {
        knobas_source::SourceError::Unauthorized { status: Some(403) } => IpcError::new(
            crate::IpcErrorCode::Unauthorized,
            format!(
                "{} does not permit this account to read {} (HTTP 403)",
                source.display_name, file.name
            ),
        )
        .with_source(source.id.clone()),
        other => crate::sources::to_ipc(&other.into(), Some(&source.id)),
    }
}

/// [`attachment_bytes`]'s body, reachable from a test.
///
/// # Errors
///
/// `invalid` for an id that is not an entity id, a file over
/// [`ATTACHMENT_CAP`](knobas_source::view::ATTACHMENT_CAP) (its size and the
/// cap in the message; one the mirror knows is too big is refused before the
/// source is asked) and a source that reads no attachments; `not_found` for
/// an entity the mirror lacks or an attachment its list does not hold;
/// `unauthorized` (a 403 naming `HTTP 403`) and `unreachable` as the source
/// answers.
pub async fn attachment_bytes_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    attachment_id: &str,
) -> Result<AttachmentBytes, IpcError> {
    use base64::Engine as _;
    use knobas_source::view::{ATTACHMENT_CAP, FileBytes};
    let too_large = |name: &str, size: u64| {
        IpcError::invalid(format!(
            "{name} is {}; knobas previews an attachment up to {}",
            mebibytes(size),
            mebibytes(ATTACHMENT_CAP)
        ))
    };
    let (file, source, from) =
        listed_attachment(pool, secrets, registry, live, entity_id, attachment_id).await?;
    if let Some(size) = file.size.filter(|size| *size > ATTACHMENT_CAP) {
        return Err(too_large(&file.name, size));
    }
    let read = source
        .attachment_bytes(entity_id, &file.content_url, ATTACHMENT_CAP)
        .await
        .map_err(|error| attachment_failed(&from, &file, error))?;
    match read {
        Some(FileBytes::Read(bytes)) => Ok(AttachmentBytes {
            mime: file
                .mime
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_owned()),
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
        }),
        Some(FileBytes::TooLarge { size }) => Err(too_large(&file.name, size)),
        None => Err(IpcError::invalid(format!(
            "{}'s source reads no attachments",
            from.display_name
        ))),
    }
}

/// One attachment's bytes, through the source's stored credential, for the
/// ticket page's preview (spec #2396, #2405). Read live and never stored.
///
/// # Errors
///
/// See [`attachment_bytes_inner`].
#[tauri::command]
pub async fn attachment_bytes<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    attachment_id: String,
) -> Result<AttachmentBytes, IpcError> {
    let state = crate::sources::state(&app)?;
    attachment_bytes_inner(
        &state.pool,
        &state.secrets,
        &state.registry,
        &state.live,
        &entity_id,
        &attachment_id,
    )
    .await
}

/// [`download_attachment`]'s body, reachable from a test: the file streamed
/// into a new file in `dir`, named as the list names it
/// ([`crate::downloads::safe_name`]) and numbered where the name is taken, as
/// a run's download is (#856). A download that fails leaves no file.
///
/// # Errors
///
/// As [`attachment_bytes_inner`], with no cap; a file that cannot be written
/// is `internal`.
pub async fn download_attachment_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    attachment_id: &str,
    dir: &std::path::Path,
) -> Result<String, IpcError> {
    let (file, source, from) =
        listed_attachment(pool, secrets, registry, live, entity_id, attachment_id).await?;
    let mut sink = crate::downloads::FileSink::new(dir);
    let streamed = source
        .download_attachment(entity_id, &file.content_url, &file.name, &mut sink)
        .await;
    let finished = match streamed {
        Ok(true) => sink.finish().await.map(Some),
        Ok(false) => Ok(None),
        Err(error) => Err(error),
    };
    match finished {
        Ok(Some(path)) => Ok(path.display().to_string()),
        Ok(None) => Err(IpcError::invalid(format!(
            "{}'s source reads no attachments",
            from.display_name
        ))),
        Err(error) => {
            sink.discard().await;
            Err(attachment_failed(&from, &file, error))
        }
    }
}

/// One attachment into the downloads folder through the source's stored
/// credential (spec #2396, #2405); answers the file's path.
///
/// # Errors
///
/// See [`download_attachment_inner`], and `internal` when the system names
/// no downloads folder.
#[tauri::command]
pub async fn download_attachment<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    attachment_id: String,
) -> Result<String, IpcError> {
    use tauri::Manager;
    let dir = app
        .path()
        .download_dir()
        .map_err(|error| IpcError::internal(format!("no downloads folder: {error}")))?;
    let state = crate::sources::state(&app)?;
    download_attachment_inner(
        &state.pool,
        &state.secrets,
        &state.registry,
        &state.live,
        &entity_id,
        &attachment_id,
        &dir,
    )
    .await
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
        assert!(
            refreshes(&registry(), Some("mock"), "ticket"),
            "the mock reads one item"
        );
        // An adapter that does not declare the read, while one is compiled
        // in; the uninstalled kind below holds the "off" side either way.
        let quiet = templates.iter().find(|template| !template.refreshes());
        if let Some(quiet) = quiet {
            assert!(
                !refreshes(&registry(), Some(&quiet.adapter_kind), "ticket"),
                "{} does not declare the read",
                quiet.adapter_kind
            );
        }
        assert!(!refreshes(&registry(), None, "ticket"));
        assert!(!refreshes(
            &registry(),
            Some("not-a-compiled-in-adapter"),
            "ticket"
        ));
    }

    /// Per kind, not per adapter (#2146): TeamCity reads a build by id and
    /// answers a build configuration *not offered*, so a configuration's
    /// detail draws *Sync now* from the start, and a build's *Refresh*. A
    /// kind the adapter does not declare is not refreshed either.
    #[test]
    fn can_refresh_is_the_kinds_declaration() {
        assert!(refreshes(&registry(), Some("teamcity"), "build"));
        assert!(!refreshes(&registry(), Some("teamcity"), "build_config"));
        assert!(refreshes(&registry(), Some("gitea"), "pr"));
        for kind in ["repo", "branch", "commit"] {
            assert!(!refreshes(&registry(), Some("gitea"), kind), "gitea {kind}");
        }
        assert!(!refreshes(&registry(), Some("jira"), "jira_view"));
        assert!(!refreshes(&registry(), Some("mock"), "no-such-kind"));
    }

    /// The `?fake-ipc` fixture's `NOT_REFRESHED` (`fake/entity.ts`) is a
    /// hand copy of the kinds each refreshing template declares not
    /// refreshed (#2146): it lists exactly those, adapter by adapter, so a
    /// flag flipped on one side fails here rather than drawing the other
    /// button in the fixture only.
    #[test]
    fn the_fake_ipcs_not_refreshed_kinds_are_the_descriptors() {
        use std::collections::{BTreeMap, BTreeSet};
        let fixture = include_str!("../../../../../app/src/lib/shell/dev/fake/entity.ts");
        let header = "const NOT_REFRESHED: Readonly<Record<string, ReadonlySet<string>>> = {";
        let start = fixture
            .find(header)
            .unwrap_or_else(|| panic!("`{header}` is not in fake/entity.ts"))
            + header.len();
        let body = &fixture[start..];
        let body = &body[..body.find("\n};").expect("NOT_REFRESHED is never closed")];
        let fake: BTreeMap<String, BTreeSet<String>> = body
            .lines()
            .filter_map(|line| line.trim().split_once(": new Set(["))
            .map(|(adapter, kinds)| {
                let kinds = kinds.split("])").next().unwrap_or_default();
                let kinds = kinds
                    .split(',')
                    .map(|kind| kind.trim().trim_matches('"').to_owned())
                    .filter(|kind| !kind.is_empty())
                    .collect();
                (adapter.to_owned(), kinds)
            })
            .collect();
        assert!(
            !fake.is_empty(),
            "no entries read from NOT_REFRESHED:\n{body}"
        );
        let declared: BTreeMap<String, BTreeSet<String>> = registry()
            .templates()
            .into_iter()
            .filter(knobas_source::SourceDescriptor::refreshes)
            .map(|template| {
                let kinds = template
                    .entity_kinds
                    .iter()
                    .filter(|info| !info.refreshes)
                    .map(|info| info.id.clone())
                    .collect::<BTreeSet<_>>();
                (template.adapter_kind, kinds)
            })
            .filter(|(_, kinds)| !kinds.is_empty())
            .collect();
        assert_eq!(
            fake, declared,
            "app/src/lib/shell/dev/fake/entity.ts's NOT_REFRESHED is no longer the kinds the \
             refreshing templates declare `refreshes: false`"
        );
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
