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

use knobas_source::instance::SourceInstance;
use knobas_source::{Source, SourceDescriptor, SourceError};

/// One compiled-in adapter kind.
struct Adapter {
    kind: &'static str,
    template: fn() -> SourceDescriptor,
    build: fn(SourceInstance) -> Result<Box<dyn Source>, SourceError>,
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
    },
    Adapter {
        kind: "jira",
        template: knobas_source_jira::descriptor_template,
        build: knobas_source_jira::build,
    },
];
// Stream B adds:  Adapter { kind: "gitea",    template: knobas_source_gitea::descriptor_template,    build: knobas_source_gitea::build }
// Stream C adds:  Adapter { kind: "teamcity", template: knobas_source_teamcity::descriptor_template, build: knobas_source_teamcity::build }

/// Turns stored configurations into live adapters.
#[derive(Debug, Default)]
pub struct Registry;

impl Registry {
    #[must_use]
    pub fn builtin() -> Registry {
        Registry
    }

    /// One descriptor **template** per compiled-in kind.
    #[must_use]
    pub fn templates() -> Vec<SourceDescriptor> {
        ADAPTERS.iter().map(|a| (a.template)()).collect()
    }

    /// The template for one kind, or `None` if no adapter answers to it.
    #[must_use]
    pub fn template_for(kind: &str) -> Option<SourceDescriptor> {
        ADAPTERS
            .iter()
            .find(|a| a.kind == kind)
            .map(|a| (a.template)())
    }

    /// Build an instance, routing on [`SourceInstance::kind`].
    ///
    /// The kind and not the id: two Jiras are `jira` and `jira-eu`, two
    /// instances of one adapter, and the id is the entity namespace rather than
    /// the thing to look up (P10).
    ///
    /// # Errors
    /// [`SourceError::Protocol`] if no adapter answers to the instance's kind,
    /// or if the adapter rejected the configuration.
    pub fn build_instance(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        let adapter = ADAPTERS
            .iter()
            .find(|a| a.kind == instance.kind)
            .ok_or_else(|| {
                SourceError::Protocol(format!(
                    "no adapter of kind {:?} is compiled in",
                    instance.kind
                ))
            })?;
        (adapter.build)(instance)
    }
}

impl knobas_sync::scheduler::AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        Registry::templates()
    }

    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Registry::build_instance(instance)
    }
}
