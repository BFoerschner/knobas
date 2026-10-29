- **`crates/knobas-source/src/**` and the IPC schema — issue #1379 (2026-10-20): the run reports
  its walk, and `SyncProgress` carries the reading.** Parent spec #1378, the first of its split:
  SPI, engine, IPC, wizard and Gitea in one PR. The WIT `sink` (`knobas:adapter@1.3.0`) and the
  other adapters are #1380–#1384, and build on this surface unchanged. No migration, no event, no
  command added, removed or renamed, no argument changed, no `WriteOp`, nothing under
  `crates/knobas-http/**`, no new module in `commands/` or `ipc/`. Reserved for Björn, ruled by the
  implementer on Opus: the rulings are in `docs/archive/2026-10-unattended-rulings.md` under
  `## #1379`.

  **`crates/knobas-source/src/**` — additive:** `Sink` gains one method with a default body that
  does nothing.

  ```rust
  async fn walked(&mut self, done: u64, of: Option<u64>, unit: &str) { let _ = (done, of, unit); }
  ```

  An adapter calls it as its outer walk advances, before each unit and once after the last: `done`
  units of `of` (`None` when the total is unknown), `unit` a plural noun the UI prints as is. It
  returns nothing and cannot fail a sync. `VecSink`, `FailingSink`, the battery and every
  existing `Sink` are unchanged. A sink that wraps another forwards it; the engine's progress
  decorator does.

  **The IPC schema — additive:** `knobas_sync::progress::SyncProgress` gains three fields after
  `message`.

  ```rust
  #[serde(default, skip_serializing_if = "Option::is_none")] #[ts(optional)] pub walked: Option<u64>,
  #[serde(default, skip_serializing_if = "Option::is_none")] #[ts(optional)] pub of: Option<u64>,
  #[serde(default, skip_serializing_if = "Option::is_none")] #[ts(optional)] pub unit: Option<String>,
  ```

  Absent on the wire, not `null`, when the adapter never reported a walk; the scheduler's own
  messages (`started`, and the `finished`/`failed` ending built by `ending`) never carry one. The
  generated `app/src/lib/ipc/generated/sources.ts` gains `walked?: number`, `of?: number`,
  `unit?: string`, so no hand-written `SyncProgress` literal changes. The engine's `Counting`
  keeps the latest reading, puts it on every message it sends, and sends a message for a reading
  on the item path's 250 ms clock (the first at once); the final `fetching` sample and the
  `writing` one carry the last reading. Witnessed by `knobas-sync`'s `tests/it/progress.rs`, the
  Gitea crate's `tests/it/sync.rs` and `tests/live_gitea.rs`, the mock's `tests/it/contract.rs`,
  and `FirstRun.test.svelte.ts`.

  **Who reports today.** Gitea: `repositories`, of the selection `select_repos` made. The mock:
  `items`, of its fixture (the tombstone included when it is on). Every other adapter, and every
  WASM component, reports nothing until its own ticket.

  **Amendment (#1380, 2026-10-20): the WIT minor, `knobas:adapter@1.3.0`.** Additive in
  `crates/knobas-wasm-host/wit/adapter.wit`, per ADR-0028. The package moves from 1.2.0 to 1.3.0,
  and the `sink` interface gains one function, `Sink::walked` with the same meaning:

  ```wit
  walked: func(done: u64, of: option<u64>, unit: string);
  ```

  It returns nothing and cannot fail a sync. The host hands each reading to the engine's sink in
  order with the items; one called outside a sync, or after the sync's sink refused an item, is
  dropped. `INTERFACE_VERSION` is 1.3.0, so the host hosts every 1.x up to 1.3.0 and install and
  load still refuse a component or manifest above it (the refusal now reads "up to 1.3.0"). A
  component built against 1.0.0 to 1.2.0 imports `sink` without `walked`, links against the 1.3.0
  `sink` through wasmtime's semver-compatible lookup, and reports nothing. The fixture adapter's
  1.3.0 build reports `items`, of the items the run hands, before each and once after the last;
  its 1.0.0 build reports nothing. Witnessed by `knobas-wasm-host`'s `tests/it/walk.rs`, which
  installs each build under a manifest and syncs it through the engine's progress decorator, and
  syncs the 1.3.0 build into a sink that refuses its second item, which hears no reading after the
  refusal. The link of an earlier component is witnessed for 1.0.0, the one older build the fixture
  keeps; 1.1.0 and 1.2.0 add no import that 1.0.0 lacks on `sink`, so they link the same way. No
  migration, no event, no command, no IPC field, no new module in `commands/` or `ipc/`. This
  supersedes *Who reports today* above for a WASM component: one built against 1.3.0 reports
  what it calls `walked` with.
