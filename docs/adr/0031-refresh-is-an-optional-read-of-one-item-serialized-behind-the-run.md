---
status: accepted
---

# Refresh is an optional read of one item, serialized behind its source's run

After a write landed, knobas caught the mirror up with an incremental run of the whole source, which several send paths skip, so a write could draw *queued* until the next scheduled run. Decided 2026-10-25 (Björn, refresh-after-write grilling): **a [refresh](../../CONTEXT.md#refresh) is a new, optional `Source` read of one item by id. It answers the same `SyncItem` a run emits, goes through the same item write, and takes the source's run lock instead of checking an item version.**

- **Optional.** An adapter that cannot read one item, or an external one built against an older WIT minor version, falls back to a run: its detail draws *Sync now* where others draw *Refresh*. In the WIT world (ADR-0028) it is an additive minor version and a §10.8 entry.
- **After every landed write, from every send path.** The item the write made (by the receipt's `remote_id`) or wrote to is refreshed first, then a run of the source follows, waited for as `sync_after_write` waits, for what the write changed elsewhere.
- **Serialized, not versioned.** A refresh waits for an in-flight run of its source and reads after it commits, because item writes overwrite unconditionally (`ITEM_UPSERT`) and a run fetches inside its transaction. *Amended 2026-11-12 (`docs/decisions/2026-11-12-a-page-asks-for-what-it-draws.md`): a run holds the lock for the whole run, minutes for a large one, so the reader's* Refresh *takes it only if it is free and otherwise reads nothing and says the sync is running; a landed write's refresh still waits. Serialized either way.*
- **A run's per-item work, for one item.** Links on arrival, a monitor's samples and alerts, the context follow and the saved-view projection, plus an event the shell re-reads on. No "synced" activity line. An id the source no longer answers for is tombstoned.
- **A failed refresh is only a failed read.** The write stays `sent`, its [echo](../../CONTEXT.md#echo) says *sent*, and the next run settles it.

## Amended 2026-11-24: a run stands aside for a landed write's read-back

The pr-state fix (`docs/decisions/2026-11-24-a-pull-requests-state-shows-at-once.md`, Fable's ruling 1). A run holds the source's lock for its whole walk of the forge, and every landed write starts one, so a second write's read-back waited out the first's whole run: create a pull request, merge it, and *Merged* waited 3.5 s at seed size, minutes at Björn's.

- **Serialized still, never beside a run.** The read-back asks for the lock first; once it waits, the run in its way is stopped (`Scheduler::stand_aside`): its future is dropped, its transaction rolls back, and Postgres hands the lock to the read-back, so a run started in the gap queues behind it. The run after the write starts again from the same stored position.
- **Ends as a quit-cancelled run does**: its row closed with `stood aside: a landed write's read-back went first, write N`, its watchers told once, no verdict on the credential, the backoff untouched, not counted as a failure.
- **Never stopped**, and the read-back waits as before: a run a person pressed or joined, a first sync or a backfill, a source with no stored position (its run owes a full sync and its sweep), a source with no run that ended `ok` within max(10 min, twice its interval), and a read-back that never waits on the lock (nothing in flight, a kind the adapter does not read).

## Rejected

- Keep the run after a write and fix the paths that skip it. A run of a whole Jira is not instant, and *Refresh* on a detail has no run-shaped answer. The run stays as the follow-up.
- A version guard on the item write. Jira's `/search` rounds `updated` to the second where `/issue/{key}` keeps milliseconds, so two reads of one issue cannot be ordered.
- A required read. It would refuse every external adapter whose API cannot read by id, and Kuma has only `/metrics`.
- Write the write's own payload into the mirror. The receipt carries an id and nothing else.

## Consequences

- ADR-0003 is unaffected: a refresh never sweeps and moves no watermark, so the next incremental run delivers the item again inside its overlap window. *Amended 2026-11-20 by [ADR-0047](0047-a-refresh-under-a-parent-walks-it-as-a-run-and-moves-its-position.md): a refresh under a parent (a Gitea repository, branch or commit) walks that parent as a run would and moves that parent's part of the position with its rows; still no sweep.*
- `sync.item` has a second writer beside `run_once`, under the same lock and item path: only the sync crate writes the mirror, under the source's lock.
- Hold detection compares mirror projections, so a refresh between two queued writes on one item can hold the second, as the post-write run already could.
