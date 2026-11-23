# 2026-11-23: Sync now on Tickets sorts the board again

**Found (the ux30 end gate, the real app on WSL's Linux build).** Tickets, *All tickets*, the filter holding "ledger". A
new Forgejo issue, then the page's *Sync now*: the run brought the issue into the mirror (the launcher found it) and the
board read it, but the card did not draw for 3 minutes, until a reload. The card sorts on top of Open, above drawn cards,
so the still-rows hold (`2026-11-11-rows-stay-where-they-drew.md`) counted it behind *Show 1 new* in the filter bar's
count, which nobody saw. A Gitea issue minutes earlier drew only because the filter was typed after it synced. Not a
cache, not a missed refresh, nothing of Forgejo's: every release since the still-rows pass does it (ux29, ux30, ux31).

**Ruling (Fable, xhigh, while Björn is away).** The page's own *Sync now* (the header's button, and ⌘R, which presses it)
is the person's act. When the runs it started have all ended, the board reads again and sorts again in the draw that read
lands in, as *Show N new* would: cards and columns on the board, rows on List. What waited from earlier runs comes in
with it, a ticket a sync moved leaves its old column, and *Show N new* and *Sort again* clear. It sorts whether or not
the pointer rests on a card, and with nothing new nothing moves. A drag in flight holds it until the drop. Pressed, then
left, the page sorts on its return.

Every other run holds as before: a scheduled sync, a write's follow-up, *Sync now* on another page or a detail, and
Settings' *Sync all*. Their tickets wait behind *Show N new*.

**Built.** The role frame (`TfKeptPage.svelte`) hands a role view `syncAsked` beside `syncRevision`: it moves only with
the role's own *Sync now* (`RoleSync.revision`), held while the page is hidden. Tickets reads again on either; on
`syncAsked` the reader's `refresh(true)` keeps the depth a sync's re-read keeps and moves `asked` (the held order's
basis) when that answer lands, the list's first page where the list pages. The flag holds until an answer lands, so a
background run's read that overtakes it still sorts. Tests: `TicketsPage.test.svelte.ts` (board and list, each failing
before), `TfPage.test.svelte.ts` (`syncAsked` moves on the press alone).

**Left for Björn.** The Builds pill (`2026-11-12-runs-newest-first.md`) on Tickets for background arrivals, since *Show N
new* has now been missed twice; the same press rule on Code, Monitors and the Docs home; lighting the cards a pressed sync
brought, as Builds lights its runs.

**Reversing it** is the `syncAsked` effect in `TicketsPage.svelte` (read with `refreshBoard` alone).
