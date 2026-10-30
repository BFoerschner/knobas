//! The contract battery: the shared test suite every adapter must pass.

use knobas_spi_types::entity::EntityRef;

/// Which failure an adapter under test should simulate.
///
/// The battery builds a fresh adapter per case, so a factory can honour this by
/// storing it and branching on it. A faulted adapter must report the mapped
/// error from **both** [`test_connection`](crate::Source::test_connection) and
/// [`sync`](crate::Source::sync): credentials are validated once when the
/// source is added and expire later, so the classification that actually
/// reaches the user is usually the one raised mid-sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    None,
    Unauthorized,
    Unreachable,
}

/// An in-memory [`Sink`](crate::Sink) that keeps everything it is handed.
pub struct VecSink(pub Vec<crate::SyncItem>);

#[async_trait::async_trait]
impl crate::Sink for VecSink {
    async fn item(&mut self, item: crate::SyncItem) -> Result<(), crate::SourceError> {
        self.0.push(item);
        Ok(())
    }
}

/// An in-memory [`Sink`](crate::Sink) that keeps the items and every
/// [`walked`](crate::Sink::walked) reading, in the order they arrived, and
/// every kind the run said it [walked whole](crate::Sink::walked_whole).
///
/// `readings_before_item[i]` is how many readings had arrived when item `i`
/// did, so a test can say where in the walk an item came, not only that it did.
#[derive(Default)]
pub struct Walking {
    pub items: Vec<crate::SyncItem>,
    pub readings: Vec<(u64, Option<u64>, String)>,
    pub readings_before_item: Vec<usize>,
    pub whole: Vec<String>,
}

#[async_trait::async_trait]
impl crate::Sink for Walking {
    async fn item(&mut self, item: crate::SyncItem) -> Result<(), crate::SourceError> {
        self.readings_before_item.push(self.readings.len());
        self.items.push(item);
        Ok(())
    }

    async fn walked(&mut self, done: u64, of: Option<u64>, unit: &str) {
        self.readings.push((done, of, unit.to_owned()));
    }

    async fn walked_whole(&mut self, kind: &str) {
        self.whole.push(kind.to_owned());
    }
}

/// A sink that rejects the very first item, so the battery can check that an
/// adapter propagates the failure instead of syncing on regardless.
struct FailingSink;

#[async_trait::async_trait]
impl crate::Sink for FailingSink {
    async fn item(&mut self, _item: crate::SyncItem) -> Result<(), crate::SourceError> {
        Err(crate::SourceError::Sink("sink is down".into()))
    }
}

/// One probe value per [`WriteOp`](crate::WriteOp) variant, each paired with
/// its own [`WriteOp::identifier`](crate::WriteOp::identifier) -- so the two
/// can never disagree.
///
/// This is what lets the battery call `write` with an op the adapter did not
/// declare, and what makes a typo'd identifier in a descriptor a test failure
/// rather than an action the UI silently never renders.
///
/// The list is exhaustive by construction: a new `WriteOp` variant stops
/// `WriteOp::identifier` compiling, and adding it there is the moment to add
/// its probe value here.
fn known_write_ops(src_id: &str) -> Vec<(&'static str, crate::WriteOp)> {
    // Every target is namespaced to the source under test and named after this
    // battery, so an adapter that (wrongly) attempted one would 404 rather than
    // touch anything real.
    let target = format!("{src_id}:contract-battery");
    [
        crate::WriteOp::Comment {
            entity: target.clone(),
            body: "contract battery probe".into(),
        },
        crate::WriteOp::Transition {
            entity: target.clone(),
            status: "contract-battery".into(),
        },
        crate::WriteOp::CreateTicket {
            entity: target.clone(),
            title: "contract battery probe".into(),
            body: "contract battery probe".into(),
            ticket_type: "contract-battery".into(),
        },
        crate::WriteOp::CreateBranch {
            entity: target.clone(),
            name: "contract-battery".into(),
            from_ref: "contract-battery".into(),
        },
        crate::WriteOp::CreatePullRequest {
            entity: target.clone(),
            title: "contract battery probe".into(),
            body: "contract battery probe".into(),
            head: "contract-battery".into(),
            base: "contract-battery".into(),
        },
        crate::WriteOp::Approve {
            entity: target.clone(),
            body: "contract battery probe".into(),
        },
        crate::WriteOp::TriggerBuild {
            entity: target.clone(),
        },
        crate::WriteOp::RerunBuild {
            entity: target.clone(),
        },
        crate::WriteOp::LogWork {
            entity: target.clone(),
            // A fixed instant, not `Utc::now()`: an adapter that (wrongly)
            // attempted this probe would otherwise write a different worklog
            // on every run, and a battery whose payload moves is one whose
            // failures cannot be compared between runs.
            started: chrono::DateTime::from_timestamp(1_788_000_000, 0).expect("a fixed instant"),
            seconds: 60,
            comment: "contract battery probe".into(),
        },
        crate::WriteOp::CreatePage {
            // The parent is the probe target like every other container here,
            // so an adapter that (wrongly) attempted this would 404 on a page
            // id that does not exist rather than create a page in somebody's
            // wiki. The space key is the battery's own name for the same
            // reason: no instance has a space called this.
            parent: target.clone(),
            space: "CONTRACTBATTERY".into(),
            title: "contract battery probe".into(),
            body: "<p>contract battery probe</p>".into(),
        },
        crate::WriteOp::UpdatePage {
            entity: target.clone(),
            // Version 0 exists on no Confluence page -- the first version is
            // 1 -- so an adapter that attempted this probe would be refused by
            // the server rather than overwrite a real page's body.
            base_version: 0,
            body: "<p>contract battery probe</p>".into(),
        },
        // The battery's own target is not a monitor id -- Uptime Kuma's are
        // integers -- so an adapter that attempted either of these against a
        // real instance is refused before anything is silenced. That matters
        // more here than elsewhere in this list: pausing a monitor is the one
        // probe whose success would be *invisible*, since a paused monitor
        // simply stops being published.
        crate::WriteOp::PauseMonitor {
            entity: target.clone(),
        },
        crate::WriteOp::ResumeMonitor {
            entity: target.clone(),
        },
        // Not a **create target** either, for the same reason and one more:
        // `create_monitor` addresses the source itself
        // (`crate::monitor_target`), and this is deliberately not that string.
        // A create is the one probe here whose success would leave something
        // behind -- a monitor knobas then reports and nobody asked for -- so
        // the battery aims it at a container no source has.
        crate::WriteOp::CreateMonitor {
            entity: target.clone(),
            name: "contract battery probe".into(),
            url: "http://127.0.0.1:1/contract-battery".into(),
            kind: crate::MonitorKind::Http,
        },
        // A merge is the probe whose success could not be taken back: a merge
        // commit on somebody's base branch. The target is no pull request's
        // address -- it has no `#<number>` -- so an adapter that attempted it
        // anyway is refused on the id's shape before a request leaves.
        crate::WriteOp::MergePullRequest {
            entity: target.clone(),
            style: crate::MergeStyle::Merge,
            delete_branch: false,
            message: None,
        },
        // An update rewrites or extends somebody's head branch. Aimed, as the
        // merge is, at no pull request's address, so it is refused on the
        // id's shape before a request leaves.
        crate::WriteOp::UpdatePullRequestBranch {
            entity: target.clone(),
            style: crate::UpdateStyle::Merge,
        },
        // A parameter set on a real configuration would change what its next
        // build runs with. The target is no build configuration's address, so
        // an adapter that attempted it anyway is refused on the id's shape.
        crate::WriteOp::SetParameter {
            entity: target.clone(),
            name: "contract.battery.probe".into(),
            value: "contract battery probe".into(),
            base: None,
        },
        // A review would be a verdict on somebody's pull request. The target
        // is no pull request's address -- it has no `#<number>` -- so an
        // adapter that attempted it anyway is refused on the id's shape.
        crate::WriteOp::SubmitReview {
            entity: target.clone(),
            verdict: crate::ReviewVerdict::Comment,
            body: "contract battery probe".into(),
            comments: vec![],
        },
        // An edit would rename somebody's pull request. The target is no pull
        // request's address -- it has no `#<number>` -- so an adapter that
        // attempted it anyway is refused on the id's shape; and it changes
        // only a title, never a state, so even that would close nothing.
        crate::WriteOp::EditPullRequest {
            entity: target.clone(),
            title: Some("contract battery probe".into()),
            body: None,
            state: None,
        },
        // A run would be a real build on somebody's agent. The target is no
        // build configuration's address, so an adapter that attempted it
        // anyway is refused on the id's shape before a request leaves.
        crate::WriteOp::RunBuild {
            entity: target.clone(),
            branch: Some("contract-battery".into()),
            parameters: vec![crate::BuildParameter {
                name: "contract.battery.probe".into(),
                value: "contract battery probe".into(),
            }],
            clean_checkout: false,
            first_in_queue: false,
        },
        // A commit would put a file on somebody's branch. The target is no
        // repository's address -- it has no `owner/` -- so an adapter that
        // attempted it anyway is refused on the id's shape; and its base is no
        // blob's, so even a request that left would not land.
        crate::WriteOp::CommitFile {
            repo: target.clone(),
            branch: "contract-battery".into(),
            new_branch: None,
            path: "contract-battery.txt".into(),
            content: "contract battery probe".into(),
            message: "contract battery probe".into(),
            base_sha: "contract-battery".into(),
        },
        // A source action (#1194) under an id this battery names, which no
        // adapter declares, aimed at no entity: refused by its id.
        crate::WriteOp::SourceAction {
            entity: target.clone(),
            id: UNDECLARED_SOURCE_ACTION.into(),
            fields: std::collections::BTreeMap::new(),
        },
        // A delete could not be taken back. The target is no branch's
        // address -- it has no `@refs/heads/` -- so an adapter that attempted
        // it anyway is refused on the id's shape before a request leaves.
        crate::WriteOp::DeleteBranch { entity: target },
    ]
    .into_iter()
    .map(|op| (op.identifier(), op))
    .collect()
}

/// The id clause 11 asks an adapter to perform as a source action, which it
/// must not declare. Named after the battery, so no adapter's real action
/// is called this.
const UNDECLARED_SOURCE_ACTION: &str = "contract_battery_probe";

/// Every adapter must pass. Panics with a descriptive message on violation.
///
/// `make` builds a fresh adapter simulating the requested [`Fault`]; the
/// battery calls it once per case rather than mutating one instance.
pub async fn battery<F>(make: F)
where
    F: Fn(Fault) -> Box<dyn crate::Source>,
{
    // 1. Full sync yields at least one item, all ids well-formed and namespaced to the source.
    let s = make(Fault::None);
    let d = s.descriptor();
    let src_id = d.id.clone();
    assert!(
        !src_id.trim().is_empty(),
        "descriptor.id must not be blank -- it is the namespace of every item this source emits"
    );
    assert!(
        !knobas_spi_types::entity::is_reserved_namespace(&src_id),
        "descriptor.id {src_id:?} is a namespace knobas keeps for its own entities \
         ({:?}) -- the sync engine refuses such a source outright, so an adapter named this \
         certifies here and then fails every real run",
        knobas_spi_types::entity::RESERVED_NAMESPACES
    );
    // §4.1: the id is the entity namespace, immutable once chosen, and typed
    // by a human into the Add-source form -- so it is a slug, not free text.
    // Certifying here is the difference between a red test in the adapter's
    // own suite and a source that installs and can never be renamed out of
    // its mistake.
    if let Err(error) = crate::instance::validate_instance_id(&src_id) {
        panic!("descriptor.id {src_id:?} is not a usable instance id: {error}");
    }
    let mut sink = VecSink(Vec::new());
    let cursor = s
        .sync(None, &mut sink)
        .await
        .expect("full sync must succeed");
    assert!(!sink.0.is_empty(), "full sync yielded no items");
    let declared: std::collections::HashSet<String> =
        d.entity_kinds.iter().map(|k| k.id.clone()).collect();
    for it in &sink.0 {
        assert_eq!(
            it.entity.namespace, src_id,
            "item {} not namespaced to source",
            it.entity
        );
        // `EntityRef::new` does not validate, so an adapter can hand out an id
        // that no longer parses -- and Task 8 stores ids as text and reads them
        // back through `EntityRef::parse`. Round-tripping through the parser is
        // the check that matches how the id is actually used downstream.
        let id = it.entity.to_string();
        assert!(
            EntityRef::parse(&id).ok().as_ref() == Some(&it.entity),
            "item id {id:?} does not survive a round trip through EntityRef::parse: \
             namespace {:?} and key {:?} must both be non-blank, and the namespace must not \
             contain ':'",
            it.entity.namespace,
            it.entity.key
        );
        assert!(
            declared.contains(&it.kind),
            "item {} has kind {:?} not declared in descriptor.entity_kinds",
            it.entity,
            it.kind
        );
    }
    // 2. Incremental sync from the returned cursor yields no items when nothing
    //    changed, and hands the same cursor straight back.
    let mut sink2 = VecSink(Vec::new());
    let idle = s
        .sync(Some(cursor.clone()), &mut sink2)
        .await
        .expect("incremental sync must succeed");
    assert!(
        sink2.0.is_empty(),
        "incremental sync after no changes must be empty"
    );
    // The engine decides whether a run is worth an activity line by comparing
    // the cursor it handed in with the one that comes back. An adapter that
    // returns a fresh cursor for a run that fetched nothing -- a timestamp of
    // "now", say -- makes every idle poll look like a change, and a
    // five-minute scheduler then writes 288 "synced nothing" lines a day per
    // source.
    assert_eq!(
        idle, cursor,
        "an incremental sync that emitted nothing must return the cursor it was given, not a \
         new one"
    );
    // 3. A healthy adapter connects, and says what it reached. Everything the
    //    Add-source flow renders comes from here (P4), and an adapter that
    //    fails its own happy path would otherwise only be caught by `sync`.
    let healthy = s.test_connection().await;
    assert!(
        healthy.is_ok(),
        "a healthy adapter's test_connection must succeed, got {healthy:?}"
    );
    //    Auth failure maps to Unauthorized, connectivity failure to Unreachable.
    let unauthorized = make(Fault::Unauthorized).test_connection().await;
    assert!(
        matches!(unauthorized, Err(crate::SourceError::Unauthorized { .. })),
        "test_connection must map an auth failure to SourceError::Unauthorized, got \
         {unauthorized:?}"
    );
    let unreachable = make(Fault::Unreachable).test_connection().await;
    assert!(
        matches!(unreachable, Err(crate::SourceError::Unreachable(_))),
        "test_connection must map a connectivity failure to SourceError::Unreachable, got \
         {unreachable:?}"
    );
    // 4. The same mapping applies mid-sync, which is where an expired credential
    //    actually surfaces -- test_connection ran once, when the source was added.
    let unauthorized = make(Fault::Unauthorized)
        .sync(None, &mut VecSink(Vec::new()))
        .await;
    assert!(
        matches!(unauthorized, Err(crate::SourceError::Unauthorized { .. })),
        "sync must map an auth failure to SourceError::Unauthorized, got {unauthorized:?}"
    );
    let unreachable = make(Fault::Unreachable)
        .sync(None, &mut VecSink(Vec::new()))
        .await;
    assert!(
        matches!(unreachable, Err(crate::SourceError::Unreachable(_))),
        "sync must map a connectivity failure to SourceError::Unreachable, got {unreachable:?}"
    );
    // 5. Write ops the descriptor does not declare are refused, not attempted.
    //    The write queue's flush and the WASM host refuse one before it
    //    reaches any adapter (`SourceDescriptor::declares`, #1950). Over a
    //    compiled-in adapter this holds it to its own refusal, the defence
    //    behind the flush's; over a WASM guest it is the host's refusal it
    //    sees.
    let known = known_write_ops(&src_id);
    for id in &d.write_ops {
        assert!(
            known.iter().any(|(k, _)| *k == id.as_str()),
            "descriptor.write_ops declares {id:?}, which is not a WriteOp identifier the SPI \
             knows -- the UI renders its action bar from these, so a typo here is an action that \
             silently never appears"
        );
    }
    // `Capability::Write` and `write_ops` are two signals for one fact. Left
    // unchecked they drift, and neither of the two ways they can disagree is a
    // valid state: a source that advertises writing with nothing to offer, or
    // one whose actions the UI renders while the source reads as read-only.
    let declares_write = d.capabilities.contains(&crate::Capability::Write);
    assert!(
        !declares_write || !d.write_ops.is_empty(),
        "descriptor declares Capability::Write but lists no write_ops -- there is no action for \
         the UI to offer"
    );
    assert!(
        declares_write || d.write_ops.is_empty(),
        "descriptor lists write_ops {:?} but does not declare Capability::Write -- the source \
         reads as read-only while advertising actions",
        d.write_ops
    );
    for (id, op) in known {
        if d.write_ops.iter().any(|w| w.as_str() == id) {
            // Declared: an adapter runs this battery against its real backend,
            // so performing the op would post an actual comment on a live
            // system. Covering a declared op is the adapter's own job.
            //
            // This branch is a safety property, not an assertion, so the
            // mutation sweep cannot see it -- `accepts_a_declared_write_op_without_calling_write`
            // pins it instead, with an adapter whose `write` panics if reached.
            continue;
        }
        let refused = s.write(op).await;
        assert!(
            matches!(refused, Err(crate::SourceError::Protocol { .. })),
            "write of {id:?}, which descriptor.write_ops does not declare, must be refused with \
             SourceError::Protocol, got {refused:?}"
        );
    }
    // 6. Declared payload paths (#277): the adapter says where its status,
    //    priority, assignee, reviewers, merged flag and project live, and this
    //    is what holds the declaration to the adapter's own corpus.
    check_payload_paths(&d, &sink.0);
    //    And where its config keeps its host's other clone addresses (#683):
    //    a schema that names the key declares it as an array of strings, or
    //    the app would read a shape the adapter never promised.
    assert!(
        d.config_schema
            .pointer(&format!("/properties/{}", crate::clone_urls::CONFIG_KEY))
            .is_none()
            || crate::clone_urls::declared(&d.config_schema),
        "config_schema property {:?} must be an array of strings",
        crate::clone_urls::CONFIG_KEY
    );
    // 7. A sink failure aborts the sync instead of being swallowed.
    let aborted = s.sync(None, &mut FailingSink).await;
    assert!(
        matches!(aborted, Err(crate::SourceError::Sink(_))),
        "a sink error must be propagated out of sync as SourceError::Sink, got {aborted:?}"
    );
    // 8. The reachable-transition read (#498): a source with no workflow
    //    refuses it **by name**, the way clause 5's undeclared write is
    //    refused.
    check_reachable_transitions(&*s, &d).await;
    // 9. The source view (#661, ADR-0020): every entity this adapter emitted
    //    answers the read without failing -- with a view, or with `None`.
    check_views(&*s, &sink.0).await;
    // 10. Role claims (#1193, ADR-0029): every role the descriptor claims is
    //     filled to its minimum -- its kinds declared, their paths declared
    //     (and, by clause 6, held to the records above), its reads claimed,
    //     and a code host's clone URLs answered from a typed address (#1197).
    if let Err(error) = crate::role::check(&d) {
        panic!("descriptor.roles: {error}");
    }
    // 11. Source actions (#1194, ADR-0029): declared exactly when
    //     `source_action` is, each id once, and one not declared is refused.
    check_source_actions(&*s, &d).await;
    // 12. Web-address forms (#1475): each names a declared kind and reads as
    //     the paste resolver needs it to -- see `url_forms::check`.
    if let Err(error) = crate::url_forms::check(&d) {
        panic!("descriptor.url_forms: {error}");
    }
    // 13. The refresh (ADR-0031): declared exactly when it answers, kind by
    //     kind (#2146), and each kind it answers for read back as the sync
    //     emitted it.
    check_refresh(&*s, &d, &sink.0).await;
    // 14. Code pages (#2274): each template starts with `/` and carries only
    //     its page's placeholders -- see `code_pages::check`.
    if let Some(pages) = &d.code_pages
        && let Err(error) = crate::code_pages::check(pages)
    {
        panic!("descriptor.code_pages: {error}");
    }
}

/// Clause 13: the read of one item by id ([`crate::Source::refresh`]).
///
/// Asked of **one item per kind** the sync emitted, the first of each, and of
/// the first item it emitted deleted: a refresh is a read, safe against a
/// live instance, but a corpus of thousands would be thousands of requests.
///
/// * A kind answered [`NotOffered`](crate::RefreshAnswer::NotOffered) is a
///   kind this adapter does not refresh, and is skipped, but only for a kind
///   whose [`KindInfo::refreshes`](crate::KindInfo::refreshes) says so: the
///   app reads that flag to draw *Refresh* or *Sync now* on the item's
///   detail, so a kind declared refreshed and answered `NotOffered` labels a
///   press as something it does not do (#2146). A kind declared not
///   refreshed and answered otherwise fails the same way round.
/// * A kind answered otherwise is held to the sync: the same entity, kind and
///   title, and the same value at every path the descriptor declares for the
///   kind (its parent among them). A live item answered *gone* fails, and a
///   deleted one must be *gone*.
/// * `Capability::Refresh` and the answers are two signals for one fact, held
///   to each other as `Capability::Write` is to `write_ops`: an adapter that
///   answers for a kind declares it, and one that declares it answers for at
///   least one kind its sync emitted.
async fn check_refresh(
    s: &dyn crate::Source,
    d: &crate::SourceDescriptor,
    items: &[crate::SyncItem],
) {
    let mut kinds = std::collections::HashSet::new();
    let mut asked: Vec<&crate::SyncItem> = items
        .iter()
        .filter(|item| !item.deleted && kinds.insert(item.kind.as_str()))
        .collect();
    asked.extend(items.iter().find(|item| item.deleted));
    let declares = d.refreshes();
    let mut answered = false;
    for item in asked {
        let id = item.entity.to_string();
        let answer = s.refresh(&id).await.unwrap_or_else(|error| {
            panic!("refresh of {id}, which the sync emitted, failed: {error}")
        });
        let kind = item.kind.as_str();
        if matches!(answer, crate::RefreshAnswer::NotOffered) {
            assert!(
                !d.refreshes_kind(kind),
                "the descriptor declares kind {kind:?} refreshed but refresh answered NotOffered \
                 for {id} -- its detail would draw Refresh for a press that runs the source"
            );
            continue;
        }
        assert!(
            declares,
            "refresh answers for {id} but the descriptor does not declare Capability::Refresh -- \
             the app reads that declaration to decide whether to ask"
        );
        assert!(
            d.refreshes_kind(kind),
            "refresh answers for {id} but the descriptor declares kind {kind:?} not refreshed -- \
             its detail would draw Sync now for an item the refresh reads"
        );
        answered = true;
        match answer {
            crate::RefreshAnswer::NotOffered => unreachable!("skipped above"),
            crate::RefreshAnswer::Gone => assert!(
                item.deleted,
                "refresh answered {id} gone, but the sync emitted it live"
            ),
            crate::RefreshAnswer::Item(read) => {
                assert!(
                    !item.deleted && !read.deleted,
                    "refresh of {id}, which the sync emitted deleted, must answer it gone, not \
                     as an item"
                );
                assert_eq!(
                    read.entity, item.entity,
                    "refresh of {id} answered another item"
                );
                assert_eq!(
                    read.kind, item.kind,
                    "refresh of {id} answered another kind"
                );
                assert_eq!(
                    read.title, item.title,
                    "refresh of {id} answered another title than the sync emitted"
                );
                for path in d
                    .payload_paths
                    .iter()
                    .filter(|paths| paths.kind == item.kind)
                    .flat_map(declared_paths)
                {
                    let at = |payload| knobas_spi_types::payload::at(payload, &path);
                    assert_eq!(
                        at(&read.payload),
                        at(&item.payload),
                        "refresh of {id} disagrees with the sync at the declared path {:?}",
                        path.segments()
                    );
                }
            }
        }
    }
    assert!(
        !declares || answered,
        "the descriptor declares Capability::Refresh but refresh answered NotOffered for every \
         kind the sync emitted"
    );
}

/// Clause 13 alone, over the items `source`'s own sync emitted (#2100).
///
/// For a live suite whose server moves under the rest of the battery: Uptime
/// Kuma writes a new response time on every heartbeat, so clause 2's idle
/// sync is a coin toss against the real container, and its suite runs the
/// battery against a recording. The refresh is what that recording cannot
/// witness, a monitor paused or deleted on the real server among it, so the
/// suite hands this check the batch one of its runs emitted.
pub async fn refreshes_as_it_syncs(source: &dyn crate::Source, items: &[crate::SyncItem]) {
    check_refresh(source, &source.descriptor(), items).await;
}

/// Every payload path a kind's declaration names, read off its serialized
/// form so a path added to [`crate::KindPaths`] is held by clause 13 without
/// a line here: each list of strings in it is a path, but the two lists of
/// status names. A path relative to a parent or list element is compared at
/// the root too, which is harmless: both answers are read the same way.
fn declared_paths(paths: &crate::KindPaths) -> Vec<knobas_spi_types::payload::PayloadPath> {
    fn walk(
        value: &serde_json::Value,
        key: &str,
        out: &mut Vec<knobas_spi_types::payload::PayloadPath>,
    ) {
        match value {
            serde_json::Value::Array(segments)
                if !segments.is_empty() && segments.iter().all(serde_json::Value::is_string) =>
            {
                if key != "blocked_statuses" && key != "done_statuses" {
                    out.push(knobas_spi_types::payload::PayloadPath::of(
                        segments.iter().filter_map(serde_json::Value::as_str),
                    ));
                }
            }
            serde_json::Value::Array(entries) => {
                for entry in entries {
                    walk(entry, key, out);
                }
            }
            serde_json::Value::Object(fields) => {
                for (key, value) in fields {
                    walk(value, key, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(
        &serde_json::to_value(paths).expect("a declaration serializes"),
        "",
        &mut out,
    );
    out
}

/// Clause 11: the source actions an adapter declares are the ones it
/// performs.
///
/// `source_actions` and `write_ops`' [`SOURCE_ACTION`](crate::SOURCE_ACTION)
/// are two signals for one fact, held to each other as `Capability::Write`
/// and `write_ops` are in clause 5. Each id is non-blank and declared once,
/// because a view's action names it and the queue hands it back by it.
///
/// **The undeclared side is probed, the declared side skipped**, for clause
/// 5's reason: the battery runs against a live backend. An adapter that
/// declares `source_action` is asked to perform an id it does not declare and
/// must refuse it with [`SourceError::Protocol`](crate::SourceError); one that
/// does not was already probed by clause 5. The skip is pinned by
/// `accepts_a_declared_source_action_without_performing_it`.
async fn check_source_actions(s: &dyn crate::Source, d: &crate::SourceDescriptor) {
    let declares_op = d.write_ops.iter().any(|w| w == crate::SOURCE_ACTION);
    assert!(
        declares_op || d.source_actions.is_empty(),
        "descriptor.source_actions declares {:?} but write_ops does not list {:?} -- the shell \
         draws no source action for a source that does not offer the op",
        d.source_actions,
        crate::SOURCE_ACTION
    );
    assert!(
        !declares_op || !d.source_actions.is_empty(),
        "write_ops lists {:?} but descriptor.source_actions declares none -- there is no action \
         for it to perform",
        crate::SOURCE_ACTION
    );
    let mut seen = std::collections::HashSet::new();
    for id in &d.source_actions {
        assert!(
            !id.trim().is_empty() && seen.insert(id.as_str()),
            "descriptor.source_actions has a blank or repeated id {id:?} -- a view's action and \
             the queued write name the action by it"
        );
    }
    if !declares_op
        || d.source_actions
            .iter()
            .any(|id| id == UNDECLARED_SOURCE_ACTION)
    {
        return;
    }
    let refused = s
        .write(crate::WriteOp::SourceAction {
            entity: format!("{}:contract-battery", d.id),
            id: UNDECLARED_SOURCE_ACTION.into(),
            fields: std::collections::BTreeMap::new(),
        })
        .await;
    assert!(
        matches!(refused, Err(crate::SourceError::Protocol { .. })),
        "{UNDECLARED_SOURCE_ACTION:?}, a source action descriptor.source_actions does not declare \
         must be refused with SourceError::Protocol, got {refused:?}"
    );
}

/// The data half of the role check (#1199, ADR-0028), held to a corpus that
/// should hold every kind its roles need: a full sync of `source`, and every
/// built-in role its descriptor claims **filled** on what it emitted, by
/// [`DataCheck`](crate::role::data::DataCheck), the check a source's first
/// sync runs in the engine.
///
/// Stricter than [`battery`] on purpose. The battery runs against any
/// instance and judges only the kinds a corpus populates; this one is for a
/// seeded corpus (the testenv products, an adapter's offline fixture), where
/// a role whose kinds came back empty is a role nothing showed it fills. The
/// live suites call it, which is what witnesses that the built-ins fill
/// every role they claim on real records.
///
/// # Panics
///
/// When the sync fails, a required path misses, or a required kind has no
/// record.
pub async fn fills_its_roles(source: &dyn crate::Source) {
    let d = source.descriptor();
    let mut sink = VecSink(Vec::new());
    if let Err(error) = source.sync(None, &mut sink).await {
        panic!("the full sync the role check reads failed: {error}");
    }
    let mut data = crate::role::data::DataCheck::new(&d);
    for item in &sink.0 {
        data.observe(&item.kind, &item.payload);
    }
    for verdict in data.finish() {
        if let Some(miss) = verdict.miss {
            panic!(
                "role {:?} is not filled: no {:?} record resolves {} at {}",
                miss.role, miss.kind, miss.field, miss.path
            );
        }
        assert!(
            verdict.complete,
            "role {:?} is not shown filled: the corpus holds no record of some kind it needs \
             (seen: {:?})",
            verdict.role, verdict.seen
        );
    }
}

/// Clause 9: the source view of every entity the adapter emitted is an answer,
/// never a failure.
///
/// **Every emitted entity, not a probe**, and that is the difference from
/// clauses 5 and 8: a view is a read of something that exists, so the battery
/// asks about things that do -- the items the full sync just handed over. A
/// view that fails for one of them is a detail that opens on *Showing the
/// mirror* for an entity the adapter itself named, which is the line ADR-0020
/// keeps for a source that could not be reached. `None` passes: it is an
/// adapter drawing nothing for that kind (rule 2).
///
/// Run against a live backend this makes real reads -- one per entity, which
/// is what a reader opening each detail would make -- and no write.
///
/// A drawn view is held to the two things the shell depends on: its section
/// ids are unique (a deferred read names one), and each **deferred** section
/// answers its own read with exactly that section, not deferred -- the read a
/// tab makes when it is first opened.
async fn check_views(s: &dyn crate::Source, items: &[crate::SyncItem]) {
    for item in items {
        let id = item.entity.to_string();
        let view = match s.view(&id, None).await {
            Ok(view) => view,
            Err(error) => panic!(
                "the source view of {id} (kind {:?}), an entity this adapter emitted, must be \
                 answered with a view or with None, got {error:?}",
                item.kind
            ),
        };
        let Some(view) = view else { continue };
        let mut seen = std::collections::HashSet::new();
        for section in &view.sections {
            assert!(
                !section.id.trim().is_empty() && seen.insert(section.id.as_str()),
                "the source view of {id} has a blank or repeated section id {:?} -- a deferred \
                 read names its section by id, so ids must be unique",
                section.id
            );
        }
        for section in view.sections.iter().filter(|s| s.deferred) {
            let read = s.view(&id, Some(&section.id)).await;
            let answered = match &read {
                Ok(Some(one)) => {
                    one.sections.len() == 1
                        && one.sections[0].id == section.id
                        && !one.sections[0].deferred
                }
                _ => false,
            };
            assert!(
                answered,
                "the deferred section {:?} of {id}'s source view must answer its own read with \
                 exactly that section, not deferred, got {read:?}",
                section.id
            );
        }
    }
}

/// Clause 8: an adapter without a workflow refuses the reachable-transition
/// read, and says which adapter that is.
///
/// **`write_ops` is the question this asks**, and it is the only honest one the
/// battery has: a workflow is not something the SPI can see, but an adapter
/// that cannot move a ticket at all is one that does not declare the
/// `"transition"` write op -- and an adapter that *does* declare it has a
/// workflow to read by definition, since `WriteOp::Transition` is resolved
/// against exactly that list at write time. So the two are one fact, and
/// letting them disagree would be a source offering *Move to …* over a read
/// that refuses, or a select that offers nothing on a source whose action bar
/// moves tickets.
///
/// **The declared side is skipped, not asserted**, for clause 5's reason: an
/// adapter runs this battery against its real backend, and the probe target
/// exists on no instance, so the only thing a call could witness is a 404
/// dressed as a refusal. Covering the answering side is the adapter's own job
/// -- `knobas-source-jira`'s mockd suite and the Atlassian live suite are
/// where it is done. The skip is control flow rather than an assertion, so the
/// mutation sweep cannot see it; `accepts_an_adapter_that_answers_the_read`
/// pins it with an adapter whose read panics if reached.
///
/// **A read-only ticket system refuses too** (#1225). The ticket-system
/// minimum requires the `reachable_transitions` read and leaves the
/// `transition` write optional (ADR-0029), so a ticket system without the
/// write claims the read and answers it with this refusal: a source that
/// cannot move a ticket has no moves to offer, and the shell draws *Move to
/// …* absent under ADR-0020's rule. The claim is the read's place in the
/// role's interface; the refusal is the answer a source without the write
/// gives. `accepts_a_read_only_ticket_system_that_refuses_its_transitions_read`
/// and `rejects_a_read_only_ticket_system_that_answers_its_transitions_read`
/// pin the agreement.
///
/// The refusal must name the adapter because a message that did not would
/// leave a reader with "protocol error" and no way to tell which of a
/// four-source room said it -- the same argument clause 5's "it must say which
/// op it refused" makes.
async fn check_reachable_transitions(s: &dyn crate::Source, d: &crate::SourceDescriptor) {
    if d.write_ops.iter().any(|w| w == "transition") {
        return;
    }
    let probe = format!("{}:contract-battery", d.id);
    let refused = s.reachable_transitions(&probe).await;
    let Err(crate::SourceError::Protocol { message, .. }) = &refused else {
        panic!(
            "an adapter that does not declare the \"transition\" write op has no workflow to \
             read, so reachable_transitions must be refused with SourceError::Protocol (a ticket \
             system that does not declare it claims the read and refuses it: #1225), got \
             {refused:?}"
        );
    };
    assert!(
        message
            .to_lowercase()
            .contains(&d.adapter_kind.to_lowercase()),
        "the refusal must name the adapter that has no workflow -- descriptor.adapter_kind is \
         {:?} and the message was {message:?}",
        d.adapter_kind
    );
}

/// Clause 6: an adapter's declared payload paths against its own corpus.
///
/// Four things, and each of them is a way a declaration could be a lie the
/// readers would carry in silence -- a path-driven read that misses looks
/// exactly like a source that says nothing, so nothing downstream can tell the
/// two apart. This is where they are told apart, once, against the items the
/// adapter itself just emitted.
///
/// 1. **Every declared kind is a kind this adapter emits**, and no kind is
///    declared twice. Checked against `entity_kinds` rather than against the
///    corpus: a kind can be legitimately empty in one instance's data, but a
///    kind the descriptor does not name at all is a declaration nothing will
///    ever read.
/// 2. **No declared path leads to a value of the wrong type.** A status
///    declared one segment short reaches the `{"name": …}` object, and `->>`
///    would stringify that into a column headed `{"name":"In Progress"}` --
///    a guess dressed as an observation.
/// 3. **Where nothing of a kind resolved a declared field, no record of that
///    kind may show the path naming a key it does not have.** This is the
///    misspelling check, and it is asked of the corpus rather than of a record
///    on purpose. One record cannot answer it: a key absent from one issue is
///    also what a field the source omits when empty looks like, and an
///    unassigned issue must not fail a declaration -- the property that makes
///    this clause safe to run against a live instance. A corpus can: one item
///    carrying the value settles the question for the whole kind, and where
///    *no* item does, a record whose object simply lacks the key is the
///    adapter having named a key its own records do not have. See
///    [`names_a_missing_key`](knobas_spi_types::payload::names_a_missing_key).
/// 4. **A field a kind does not declare resolves to nothing.** Narrow and
///    worth having: it says nothing about the *declaration*, but it is what a
///    resolver that grew a knobas-side fallback for an undeclared field dies
///    on, and such a resolver would pass every other clause here while quietly
///    putting a value on screen that no source said.
///
/// **The parent declaration (#595, ADR-0018) is held to the same four**, with
/// two things only it can get wrong. Its `kind` must be one of `entity_kinds`
/// (an `under:` suggestion says what a parent is from that word alone, since
/// the parent need not be mirrored), and its `at` must not be the empty path,
/// which is the record itself. Clause 2's type check reads "an object or an
/// array of objects" for `at` and "a string" for each name inside a parent
/// object; clause 3 weighs `at` and the names in separate slots, so a parent
/// that is found cannot excuse a misspelled name inside it. A kind declaring no
/// parent must resolve none, which is clause 4. Ratified by the deputy's ruling
/// of 2026-10-11 on #594 and recorded in §10.8's #595 entry.
///
/// **A parent kind's own records answer to the names declared for it.**
/// `under:` returns a mirrored parent by reading the children's declared
/// `names` off the parent's own record root
/// ([`resolve_names_as_parent`](knobas_spi_types::payload::resolve_names_as_parent)),
/// which works only if a source names a parent in the parent's own shape. So
/// on every record of a declared parent `kind`, each of those names is held to
/// clause 2 (it leads to a string or to nothing) and clause 3 (where none
/// resolved, no record names a key it does not have), in an evidence slot per
/// parent kind of its own, so the children's parent objects resolving a name
/// cannot excuse the parent's own records lacking it. Both tolerances below
/// hold here too: a parent kind the corpus never populates (a Jira epic
/// outside the JQL) is asked nothing, and the names share their slot as a
/// candidate list does. Ratified by the deputy's ruling of 2026-10-11 on #595.
///
/// **The state declaration (#666) is held to the same four, and to one
/// more.** Clause 2 reads "a string or a boolean" for each candidate's `at`,
/// clause 3 weighs the candidates in one slot, and clause 4 is a kind that
/// declares no state drawing no chip. The one more: a kind that declares a
/// state, and that the corpus populates, must draw a chip on at least one of
/// its items. A value map that names nothing the source writes (`FAILED` for
/// a TeamCity that says `FAILURE`) resolves its path on every record and
/// still shows no chip on any row, which clause 3 cannot see. Recorded in
/// §10.8's #666 entry.
///
/// # What this cannot see, and why neither is fixable here
///
/// A corpus-shaped question is answered by the corpus, so two declarations
/// pass without being asked anything. Both are recorded rather than left for
/// the next reader to discover as a hole, and both are pinned by a test that
/// goes red on a tightening (#301):
/// `accepts_a_wrong_declaration_for_a_kind_the_corpus_never_populates` and
/// `accepts_a_later_candidate_an_earlier_one_resolves_for`.
///
/// * **A kind the corpus never populates.** The evidence below is gathered
///   inside the walk over `items`, so a kind with no items contributes no
///   evidence and clauses 2, 3 and 4 say nothing about it: a wholly wrong
///   declaration for it is silent here. The alternative -- demanding that
///   every declared kind be populated -- would fail a battery run against any
///   instance that happens to have no build configurations, which is the same
///   move clause 3 refuses for an unassigned issue. Clause 1 is what still
///   holds for such a kind: it must at least be one the adapter emits.
/// * **A candidate an earlier candidate resolves for.** The scalars share one
///   evidence slot across a field's candidate list, so `fields.assignee.keyy`
///   after a working `fields.assignee.name` passes. That is the price of a
///   candidate list being a list: two candidates are *one adapter's*
///   alternative spellings, and an instance uses one of them -- per-candidate
///   evidence would fail the second spelling on every instance that does not
///   use it, which is the case the list exists for. So a candidate list is
///   certified as a whole, and the first candidate is the one this clause
///   really pins. `reviewers` is not an exception to that: the array path gets
///   its own slot only so a resolving *array* cannot excuse a wrong key
///   *inside* its elements, which is a different question from one candidate
///   excusing another.
fn check_payload_paths(d: &crate::SourceDescriptor, items: &[crate::SyncItem]) {
    use knobas_spi_types::payload::{
        KindPaths, PayloadPath, at, field, names_a_missing_key, resolve_flag, resolve_list,
        resolve_parents, resolve_state, resolve_string, resolve_trigger,
    };

    let kinds: std::collections::HashSet<&str> =
        d.entity_kinds.iter().map(|k| k.id.as_str()).collect();
    let mut declared_for: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for paths in &d.payload_paths {
        assert!(
            kinds.contains(paths.kind.as_str()),
            "descriptor.payload_paths declares paths for kind {:?}, which is not one of \
             descriptor.entity_kinds {:?} -- nothing will ever read a declaration for a kind \
             this adapter does not emit",
            paths.kind,
            kinds
        );
        // A parent's kind is what an `under:` suggestion says it is, and it is
        // declared rather than looked up because the parent need not be
        // mirrored (ADR-0018) -- so it has to be a kind this adapter emits, or
        // the suggestion names something nothing here can ever open.
        for parent in &paths.parent {
            assert!(
                kinds.contains(parent.kind.as_str()),
                "descriptor.payload_paths declares a parent of kind {:?} for kind {:?}, which is not \
                 one of descriptor.entity_kinds {:?} -- a parent is an entity of this adapter's \
                 own, and `under:` would name a kind nothing here emits",
                parent.kind,
                paths.kind,
                kinds
            );
            assert!(
                !parent.at.segments().is_empty(),
                "descriptor.payload_paths declares a parent for kind {:?} at the empty path, which \
                 is the record itself -- an item is never its own parent",
                paths.kind
            );
        }
        // The Builds frame (ADR-0022, #848): a run is drawn under the
        // pipeline whose declared `pipeline` equals its own, so both kinds
        // must say where that identity is, and a pipeline must say what it is
        // called -- or the frame lists rows it cannot name or join. A saved
        // view (#1207) is no part of it.
        if let Some(frame) = paths
            .frame
            .filter(|f| *f != knobas_spi_types::payload::Frame::SavedView)
        {
            assert!(
                !paths.pipeline.is_empty(),
                "descriptor.payload_paths declares kind {:?} a {frame:?} of the Builds frame but \
                 no `pipeline` path -- a run is joined to its pipeline on that identity, so \
                 without it this kind is a frame row nothing can place",
                paths.kind
            );
            if frame == knobas_spi_types::payload::Frame::Pipeline {
                assert!(
                    !paths.name.is_empty(),
                    "descriptor.payload_paths declares kind {:?} a pipeline but no `name` path -- \
                     the Builds list names every pipeline it draws",
                    paths.kind
                );
            }
        }
        // A ticket system's saved view (#2050): the projection reads a view's
        // query at the declared `source_query` and at no key of its own, so a
        // kind on the frame that declares none is a view that says nothing
        // about which tickets it holds. Claimed or not: the projection keys
        // on the frame, not on the ticket-system claim clause 10 checks.
        if paths.frame == Some(knobas_spi_types::payload::Frame::SavedView) {
            assert!(
                !paths.source_query.is_empty(),
                "descriptor.payload_paths declares kind {:?} a saved view but no `source_query` \
                 path -- the Tickets screen says the view's query decides which tickets it \
                 holds, and reads it there and nowhere else",
                paths.kind
            );
        }
        assert!(
            declared_for.insert(paths.kind.as_str()),
            "descriptor.payload_paths declares kind {:?} twice; readers take the first entry, \
             so the second is a declaration that silently does nothing",
            paths.kind
        );
    }

    /// What a declared field must lead to.
    #[derive(Clone, Copy)]
    enum Wanted {
        /// A status, a priority, an assignee, a project key or name.
        Text,
        /// A merged flag.
        Flag,
        /// The array a list path opens.
        List,
        /// A whole number: an estimate in seconds (#816), a comment count
        /// (#894), a page's place among its siblings (#953), a run's test
        /// counts (#1448).
        Integer,
        /// Any JSON number: a monitor's response time in milliseconds
        /// (#1224), which Uptime Kuma reports with a fraction.
        Number,
        /// An ISO 8601 instant, basic or extended: when a run started or
        /// finished (#848), as `parse_instant` reads one.
        Instant,
        /// A JSON object: a saved view's sprint (#2050).
        Object,
    }

    impl Wanted {
        fn matches(self, value: &serde_json::Value) -> bool {
            match self {
                Wanted::Text => value.is_string(),
                Wanted::Flag => value.is_boolean(),
                Wanted::List => value.is_array(),
                // At most 18 digits, as `declared_integer!` reads one: a
                // value the reader would miss is not one the battery passes.
                Wanted::Integer => value
                    .as_i64()
                    .is_some_and(|n| n.unsigned_abs() < 1_000_000_000_000_000_000),
                Wanted::Number => value.is_number(),
                Wanted::Object => value.is_object(),
                Wanted::Instant => value
                    .as_str()
                    .is_some_and(|t| knobas_spi_types::payload::parse_instant(t).is_some()),
            }
        }

        fn word(self) -> &'static str {
            match self {
                Wanted::Text => "string",
                Wanted::Flag => "boolean",
                Wanted::List => "array",
                Wanted::Integer => "whole number",
                Wanted::Number => "number",
                Wanted::Instant => "ISO 8601 instant",
                Wanted::Object => "object",
            }
        }
    }

    /// What one (kind, field) pair's paths showed across the whole corpus.
    #[derive(Default)]
    struct Evidence {
        /// Some item of this kind resolved the field.
        resolved: bool,
        /// Some item of this kind has an object where the path's next key
        /// would sit, and does not have that key.
        names_a_missing_key: bool,
    }

    // A parent kind -> every `names` path any kind of this adapter declares
    // for it, deduplicated: the names a record of that kind answers to as
    // someone's parent, which is what `resolve_names_as_parent` reads off it.
    let mut names_as_parent: std::collections::BTreeMap<&str, Vec<&PayloadPath>> =
        std::collections::BTreeMap::new();
    for parent in d.payload_paths.iter().flat_map(|p| &p.parent) {
        let names = names_as_parent.entry(parent.kind.as_str()).or_default();
        for name in &parent.names {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }

    // Kinds declaring a state -> whether some item of that kind drew a chip
    // (#666). Only kinds the corpus populates are judged, as clause 3 judges.
    let mut chips: std::collections::BTreeMap<&str, bool> = std::collections::BTreeMap::new();

    let nothing = KindPaths::default();
    // (kind, field) -> what the corpus showed. Ordered, so a failure names the
    // same field on every run.
    let mut evidence: std::collections::BTreeMap<(&str, &str), Evidence> =
        std::collections::BTreeMap::new();

    for item in items {
        let paths = d
            .payload_paths
            .iter()
            .find(|p| p.kind == item.kind)
            .unwrap_or(&nothing);
        let scalars: [(&str, &Vec<PayloadPath>, Wanted); 36] = [
            (field::STATUS_NAME, &paths.status_name, Wanted::Text),
            (field::PRIORITY, &paths.priority, Wanted::Text),
            (field::ASSIGNEE, &paths.assignee, Wanted::Text),
            (field::PROJECT_KEY, &paths.project_key, Wanted::Text),
            (field::PROJECT_NAME, &paths.project_name, Wanted::Text),
            (field::MERGED, &paths.merged, Wanted::Flag),
            (field::HEAD, &paths.head, Wanted::Text),
            (field::COMMIT, &paths.commit, Wanted::Text),
            (field::HEAD_REPO, &paths.head_repo, Wanted::Text),
            (field::DEFAULT_BRANCH, &paths.default_branch, Wanted::Text),
            (field::ISSUE_TYPE, &paths.issue_type, Wanted::Text),
            (field::ESTIMATE, &paths.estimate, Wanted::Integer),
            (field::COMMENT_COUNT, &paths.comment_count, Wanted::Integer),
            (field::POSITION, &paths.position, Wanted::Integer),
            (field::NAME, &paths.name, Wanted::Text),
            (field::GROUP, &paths.group, Wanted::Text),
            (field::PIPELINE, &paths.pipeline, Wanted::Text),
            (field::NUMBER, &paths.number, Wanted::Text),
            (field::BRANCH, &paths.branch, Wanted::Text),
            (field::STARTED, &paths.started, Wanted::Instant),
            (field::FINISHED, &paths.finished, Wanted::Instant),
            (field::STARTED_BY, &paths.started_by, Wanted::Text),
            (field::AGENT, &paths.agent, Wanted::Text),
            (field::CURRENT_STEP, &paths.current_step, Wanted::Text),
            (field::TESTS_FAILED, &paths.tests_failed, Wanted::Integer),
            (field::TESTS_PASSED, &paths.tests_passed, Wanted::Integer),
            (field::TESTS_IGNORED, &paths.tests_ignored, Wanted::Integer),
            (field::LANGUAGE, &paths.language, Wanted::Text),
            (field::OWNER, &paths.owner, Wanted::Text),
            (field::MONITOR_TYPE, &paths.monitor_type, Wanted::Text),
            (field::RESPONSE_TIME, &paths.response_time, Wanted::Number),
            (field::BASE, &paths.base, Wanted::Text),
            (field::SOURCE_QUERY, &paths.source_query, Wanted::Text),
            (field::BOARD_TYPE, &paths.board_type, Wanted::Text),
            (field::SUB_QUERY, &paths.sub_query, Wanted::Text),
            (field::SPRINT, &paths.sprint, Wanted::Object),
        ];
        // The list fields: who a review is asked of, the commits a build ran
        // on (#710), a ticket's labels and its sprints' states (#811), a
        // pipeline's chain of groups (#849), a repository's teams (#966). One
        // rule for all six.
        let lists: [(&str, &Vec<knobas_spi_types::payload::ListPath>); 6] = [
            (field::REVIEWERS, &paths.reviewers),
            (field::REVISION, &paths.revision),
            (field::LABELS, &paths.labels),
            (field::SPRINT_STATE, &paths.sprint_state),
            (field::GROUP_PATH, &paths.group_path),
            (field::TEAMS, &paths.teams),
        ];

        // Clause 2 for one path on one item, and the evidence clause 3 wants
        // from it.
        let weigh = |seen: &mut Evidence,
                     name: &str,
                     payload: &serde_json::Value,
                     path: &PayloadPath,
                     wanted: Wanted,
                     within: &str| {
            if let Some(value) = at(payload, path) {
                assert!(
                    wanted.matches(value),
                    "descriptor.payload_paths declares {name} of kind {:?} at {:?}, which leads \
                     to {} {within} this adapter's own item {} -- a declared path must lead to a \
                     {} or to nothing at all, or every reader of it gets a value no source meant. \
                     Payload: {}",
                    item.kind,
                    path.segments(),
                    match value {
                        serde_json::Value::Object(_) => "an object",
                        serde_json::Value::Array(_) => "an array",
                        serde_json::Value::Number(_) => "a number",
                        serde_json::Value::Bool(_) => "a boolean",
                        _ => "a string",
                    },
                    item.entity,
                    wanted.word(),
                    item.payload
                );
                seen.resolved = true;
            }
            seen.names_a_missing_key |= names_a_missing_key(payload, path);
        };

        // A run its declared trigger says a change or a schedule started
        // names nobody (#961): TeamCity omits `triggered.user` on every VCS
        // build, so its `started_by` there is the source saying nothing, not
        // a key the declaration invented. Type-checked still, never evidence.
        let nobody_started = matches!(
            resolve_trigger(&item.payload, &paths.trigger),
            Some(
                knobas_spi_types::payload::Trigger::Change
                    | knobas_spi_types::payload::Trigger::Schedule
            )
        );
        // Only a run in flight is doing something now (#1004): its declared
        // `started` reads and its `finished` does not. TeamCity serves
        // `running-info` on a running build alone, so a finished or a queued
        // one is the source saying nothing about `current_step`, and a corpus
        // with no build running at all has no evidence either way.
        let in_flight = resolve_string(&item.payload, &paths.started).is_some()
            && resolve_string(&item.payload, &paths.finished).is_none();
        // A run that ran no tests has no test summary (#1448): TeamCity
        // serves no `testOccurrences` on such a build, so an absent count is
        // the source saying nothing, unless one of the run's other counts
        // reads. Then the summary is there, and a count missing from it is a
        // misspelled path.
        let tests = [
            field::TESTS_FAILED,
            field::TESTS_PASSED,
            field::TESTS_IGNORED,
        ];
        let ran_tests = [
            &paths.tests_failed,
            &paths.tests_passed,
            &paths.tests_ignored,
        ]
        .into_iter()
        .flatten()
        .any(|path| at(&item.payload, path).is_some());
        for (name, candidates, wanted) in scalars {
            let seen = evidence.entry((&item.kind, name)).or_default();
            let judged = seen.names_a_missing_key;
            for path in candidates {
                weigh(seen, name, &item.payload, path, wanted, "on");
            }
            if (name == field::STARTED_BY && nobody_started)
                || (name == field::CURRENT_STEP && !in_flight)
                || (tests.contains(&name) && !ran_tests)
            {
                seen.names_a_missing_key = judged;
            }
        }
        for (name, candidates) in lists {
            for candidate in candidates {
                let seen = evidence.entry((&item.kind, name)).or_default();
                // The array itself is weighed against its own evidence slot, so a
                // list whose array is there and whose entry key is wrong is not
                // excused by the array having resolved.
                let mut array = Evidence::default();
                weigh(
                    &mut array,
                    name,
                    &item.payload,
                    &candidate.at,
                    Wanted::List,
                    "on",
                );
                seen.names_a_missing_key |= array.names_a_missing_key;
                for element in at(&item.payload, &candidate.at)
                    .and_then(serde_json::Value::as_array)
                    .map_or(&[][..], Vec::as_slice)
                {
                    weigh(
                        seen,
                        name,
                        element,
                        &candidate.entry,
                        Wanted::Text,
                        "in an element of",
                    );
                }
            }
        }

        // The parent declaration (#595): clause 2's type check on where the
        // parent is -- one object, or an array of objects -- and on each name
        // inside one, and clause 3's evidence for both. `at` and the names get
        // a slot each, for `reviewers`' reason: a resolving `at` must not
        // excuse a misspelled name inside the parent it found.
        for candidate in &paths.parent {
            let seen = evidence.entry((&item.kind, field::PARENT)).or_default();
            let found = at(&item.payload, &candidate.at);
            if let Some(value) = found {
                let shape = match value {
                    serde_json::Value::Object(_) => None,
                    serde_json::Value::Array(chain)
                        if chain.iter().all(serde_json::Value::is_object) =>
                    {
                        None
                    }
                    serde_json::Value::Array(_) => Some("an array that is not all objects"),
                    serde_json::Value::String(_) => Some("a string"),
                    serde_json::Value::Number(_) => Some("a number"),
                    _ => Some("a boolean"),
                };
                if let Some(shape) = shape {
                    panic!(
                        "descriptor.payload_paths declares parent of kind {:?} at {:?}, which leads \
                         to {shape} on this adapter's own item {} -- a parent must be an object or \
                         an array of objects, or nothing at all, or every `under:` read misses on \
                         it while looking like an item with no parent. Payload: {}",
                        item.kind,
                        candidate.at.segments(),
                        item.entity,
                        item.payload
                    );
                }
                seen.resolved = true;
            }
            seen.names_a_missing_key |= names_a_missing_key(&item.payload, &candidate.at);

            let objects: Vec<&serde_json::Value> = match found {
                Some(serde_json::Value::Array(chain)) => chain.iter().collect(),
                Some(object) => vec![object],
                None => Vec::new(),
            };
            let names = evidence.entry((&item.kind, "parent names")).or_default();
            for object in objects {
                for name in &candidate.names {
                    weigh(
                        names,
                        "parent names",
                        object,
                        name,
                        Wanted::Text,
                        "in a parent object of",
                    );
                }
            }
        }
        // The chain of projects (#1019): `at` leads to an array of objects or
        // to nothing, and each key and name inside an element is a text path,
        // weighed in slots of their own so a resolving `at` excuses neither.
        for candidate in &paths.project_path {
            let seen = evidence
                .entry((&item.kind, field::PROJECT_PATH))
                .or_default();
            let found = at(&item.payload, &candidate.at);
            if let Some(value) = found {
                assert!(
                    value
                        .as_array()
                        .is_some_and(|chain| chain.iter().all(serde_json::Value::is_object)),
                    "descriptor.payload_paths declares project_path of kind {:?} at {:?}, which \
                     does not lead to an array of objects on this adapter's own item {} -- a chain \
                     of projects must be one, or nothing at all. Payload: {}",
                    item.kind,
                    candidate.at.segments(),
                    item.entity,
                    item.payload
                );
                seen.resolved = true;
            }
            seen.names_a_missing_key |= names_a_missing_key(&item.payload, &candidate.at);
            let elements = found
                .and_then(serde_json::Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            for (slot, paths_in) in [
                ("project_path keys", &candidate.key),
                ("project_path names", &candidate.name),
            ] {
                let seen = evidence.entry((&item.kind, slot)).or_default();
                for element in elements {
                    for path in paths_in {
                        weigh(seen, slot, element, path, Wanted::Text, "in a project of");
                    }
                }
            }
        }
        if paths.project_path.is_empty() {
            assert!(
                knobas_spi_types::payload::resolve_project_path(&item.payload, &paths.project_path)
                    .is_empty(),
                "kind {:?} declares no project_path, so every reader must miss on item {}",
                item.kind,
                item.entity
            );
        }
        // The state declaration (#666): each candidate's `at` leads to a
        // string or a boolean or nothing, and is weighed for clause 3 like
        // any path; and the kind, if it declares a state, must draw a chip on
        // some item of the corpus -- a value map naming nothing the source
        // writes is a chip no row shows.
        for candidate in &paths.state {
            let seen = evidence.entry((&item.kind, field::STATE)).or_default();
            if let Some(value) = at(&item.payload, &candidate.at) {
                assert!(
                    value.is_string() || value.is_boolean(),
                    "descriptor.payload_paths declares state of kind {:?} at {:?}, which leads \
                     to {} on this adapter's own item {} -- a state must be a string or a \
                     boolean, or nothing at all. Payload: {}",
                    item.kind,
                    candidate.at.segments(),
                    match value {
                        serde_json::Value::Object(_) => "an object",
                        serde_json::Value::Array(_) => "an array",
                        _ => "a number",
                    },
                    item.entity,
                    item.payload
                );
                seen.resolved = true;
            }
            seen.names_a_missing_key |= names_a_missing_key(&item.payload, &candidate.at);
        }
        // A run's trigger (#961): each candidate's `at` is weighed as a text
        // path, in one slot as `state`'s candidates are; a kind that declares
        // none must miss.
        for candidate in &paths.trigger {
            let seen = evidence.entry((&item.kind, field::TRIGGER)).or_default();
            weigh(
                seen,
                field::TRIGGER,
                &item.payload,
                &candidate.at,
                Wanted::Text,
                "on",
            );
        }
        if paths.trigger.is_empty() {
            assert_eq!(
                resolve_trigger(&item.payload, &paths.trigger),
                None,
                "kind {:?} declares no trigger, so every reader must miss on item {}",
                item.kind,
                item.entity
            );
        }
        if !paths.state.is_empty() {
            *chips.entry(item.kind.as_str()).or_default() |=
                resolve_state(&item.payload, &paths.state).is_some();
        } else {
            assert_eq!(
                resolve_state(&item.payload, &paths.state),
                None,
                "kind {:?} declares no state, so every reader must miss on item {}",
                item.kind,
                item.entity
            );
        }

        // The parent's own record (the deputy's ruling of 2026-10-11 on
        // #595): `under:` returns a mirrored parent by reading the children's
        // declared names off its root, so on this kind's records -- if it is
        // anyone's parent kind -- each of those names is held to clause 2 and
        // clause 3 in a slot of its own, apart from the children's.
        if let Some(names) = names_as_parent.get(item.kind.as_str()) {
            let seen = evidence
                .entry((&item.kind, "the parent's own names"))
                .or_default();
            for name in names {
                weigh(
                    seen,
                    "the parent's own names",
                    &item.payload,
                    name,
                    Wanted::Text,
                    "on",
                );
            }
        }
        if paths.parent.is_empty() {
            assert!(
                resolve_parents(&item.payload, &paths.parent).is_empty(),
                "kind {:?} declares no parent, so every `under:` read must miss on item {}",
                item.kind,
                item.entity
            );
        }

        // Clause 4: what this kind does not declare, no reader may produce --
        // including for a kind that declares nothing at all, which is most of
        // a corpus and the place a knobas-side fallback would show up first.
        for (name, candidates, _) in scalars {
            if candidates.is_empty() && name != field::MERGED {
                assert_eq!(
                    resolve_string(&item.payload, candidates),
                    None,
                    "kind {:?} declares no {name}, so every reader must miss on item {} -- a \
                     value here is knobas guessing at a shape no adapter claimed",
                    item.kind,
                    item.entity
                );
            }
        }
        if paths.merged.is_empty() {
            assert_eq!(
                resolve_flag(&item.payload, &paths.merged),
                None,
                "kind {:?} declares no merged flag, so every reader must miss on item {}",
                item.kind,
                item.entity
            );
        }
        for (name, candidates) in lists {
            if candidates.is_empty() {
                assert!(
                    resolve_list(&item.payload, candidates).is_empty(),
                    "kind {:?} declares no {name}, so every reader must miss on item {}",
                    item.kind,
                    item.entity
                );
            }
        }
    }

    for (kind, drew) in chips {
        assert!(
            drew,
            "descriptor.payload_paths declares state of kind {kind:?}, and no item of that kind \
             draws a chip -- the value map names nothing this adapter's own records carry, so \
             every row of the kind shows no state while looking like a source that has none"
        );
    }

    for ((kind, name), seen) in evidence {
        assert!(
            seen.resolved || !seen.names_a_missing_key,
            "descriptor.payload_paths declares {name} of kind {kind:?} at a path no item of that \
             kind resolves, and some of those items carry an object the path's next key is simply \
             not in -- so it names a key this adapter's own records do not have, and every reader \
             of it misses for ever while looking exactly like a source that says nothing"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{Fault, battery};
    use crate::{
        AuthMethod, Capability, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor,
        SourceError, SyncItem, WriteOp,
    };
    use knobas_spi_types::entity::EntityRef;

    /// How the adapter under test misbehaves -- one variant per battery
    /// assertion, so an assertion that stops being enforced turns exactly one
    /// test red.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Behavior {
        /// Honours the whole contract, declaring no write ops.
        Good,
        /// Honours the whole contract and declares `comment`, shaped like Task
        /// 7's mock. Its `write` panics, so the battery accepting it is proof
        /// that the declared-op guard skipped it rather than performing a real
        /// write against what would be a live system.
        DeclaresAWriteOp,
        /// Emits nothing at all: the do-nothing adapter.
        Null,
        /// Declares a blank source id, so every id it emits is unparseable.
        BlankSourceId,
        /// Names itself after a namespace knobas keeps for its own entities.
        ReservedSourceId,
        /// Names itself with something that is not a slug -- uppercase and an
        /// underscore, the two things a human types first. Otherwise perfect:
        /// it emits into its own odd namespace consistently, so the slug rule
        /// is the *only* clause it fails.
        NonSlugSourceId,
        /// Fails `test_connection` when nothing is wrong, while classifying
        /// both faults correctly -- so it fails the healthy-connect clause
        /// alone.
        FailsWhileHealthy,
        /// Emits an item namespaced to something other than the source id.
        ForeignNamespace,
        /// Emits an item with a blank key -- `EntityRef::new` does not check.
        IllFormedId,
        /// Emits an item of a kind absent from `descriptor.entity_kinds`.
        UndeclaredKind,
        /// Ignores the cursor and re-emits everything on an incremental sync.
        IgnoresCursor,
        /// Emits nothing on an incremental sync but hands back a *new* cursor,
        /// so every idle poll reads as a change.
        MovesTheCursorWhenIdle,
        /// `test_connection` reports an auth failure as `Protocol`.
        MisclassifiesAuthOnConnect,
        /// `test_connection` reports a connectivity failure as `Protocol`.
        MisclassifiesReachOnConnect,
        /// `sync` reports an auth failure as `Protocol`, though
        /// `test_connection` gets it right.
        MisclassifiesAuthOnSync,
        /// `sync` reports a connectivity failure as `Protocol`.
        MisclassifiesReachOnSync,
        /// Declares a write-op identifier that is not one the SPI defines.
        UnknownWriteOpId,
        /// Advertises `Capability::Write` while listing no write ops.
        WriteCapabilityWithoutOps,
        /// Lists write ops while reading as read-only.
        WriteOpsWithoutCapability,
        /// Accepts a write op it never declared instead of refusing it.
        AcceptsUndeclaredWrite,
        /// Declares `transition` -- so it has a workflow -- and its
        /// reachable-transition read panics if reached. The battery accepting
        /// it is what proves clause 8 skips the answering side instead of
        /// probing a live instance with a target that exists nowhere.
        DeclaresTransition,
        /// Has no workflow and answers the reachable-transition read anyway,
        /// with a list it made up.
        AnswersTheTransitionRead,
        /// Refuses the reachable-transition read without saying which adapter
        /// refused, leaving a four-source room with an unattributable
        /// "protocol error".
        RefusesTheReadAnonymously,
        /// Swallows the sink's error and reports a successful sync.
        SwallowsSinkError,
        /// Draws nothing for its one kind: `None` is an answer, not a failure.
        DrawsNoView,
        /// Fails the source view of an entity it emitted itself.
        FailsTheView,
        /// Draws a view whose two sections share one id, so a deferred read
        /// could not say which it wants.
        RepeatsASectionId,
        /// Draws a deferred section whose own read answers a different one.
        DefersASectionItCannotRead,
        /// Declares payload paths for a kind absent from `entity_kinds`.
        PayloadPathsForAKindItDoesNotEmit,
        /// Declares the same kind's payload paths twice, so the second entry
        /// is a declaration nothing reads.
        PayloadPathsForOneKindTwice,
        /// Declares a status one key off in its **last** segment
        /// (`fields.status.nam`), which its own items do not carry -- a read
        /// that misses for ever and looks exactly like a source that says
        /// nothing.
        MisspelledPayloadPath,
        /// The same typo one segment **earlier** (`fieldz.status.name`), where
        /// the walk stops at the payload root rather than deep inside it.
        /// Structurally the harder of the two to catch, and the reason clause
        /// 3 asks about a missing key rather than about a container.
        MisspelledPayloadPathAtTheRoot,
        /// Declares a status at `fields.status`, which lands on the object
        /// rather than on a string.
        PayloadPathOntoAnObject,
        /// Declares reviewers whose entry key its own elements do not carry.
        MisspelledReviewerEntry,
        /// Emits `ticket` correctly while also declaring a second kind,
        /// `gadget`, whose declaration is wholly wrong -- and emits no
        /// `gadget`. The corpus never populates that kind, so the clause
        /// gathers no evidence about it and accepts. Tolerance, not a bug:
        /// see `accepts_a_wrong_declaration_for_a_kind_the_corpus_never_populates`.
        WrongPathsForAKindTheCorpusNeverPopulates,
        /// Declares two status candidates: the first resolves on every item,
        /// the second names a key those items do not have. The candidates
        /// pool one evidence slot, so the first excuses the second and the
        /// clause accepts. Tolerance, not a bug: see
        /// `accepts_a_later_candidate_an_earlier_one_resolves_for`.
        ASecondCandidateNoItemResolves,
        /// Declares a parent whose kind is not one of its own `entity_kinds`
        /// -- a name no `under:` suggestion could ever say what it is.
        ParentOfAKindItDoesNotEmit,
        /// Declares a parent `at` the status name, which leads to a string:
        /// neither an object nor a chain of them (#595).
        ParentAtOntoAString,
        /// Declares a parent `at` an array whose elements are strings, not
        /// parent objects.
        ParentAtOntoAnArrayOfStrings,
        /// Declares a parent at `fields.epicc`, a key its records do not have.
        MisspelledParentAt,
        /// Declares a parent whose name sits at `kee` inside the parent
        /// object, which carries `key`.
        MisspelledParentName,
        /// Its tickets are their own parent kind, and the children's parent
        /// objects carry `key` -- but the tickets' own records do not, so
        /// `under:` could reach the children of an epic and never the epic
        /// itself (the deputy's ruling of 2026-10-11 on #595).
        ParentRecordsLackTheirName,
        /// Its tickets carry `key` on their own records, but as an object
        /// rather than the string a name has to be.
        ParentRecordsNameIsNotAString,
        /// Declares the parent's names as `key` then `kee`. `key` resolves on
        /// the epic objects and on the tickets' own records, `kee` on neither:
        /// the names share their slot, so the battery accepts. Tolerance, not
        /// a bug: see `accepts_a_parent_name_an_earlier_name_resolves_for`.
        ASecondParentNameNoRecordCarries,
        /// Declares a state whose value map names no value its records carry
        /// (`Done`, on tickets that are all `In Progress`): no row of the
        /// kind would ever draw a chip (#666).
        StateNoItemResolves,
        /// Declares a state `at` the status object rather than its name.
        StateOntoAnObject,
        /// Declares its tickets runs of the Builds frame with no `pipeline`
        /// path: rows nothing can place under a pipeline (#848).
        FrameWithoutPipeline,
        /// Declares its tickets pipelines with a `pipeline` path but no
        /// `name`: rows the Builds list cannot name (#848).
        PipelineWithoutName,
        /// Declares a run's `started` at the ticket key, a string that is no
        /// instant (#848).
        StartedThatIsNoInstant,
        /// Declares a run's trigger at a key its records do not have
        /// (`triggerd`, #961).
        TriggerAtAMissingKey,
        /// Declares who started a run at a key its records do not have
        /// (`startedBy`), with no trigger to say nobody did (#961).
        StartedByAtAMissingKey,
        /// The same `started_by`, on records whose declared trigger is a
        /// change: runs nobody started, so the absent key is the source
        /// saying nothing (#961). Conforming.
        StartedByOnRunsNobodyStarted,
        /// The same `started_by`, on records whose declared trigger is a
        /// person: runs somebody started, so the absent key is a misspelled
        /// path and still evidence (#961).
        StartedByOnRunsAPersonStarted,
        /// Declares the agent a run ran on at a key its records do not have
        /// (`agnt`, #1004).
        AgentAtAMissingKey,
        /// Declares a response time (#1224) at the ticket's `key`, a string:
        /// a reading must be a number.
        ResponseTimeThatIsNoNumber,
        /// Declares a response time at a reading with a fraction (`13.5`),
        /// as Uptime Kuma reports one: conforming (#1224).
        ResponseTimeWithAFraction,
        /// Declares the branch a pull request is opened into (#1227) at a
        /// key its records do not have (`bse`).
        BaseAtAMissingKey,
        /// Declares its tickets saved views (#2050) with no `source_query`:
        /// the projection reads a view's query there and nowhere else.
        SavedViewWithoutQuery,
        /// Declares its tickets saved views with a query, a board type, a
        /// sub-query and a sprint, the sprint at an object: conforming.
        SavedViewWithItsPaths,
        /// Declares a saved view's sprint at the ticket's `key`, a string: a
        /// sprint is an object.
        SprintThatIsNoObject,
        /// Declares what a run is doing now at a key its records do not
        /// have (`stage`), on records with no `started`: runs not in flight,
        /// so the absent key is the source saying nothing (#1004).
        /// Conforming.
        CurrentStepOnRunsNotInFlight,
        /// The same `current_step`, on records that started and have not
        /// finished: runs in flight, so the absent key is a misspelled path
        /// (#1004).
        CurrentStepOnRunsInFlight,
        /// Declares a run's three test counts at keys its records do not
        /// have (`tests.*`), on records with no test summary at all: runs
        /// that ran no tests, so the absent keys are the source saying
        /// nothing (#1448). Conforming.
        TestCountsOnRunsWithoutTests,
        /// Declares `tests_failed` where the records have a count and
        /// `tests_passed` at a key beside it they do not (`pased`): the
        /// summary is there, so the missing count is a misspelled path
        /// (#1448).
        TestCountMissingFromASummary,
        /// Claims the code-host role, emitting only tickets: no `repo` kind
        /// (#1193).
        ClaimsCodeHostWithoutItsKinds,
        /// Names an address to clone from, and claims no code host (#1197).
        ClonesWithoutACodeHostClaim,
        /// Claims the ticket-system role over tickets that declare every
        /// path it needs, and the read: conforming (#1193). It declares no
        /// `transition`, so it is a read-only ticket system and answers the
        /// read it claims with a refusal by name (#1225).
        ClaimsTicketSystem,
        /// The same read-only ticket system, answering the transitions read
        /// with a list of moves it cannot make (#1225).
        ClaimsTicketSystemAndAnswersTheTransitionRead,
        /// The same claim over tickets that declare no priority (#1193).
        ClaimsTicketSystemWithoutPriority,
        /// Claims CI over a pipeline and a run kind declaring their paths,
        /// with `run_log` and one optional read: conforming (#1193).
        ClaimsCi,
        /// The same claim without the `run_log` read (#1193).
        ClaimsCiWithoutRunLog,
        /// Claims the wiki role over pages that declare a space and no
        /// parent (#1193).
        ClaimsWikiWithoutParent,
        /// Claims monitoring over monitors that declare no state (#1193).
        ClaimsMonitoringWithoutState,
        /// Claims a custom role namespaced to its own kind, `test:thing`, with
        /// no reads: conforming (#1201, ADR-0029).
        ClaimsItsCustomRole,
        /// Claims `other-adapter:thing`, a custom role namespaced to another
        /// adapter, which no adapter but that one may claim (#1201).
        ClaimsAnotherAdaptersCustomRole,
        /// Declares the source action `add_label` and `source_action` beside
        /// it, refuses every other id, and panics if the declared one is
        /// performed (#1194): the conforming case of clause 11.
        DeclaresASourceAction,
        /// Declares `add_label` and performs any source action it is handed.
        PerformsAnUndeclaredSourceAction,
        /// Declares `add_label` without `source_action` in `write_ops`.
        SourceActionsWithoutTheOp,
        /// Lists `source_action` in `write_ops` and declares no action.
        SourceActionOpWithoutActions,
        /// Declares `add_label` twice.
        RepeatsASourceAction,
        /// Declares `Capability::Refresh` and reads its item back as the
        /// sync emitted it: the conforming case of clause 13.
        Refreshes,
        /// Reads its item back without declaring `Capability::Refresh`.
        RefreshesWithoutTheCapability,
        /// Declares `Capability::Refresh` and answers `NotOffered` for all.
        DeclaresRefreshAndOffersNone,
        /// Declares `Capability::Refresh`, says its one kind is not
        /// refreshed, and answers `NotOffered` for it: the capability with no
        /// kind behind it (#2146).
        DeclaresRefreshForNoKind,
        /// Declares `Capability::Refresh`, says its one kind is not
        /// refreshed, and reads its item back all the same (#2146).
        RefreshesAKindDeclaredNotRefreshed,
        /// Reads its item back under another title than the sync's.
        RefreshesAnotherTitle,
        /// Reads its item back with another status at the declared path.
        RefreshesAnotherStatus,
        /// Answers a live item gone.
        RefreshesALiveItemAsGone,
    }

    struct TestSource {
        fault: Fault,
        behavior: Behavior,
    }

    impl TestSource {
        /// The fault this adapter should report, or `None` to carry on.
        fn faulted(&self, misclassify_auth: bool, misclassify_reach: bool) -> Option<SourceError> {
            match self.fault {
                Fault::None => None,
                Fault::Unauthorized if misclassify_auth => Some(SourceError::protocol("401")),
                Fault::Unauthorized => Some(SourceError::unauthorized()),
                Fault::Unreachable if misclassify_reach => {
                    Some(SourceError::protocol("no route to host"))
                }
                Fault::Unreachable => Some(SourceError::Unreachable("connection refused".into())),
            }
        }
    }

    #[async_trait::async_trait]
    impl Source for TestSource {
        fn descriptor(&self) -> SourceDescriptor {
            SourceDescriptor {
                id: match self.behavior {
                    Behavior::BlankSourceId => String::new(),
                    // A real one: knobas' own monitor entities live here, and
                    // an Uptime Kuma adapter calling itself `monitor` is the
                    // obvious mistake.
                    Behavior::ReservedSourceId => "monitor".into(),
                    Behavior::NonSlugSourceId => "Test_Source".into(),
                    _ => "test".into(),
                },
                adapter_kind: "test".into(),
                name: "Test".into(),
                // Kept coherent with `write_ops` below for every behavior but
                // the two that exist to violate exactly that.
                capabilities: match self.behavior {
                    Behavior::DeclaresAWriteOp
                    | Behavior::DeclaresTransition
                    | Behavior::UnknownWriteOpId
                    | Behavior::WriteCapabilityWithoutOps
                    | Behavior::DeclaresASourceAction
                    | Behavior::PerformsAnUndeclaredSourceAction
                    | Behavior::SourceActionsWithoutTheOp
                    | Behavior::SourceActionOpWithoutActions
                    | Behavior::RepeatsASourceAction => {
                        vec![Capability::Search, Capability::Write]
                    }
                    Behavior::Refreshes
                    | Behavior::DeclaresRefreshAndOffersNone
                    | Behavior::DeclaresRefreshForNoKind
                    | Behavior::RefreshesAKindDeclaredNotRefreshed
                    | Behavior::RefreshesAnotherTitle
                    | Behavior::RefreshesAnotherStatus
                    | Behavior::RefreshesALiveItemAsGone => {
                        vec![Capability::Search, Capability::Refresh]
                    }
                    _ => vec![Capability::Search],
                },
                adapter_version: "0.1.0".into(),
                auth_methods: vec![AuthMethod::Pat],
                accepts_account: false,
                min_sync_interval_secs: 60,
                roles: {
                    use crate::role::{CI, CODE_HOST, MONITORING, RoleClaim, TICKET_SYSTEM, WIKI};
                    match self.behavior {
                        Behavior::ClaimsCodeHostWithoutItsKinds => {
                            vec![RoleClaim::new(CODE_HOST, [])]
                        }
                        Behavior::ClaimsTicketSystem
                        | Behavior::ClaimsTicketSystemAndAnswersTheTransitionRead
                        | Behavior::ClaimsTicketSystemWithoutPriority => {
                            vec![RoleClaim::new(TICKET_SYSTEM, ["reachable_transitions"])]
                        }
                        Behavior::ClaimsCi => vec![RoleClaim::new(CI, ["run_log", "run_tests"])],
                        Behavior::ClaimsCiWithoutRunLog => vec![RoleClaim::new(CI, ["run_tests"])],
                        Behavior::ClaimsWikiWithoutParent => {
                            vec![RoleClaim::new(WIKI, ["doc_content"])]
                        }
                        Behavior::ClaimsMonitoringWithoutState => {
                            vec![RoleClaim::new(MONITORING, [])]
                        }
                        Behavior::ClaimsItsCustomRole => vec![RoleClaim::new("test:thing", [])],
                        Behavior::ClaimsAnotherAdaptersCustomRole => {
                            vec![RoleClaim::new("other-adapter:thing", [])]
                        }
                        _ => Vec::new(),
                    }
                },
                // Declares no write ops, so the battery probes `comment` and
                // expects a refusal -- except where the behavior needs otherwise.
                write_ops: match self.behavior {
                    Behavior::UnknownWriteOpId => vec!["Comment".into()],
                    Behavior::DeclaresAWriteOp | Behavior::WriteOpsWithoutCapability => {
                        vec!["comment".into()]
                    }
                    Behavior::DeclaresTransition => vec!["transition".into()],
                    Behavior::DeclaresASourceAction
                    | Behavior::PerformsAnUndeclaredSourceAction
                    | Behavior::SourceActionOpWithoutActions
                    | Behavior::RepeatsASourceAction => vec!["source_action".into()],
                    // Something to write, so the capability clause passes and
                    // clause 11 is the one this fails.
                    Behavior::SourceActionsWithoutTheOp => vec!["comment".into()],
                    _ => Vec::new(),
                },
                source_actions: match self.behavior {
                    Behavior::DeclaresASourceAction
                    | Behavior::PerformsAnUndeclaredSourceAction
                    | Behavior::SourceActionsWithoutTheOp => vec!["add_label".into()],
                    Behavior::RepeatsASourceAction => vec!["add_label".into(), "add_label".into()],
                    _ => Vec::new(),
                },
                clone_from: match self.behavior {
                    Behavior::ClonesWithoutACodeHostClaim => {
                        vec![crate::clone_urls::CloneFrom::BaseUrl]
                    }
                    _ => Vec::new(),
                },
                url_forms: Vec::new(),
                code_pages: None,
                entity_kinds: {
                    let kind = |id: &str, label: &str, monogram: &str| KindInfo {
                        id: id.to_owned(),
                        label: label.to_owned(),
                        plural: format!("{label}s"),
                        monogram: monogram.to_owned(),
                        full_sync_exhaustive: true,
                        refreshes: !matches!(
                            self.behavior,
                            Behavior::DeclaresRefreshForNoKind
                                | Behavior::RefreshesAKindDeclaredNotRefreshed
                        ),
                    };
                    let mut kinds = vec![kind("ticket", "Ticket", "TE")];
                    // The second kind exists only for the tolerance case, and
                    // only in `entity_kinds`: clause 1 wants a declared kind
                    // to be one the adapter emits, and nothing anywhere wants
                    // it to be one the adapter's corpus happens to contain.
                    // Kinds a role claim names (#1193), declared and never
                    // emitted: the claim is checked on the descriptor, and
                    // clause 6 tolerates paths of a kind the corpus lacks.
                    match self.behavior {
                        Behavior::ClaimsCi | Behavior::ClaimsCiWithoutRunLog => {
                            kinds.push(kind("pipeline", "Pipeline", "PI"));
                            kinds.push(kind("run", "Run", "RU"));
                        }
                        Behavior::ClaimsWikiWithoutParent => kinds.push(kind("page", "Page", "PG")),
                        Behavior::ClaimsMonitoringWithoutState => {
                            kinds.push(kind("monitor", "Monitor", "MO"));
                        }
                        _ => {}
                    }
                    if self.behavior == Behavior::WrongPathsForAKindTheCorpusNeverPopulates {
                        kinds.push(kind("gadget", "Gadget", "GA"));
                    }
                    kinds
                },
                config_schema: serde_json::json!({ "type": "object", "properties": {} }),
                // The conforming adapter declares a status and an assignee,
                // and its items carry a status and an explicit `null`
                // assignee -- so the happy path exercises both halves of the
                // clause: a path that lands, and a source saying "nothing
                // here", which is what keeps this safe to run against a live
                // instance whose issues are unassigned.
                payload_paths: {
                    use crate::ParentPath;
                    use crate::{KindPaths, ListPath, PayloadPath, StatePath, StateValue, Tone};
                    let parent = |kind: &str, at: &[&str], name: &str| ParentPath {
                        kind: kind.to_owned(),
                        at: PayloadPath::of(at.iter().copied()),
                        names: vec![PayloadPath::of([name])],
                    };
                    // The conforming adapter's tickets sit under an epic, a
                    // ticket itself -- so its parent kind is one it emits.
                    let state = |at: &[&str], value: &str| {
                        vec![StatePath {
                            at: PayloadPath::of(at.iter().copied()),
                            values: vec![StateValue::of(value, Tone::Info)],
                        }]
                    };
                    let ticket = |status: PayloadPath| KindPaths {
                        kind: "ticket".to_owned(),
                        status_name: vec![status],
                        assignee: vec![PayloadPath::of(["fields", "assignee", "name"])],
                        parent: vec![parent("ticket", &["fields", "epic"], "key")],
                        state: state(&["fields", "status", "name"], "In Progress"),
                        ..KindPaths::default()
                    };
                    let good = ticket(PayloadPath::of(["fields", "status", "name"]));
                    // Every path the ticket-system role needs (#1193), each
                    // landing on the records below; `priority` only if asked.
                    let ticket_system = |priority: bool| KindPaths {
                        priority: if priority {
                            vec![PayloadPath::of(["fields", "priority", "name"])]
                        } else {
                            vec![]
                        },
                        issue_type: vec![PayloadPath::of(["fields", "issuetype", "name"])],
                        labels: vec![ListPath {
                            at: PayloadPath::of(["labels"]),
                            entry: PayloadPath::default(),
                        }],
                        project_key: vec![PayloadPath::of(["fields", "project", "key"])],
                        ..good.clone()
                    };
                    match self.behavior {
                        Behavior::PayloadPathsForAKindItDoesNotEmit => vec![KindPaths {
                            kind: "gadget".to_owned(),
                            ..good
                        }],
                        Behavior::PayloadPathsForOneKindTwice => {
                            vec![good.clone(), good]
                        }
                        Behavior::MisspelledPayloadPath => {
                            vec![ticket(PayloadPath::of(["fields", "status", "nam"]))]
                        }
                        Behavior::MisspelledPayloadPathAtTheRoot => {
                            vec![ticket(PayloadPath::of(["fieldz", "status", "name"]))]
                        }
                        Behavior::PayloadPathOntoAnObject => {
                            vec![ticket(PayloadPath::of(["fields", "status"]))]
                        }
                        Behavior::MisspelledReviewerEntry => vec![KindPaths {
                            reviewers: vec![ListPath {
                                at: PayloadPath::of(["reviewers"]),
                                entry: PayloadPath::of(["user"]),
                            }],
                            ..good
                        }],
                        // Every path here is wrong in a way the corpus would
                        // catch on a populated kind -- the status is
                        // misspelled at the root, the assignee names a key
                        // `fields` does not have -- and none of it is asked,
                        // because no item is of this kind.
                        Behavior::WrongPathsForAKindTheCorpusNeverPopulates => vec![
                            good,
                            KindPaths {
                                kind: "gadget".to_owned(),
                                status_name: vec![PayloadPath::of(["fieldz", "status", "name"])],
                                assignee: vec![PayloadPath::of(["fields", "assignerr"])],
                                ..KindPaths::default()
                            },
                        ],
                        // Two spellings of one field, in the shape a candidate
                        // list exists for. The first lands on every item; the
                        // second names a key `fields.status` does not have.
                        Behavior::ASecondCandidateNoItemResolves => vec![KindPaths {
                            status_name: vec![
                                PayloadPath::of(["fields", "status", "name"]),
                                PayloadPath::of(["fields", "status", "nam"]),
                            ],
                            ..good
                        }],
                        Behavior::ParentOfAKindItDoesNotEmit => vec![KindPaths {
                            parent: vec![parent("epic", &["fields", "epic"], "key")],
                            ..good
                        }],
                        Behavior::ParentAtOntoAString => vec![KindPaths {
                            parent: vec![parent("ticket", &["fields", "status", "name"], "key")],
                            ..good
                        }],
                        Behavior::ParentAtOntoAnArrayOfStrings => vec![KindPaths {
                            parent: vec![parent("ticket", &["labels"], "key")],
                            ..good
                        }],
                        Behavior::MisspelledParentAt => vec![KindPaths {
                            parent: vec![parent("ticket", &["fields", "epicc"], "key")],
                            ..good
                        }],
                        Behavior::MisspelledParentName => vec![KindPaths {
                            parent: vec![parent("ticket", &["fields", "epic"], "kee")],
                            ..good
                        }],
                        Behavior::ASecondParentNameNoRecordCarries => vec![KindPaths {
                            parent: vec![ParentPath {
                                names: vec![PayloadPath::of(["key"]), PayloadPath::of(["kee"])],
                                ..parent("ticket", &["fields", "epic"], "key")
                            }],
                            ..good
                        }],
                        Behavior::StateNoItemResolves => vec![KindPaths {
                            state: state(&["fields", "status", "name"], "Done"),
                            ..good
                        }],
                        Behavior::StateOntoAnObject => vec![KindPaths {
                            state: state(&["fields", "status"], "In Progress"),
                            ..good
                        }],
                        Behavior::FrameWithoutPipeline => vec![KindPaths {
                            frame: Some(knobas_spi_types::payload::Frame::Run),
                            ..good
                        }],
                        Behavior::PipelineWithoutName => vec![KindPaths {
                            frame: Some(knobas_spi_types::payload::Frame::Pipeline),
                            pipeline: vec![PayloadPath::of(["key"])],
                            ..good
                        }],
                        Behavior::StartedThatIsNoInstant => vec![KindPaths {
                            frame: Some(knobas_spi_types::payload::Frame::Run),
                            pipeline: vec![PayloadPath::of(["key"])],
                            started: vec![PayloadPath::of(["key"])],
                            ..good
                        }],
                        Behavior::TriggerAtAMissingKey => vec![KindPaths {
                            trigger: vec![knobas_spi_types::payload::TriggerPath {
                                at: PayloadPath::of(["triggerd"]),
                                values: vec![knobas_spi_types::payload::TriggerValue::of(
                                    "vcs",
                                    knobas_spi_types::payload::Trigger::Change,
                                )],
                            }],
                            ..good
                        }],
                        Behavior::StartedByAtAMissingKey => vec![KindPaths {
                            started_by: vec![PayloadPath::of(["startedBy"])],
                            ..good
                        }],
                        Behavior::StartedByOnRunsNobodyStarted => vec![KindPaths {
                            started_by: vec![PayloadPath::of(["startedBy"])],
                            trigger: vec![knobas_spi_types::payload::TriggerPath {
                                at: PayloadPath::of(["fields", "status", "name"]),
                                values: vec![knobas_spi_types::payload::TriggerValue::of(
                                    "In Progress",
                                    knobas_spi_types::payload::Trigger::Change,
                                )],
                            }],
                            ..good
                        }],
                        Behavior::StartedByOnRunsAPersonStarted => vec![KindPaths {
                            started_by: vec![PayloadPath::of(["startedBy"])],
                            trigger: vec![knobas_spi_types::payload::TriggerPath {
                                at: PayloadPath::of(["fields", "status", "name"]),
                                values: vec![knobas_spi_types::payload::TriggerValue::of(
                                    "In Progress",
                                    knobas_spi_types::payload::Trigger::Person,
                                )],
                            }],
                            ..good
                        }],
                        Behavior::ResponseTimeWithAFraction => vec![KindPaths {
                            response_time: vec![PayloadPath::of(["response_time_ms"])],
                            ..good
                        }],
                        Behavior::ResponseTimeThatIsNoNumber => vec![KindPaths {
                            response_time: vec![PayloadPath::of(["key"])],
                            ..good
                        }],
                        Behavior::BaseAtAMissingKey => vec![KindPaths {
                            base: vec![PayloadPath::of(["bse"])],
                            ..good
                        }],
                        Behavior::SavedViewWithoutQuery => vec![KindPaths {
                            frame: Some(knobas_spi_types::payload::Frame::SavedView),
                            ..good
                        }],
                        Behavior::SavedViewWithItsPaths => vec![KindPaths {
                            frame: Some(knobas_spi_types::payload::Frame::SavedView),
                            source_query: vec![PayloadPath::of(["key"])],
                            board_type: vec![PayloadPath::of(["fields", "issuetype", "name"])],
                            sub_query: vec![PayloadPath::of(["fields", "project", "key"])],
                            sprint: vec![PayloadPath::of(["fields", "epic"])],
                            ..good
                        }],
                        Behavior::SprintThatIsNoObject => vec![KindPaths {
                            sprint: vec![PayloadPath::of(["key"])],
                            ..good
                        }],
                        Behavior::AgentAtAMissingKey => vec![KindPaths {
                            agent: vec![PayloadPath::of(["agnt"])],
                            ..good
                        }],
                        Behavior::CurrentStepOnRunsNotInFlight => vec![KindPaths {
                            current_step: vec![PayloadPath::of(["stage"])],
                            started: vec![PayloadPath::of(["started"])],
                            finished: vec![PayloadPath::of(["finished"])],
                            ..good
                        }],
                        Behavior::CurrentStepOnRunsInFlight => vec![KindPaths {
                            current_step: vec![PayloadPath::of(["stage"])],
                            started: vec![PayloadPath::of(["started"])],
                            finished: vec![PayloadPath::of(["finished"])],
                            ..good
                        }],
                        Behavior::TestCountsOnRunsWithoutTests => vec![KindPaths {
                            tests_failed: vec![PayloadPath::of(["tests", "failed"])],
                            tests_passed: vec![PayloadPath::of(["tests", "passed"])],
                            tests_ignored: vec![PayloadPath::of(["tests", "ignored"])],
                            ..good
                        }],
                        Behavior::TestCountMissingFromASummary => vec![KindPaths {
                            tests_failed: vec![PayloadPath::of(["tests", "failed"])],
                            tests_passed: vec![PayloadPath::of(["tests", "pased"])],
                            ..good
                        }],
                        Behavior::ClaimsTicketSystem
                        | Behavior::ClaimsTicketSystemAndAnswersTheTransitionRead => {
                            vec![ticket_system(true)]
                        }
                        Behavior::ClaimsTicketSystemWithoutPriority => vec![ticket_system(false)],
                        Behavior::ClaimsCi | Behavior::ClaimsCiWithoutRunLog => {
                            use knobas_spi_types::payload::Frame;
                            let at = |key: &str| vec![PayloadPath::of([key])];
                            vec![
                                good,
                                KindPaths {
                                    kind: "pipeline".to_owned(),
                                    frame: Some(Frame::Pipeline),
                                    name: at("name"),
                                    pipeline: at("id"),
                                    ..KindPaths::default()
                                },
                                KindPaths {
                                    kind: "run".to_owned(),
                                    frame: Some(Frame::Run),
                                    pipeline: at("pipeline"),
                                    state: state(&["status"], "SUCCESS"),
                                    number: at("number"),
                                    branch: at("branch"),
                                    started: at("started"),
                                    finished: at("finished"),
                                    ..KindPaths::default()
                                },
                            ]
                        }
                        Behavior::ClaimsWikiWithoutParent => vec![
                            good,
                            KindPaths {
                                kind: "page".to_owned(),
                                project_key: vec![PayloadPath::of(["space", "key"])],
                                ..KindPaths::default()
                            },
                        ],
                        Behavior::ClaimsMonitoringWithoutState => vec![
                            good,
                            KindPaths {
                                kind: "monitor".to_owned(),
                                monitor_type: vec![PayloadPath::of(["type"])],
                                ..KindPaths::default()
                            },
                        ],
                        _ => vec![good],
                    }
                },
            }
        }

        async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
            // Only the healthy path: a faulted instance still classifies its
            // fault correctly, so this behavior fails clause 3's new
            // assertion and nothing else.
            if self.behavior == Behavior::FailsWhileHealthy && self.fault == Fault::None {
                return Err(SourceError::protocol("the server said no"));
            }
            match self.faulted(
                self.behavior == Behavior::MisclassifiesAuthOnConnect,
                self.behavior == Behavior::MisclassifiesReachOnConnect,
            ) {
                Some(err) => Err(err),
                None => Ok(ConnectionInfo {
                    account: Some("test".into()),
                    ..ConnectionInfo::default()
                }),
            }
        }

        async fn sync(
            &self,
            cursor: Option<Cursor>,
            sink: &mut (dyn Sink + Send),
        ) -> Result<Cursor, SourceError> {
            if let Some(err) = self.faulted(
                self.behavior == Behavior::MisclassifiesAuthOnSync,
                self.behavior == Behavior::MisclassifiesReachOnSync,
            ) {
                return Err(err);
            }
            let quiet = self.behavior == Behavior::Null
                || (cursor.is_some() && self.behavior != Behavior::IgnoresCursor);
            if quiet {
                return Ok(match self.behavior {
                    // Nothing fetched, yet the position moved: the shape of an
                    // adapter that stamps `now()` into its cursor.
                    Behavior::MovesTheCursorWhenIdle => "2".into(),
                    _ => "1".into(),
                });
            }
            let namespace = match self.behavior {
                Behavior::ForeignNamespace => "elsewhere",
                Behavior::BlankSourceId => "",
                // Consistent with its own descriptor id, so the only thing
                // wrong with this adapter is that the id is not a slug.
                Behavior::NonSlugSourceId => "Test_Source",
                _ => "test",
            };
            let key = match self.behavior {
                Behavior::IllFormedId => "",
                _ => "1",
            };
            let kind = match self.behavior {
                Behavior::UndeclaredKind => "gadget",
                _ => "ticket",
            };
            let pushed = sink
                .item(SyncItem {
                    entity: EntityRef::new(namespace, key),
                    kind: kind.into(),
                    title: "One".into(),
                    body_text: "the first item".into(),
                    author: None,
                    updated_at: None,
                    // Jira Data Center's shape, because that is what the
                    // declarations above address: a status the paths reach,
                    // and an assignee the source explicitly says nothing
                    // about.
                    payload: {
                        let mut payload = serde_json::json!({
                            "key": "PAY-1",
                            "fields": {
                                "status": { "name": "In Progress" },
                                "assignee": serde_json::Value::Null,
                                "epic": { "key": "PAY-100" },
                                "priority": { "name": "High" },
                                "issuetype": { "name": "Bug" },
                                "project": { "key": "PAY" }
                            },
                            "labels": ["payments"],
                            "reviewers": [{ "login": "mara.lindqvist" }]
                        });
                        match self.behavior {
                            Behavior::ParentRecordsLackTheirName => {
                                if let Some(record) = payload.as_object_mut() {
                                    record.remove("key");
                                }
                            }
                            Behavior::ParentRecordsNameIsNotAString => {
                                payload["key"] = serde_json::json!({ "id": 1 });
                            }
                            // A run in flight (#1004): started, not finished.
                            Behavior::CurrentStepOnRunsInFlight => {
                                payload["started"] = serde_json::json!("2026-08-22T10:20:00Z");
                            }
                            Behavior::ResponseTimeWithAFraction => {
                                payload["response_time_ms"] = serde_json::json!(13.5);
                            }
                            // A run that ran tests (#1448), as TeamCity
                            // serves its summary: every count, zeros too.
                            Behavior::TestCountMissingFromASummary => {
                                payload["tests"] =
                                    serde_json::json!({ "failed": 1, "passed": 41, "ignored": 0 });
                            }
                            // A finished run, which is doing nothing now.
                            Behavior::CurrentStepOnRunsNotInFlight => {
                                payload["started"] = serde_json::json!("2026-08-22T10:20:00Z");
                                payload["finished"] = serde_json::json!("2026-08-22T10:24:12Z");
                            }
                            _ => {}
                        }
                        payload
                    },
                    web_url: None,
                    deleted: false,
                })
                .await;
            if self.behavior != Behavior::SwallowsSinkError {
                pushed?;
            }
            Ok("1".into())
        }

        /// The item its own full sync emits for `entity`, bent as the
        /// behavior says; `NotOffered` for every behavior that is not about
        /// clause 13.
        async fn refresh(&self, entity: &str) -> Result<crate::RefreshAnswer, SourceError> {
            match self.behavior {
                Behavior::Refreshes
                | Behavior::RefreshesWithoutTheCapability
                | Behavior::RefreshesAKindDeclaredNotRefreshed
                | Behavior::RefreshesAnotherTitle
                | Behavior::RefreshesAnotherStatus => {}
                Behavior::RefreshesALiveItemAsGone => return Ok(crate::RefreshAnswer::Gone),
                _ => return Ok(crate::RefreshAnswer::NotOffered),
            }
            let mut synced = super::VecSink(Vec::new());
            self.sync(None, &mut synced).await?;
            let Some(mut item) = synced
                .0
                .into_iter()
                .find(|item| item.entity.to_string() == entity)
            else {
                return Ok(crate::RefreshAnswer::Gone);
            };
            match self.behavior {
                Behavior::RefreshesAnotherTitle => item.title = "Renamed".into(),
                Behavior::RefreshesAnotherStatus => {
                    item.payload["fields"]["status"]["name"] = "Done".into();
                }
                _ => {}
            }
            Ok(crate::RefreshAnswer::Item(item))
        }

        async fn write(&self, op: WriteOp) -> Result<crate::WriteReceipt, SourceError> {
            if self.behavior == Behavior::DeclaresAWriteOp
                && self
                    .descriptor()
                    .write_ops
                    .iter()
                    .any(|w| w == op.identifier())
            {
                // A real adapter would post a comment to a live system here.
                // Only for the op this adapter *declares*: the battery probes
                // every op it knows, and refusing the undeclared ones is what
                // this adapter is otherwise required to do.
                unreachable!("battery must not perform a write the descriptor declares");
            }
            if self.behavior == Behavior::AcceptsUndeclaredWrite
                || self.behavior == Behavior::PerformsAnUndeclaredSourceAction
                    && matches!(op, WriteOp::SourceAction { .. })
            {
                return Ok(crate::WriteReceipt::none());
            }
            if let WriteOp::SourceAction { id, .. } = &op
                && self.behavior == Behavior::DeclaresASourceAction
                && self.descriptor().source_actions.contains(id)
            {
                unreachable!("battery must not perform a source action the descriptor declares");
            }
            Err(SourceError::protocol(format!("unsupported op: {op:?}")))
        }

        /// The conforming adapter draws an overview and a deferred log, so the
        /// happy path walks both halves of clause 9.
        async fn view(
            &self,
            _entity: &str,
            section: Option<&str>,
        ) -> Result<Option<crate::view::SourceView>, SourceError> {
            use crate::view::{Block, SourceView, ViewSection};
            let overview = |id: &str| ViewSection {
                id: id.into(),
                title: "Overview".into(),
                count: None,
                deferred: false,
                blocks: vec![Block::Text {
                    text: "the first item".into(),
                    markdown: false,
                }],
            };
            let log = |deferred: bool| ViewSection {
                id: "log".into(),
                title: "Log".into(),
                count: None,
                deferred,
                blocks: if deferred {
                    vec![]
                } else {
                    vec![Block::Log {
                        text: "ok".into(),
                        truncated: false,
                    }]
                },
            };
            match (self.behavior, section) {
                (Behavior::DrawsNoView, _) => Ok(None),
                (Behavior::FailsTheView, _) => Err(SourceError::protocol("no view today")),
                (Behavior::RepeatsASectionId, _) => Ok(Some(SourceView {
                    actions: vec![],
                    sections: vec![overview("overview"), overview("overview")],
                })),
                (Behavior::DefersASectionItCannotRead, Some(_)) => Ok(Some(SourceView {
                    actions: vec![],
                    sections: vec![overview("overview")],
                })),
                (_, None) => Ok(Some(SourceView {
                    actions: vec![],
                    sections: vec![overview("overview"), log(true)],
                })),
                (_, Some("log")) => Ok(Some(SourceView {
                    actions: vec![],
                    sections: vec![log(false)],
                })),
                (_, Some(other)) => Err(SourceError::protocol(format!("no section {other:?}"))),
            }
        }

        async fn reachable_transitions(&self, _entity: &str) -> Result<Vec<String>, SourceError> {
            match self.behavior {
                // A real adapter with a workflow would ask its source here.
                // Clause 8 skips this side, and this is what says so.
                Behavior::DeclaresTransition => {
                    unreachable!("battery must not read the workflow of an adapter that has one")
                }
                Behavior::AnswersTheTransitionRead
                | Behavior::ClaimsTicketSystemAndAnswersTheTransitionRead => {
                    Ok(vec!["In Progress".into()])
                }
                Behavior::RefusesTheReadAnonymously => {
                    Err(SourceError::protocol("no workflow here"))
                }
                _ => Err(SourceError::protocol(
                    "the test adapter has no workflow to read",
                )),
            }
        }
    }

    /// Run the battery against `behavior` on its own task, so a battery panic
    /// surfaces as a `JoinError` instead of taking the test process down. The
    /// recovered panic message is what each negative case asserts on -- without
    /// it, one assertion could silently stop being enforced while another kept
    /// the test green.
    async fn run(behavior: Behavior) -> Result<(), String> {
        let joined = tokio::spawn(async move {
            battery(move |fault| Box::new(TestSource { fault, behavior }) as Box<dyn Source>).await;
        })
        .await;
        joined.map_err(|err| {
            assert!(err.is_panic(), "battery task must fail by panicking");
            let payload = err.into_panic();
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_else(|| "<non-string panic payload>".to_owned())
        })
    }

    /// Assert the battery rejects `behavior`, and that it does so at the
    /// intended assertion rather than tripping over some other one.
    async fn rejects(behavior: Behavior, expected: &str) {
        let msg = run(behavior).await.expect_err("battery must reject");
        assert!(
            msg.contains(expected),
            "expected a panic mentioning {expected:?}, got: {msg}"
        );
    }

    /// Without this the negative cases below prove nothing: a battery that
    /// panics unconditionally would pass all of them.
    #[tokio::test]
    async fn accepts_a_conforming_adapter() {
        run(Behavior::Good).await.expect("conforming adapter");
    }

    /// Pins the guard that skips declared ops -- the one part of clause 5 that
    /// is control flow rather than an assertion, so the mutation sweep cannot
    /// reach it. This adapter's `write` panics; the battery accepting it proves
    /// `write` was never called. Delete the guard and this goes red, which is
    /// what stops the battery from posting a real comment through an adapter
    /// running it against a live backend -- and from rejecting every adapter
    /// that correctly implements the op it declared, Task 7's mock included.
    #[tokio::test]
    async fn accepts_a_declared_write_op_without_calling_write() {
        run(Behavior::DeclaresAWriteOp)
            .await
            .expect("an adapter that declares an op it supports must pass, uncalled");
    }

    // -- clause 1: full sync ------------------------------------------------

    #[tokio::test]
    async fn rejects_a_do_nothing_adapter() {
        rejects(Behavior::Null, "no items").await;
    }

    #[tokio::test]
    async fn rejects_a_blank_source_id() {
        rejects(Behavior::BlankSourceId, "descriptor.id must not be blank").await;
    }

    /// An adapter named after one of knobas' own namespaces passes every other
    /// clause and then fails every real sync: the engine refuses the run
    /// outright. Catching it here is the difference between a red test in the
    /// adapter's own suite and a source that installs and never works.
    #[tokio::test]
    async fn rejects_a_source_id_knobas_keeps_for_itself() {
        rejects(
            Behavior::ReservedSourceId,
            "is a namespace knobas keeps for its own entities",
        )
        .await;
    }

    /// The id is also what a human types into the Add-source form and what
    /// every entity id this source writes is prefixed with, so free text is
    /// refused before it becomes immutable.
    #[tokio::test]
    async fn rejects_an_id_that_is_not_a_slug() {
        rejects(Behavior::NonSlugSourceId, "is not a usable instance id").await;
    }

    #[tokio::test]
    async fn rejects_a_foreign_namespace() {
        rejects(Behavior::ForeignNamespace, "not namespaced").await;
    }

    #[tokio::test]
    async fn rejects_an_ill_formed_item_id() {
        rejects(Behavior::IllFormedId, "does not survive a round trip").await;
    }

    #[tokio::test]
    async fn rejects_an_undeclared_kind() {
        rejects(
            Behavior::UndeclaredKind,
            "not declared in descriptor.entity_kinds",
        )
        .await;
    }

    // -- clause 2: incremental sync ------------------------------------------

    #[tokio::test]
    async fn rejects_an_adapter_that_ignores_the_cursor() {
        rejects(Behavior::IgnoresCursor, "incremental sync").await;
    }

    /// The other half of clause 2: an idle sync must not move the position.
    /// The engine's "this run changed nothing" test is exactly `no items and
    /// the same cursor`, so an adapter that fails this one quietly fills the
    /// activity log instead of failing anything.
    #[tokio::test]
    async fn rejects_an_adapter_whose_cursor_moves_while_idle() {
        rejects(
            Behavior::MovesTheCursorWhenIdle,
            "must return the cursor it was given",
        )
        .await;
    }

    // -- clauses 3 and 4: connection and fault classification -----------------

    /// The happy path of `test_connection` is what the Add-source flow runs
    /// first, so an adapter that cannot connect when nothing is wrong is
    /// caught here rather than by a user staring at a red dialog.
    #[tokio::test]
    async fn rejects_an_adapter_that_cannot_connect_when_nothing_is_wrong() {
        rejects(Behavior::FailsWhileHealthy, "test_connection must succeed").await;
    }

    #[tokio::test]
    async fn rejects_an_auth_failure_misclassified_by_test_connection() {
        rejects(
            Behavior::MisclassifiesAuthOnConnect,
            "test_connection must map an auth failure",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_connectivity_failure_misclassified_by_test_connection() {
        rejects(
            Behavior::MisclassifiesReachOnConnect,
            "test_connection must map a connectivity failure",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_an_auth_failure_misclassified_by_sync() {
        rejects(
            Behavior::MisclassifiesAuthOnSync,
            "sync must map an auth failure",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_connectivity_failure_misclassified_by_sync() {
        rejects(
            Behavior::MisclassifiesReachOnSync,
            "sync must map a connectivity failure",
        )
        .await;
    }

    // -- clause 5: write ops --------------------------------------------------

    #[tokio::test]
    async fn rejects_an_unknown_write_op_identifier() {
        rejects(
            Behavior::UnknownWriteOpId,
            "is not a WriteOp identifier the SPI knows",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_write_capability_with_no_write_ops() {
        rejects(
            Behavior::WriteCapabilityWithoutOps,
            "declares Capability::Write but lists no write_ops",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_write_ops_without_the_write_capability() {
        rejects(
            Behavior::WriteOpsWithoutCapability,
            "but does not declare Capability::Write",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_an_adapter_that_performs_an_undeclared_write() {
        rejects(
            Behavior::AcceptsUndeclaredWrite,
            "must be refused with SourceError::Protocol",
        )
        .await;
    }

    // -- clause 6: declared payload paths (#277) ------------------------------

    /// A declaration for a kind the adapter does not emit is a declaration
    /// nothing will ever read: no item carries that kind, so no reader ever
    /// looks it up, and the field it names silently misses for ever.
    #[tokio::test]
    async fn rejects_payload_paths_for_a_kind_the_adapter_does_not_emit() {
        rejects(
            Behavior::PayloadPathsForAKindItDoesNotEmit,
            "which is not one of descriptor.entity_kinds",
        )
        .await;
    }

    /// Readers take the first entry for a kind, so a second one is a
    /// declaration that does nothing -- and the two can disagree.
    #[tokio::test]
    async fn rejects_one_kinds_payload_paths_declared_twice() {
        rejects(Behavior::PayloadPathsForOneKindTwice, "declares kind").await;
    }

    /// The typo case, and the reason this clause exists at all. A path one key
    /// off resolves to nothing on every record for ever, and a path-driven
    /// read that misses is indistinguishable downstream from a source that
    /// says nothing -- so if the adapter's own corpus does not catch it here,
    /// nothing catches it.
    #[tokio::test]
    async fn rejects_a_path_into_an_object_that_does_not_have_that_key() {
        rejects(
            Behavior::MisspelledPayloadPath,
            "at a path no item of that kind resolves",
        )
        .await;
    }

    /// The same typo one segment earlier, which has no container anywhere in
    /// the corpus and would pass a clause that asked "does the object this key
    /// would sit in exist?" -- it does not, because the *previous* key is the
    /// wrong one. Asking instead "does some record show this path naming a key
    /// it does not have?" catches both, and a null still excuses neither the
    /// declaration nor a reader.
    #[tokio::test]
    async fn rejects_a_path_misspelled_before_its_last_segment() {
        rejects(
            Behavior::MisspelledPayloadPathAtTheRoot,
            "at a path no item of that kind resolves",
        )
        .await;
    }

    /// A path that stops one segment short lands on the object. `->>` would
    /// stringify it into a status called `{"name":"In Progress"}` -- a guess
    /// dressed as an observation, which is what the type checks refuse.
    #[tokio::test]
    async fn rejects_a_path_that_lands_on_an_object() {
        rejects(
            Behavior::PayloadPathOntoAnObject,
            "which leads to an object on this adapter's own item",
        )
        .await;
    }

    /// The same for a list: the array is found and the key inside its elements
    /// is not, so the rule that walks it finds nobody, for ever.
    #[tokio::test]
    async fn rejects_a_reviewer_entry_key_the_elements_do_not_carry() {
        rejects(
            Behavior::MisspelledReviewerEntry,
            "declares reviewers of kind \"ticket\" at a path no item of that kind resolves",
        )
        .await;
    }

    // -- clause 6 on the parent declaration (#595) ---------------------------
    //
    // The same rules as every other declared field, applied to `parent`: its
    // kind is one the adapter emits, its `at` leads to an object or an array
    // of objects, each name inside a parent leads to a string, and where
    // nothing of a kind resolved, no record names a key it does not have. A
    // wrong parent declaration otherwise surfaces only as an empty `under:`,
    // which looks exactly like an empty repo.

    #[tokio::test]
    async fn rejects_a_parent_of_a_kind_the_adapter_does_not_emit() {
        rejects(
            Behavior::ParentOfAKindItDoesNotEmit,
            "declares a parent of kind \"epic\" for kind \"ticket\", which is not one of \
             descriptor.entity_kinds",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_parent_at_that_leads_to_a_string() {
        rejects(
            Behavior::ParentAtOntoAString,
            "declares parent of kind \"ticket\" at [\"fields\", \"status\", \"name\"], which \
             leads to a string",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_parent_at_that_leads_to_an_array_of_strings() {
        rejects(
            Behavior::ParentAtOntoAnArrayOfStrings,
            "declares parent of kind \"ticket\" at [\"labels\"], which leads to an array that \
             is not all objects",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_parent_at_a_key_the_records_do_not_have() {
        rejects(
            Behavior::MisspelledParentAt,
            "declares parent of kind \"ticket\" at a path no item of that kind resolves",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_parent_name_key_the_parent_objects_do_not_carry() {
        rejects(
            Behavior::MisspelledParentName,
            "declares parent names of kind \"ticket\" at a path no item of that kind resolves",
        )
        .await;
    }

    // -- clause 6 on the state declaration (#666) ----------------------------
    //
    // A state's `at` leads to a string or a boolean, and a kind that declares
    // one must draw a chip on some record of the adapter's own corpus: a value
    // map that names nothing the source writes is a chip no row ever shows,
    // which looks exactly like a source with no state.

    #[tokio::test]
    async fn rejects_a_state_no_item_draws_a_chip_for() {
        rejects(
            Behavior::StateNoItemResolves,
            "declares state of kind \"ticket\", and no item of that kind draws a chip",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_state_at_that_leads_to_an_object() {
        rejects(
            Behavior::StateOntoAnObject,
            "declares state of kind \"ticket\" at [\"fields\", \"status\"], which leads to an \
             object",
        )
        .await;
    }

    // -- the Builds frame (ADR-0022, #848) -------------------------------------

    #[tokio::test]
    async fn rejects_a_frame_kind_with_no_pipeline_path() {
        rejects(Behavior::FrameWithoutPipeline, "but no `pipeline` path").await;
    }

    #[tokio::test]
    async fn rejects_a_pipeline_with_no_name_path() {
        rejects(
            Behavior::PipelineWithoutName,
            "a pipeline but no `name` path",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_trigger_at_a_key_no_record_has() {
        rejects(
            Behavior::TriggerAtAMissingKey,
            "declares trigger of kind \"ticket\" at a path no item of that kind resolves",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_started_by_at_a_key_no_record_has() {
        rejects(
            Behavior::StartedByAtAMissingKey,
            "declares started_by of kind \"ticket\" at a path no item of that kind resolves",
        )
        .await;
    }

    /// A run a change or a schedule started names nobody (#961): a source
    /// that omits the key there is saying so, as TeamCity does for every VCS
    /// build, and is not judged by it.
    #[tokio::test]
    async fn accepts_a_started_by_absent_from_runs_nobody_started() {
        run(Behavior::StartedByOnRunsNobodyStarted)
            .await
            .expect("runs a change started name nobody");
    }

    /// The excuse is the trigger's, not the declaration's (#961): on runs
    /// whose declared trigger is a person, an absent `started_by` key is a
    /// misspelled path again.
    #[tokio::test]
    async fn rejects_a_started_by_absent_from_runs_a_person_started() {
        rejects(
            Behavior::StartedByOnRunsAPersonStarted,
            "declares started_by of kind \"ticket\" at a path no item of that kind resolves",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_an_agent_at_a_key_no_record_has() {
        rejects(
            Behavior::AgentAtAMissingKey,
            "declares agent of kind \"ticket\" at a path no item of that kind resolves",
        )
        .await;
    }

    /// A run that ran no tests has no summary (#1448): TeamCity serves no
    /// `testOccurrences` on such a build, so its absence is not judged.
    #[tokio::test]
    async fn accepts_test_counts_absent_from_runs_without_tests() {
        run(Behavior::TestCountsOnRunsWithoutTests)
            .await
            .expect("a run that ran no tests has no test summary");
    }

    /// The excuse is the summary's, not the declaration's (#1448): where one
    /// count reads, a count missing beside it is a misspelled path.
    #[tokio::test]
    async fn rejects_a_test_count_missing_from_a_summary_that_reads() {
        rejects(
            Behavior::TestCountMissingFromASummary,
            "declares tests_passed of kind \"ticket\" at a path no item of that kind resolves",
        )
        .await;
    }

    /// A finished run is doing nothing now (#1004): TeamCity serves
    /// `running-info` only while a build runs, so its absence there is not
    /// judged.
    #[tokio::test]
    async fn accepts_a_current_step_absent_from_runs_not_in_flight() {
        run(Behavior::CurrentStepOnRunsNotInFlight)
            .await
            .expect("a finished run is doing nothing now");
    }

    /// The excuse is the run's, not the declaration's (#1004): on a run in
    /// flight, an absent `current_step` key is a misspelled path again.
    #[tokio::test]
    async fn rejects_a_current_step_absent_from_runs_in_flight() {
        rejects(
            Behavior::CurrentStepOnRunsInFlight,
            "declares current_step of kind \"ticket\" at a path no item of that kind resolves",
        )
        .await;
    }

    #[tokio::test]
    async fn accepts_a_response_time_with_a_fraction() {
        run(Behavior::ResponseTimeWithAFraction)
            .await
            .expect("a reading in milliseconds may carry a fraction");
    }

    #[tokio::test]
    async fn rejects_a_response_time_that_is_no_number() {
        rejects(
            Behavior::ResponseTimeThatIsNoNumber,
            "declares response_time of kind \"ticket\" at [\"key\"], which leads to a string",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_base_at_a_key_no_record_has() {
        rejects(
            Behavior::BaseAtAMissingKey,
            "declares base of kind \"ticket\" at a path no item of that kind resolves",
        )
        .await;
    }

    // -- a ticket system's saved views (#1207, #2050) ---------------------------

    #[tokio::test]
    async fn rejects_a_saved_view_kind_with_no_query_path() {
        rejects(
            Behavior::SavedViewWithoutQuery,
            "a saved view but no `source_query` path",
        )
        .await;
    }

    #[tokio::test]
    async fn accepts_a_saved_view_kind_that_declares_its_paths() {
        run(Behavior::SavedViewWithItsPaths)
            .await
            .expect("a query, a board type, a sub-query and a sprint object");
    }

    #[tokio::test]
    async fn rejects_a_sprint_that_is_no_object() {
        rejects(
            Behavior::SprintThatIsNoObject,
            "declares sprint of kind \"ticket\" at [\"key\"], which leads to a string",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_started_time_that_is_no_instant() {
        rejects(
            Behavior::StartedThatIsNoInstant,
            "declares started of kind \"ticket\" at [\"key\"], which leads to a string",
        )
        .await;
    }

    // -- clause 6 on a parent's own records (the deputy's ruling of
    // 2026-10-11 on #595) ----------------------------------------------------
    //
    // `under:` returns the parent itself by reading the children's declared
    // names off the parent's own record, so those names must resolve there
    // too. The conforming adapter's tickets carry `key` at their root, as the
    // epic objects inside them do.

    #[tokio::test]
    async fn rejects_parent_records_that_lack_the_name_their_children_give_them() {
        rejects(
            Behavior::ParentRecordsLackTheirName,
            "declares the parent's own names of kind \"ticket\" at a path no item of that kind \
             resolves",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_parent_record_whose_name_is_not_a_string() {
        rejects(
            Behavior::ParentRecordsNameIsNotAString,
            "declares the parent's own names of kind \"ticket\" at [\"key\"], which leads to an \
             object on this adapter's own item",
        )
        .await;
    }

    // -- clause 6's two tolerances (#301) -------------------------------------
    //
    // Both are deliberate, both are documented on `check_payload_paths` and in
    // the §10.8 #277 entry, and until now neither was pinned. A tightening of
    // either would pass every other test in this module and fail only against a
    // live instance with the shape the tolerance exists for, which is the
    // unwitnessed-wire class. These two say out loud that the acceptance is the
    // contract: changing it is a decision, not a refactor.

    /// Tolerance 1 of clause 6, reproduced and documented by #301's merge
    /// review: **a kind the corpus never populates is asked nothing.**
    ///
    /// This adapter declares `gadget` in `entity_kinds` and gives it a
    /// declaration that is wrong in both of the ways clause 3 catches on a
    /// populated kind -- and emits no `gadget` item. The evidence clause 6
    /// weighs is gathered inside the walk over the corpus, so a kind with no
    /// items contributes none and nothing is asked of its declaration.
    ///
    /// Demanding instead that every declared kind be populated would fail a
    /// battery run against any instance that happens to have no build
    /// configurations -- the same move clause 3 refuses for an unassigned
    /// issue, and the property that makes this clause safe against a live
    /// instance. Clause 1 is what still holds here: `gadget` must at least be
    /// a kind the adapter declares it emits.
    #[tokio::test]
    async fn accepts_a_wrong_declaration_for_a_kind_the_corpus_never_populates() {
        run(Behavior::WrongPathsForAKindTheCorpusNeverPopulates)
            .await
            .expect(
                "a kind with no items in the corpus must be asked nothing about its declared \
                 paths -- an instance with none of that kind must certify",
            );
    }

    /// Tolerance 2 of clause 6, reproduced and documented by #301's merge
    /// review: **a candidate an earlier candidate resolves for is excused.**
    ///
    /// The candidates of one field share a single evidence slot across the
    /// corpus, so a second spelling that names a key no item has passes once
    /// the first has landed anywhere. Here that is `fields.status.nam` behind a
    /// working `fields.status.name`.
    ///
    /// The documented example is `fields.assignee.keyy` behind
    /// `fields.assignee.name`, and it is deliberately *not* what this fixture
    /// uses: its items leave `fields.assignee` explicitly null, so both
    /// candidates stop at that null and read as a source saying nothing --
    /// neither resolves, neither names a missing key, and per-candidate
    /// evidence would accept them too. That pair witnesses nothing here. The
    /// tolerance needs a field this corpus really resolves, and `status_name`
    /// is the one it has.
    ///
    /// Per-candidate evidence would fail the *second* spelling on every
    /// instance that uses the first, which is precisely the case a candidate
    /// list exists for. So a candidate list is certified as a whole, and the
    /// first candidate is the one clause 3 really pins.
    #[tokio::test]
    async fn accepts_a_later_candidate_an_earlier_one_resolves_for() {
        run(Behavior::ASecondCandidateNoItemResolves).await.expect(
            "a field's candidates are one adapter's alternative spellings and share one \
                 evidence slot -- a spelling this instance does not use must not fail it",
        );
    }

    /// The second tolerance as it reaches a parent's names, on the children's
    /// parent objects and on the parent's own records alike. A parent answers
    /// to every name that resolves (`resolve_names_as_parent` unions them), so
    /// a name no record carries is one `under:` can never reach the parent by
    /// -- but the deputy's ruling of 2026-10-11 on #595 put this rule "under
    /// the same two #301 tolerances", and per-name slots would be a
    /// tightening. If that is ever ruled, this test goes red first.
    #[tokio::test]
    async fn accepts_a_parent_name_an_earlier_name_resolves_for() {
        run(Behavior::ASecondParentNameNoRecordCarries)
            .await
            .expect("a parent's names share one evidence slot, as a field's candidates do");
    }

    // -- clause 7: sink failures ---------------------------------------------

    #[tokio::test]
    async fn rejects_an_adapter_that_swallows_a_sink_error() {
        rejects(
            Behavior::SwallowsSinkError,
            "must be propagated out of sync",
        )
        .await;
    }

    // -- clause 8: the reachable-transition read (#498) -----------------------

    /// The mirror image of `accepts_a_declared_write_op_without_calling_write`,
    /// and it exists for the same reason: the skip is control flow, so nothing
    /// else can witness it. This adapter declares `transition` and panics if
    /// its workflow is read; the battery accepting it is the proof the read was
    /// never made. Delete the guard and this goes red -- which is what stops
    /// the battery from asking a live Jira about an issue key no instance has,
    /// and being told "no such issue" in a shape it would then have to decide
    /// was a refusal.
    #[tokio::test]
    async fn accepts_an_adapter_that_answers_the_read() {
        run(Behavior::DeclaresTransition)
            .await
            .expect("an adapter with a workflow must pass clause 8 unread");
    }

    #[tokio::test]
    async fn rejects_a_source_with_no_workflow_that_answers_the_read() {
        rejects(
            Behavior::AnswersTheTransitionRead,
            "reachable_transitions must be refused",
        )
        .await;
    }

    /// A refusal that does not say who refused. The message is a real refusal
    /// -- `Protocol`, and it even says there is no workflow -- so only the
    /// naming clause can reject it, which is what makes this a test of that
    /// clause rather than of the one above.
    #[tokio::test]
    async fn rejects_a_refusal_that_does_not_name_the_adapter() {
        rejects(
            Behavior::RefusesTheReadAnonymously,
            "must name the adapter that has no workflow",
        )
        .await;
    }

    /// The ticket-system minimum and clause 8 agree (#1225): a ticket system
    /// that declares no `transition` claims the required read and answers it
    /// with a refusal by name, and one answering it with moves is refused.
    #[tokio::test]
    async fn accepts_a_read_only_ticket_system_that_refuses_its_transitions_read() {
        run(Behavior::ClaimsTicketSystem)
            .await
            .expect("a read-only ticket system refuses the read it claims");
    }

    #[tokio::test]
    async fn rejects_a_read_only_ticket_system_that_answers_its_transitions_read() {
        rejects(
            Behavior::ClaimsTicketSystemAndAnswersTheTransitionRead,
            "a ticket system that does not declare it claims the read and refuses it",
        )
        .await;
    }

    // -- clause 9: the source view (#661) ------------------------------------

    #[tokio::test]
    async fn accepts_an_adapter_that_draws_no_view() {
        run(Behavior::DrawsNoView)
            .await
            .expect("None is an answer: the adapter draws nothing for this kind");
    }

    #[tokio::test]
    async fn rejects_an_adapter_whose_view_of_its_own_entity_fails() {
        rejects(
            Behavior::FailsTheView,
            "must be answered with a view or with None",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_view_whose_section_ids_repeat() {
        rejects(Behavior::RepeatsASectionId, "blank or repeated section id").await;
    }

    #[tokio::test]
    async fn rejects_a_deferred_section_that_does_not_answer_its_own_read() {
        rejects(
            Behavior::DefersASectionItCannotRead,
            "must answer its own read with exactly that section",
        )
        .await;
    }

    // -- the role data check on a seeded corpus (#1199) -----------------------

    fn source(behavior: Behavior) -> Box<dyn Source> {
        Box::new(TestSource {
            fault: Fault::None,
            behavior,
        })
    }

    #[tokio::test]
    async fn a_ticket_system_whose_tickets_resolve_its_paths_fills_its_role() {
        super::fills_its_roles(&*source(Behavior::ClaimsTicketSystem)).await;
    }

    #[tokio::test]
    #[should_panic(expected = "role \"wiki\" is not shown filled")]
    async fn a_role_whose_kind_the_corpus_lacks_is_not_shown_filled() {
        super::fills_its_roles(&*source(Behavior::ClaimsWikiWithoutParent)).await;
    }

    // -- clause 10: role claims (#1193) ---------------------------------------

    #[tokio::test]
    async fn accepts_a_ticket_system_claim_its_tickets_fill() {
        run(Behavior::ClaimsTicketSystem)
            .await
            .expect("a ticket-system claim the tickets fill");
    }

    /// Pins the frame lookup: CI's kinds are named by the frame they
    /// declare, not by an id.
    #[tokio::test]
    async fn accepts_a_ci_claim_over_a_pipeline_and_a_run() {
        run(Behavior::ClaimsCi)
            .await
            .expect("a CI claim its pipeline and run fill");
    }

    #[tokio::test]
    async fn rejects_a_code_host_claim_without_a_repo_kind() {
        rejects(
            Behavior::ClaimsCodeHostWithoutItsKinds,
            r#"descriptor.roles: role "code-host" needs kind "repo""#,
        )
        .await;
    }

    /// Clause 10 holds the clone-URLs read (#1197) in its other direction: an
    /// address to clone from, declared by a source that claims no code host.
    #[tokio::test]
    async fn rejects_a_clone_address_without_a_code_host_claim() {
        rejects(
            Behavior::ClonesWithoutACodeHostClaim,
            r#"descriptor.roles: descriptor.clone_from names an address to clone from, and no "code-host" claim reads "clone_urls""#,
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_ticket_system_claim_whose_tickets_declare_no_priority() {
        rejects(
            Behavior::ClaimsTicketSystemWithoutPriority,
            r#"descriptor.roles: role "ticket-system" needs kind "ticket" to declare the path priority"#,
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_ci_claim_without_the_run_log_read() {
        rejects(
            Behavior::ClaimsCiWithoutRunLog,
            r#"descriptor.roles: role "ci" needs the read "run_log""#,
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_wiki_claim_whose_pages_declare_no_parent() {
        rejects(
            Behavior::ClaimsWikiWithoutParent,
            r#"descriptor.roles: role "wiki" needs kind "page" to declare the path parent"#,
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_monitoring_claim_whose_monitors_declare_no_state() {
        rejects(
            Behavior::ClaimsMonitoringWithoutState,
            r#"descriptor.roles: role "monitoring" needs kind "monitor" to declare the path status_name"#,
        )
        .await;
    }

    /// A custom role has no minimum: a claim namespaced to the adapter's own
    /// kind passes with no reads (#1201).
    #[tokio::test]
    async fn accepts_a_custom_role_namespaced_to_the_adapter() {
        run(Behavior::ClaimsItsCustomRole)
            .await
            .expect("a custom role of the adapter's own kind");
    }

    /// No other adapter fills a custom role (ADR-0029): adapter `test` may not
    /// claim `other-adapter:thing`.
    #[tokio::test]
    async fn rejects_a_custom_role_namespaced_to_another_adapter() {
        rejects(
            Behavior::ClaimsAnotherAdaptersCustomRole,
            r#"descriptor.roles: role "other-adapter:thing" is neither a built-in role (code-host, ticket-system, ci, wiki, monitoring) nor a custom role named "test:<name>""#,
        )
        .await;
    }

    // -- clause 11: source actions (#1194) --------------------------------------

    /// The conforming adapter's declared action is never performed: its
    /// `write` panics on it, as the declared-op guard of clause 5 is pinned.
    #[tokio::test]
    async fn accepts_a_declared_source_action_without_performing_it() {
        run(Behavior::DeclaresASourceAction)
            .await
            .expect("an adapter that refuses only the actions it does not declare must pass");
    }

    #[tokio::test]
    async fn rejects_an_adapter_that_performs_a_source_action_it_did_not_declare() {
        rejects(
            Behavior::PerformsAnUndeclaredSourceAction,
            "a source action descriptor.source_actions does not declare must be refused",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_source_actions_without_the_source_action_op() {
        rejects(
            Behavior::SourceActionsWithoutTheOp,
            "descriptor.source_actions declares [\"add_label\"] but write_ops does not list \"source_action\"",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_the_source_action_op_with_no_source_actions() {
        rejects(
            Behavior::SourceActionOpWithoutActions,
            "write_ops lists \"source_action\" but descriptor.source_actions declares none",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_source_action_declared_twice() {
        rejects(
            Behavior::RepeatsASourceAction,
            "descriptor.source_actions has a blank or repeated id \"add_label\"",
        )
        .await;
    }

    /// A source action is a write outside **every** role's vocabulary
    /// (ADR-0029), so no role's minimum may name it.
    #[test]
    fn no_role_names_the_source_action() {
        for min in crate::role::MINIMUMS {
            assert!(
                !min.writes.contains(&crate::SOURCE_ACTION),
                "role {:?} names source_action",
                min.role
            );
        }
    }

    /// Every probe answers a delivery whose target field the op really has
    /// (ADR-0006, amended 2026-10-19), and `WriteOp::target` reads that
    /// field: a `TargetField::Parent` on a variant with no `parent` would
    /// order and hold the write against nothing. That
    /// every variant has a probe and a delivery at all is the compiler's to
    /// say, through the no-wildcard matches; this test cannot see a missing
    /// variant, only a duplicated probe.
    #[test]
    fn every_write_op_answers_a_delivery_whose_target_it_carries() {
        use knobas_spi_types::delivery::TargetField;
        let known = super::known_write_ops("test");
        let mut identifiers = std::collections::BTreeSet::new();
        for (id, op) in &known {
            assert!(identifiers.insert(*id), "{id} has two probes");
            let field = match op.delivery().target {
                TargetField::Entity => "entity",
                TargetField::Parent => "parent",
                TargetField::Repository => "repo",
            };
            let wire = serde_json::to_value(op).expect("an op serialises");
            let fields = wire
                .as_object()
                .and_then(|variant| variant.values().next())
                .expect("an op is externally tagged");
            assert!(
                fields[field].as_str().is_some_and(|t| !t.is_empty()),
                "{id}: its delivery names `{field}` as the target, which it does not carry"
            );
            assert_eq!(
                Some(op.target()),
                fields[field].as_str(),
                "{id}: `target` reads another field than its delivery names"
            );
        }
    }

    // -- clause 13: the refresh (ADR-0031) -------------------------------------

    #[tokio::test]
    async fn accepts_an_adapter_that_reads_its_items_back_as_it_synced_them() {
        run(Behavior::Refreshes)
            .await
            .expect("an adapter that refreshes faithfully must pass");
    }

    #[tokio::test]
    async fn rejects_a_refresh_without_the_capability() {
        rejects(
            Behavior::RefreshesWithoutTheCapability,
            "does not declare Capability::Refresh",
        )
        .await;
    }

    /// A kind the descriptor says is refreshed and the refresh does not
    /// read: the detail would draw *Refresh* for a press that runs the
    /// source (#2146).
    #[tokio::test]
    async fn rejects_a_kind_declared_refreshed_that_the_refresh_does_not_offer() {
        rejects(
            Behavior::DeclaresRefreshAndOffersNone,
            "declares kind \"ticket\" refreshed but refresh answered NotOffered",
        )
        .await;
    }

    /// The other direction: a kind declared not refreshed that the refresh
    /// reads all the same draws *Sync now* for an item it could read.
    #[tokio::test]
    async fn rejects_a_refresh_of_a_kind_declared_not_refreshed() {
        rejects(
            Behavior::RefreshesAKindDeclaredNotRefreshed,
            "but the descriptor declares kind \"ticket\" not refreshed",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_the_capability_without_a_refresh() {
        rejects(
            Behavior::DeclaresRefreshForNoKind,
            "declares Capability::Refresh but refresh answered NotOffered",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_refresh_under_another_title() {
        rejects(Behavior::RefreshesAnotherTitle, "answered another title").await;
    }

    /// The declared path, not only the title: a status read back differently
    /// is a ticket that moves column when it is refreshed.
    #[tokio::test]
    async fn rejects_a_refresh_that_disagrees_at_a_declared_path() {
        rejects(
            Behavior::RefreshesAnotherStatus,
            "disagrees with the sync at the declared path",
        )
        .await;
    }

    #[tokio::test]
    async fn rejects_a_refresh_that_answers_a_live_item_gone() {
        rejects(
            Behavior::RefreshesALiveItemAsGone,
            "but the sync emitted it live",
        )
        .await;
    }

    /// Every write a role's vocabulary names is a `WriteOp` the SPI knows, so
    /// a typo in a minimum is a red test and not a write no source can offer.
    #[test]
    fn every_role_write_is_a_known_write_op() {
        let known = super::known_write_ops("test");
        for min in crate::role::MINIMUMS {
            for write in min.writes {
                assert!(
                    known.iter().any(|(id, _)| id == write),
                    "role {:?} names the write {write:?}, which is no WriteOp identifier",
                    min.role
                );
            }
        }
    }
}
