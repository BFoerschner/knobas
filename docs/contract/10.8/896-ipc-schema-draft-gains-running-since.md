- **IPC schema, issue #896 (2026-10-16):** `worklog_draft`'s answer, `knobas_app::time::worklog::Draft`,
  gains `running_since: Option<DateTime<Utc>>`, with the matching `running_since: string | null` on
  `Draft` in `app/src/lib/ipc/time.ts`. The command's name and arguments are unchanged.

  **What changes in the answer.** When the running timer is on the drafted ticket and started inside
  the drafted day (the day rule `UNLOGGED_BLOCKS` already applies: a stretch belongs to the day it
  started on), the draft counts that open stretch up to the database's `now()`:

  - `running_since` is the timer's `started_at`; `null` otherwise;
  - `seconds` adds the open stretch to the closed blocks' sum;
  - `ended_at` is that `now()`, and `started_at` the earliest of the closed blocks and the stretch;
  - the candidates' window is that whole interval, so what the reader did while the clock is still
    running is offered;
  - `block_ids` stays the **closed** blocks only. The stretch has no block until the clock stops.

  A running timer with no closed block behind it is now a draft of its own, where before the answer
  was `null`.

  **Why.** The new shell opens the draft before the clock stops, as prototype-6 does (#826's
  ruling, reversed here as it said it would be): *Keep running* then only closes the dialog and
  loses no time, and the chip reads the running timer behind it. `log_work` is unchanged: it still
  re-derives the covered blocks from closed, unlogged blocks, so a shell stops the clock before it
  logs, and the stretch it just closed is covered.

  **What did not change.** No new command, event or migration; neither append-only barrel is
  touched. `log_work`, `log_all` and the week's reads still see closed blocks only. The rooms UI
  stops the clock before it asks for a draft, so it reads `running_since: null` and draws as before.
