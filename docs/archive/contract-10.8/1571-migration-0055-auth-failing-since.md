- **Migration `0055`: one column and one trigger on `knobas.source_config` — issue #1571
  (2026-10-21): when a source's current run of failed sign-ins began.** No IPC change of its own (the
  inbox category that reads it is the separate entry
  `1571-failed-sign-in-inbox-category.md`), no command, no event, nothing in
  `crates/knobas-source/src/**`, `crates/knobas-http/**`, `error.rs` or `profile.rs`, nothing added
  to either append-only barrel or to `commands/` / `ipc/`. Ratified by the implementer on Opus under
  Björn's 2026-10-18 reserved-steps rule; the rulings are in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1571`.

  **The migration** (`0055_when_a_sign_in_started_failing.sql`). `auth_failing_since timestamptz`,
  nullable, on `knobas.source_config`, and the plpgsql function `knobas.keep_auth_failing_since()`
  behind the trigger `source_config_auth_failing_since` (`before insert or update of auth_state`,
  per row). The column is null while the state is anything but `unauthorized` or
  `missing_secret`. It is set to `now()` by the first failing verdict after one that did not fail,
  or by a new row that starts out failing. A failing verdict after a failing one keeps it, and that
  includes `unauthorized` turning into `missing_secret`. Existing failing rows are backfilled with
  `coalesce(auth_checked_at, now())`. `knobas_sync::config::set_health` and `clear_unreachable`
  stay unchanged: the trigger keeps the column whoever writes `auth_state`. `CredentialHealth`
  does not carry it, so the `sources` barrel is untouched.

  **Backups.** `source_config` is in the `knobas` schema, so an archive carries the column. An
  archive taken before `0055` restores with the column null, because a `COPY` names its columns and
  a column it leaves out takes its default. The restore's own `missing_secret` verdict then fires
  the trigger and starts the episode. A data-only load runs with `--disable-triggers`, so an archive's own value is
  kept as written. `RENAMED_COLUMNS` needs no row.

  Pinned by `crates/knobas-db/tests/schema.rs`'
  `zero_fifty_five_keeps_the_start_of_an_episode_of_failed_sign_ins` (start, keep through a retry and
  through a missing secret, end on `unreachable` and on `ok`, a later failure starts a new one) and
  `zero_fifty_five_dates_an_episode_already_under_way_at_its_last_check` (the backfill, with an `ok`
  row as its negative).
