- **The IPC schema and `crates/knobas-app/src/profile.rs`, issue #38 (2026-08-28):** the scheduled
  backup export adds four commands — `backup_status`, `backup_now`, `set_backup_schedule`,
  `restore_backup` — with `crates/knobas-app/src/commands/backup.rs` owning them and
  `app/src/lib/ipc/backup.ts` mirroring them by hand, appended to both barrels. `Profile` gains
  `backup_dir()` (`<profile>/backups`), which is the whole of the change to that file: archives have
  to live inside the profile or a demo run writes into the real profile's backups (P13), and the
  profile is the only thing that knows where that is. Additive on every axis — no existing command,
  DTO field, event name or profile method changes — and the `commands/` + `ipc/` layout is followed
  rather than altered: one module per feature, the barrels append-only, the mirror pinned to the Rust
  by `include_str!` tests in `commands/backup.rs` so a field or a command name added on one side only
  fails `cargo test`. No migration: the schedule and the last run live in `knobas.setting`, which
  migration `0002` comment 6 already names "later the export schedule" as a reason for. **`0004` is
  still the next free migration number.** Ratified by the orchestrator as issue #38 itself, which
  specifies the feature and its ratified defaults; the settings surface §14 asks for is split to
  issue **#69** and is not in this change.

