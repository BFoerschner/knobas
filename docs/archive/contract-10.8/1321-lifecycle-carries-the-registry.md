- **`crates/knobas-app/src/sources/registry.rs` and the `AdapterRegistry` trait: the singleton
  accessor is renamed and the trait answers the Docs projection and writes, issue #1321
  (2026-10-19).** Parent spec #1305, ADR-0028, the first of its three tickets. Booked against the
  ownership line that makes `registry.rs` F-owned and **append-only, one row per adapter**: this
  entry changes that file outside its table. No row is added, removed or edited. No migration, no
  event, no `WriteOp`, nothing under `crates/knobas-source/src/**`, no command added, removed or
  renamed, no argument the frontend sends changed, no DTO field changed, no module added to either
  barrel or to `commands/`. Ratified by the implementer on Opus under Björn's 2026-10-18
  reserved-steps rule; the rulings are in `docs/archive/2026-10-unattended-rulings.md` under
  `## #1321`.

  **The rename.** `Registry::builtin()` becomes `Registry::for_process()`. It returns the same
  list as before: the compiled-in table and every external adapter this process has loaded or
  installed, shared by every clone. Its doc now says so and names its two intended callers:
  bring-up (`sources::start`, which loads the installed adapters into it) and
  `Lifecycle::new()`. The other call sites are renamed in place. #1322 and #1323 move them to the
  lifecycle's registry.

  **The trait growth.** `knobas_sync::scheduler::AdapterRegistry` gains two provided methods
  beside `descriptors()` and `build()`:

  | method | answers | default |
  | --- | --- | --- |
  | `doc_projection(&self, kind: &str) -> Option<knobas_source::docs::DocProjection>` | the Docs projection of an adapter kind (#859) | `None` |
  | `doc_writes(&self, kind: &str) -> Option<knobas_source::docs::DocWrites>` | the Docs writes of an adapter kind (#862) | `None` |

  `Registry` overrides both with its inherent methods of the same name. The templates (kinds,
  declared paths, roles, write ops per adapter kind) were already `descriptors()`, which
  `Registry` answers with `templates()`. Its doc now says it covers the installed adapters too.
  A registry that draws no pages, such as every test registry, implements neither method.

  **The lifecycle carries the registry.** `Lifecycle` holds an `Arc<dyn AdapterRegistry>`.
  `Lifecycle::new()` holds `Registry::for_process()`, the list bring-up fills and
  `SourcesState::registry` shares. `Lifecycle::with_registry(Arc<dyn AdapterRegistry>)` gives a
  test a lifecycle over a registry it built, and `Lifecycle::registry()` returns it.

  **One command reads it.** `list_adapters` now takes `State<'_, Lifecycle>` and answers
  `lifecycle.registry().descriptors()`. `Lifecycle` is managed synchronously at the start of
  `setup`, before anything can call in, so the command still answers before bring-up. Its name, its arguments from the frontend (none) and
  its `Vec<SourceDescriptor>` are unchanged, and `app/src/lib/ipc/sources.ts` is untouched.
