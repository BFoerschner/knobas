- **Migration `0021` and a sixteenth and seventeenth `assets` command, issue #443 (2026-09-07):**
  one sample per poll per monitor, a retention setting and the response-time threshold that makes
  *warn*. **Ratified in advance by the spec (#427) Björn approved** — "Migrations from the next free
  number: asset, route (M4.0); **sample**, alert (M4.1)" and "**Settings keys for the sample
  retention, the response-time threshold**, and the alert notification category" — and by the #428
  entry above, whose module-pair paragraph names the M4.1 commands among the ones that belong there.
  Written with the implementing PR, per #428's, #431's, #434's, #435's and #439's pattern.

  **`0021` is claimed here; `0022` is the next free number**, and #444's alert table is what is
  expected to take it. This supersedes the sentence in the #439 entry above that left `0021` free;
  the old sentence is left as history rather than rewritten, the treatment #53, #278, #428 and #439
  give the sentences they supersede.

  **The migration.** `0021_a_sample_per_poll_per_monitor.sql` adds one table and edits nothing.

  ```sql
  create table knobas.monitor_sample (
    id               bigint generated always as identity primary key,
    entity_id        text not null references knobas.entity(id) on delete cascade,
    taken_at         timestamptz not null default now(),
    state            text,
    response_time_ms integer,
    constraint monitor_sample_state_chk
      check (state is null or state in ('up', 'down', 'warn', 'pending', 'maintenance')),
    constraint monitor_sample_response_time_chk
      check (response_time_ms is null or response_time_ms >= 0)
  );
  create index monitor_sample_entity_idx on knobas.monitor_sample (entity_id, taken_at desc);
  create index monitor_sample_taken_idx  on knobas.monitor_sample (taken_at);
  ```

  **`monitor_sample` and not `sample`**, although the spec's word is *sample*: the schema already
  holds `heartbeat`, which is a beat of the *app*, and a bare `sample` beside it would be a table
  whose name does not say what it is a sample of. `CONTEXT.md` gains the glossary term **Sample**,
  which is the word people use; the table is the one place the qualification is worth paying for.

  **In the `knobas` schema, and that is the whole of "in the backup, out of the share export".** A
  backup is `pg_dump --schema=knobas` and a share export is that dump restricted to a table list per
  part (`knobas_app::backup::share`), so both halves of spec #427's sentence are properties of
  *where this table is* rather than rules anyone can forget to apply. `share.rs`'s
  `nothing_carries_the_activity_stream_the_queue_or_the_mirror` names the table so the second half
  stops being silent.

  **The two commands, and the DTO.**

  ```rust
  #[tauri::command] pub async fn monitoring_settings(..) -> Result<assets::MonitoringSettings, IpcError>;
  #[tauri::command] pub async fn set_monitoring_settings(.., settings: MonitoringSettings)
      -> Result<MonitoringSettings, IpcError>;

  pub struct MonitoringSettings { pub sample_retention_days: u32, pub response_time_warn_ms: u32 }
  ```

  Mirrored in `app/src/lib/ipc/assets.ts` as `monitoringSettings()`, `setMonitoringSettings(settings)`
  and `interface MonitoringSettings`. **One DTO and one transaction**: the section saves both
  numbers on one button and `knobas_sync::samples::set_settings` writes both rows inside one
  transaction, so the half-saved state the single DTO exists to rule out is ruled out on the
  database side too. **Two lines appended** at the foot of
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, under #439's; neither barrel is
  rewritten (`app/src/lib/ipc/index.ts` already re-exports `./assets` wholesale) and the `commands/`
  + `ipc/` module **layout is untouched** — no new module pair, which is the thing §10.8 freezes
  about that directory. They are in `commands::assets` because that module's own header says it is
  "where **every** asset command lives, including the route, import and alert commands #432, #439
  and M4.1 add", and monitoring is the Assets view's second tab.

  **No migration for the two settings keys**, `monitoring.sample_retention_days` and
  `monitoring.response_time_warn_ms`: `knobas.setting` exists for exactly this (`0002`, comment 6),
  no migration has ever inserted a default into it, and the defaults live in Rust beside the reader
  (`knobas_sync::samples::DEFAULT_RETENTION_DAYS`, `DEFAULT_THRESHOLD_MS`) — so a fresh profile and
  a profile whose row was deleted give the same answer. Both are clamped on read and on write, the
  rule `BackupSchedule::clamped` records.

  **`SyncReport` does not grow a field, deliberately.** A sample count would be the obvious thing to
  report and it would be an IPC schema change (`demo_load` answers a `SyncReport`, and it has a
  TypeScript mirror) for a number nothing reads. It would also be a number that changes on every
  idle poll, which is exactly the run the engine deliberately writes *no* activity line for.

  **Where the write sits, and why it is three separate claims.** `knobas_sync::run_locked`, after
  the sweep and before the cursor update, inside the run's own transaction and under its advisory
  lock. After the sweep, so a monitor this run tombstoned is already out of `sync.live_item` and
  gets no sample — which is how "a monitor absent from this run gets no sample" is a property of the
  view rather than a rule someone maintains. Inside the transaction, so a failed run leaves behind
  no sample claiming to have seen a state it never committed. And **off the mirror rather than off
  the run's emitted items**, because the Kuma adapter's cursor is a digest of the last corpus (§4.2
  E) and an unchanged Kuma emits *nothing* — a timeseries built from emissions would go quiet on
  exactly the monitors that are steadily up.

  **The state is a declared read; the response time is an ADR-0007 interim one.** `status_name` is
  the declared path (`knobas_source_kuma::descriptor`'s one `payload_paths` entry) and
  `knobas_core::payload::resolve_string` is what reads it, so the engine never spells `state`.
  `response_time_ms` has no `KindPaths` slot shaped like "a duration in milliseconds", and adding
  one would be a `crates/knobas-source/src/**` change and a §10.8 conversation of its own — the same
  position #284's `ancestor_path_read!` is in, and the same disposition: one named place
  (`samples::sample_of`), a miss for every unusable shape, and a failure direction of *absence*
  pinned by test. A drifted read yields a row with no reading and therefore no derived *warn* —
  never a *down* nobody is having.

  **The retention rule is in `knobas_sync::samples` and the clock is `knobas_app::backup::tick`**,
  which is the arrangement `time::passive::prune` and that same tick already have: the rule stays
  with the module that owns what the swept thing *is*, and the app contributes its one wall-clock
  loop that is not per-source. `prune` takes `now` as a parameter rather than reading a clock, so a
  fixture can place the horizon.

  Pinned by: `crates/knobas-sync/tests/samples.rs` (fifteen tests — two runs two samples, an idle
  poll that emits nothing still sampling, a monitor absent from the second run getting no second
  sample, a source with no `monitor` kind writing none, `a_source_that_emits_two_kinds_samples_only_its_monitors`,
  warn at and over the threshold, a stored threshold moving it, a down monitor never softened, both
  directions of the ADR-0007 miss, the sweep taking what is past the horizon and nothing younger,
  the defaults on a database nobody has written a setting to, and `both_settings_are_stored_by_one_save`
  for the paired write);
  `crates/knobas-app/tests/adapter_to_mirror.rs`'
  `a_kuma_poll_leaves_one_sample_per_monitor_and_derives_warn_from_the_threshold`, which is the
  same engine driven by the **real** adapter over the recording, and the only place the declared
  `status_name` read is checked against the payload `knobas-source-kuma` actually writes;
  `knobas_sync::samples`'s own unit tests including
  `the_state_words_are_the_ones_the_column_accepts`, which reads the migration's CHECK;
  `commands::assets::tests::the_monitoring_settings_match_their_typescript_mirror` and the
  registration and argument-name loops (now seventeen commands); `tests/assets_ipc.rs`' handler
  list; `tests/backup_ipc.rs`' `a_tick_sweeps_the_samples_retention_has_aged_out`;
  `backup::share::tests::nothing_carries_the_activity_stream_the_queue_or_the_mirror`; and six tests
  in `MonitoringSection.test.svelte.ts` plus `monitoring.test.ts`.

  **And the backup half is pinned too, by a test nobody had to edit**:
  `crates/knobas-db/tests/backup.rs`' `the_archive_carries_the_owned_schema_and_leaves_the_mirror_out`
  reads the
  expected table set out of `pg_class` rather than from a list, precisely so "a table any later
  migration adds is in the archive without anyone remembering". `knobas.monitor_sample` is in that
  set from this migration on, which is what makes "in the backup" a witnessed claim rather than an
  argument about `--schema`.

  **Björn keeps the gate for frozen contracts and this entry is flagged for his review**, and in
  particular the migration and the two commands.

