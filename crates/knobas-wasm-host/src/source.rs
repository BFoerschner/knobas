//! [`WasmSource`]: the `Source` trait, forwarded across the WIT boundary.

use knobas_source::{
    ConnectionInfo, Cursor, Sink, Source, SourceDescriptor, SourceError, SyncItem, WriteOp,
    WriteReceipt, pipeline, run, view,
};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use wasmtime::Store;

use wasmtime::component::ResourceAny;

use crate::bindings::knobas::adapter::{log, sink};
use crate::{HostState, WasmAdapter, bindings, convert, net, new_store, wit};

/// One item on its way from the component to the engine's sink, with the
/// channel the sink's answer goes back on.
type Emit = (SyncItem, oneshot::Sender<Result<(), SourceError>>);

/// The sink a call's store holds: a channel to the engine's sink while a
/// sync runs, and none for every other call.
pub(crate) struct SinkSlot {
    items: Option<mpsc::Sender<Emit>>,
    /// The first item the sink refused, or the first item that did not
    /// decode. Once set the sync has failed, whatever the component answers:
    /// a component that swallowed it would otherwise hand back a cursor past
    /// items the engine never stored.
    failure: Option<SourceError>,
}

impl SinkSlot {
    fn none() -> Self {
        Self {
            items: None,
            failure: None,
        }
    }

    fn fail(&mut self, error: SourceError) -> wit::SourceError {
        let answer = convert::to_wit_error(&error);
        self.failure.get_or_insert(error);
        answer
    }
}

impl wit::Host for HostState {}

impl sink::Host for HostState {
    async fn item(&mut self, item: wit::SyncItem) -> Result<(), wit::SourceError> {
        if let Some(failure) = &self.sink.failure {
            return Err(convert::to_wit_error(failure));
        }
        let Some(items) = self.sink.items.clone() else {
            return Err(convert::to_wit_error(&SourceError::protocol(
                "sink.item was called outside a sync",
            )));
        };
        let item = match convert::sync_item(item) {
            Ok(item) => item,
            Err(error) => return Err(self.sink.fail(error)),
        };
        let (answer, answered) = oneshot::channel();
        let gone = || SourceError::Sink("the sync's sink is gone".into());
        if items.send((item, answer)).await.is_err() {
            return Err(self.sink.fail(gone()));
        }
        match answered.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(self.sink.fail(error)),
            Err(_) => Err(self.sink.fail(gone())),
        }
    }
}

impl log::Host for HostState {
    async fn log(&mut self, level: log::Level, message: String) {
        let source = self.source_id.as_str();
        match level {
            log::Level::Trace => tracing::trace!(source, "{message}"),
            log::Level::Debug => tracing::debug!(source, "{message}"),
            log::Level::Info => tracing::info!(source, "{message}"),
            log::Level::Warn => tracing::warn!(source, "{message}"),
            log::Level::Error => tracing::error!(source, "{message}"),
        }
    }
}

/// A trap, or a failure to instantiate: the component failed the call.
fn failed(adapter: &WasmAdapter, error: &wasmtime::Error) -> SourceError {
    SourceError::protocol(format!(
        "the external adapter {:?} failed: {error:#}",
        adapter.kind()
    ))
}

/// A fresh store, the component instantiated in it, and its `adapter`
/// constructed from `instance`.
async fn open(
    adapter: &WasmAdapter,
    instance: &wit::Instance,
    sink: SinkSlot,
    policy: Arc<net::Policy>,
) -> Result<(Store<HostState>, bindings::Adapter, ResourceAny), SourceError> {
    let mut store = new_store(adapter, &instance.id, sink, policy);
    let bindings = adapter
        .inner
        .pre
        .instantiate_async(&mut store)
        .await
        .map_err(|e| failed(adapter, &e))?;
    let resource = bindings
        .knobas_adapter_source()
        .adapter()
        .call_constructor(&mut store, instance)
        .await
        .map_err(|e| failed(adapter, &e))?;
    Ok((store, bindings, resource))
}

/// One call that needs no sink: open a store, call `$method` with `$args`,
/// and hand back what it answered as a `Result<_, SourceError>`.
macro_rules! call {
    ($self:expr, $method:ident $(, $arg:expr)*) => {{
        let (mut store, bindings, resource) =
            open(&$self.adapter, &$self.instance, SinkSlot::none(), Arc::clone(&$self.policy)).await?;
        bindings
            .knobas_adapter_source()
            .adapter()
            .$method(&mut store, resource $(, $arg)*)
            .await
            .map_err(|e| failed(&$self.adapter, &e))?
            .map_err(convert::error)
    }};
}

/// The component's descriptor for `instance`, decoded.
pub(crate) async fn descriptor(
    adapter: &WasmAdapter,
    instance: wit::Instance,
    policy: Arc<net::Policy>,
) -> Result<SourceDescriptor, SourceError> {
    let (mut store, bindings, resource) =
        open(adapter, &instance, SinkSlot::none(), policy).await?;
    let text = bindings
        .knobas_adapter_source()
        .adapter()
        .call_descriptor(&mut store, resource)
        .await
        .map_err(|e| failed(adapter, &e))?;
    convert::json(&text, "descriptor")
}

/// A configured source of an external adapter.
pub struct WasmSource {
    adapter: WasmAdapter,
    instance: wit::Instance,
    descriptor: SourceDescriptor,
    /// What its calls may reach, and the credential the host attaches.
    policy: Arc<net::Policy>,
}

impl WasmSource {
    pub(crate) fn new(
        adapter: WasmAdapter,
        instance: wit::Instance,
        descriptor: SourceDescriptor,
        policy: Arc<net::Policy>,
    ) -> Self {
        Self {
            adapter,
            instance,
            descriptor,
            policy,
        }
    }

    /// The JSON answer of a read, decoded as the SPI type `what` names.
    fn decode<T: serde::de::DeserializeOwned>(
        answer: Option<String>,
        what: &str,
    ) -> Result<Option<T>, SourceError> {
        answer.map(|text| convert::json(&text, what)).transpose()
    }
}

#[async_trait::async_trait]
impl Source for WasmSource {
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        convert::connection_info(call!(self, call_test_connection)?)
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        // A channel of one: the component waits on each item until the sink
        // has answered it, so a refused item stops the sync where it stood.
        let (items, mut received) = mpsc::channel::<Emit>(1);
        let run = async {
            let slot = SinkSlot {
                items: Some(items),
                failure: None,
            };
            let (mut store, bindings, resource) = open(
                &self.adapter,
                &self.instance,
                slot,
                Arc::clone(&self.policy),
            )
            .await?;
            let answer = bindings
                .knobas_adapter_source()
                .adapter()
                .call_sync(&mut store, resource, cursor.as_ref())
                .await
                .map_err(|e| failed(&self.adapter, &e))?;
            let failure = store.data_mut().sink.failure.take();
            // Dropping the store closes the channel, which ends the pump.
            drop(store);
            match failure {
                Some(failure) => Err(failure),
                None => answer.map_err(convert::error),
            }
        };
        let pump = async {
            while let Some((item, answer)) = received.recv().await {
                let _ = answer.send(sink.item(item).await);
            }
        };
        let (result, ()) = tokio::join!(run, pump);
        result
    }

    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        let op = convert::to_json(&op);
        call!(self, call_write, &op).map(convert::receipt)
    }

    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        call!(self, call_reachable_transitions, entity)
    }

    async fn view(
        &self,
        entity: &str,
        section: Option<&str>,
    ) -> Result<Option<view::SourceView>, SourceError> {
        Self::decode(call!(self, call_view, entity, section)?, "view")
    }

    async fn pipeline_settings(
        &self,
        entity: &str,
    ) -> Result<Option<pipeline::PipelineSettings>, SourceError> {
        Self::decode(
            call!(self, call_pipeline_settings, entity)?,
            "pipeline settings",
        )
    }

    async fn older_runs(
        &self,
        entity: &str,
        before: &str,
        count: u32,
    ) -> Result<Option<Vec<SyncItem>>, SourceError> {
        call!(self, call_older_runs, entity, before, count)?
            .map(|runs| runs.into_iter().map(convert::sync_item).collect())
            .transpose()
    }

    async fn run_chain(
        &self,
        run: &str,
    ) -> Result<Option<Vec<pipeline::RunChainStep>>, SourceError> {
        call!(self, call_run_chain, run)?
            .map(|steps| steps.into_iter().map(convert::run_chain_step).collect())
            .transpose()
    }

    async fn run_log(
        &self,
        entity: &str,
        from: Option<u64>,
        count: u32,
    ) -> Result<Option<run::RunLog>, SourceError> {
        Self::decode(call!(self, call_run_log, entity, from, count)?, "run log")
    }

    async fn run_tests(&self, entity: &str) -> Result<Option<run::RunTests>, SourceError> {
        Self::decode(call!(self, call_run_tests, entity)?, "run tests")
    }

    async fn test_history(
        &self,
        entity: &str,
        test: &str,
    ) -> Result<Option<Vec<run::TestOutcome>>, SourceError> {
        Self::decode(
            call!(self, call_test_history, entity, test)?,
            "test history",
        )
    }

    async fn run_artifacts(
        &self,
        entity: &str,
        path: &str,
    ) -> Result<Option<Vec<run::ArtifactEntry>>, SourceError> {
        Self::decode(
            call!(self, call_run_artifacts, entity, path)?,
            "run artifacts",
        )
    }

    async fn artifact_preview(
        &self,
        entity: &str,
        path: &str,
    ) -> Result<Option<run::ArtifactPreview>, SourceError> {
        Self::decode(
            call!(self, call_artifact_preview, entity, path)?,
            "artifact preview",
        )
    }
}
