- **Three block commands in the `time` module pair, issue #279 (2026-09-03):** the day review's
  read and its two edits, landing inside the module pair #278's entry above ratified — "it is
  where **every** time command lives, including the block, worklog, draft, day and week commands
  #279 and #280 add". **No migration and no new module**: `knobas.block` is `0013`'s, and the
  Rust is a second file *inside* `crates/knobas-app/src/time/`. The IPC schema still records each
  addition, which is what this entry is.

  ```rust
  #[tauri::command] pub async fn day_blocks(.., from: DateTime<Utc>, to: DateTime<Utc>)
      -> Result<Vec<time::day::DayBlock>, IpcError>;
  #[tauri::command] pub async fn update_block(.., id: i64, started_at: DateTime<Utc>,
      ended_at: DateTime<Utc>, target: time::TimerTarget) -> Result<time::day::DayBlock, IpcError>;
  #[tauri::command] pub async fn delete_block(.., id: i64) -> Result<(), IpcError>;
  ```

  `DayBlock` is `{ block: Block, title: string | null }` — the block beside the mirror's *current*
  opinion of what its target is called, never folded into it. A block is knobas' own durable
  record and migration `0013` deliberately keeps no foreign key on `entity_id`, so a purged
  mirror leaves a block that still says how long it was; a `title` field on `Block` would make
  that look like something the block had forgotten. A blank title is `null` too, so the strip has
  one question with one answer: show the id instead.

  **A day is two instants, not a `YYYY-MM-DD`, and the webview computes them.** The machine's
  timezone is a fact only the webview holds — nothing on this bridge has ever carried one — and a
  UTC offset passed instead would be the wrong *shape* as well as the wrong owner: a day
  containing a daylight-saving change is 23 or 25 hours long and has two offsets, so an offset
  would move one edge of the strip wrongly twice a year. `app/src/lib/time/day.ts`'s `dayBounds`
  asks `Date` for that day's midnight and the next day's. It is also the shape the week timesheet
  (#283) wants without a second command. The interval is half-open and matches on **overlap**:
  work that ran through midnight is on both days it touched, while a block that stops *exactly*
  where a day begins is the previous evening's — returning it would put a zero-width sliver at the
  head of the strip and the whole night would then draw as unaccounted time. A block of no length
  starting at midnight is still that day's, because a block of no length is still a block.

  **`update_block` takes the whole editable shape, and clears `ended_by_relaunch` on every
  success.** A patch would leave "the end is unchanged" and "the end is absent" spelled the same
  way on the wire; the whole shape means every call is the reader stating what the block *is*.
  That is what makes clearing the marker honest — it means *knobas guessed this end*, written by
  the relaunch sweep at the last moment knobas was known to be alive, and once a person has said
  what the end is it is theirs. *Extend to now* is therefore this one command with `ended_at` set
  to now rather than a fourth command, so "the end moved" and "the marker went" cannot come
  apart.

  **A logged block is read-only, enforced as a `where` clause rather than as a check.** Both
  writing statements carry `worklog_id is null`, so a caller that never asked whether the block
  was logged still cannot edit one, and there is no gap between a check and a write. The reason a
  write matched no row is then diagnosed *afterwards* into three sentences the day review shows —
  `invalid` naming the worklog, `not_found` for a block that is gone, `conflict` for one that
  moved underneath the write — because "nothing happened" is not something anybody can act on.
  `knobas.worklog` is #280's table; the column is `0013`'s precisely so this rule did not have to
  wait for it, and `time_ipc.rs` reaches it by setting the column directly, which is the only way
  until #280.

  **No activity line and no `AppHandle`,** unlike `start_timer` and `stop_timer`. An edit is a
  correction to knobas' own record of a stretch that has already stopped, not something that
  happened; and the shell's timer store re-reads itself on every `activity:new`, so a line here
  would make every correction a reason for the top strip to go back to the database for a clock
  that did not move.

  **The navigation contract grows one address**, `#/time/<YYYY-MM-DD>` with `#/time` meaning
  today, and `time` stays in `router.svelte.ts`'s `RESERVED` for the reason `inbox` does: a *kind*
  called `time` must never claim it. `parseHash` stays pure — a bare `#/time` parses to
  `day: null` and the view resolves it against its own clock, because a parse that read the clock
  would give one address two meanings across a midnight the reader is sitting through. A tail that
  is not a day is today, the head-owns-the-address rule `#/sources/x` and `#/settings/x` already
  follow.

  **Which barrels were appended**: three lines at the foot of the `commands::time::` group in
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, and three functions at the foot of
  `app/src/lib/ipc/time.ts`. No barrel is rewritten, and `commands/mod.rs` and `ipc/index.ts` are
  untouched — the modules they name already exist.

  **What did not change.** No migration, no existing command, DTO field or event name, and no new
  event. Nothing under `crates/knobas-source/src/**` — a block is knobas' own and no adapter hears
  about one. `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are untouched:
  the three failures are `invalid`, `not_found` and `conflict`, which `IpcError`'s existing
  constructors already carry. No settings key. The backup export needs no change — it dumps the
  whole `knobas` schema (design §16.12), and `knobas.block` has ridden in it since #278.
  `knobas_core` gains nothing.

  Ratified by the orchestrator as spec #272 and issue #279, whose acceptance criteria specify the
  address, the three commands, the strip, *Extend to now*, the tests and this entry.

