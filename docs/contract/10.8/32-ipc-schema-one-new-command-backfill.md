- **IPC schema**, issue #32 (2026-08-28): one new command, `backfill_source` (`sourceId` -> run id),
  with `backfillSource` in `app/src/lib/ipc/sources.ts`. It re-reads a source from the top,
  ignoring its stored position, so a *payload widening* reaches items nobody has touched -- the job
  nothing else can do, since an incremental run re-fetches what changed upstream and widening a
  `fields=` list changes nothing upstream. It **deliberately does not sweep**: ratified non-sweeping
  by Björn 2026-08-28, because the way a cursor-less run goes wrong produces no error -- a credential
  that quietly loses sight of a project answers with a smaller corpus and a 200 -- and a sweeping
  backfill would read that as "those items are gone". Additive; no existing command's shape changes.
  No UI affordance calls it yet (#69 owns the settings surface).

