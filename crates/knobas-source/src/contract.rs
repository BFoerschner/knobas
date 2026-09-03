//! The contract battery: the shared test suite every adapter must pass.

use knobas_core::entity::EntityRef;

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
            entity: target,
            // A fixed instant, not `Utc::now()`: an adapter that (wrongly)
            // attempted this probe would otherwise write a different worklog
            // on every run, and a battery whose payload moves is one whose
            // failures cannot be compared between runs.
            started: chrono::DateTime::from_timestamp(1_788_000_000, 0)
                .expect("a fixed instant"),
            seconds: 60,
            comment: "contract battery probe".into(),
        },
    ]
    .into_iter()
    .map(|op| (op.identifier(), op))
    .collect()
}

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
        !knobas_core::entity::is_reserved_namespace(&src_id),
        "descriptor.id {src_id:?} is a namespace knobas keeps for its own entities \
         ({:?}) -- the sync engine refuses such a source outright, so an adapter named this \
         certifies here and then fails every real run",
        knobas_core::entity::RESERVED_NAMESPACES
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
    // 7. A sink failure aborts the sync instead of being swallowed.
    let aborted = s.sync(None, &mut FailingSink).await;
    assert!(
        matches!(aborted, Err(crate::SourceError::Sink(_))),
        "a sink error must be propagated out of sync as SourceError::Sink, got {aborted:?}"
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
///    [`names_a_missing_key`](knobas_core::payload::names_a_missing_key).
/// 4. **A field a kind does not declare resolves to nothing.** Narrow and
///    worth having: it says nothing about the *declaration*, but it is what a
///    resolver that grew a knobas-side fallback for an undeclared field dies
///    on, and such a resolver would pass every other clause here while quietly
///    putting a value on screen that no source said.
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
    use knobas_core::payload::{
        KindPaths, PayloadPath, at, field, names_a_missing_key, resolve_flag, resolve_list,
        resolve_string,
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
    }

    impl Wanted {
        fn matches(self, value: &serde_json::Value) -> bool {
            match self {
                Wanted::Text => value.is_string(),
                Wanted::Flag => value.is_boolean(),
                Wanted::List => value.is_array(),
            }
        }

        fn word(self) -> &'static str {
            match self {
                Wanted::Text => "string",
                Wanted::Flag => "boolean",
                Wanted::List => "array",
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
        let scalars: [(&str, &Vec<PayloadPath>, Wanted); 6] = [
            (field::STATUS_NAME, &paths.status_name, Wanted::Text),
            (field::PRIORITY, &paths.priority, Wanted::Text),
            (field::ASSIGNEE, &paths.assignee, Wanted::Text),
            (field::PROJECT_KEY, &paths.project_key, Wanted::Text),
            (field::PROJECT_NAME, &paths.project_name, Wanted::Text),
            (field::MERGED, &paths.merged, Wanted::Flag),
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

        for (name, candidates, wanted) in scalars {
            let seen = evidence.entry((&item.kind, name)).or_default();
            for path in candidates {
                weigh(seen, name, &item.payload, path, wanted, "on");
            }
        }
        for candidate in &paths.reviewers {
            let seen = evidence.entry((&item.kind, field::REVIEWERS)).or_default();
            // The array itself is weighed against its own evidence slot, so a
            // list whose array is there and whose entry key is wrong is not
            // excused by the array having resolved.
            let mut array = Evidence::default();
            weigh(
                &mut array,
                field::REVIEWERS,
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
                    field::REVIEWERS,
                    element,
                    &candidate.entry,
                    Wanted::Text,
                    "in an element of",
                );
            }
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
        if paths.reviewers.is_empty() {
            assert!(
                resolve_list(&item.payload, &paths.reviewers).is_empty(),
                "kind {:?} declares no reviewers, so every reader must miss on item {}",
                item.kind,
                item.entity
            );
        }
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
    use knobas_core::entity::EntityRef;

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
        /// Swallows the sink's error and reports a successful sync.
        SwallowsSinkError,
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
                    | Behavior::UnknownWriteOpId
                    | Behavior::WriteCapabilityWithoutOps => {
                        vec![Capability::Search, Capability::Write]
                    }
                    _ => vec![Capability::Search],
                },
                adapter_version: "0.1.0".into(),
                auth_methods: vec![AuthMethod::Pat],
                // Declares no write ops, so the battery probes `comment` and
                // expects a refusal -- except where the behavior needs otherwise.
                write_ops: match self.behavior {
                    Behavior::UnknownWriteOpId => vec!["Comment".into()],
                    Behavior::DeclaresAWriteOp | Behavior::WriteOpsWithoutCapability => {
                        vec!["comment".into()]
                    }
                    _ => Vec::new(),
                },
                entity_kinds: {
                    let kind = |id: &str, label: &str, monogram: &str| KindInfo {
                        id: id.to_owned(),
                        label: label.to_owned(),
                        plural: format!("{label}s"),
                        monogram: monogram.to_owned(),
                        full_sync_exhaustive: true,
                    };
                    let mut kinds = vec![kind("ticket", "Ticket", "TE")];
                    // The second kind exists only for the tolerance case, and
                    // only in `entity_kinds`: clause 1 wants a declared kind
                    // to be one the adapter emits, and nothing anywhere wants
                    // it to be one the adapter's corpus happens to contain.
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
                    use crate::{KindPaths, ListPath, PayloadPath};
                    let ticket = |status: PayloadPath| KindPaths {
                        kind: "ticket".to_owned(),
                        status_name: vec![status],
                        assignee: vec![PayloadPath::of(["fields", "assignee", "name"])],
                        ..KindPaths::default()
                    };
                    let good = ticket(PayloadPath::of(["fields", "status", "name"]));
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
                    payload: serde_json::json!({
                        "fields": {
                            "status": { "name": "In Progress" },
                            "assignee": serde_json::Value::Null
                        },
                        "reviewers": [{ "login": "mara.lindqvist" }]
                    }),
                    web_url: None,
                    deleted: false,
                })
                .await;
            if self.behavior != Behavior::SwallowsSinkError {
                pushed?;
            }
            Ok("1".into())
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
            if self.behavior == Behavior::AcceptsUndeclaredWrite {
                return Ok(crate::WriteReceipt::none());
            }
            Err(SourceError::protocol(format!("unsupported op: {op:?}")))
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

    // -- clause 7: sink failures ---------------------------------------------

    #[tokio::test]
    async fn rejects_an_adapter_that_swallows_a_sink_error() {
        rejects(
            Behavior::SwallowsSinkError,
            "must be propagated out of sync",
        )
        .await;
    }
}
