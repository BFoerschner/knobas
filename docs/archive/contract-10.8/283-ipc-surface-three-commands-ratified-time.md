- **The IPC surface, issue #283 (2026-09-03): three commands on the ratified `time` module pair —
  the week timesheet, *Log all*'s plan, and *Log all*.** The pair itself is #278's ratified
  exception; this is the entry each command addition owes it. `week_timesheet`,
  `log_all_preview` and `log_all` are appended to the foot of the `commands::time::` group in
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list and to the foot of
  `app/src/lib/ipc/time.ts`. Neither barrel is rewritten, and no existing command, DTO field or
  event name changes meaning.

  **No migration.** `0013`, `0014` and `0015` hold everything the week reads: blocks, worklogs,
  the write queue's state and the heartbeat. This ticket adds no column and takes no number.

  **The week arrives as seven `DayWindow`s, not as a date and an offset**, and that is the one
  shape decision on the wire worth arguing about. #279 established that a *day* crosses the
  bridge as two instants because the machine's timezone is a fact only the webview holds. A week
  makes the cost of the alternative concrete: the week containing a daylight-saving change has one
  day of 23 or 25 hours in it and two offsets across it, so seven windows built by adding 24 hours
  to a Monday would file one evening under the wrong column in the one week of the year a reader
  is most likely to check. Each window carries the date **and** the two instants, because neither
  side can derive the other: the query needs the instants, the cell is keyed and labelled by the
  date, and a backend recomputing the second from the first would be doing exactly the arithmetic
  this shape exists to keep out of it. The read refuses a list that is empty, longer than seven, or
  whose windows overlap — an overlap would count one block into two columns.

  **Which day a stretch is on, and the one deliberate exception.** A block is counted on the day it
  **started** on, whole, which is the rule `time::worklog`'s `UNLOGGED_BLOCKS` already keys on. It
  has to be the same rule, because *Log all* logs by that rule: a timesheet that split a block
  across midnight would show Tuesday time that *Log all* then sent on Monday, and the two columns
  would never reconcile. The exception is **coverage** — the "no target, app open" row asks whether
  a given *instant* was inside a block, so it uses overlap clipped to the day. Two questions, two
  rules, and the difference is written down beside both.

  **Logged, held, and why `refused` rides with `held`.** The numbers come from the local worklog
  copy joined to the write queue's state and **never from the mirror**, which is spec #272's
  wording. `pending` and `sent` are *logged*, because story 39 says the number is about what the
  reader did rather than about sync timing. Everything else — `held`, `refused`, `discarded`, or a
  copy whose queue row has been pruned — is *held*: one word for "this time has not reached the
  ticket". It is reported separately from
  *unlogged* rather than folded into it, because the blocks under it already carry a worklog id and
  *Log all* will not offer them again — drawing it as unlogged would be an invitation to log one
  afternoon twice, and drawing it as logged would be false.

  **The ruling on the four, made at the merge (2026-09-03).** `held` and `refused` are *open*
  states — `write_queue::open` selects `state in ('pending','held','refused')` — so they sit in the
  pending-writes panel and a person can retry or withdraw one; held is plainly right for them.
  `discarded` is not: that module's own words are "a sent or discarded write is history", and
  `write_queue::discard` touches only the queue row, leaving the worklog copy and the block's
  `worklog_id` in place. Discarded time therefore reads as held for good, `unlogged` stays zero,
  and neither *Log all* nor the draft offers the blocks again. Held is still the honest cell of the
  four this read has — the time has not reached the ticket and the blocks are spoken for — but the
  way out is the discard path's to build (clear the copy and the mark), not this read's to paper
  over: filed as **#328** rather than fixed here. **Built there since (#328, 2026-09-03):**
  `knobas_core::write_queue::discard` now deletes the worklog copy in the statement that settles
  the queue row, and `block_worklog_fk`'s `on delete set null` gives the blocks back, so discarded
  time reads as *unlogged* and every surface offers it again. No migration and no new command --
  no surface in this section's frozen list changed, which is why #328 has no entry of its own.
  `discarded` stays in `is_logged`'s *held* half as a backstop: the release spares a copy carrying
  a `remote_id`, so an hour Jira answered for is never offered for logging twice, and
  `a_discard_leaves_a_worklog_jira_answered_for_alone` reads that cell rather than leaving the
  claim unwitnessed. *Unlogged* is `tracked - logged - held` floored at zero; the floor is not
  tidiness, it is that the draft's interval and seconds are the reader's own and may exceed the
  blocks they were made of.

  **Tracked is manual time; offered is the guess, counted beside it.** A passive block is not in
  the tracked total, which is the same sentence the day review's heading already says on the same
  screen. It is not *dropped* either: `WeekCell::offered_seconds` carries it in the strip's own
  word, because a week that lost an afternoon the strip directly above it is drawing would be two
  surfaces disagreeing about one day, and story 41 asks for a total that is honest. Offered pays
  into no other column — it is not tracked, it is never logged, and it is not time somebody failed
  to log — so *unlogged* stays `tracked - logged - held`.

  **The "no target, app open" row, and the one thing it cannot say.** It is focused time no block
  covers, measured by `time::passive::focused_spans` — extracted from `derive`'s cap loop by this
  ticket so that there is **one** walk over focused time in the crate rather than two opinions
  about one afternoon. The row is present only when it has something to say, so a week knobas was
  shut for has no such row at all. What it cannot distinguish is a week with passive attribution
  **off**: no heartbeat is recorded then, so an open window and a shut one look the same. That is
  inherent to the setting being opt-in (#282) and is stated here rather than worked around, because
  the alternative — recording beats for a person who has not asked — is the one thing that setting
  exists to prevent.

  **The week read reconciles passive blocks per day**, the same `passive::materialize` call
  `time::day::list` makes and for the same reason: the derivation's cap is a rule about a day, and
  these seven windows are the only place the reader's midnights are known.

  **One counter holds the screen together.** The day strip and the week share one address, and an
  edit on either changes what the other draws: assigning a block moves the week's unlogged total,
  and *Log all* makes the strip's blocks read-only. Neither view owns the other, and the time
  commands deliberately write **no activity line** (#278's entry gives the reason for the timer's
  reads; `update_block` and `create_block` carry their own), so there is no signal to listen to and
  inventing one would be a second thing to keep in step with the first. `App.svelte` therefore
  holds a `timeRevision` counter: each view bumps it after a write and re-reads when it moves.
  That is spec #272 story 44 — "assigning a block and watching the week's unlogged total change is
  one glance". **The rule is "after something was written", not "after the call returned `ok`",
  and the two differ in exactly one place.** A refused *edit* does not bump it, because one
  refused statement wrote nothing. A refused ***Log all*** does, because it is not one statement:
  a day that fails does not roll back the days that succeeded (ADR-0012), so most of a week's
  worklogs may exist behind that rejection, and a success-only bump would leave the strip offering
  *Edit* on blocks they have just made read-only. The shell's own two writers bump it as well —
  the worklog draft when it logs (the ordinary way a worklog is made, and it moves the same
  `logged` column *Log all* moves) and the ad-hoc dialog when it re-targets a block, which moves a
  row of the week from one target to another. This is shell state, not a frozen surface: no event
  name, no DTO field, no command.

  **The column the strip stands on is never collapsed.** The weekend rule and the highlight rule
  meet on an empty Saturday, and the naive composition loses: the highlighted column would be the
  one column the table does not draw. `visibleColumns(week, current)` keeps that column for exactly
  as long as the strip is on it.

  ***Log all* is a plan and then a write, and the gap is the point.** `log_all_preview` lists one line
  per (day, ticket) and queues nothing; `log_all` **re-derives** its own work rather than being
  handed that plan back, the reason `log_work` does not take block ids — a caller that could name
  the blocks could name another ticket's, and a timer that stopped while the confirmation was on
  screen is logged too. The exclusions are `time::worklog`'s own `UNLOGGED_BLOCKS` reused rather
  than restated: `kind = 'manual'` keeps passive blocks out (story 30 — no passive block is logged
  without a person saying so), `entity_id is not null` keeps label blocks out (a label is not
  somewhere a worklog can go; story 25's *Log to a ticket…* is that path and it is a person
  choosing), and `worklog_id is null` is what makes a second run send nothing. The comment is
  **empty**, deliberately: a worklog with no words is a worklog (story 33), and generating the
  draft's candidate bullets for a whole week without anybody reading one of them would send text
  nobody ticked into a system other people read. A day that fails does not roll back the days that
  succeeded — the copies are knobas' record of writes that may already have landed, and ADR-0012
  forbids losing one.

  **The backup export needed no change, and now has a test saying so.** The dump is schema-scoped
  (design §16.12), so `knobas.timer`, `knobas.block`, `knobas.worklog`, `knobas.heartbeat` and the
  time settings ride in it already — which is exactly the kind of claim that stops being true
  silently, and a `--schema` somebody narrowed to a table list would have broken nothing else in
  the suite. `tests/backup.rs` now reads the archive's own table of contents back off the file and
  round-trips a timer, its blocks, a worklog and the passive-attribution setting into a database
  that has never seen them.

  **The share export's time toggle is recorded, not built.** The curated share export is M4's
  (roadmap §"export/import complete"); there is no share export in the tree to add a toggle to.
  What this ticket owes it — the sentence #282's entry called "#283's paperwork" — is the
  **default**, and that is now in three places: `CONTEXT.md`'s **Share export** entry, the design
  doc's **§14 Export / import row** — where an M4 implementer reads the defaults off — and its
  **§16 answer 12**, which is where that row's defaults were ratified. Time is out of a share
  export by default, personal the way a note is, and toggleable with every other part when that
  export is built. Recorded at the merge because the criterion says *toggle* and there is no
  toggle: the criterion is discharged by the default, and the M4 ticket that builds the export
  inherits the rest of it.

  **What did not change.** Nothing under `crates/knobas-source/src/**` — a block, a worklog and a
  heartbeat are knobas' own and no adapter hears about one; `WriteOp::LogWork` is #280's growth
  with its own entry and is reused unchanged. `crates/knobas-db/migrations/**` is untouched.
  `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are untouched: the
  failures are `invalid`, `conflict` and query failures, which `IpcError` already carries.
  `knobas_core` gains nothing. `crates/knobas-db/src/backup.rs` is unchanged — the point of the
  test above is that it did not need to be.

  Ratified by the orchestrator as spec #272 and issue #283, whose acceptance criteria specify the
  week read, *Log all* and its confirmation, the view, the backup round trip, the tests and this
  entry.
