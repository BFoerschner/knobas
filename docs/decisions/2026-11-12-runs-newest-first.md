# 2026-11-12: Runs are newest first, always

**Asked (Björn, on ux6):** "buildruns aren't sorted by date anymore? how am i supposed to find the most recent run which is the most important anyways?"

**Found.** Two things put runs out of date order.

- **The read's order was never the date.** The mirror answers a pipeline's runs by the source's last word on each run (`item_updated_at`; for a TeamCity build its finish, else its start, else its queue time). A long build that finished after a newer short one sorted above it, and the day groups, which go by start, came apart.
- **The still-rows hold (2026-11-11, `2026-11-11-rows-stay-where-they-drew.md`) kept them out.** A run a sync brought waited behind *Show N new* in the filter bar's count, on a kept page for as long as the page lived: a return to Builds an hour later drew the runs of an hour ago on top, and the new ones behind a count he did not see.

**Taken.** Every list of runs is newest first by when the run started, or, not started yet, when it was queued (`newestFirst` in `tf/builds/builds.ts`): a pipeline's run history and its chart, and the rail's strips and latest-run times. The hold on a pipeline's runs is for the pointer alone.

- **On a first draw, a return and a sync**, the runs draw newest first. A sync that lands while the page is away, or while the pointer is anywhere but the runs, puts its runs on top at once, and a run that came in that way is lit once as it draws (an `--info-bg` fade, none under reduced motion).
- **While the pointer rests on the runs**, a run a sync brings waits, so nothing moves under it, and is not hidden away: *Show 1 new run* stands over the list's top, under the sticky filter bar, in the inverted ink the rail's selection uses, taking no room. Pressing it brings the runs in and the top into view. Taking the pointer off the list brings them in by itself.
- The rest of the still-rows rule stands: a run you started is your own press and sorts at once, and Code, Tickets, the Inbox, Monitors and Docs hold as they did.

**Reversing it** is the pointer condition in `TfPipeline.svelte` (`pointerOn`) and `newestFirst` where the runs and the rail's pipelines are read. The rows-still trip `builds-rows-still` rests the pointer on the runs for its *shown* way and reads the pill.
