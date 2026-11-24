# 2026-11-24: A pull request's state shows at once

**Found (Björn).** "It takes too long to update when I create a pull request or merge it: it doesn't immediately show
as open, closed, merged." Measured in the real app (the Linux build under Xvfb on the WSL box, the live Gitea, seed
size, idle):

| Press | Before | After |
|---|---|---|
| *Create pull request*: the list's row says *Open* | 679 ms (queued row at 103 ms said *Queued*) | in the draw of the queue's answer (106 to 284 ms after the press, the tab's first draw included) |
| *Merge* on the PR page: the header says *Merged* | 943 ms | 82 to 143 ms, the draw of the queue's answer |
| *Merge* while a run of the source is going | 3,544 ms (a run is 2.5 s at seed size, 74 to 242 s at Björn's scale) | 143 ms |
| *Close*, *Reopen* | about as merge | 117 ms, 102 ms |

**Cause.** Every surface drew what its read said, and the read moved only once the forge had answered the write and
the item had been read back (ADR-0031). That read-back takes the source's run lock, so it waits for a run going at
the time, and every landed write starts a run (`sync_after_write`): create-then-merge always waits for the create's
run. The repository's *Pull requests* tab reads only the mirror, so it waited for the same read-back.

**Ruled (Fable, xhigh, while Björn is away).**

1. **An overlay, as the ticket move's** (`tickets/echo-board.ts`): a merge, close or reopen queued in knobas draws the
   state it leaves at once on every surface that draws the pull request (its header, the repository's *Pull requests*
   with its filter and count, Code's rows, the Branches tab, a ticket's connections, Everything's monogram),
   `app/src/lib/tf/pr/pr-state.svelte.ts`. **One clock, the window's**: the move is drawn over a read that still says
   the old state until the write's read-back ended (`item:refreshed` naming the write, or the end of the first run of
   its source that began after the send); a surface that says when it asked its rows draws rows asked after that as
   read, one that does not stops 10 s after it. Never the forge's `updated_at`, whose clock is not the window's
   (Fable's veto). A held, refused or withdrawn write takes the move with it. **The stuck case**: once the read-back
   ended, the PR page reads its row again; if it still says the old state, the forge took the write and did not move
   it, and the move and its echo go.
2. **The PR page reads its live view again when the queue says the write was sent**, not waiting for the read-back
   behind a run.
3. **A new pull request's queued row says *Open*** at once, with *Queued* then *Sent* where its time goes.

**Owed by the same ruling** (later commits on the branch): a landed write's read-back stops a scheduled run in its way
(ADR-0031 amended); the repository page walks its repository when shown (ADR-0047's refresh under a parent), so a
change made at the forge shows on *Pull requests* when it is opened; the PR header's chip holds its width, the PR page
refreshes the mirror's row where its live view says another state, and reads again on its return. No polling.

**Reversing it** is `pr-state.svelte.ts` and its callers (`TfPr`, `TfRepo`, `TfCode`, `TfRepoBranches`,
`TfConnections`, the launcher's `Results`).
