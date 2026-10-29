- **`crates/knobas-source/src/**` (one `Sink` method) and the engine's sweep — issue #2186
  (2026-10-26): a run handed a position reconciles a kind it walked whole.** Follows #2181, whose
  adapters walk a changed scope in full under a stored cursor. No migration, no event, no command
  added, removed or renamed, no argument or IPC field changed, no `WriteOp`, nothing under
  `crates/knobas-http/**`, no new module in `commands/` or `ipc/`, no WIT change, no cursor
  version moved. The rulings are in `docs/archive/2026-10-unattended-rulings.md` under `## #2186`.

  **`crates/knobas-source/src/**` — additive:** `Sink` gains one method with a default body that
  does nothing.

  ```rust
  async fn walked_whole(&mut self, kind: &str) { let _ = kind; }
  ```

  An adapter calls it to say that this run emitted every item of `kind` its current scope holds,
  as a cursor-less run would. It returns nothing, reads nothing (the sink stays write-only,
  ADR-0003) and cannot fail a sync. `VecSink`, `FailingSink` and the battery are unchanged;
  `contract::Walking` keeps the kinds in a new `whole` field, and the engine's `PgSink` keeps them
  for the sweep. A sink that wraps another forwards it; the engine's progress decorator does.

  **The engine's sweep:** a kind is swept when the run was cursor-less **or** the adapter said it
  walked that kind whole, and, as before, the kind declares `full_sync_exhaustive`, the run emitted
  something of it, and the run is not a backfill. `SyncReport::swept` can therefore be non-zero
  for a run handed a cursor. Witnessed by `knobas-sync`'s `tests/it/sweep.rs` (three tests) and
  `tests/it/progress.rs`.

  **Who says it.** Jira: `ticket`, after a walk from no watermark (a cursor-less run, an unreadable
  cursor, or one under another `projects` / `jql_filter` scope, #2181). Confluence: `page`, after a
  walk from no watermark (the same, under another space list). Gitea: `repo` and `branch`, after a
  run it makes a full sync, which now includes a run whose position holds a repository `owners[]` or
  `repos[]` no longer admits. TeamCity says nothing: its kinds are not exhaustive. A WASM component
  cannot say it (the WIT `sink` has no such function), so its runs stay as before.
