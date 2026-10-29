- **`crates/knobas-wasm-host/wit/adapter.wit`, the WIT world `knobas:adapter@1.0.0`, joins the
  frozen list — issue #1198 (2026-10-18): the interface an external adapter implements.** No
  migration, no new command, no `commands/` or `ipc/` module, no change to
  `crates/knobas-source/src/**` (no `WriteOp`, no `KindPaths` field, no battery clause), no change
  to `crates/knobas-app/src/error.rs`. Parent spec #1192 (ADR-0028, glossary **External
  adapter**, **Adapter standard**). Ratified by the implementer on Opus under Björn's 2026-10-18
  reserved-steps rule; the ruling is in `docs/archive/2026-10-unattended-rulings.md`.

  **What is frozen.** The file, from 1.0.0 on. A knobas release hosts one major version. A minor
  version only adds (a new `knobas:adapter` interface the host provides or reads; WASI 0.2's
  own interfaces beside the world, `wasi:http` among them since #1203, are not `knobas:adapter`'s, a new function on `adapter`
  that the host looks up as optional, a new JSON shape inside a `json` value), and each minor is
  a §10.8 entry of its own, as this one is. **A new case in a WIT variant or a new field in a WIT
  record is not additive**: wasmtime type-checks a component's functions against the host's
  bindings exactly (case count and names), so a component built against the old type no longer
  links. Such a change is a new major version, or it travels as JSON. Changing or removing
  anything in it is a new major version, which every external adapter has to be rebuilt for.

  **The world.** `world adapter { import sink; import log; export source; }`.
  - `source` exports the resource `adapter`, constructed from an `instance` record (id, kind,
    display name, base URL, config as JSON; **no secret and no account**: ADR-0028), with one
    function per `Source` method in the trait's order: `descriptor`, `test-connection`, `sync`,
    `write`, `reachable-transitions`, `view`, `pipeline-settings`, `older-runs`, `run-chain`,
    `run-log`, `run-tests`, `test-history`, `run-artifacts`, `artifact-preview`. `download` is not
    in 1.0.0: its stream is host-owned and arrives with #1205 as a minor version.
  - `sink.item(sync-item) -> result<_, source-error>` is `Sink::item`.
  - `log.log(level, message)` writes into knobas' log, tagged with the source id.
  - Beyond these the component gets WASI 0.2 with nothing in it (no preopened directory, no
    environment, no argument, null stdio; the socket and name-lookup checks refuse everything).
    The HTTP allowlist and credential injection are #1203.

  **How the SPI's types cross.** WIT records and variants for the types the host branches on:
  `source-error` (variant for variant with `SourceError`, statuses included), `instance`,
  `sync-item` (timestamps RFC 3339, `payload` as JSON), `connection-info`, `write-receipt`, and
  `run-chain-step` (`RunChainStep` has no serde form). Every other SPI type crosses as its serde
  JSON (`type json = string`), the exact form `serde_json` writes the `knobas-source` type in:
  `SourceDescriptor`, `WriteOp`, `view::SourceView`, `pipeline::PipelineSettings`, `run::RunLog`,
  `run::RunTests`, `run::TestOutcome` lists, `run::ArtifactEntry` lists, `run::ArtifactPreview`.
  That serde form is therefore frozen *through* this entry as well: a serde rename in
  `knobas-source` is now also a change to the interface every external adapter speaks. An SPI
  addition that is `#[serde(default)]` (the `roles` and `payload_paths` precedent) stays additive
  for a component; #1194's `WriteOp::SourceAction` reaches a component as one more JSON shape.

  **Versions at load.** `knobas_wasm_host::WasmAdapter::load` reads the version of the
  `knobas:adapter/source` export and refuses, before linking, a component of another major
  version or of a newer minor than the host's (`INTERFACE_VERSION`), and one that exports no
  source at all.

  **One store per call.** Every call instantiates the component afresh; an adapter keeps no state
  between calls but its cursor. The descriptor is read once, when the registry builds the source.

  **Registry.** `knobas_app::sources::Registry` gains external adapters beside the compiled-in
  rows (`Registry::with_external`); a kind a row already has is dropped. In a debug build
  `KNOBAS_DEV_ADAPTER=<file.wasm>` loads one dev adapter at the first `Registry::builtin()`.
  Install (#1202) replaces the variable with a knobas-owned table.

  **Build.** `rust-toolchain.toml` lists the `wasm32-wasip2` target; `just wasm-fixture` builds
  the fixture component (`crates/knobas-wasm-fixture`), and `just test` runs it first.
