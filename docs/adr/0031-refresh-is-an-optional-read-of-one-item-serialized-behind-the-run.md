---
status: proposed
---

# Refresh is an optional read of one item, serialized behind its source's run

After a write lands, knobas has so far caught the mirror up by starting an incremental run of the whole source (`refresh` in `crates/knobas-app/src/sources/write_queue.rs`, story 15). That run is skipped on the scheduler's flush tick, *Flush now*, *Apply anyway* and after an *amend*. It can also join a run that started before the write. So a write that went out can still draw *queued* until the next scheduled run, 15 minutes by default for Jira, Confluence and Gitea. `Source` had no read of one item: `sync` was its whole read side.

Decided 2026-10-25 (Björn, refresh-after-write grilling): **a [refresh](../../CONTEXT.md#refresh) is a new, optional `Source` read of one item by id. It answers the same `SyncItem` a run emits, goes through the same item write, and takes the source's run lock instead of checking an item version.**

- **Optional.** An adapter may leave it out. A built-in adapter that cannot read one item, or an external one built against an older WIT minor version, falls back to a run: its detail draws *Sync now* where others draw *Refresh*, and a landed write is followed by the run alone. In the WIT world (ADR-0028) it is an additive minor version and a §10.8 entry.
- **After every landed write, from every send path.** The item the write made (by the receipt's `remote_id`) or wrote to (a comment's page) is refreshed first, and then a run of the source follows, waited for the way `sync_after_write` waits rather than joined. The run is for what the write changed elsewhere: a merge's branch and commit, a transition that moves an epic.
- **Serialized, not versioned.** A refresh waits for a run of its source that is already in flight and reads after that run commits. Item writes overwrite unconditionally (`ITEM_UPSERT`), and a run fetches inside its transaction, so a run that began before the write could otherwise commit an older payload over the refresh.
- **A run's per-item work, for one item.** Links on arrival, a monitor's samples and alerts, the context follow and the saved-view projection, plus an event the shell re-reads on. No "synced" activity line. An id the source no longer answers for is tombstoned, as Gitea's probes already do. For Kuma that is what draws a paused monitor.
- **A failed refresh is only a failed read.** The write stays `sent`, its [echo](../../CONTEXT.md#echo) says *sent*, and the next run settles it.

## Considered options

- **Keep the run after a write and fix the paths that skip it.** That fixes the hours-long case, but a run of a whole Jira is not instant, and the detail's *Refresh* (read only what I am looking at) has no run-shaped answer. The run is kept as the follow-up, not as the fix.
- **A version guard on the item write**, refusing a payload older than the stored one. Rejected: Jira's `/search` rounds `updated` to the second where `/issue/{key}` keeps milliseconds, so the two reads of one issue cannot be ordered reliably, and every adapter would have to declare what "newer" means.
- **A required read.** Rejected: it would refuse every external adapter whose API cannot read by id, and Kuma has only `/metrics`.
- **Write the write's own payload into the mirror.** Rejected: the receipt carries an id and nothing else, and what the source made of the write (rendered macros, mentions, its own timestamps) is the point of reading it back.

## Consequences

- ADR-0003 is unaffected. A refresh never sweeps and moves no watermark, so the next incremental run delivers the item again inside its overlap window. Item writes are idempotent.
- `sync.item` gains a second writer beside `run_once`. It shares the run's lock and the run's item path, so the "only a run writes the mirror" reading of `knobas-sync/src/lib.rs:3` becomes "only the sync crate writes it, under the source's lock".
- Hold detection compares mirror projections, so a refresh between two queued writes on one item can hold the second. The post-write run already could. This adds no new case.
