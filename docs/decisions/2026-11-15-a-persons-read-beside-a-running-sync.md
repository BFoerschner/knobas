# 2026-11-15: A person's read beside a running sync, and a repository page left open

**Asked (Björn, the ux13 Windows pass, PR #2479):** on his PC against the live Gitea in WSL, a forge read knobas had never
made took 2.6 to 9.2 s at 4x while a Gitea sync ran every 45 s, against 100 to 240 ms with no sync. The limiter fix of the
ux12 pass held (no "local rate limit" stop), but the read still waited. And a repository page left open sent the forge 17
requests a minute. Find where each comes from and cut it; measure on the Mac against a stand-in forge with a sync running.

## Where the wait came from

The priority lane (2026-11-13) holds back the sync's **next** requests while a person's request waits or is in flight. It
did nothing about the ones already on the server. A Gitea run walks eight repositories at once (`concurrent_repos`), so
it kept up to eight requests on the forge. A forge that works through its requests in turn (the WSL Gitea answered a
branch's view in 300 ms alone and four at once in 1 to 2.8 s; a run kept eight requests there at ten a second, so each
took about 800 ms in the forge) answered a person's request after the sync's eight. A read that needs two or three rounds
(a file's entry, then its bytes) paid that once a round. And in the moment between two rounds, when no request of the
person's was in flight, the sync's waiting requests went first, so each round met a freshly loaded forge.

Ruled out on the way: the connection pool (each `HttpClient` has its own), the sync's after-run passes (stored trees run
on the run's task, in the background lane, and so does any later pass there), the database (a page's reads wait on no
run's connection), the quota (a person's request takes the next one ahead of the sync's, and the sync leaves half the
burst).

## What changed

- **The sync keeps at most two requests in flight per server** (`knobas_http::BACKGROUND_IN_FLIGHT`). A background
  request takes one of two places before its wait on the quota and gives it back when its answer is in (`Sending`, held
  by `HttpClient` and the wasm host over the request). A person's read meets two of the sync's requests on the forge,
  not eight. The wait for a place is the forge's time on the sync's earlier requests, so it is not counted against the
  request's 45 s budget (at most the five-minute stand-aside ceiling more): a slow forge cannot fail a run on knobas'
  own wait, which was the ux12 bug.
- **The sync stands aside 150 ms past a person's last answer** (`INTERACTIVE_LINGER`), so the next round of the same
  read reaches the forge before the sync's waiting requests. Only after a request held over its answer: one that only
  waited on the quota has no next round. Inside `BACKGROUND_FLOOR`, so the sync still gets about a request a second
  however a person's requests come (pinned: `the_linger_holds_the_sync_no_longer_than_the_floor`).
- **What it costs the sync.** Nothing against a forge that answers in 50 ms (two in flight carry 40 requests a
  second). Against one that takes 300 ms an answer and answers many at once, a backfill of 334 requests took 50 and 51 s
  where it took 36, 41 and 42 s: about 6.7 requests a second, under a quota of ten. Against a forge that works through
  its requests in turn, the forge was the bound either way (49 s before, 38 and 59 s after). Fable ruled the value
  (below).

Pinned on tokio's paused clock (`limiter::tests::lane`): `a_sync_keeps_at_most_two_requests_in_flight` (red before: all
eight at once), `a_persons_next_round_finds_the_sync_still_aside` (red before: the sync's request went between the
rounds), `waiting_for_a_place_in_flight_does_not_count_against_the_wait`, the floor test above, and
`the_sync_stands_aside_until_a_persons_answer_is_in` moved by the linger. The 29 earlier lane and quota tests pass.

## A repository page left open

Found by counting what the stand-in answered while the page was told a run of its source ended every 40 s
(`testenv/perf/page-requests.mjs`):

- **Every run's end re-read the branch list**, whether the run found anything or not, and the page's read behind it
  told `view:fresh`, which the page answered with a second read, which the backend read behind again: the branch list
  (three pages on the scale repository) and its checks twice a run. Now the page re-reads on a run's end only when the
  run brought something (`last_changed`), and while the window is hidden it notes such a run and reads once on its return
  (`TfRepo.svelte`). The backend answers a page's read within five seconds of a tell from the answer it just read, once,
  with no read behind (`ECHO_WINDOW`, `live.rs`).
- **The header's *New pull request* look read each branch's whole view twice** after a run: the branch list and the pull
  requests land a moment apart, each started the look again, and each look read the branches the last one was still
  reading. A look now waits for a read already out for that branch.
- **Write grants were read behind on every ask** (Gitea's: two probes on the forge). Now only once the kept answer is a
  minute old (`GRANTS_FRESH`), and a source's kept grants go at once when one of its writes is refused, as on a new
  credential.

Requests a minute to the forge, the page left open on `scale-commerce/cart-service`, a run's end every 40 s, two
minutes each:

| Runs' ends | before | after |
|---|---|---|
| none brought anything | 16 | 0 |
| every other one brought something | 19 | 15 |
| every one brought something | 34 | 25 |
| every one brought something, window hidden | 34 | 0 (30 once on its return) |

Of the 30 for the first run with news, about 20 are the look's first read of the two branches the forge lists and the
mirror does not (each branch's view lists the branches and reads its checks); later runs with news cost about 11 each,
the branch list read behind and the open tab's reads behind. Pinned: `TfRepo.kept.test.svelte.ts` (a run that changed
nothing reads nothing; hidden, one read on return; red before: 6 reads where 3 were expected),
`TfRepo.test.svelte.ts` (a look started again asks each branch once; red before: 6 asks for 2 branches),
`live_reads::the_pages_read_of_a_told_view_is_not_read_behind_again`, `live_reads::grants_are_kept_and_read_again_behind`.

## Found on the way: a sync after an upgrade deadlocked with the fill

On a copy of the scale home just migrated, one Gitea backfill in three failed with `deadlock detected`: bring-up's fill
of the stored rows (`sync.fill_item_runs`, and the same shape in the cards' and list rows' fills) inserted rows for items
a running sync had written and not committed, and the sync then inserted rows the fill had inserted and not committed;
`on conflict do nothing` waits on both. Migration `0091` replaces the three fills: each batch takes its items `for share
skip locked`, so it steps past what a running sync holds and never waits on it, and the fill passes again, two seconds
later, while a pass stepped past any (`knobas_core::fill`), since a run that locks an item it leaves unchanged writes no
row for it. Pinned: `item_run::the_fill_steps_past_an_item_a_sync_holds_and_fills_it_after`. The three fills' migrations
are on this branch only, so `main` never had it.

## Measured

The windowless backend (`examples/headless_ipc.rs`, release with debug assertions), before (`0998d62d`) and after, each
run on a fresh APFS copy of the saved scale home (Gitea pointed at the stand-in, the other sources' secrets left out),
against `standin-forge.mjs --repos --sync --workers 1 --delay 40 --heavy 100`: one request answered at a time, a page's
routes 40 ms, the listings a run walks 100 ms, so the forge keeps up with the quota and no more, as the WSL Gitea did.
A backfill kept running (`syncing.mjs --mode backfill`, through `lane-probe.mjs`), 24 reads knobas had never made: a
file at the default branch, a branch's whole view, a commit's page, in eight repositories. Settled ms, median / worst,
two runs each, alternated (before, after, after, before), and the same with no sync:

| Read | no sync | before, sync | after, sync |
|---|---|---|---|
| a file | 612–708 / 966 | 2,920 / 4,705 · 912 / 4,224 | 598 / 823 · 660 / 1,040 |
| a branch's whole view | 1,438–1,881 / 2,718 | 2,923 / 4,104 · 1,942 / 3,321 | 1,364 / 1,709 · 1,350 / 2,167 |
| a commit | 322–356 / 683 | 680 / 1,360 · 400 / 729 | 459 / 637 · 277 / 624 |

With the sync running, a person's read is back at what it takes with no sync. A branch's view at 1.4 s is the view's own
chain of rounds on a one-hand forge (record, tips, compare, checks, grants), with or without a sync. The machine carried
other agents' work (a load of 15 to 60). Before the stand-in modelled the forge's queue (two hands, every route 200 ms),
the sync added about a second and the person's own fan-out the rest, which is why `--workers` and `--heavy` were added.

## Fable's ruling (2026-11-15, xhigh)

Asked: the cap's value, the linger's, whether to fix the fill's deadlock in this PR, and the page's change.

- **Keep two, for every adapter**, one constant: at two, the reads sit at the no-sync reference; one saves at most one
  100 ms listing and makes a far forge's backfill about 2.5 times as long. No per-adapter value until a real far forge
  shows the need. Name the 6.7 requests a second at 300 ms in the contract (§4.1, done) and correct
  `concurrent_repos`' docs (done).
- **Keep 150 ms**, with a test that the linger never holds the sync past the floor (done). Raise it only if the Windows
  6x pass shows a gap between rounds over 150 ms.
- **Fix the deadlock now, as its own commit, migration `0091`**, with the fill passing again until a pass steps past
  nothing, and a test of a locked, unchanged item filled once the lock is gone (done).
- **No objection to the page's change.** Grants get the minute's window, dropped on a refused write or a new credential
  (done).

## Owed on WSL

- The felt waits with the live Gitea syncing every 45 s on the scale estate (`lane-probe.mjs` with these probes'
  shape, or the inner pages' first opens at 4x), before (ux13) and after (ux14): the target is the no-sync time.
- A Gitea backfill's length on the scale estate, before and after, to see what two in flight cost against the WSL
  Gitea.

## Owed on Windows

- A repository page left open 60 s with the sync loop running, counted through the proxy as on ux13 (17 then).
- The inner pages' first opens at 1x, 4x and 6x with the sync loop running.
- A sync right after the ux14 upgrade: no `deadlock detected` in the log while the fills run.

## What reversing it costs

The cap is `BACKGROUND_IN_FLIGHT` (raising it to `usize::MAX` is the old behaviour); the linger is
`INTERACTIVE_LINGER` (zero is the old). The page's change is one condition in `TfRepo.svelte`'s run-end listener; the
echo is `ECHO_WINDOW`; the grants' window `GRANTS_FRESH`. Migration `0091` cannot be taken back, only replaced.
