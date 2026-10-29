---
status: accepted
---

# Refresh is an optional read of one item, serialized behind its source's run

After a write landed, knobas caught the mirror up with an incremental run of the whole source, which several send paths skip, so a write could draw *queued* until the next scheduled run. Decided 2026-10-25 (Björn, refresh-after-write grilling): **a [refresh](../../CONTEXT.md#refresh) is a new, optional `Source` read of one item by id. It answers the same `SyncItem` a run emits, goes through the same item write, and takes the source's run lock instead of checking an item version.**

- **Optional.** An adapter that cannot read one item, or an external one built against an older WIT minor version, falls back to a run: its detail draws *Sync now* where others draw *Refresh*. In the WIT world (ADR-0028) it is an additive minor version and a §10.8 entry.
- **After every landed write, from every send path.** The item the write made (by the receipt's `remote_id`) or wrote to is refreshed first, then a run of the source follows, waited for as `sync_after_write` waits, for what the write changed elsewhere.
- **Serialized, not versioned.** A refresh waits for an in-flight run of its source and reads after it commits, because item writes overwrite unconditionally (`ITEM_UPSERT`) and a run fetches inside its transaction.
- **A run's per-item work, for one item.** Links on arrival, a monitor's samples and alerts, the context follow and the saved-view projection, plus an event the shell re-reads on. No "synced" activity line. An id the source no longer answers for is tombstoned.
- **A failed refresh is only a failed read.** The write stays `sent`, its [echo](../../CONTEXT.md#echo) says *sent*, and the next run settles it.

## Rejected

- Keep the run after a write and fix the paths that skip it. A run of a whole Jira is not instant, and *Refresh* on a detail has no run-shaped answer. The run stays as the follow-up.
- A version guard on the item write. Jira's `/search` rounds `updated` to the second where `/issue/{key}` keeps milliseconds, so two reads of one issue cannot be ordered.
- A required read. It would refuse every external adapter whose API cannot read by id, and Kuma has only `/metrics`.
- Write the write's own payload into the mirror. The receipt carries an id and nothing else.

## Consequences

- ADR-0003 is unaffected: a refresh never sweeps and moves no watermark, so the next incremental run delivers the item again inside its overlap window.
- `sync.item` has a second writer beside `run_once`, under the same lock and item path: only the sync crate writes the mirror, under the source's lock.
- Hold detection compares mirror projections, so a refresh between two queued writes on one item can hold the second, as the post-write run already could.
