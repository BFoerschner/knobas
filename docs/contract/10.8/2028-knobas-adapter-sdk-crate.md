- **`knobas-adapter-sdk`, the adapter standard's Rust SDK, binds `knobas:adapter@1.3.0` unchanged —
  issue #2028 (2026-10-24).** No WIT change (`crates/knobas-wasm-host/wit/adapter.wit` keeps
  1.3.0, byte for byte), no migration, no new command, no `commands/` or `ipc/` module, no change
  to `crates/knobas-source/src/**` (no `WriteOp`, no `KindPaths` field, no battery clause), no
  change to `crates/knobas-http/**` or `error.rs`. Parent spec #1192 (story 40, ADR-0028 *The
  checks*, glossary **Adapter standard**). Recorded because the crate is the guest half of a frozen
  surface. Ratified by the implementer on Opus; the rulings are in
  `docs/decisions/2026-10-unattended-rulings.md`.

  **What the crate is.** `crates/knobas-adapter-sdk` generates the guest bindings of `world
  adapter` (wit-bindgen 0.62.0) and keeps them `#[doc(hidden)]`. An adapter implements one trait,
  `knobas_adapter_sdk::Adapter`: `Source`'s methods in the trait's order but `undeclared_reason`
  (the host's to word, for every guest), synchronous, over the SPI types, with `new(Instance)` for
  the resource's constructor and `Source`'s defaults for the optional reads (`translate_query` and `download` included, though they are the `query` and
  `downloads` exports). `knobas_adapter_sdk::export!(Type)` exports it. The crate re-exports
  `knobas-source` as `spi`, and gives `Instance` (the WIT `instance` with its config as a
  `serde_json::Value`), `Sink` (`item`, `walked`), `DownloadSink` (`start`, `write`) and `log`.
  Every conversion is its own: each `json` crosses as `serde_json` writes the SPI type, each record
  and variant as `knobas-wasm-host`'s `convert` decodes it, a timestamp as RFC 3339 in UTC with its
  fraction kept.

  **Its version is the interface version.** The crate does not take the workspace version:
  `version = "1.3.0"`, the WIT package's, and `INTERFACE_VERSION` says the same.
  `the_crate_version_is_the_interface_version_it_binds` fails when they differ. **A future WIT
  minor bumps the SDK's version in the same PR**, and its own §10.8 entry says so. The SDK may
  carry a patch of its own (a conversion fix) without a WIT change.

  **The frozen 1.0.0 world moved.** `crates/knobas-wasm-fixture/wit-1.0.0/adapter.wit` is now
  `crates/knobas-adapter-sdk/wit-1.0.0/adapter.wit`, unchanged. The SDK's `v1_0` feature builds
  against it. Under it, `Adapter` has no `translate_query` or `download`, there is no
  `DownloadSink`, and `Sink::walked` reports nothing. The fixture's `v1_0` turns it on, and the
  host's hold on 1.0.0 components (#1205) is still tested with that build.

  **The fixture is its first user.** `crates/knobas-wasm-fixture` implements `Adapter` and holds
  no bindings, no `wit_bindgen` dependency and no WIT conversion. Nothing in
  `crates/knobas-wasm-host/tests/it/` changed. For everything a knobas host sends, what crosses
  is the same. Only input a host never sends answers differently: a config that is not JSON reads
  as `null`, and a write op, saved view or download target that does not decode is refused by the
  SDK (`the host's <what> does not decode: …`) before the fixture sees it. Timestamps are written
  with their fraction (`AutoSi`), which for the fixture's whole-second instants is the same text
  as before.
