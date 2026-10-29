- **The IPC schema — issue #1585 (2026-10-21): `AppStatus` gains `secrets_in_memory`.** Under
  `KNOBAS_SECRET_STORE=memory` (the headless-QA store) a source's page said its token was "Stored
  in the macOS keychain", picked from the user agent, though the credential lived in the process's
  memory and went at exit. No new command, no argument changed, no migration, no event, no
  `WriteOp`, nothing under `crates/knobas-source/src/**`, no `commands/` or `ipc/` module, nothing
  added to either append-only barrel, nothing in `error.rs` or `profile.rs`. Ruled by the
  implementer on Opus; the ruling is in `docs/archive/2026-10-unattended-rulings.md` under
  `## #1585`.

  **The IPC schema.** `app_status`'s `AppStatus` gains `secrets_in_memory: bool`, additive: `true`
  when the process keeps credentials in `knobas_secrets::MemoryStore`, `false` for the OS keychain.
  It is read from `knobas_app::sources::SecretStoreKind`, managed in `setup` beside `Profile` and
  decided once from the environment; the sources bring-up builds its store from the same managed
  value, so what is reported and what is used cannot differ. `app_status` still takes no
  `State<'_, AppState>` (§2.1). Generated into `app/src/lib/ipc/generated/app.ts` (§2.6 as amended
  by #1325); witnessed by `knobas-app`'s `tests/it/ipc.rs` (`app_status_says_when_credentials_live_in_memory`,
  and `false` in `app_status_answers_before_the_database_does`).
