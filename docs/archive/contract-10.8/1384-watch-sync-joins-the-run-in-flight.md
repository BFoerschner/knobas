- **One new command, `watch_sync` — issue #1384 (2026-10-20): the source's settings page joins
  the run in flight.** Parent spec #1378. Per-item progress travels on a channel and nowhere else
  (P3), and only a caller that started or joined a run had one, so a page opened on a scheduled
  run could only disable *Sync now*. No migration, no event, no DTO, no argument changed on any
  existing command, no new module in `commands/` or `ipc/`, nothing under
  `crates/knobas-source/src/**` or `crates/knobas-http/**`, no change to `error.rs` or
  `profile.rs`. Reserved for Björn, ruled by the implementer on Opus (Björn, 2026-10-20: Opus may
  amend without him); the ruling is in `docs/archive/2026-10-unattended-rulings.md` under
  `## #1384`.

  **The command**, in `crates/knobas-app/src/commands/sources.rs`, appended at the foot of
  `lib.rs`'s handler list:

  ```rust
  pub async fn watch_sync<R: tauri::Runtime>(
      app: tauri::AppHandle<R>,
      source_id: String,
      progress: tauri::ipc::Channel<knobas_sync::SyncProgress>,
  ) -> Result<Option<i64>, IpcError>
  ```

  It enrols the channel in the source's run in flight, the join `sync_now_with_progress`'s second
  call makes (`Watchers::attach`, under the scheduler's `runs` lock, through the new
  `Scheduler::watch`), and answers that run's id; the channel then hears the run's remaining
  progress and its ending. On an idle source, an unknown one included, it answers `null`, starts
  nothing and sends nothing; a run that ended before the call is idle. It never spends the
  first-sync claim. There is no detach: the channel stays enrolled until the run ends, and a page
  that leaves stops listening. Errors: `not_ready` while the engine is starting or stopping.

  **TypeScript:** `watchSync(sourceId, progress: Channel<SyncProgress>): Promise<number | null>` in
  `app/src/lib/ipc/sources.ts`, re-exported by the `index.ts` barrel's existing
  `export * from "./sources"`. No generated type changes. The fake IPC answers `null`: no fixture
  source is running. Witnessed by `knobas-app`'s
  `tests/it/ipc.rs::watch_sync_joins_the_run_in_flight_and_hears_its_ending` and the registry
  check in the same file, and by `TfSettings.test.svelte.ts`.
