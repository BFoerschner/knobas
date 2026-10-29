---
status: accepted
---

# A run id always comes with an ending

`Scheduler::trigger` deduplicates: a source already running is not started again and the in-flight run's id comes back instead. That path discarded the caller's progress sink, so the first-run wizard, whose phase and item count come only from that channel, could wait on "started" for ever. Decided 2026-08-29 (Björn): **a run holds a set of sinks, and any caller that receives a run id receives an ending for it.** An id you cannot observe is not a useful id.

- A caller that attaches to a run that already finished gets a terminal message synthesised from the recorded outcome, failure phase and stored error text included.
- A sink whose listener has gone away is discarded silently and never fails the run. Webview liveness stays irrelevant to a sync, as `ProgressSink` requires.
- The first-run wizard triggers `FirstRun`, not `Manual`, so a first sync carries that spelling whoever starts it.
- "Mirrored N items" means the corpus, read from `SourceSummary.item_count`, never a run's `Upserted` delta. `CONTEXT.md` carries the distinction.

## Rejected

- Each caller reads the run's outcome itself when deduplicated. Hands the same race to every caller.
- One sink per run, last attach wins. Silently displaces whoever attached first.
- One sink per run, first attach wins. Refuses the wizard, which is the case that prompted this.
- A timeout in the wizard. Masks the regression; a long first sync and a hang look the same by wall-clock.
- Stop `add_source` waking the scheduler. Removes a deliberate behaviour: a new source would idle until its first tick.

## Consequences

- No frozen surface changes: `crates/knobas-sync/**` is not in §10.8's list and `item_count` already crosses IPC.
- The contract has its own test: a caller attaching to an already-finished run receives an ending. A later reader who sees only the happy path would simplify it away.
