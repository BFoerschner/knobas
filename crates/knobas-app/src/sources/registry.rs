//! The compiled-in adapter table (P6).
//!
//! Spec §3a: "v1 plugins are Rust crates: one crate implementing `Source` + one
//! registry line." This is that line -- literally one row per adapter, holding
//! the two functions every adapter crate exposes:
//!
//! ```ignore
//! pub fn descriptor_template() -> SourceDescriptor;   // id == adapter_kind
//! pub fn build(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError>;
//! ```
//!
//! **The table is append-only, like the `generate_handler!` list**: streams B
//! and C each add one row as their adapter lands, and a rebase conflict here is
//! one line. Nothing else in knobas may hold a per-adapter table (§3a: the UI
//! renders a source from its descriptor, never from a hardcoded map), so if a
//! second one ever appears, it is a bug.
//!
//! **External adapters sit beside the table** (ADR-0028, #1198): a
//! [`WasmAdapter`] builds a `dyn Source` as a row does, and the rest of
//! knobas cannot tell the two apart. Until install lands (#1202) the only
//! external adapter is the **dev adapter**: in a debug build,
//! `KNOBAS_DEV_ADAPTER=<file.wasm>` loads one component at the first
//! [`Registry::builtin`] and keeps it for the life of the process.

use knobas_source::instance::SourceInstance;
use knobas_source::{Source, SourceDescriptor, SourceError};
use knobas_wasm_host::WasmAdapter;

/// One compiled-in adapter kind.
struct Adapter {
    kind: &'static str,
    template: fn() -> SourceDescriptor,
    build: fn(SourceInstance) -> Result<Box<dyn Source>, SourceError>,
    /// The Docs projection (#859, ADR-0022), for an adapter that emits pages.
    doc: Option<knobas_source::docs::DocProjection>,
    /// The Docs writes in the source's markup (#862), for an adapter whose
    /// pages knobas edits and creates.
    doc_writes: Option<knobas_source::docs::DocWrites>,
}

/// The table. One row per adapter crate.
///
/// `kind` is written out rather than read from the template, because it is the
/// **lookup** key: reading it from `(template)()` would build every descriptor
/// on every call, and `list_adapters` is explicitly the path that instantiates
/// nothing. `every_adapter_crate_linked_into_the_app_has_a_row` is what keeps
/// the two spellings honest.
const ADAPTERS: &[Adapter] = &[
    Adapter {
        kind: "mock",
        template: knobas_source_mock::descriptor_template,
        build: knobas_source_mock::build,
        doc: None,
        doc_writes: None,
    },
    Adapter {
        kind: "jira",
        template: knobas_source_jira::descriptor_template,
        build: knobas_source_jira::build,
        doc: None,
        doc_writes: None,
    },
    Adapter {
        kind: "teamcity",
        template: knobas_source_teamcity::descriptor_template,
        build: knobas_source_teamcity::build,
        doc: None,
        doc_writes: None,
    },
    Adapter {
        kind: "gitea",
        template: knobas_source_gitea::descriptor_template,
        build: knobas_source_gitea::build,
        doc: None,
        doc_writes: None,
    },
    Adapter {
        kind: "confluence",
        template: knobas_source_confluence::descriptor_template,
        build: knobas_source_confluence::build,
        doc: Some(knobas_source_confluence::doc_content),
        doc_writes: Some(knobas_source_confluence::DOC_WRITES),
    },
    Adapter {
        kind: "kuma",
        template: knobas_source_kuma::descriptor_template,
        build: knobas_source_kuma::build,
        doc: None,
        doc_writes: None,
    },
];

/// The variable that names the dev adapter's component.
pub const DEV_ADAPTER_VAR: &str = "KNOBAS_DEV_ADAPTER";

/// Turns stored configurations into live adapters.
#[derive(Debug, Default, Clone)]
pub struct Registry {
    /// External adapters, after every compiled-in row. Never of a kind a
    /// row has: [`Registry::with_external`] drops such a one.
    external: Vec<WasmAdapter>,
}

impl Registry {
    /// The compiled-in table, and in a debug build the dev adapter, if
    /// [`DEV_ADAPTER_VAR`] names one that loads.
    #[must_use]
    pub fn builtin() -> Registry {
        static DEV: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();
        DEV.get_or_init(|| {
            let var = std::env::var_os(DEV_ADAPTER_VAR);
            Registry::with_external(dev_adapter(cfg!(debug_assertions), var.as_deref()))
        })
        .clone()
    }

    /// The compiled-in table and `external`, minus any external adapter of
    /// a kind already taken: a row's kind, or an earlier adapter's. Each
    /// dropped one is logged.
    #[must_use]
    pub fn with_external(external: Vec<WasmAdapter>) -> Registry {
        let mut kept: Vec<WasmAdapter> = Vec::new();
        for adapter in external {
            let taken = ADAPTERS.iter().any(|a| a.kind == adapter.kind())
                || kept.iter().any(|k| k.kind() == adapter.kind());
            if taken {
                tracing::error!(
                    kind = adapter.kind(),
                    "an external adapter of a kind knobas already has is not loaded"
                );
            } else {
                kept.push(adapter);
            }
        }
        Registry { external: kept }
    }

    /// One descriptor **template** per compiled-in kind.
    ///
    /// Also reachable as [`AdapterRegistry::descriptors`]; kept as an inherent
    /// method because `list_adapters` is the one caller that has no
    /// `SourcesState` to reach a trait object through -- it answers before
    /// bring-up, which is what lets the Add-source form be drawn on a cold
    /// start.
    #[must_use]
    pub fn templates(&self) -> Vec<SourceDescriptor> {
        ADAPTERS
            .iter()
            .map(|a| (a.template)())
            .chain(self.external.iter().map(WasmAdapter::template))
            .collect()
    }

    /// The Docs projection of an adapter kind (#859), `None` for a kind that
    /// draws no pages or is not compiled in. A plain function over the
    /// mirrored payload, so the reader needs no built source and no secret.
    #[must_use]
    pub fn doc_projection(&self, kind: &str) -> Option<knobas_source::docs::DocProjection> {
        ADAPTERS.iter().find(|a| a.kind == kind).and_then(|a| a.doc)
    }

    /// The Docs writes of an adapter kind (#862): a section edit and a new
    /// page's body in its own markup. `None` for a kind knobas writes no
    /// pages for.
    #[must_use]
    pub fn doc_writes(&self, kind: &str) -> Option<knobas_source::docs::DocWrites> {
        ADAPTERS
            .iter()
            .find(|a| a.kind == kind)
            .and_then(|a| a.doc_writes)
    }
}

impl knobas_sync::scheduler::AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        self.templates()
    }

    /// Build an instance, routing on [`SourceInstance::kind`].
    ///
    /// The kind and not the id: two Jiras are `jira` and `jira-eu`, two
    /// instances of one adapter, and the id is the entity namespace rather than
    /// the thing to look up (P10).
    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        if let Some(adapter) = ADAPTERS.iter().find(|a| a.kind == instance.kind) {
            return (adapter.build)(instance);
        }
        if let Some(adapter) = self.external.iter().find(|a| a.kind() == instance.kind) {
            return adapter.build(instance);
        }
        Err(SourceError::protocol(format!(
            "no adapter of kind {:?} is compiled in",
            instance.kind
        )))
    }
}

/// The dev adapter [`DEV_ADAPTER_VAR`] names: none in a release build, none
/// when the variable is unset or empty, and none, with the reason logged,
/// when the file does not load.
fn dev_adapter(debug: bool, var: Option<&std::ffi::OsStr>) -> Vec<WasmAdapter> {
    let Some(path) = var.filter(|v| debug && !v.is_empty()) else {
        return Vec::new();
    };
    let path = std::path::Path::new(path);
    match WasmAdapter::load(path) {
        Ok(adapter) => {
            tracing::info!(kind = adapter.kind(), path = %path.display(), "dev adapter loaded");
            vec![adapter]
        }
        Err(error) => {
            tracing::error!(path = %path.display(), %error, "the dev adapter does not load");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> std::ffi::OsString {
        knobas_wasm_host::fixture_component().into_os_string()
    }

    #[test]
    fn a_debug_build_loads_the_dev_adapter_the_variable_names() {
        let kinds: Vec<String> = dev_adapter(true, Some(&fixture()))
            .iter()
            .map(|a| a.kind().to_owned())
            .collect();
        assert_eq!(kinds, ["wasm-fixture"]);
    }

    #[test]
    fn a_release_build_loads_no_dev_adapter() {
        assert!(dev_adapter(false, Some(&fixture())).is_empty());
    }

    #[test]
    fn an_unset_or_unloadable_variable_loads_none() {
        assert!(dev_adapter(true, None).is_empty());
        assert!(dev_adapter(true, Some(std::ffi::OsStr::new(""))).is_empty());
        assert!(dev_adapter(true, Some(std::ffi::OsStr::new("/nonexistent/x.wasm"))).is_empty());
    }

    #[test]
    fn an_external_adapter_joins_the_templates_once() {
        let fixture = knobas_wasm_host::load_fixture();
        let registry = Registry::with_external(vec![fixture.clone(), fixture]);
        let kinds: Vec<String> = registry.templates().into_iter().map(|t| t.id).collect();
        assert_eq!(kinds.iter().filter(|k| *k == "wasm-fixture").count(), 1);
        assert_eq!(kinds.len(), ADAPTERS.len() + 1);
    }
}
