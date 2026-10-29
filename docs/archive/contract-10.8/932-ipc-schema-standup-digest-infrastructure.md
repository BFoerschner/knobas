- **IPC schema, issue #932 (2026-10-17): `standup_digest`'s answer gains `infrastructure`.**
  `knobas_app::standup::StandupDigest` gains `infrastructure: Vec<DigestLine>`, with the matching
  required `infrastructure: DigestLine[]` on `StandupDigest` in `app/src/lib/ipc/entity.ts`. The
  command's name and arguments are unchanged. No new command, no migration, no event, no
  `commands/` or `ipc/` module.

  **What the list is.** One `DigestLine` per `knobas.monitor_alert` row that was open at any
  instant from the start of `yesterday_day`'s window (today's start when no such day was found) to
  the end of today's window, newest `opened_at` first, at most 200. `entity_id` is the monitor,
  `kind` is `"monitor"`, `title` its name, `source` the entity id's namespace, `verb` the alert's
  state (`"down"` or `"warn"`), `at` when the alert opened. `reason` is a duration with no clock
  reading: `was down 12 min`, `was slow 1 h 5 min`, or `is still down` while the alert is open. The
  clock is the reader's timezone, which only the webview holds, so the view draws `from 03:10` out
  of `at`.

  **What it is not.** Not mine-only: a monitor has no owner in the mirror. No restart lines: the
  mirror records a state change and never why it changed, so prototype-6's "vm-pay-01 restarted at
  03:12" has no source.

  **What did not change.** The three existing lists, `yesterday_day`, and the window arguments.
