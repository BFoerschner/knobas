- **`crates/knobas-source/src/**` and the IPC schema — issue #1379 (2026-10-20): the run reports
  its walk, and `SyncProgress` carries the reading.** Parent spec #1378, the first of its split:
  SPI, engine, IPC, wizard and Gitea in one PR. The WIT `sink` (`knobas:adapter@1.3.0`) and the
  other adapters are #1380–#1384, and build on this surface unchanged. No migration, no event, no
  command added, removed or renamed, no argument changed, no `WriteOp`, nothing under
  `crates/knobas-http/**`, no new module in `commands/` or `ipc/`. Reserved for Björn, ruled by the
  implementer on Opus: the rulings are in `docs/decisions/2026-10-unattended-rulings.md` under
  `## #1379`.

  **`crates/knobas-source/src/**` — additive:** `Sink` gains one method with a default body that
  does nothing.

  ```rust
  async fn walked(&mut self, done: u64, of: Option<u64>, unit: &str) {}
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
