---
status: accepted
---

# A run id always comes with an ending

`Scheduler::trigger` deduplicates: a source already running is not started again, and the id of the run in flight comes back instead, which is what makes a double-click on *Sync now* harmless. But the caller's progress sink was discarded on that path, so a caller handed an id then heard nothing — and the first-run wizard, whose only source of both its phase and its item count is that channel, would sit on "started" for ever with no timeout. The same silence hid a second defect: the wizard also raced `add_source`'s `scheduler.wake()`, so *"knobas mirrored N items"* reported whichever run the wizard happened to own, which on a slow machine was the one that found an already-mirrored corpus and wrote nothing. Decided 2026-08-29 (Björn, grilled): **a run holds a set of sinks, and any caller that receives a run id receives an ending for it** — including a caller that attaches to a run which has already finished, which is served a terminal message synthesised from the recorded outcome, faithfully, failure phase and stored error text included.

The invariant is the point: an id you cannot observe is not a useful id. Every caller of `trigger` gets the same guarantee whether it started the run, joined it mid-flight, or arrived after it ended, so no caller has to know which of those happened.

## Considered options

- **Let each caller read the run's outcome itself** when it detects it was deduplicated — returns enough state and pushes the rest onto the caller. Rejected: it hands the same race to every caller in turn, and each will get it wrong differently. The wizard's version of getting it wrong is a success panel over a failed sync, since its Retry/Skip flow is driven entirely by the failure phase on that channel.
- **One sink per run, last attach wins** — simpler, and reintroduces the same bug for whoever attached first, silently displacing the scheduler's own reporting.
- **One sink per run, first attach wins, later attaches refused** — refuses the wizard again, which is the case that prompted this.
- **A timeout in the wizard** — treats the symptom, and masks the regression it guards against: if the ending contract breaks, a red test is wanted, not a wizard that quietly shows "taking longer than expected". A long-but-working first sync of a large Jira is indistinguishable from a hang by wall-clock, so any threshold is either too short to be safe or too long to help.
- **Stop `add_source` waking the scheduler**, leaving the wizard's run the only one — fixes the race by removing a deliberate behaviour: the wake exists so a brand-new source is not idle until its first tick, and a source added outside the wizard would regress.

## Consequences

- A sink whose listener has gone away is discarded silently and never fails the run: liveness of a webview stays irrelevant to a sync, as `ProgressSink` already required.
- The first-run wizard triggers `FirstRun` rather than `Manual`, so a first sync carries that spelling whoever starts it and the run log stops recording who won a race. This is the same reasoning that gave a backfill its own spelling in migration 0004.
- User-facing **"mirrored N items"** means the **corpus**, read from `SourceSummary.item_count`, not a run's `Upserted` delta — so the sentence is true regardless of which run the wizard observed. `CONTEXT.md` carries the distinction.
- No frozen surface changes: `crates/knobas-sync/**` is not in contract §10.8's list, `item_count` already crosses IPC, and no command or event shape moves.
- The contract needs its own test — a caller attaching to an already-finished run receives an ending — because it is precisely the kind of invariant a later reader who sees only the happy path will simplify away.
