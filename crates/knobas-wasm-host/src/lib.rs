//! The WASM host (ADR-0028, issue #1198): an external adapter, run under
//! wasmtime, is one more [`Source`].
//!
//! An external adapter is one WASM component built against the WIT world in
//! `wit/adapter.wit`, `knobas:adapter@1.4.0`. [`WasmAdapter::load`] compiles
//! it, refuses one built against a version this knobas cannot host, and
//! reads its descriptor. A 1.0.0 to 1.3.0 component is hosted too: what
//! the later minors added is an import it does not use (`download-sink` in
//! 1.1.0, `sink.walked` in 1.3.0) and exports (`downloads` in 1.1.0, `query`
//! in 1.2.0, `refresh` in 1.4.0) the host looks up as optional. [`WasmAdapter::build`] turns a
//! configured instance into a `Box<dyn Source>`, which forwards each trait
//! call across the interface. The sync engine, the write queue, IPC and the
//! shell see that and nothing else.
//!
//! **What the component is given.** Its own `instance` record, which holds
//! no credential; the `sink` for a sync's items; the `download-sink` for a
//! download's bytes (#1205); `log`; WASI 0.2 with
//! nothing in it: no directory, no environment, no argument, and the
//! context's defaults refuse every socket and name lookup; and `wasi:http`
//! 0.2's outgoing handler, held to the hosts its manifest declares, with the
//! credential attached by the host to requests for the source's own address
//! ([`net`], #1203). Every call has a deadline and a memory cap
//! ([`limits`], #1204): one that passes either is stopped, and fails naming
//! the limit.
//!
//! **One store per call.** Every call instantiates the component afresh,
//! constructs its `adapter` from the instance, calls one function and drops
//! the store. Nothing the component keeps outlives the call, so an adapter
//! carries state between calls only in its cursor, and a component that
//! trapped leaves nothing behind for the next call.

use std::path::Path;
use std::sync::Arc;

use knobas_source::instance::SourceInstance;
use knobas_source::{Source, SourceDescriptor, SourceError};
use wasmtime::component::{
    Component, ComponentExportIndex, Linker, ResourceTable, types::ComponentItem,
};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

pub mod allow;
mod convert;
pub mod limits;
pub mod manifest;
mod net;
mod redact;
mod source;

pub use source::WasmSource;

#[cfg(feature = "test-util")]
pub use net::{fake_dns, fake_dns_hangs, fake_dns_slow, fake_proxy};

mod bindings {
    wasmtime::component::bindgen!({
        // `hosted`, not `adapter`: the optional reads stay out of the static
        // bindings, so a component without them links (#1208).
        world: "hosted",
        path: "wit",
        imports: { default: async },
        exports: { default: async },
    });
}

use bindings::knobas::adapter::types as wit;

/// The interface package an external adapter is built against.
pub const PACKAGE: &str = "knobas:adapter";

/// The version of [`PACKAGE`] this knobas hosts: `wit/adapter.wit`'s. A
/// component built against the same major version and a minor version no
/// newer than this one is hosted; every other is refused at load.
pub const INTERFACE_VERSION: Version = Version {
    major: 1,
    minor: 4,
    patch: 0,
};

/// The interface the component exports, without its version.
const SOURCE_EXPORT: &str = "knobas:adapter/source";

/// The optional interface a 1.1.0 component exports `download` in.
const DOWNLOADS_EXPORT: &str = "knobas:adapter/downloads";

/// The optional interface a 1.2.0 component exports `translate-query` in
/// (#1208).
const QUERY_EXPORT: &str = "knobas:adapter/query";

/// The optional interface a 1.4.0 component exports `refresh` in (#2101).
const REFRESH_EXPORT: &str = "knobas:adapter/refresh";

/// The function `name` of the optional interface `interface` (without its
/// version), or `None` when the component does not export the interface:
/// how the host finds an optional read (#1205, #1208). The lookup names the
/// host's version, and wasmtime's semver-compatible lookup finds the
/// interface a component built against any 1.x exports. An interface without
/// the function is refused.
pub(crate) fn optional_export(
    component: &Component,
    interface: &str,
    name: &str,
) -> Result<Option<ComponentExportIndex>, LoadError> {
    let Some(instance) =
        component.get_export_index(None, format!("{interface}@{INTERFACE_VERSION}"))
    else {
        return Ok(None);
    };
    component
        .get_export_index(Some(&instance), name)
        .map(Some)
        .ok_or_else(|| LoadError::Link(format!("{interface} exports no {name}")))
}

/// A semver `major.minor.patch`, as a component's export names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.').map(|p| p.parse::<u64>().ok());
        let version = Self {
            major: parts.next()??,
            minor: parts.next()??,
            patch: parts.next()??,
        };
        parts.next().is_none().then_some(version)
    }

    /// Whether a component built against `self` can run on this host.
    #[must_use]
    pub fn is_hosted(self) -> bool {
        self.major == INTERFACE_VERSION.major && self.minor <= INTERFACE_VERSION.minor
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Why a component could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("cannot read {path}: {error}")]
    Read { path: String, error: std::io::Error },
    #[error("not a WASM component: {0}")]
    NotAComponent(String),
    #[error("the component exports no {SOURCE_EXPORT}, so it is not a knobas adapter")]
    NoSource,
    #[error(
        "the component is built against {PACKAGE}@{found}, and this knobas hosts \
         {PACKAGE}@{major}.x up to {INTERFACE_VERSION}",
        major = INTERFACE_VERSION.major
    )]
    Unsupported { found: Version },
    #[error("the component cannot be linked: {0}")]
    Link(String),
    #[error("the component's descriptor is refused: {0}")]
    Descriptor(String),
}

/// One loaded external adapter: a compiled component, ready to build
/// sources from. Cheap to clone.
#[derive(Clone)]
pub struct WasmAdapter {
    inner: Arc<Inner>,
    /// The hosts its manifest declares; none for a component loaded
    /// without one, which then reaches nothing.
    hosts: Arc<[allow::HostPattern]>,
    /// The deadline and memory cap of every call into it.
    limits: limits::Limits,
}

struct Inner {
    engine: Engine,
    pre: bindings::HostedPre<HostState>,
    /// `downloads.download`, when the component exports it (1.1.0).
    download: Option<ComponentExportIndex>,
    version: Version,
    template: SourceDescriptor,
    /// `query.translate-query`, when the component exports it (1.2.0, #1208).
    translate_query: Option<ComponentExportIndex>,
    /// `refresh.refresh`, when the component exports it (1.4.0, #2101).
    refresh: Option<ComponentExportIndex>,
}

impl std::fmt::Debug for WasmAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmAdapter")
            .field("kind", &self.inner.template.adapter_kind)
            .field("version", &self.inner.version)
            .field("hosts", &self.hosts)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

/// The instance id the template probe constructs the adapter with, before
/// the adapter has said what kind it is.
const PROBE_ID: &str = "wasm-probe";

impl WasmAdapter {
    /// Compile the component at `path` and check it (see [`Self::from_bytes`]).
    ///
    /// # Errors
    ///
    /// [`LoadError::Read`] when the file cannot be read; otherwise as
    /// [`Self::from_bytes`].
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let bytes = std::fs::read(path).map_err(|error| LoadError::Read {
            path: path.display().to_string(),
            error,
        })?;
        Self::from_bytes(&bytes)
    }

    /// Compile a component and check it: it must export
    /// `knobas:adapter/source` at a version this knobas hosts, link against
    /// what the host provides, and answer a descriptor that decodes.
    ///
    /// Blocks while the component runs its descriptor, on a thread of its
    /// own, so it may be called from inside an async runtime.
    ///
    /// # Errors
    ///
    /// The [`LoadError`] naming which of those failed.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LoadError> {
        let engine = limits::engine().clone();
        let component = Component::from_binary(&engine, bytes)
            .map_err(|e| LoadError::NotAComponent(format!("{e:#}")))?;
        let version = interface_version(&engine, &component)?;
        if !version.is_hosted() {
            return Err(LoadError::Unsupported { found: version });
        }
        let mut linker = Linker::<HostState>::new(&engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)
            .map_err(|e| LoadError::Link(format!("{e:#}")))?;
        wasmtime_wasi_http::p2::add_only_http_to_linker_async(&mut linker)
            .map_err(|e| LoadError::Link(format!("{e:#}")))?;
        bindings::Hosted::add_to_linker::<_, wasmtime::component::HasSelf<_>>(
            &mut linker,
            |state| state,
        )
        .map_err(|e| LoadError::Link(format!("{e:#}")))?;
        let pre = linker
            .instantiate_pre(&component)
            .and_then(bindings::HostedPre::new)
            .map_err(|e| LoadError::Link(format!("{e:#}")))?;
        let download = optional_export(&component, DOWNLOADS_EXPORT, "download")?;
        let translate_query = optional_export(&component, QUERY_EXPORT, "translate-query")?;
        let refresh = optional_export(&component, REFRESH_EXPORT, "refresh")?;
        let mut adapter = Self {
            inner: Arc::new(Inner {
                engine,
                pre,
                download,
                version,
                template: placeholder_descriptor(),
                translate_query,
                refresh,
            }),
            hosts: Arc::from([]),
            limits: limits::Limits::default(),
        };
        // Twice: the template's id is the adapter kind (`descriptor_template`'s
        // rule for a compiled-in adapter), which only the first answer names.
        let probe = adapter
            .describe(PROBE_ID, PROBE_ID)
            .map_err(|e| LoadError::Descriptor(e.to_string()))?;
        let template = adapter
            .describe(&probe.adapter_kind, &probe.adapter_kind)
            .map_err(|e| LoadError::Descriptor(e.to_string()))?;
        Arc::get_mut(&mut adapter.inner)
            .expect("nothing else holds the adapter yet")
            .template = template;
        if adapter.inner.download.is_some() {
            block_on(source::check_download(&adapter)).map_err(LoadError::Link)?;
        }
        if adapter.inner.refresh.is_some() {
            block_on(source::check_refresh(&adapter)).map_err(LoadError::Link)?;
        }
        Ok(adapter)
    }

    /// This adapter with the hosts its manifest declares: what each of its
    /// sources may reach, resolved per source at [`Self::build`].
    #[must_use]
    pub fn with_hosts(mut self, hosts: Vec<allow::HostPattern>) -> Self {
        self.hosts = Arc::from(hosts);
        self
    }

    /// This adapter with the limits its manifest asks for
    /// ([`limits::Limits::from_manifest`]).
    #[must_use]
    pub fn with_limits(mut self, limits: limits::Limits) -> Self {
        self.limits = limits;
        self
    }

    /// The deadline and memory cap of every call into it.
    #[must_use]
    pub fn limits(&self) -> limits::Limits {
        self.limits
    }

    /// The hosts its manifest declares.
    #[must_use]
    pub fn hosts(&self) -> &[allow::HostPattern] {
        &self.hosts
    }

    /// The adapter kind the component says it is.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.inner.template.adapter_kind
    }

    /// Whether the component exports `download` (1.1.0); a source of one
    /// that does not keeps no runs to download.
    #[must_use]
    pub fn downloads(&self) -> bool {
        self.inner.download.is_some()
    }

    /// Whether the component exports `refresh` (1.4.0, #2101). A source of
    /// one that does not answers *not offered* for every id, and its
    /// descriptor declares no `Capability::Refresh` whatever the component
    /// says.
    #[must_use]
    pub fn refreshes(&self) -> bool {
        self.inner.refresh.is_some()
    }

    /// The interface version the component is built against.
    #[must_use]
    pub fn version(&self) -> Version {
        self.inner.version
    }

    /// The descriptor template: the component's descriptor for an instance
    /// whose id is its adapter kind, as a compiled-in adapter's
    /// `descriptor_template` is.
    #[must_use]
    pub fn template(&self) -> SourceDescriptor {
        self.inner.template.clone()
    }

    /// A source of this adapter, from a configured instance: the registry's
    /// `build`. The component's descriptor is read once, here.
    ///
    /// Its policy is set here too: the manifest's hosts resolved over the
    /// instance's address and declared config, and its credential.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] for an instance of another kind, an id that
    /// is not a usable instance id, a credential the host cannot attach
    /// ([`net::Policy::for_source`]), or a component whose descriptor does
    /// not decode or does not name the instance it was given.
    pub fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        if instance.kind != self.kind() {
            return Err(SourceError::protocol(format!(
                "the external adapter {:?} cannot build an instance of kind {:?}",
                self.kind(),
                instance.kind
            )));
        }
        knobas_source::instance::validate_instance_id(&instance.id)
            .map_err(|error| SourceError::protocol(error.to_string()))?;
        let policy = Arc::new(net::Policy::for_source(
            &instance,
            &self.hosts,
            &self.inner.template.config_schema,
        )?);
        let wit_instance = convert::instance(&instance);
        let descriptor = block_on(source::descriptor(
            self,
            wit_instance.clone(),
            Arc::clone(&policy),
        ))?;
        if descriptor.id != instance.id || descriptor.adapter_kind != instance.kind {
            return Err(SourceError::protocol(format!(
                "the external adapter {:?} described itself as {:?} of kind {:?} when built as \
                 {:?}",
                self.kind(),
                descriptor.id,
                descriptor.adapter_kind,
                instance.id
            )));
        }
        Ok(Box::new(WasmSource::new(
            self.clone(),
            wit_instance,
            descriptor,
            policy,
        )))
    }

    fn describe(&self, id: &str, kind: &str) -> Result<SourceDescriptor, SourceError> {
        block_on(source::descriptor(
            self,
            wit::Instance {
                id: id.to_owned(),
                kind: kind.to_owned(),
                display_name: id.to_owned(),
                base_url: String::new(),
                config: "{}".to_owned(),
            },
            Arc::new(net::Policy::none(id)),
        ))
    }
}

/// Which version of `knobas:adapter/source` the component exports.
fn interface_version(engine: &Engine, component: &Component) -> Result<Version, LoadError> {
    let ty = component.component_type();
    let mut found = None;
    for (name, item) in ty.exports(engine) {
        let Some(rest) = name.strip_prefix(SOURCE_EXPORT) else {
            continue;
        };
        if !matches!(item.ty, ComponentItem::ComponentInstance(_)) {
            continue;
        }
        let version = rest
            .strip_prefix('@')
            .and_then(Version::parse)
            .ok_or_else(|| LoadError::NotAComponent(format!("export {name:?} names no version")))?;
        found = Some(version);
    }
    found.ok_or(LoadError::NoSource)
}

/// A descriptor that holds nothing, for the moment between compiling a
/// component and reading its own.
fn placeholder_descriptor() -> SourceDescriptor {
    serde_json::from_value(serde_json::json!({
        "id": PROBE_ID,
        "adapter_kind": PROBE_ID,
        "name": PROBE_ID,
        "capabilities": [],
        "adapter_version": "",
        "auth_methods": [],
        "write_ops": [],
        "entity_kinds": [],
        "config_schema": {},
    }))
    .expect("a descriptor with every required field decodes")
}

/// Run `future` to completion on a thread of its own, so a synchronous SPI
/// call (`build`, `descriptor`) can reach the component from anywhere,
/// including from inside a runtime that would refuse a nested `block_on`.
fn block_on<F>(future: F) -> F::Output
where
    F: std::future::Future + Send,
    F::Output: Send,
{
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("a current-thread runtime builds")
                    .block_on(future)
            })
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

/// What one call's store holds.
pub(crate) struct HostState {
    wasi: WasiCtx,
    http: wasmtime_wasi_http::WasiHttpCtx,
    hooks: net::Hooks,
    table: ResourceTable,
    memory: limits::MemoryCap,
    sink: source::SinkSlot,
    download: source::DownloadSlot,
    /// The instance id, for the component's log lines.
    source_id: String,
    /// What the component may still write to the log.
    log: limits::LogBudget,
}

impl Drop for HostState {
    fn drop(&mut self) {
        self.report_dropped_log_lines();
    }
}

impl HostState {
    fn new(
        source_id: String,
        sink: source::SinkSlot,
        policy: Arc<net::Policy>,
        limits: limits::Limits,
    ) -> Self {
        Self {
            // Nothing in it: no preopened directory, no environment, no
            // argument, stdio to nowhere, and the defaults refuse every
            // socket address and every name lookup.
            wasi: WasiCtx::builder().build(),
            http: wasmtime_wasi_http::WasiHttpCtx::new(),
            hooks: net::Hooks::new(policy, limits),
            table: ResourceTable::new(),
            memory: limits::MemoryCap::new(limits),
            sink,
            download: source::DownloadSlot::none(),
            source_id,
            log: limits::LogBudget::new(std::time::Instant::now()),
        }
    }

    /// Say how many of the component's log lines were dropped since this
    /// was last said.
    fn report_dropped_log_lines(&mut self) {
        let dropped = self.log.take_dropped();
        if dropped > 0 {
            tracing::warn!(
                source = self.source_id.as_str(),
                "the external adapter wrote {dropped} more log lines than the host writes \
                 ({} a call, then one each {:?}); they were dropped",
                limits::LOG_BURST,
                limits::LOG_REFILL,
            );
        }
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl wasmtime_wasi_http::WasiHttpView for HostState {
    fn http(&mut self) -> wasmtime_wasi_http::WasiHttpCtxView<'_> {
        wasmtime_wasi_http::WasiHttpCtxView {
            ctx: &mut self.http,
            table: &mut self.table,
            hooks: &mut self.hooks,
        }
    }
}

fn new_store(
    adapter: &WasmAdapter,
    source_id: &str,
    sink: source::SinkSlot,
    policy: Arc<net::Policy>,
) -> Store<HostState> {
    let mut store = Store::new(
        &adapter.inner.engine,
        HostState::new(source_id.to_owned(), sink, policy, adapter.limits),
    );
    store.limiter(|state| &mut state.memory);
    // Yield at every tick, so the deadline (`limits::bounded`) can drop a
    // call whose WASM never returns to the executor on its own.
    store.set_epoch_deadline(1);
    store.epoch_deadline_callback(|_| Ok(wasmtime::UpdateDeadline::Yield(1)));
    store
}

/// Where `just wasm-fixture` writes the fixture component, or
/// `KNOBAS_WASM_FIXTURE` when that names another file. For tests: the
/// fixture is a test adapter and nothing ships it, hence `test-util`.
#[cfg(feature = "test-util")]
#[must_use]
pub fn fixture_component() -> std::path::PathBuf {
    std::env::var_os("KNOBAS_WASM_FIXTURE").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/wasm32-wasip2/release/knobas_wasm_fixture.wasm")
        },
        std::path::PathBuf::from,
    )
}

/// The fixture built against the frozen 1.0.0 world, which `just
/// wasm-fixture` keeps beside [`fixture_component`] (#1205).
#[cfg(feature = "test-util")]
#[must_use]
pub fn fixture_component_v1_0() -> std::path::PathBuf {
    let current = fixture_component();
    current.with_file_name("knobas_wasm_fixture_v1_0.wasm")
}

/// The fixture built against the frozen 1.3.0 world, the minor before
/// `refresh`, which `just wasm-fixture` keeps beside [`fixture_component`]
/// (#2101).
#[cfg(feature = "test-util")]
#[must_use]
pub fn fixture_component_v1_3() -> std::path::PathBuf {
    let current = fixture_component();
    current.with_file_name("knobas_wasm_fixture_v1_3.wasm")
}

/// The fixture component, loaded; panics naming the recipe that builds it.
///
/// Compiled once per test binary and cloned after: a compile is most of
/// what a test of the fixture costs, and a clone shares nothing a call can
/// change (each call has a store of its own).
#[cfg(feature = "test-util")]
#[must_use]
pub fn load_fixture() -> WasmAdapter {
    static LOADED: std::sync::OnceLock<WasmAdapter> = std::sync::OnceLock::new();
    LOADED
        .get_or_init(|| {
            let path = fixture_component();
            WasmAdapter::load(&path).unwrap_or_else(|e| {
                panic!(
                    "the fixture component at {} does not load ({e}); `just wasm-fixture` \
                     builds it",
                    path.display()
                )
            })
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A component exporting `interface` with one function `function`, or
    /// with none when `interface` is `None`; `knobas:adapter/source@1.0.0`
    /// beside it either way.
    fn component(interface: Option<&str>, function: &str) -> Component {
        let optional = interface.map_or(String::new(), |name| {
            format!(
                r#"(core module $m (func (export "f")))
                   (core instance $i (instantiate $m))
                   (func $f (canon lift (core func $i "f")))
                   (instance $q (export "{function}" (func $f)))
                   (export "{name}" (instance $q))"#
            )
        });
        let text = format!(
            r#"(component
                 (instance $source)
                 (export "knobas:adapter/source@1.0.0" (instance $source))
                 {optional})"#
        );
        Component::new(&Engine::default(), wat::parse_str(text).unwrap()).unwrap()
    }

    fn translates(interface: Option<&str>) -> bool {
        optional_export(
            &component(interface, "translate-query"),
            QUERY_EXPORT,
            "translate-query",
        )
        .unwrap()
        .is_some()
    }

    #[test]
    fn the_optional_read_is_found_on_any_1_x_that_exports_it_and_on_nothing_else() {
        assert!(translates(Some("knobas:adapter/query@1.2.0")));
        assert!(
            translates(Some("knobas:adapter/query@1.4.2")),
            "a newer 1.x"
        );
        assert!(!translates(None), "a 1.0.0 component has no query");
        assert!(
            !translates(Some("knobas:adapter/query@2.0.0")),
            "another major"
        );
        assert!(
            !translates(Some("knobas:adapter/queries@1.2.0")),
            "another interface"
        );
    }

    #[test]
    fn a_1_1_0_component_s_download_is_found_by_the_1_2_0_host() {
        let built = component(Some("knobas:adapter/downloads@1.1.0"), "download");
        assert!(
            optional_export(&built, DOWNLOADS_EXPORT, "download")
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn an_optional_interface_without_its_function_is_refused() {
        let built = component(Some("knobas:adapter/query@1.2.0"), "translate");
        let error = optional_export(&built, QUERY_EXPORT, "translate-query").unwrap_err();
        assert_eq!(
            error.to_string(),
            LoadError::Link("knobas:adapter/query exports no translate-query".into()).to_string()
        );
    }
}
