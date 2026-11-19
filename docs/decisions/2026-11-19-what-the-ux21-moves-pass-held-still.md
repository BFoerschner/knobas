# 2026-11-19: What the ux21 moves pass held still

**Asked (the ux20 compare pass's leftovers, for ux21):** two things on a repository's landing that moved once drawn, a
probe item that could never pass, and a Compare test that failed once in six full runs under load 15.

## A file's actions wait for the checkout

**Seen.** On a repository's first open at 4x (1440x868 and 2560x1440), the README header's *Render as Markdown* and
*Pipeline* moved 59 px left when *Clone* came in beside them. `TfOpenIn` draws nothing while the checkout is unread, and
the file header's actions (`.tf-trail`) stand at the row's end, so a button arriving late pushed the ones before it. The
repository's header already waits for the same read (`held-head.svelte.ts`, `checkoutAnswered`); the file view did not.

**Taken.** The file header's actions are held unseen in their room (`tf-heldbelow`) until the checkout's read has
answered or failed (`TfRepoFiles`' `checkoutRead`, the page's `checkoutAnswered`). They show in the frame the page
header shows, whole. The path and the file's lines draw at once as before.

**Held by** `moves.mjs`' `repo-checkout-late` (the fixture: `ledger`, not cloned, its views at once and its checkout
450 ms late, since the fixture's checkout otherwise answers at once and the live run lands it either side of the README).
Before, at 4x and 1440x868: *Render as Markdown* and *Pipeline* 59 px left as *Clone* came in at 400 ms. After: nothing
moved in 18 opens (1x / 4x / 6x, first and return, at 1440x868@1, 2560x1440@1.25 and 5120x1440@1).

## The way to the history keeps its room

**Seen.** On a return at 5120x1440 (and at 1440x868 at 4x), the last commit's *History* became *438 commits* 50 ms after
it drew, and the sha and the time beside it moved 35 px left. A repository's `commits` section asked for the first
time is the mirror's (`from_branch_records`): its default branch's tip and no count, since the mirror holds none. The
source's read behind it brings the count, and the strip reads again on `view:fresh`. Whether that lands inside the
probe's five seconds of the first open or after them, on the return, is the forge's timing.

**Taken.**

- The button keeps a room as wide as *8,888 commits* from its first frame, its label centred in it (`.tf-lastc-all`,
  a hidden `::after` in the label's cell), so the count arriving widens nothing below 10,000 commits.
- A held answer (`held_at`) with no count draws the count this place was last read with (`last-commit-counts.ts`), so
  a return in the session says *438 commits* at once. A fresh answer with no count still says *History*.

**Measured** with `moves.mjs --real --only repo` (the repository and each of its tabs, first open and return) on fresh
copies of the saved scale home, the windowless backend on the Mac against the live Gitea. Before, at 4x: the return moved
the sha and *438 commits* 35 px at 1440x868@1 and at 5120x1440@1. After: nothing moved in 144 opens, 48 at each of
1440x868@1, 2560x1440@1.25 and 5120x1440@1, at 1x, 4x and 6x (Chrome on the Mac for 5120, on the WSL box for the others).

## `compare-large` measures a large compare again

**Seen.** The inner probe's `compare-large` was never useful: it waited for three diff cards on screen. Its branches
change 8, 13 and 8 files (`fix/ledger-writer-49`, `fix/data-warehouse-loader-69`, `chore/cart-service-5`, as Gitea's
compare and knobas's `entity_range_files` both list them), but Compare's form stands above the diff, so at 1440x868 one or
two cards reach the window whatever the branch changes.

**Taken.** The place is useful when the form is drawn, at least three rows of the diff's file list are drawn wherever
they stand, and the first file's lines are on screen. Measured at 1x on the live Gitea: never useful before, **221 ms**
on its first open and 8 ms on its return after.

## The read-ahead test counts by pair

**Seen.** *The names of the next picks are read at the clone while the page is idle* counted the page's asks for names
after a wait, then waited for one more. The read-ahead goes out on a 100 ms timer (jsdom has no idle callback); on a
loaded machine the wait's zero-delay timers came due after it, so the first read-ahead was counted with the pick's
own, and the test waited 10 s for an ask the page makes only once that one is answered. A 150 ms pause at that point
failed it every time.

**Taken.** The test and its neighbour (*nothing is read ahead from a list the forge answered*) take the read-ahead's
asks as every ask but the pick's own pair, whenever they come. The same pause passes; the read-ahead's order
reversed, or a read-ahead from a list the forge answered, fails them.

## Seen, not this pass's

- A README's image read from the repository (`payout-service`'s `docs/img/logo.png` on the fixture) moves the text under
  it 15 px on its first load in a session, when the 4x fixture draws the text first: its size is kept for every later
  draw (`tf/markdown/image-sizes.ts`), and none is known before its bytes. `repo-checkout-late` opens `ledger`, whose
  README has no image, so it measures the checkout alone.

**Reversing it** is the trail's `tf-heldbelow`, the strip's `tf-lastc-all` and its kept count, and the probe's old
`useful` taken back; each place then moves or measures as above.
