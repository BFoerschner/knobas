# 2026-11-20: ⌘R on a repository reads that repository

**Asked (Björn, through the orchestrator):** "is it possible, when I am on a repository, to refresh only it and not
everything when pressing cmd+r? Because right now it takes minutes until it does."

## What ⌘R did

⌘R presses the page's own button (`tf/page-refresh.svelte.ts`): a detail's *Refresh* where its source reads that kind
of item by id, else the detail's *Sync now*, else the role header's *Sync now*. Gitea read a pull request and an issue
by id, and no repository, branch or commit. So a repository page and every tab in it (Files, Commits, Branches, a
commit, Compare) drew *Sync now*, and ⌘R there ran the whole Gitea source: every repository of the estate, each one's
branch listing, pull requests, issues and moved branches' commits, at the source's quota, in one transaction holding the
source's lock for the whole run.

Per page, before this pass:

| Page | ⌘R was | Now |
|---|---|---|
| A repository, its tabs, a commit, a branch (Gitea, Forgejo) | the whole source's run | that repository walked as a run walks it |
| A pull request, an issue, a Jira ticket, a Docs page, a run, a monitor | that item | unchanged, and named in its toast |
| A pipeline (TeamCity build configuration) | the whole TeamCity run | unchanged (Fable: a run of three queries, no per-configuration position) |
| Tickets, Builds, Docs home, Code's list, Monitors | the role's *Sync now* | unchanged (Fable: a list's truth is the whole role) |

## Measured

The windowless backend (release with debug assertions) on an APFS copy of the saved scale database, its Gitea source
on the testenv's live Gitea 1.27.3 (58 repositories of the scale estate and the seeded four), on the Mac under its
timing queue, nothing changed on the forge between runs:

| | time |
|---|---|
| before: ⌘R on a repository = a run of the Gitea source, three runs | 44.6, 63.1, 61.5 s |
| after: ⌘R on `scale-commerce/cart-service`, three presses | 0.35, 0.34, 0.51 s |
| after: ⌘R on `scale-platform/auth-gateway` | 0.68, 0.77, 0.76 s |
| after: ⌘R on `tidewater/payout-service` | 0.78, 0.67, 0.78 s |
| after: ⌘R while a run of the source goes | 0.04 s (*is syncing now*, the page read again) |

The first run on the copy, catching up five days, wrote 80 items in 34.4 s. Björn's estate on his work laptop is
larger and further away; the run grows with it, the walk with the one repository.

## Ruled

By Fable, the pass's one decision agent (xhigh), recorded as
[ADR-0047](../adr/0047-a-refresh-under-a-parent-walks-it-as-a-run-and-moves-its-position.md):

1. **An optional SPI read, `Source::refresh_under`**: the run's walk of one parent from where the stored position says
   it stood. Gitea answers it for a repository, a branch and a commit; those kinds declare `refreshes: true`.
2. **Under the source's lock, taken only if free**, the cursor read inside it; items through the run's item write, the
   per-item work, the cursor in the same transaction; no sweep, no "synced" line.
3. **The repository's part of the position moves with its rows.** Required, not an economy: Gitea's change gates
   compare the forge with the position, so a walk that wrote rows and left the position would keep a branch tombstoned
   that the forge restored at the same head. No usable position: not offered, the run instead.
4. **Never beside a run.** A press during a run writes nothing to the mirror, reads the page again at once and presses
   once more when that run ends, while the page still shows the item.
5. **Lists keep the role's *Sync now*.** Why an incremental Gitea run takes a minute, holding the lock across the
   network, is its own ticket.
6. Identity first; the repository read by name answers the listing's record (checked on the live Gitea: identical for
   `scale-commerce/cart-service`), so its fingerprint does not flip.

## Shown

The page's *Refresh* holds the width of its widest word (`.tf-keepw` with a second word), so *Refreshing…* moves
nothing beside it; the role's *Sync now* and a detail's *Sync now* the same. What the press reads is said in its one
toast, *Refreshing payout-service…*, which the end replaces in place: *Refreshed payout-service from Tidewater Gitea*.
A failure drops the toast and keeps its one line under the header (`TfDetailBanners`), as before.

## Tests that fail before

- `knobas-source-gitea` `refresh_under.rs`: a push walks its repository alone and moves only its position; a branch the
  walk tombstoned and the forge restored comes back on the run (fails with the position left behind, checked by
  mutant); an unchanged repository emits nothing and moves nothing; a branch and a commit walk their repository; no
  position or a pull request is not offered; a repository answering 404 is its tombstone and leaves the position.
- `knobas-source` contract battery clause 13: a walk is accepted per kind, and one that finds things or moves the
  position on an unchanged source is rejected (three self-tests).
- `knobas-app` `tests/it/refresh.rs`: a refresh under a parent writes what it walked and moves the position in the one
  transaction; a walk that found nothing leaves the position; no walk while a run holds the source.
- `TfHeaderRefresh.test.svelte.ts`: a press during a run reads the page again at once and presses once more when that
  run ends, and only once, and not for an item the page left; a named press's toast is replaced in place; a failure
  drops it; the button holds its width. `TfRepo.test.svelte.ts`: ⌘R on a repository names it. Each killed by a mutant.
