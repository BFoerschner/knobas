- **The share export's *time* part, issue #1028 (2026-10-17): it carries `knobas.passive_dismissal`.**
  Amends #454's entry (`454-fifth-backup-command-one-new-dto.md`), whose part list reads
  "time → `timer`, `block`, `worklog`, `heartbeat`". It now reads time → `timer`, `block`,
  `worklog`, `heartbeat`, `passive_dismissal`. Spec #808 (found while building #900). No
  migration, no command, no DTO change, no event: `ShareParts` keeps its fields and defaults, and
  `app/src/lib/ipc/backup.ts` is untouched.

  **Why.** Migration `0041` (#900) added the table of stretches a person dismissed, and the day
  read re-derives its offers from `knobas.heartbeat` before every read. A time part carrying the
  observations without the dismissals offers every dismissed stretch again on the far machine's
  first day read.

  **The restore needs nothing new.** `passive_dismissal` has no foreign key (it is keyed by the
  stretch's start, `started_at`), so it loads without any other part's rows. An archive taken
  before this change lacks the table; the restore is data-only and loads only the tables an
  archive names, so it leaves the table empty, as it already does for any table a share export
  left out. A full backup was never affected: it dumps the whole schema.

  Held by `backup::share::tests::each_part_alone_is_exactly_its_own_tables`,
  `tests/it/backup_ipc.rs`'s `each_part_alone_brings_exactly_its_own_tables` (the archive's table
  of contents) and `a_dismissed_offer_crosses_the_time_part_still_dismissed` (dismiss, export the
  time part, restore into an empty knobas, read the day there).
