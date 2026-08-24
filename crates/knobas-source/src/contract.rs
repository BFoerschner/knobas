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

/// The stable identifier adapters declare in
/// [`SourceDescriptor::write_ops`](crate::SourceDescriptor::write_ops) for `op`.
///
/// **No wildcard arm, deliberately.** `WriteOp` is documented to grow per
/// milestone, and a stale table here does not fail quietly -- it falsely
/// rejects the first adapter to declare the new identifier, with a message
/// pointing at that adapter's descriptor instead of at this file. So the
/// reminder is the compiler: adding a variant stops this module compiling until
/// the variant is given an identifier here and a probe value in
/// [`known_write_ops`] below.
fn write_op_identifier(op: &crate::WriteOp) -> &'static str {
    match op {
        crate::WriteOp::Comment { .. } => "comment",
    }
}

/// One probe value per [`WriteOp`](crate::WriteOp) variant, each paired with its
/// identifier from [`write_op_identifier`] -- so the two can never disagree.
///
/// This is what lets the battery call `write` with an op the adapter did not
/// declare, and what makes a typo'd identifier in a descriptor a test failure
/// rather than an action the UI silently never renders.
fn known_write_ops(src_id: &str) -> Vec<(&'static str, crate::WriteOp)> {
    [crate::WriteOp::Comment {
        entity: format!("{src_id}:contract-battery"),
        body: "contract battery probe".into(),
    }]
    .into_iter()
    .map(|op| (write_op_identifier(&op), op))
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
    // 3. Auth failure maps to Unauthorized, connectivity failure to Unreachable.
    let unauthorized = make(Fault::Unauthorized).test_connection().await;
    assert!(
        matches!(unauthorized, Err(crate::SourceError::Unauthorized)),
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
        matches!(unauthorized, Err(crate::SourceError::Unauthorized)),
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
            matches!(refused, Err(crate::SourceError::Protocol(_))),
            "write of {id:?}, which descriptor.write_ops does not declare, must be refused with \
             SourceError::Protocol, got {refused:?}"
        );
    }
    // 6. A sink failure aborts the sync instead of being swallowed.
    let aborted = s.sync(None, &mut FailingSink).await;
    assert!(
        matches!(aborted, Err(crate::SourceError::Sink(_))),
        "a sink error must be propagated out of sync as SourceError::Sink, got {aborted:?}"
    );
}

#[cfg(test)]
mod tests {
    use super::{Fault, battery};
    use crate::{
        AuthMethod, Capability, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
        SyncItem, WriteOp,
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
                Fault::Unauthorized if misclassify_auth => {
                    Some(SourceError::Protocol("401".into()))
                }
                Fault::Unauthorized => Some(SourceError::Unauthorized),
                Fault::Unreachable if misclassify_reach => {
                    Some(SourceError::Protocol("no route to host".into()))
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
                entity_kinds: vec![KindInfo {
                    id: "ticket".into(),
                    label: "Ticket".into(),
                    plural: "Tickets".into(),
                    monogram: "TE".into(),
                }],
                config_schema: serde_json::json!({ "type": "object", "properties": {} }),
            }
        }

        async fn test_connection(&self) -> Result<(), SourceError> {
            match self.faulted(
                self.behavior == Behavior::MisclassifiesAuthOnConnect,
                self.behavior == Behavior::MisclassifiesReachOnConnect,
            ) {
                Some(err) => Err(err),
                None => Ok(()),
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
                    payload: serde_json::json!({}),
                    deleted: false,
                })
                .await;
            if self.behavior != Behavior::SwallowsSinkError {
                pushed?;
            }
            Ok("1".into())
        }

        async fn write(&self, op: WriteOp) -> Result<(), SourceError> {
            if self.behavior == Behavior::DeclaresAWriteOp {
                // A real adapter would post a comment to a live system here.
                unreachable!("battery must not perform a write the descriptor declares");
            }
            if self.behavior == Behavior::AcceptsUndeclaredWrite {
                return Ok(());
            }
            Err(SourceError::Protocol(format!("unsupported op: {op:?}")))
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

    // -- clauses 3 and 4: fault classification -------------------------------

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

    // -- clause 6: sink failures ---------------------------------------------

    #[tokio::test]
    async fn rejects_an_adapter_that_swallows_a_sink_error() {
        rejects(
            Behavior::SwallowsSinkError,
            "must be propagated out of sync",
        )
        .await;
    }
}
