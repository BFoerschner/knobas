- **IPC event and command semantics, issue #1636 (2026-10-23): a second launch on a profile a live
  knobas has open ends bring-up in `DbState::Failed`, and the message names the profile and the
  owning process.** Spec #1619, rulings B2, B5 and B6, under Björn's 2026-10-22 waiver for this
  spec's frozen-surface steps. No signature change, no new variant, no migration, no new module in
  `commands/` or `ipc/`. The generated mirror does not change.

  **What changes on the wire.** `db:state` and `app_status` used to answer `ready` for a second
  launch on a live owner: it borrowed the owner's server, and the owner's quit stopped the server
  under it. They now answer `failed`, and `message` is `knobas_db::DbError::AlreadyRunning`'s text
  with the profile in front, for example `demo profile: the database at <data dir> is in use:
  knobas process 4242 has this profile open; quit that window first`. `AlreadyRunning`'s text was
  `another knobas instance is using the database at <data dir> (<reason>)` and is now `the database
  at <data dir> is in use: <reason>`. Every other failure's message is unchanged. The boot screen
  shows the message as text, as before.

  **Who gets it.** A second `--demo` launch, which the single-instance guard leaves through (B6),
  and any other launch that reaches the embedded lifecycle while a live process holds the profile's
  `.owner.lock`. A lock whose recorded process is gone and whose port answers with a PostgreSQL
  that refuses our password is now a stale lock, cleared like the others (B5). While the recorded
  process is alive it is still `AlreadyRunning`, and the reason says to delete `postmaster.pid` if
  no knobas has the profile open.

  Witnessed by `crates/knobas-app/tests/it/bring_up.rs`
  (`a_second_demo_launch_on_a_live_owner_is_refused_naming_the_profile_and_the_owner`) and
  `crates/knobas-db/tests/embedded.rs` (the refusal, race, orphan and squatter tests).
