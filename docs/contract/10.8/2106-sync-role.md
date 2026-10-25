- **One new command, `sync_role` — issue #2106 (2026-10-25): *Sync now* in every role view's
  header.** Parent spec #2085, story 32 to 36. No migration, no event, no change to an existing
  command or DTO, no new module in `commands/` or `ipc/`, nothing under
  `crates/knobas-source/src/**` or `crates/knobas-http/**`, no change to `error.rs` or
  `profile.rs`.

  **The command**, in `crates/knobas-app/src/commands/sources.rs` beside `sync_all`, appended at
  the foot of `lib.rs`'s handler list:

  ```rust
  pub async fn sync_role<R: tauri::Runtime>(
      app: tauri::AppHandle<R>,
      role: String,
  ) -> Result<Vec<crate::sources::RoleRun>, IpcError>
  ```

  It starts one incremental run (`SyncTrigger::Manual`) of each **enabled** source that fills
  `role`, as `sources::crud::fillers` answers it (the mirror's "which sources fill this role",
  never the surface's), and answers them in id order. A source already running is joined, as
  `sync_now` joins it, and its run in flight comes back. A turned-off filler is skipped; a role
  nobody fills, or whose fillers are all turned off, answers `[]`. Like `sync_now` it returns as
  soon as the runs are recorded, and each run ends on `sync:state` under its id (ADR-0005).
  Errors: `not_ready` while the engine is starting or stopping, and when the scheduler is
  shutting down mid-ask; `internal` when the database read of the role's fillers fails.

  **One new DTO**, `crate::sources::RoleRun { source_id: String, run_id: i64 }`, in the generated
  `app/src/lib/ipc/generated/sources.ts` (`ipc_mirror`).

  **TypeScript:** `syncRole(role): Promise<RoleRun[]>` and the re-export of `RoleRun` in
  `app/src/lib/ipc/sources.ts`. The `?fake-ipc` fixture answers it from its sources
  (`app/src/lib/shell/dev/fake/sources.ts`), one run per enabled fixture source that fills the
  role, and ends each on `sync:state` a moment later; its answer type is in
  `app/src/lib/shell/dev/fake-answers.ts`. Witnessed by `knobas-app`'s
  `tests/it/ipc.rs::sync_role_runs_each_enabled_filler_and_joins_a_run_in_flight` and the registry
  check in the same file, and by `role-sync.test.ts` and `TfShell.test.svelte.ts`.
