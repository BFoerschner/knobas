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
//! knobas cannot tell the two apart. They are the installed ones (#1202):
//! bring-up loads each row of `knobas.external_adapter` into
//! [`Registry::for_process`]'s list (`sources::install::load_installed`), and an
//! install adds to it while the app runs. Every clone of a registry shares
//! its list, so the scheduler's copy sees an adapter installed after it
//! started.

use knobas_source::instance::SourceInstance;
use knobas_source::{Source, SourceDescriptor, SourceError};
use std::sync::{Arc, PoisonError, RwLock};

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
        doc: Some(knobas_source_mock::doc_content),
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

/// Turns stored configurations into live adapters.
#[derive(Debug, Default, Clone)]
pub struct Registry {
    /// External adapters, after every compiled-in row, shared by every clone.
    external: Arc<RwLock<External>>,
}

#[derive(Debug, Default)]
struct External {
    /// Never of a kind a row has, and one per kind: [`Registry::add`]
    /// refuses a taken kind.
    loaded: Vec<WasmAdapter>,
    /// Installed adapters refused at load, by kind, with the reason; a
    /// source of one fails to build with it.
    refused: Vec<(String, String)>,
    /// The last update check of each installed adapter, by kind (#1209):
    /// what the reader is told, which changes nothing until they act.
    updates: Vec<(String, super::installed::UpdateCheck)>,
}

/// An external adapter of a kind the registry already has.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("an adapter of kind {kind:?} is already {by}")]
pub struct KindTaken {
    pub kind: String,
    pub by: TakenBy,
}

/// What holds a taken kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TakenBy {
    /// A compiled-in row.
    BuiltIn,
    /// An external adapter already loaded or installed.
    Installed,
}

impl std::fmt::Display for TakenBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::BuiltIn => "built in",
            Self::Installed => "installed",
        })
    }
}

impl KindTaken {
    #[must_use]
    pub fn new(kind: &str, by: TakenBy) -> Self {
        Self {
            kind: kind.to_owned(),
            by,
        }
    }
}

impl Registry {
    /// The process's registry: the compiled-in table **and** every external
    /// adapter this process has loaded or installed. One list for the
    /// process, which bring-up fills and an install adds to; every call
    /// returns a clone sharing it.
    ///
    /// Bring-up and [`Lifecycle`](crate::Lifecycle)'s default are the only
    /// callers, and a scan test keeps it so (#1323). A read takes the lifecycle's
    /// registry as a trait object instead (`&dyn AdapterRegistry`, or
    /// `&Arc<dyn AdapterRegistry>` where it builds an adapter, #1705), so a
    /// test can give it one of its own.
    #[must_use]
    pub fn for_process() -> Registry {
        static PROCESS: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();
        PROCESS.get_or_init(Registry::default).clone()
    }

    /// The compiled-in table and `external`, minus any external adapter of
    /// a kind already taken: a row's kind, or an earlier adapter's. Each
    /// dropped one is logged. A list of its own, shared with nothing.
    #[must_use]
    pub fn with_external(external: Vec<WasmAdapter>) -> Registry {
        let registry = Registry::default();
        for adapter in external {
            if let Err(error) = registry.add(adapter) {
                tracing::error!(%error, "an external adapter is not loaded");
            }
        }
        registry
    }

    /// Whether `kind` is taken: by a compiled-in row or by an external
    /// adapter already loaded.
    ///
    /// # Errors
    ///
    /// [`KindTaken`] naming which.
    pub fn check_free(&self, kind: &str) -> Result<(), KindTaken> {
        Self::free_in(&self.read(), kind)
    }

    fn free_in(external: &External, kind: &str) -> Result<(), KindTaken> {
        if ADAPTERS.iter().any(|a| a.kind == kind) {
            return Err(KindTaken::new(kind, TakenBy::BuiltIn));
        }
        if external.loaded.iter().any(|a| a.kind() == kind) {
            return Err(KindTaken::new(kind, TakenBy::Installed));
        }
        Ok(())
    }

    /// Add an external adapter to every clone of this registry.
    ///
    /// # Errors
    ///
    /// [`KindTaken`] when a row or a loaded adapter has its kind.
    pub fn add(&self, adapter: WasmAdapter) -> Result<(), KindTaken> {
        // Checked under the write lock, so two adds of one kind cannot both pass.
        let mut external = self.write();
        Self::free_in(&external, adapter.kind())?;
        external.refused.retain(|(kind, _)| kind != adapter.kind());
        external.loaded.push(adapter);
        Ok(())
    }

    /// Put `adapter` in place of the loaded adapter of its kind, or add it
    /// (an update, #1209; a restore's reinstall, #1210). A built-in kind is
    /// still refused.
    ///
    /// # Errors
    ///
    /// [`KindTaken`] when a compiled-in row has its kind.
    pub fn replace(&self, adapter: WasmAdapter) -> Result<(), KindTaken> {
        let mut external = self.write();
        let kind = adapter.kind().to_owned();
        external.loaded.retain(|a| a.kind() != kind);
        Self::free_in(&external, &kind)?;
        external.refused.retain(|(k, _)| *k != kind);
        external.updates.retain(|(k, _)| *k != kind);
        external.loaded.push(adapter);
        Ok(())
    }

    /// Forget the external adapter of `kind`: loaded, refused, its update
    /// check (an uninstall, #1210).
    pub fn remove(&self, kind: &str) {
        let mut external = self.write();
        external.loaded.retain(|a| a.kind() != kind);
        external.refused.retain(|(k, _)| k != kind);
        external.updates.retain(|(k, _)| k != kind);
    }

    /// Why the installed adapter of `kind` was refused, if it was.
    #[must_use]
    pub fn refusal(&self, kind: &str) -> Option<String> {
        let external = self.read();
        external
            .refused
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, reason)| reason.clone())
    }

    /// The loaded external adapter of `kind`'s descriptor template.
    #[must_use]
    pub fn external_template(&self, kind: &str) -> Option<SourceDescriptor> {
        let external = self.read();
        external
            .loaded
            .iter()
            .find(|a| a.kind() == kind)
            .map(WasmAdapter::template)
    }

    /// Record the last update check of `kind`.
    pub fn set_update(&self, kind: &str, check: super::installed::UpdateCheck) {
        let mut external = self.write();
        external.updates.retain(|(k, _)| k != kind);
        external.updates.push((kind.to_owned(), check));
    }

    /// The last update check of `kind`, if one ran.
    #[must_use]
    pub fn update(&self, kind: &str) -> Option<super::installed::UpdateCheck> {
        let external = self.read();
        external
            .updates
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, check)| check.clone())
    }

    /// Record that the installed adapter of `kind` was refused at load, so a
    /// source of it fails to build with `reason` rather than as unknown.
    ///
    /// A loaded adapter of `kind` goes (#1632): the registry is the process's,
    /// so a retried bring-up or a restore's reinstall can refuse a kind an
    /// earlier load put there, and a kind both loaded and refused would go on
    /// building sources of the adapter Settings calls refused.
    pub fn refuse(&self, kind: &str, reason: String) {
        let mut external = self.write();
        external.loaded.retain(|a| a.kind() != kind);
        external.refused.retain(|(k, _)| k != kind);
        external.refused.push((kind.to_owned(), reason));
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, External> {
        self.external.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, External> {
        self.external
            .write()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// One descriptor **template** per compiled-in kind, then one per
    /// loaded external adapter.
    ///
    /// Also reachable as [`descriptors`](knobas_sync::scheduler::AdapterRegistry::descriptors), which is what a
    /// read takes.
    #[must_use]
    pub fn templates(&self) -> Vec<SourceDescriptor> {
        ADAPTERS
            .iter()
            .map(|a| (a.template)())
            .chain(self.read().loaded.iter().map(WasmAdapter::template))
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

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn doc_projection(&self, kind: &str) -> Option<knobas_source::docs::DocProjection> {
        Registry::doc_projection(self, kind)
    }

    fn doc_writes(&self, kind: &str) -> Option<knobas_source::docs::DocWrites> {
        Registry::doc_writes(self, kind)
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
        let external = self.read();
        if let Some(adapter) = external.loaded.iter().find(|a| a.kind() == instance.kind) {
            let adapter = adapter.clone();
            drop(external);
            return adapter.build(instance);
        }
        if let Some((_, reason)) = external.refused.iter().find(|(k, _)| *k == instance.kind) {
            return Err(SourceError::protocol(format!(
                "the external adapter {:?} was refused at load: {reason}",
                instance.kind
            )));
        }
        Err(SourceError::protocol(format!(
            "no adapter of kind {:?} is compiled in",
            instance.kind
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_external_adapter_joins_the_templates_once() {
        let fixture = knobas_wasm_host::load_fixture();
        let registry = Registry::with_external(vec![fixture.clone(), fixture]);
        let kinds: Vec<String> = registry.templates().into_iter().map(|t| t.id).collect();
        assert_eq!(kinds.iter().filter(|k| *k == "wasm-fixture").count(), 1);
        assert_eq!(kinds.len(), ADAPTERS.len() + 1);
    }

    fn fixture_instance() -> SourceInstance {
        SourceInstance {
            id: "surveys".to_owned(),
            kind: "wasm-fixture".to_owned(),
            display_name: "Harbour surveys".to_owned(),
            base_url: String::new(),
            auth: None,
            secret: None,
            account: None,
            config: serde_json::json!({}),
        }
    }

    /// The scheduler holds a clone made at bring-up; an install after that
    /// must still reach it.
    #[test]
    fn a_clone_made_before_an_install_builds_the_installed_adapter() {
        use knobas_sync::scheduler::AdapterRegistry as _;
        let registry = Registry::default();
        let scheduler_copy = registry.clone();
        registry.add(knobas_wasm_host::load_fixture()).unwrap();
        assert!(scheduler_copy.build(fixture_instance()).is_ok());
        assert_eq!(
            registry.add(knobas_wasm_host::load_fixture()),
            Err(KindTaken::new("wasm-fixture", TakenBy::Installed))
        );
    }

    #[test]
    fn a_source_of_an_adapter_refused_at_load_fails_with_the_reason() {
        use knobas_sync::scheduler::AdapterRegistry as _;
        let refusing = Registry::default();
        refusing.refuse("wasm-fixture", "its hash changed".to_owned());
        let Err(error) = refusing.build(fixture_instance()) else {
            panic!("a refused adapter must not build");
        };
        assert!(error.to_string().contains("its hash changed"), "{error}");
    }

    /// Bring-up's retry and a restore's reinstall both refuse through here,
    /// over a registry an earlier load may have filled (#1632).
    #[test]
    fn refusing_a_loaded_kind_unloads_it() {
        use knobas_sync::scheduler::AdapterRegistry as _;
        let registry = Registry::default();
        registry.add(knobas_wasm_host::load_fixture()).unwrap();
        registry.refuse("wasm-fixture", "its artifact is gone".to_owned());
        assert!(registry.external_template("wasm-fixture").is_none());
        let Err(error) = registry.build(fixture_instance()) else {
            panic!("a refused adapter must not build");
        };
        assert!(error.to_string().contains("its artifact is gone"), "{error}");
    }

    /// A read holding the registry as the trait object reaches the Docs
    /// projection and writes the concrete table answers (#1321).
    #[test]
    fn the_trait_object_answers_the_docs_projection_and_writes() {
        use knobas_sync::scheduler::AdapterRegistry;
        let registry: &dyn AdapterRegistry = &Registry::default();
        assert!(registry.doc_projection("confluence").is_some());
        assert!(registry.doc_writes("confluence").is_some());
        assert!(registry.doc_projection("jira").is_none());
        assert!(registry.doc_writes("mock").is_none());
    }

    #[test]
    fn a_built_in_kind_is_taken() {
        assert_eq!(
            Registry::default().check_free("jira"),
            Err(KindTaken::new("jira", TakenBy::BuiltIn))
        );
    }

    /// **Only bring-up and the lifecycle's default reach the process registry**
    /// (#1305, #1323). Every other read takes the registry it is given as a
    /// trait object, which is what lets a test hand it one of its own; a
    /// third direct call is the second route growing back.
    ///
    /// Scans the non-test source of the app crate: every item gated by
    /// `#[cfg(test)]` is skipped (it ends at the first line at the attribute's
    /// own indent that ends in `;` or is a closing brace, which is where
    /// rustfmt puts it), so a gated `use` near the top of a file or a test
    /// module in its middle hides nothing after it. Comment lines and the
    /// definition itself are left out too. A
    /// unit test's fixture and the integration tests under `tests/` build the
    /// process registry as a fixture and are not the route this pins (the
    /// command-module scan in `commands/mod.rs` likewise reads `src/commands`
    /// only). The allow-list is a count per file, so a second call beside an
    /// allowed one is caught too.
    #[test]
    fn only_bring_up_and_the_lifecycles_default_call_for_process() {
        const ALLOWED: [(&str, &str); 2] = [
            ("lib.rs", "bring-up (`spawn_bring_up`)"),
            (
                "commands/app.rs",
                "the lifecycle's default (`Lifecycle::new`)",
            ),
        ];
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("a source directory is readable") {
                let path = entry.expect("a directory entry").path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|extension| extension == "rs") {
                    out.push(path);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&root, &mut files);
        assert!(
            files.len() > 50,
            "the scan found only {} files",
            files.len()
        );

        let needle = ["for_process", "()"].concat();
        let mut hits: Vec<(String, usize)> = Vec::new();
        for path in files {
            let relative = path
                .strip_prefix(&root)
                .expect("under src")
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(&path).expect("a source file is readable");
            let mut lines = text.lines().enumerate();
            while let Some((at, raw)) = lines.next() {
                let line = raw.trim_start();
                if line.starts_with("#[cfg(test)]") {
                    let indent = &raw[..raw.len() - line.len()];
                    for (_, item) in lines.by_ref() {
                        let Some(rest) = item.strip_prefix(indent) else {
                            continue;
                        };
                        let rest = rest.trim_end();
                        if !rest.starts_with(char::is_whitespace)
                            && (rest.ends_with(';') || rest == "}")
                        {
                            break;
                        }
                    }
                    continue;
                }
                if line.starts_with("//") || line.contains("fn for_process") {
                    continue;
                }
                for _ in line.matches(needle.as_str()) {
                    hits.push((relative.clone(), at + 1));
                }
            }
        }

        let mut expected: Vec<String> =
            ALLOWED.iter().map(|(file, _)| (*file).to_owned()).collect();
        let mut found: Vec<String> = hits.iter().map(|(file, _)| file.clone()).collect();
        expected.sort();
        found.sort();
        let sites = ALLOWED
            .iter()
            .map(|(file, what)| format!("{file}: {what}"))
            .collect::<Vec<_>>()
            .join("; ");
        let at = hits
            .iter()
            .map(|(file, line)| format!("{file}:{line}"))
            .collect::<Vec<_>>()
            .join(", ");
        assert_eq!(
            found, expected,
            "Registry::for_process() may be called from exactly two places: {sites}. \
             Every other read takes the registry it is given (a trait object, from \
             the lifecycle or the sources state). Found at: {at}"
        );
    }
}
