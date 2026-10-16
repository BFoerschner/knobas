- **Migration `0041`, one command and the IPC schema of the day read — issues #900 and #901 (2026-10-16): *Dismiss* on an offered block, and the day's open stretches.** Spec #808, *Time and Settings*; requirement R-13.4 ("A passive block offers *Add to <suggested ticket>*, *Assign…*, *Dismiss*"; "focused time with no target dashed (*open*)"); milestone TF2. Amends #337's entry (`337-ipc-schema-day-read-s-answer.md`) for `DayRecord`. No event, no `WriteOp`, no declared path, nothing under `crates/knobas-source/src/**`, no new module in either barrel or in `commands/`.

  **The migration.** `create table knobas.passive_dismissal (started_at timestamptz primary key, ended_at timestamptz not null, check (ended_at >= started_at))`. A dismissal is keyed by the stretch's start, a passive block's identity (`0015`'s `block_passive_start_idx`). A table and not a marker on `knobas.block`: a passive row is re-derived before every day read, and a marked row would be one every other reader of blocks (week, worklog, standup) had to learn to skip.

  **The command**, in the existing `commands/time.rs`, after `delete_block`, registered beside it:

  ```rust
  #[tauri::command] pub async fn dismiss_block(lifecycle: State<'_, Lifecycle>, id: i64)
      -> Result<(), IpcError>;   // time::passive::dismiss
  ```

  One statement deletes the row (`kind = 'passive' and worklog_id is null`) and upserts its stretch into `passive_dismissal` (the end kept at the greater). `invalid` for a block that is not an offer, `not_found` for a block that is not there. `passive::materialize` does not offer a derived span whose start is dismissed, and moves the dismissal's end with the span, so the part of a growing stretch past the dismissal is still dismissed.

  **The day read's answer** gains `open`:

  ```rust
  pub struct DayRecord { pub blocks: Vec<DayBlock>, pub past_horizon: bool,
      pub open: Vec<OpenStretch> }
  pub struct OpenStretch { pub started_at: DateTime<Utc>, pub ended_at: DateTime<Utc> }
  ```

  `open` is the day's focused time (`passive::focused_spans` over the day's observations) minus every block overlapping the day, every dismissed stretch and the running timer from its start to now (`passive::set_aside`), earliest first; empty past the horizon and on a day with no observations. The subtraction is one pure function, `passive::open_spans`, and `week::read`'s "no target, app open" row now adds up the same stretches (it replaces `week::uncovered_seconds`), so the Day tab's *open* figure and the week's cell agree. That row now also leaves out dismissed stretches and the running timer's time, which it used to count.

  Mirrored in `app/src/lib/ipc/time.ts` (`dismissBlock`, `DayRecord.open`, `interface OpenStretch`) and answered by `?fake-ipc` (`app/src/lib/shell/dev/fake/time.ts`). Pinned by `commands::time::tests::the_day_record_matches_its_typescript_mirror` (with `open` filled), `the_mirror_invokes_the_commands_by_their_registered_names`, `the_mirror_sends_the_argument_names_tauri_expects`, `time::passive::tests::open_stretches_are_focused_time_minus_the_union_of_what_claims_it`, and `tests/it/day_offers_ipc.rs`.
