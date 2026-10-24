- **IPC command semantics, issue #1932 (2026-10-24): the quit waits for a send a command waits
  on, and a send it cuts is held.** Ruled by the implementer on Opus
  (`docs/decisions/2026-10-unattended-rulings.md`, #1932). No signature change, no new variant,
  no migration, no event, no new module in `commands/` or `ipc/`. The generated mirror does not
  change. `knobas_sync::background::Background` gains `try_spawn` and `running`, crate functions,
  not commands.

  *Flush now* (`flush_writes`), *Send mine anyway* (`apply_held_write`), *Edit and send*
  (`amend_write`), and the app's awaited submit and worklog delivery used to await their flush on
  the command's own task, outside the scheduler's background set. The quit did not wait for it, so
  a write on the wire at the quit had the pool closed under it: its `sent` mark failed, the row
  stayed pending, and the next start sent it again. Each flush now runs in the background set and
  the command awaits its answer there, so the quit waits for that write under its grace (#1633).

  A write still at its source when the grace runs out, on any path, the background send and the
  flush task's included, is aborted and then **held** as one the source may have, with the source
  to check named (`held_snapshot.unconfirmed`, as #1125's unanswered send). Before, it stayed
  pending and the next start sent it again. A write whose `resend_harmless` is true stays pending,
  as an unanswered send of it does.

  What a caller sees: the three commands answer `not_ready` if the quit's grace ran out on their
  write. A command made once the quit has begun answers as before: the set takes nothing then, so
  the flush runs on the command's task, sends nothing (#1924), and the row changes as asked and
  waits for the next start. A held write from a cut quit reads like any unconfirmed held write in
  `pending_writes`, and `apply_held_write` sends it again.

  Witnessed by `crates/knobas-app/tests/it/background_write.rs`
  (`a_write_a_command_waits_on_at_quit_lands_and_is_not_sent_again`,
  `a_write_still_at_the_source_when_the_grace_ends_is_held_as_unconfirmed`,
  `a_command_made_during_the_quit_changes_its_row_and_sends_nothing`).
