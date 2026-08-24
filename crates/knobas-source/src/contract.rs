//! The contract battery: the shared test suite every adapter must pass.

/// Which failure an adapter under test should simulate.
///
/// The battery builds a fresh adapter per case, so a factory can honour this by
/// storing it and branching in [`Source::test_connection`].
///
/// [`Source::test_connection`]: crate::Source::test_connection
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
    async fn item(&mut self, item: crate::SyncItem) {
        self.0.push(item);
    }
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
    let src_id = s.descriptor().id.clone();
    let mut sink = VecSink(Vec::new());
    let cursor = s
        .sync(None, &mut sink)
        .await
        .expect("full sync must succeed");
    assert!(!sink.0.is_empty(), "full sync yielded no items");
    let declared: std::collections::HashSet<String> =
        s.descriptor().kinds.iter().map(|k| k.id.clone()).collect();
    for it in &sink.0 {
        assert_eq!(
            it.entity.namespace, src_id,
            "item {} not namespaced to source",
            it.entity
        );
        assert!(
            declared.contains(&it.kind),
            "item {} has kind {:?} not declared in descriptor.kinds",
            it.entity,
            it.kind
        );
    }
    // 2. Incremental sync from the returned cursor yields no items when nothing changed.
    let mut sink2 = VecSink(Vec::new());
    s.sync(Some(cursor), &mut sink2)
        .await
        .expect("incremental sync must succeed");
    assert!(
        sink2.0.is_empty(),
        "incremental sync after no changes must be empty"
    );
    // 3. Auth failure maps to Unauthorized, connectivity failure to Unreachable.
    let unauthorized = make(Fault::Unauthorized).test_connection().await;
    assert!(
        matches!(unauthorized, Err(crate::SourceError::Unauthorized)),
        "an auth failure must surface as SourceError::Unauthorized, got {unauthorized:?}"
    );
    let unreachable = make(Fault::Unreachable).test_connection().await;
    assert!(
        matches!(unreachable, Err(crate::SourceError::Unreachable(_))),
        "a connectivity failure must surface as SourceError::Unreachable, got {unreachable:?}"
    );
}

#[cfg(test)]
mod tests {
    use super::{Fault, battery};
    use crate::{
        Capability, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError, SyncItem,
        WriteOp,
    };
    use knobas_core::entity::EntityRef;

    /// How the adapter under test misbehaves -- one variant per battery clause,
    /// so a clause that stops being enforced turns a test red.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Behavior {
        /// Honours the whole contract.
        Good,
        /// Emits nothing at all: the do-nothing adapter.
        Null,
        /// Emits an item of a kind absent from `descriptor.kinds`.
        UndeclaredKind,
        /// Emits an item namespaced to something other than the source id.
        ForeignNamespace,
        /// Ignores the cursor and re-emits everything on an incremental sync.
        IgnoresCursor,
        /// Reports every connection failure as `Protocol`.
        MisclassifiedFaults,
    }

    struct TestSource {
        fault: Fault,
        behavior: Behavior,
    }

    #[async_trait::async_trait]
    impl Source for TestSource {
        fn descriptor(&self) -> SourceDescriptor {
            SourceDescriptor {
                id: "test".into(),
                kind: "test".into(),
                name: "Test".into(),
                capabilities: vec![Capability::Search],
                adapter_version: "0.1.0".into(),
                kinds: vec![KindInfo {
                    id: "ticket".into(),
                    label: "Ticket".into(),
                    plural: "Tickets".into(),
                    monogram: "TE".into(),
                }],
                config_schema: serde_json::json!({ "type": "object", "properties": {} }),
            }
        }

        async fn test_connection(&self) -> Result<(), SourceError> {
            if self.behavior == Behavior::MisclassifiedFaults && self.fault != Fault::None {
                return Err(SourceError::Protocol("something went wrong".into()));
            }
            match self.fault {
                Fault::None => Ok(()),
                Fault::Unauthorized => Err(SourceError::Unauthorized),
                Fault::Unreachable => Err(SourceError::Unreachable("connection refused".into())),
            }
        }

        async fn sync(
            &self,
            cursor: Option<Cursor>,
            sink: &mut (dyn Sink + Send),
        ) -> Result<Cursor, SourceError> {
            let quiet = self.behavior == Behavior::Null
                || (cursor.is_some() && self.behavior != Behavior::IgnoresCursor);
            if quiet {
                return Ok("1".into());
            }
            let namespace = match self.behavior {
                Behavior::ForeignNamespace => "elsewhere",
                _ => "test",
            };
            let kind = match self.behavior {
                Behavior::UndeclaredKind => "gadget",
                _ => "ticket",
            };
            sink.item(SyncItem {
                entity: EntityRef::new(namespace, "1"),
                kind: kind.into(),
                title: "One".into(),
                body_text: "the first item".into(),
                author: None,
                updated_at: None,
                payload: serde_json::json!({}),
                deleted: false,
            })
            .await;
            Ok("1".into())
        }

        async fn write(&self, op: WriteOp) -> Result<(), SourceError> {
            Err(SourceError::Protocol(format!("unsupported op: {op:?}")))
        }
    }

    /// Run the battery against `behavior` on its own task, so a battery panic
    /// surfaces as a `JoinError` instead of taking the test process down.
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

    /// Without this the negative cases below prove nothing: a battery that
    /// panics unconditionally would pass all of them.
    #[tokio::test]
    async fn accepts_a_conforming_adapter() {
        run(Behavior::Good).await.expect("conforming adapter");
    }

    #[tokio::test]
    async fn rejects_a_do_nothing_adapter() {
        let msg = run(Behavior::Null).await.unwrap_err();
        assert!(msg.contains("no items"), "unexpected panic: {msg}");
    }

    #[tokio::test]
    async fn rejects_an_undeclared_kind() {
        let msg = run(Behavior::UndeclaredKind).await.unwrap_err();
        assert!(msg.contains("not declared"), "unexpected panic: {msg}");
    }

    #[tokio::test]
    async fn rejects_a_foreign_namespace() {
        let msg = run(Behavior::ForeignNamespace).await.unwrap_err();
        assert!(msg.contains("not namespaced"), "unexpected panic: {msg}");
    }

    #[tokio::test]
    async fn rejects_an_adapter_that_ignores_the_cursor() {
        let msg = run(Behavior::IgnoresCursor).await.unwrap_err();
        assert!(msg.contains("incremental"), "unexpected panic: {msg}");
    }

    #[tokio::test]
    async fn rejects_misclassified_connection_faults() {
        run(Behavior::MisclassifiedFaults).await.unwrap_err();
    }
}
