//! The WASM host (ADR-0028, issue #1198): an external adapter, run under
//! wasmtime, is one more [`Source`].
//!
//! An external adapter is one WASM component built against the WIT world in
//! `wit/adapter.wit`, `knobas:adapter@1.0.0`. [`WasmAdapter::load`] compiles
//! it, refuses one built against a version this knobas cannot host, and
//! reads its descriptor. [`WasmAdapter::build`] turns a configured instance
//! into a `Box<dyn Source>`, which forwards each trait call across the
//! interface. The sync engine, the write queue, IPC and the shell see that
//! and nothing else.
//!
//! **What the component is given.** Its own `instance` record, which holds
//! no credential; the `sink` for a sync's items; `log`; and WASI 0.2 with
//! nothing in it: no directory, no environment, no argument, and the
//! context's defaults refuse every socket and name lookup. Network access
//! through an allowlist, and the credential the host attaches, are #1203.
//! Call deadlines and memory caps are #1204.
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
use wasmtime::component::{Component, Linker, ResourceTable, types::ComponentItem};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

mod convert;
pub mod manifest;
mod source;

pub use source::WasmSource;

mod bindings {
    wasmtime::component::bindgen!({
        world: "adapter",
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
    minor: 0,
    patch: 0,
};

/// The interface the component exports, without its version.
const SOURCE_EXPORT: &str = "knobas:adapter/source";

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
    // At 1.0.0 the minor comparison is `== 0`; written as `<=` because it is
    // the rule, and the rule holds once the host's minor grows.
    #[allow(clippy::absurd_extreme_comparisons)]
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
}

struct Inner {
    engine: Engine,
    pre: bindings::AdapterPre<HostState>,
    version: Version,
    template: SourceDescriptor,
}

impl std::fmt::Debug for WasmAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmAdapter")
            .field("kind", &self.inner.template.adapter_kind)
            .field("version", &self.inner.version)
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
        let engine = Engine::default();
        let component = Component::from_binary(&engine, bytes)
            .map_err(|e| LoadError::NotAComponent(format!("{e:#}")))?;
        let version = interface_version(&engine, &component)?;
        if !version.is_hosted() {
            return Err(LoadError::Unsupported { found: version });
        }
        let mut linker = Linker::<HostState>::new(&engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)
            .map_err(|e| LoadError::Link(format!("{e:#}")))?;
        bindings::Adapter::add_to_linker::<_, wasmtime::component::HasSelf<_>>(
            &mut linker,
            |state| state,
        )
        .map_err(|e| LoadError::Link(format!("{e:#}")))?;
        let pre = linker
            .instantiate_pre(&component)
            .and_then(bindings::AdapterPre::new)
            .map_err(|e| LoadError::Link(format!("{e:#}")))?;
        let mut adapter = Self {
            inner: Arc::new(Inner {
                engine,
                pre,
                version,
                template: placeholder_descriptor(),
            }),
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
        Ok(adapter)
    }

    /// The adapter kind the component says it is.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.inner.template.adapter_kind
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
    /// # Errors
    ///
    /// [`SourceError::Protocol`] for an instance of another kind, an id that
    /// is not a usable instance id, or a component whose descriptor does
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
        let wit_instance = convert::instance(&instance);
        let descriptor = block_on(source::descriptor(self, wit_instance.clone()))?;
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
    table: ResourceTable,
    sink: source::SinkSlot,
    /// The instance id, for the component's log lines.
    source_id: String,
}

impl HostState {
    fn new(source_id: String, sink: source::SinkSlot) -> Self {
        Self {
            // Nothing in it: no preopened directory, no environment, no
            // argument, stdio to nowhere, and the defaults refuse every
            // socket address and every name lookup.
            wasi: WasiCtx::builder().build(),
            table: ResourceTable::new(),
            sink,
            source_id,
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

fn new_store(adapter: &WasmAdapter, source_id: &str, sink: source::SinkSlot) -> Store<HostState> {
    Store::new(
        &adapter.inner.engine,
        HostState::new(source_id.to_owned(), sink),
    )
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

/// The fixture component, loaded; panics naming the recipe that builds it.
#[cfg(feature = "test-util")]
#[must_use]
pub fn load_fixture() -> WasmAdapter {
    let path = fixture_component();
    WasmAdapter::load(&path).unwrap_or_else(|e| {
        panic!(
            "the fixture component at {} does not load ({e}); `just wasm-fixture` builds it",
            path.display()
        )
    })
}
