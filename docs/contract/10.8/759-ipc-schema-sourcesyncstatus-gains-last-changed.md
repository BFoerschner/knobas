- **IPC schema, issue #759 (2026-10-14): `SourceSyncStatus` gains `last_changed`.** Ruled by the
  deputy for Björn on #759 on 2026-10-14: "the projects store re-reads only after a run that
  changed something". The payload carried no run counts, so the store had nothing to tell a
  no-op run by. This adds that signal.

  **The field.** `last_changed: Option<bool>` (TypeScript `boolean | null`): whether the run
  `last_outcome` describes wrote anything to the mirror, i.e. its `upserted + deleted + swept > 0`.
  It is `None` while no run of the source has finished. In `sync_status`'s listing it comes from
  the newest finished run's row (the `STATUS` statement's `f` lateral now reads the three counts).
  On a terminal `sync:state` event it comes from that run's own row, or `None` when the row could
  not be read. A starting event carries the source's, the way it carries `last_outcome`. It
  over-approximates on purpose: `upserted` counts items rewritten unchanged, so `true` may be a
  no-op, but `false` never hides a run that changed something.

  **The shell.** `shell/projects.svelte.ts` re-reads the census on a finished run unless
  `last_changed === false`; `null` is read as "maybe". `SourcesView`'s optimistic
  `markStarted` keeps the previous value. Every TypeScript fixture of the type gains the field.

  Pinned by `knobas-sync`'s `the_sync_status_shape_matches_its_typescript_mirror`,
  `an_emits_identity_comes_from_its_run_and_its_countdown_from_the_source` (the terminal event's
  own counts, not the source's, in both directions), `status_reports_running_then_the_finished_shape`
  (the listing: `None`, then `Some(true)`, then `Some(false)` once the run's counts read zero), and
  `projects.test.svelte.ts` (`a finished run that changed nothing does not re-list`). Mutants
  killed: the terminal event reading the source's value; the store ignoring the field.
