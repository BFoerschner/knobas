//! The Rust SDK of the adapter standard (spec #1192, ADR-0028): what an
//! external adapter written in Rust depends on.
//!
//! Writing an adapter is implementing [`Adapter`] and naming the type in
//! [`export!`]:
//!
//! ```no_run
//! use knobas_adapter_sdk::{Adapter, ConnectionInfo, Instance, Sink, SourceDescriptor};
//! use knobas_adapter_sdk::{Cursor, SourceError, WriteOp, WriteReceipt, spi::view::SourceView};
//!
//! struct Surveys { instance: Instance }
//!
//! impl Adapter for Surveys {
//!     fn new(instance: Instance) -> Self { Self { instance } }
//!     fn descriptor(&self) -> SourceDescriptor { todo!() }
//!     fn test_connection(&self) -> Result<ConnectionInfo, SourceError> { todo!() }
//!     fn sync(&self, cursor: Option<Cursor>, sink: &mut Sink) -> Result<Cursor, SourceError> { todo!() }
//!     fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> { todo!() }
//!     fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> { todo!() }
//!     fn view(&self, entity: &str, section: Option<&str>) -> Result<Option<SourceView>, SourceError> { todo!() }
//! }
//!
//! knobas_adapter_sdk::export!(Surveys);
//! ```
//!
//! built as a `cdylib` with `cargo build --target wasm32-wasip2`.
//!
//! * **The interface** is `knobas:adapter`, the WIT world in
//!   `crates/knobas-wasm-host/wit/adapter.wit`, at [`INTERFACE_VERSION`],
//!   which is this crate's version too. A knobas whose host has the same
//!   major version and the same minor version or a newer one hosts what this
//!   SDK builds.
//! * **The SPI types** are `knobas-source`'s own, re-exported whole as
//!   [`spi`] and the common ones at the root. Every one crosses the
//!   interface in exactly the serde form the host decodes it with; the
//!   conversions are this crate's, and an adapter writes none.
//! * **What the component may reach** is the host's to grant (ADR-0028): no
//!   disk, no environment, no keychain, and `wasi:http` 0.2's outgoing
//!   handler held to the hosts the adapter's manifest declares. The host
//!   attaches the credential, so an [`Instance`] carries none.

// The bindings name this crate by its own name, so that the paths they
// write resolve in the adapter crate the export macro expands in, and here.
extern crate self as knobas_adapter_sdk;

mod convert;
mod glue;

pub use knobas_source as spi;
pub use knobas_source::{
    ConnectionInfo, Cursor, SourceDescriptor, SourceError, SyncItem, WriteOp, WriteReceipt,
};

use knobas_source::{pipeline, run, view};

/// The `knobas:adapter` version this SDK builds against: its WIT package's,
/// and this crate's own version.
#[cfg(not(feature = "v1_0"))]
pub const INTERFACE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The `knobas:adapter` version this SDK builds against: the frozen 1.0.0
/// world, under the `v1_0` feature.
#[cfg(feature = "v1_0")]
pub const INTERFACE_VERSION: &str = "1.0.0";

/// The generated bindings, for [`export!`] alone. Nothing in them is an
/// adapter's to name: the SDK's own types stand for every one.
#[doc(hidden)]
pub mod bindings {
    #[cfg(not(feature = "v1_0"))]
    wit_bindgen::generate!({
        world: "adapter",
        path: "../knobas-wasm-host/wit",
        pub_export_macro: true,
        export_macro_name: "export_world",
        default_bindings_module: "knobas_adapter_sdk::bindings",
        runtime_path: "knobas_adapter_sdk::__private::wit_bindgen::rt",
    });

    #[cfg(feature = "v1_0")]
    wit_bindgen::generate!({
        world: "adapter",
        path: "wit-1.0.0",
        pub_export_macro: true,
        export_macro_name: "export_world",
        default_bindings_module: "knobas_adapter_sdk::bindings",
        runtime_path: "knobas_adapter_sdk::__private::wit_bindgen::rt",
    });
}

#[doc(hidden)]
pub mod __private {
    pub use wit_bindgen;

    pub use crate::glue::Component;
}

/// Export `$adapter`, a type that implements [`Adapter`], as the
/// component's `knobas:adapter` world. Once per component, in the crate
/// built as the `cdylib`.
#[macro_export]
macro_rules! export {
    ($adapter:ident) => {
        $crate::bindings::export_world!($adapter with_types_in $crate::bindings);
    };
}

/// One configured source instance, as the host hands it over:
/// `knobas_source::instance::SourceInstance` without its credentials, which
/// the component never sees (ADR-0028).
#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    /// The source's id: the namespace of every entity it emits.
    pub id: String,
    /// The adapter kind, the descriptor's `adapter_kind`.
    pub kind: String,
    pub display_name: String,
    pub base_url: String,
    /// `source_config.config`, the object the descriptor's `config_schema`
    /// describes. The host always sends JSON; text that is not reads as
    /// `null`.
    pub config: serde_json::Value,
}

/// Lines into knobas' own log, tagged with the source's id.
pub mod log {
    pub use crate::bindings::knobas::adapter::log::{Level, log};
}

/// Where [`Adapter::sync`] hands its items: `knobas_source::Sink`, owned by
/// the host.
pub struct Sink {
    /// Private, so only the SDK makes one.
    _host: (),
}

impl Sink {
    /// Accept one item, or fail the sync. An error is the sink's: return it
    /// from `sync` with `?`, as it came (`Source::sync` says why).
    pub fn item(&mut self, item: SyncItem) -> Result<(), SourceError> {
        bindings::knobas::adapter::sink::item(&convert::item(item)).map_err(convert::error_from_wit)
    }

    /// How far the run's outer walk has got: `Sink::walked`. `done` units
    /// of `of` (`None` when the adapter cannot know the total), `unit` a
    /// plural noun the UI prints as is. Before each unit and once after the
    /// last. A reading: it cannot fail the sync. Built against 1.0.0, which
    /// has no `walked`, it reports nothing.
    pub fn walked(&mut self, done: u64, of: Option<u64>, unit: &str) {
        #[cfg(not(feature = "v1_0"))]
        bindings::knobas::adapter::sink::walked(done, of, unit);
        #[cfg(feature = "v1_0")]
        let _ = (done, of, unit);
    }
}

/// Where [`Adapter::download`] streams a download's bytes:
/// `knobas_source::run::DownloadSink`, owned by the host, which hands each
/// chunk to the file being written.
#[cfg(not(feature = "v1_0"))]
pub struct DownloadSink {
    /// Private, so only the SDK makes one.
    _host: (),
}

#[cfg(not(feature = "v1_0"))]
impl DownloadSink {
    /// Once, before the first byte: the file name the source suggests.
    pub fn start(&mut self, file_name: &str) -> Result<(), SourceError> {
        bindings::knobas::adapter::download_sink::start(file_name).map_err(convert::error_from_wit)
    }

    /// The next bytes, in order. An error is the sink's: return it from
    /// `download` as it came.
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), SourceError> {
        bindings::knobas::adapter::download_sink::write(bytes).map_err(convert::error_from_wit)
    }
}

/// An external adapter: `knobas_source::Source`, method for method in the
/// trait's order, as a component implements it. `undeclared_reason` is left
/// out: the host refuses an undeclared write before it crosses, with the
/// generic wording, as `Source` says of every WASM guest.
///
/// What each method answers, and what the contract battery holds it to, is
/// the `Source` method of the same name; only the shape differs. A
/// component makes blocking calls, so every method is synchronous. The host
/// makes one instance for each call, with [`new`](Adapter::new), so an
/// adapter keeps no state between calls but what its cursor carries. The
/// optional reads default as `Source`'s do.
pub trait Adapter: Sized + 'static {
    /// The instance one call runs against.
    fn new(instance: Instance) -> Self;

    fn descriptor(&self) -> SourceDescriptor;

    fn test_connection(&self) -> Result<ConnectionInfo, SourceError>;

    /// Hand every item changed since `cursor` to `sink` (`None` is a full
    /// sync), then return the new cursor.
    fn sync(&self, cursor: Option<Cursor>, sink: &mut Sink) -> Result<Cursor, SourceError>;

    fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError>;

    fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError>;

    fn view(
        &self,
        entity: &str,
        section: Option<&str>,
    ) -> Result<Option<view::SourceView>, SourceError>;

    fn pipeline_settings(
        &self,
        entity: &str,
    ) -> Result<Option<pipeline::PipelineSettings>, SourceError> {
        let _ = entity;
        Ok(None)
    }

    fn older_runs(
        &self,
        entity: &str,
        before: &str,
        count: u32,
    ) -> Result<Option<Vec<SyncItem>>, SourceError> {
        let _ = (entity, before, count);
        Ok(None)
    }

    fn run_chain(&self, run: &str) -> Result<Option<Vec<pipeline::RunChainStep>>, SourceError> {
        let _ = run;
        Ok(None)
    }

    fn run_log(
        &self,
        entity: &str,
        from: Option<u64>,
        count: u32,
    ) -> Result<Option<run::RunLog>, SourceError> {
        let _ = (entity, from, count);
        Ok(None)
    }

    fn run_tests(&self, entity: &str) -> Result<Option<run::RunTests>, SourceError> {
        let _ = entity;
        Ok(None)
    }

    fn test_history(
        &self,
        entity: &str,
        test: &str,
    ) -> Result<Option<Vec<run::TestOutcome>>, SourceError> {
        let _ = (entity, test);
        Ok(None)
    }

    fn run_artifacts(
        &self,
        entity: &str,
        path: &str,
    ) -> Result<Option<Vec<run::ArtifactEntry>>, SourceError> {
        let _ = (entity, path);
        Ok(None)
    }

    fn artifact_preview(
        &self,
        entity: &str,
        path: &str,
    ) -> Result<Option<run::ArtifactPreview>, SourceError> {
        let _ = (entity, path);
        Ok(None)
    }

    /// `view` is the saved view's payload as this adapter emitted it.
    #[cfg(not(feature = "v1_0"))]
    fn translate_query(
        &self,
        view: &serde_json::Value,
    ) -> Result<Option<knobas_source::query::Translation>, SourceError> {
        let _ = view;
        Ok(None)
    }

    #[cfg(not(feature = "v1_0"))]
    fn download(
        &self,
        entity: &str,
        target: &run::DownloadTarget,
        sink: &mut DownloadSink,
    ) -> Result<bool, SourceError> {
        let _ = (entity, target, sink);
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    /// The interface version the bindings are generated from: the
    /// `package` line of the WIT this crate binds.
    fn wit_version(wit: &str) -> &str {
        wit.lines()
            .find_map(|line| line.strip_prefix("package knobas:adapter@"))
            .and_then(|rest| rest.strip_suffix(';'))
            .expect("the WIT names its package")
    }

    /// The crate's version names the interface it targets (#2028): a WIT
    /// minor that does not bump this crate fails here.
    #[test]
    fn the_crate_version_is_the_interface_version_it_binds() {
        let wit = include_str!("../../knobas-wasm-host/wit/adapter.wit");
        assert_eq!(env!("CARGO_PKG_VERSION"), wit_version(wit));
        #[cfg(not(feature = "v1_0"))]
        assert_eq!(super::INTERFACE_VERSION, wit_version(wit));
        #[cfg(feature = "v1_0")]
        assert_eq!(
            super::INTERFACE_VERSION,
            wit_version(include_str!("../wit-1.0.0/adapter.wit"))
        );
    }
}
