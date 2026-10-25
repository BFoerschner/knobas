//! [`WasmSource`]: the `Source` trait, forwarded across the WIT boundary.

use knobas_source::{
    Capability, ConnectionInfo, Cursor, RefreshAnswer, Sink, Source, SourceDescriptor, SourceError,
    SyncItem, WriteOp, WriteReceipt, pipeline, run, view,
};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use wasmtime::Store;
use wasmtime::component::{
    ComponentExportIndex, ComponentNamedList, Lift, Lower, ResourceAny, TypedFunc,
};

use crate::bindings::knobas::adapter::{download_sink, log, sink};
use crate::{HostState, WasmAdapter, bindings, convert, limits, net, new_store, wit};

/// How many walk readings the sync's channel holds beside its one item.
const READINGS_IN_FLIGHT: usize = 16;

/// What the component hands the engine's sink during a sync, in order.
enum Emit {
    /// One item, with the channel the sink's answer goes back on.
    Item(SyncItem, oneshot::Sender<Result<(), SourceError>>),
    /// A walk reading (1.3.0, #1380), which has no answer.
    Walked {
        done: u64,
        of: Option<u64>,
        unit: String,
    },
}

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

/// One piece of a download on its way from the component to the engine's
/// download sink, with the channel the sink's answer goes back on.
enum Piece {
    Start(String),
    Bytes(Vec<u8>),
}

type Chunk = (Piece, oneshot::Sender<Result<(), SourceError>>);

/// The download sink a call's store holds: a channel to the engine's sink
/// while a download runs, and none for every other call.
pub(crate) struct DownloadSlot {
    chunks: Option<mpsc::Sender<Chunk>>,
    started: bool,
    /// The first piece the sink refused, or the first out of order. Once
    /// set the download has failed, whatever the component answers.
    failure: Option<SourceError>,
}

impl DownloadSlot {
    pub(crate) fn none() -> Self {
        Self {
            chunks: None,
            started: false,
            failure: None,
        }
    }

    /// The slot of a download whose pieces go to `chunks`.
    fn to(chunks: mpsc::Sender<Chunk>) -> Self {
        Self {
            chunks: Some(chunks),
            ..Self::none()
        }
    }

    fn fail(&mut self, error: SourceError) -> wit::SourceError {
        let answer = convert::to_wit_error(&error);
        self.failure.get_or_insert(error);
        answer
    }

    /// Hand `piece` to the engine's sink and wait for its answer.
    async fn send(&mut self, piece: Piece) -> Result<(), wit::SourceError> {
        if let Some(failure) = &self.failure {
            return Err(convert::to_wit_error(failure));
        }
        let Some(chunks) = self.chunks.clone() else {
            return Err(convert::to_wit_error(&SourceError::protocol(
                "download-sink was called outside a download",
            )));
        };
        let out_of_order = match (&piece, self.started) {
            (Piece::Start(_), true) => Some("download-sink.start was called twice"),
            (Piece::Bytes(_), false) => Some("download-sink.write was called before start"),
            _ => None,
        };
        if let Some(why) = out_of_order {
            return Err(self.fail(SourceError::protocol(why)));
        }
        self.started = true;
        let (answer, answered) = oneshot::channel();
        let gone = || SourceError::protocol("the download's sink is gone");
        if chunks.send((piece, answer)).await.is_err() {
            return Err(self.fail(gone()));
        }
        match answered.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(self.fail(error)),
            Err(_) => Err(self.fail(gone())),
        }
    }
}

impl download_sink::Host for HostState {
    async fn start(&mut self, file_name: String) -> Result<(), wit::SourceError> {
        self.download.send(Piece::Start(file_name)).await
    }

    async fn write(&mut self, bytes: Vec<u8>) -> Result<(), wit::SourceError> {
        self.download.send(Piece::Bytes(bytes)).await
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
        if items.send(Emit::Item(item, answer)).await.is_err() {
            return Err(self.sink.fail(gone()));
        }
        match answered.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(self.sink.fail(error)),
            Err(_) => Err(self.sink.fail(gone())),
        }
    }

    /// Handed on in order with the items. Ignored outside a sync and once
    /// the sync has failed: a reading cannot fail a sync, and one after a
    /// refusal describes a run that is over.
    async fn walked(&mut self, done: u64, of: Option<u64>, unit: String) {
        if self.sink.failure.is_some() {
            return;
        }
        if let Some(items) = &self.sink.items {
            let _ = items.send(Emit::Walked { done, of, unit }).await;
        }
    }
}

/// Each line cut and the call's lines paced ([`limits::LogBudget`]): the
/// deadline bounds how long a component runs, not how much it writes to
/// the reader's log file (#1641).
impl log::Host for HostState {
    async fn log(&mut self, level: log::Level, message: String) {
        if !self.log.admit(std::time::Instant::now()) {
            return;
        }
        self.report_dropped_log_lines();
        let message = limits::log_line(&message);
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

/// A trap, or a failure to instantiate: the component failed the call, or
/// passed its memory cap.
fn failed(adapter: &WasmAdapter, error: &wasmtime::Error) -> SourceError {
    if let Some(limit) = limits::exceeded(error) {
        return limit.into_error(adapter.kind());
    }
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
) -> Result<(Store<HostState>, bindings::Hosted, ResourceAny), SourceError> {
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
/// and hand back what it answered as a `Result<_, SourceError>`, all of it
/// within the adapter's deadline.
macro_rules! call {
    ($self:expr, $method:ident $(, $arg:expr)*) => {
        limits::bounded(&$self.adapter, async {
            let (mut store, bindings, resource) = open(
                &$self.adapter,
                &$self.instance,
                SinkSlot::none(),
                Arc::clone(&$self.policy),
            )
            .await?;
            bindings
                .knobas_adapter_source()
                .adapter()
                .$method(&mut store, resource $(, $arg)*)
                .await
                .map_err(|e| failed(&$self.adapter, &e))?
                .map_err(convert::error)
        })
        .await
    };
}

/// `downloads.download`'s parameters and results, as the host calls it.
type DownloadParams<'a> = (&'a wit::Instance, &'a str, &'a str);
type DownloadResults = (Result<bool, wit::SourceError>,);

/// The optional function `name` at `index` (looked up at load by
/// `optional_export`) of a fresh instance of the component in `store`, typed
/// as the host calls it: `downloads.download` (1.1.0) and `refresh.refresh`
/// (1.4.0).
async fn optional_func<P, R>(
    adapter: &WasmAdapter,
    store: &mut Store<HostState>,
    index: Option<&ComponentExportIndex>,
    name: &str,
) -> Result<TypedFunc<P, R>, SourceError>
where
    P: ComponentNamedList + Lower,
    R: ComponentNamedList + Lift,
{
    let index =
        index.ok_or_else(|| SourceError::protocol(format!("the component exports no {name}")))?;
    let instance = adapter
        .inner
        .pre
        .instance_pre()
        .instantiate_async(&mut *store)
        .await
        .map_err(|e| failed(adapter, &e))?;
    instance
        .get_typed_func(&mut *store, index)
        .map_err(|e| failed(adapter, &e))
}

/// `downloads.download` of a fresh instance of the component in `store`.
async fn download_func<'a>(
    adapter: &WasmAdapter,
    store: &mut Store<HostState>,
) -> Result<TypedFunc<DownloadParams<'a>, DownloadResults>, SourceError> {
    optional_func(adapter, store, adapter.inner.download.as_ref(), "download").await
}

/// `refresh.refresh-answer` (1.4.0, #2101), as the host lifts it: the
/// `refresh` interface is not in the statically bound world `hosted`, so its
/// one type is written out here, case for case.
#[derive(wasmtime::component::ComponentType, wasmtime::component::Lift, Debug)]
#[component(variant)]
enum WitRefreshAnswer {
    #[component(name = "item")]
    Item(wit::SyncItem),
    #[component(name = "gone")]
    Gone,
    #[component(name = "not-offered")]
    NotOffered,
}

/// `refresh.refresh`'s parameters and results, as the host calls it.
type RefreshParams<'a> = (&'a wit::Instance, &'a str);
type RefreshResults = (Result<WitRefreshAnswer, wit::SourceError>,);

/// `refresh.refresh` of a fresh instance of the component in `store`.
async fn refresh_func<'a>(
    adapter: &WasmAdapter,
    store: &mut Store<HostState>,
) -> Result<TypedFunc<RefreshParams<'a>, RefreshResults>, SourceError> {
    optional_func(adapter, store, adapter.inner.refresh.as_ref(), "refresh").await
}

/// A store for a check at load, before any source of the adapter exists.
fn probe_store(adapter: &WasmAdapter) -> Store<HostState> {
    new_store(
        adapter,
        crate::PROBE_ID,
        SinkSlot::none(),
        Arc::new(net::Policy::none(crate::PROBE_ID)),
    )
}

/// At load: the `refresh` a component exports has the type 1.4.0 gives it.
pub(crate) async fn check_refresh(adapter: &WasmAdapter) -> Result<(), String> {
    let mut store = probe_store(adapter);
    limits::bounded(adapter, async {
        refresh_func(adapter, &mut store).await.map(drop)
    })
    .await
    .map_err(|e| format!("its refresh does not have the type knobas hosts: {e}"))
}

/// At load: the `download` a component exports has the type 1.1.0 gives it.
pub(crate) async fn check_download(adapter: &WasmAdapter) -> Result<(), String> {
    let mut store = probe_store(adapter);
    limits::bounded(adapter, async {
        download_func(adapter, &mut store).await.map(drop)
    })
    .await
    .map_err(|e| format!("its download does not have the type knobas hosts: {e}"))
}

/// The component's descriptor for `instance`, decoded. A component that
/// exports no `refresh` (any built against 1.3.0 or earlier) declares no
/// [`Capability::Refresh`] whatever its JSON says: the app reads the
/// declaration to choose between *Refresh* and *Sync now*, and between a
/// refresh and the run alone after a landed write, and the host would answer
/// *not offered* to every refresh of it (#2101).
pub(crate) async fn descriptor(
    adapter: &WasmAdapter,
    instance: wit::Instance,
    policy: Arc<net::Policy>,
) -> Result<SourceDescriptor, SourceError> {
    limits::bounded(adapter, async {
        let (mut store, bindings, resource) =
            open(adapter, &instance, SinkSlot::none(), policy).await?;
        let text = bindings
            .knobas_adapter_source()
            .adapter()
            .call_descriptor(&mut store, resource)
            .await
            .map_err(|e| failed(adapter, &e))?;
        let mut descriptor: SourceDescriptor = convert::json(&text, "descriptor")?;
        if !adapter.refreshes() {
            descriptor
                .capabilities
                .retain(|c| *c != Capability::Refresh);
        }
        Ok(descriptor)
    })
    .await
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
        // The component waits on each item until the sink has answered it, so
        // a refused item stops the sync where it stood; one item is in flight
        // at a time. The channel holds walk readings beside it (#1380), so a
        // reading does not make the next item wait for the pump: every wait
        // hands the thread to whatever else runs on it, a neighbour's
        // component included, and with a channel of one a 1.3.0 sync took
        // twice the waits a 1.2.0 one did.
        let (items, mut received) = mpsc::channel::<Emit>(READINGS_IN_FLIGHT + 1);
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
            while let Some(emit) = received.recv().await {
                match emit {
                    Emit::Item(item, answer) => {
                        let _ = answer.send(sink.item(item).await);
                    }
                    Emit::Walked { done, of, unit } => sink.walked(done, of, &unit).await,
                }
            }
        };
        // The deadline holds the whole sync, the sink's time included: a
        // pump that outlived it would hand the engine items of a call that
        // was stopped.
        limits::bounded(&self.adapter, async {
            let (result, ()) = tokio::join!(run, pump);
            result
        })
        .await
    }

    /// Battery clause 5, held by the host (#1950): an installed guest is not
    /// run against the battery, so the host refuses an op its descriptor does
    /// not declare, by name, and the call never crosses into the guest.
    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        self.descriptor.declares(&op)?;
        let op = convert::to_json(&op);
        call!(self, call_write, &op).map(convert::receipt)
    }

    /// `refresh.refresh` where the component exports it (1.4.0, #2101), and
    /// the trait's default, *not offered*, where it does not: a component
    /// built against 1.3.0 or earlier is never called, so its source falls
    /// back to a run. Within the adapter's deadline, as every call is
    /// (#1204). An item crosses as `sink.item`'s does, and one that does not
    /// decode fails the read.
    async fn refresh(&self, entity: &str) -> Result<RefreshAnswer, SourceError> {
        if !self.adapter.refreshes() {
            return Ok(RefreshAnswer::NotOffered);
        }
        let answer = limits::bounded(&self.adapter, async {
            let mut store = new_store(
                &self.adapter,
                &self.instance.id,
                SinkSlot::none(),
                Arc::clone(&self.policy),
            );
            let func = refresh_func(&self.adapter, &mut store).await?;
            let (answer,) = func
                .call_async(&mut store, (&self.instance, entity))
                .await
                .map_err(|e| failed(&self.adapter, &e))?;
            answer.map_err(convert::error)
        })
        .await?;
        Ok(match answer {
            WitRefreshAnswer::Item(item) => RefreshAnswer::Item(convert::sync_item(item)?),
            WitRefreshAnswer::Gone => RefreshAnswer::Gone,
            WitRefreshAnswer::NotOffered => RefreshAnswer::NotOffered,
        })
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

    /// `query.translate-query` where the component exports it (#1208), and
    /// the trait's default, `None`, where it does not: a component built
    /// against 1.0.0 or 1.1.0 translates no query. Within the adapter's
    /// deadline, as every call is (#1204).
    async fn translate_query(
        &self,
        view: &serde_json::Value,
    ) -> Result<Option<knobas_source::query::Translation>, SourceError> {
        let Some(index) = self.adapter.inner.translate_query else {
            return Ok(None);
        };
        let answer = limits::bounded(&self.adapter, async {
            let mut store = new_store(
                &self.adapter,
                &self.instance.id,
                SinkSlot::none(),
                Arc::clone(&self.policy),
            );
            let instance = self
                .adapter
                .inner
                .pre
                .instance_pre()
                .instantiate_async(&mut store)
                .await
                .map_err(|e| failed(&self.adapter, &e))?;
            type Answer = (Result<Option<String>, wit::SourceError>,);
            let func = instance
                .get_typed_func::<(wit::Instance, String), Answer>(&mut store, index)
                .map_err(|e| failed(&self.adapter, &e))?;
            let (answer,) = func
                .call_async(&mut store, (self.instance.clone(), convert::to_json(view)))
                .await
                .map_err(|e| failed(&self.adapter, &e))?;
            answer.map_err(convert::error)
        })
        .await?;
        Self::decode(answer, "query translation")
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

    /// `downloads.download`, streaming: the component hands each chunk to
    /// `download-sink.write`, and each waits until `sink` has written it, so
    /// neither side holds the download whole. A 1.0.0 component, which
    /// exports no `download`, keeps no runs to download.
    async fn download(
        &self,
        entity: &str,
        target: &run::DownloadTarget,
        sink: &mut dyn run::DownloadSink,
    ) -> Result<bool, SourceError> {
        if !self.adapter.downloads() {
            return Ok(false);
        }
        let target = convert::to_json(target);
        let (chunks, mut received) = mpsc::channel::<Chunk>(1);
        let run = async {
            let mut store = new_store(
                &self.adapter,
                &self.instance.id,
                SinkSlot::none(),
                Arc::clone(&self.policy),
            );
            store.data_mut().download = DownloadSlot::to(chunks);
            let func = download_func(&self.adapter, &mut store).await?;
            let (answer,) = func
                .call_async(&mut store, (&self.instance, entity, &target))
                .await
                .map_err(|e| failed(&self.adapter, &e))?;
            let failure = store.data_mut().download.failure.take();
            // Dropping the store closes the channel, which ends the pump.
            drop(store);
            match failure {
                Some(failure) => Err(failure),
                None => answer.map_err(convert::error),
            }
        };
        let pump = async {
            while let Some((piece, answer)) = received.recv().await {
                let written = match piece {
                    Piece::Start(name) => sink.start(&name).await,
                    Piece::Bytes(bytes) => sink.write(&bytes).await,
                };
                let _ = answer.send(written);
            }
        };
        limits::bounded(&self.adapter, async {
            let (result, ()) = tokio::join!(run, pump);
            result
        })
        .await
    }
}
