- **Migration `0016`, one command and one argument on `start_timer`, issue #281 (2026-09-03):**
  M3.1's ad-hoc block, ratified in advance by the spec (#272) Björn approved — "Schema and
  settings. Migrations from the next free number for the timer row, blocks, worklogs" and "Time's
  IPC. One §10.8-ratified exception for a `time` module pair on both sides of the bridge, holding
  timer, heartbeat, block, worklog, draft, day and week commands." Written with the implementing
  PR per the #175/#177/#208 pattern.

  `0013` was the timer, `0014` #280's worklog, `0015` #282's heartbeat; this stream took `0016`,
  and the next free number is `0017`.

  **The migration.** `0016_the_room_a_block_ran_in.sql` adds one nullable column to each of two
  tables and alters nothing:

  ```sql
  alter table knobas.timer add column context_id text;
  alter table knobas.block  add column context_id text;
  ```

  **It is on both tables because the block is written by the statement that deletes the timer.**
  `STOP_TIMER` and `CLOSE_STRANDED` are `insert ... select` over the deleted row (`0013`), so a
  column on the block alone would have to be bound by a writer that no longer holds the value —
  the room the reader stood in an hour ago. The timer is where it waits, exactly as `started_at`
  does.

  **It records the room at the *start*, and that is the whole point of storing it at all.** The
  ad-hoc dialog's second rule is *the anchor ticket of the stored context the block ran in*, and
  "ran in" is a fact about the moment the clock started. The room the reader is standing in when
  the dialog opens is a different fact — they may have switched rooms twice since — and reading
  that one would attribute this morning's page to this afternoon's epic. There is no back-fill
  and there can be none: every block written before this migration carries `null`, which reads as
  *the reader was not in a stored room*, and rule two does not fire for them.

  **Null is a real answer, not a missing one.** *All work*, a source room and a project room are
  derived rooms with no `ctx:` row behind them (`app/src/lib/shell/contexts.ts`), and each carries
  `null` in `RoomContext.filter.context`. The shell sends that, not the room's own id.

  **No foreign key on `context_id`**, the decision `0005`, `0008`, `0009` and `0013` all record,
  with one addition: deleting a context must not delete the afternoon that ran in it, nor rewrite
  where it happened. A dangling `ctx:` id reads correctly without the constraint — the anchor read
  finds no row and rule two does not fire. **The column holds a `ctx:` id and the schema does not
  say so**, the mirror image of `0013`'s rule about targets and for the same reason: a check
  constraint that parsed entity ids would be a second copy of
  `knobas_core::entity::RESERVED_NAMESPACES`. `knobas_app::time::in_room` is the one enforcement,
  and it **refuses** rather than dropping — the opposite call `heartbeat` makes about its
  foreground, because the two failures cost different things: a refused beat freezes the
  last-alive stamp and shortens a block by hours, while a refused start is a sentence in front of
  a reader whose finger is still on the key.

  **`context_id` is deliberately not on the `Block` DTO.** Nothing on screen draws it; it is
  knobas' own reason for a suggestion, read only by `time::suggest`. Putting it on the wire would
  add a column to every statement in the module and a field to every mirror test for no reader.

  **The command**, in the existing `time` pair:

  ```rust
  #[tauri::command] pub async fn ad_hoc_block(.., block_id: i64, day: NaiveDate, offset_minutes: i32)
      -> Result<Option<time::suggest::AdHocBlock>, IpcError>;
  ```

  **Two absences, and they are different answers**, which is why `AdHocBlock` is a struct rather
  than an `Option<Option<Suggestion>>` — serde spells both of those `null`. The command's own
  `null` is *this block is on a ticket*, and the shell opens #280's worklog draft instead;
  `{ suggestion: null }` is *the dialog opens and knobas has nothing to suggest*, where *Keep
  local* is the default. The block's id is not echoed back: the caller passed it in and still holds
  it. **That is what keeps the shell free of a list of kinds**: whether a
  block's target is somewhere a worklog can go is read off the source's declared `log_work`, the
  same descriptor question `worklog::takes_a_worklog` asks, so an adapter that starts taking
  worklogs needs no frontend change. A table of kinds here is exactly what §3a exists to prevent.

  **`start_timer` grows one argument, `inRoom: Option<String>`.** On the command rather than in a
  second call for the reason #278 put `foreground` on the heartbeat before there was a table for
  it: the value is only knowable at the moment of the start, and adding it later would be a second
  §10.8 touch on a command that already exists. It is `null` for every derived room. The shell
  holds it ambiently on the timer store beside `foreground`, because ⌘T and the launcher's *Start
  timer* row both start a timer without knowing about rooms, and a parameter each caller had to
  remember is one a caller eventually forgets — which would record every block as having run
  nowhere and stop rule two firing with nothing on screen to say so.

  **The rules read `knobas.confirmed_link`, never `knobas.link` (ADR-0008, #41).** A proposal is a
  detector's guess, and a suggestion built on one would be a guess about a guess in a dialog whose
  *Log to a ticket…* puts real minutes on a real ticket. `crates/knobas-core/tests/link_reads.rs`
  holds the file to it. "Most recently linked" is read as `confirmed_at`, not `created_at`: for an
  accepted suggestion the row exists from the moment a detector guessed, and the moment it became
  a *link* is the moment somebody agreed.

  **What did not change.** No existing DTO field, no event name, and no new event: an ad-hoc block
  is a dialog the shell opens on an answer it asked for, not something that happened. No new write
  path — *Log to a ticket…* re-targets through the existing `update_block` and then opens #280's
  existing draft, in that order, because the draft is built from the day's unlogged blocks *on
  that ticket*. Nothing under `crates/knobas-source/src/**`: a block is knobas' own and no adapter
  hears about one. `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are
  untouched — the failures are `invalid` and `not_found`, which `IpcError` already carries.
  `knobas_core` gains nothing. The backup export needs no change: it dumps the whole `knobas`
  schema (design §16.12), so the new column rides in it. Two items in
  `crates/knobas-app/src/time/worklog.rs` widen from private to `pub(super)` — `LOG_WORK` and
  `day_bounds` — so that the one spelling of `log_work` and the one reckoning of a reader's day
  stay one each; nothing leaves the crate.

  **Which barrels were appended**: one line at the foot of the `commands::time::` group in
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, and two interfaces, one union and one
  function at the foot of `app/src/lib/ipc/time.ts` (`startTimer`'s signature grows its second
  argument in place). No barrel is rewritten.

  Ratified by the orchestrator as spec #272 and issue #281, whose acceptance criteria specify the
  read, the recorded room, the dialog's two actions, the tests and this entry.

