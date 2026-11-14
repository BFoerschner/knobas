# 2026-11-13: A person's read goes ahead of the sync, for every adapter

**Asked (Björn, on PR #2479):** measured on ux9 against the live Gitea on the WSL box with the scale estate, a first open of something only the forge holds took 5 to 7 s, and a pull request's Conversation over 11 s, while Gitea answers each request in about 50 ms. The wait was knobas's own: the per-origin quota (#1630, `crates/knobas-http/src/limiter.rs`) is shared by the background sync and the read a person waits for, the sync keeps it full, and the person's read queued behind it. Fix it with a priority lane for every adapter: Jira, Confluence, TeamCity, Gitea and Forgejo, Kuma and wasm plugin sources.

## What was built

- **Two lanes, one quota.** Every request waits in a lane read from the task it is sent on (`knobas_http::on_lane`, `Lane`). A sync run (discovery, the run, the pipelines' latest logs after it), the write queue's tick and the links resolved after a run are **background**; everything else is **interactive**. The wasm host takes the calling lane with the request it hands back, since its future may be driven on another task. Every request of both lanes passes the same limiter, so the server sees no bigger burst and no more a second than the quota allows: pinned on tokio's paused clock by replaying every grant through the quota's own arithmetic (`limiter::tests::lane`), beside the earlier #1728, #1740, #1764, #1771 and #1925 tests, which pass unchanged.
- **The sync stands aside while a person's request waits or is in flight.** A background request yields while any interactive request for the same origin is waiting on the quota or, sent through `HttpClient` or the wasm host, waiting for its answer (`Sending`). The answer counts because the quota is not all a busy server spends: with a sync's eight repositories in flight, a Gitea that answers a pull request in 50 ms took 2.2 s.
- **The sync leaves half the burst.** A background request goes only while half the burst could still go at once after it, so a page's first requests go the moment they are sent even while a sync keeps the quota full, instead of one a refill apart (1.2 s for a dozen at Gitea's ten a second). Governor cannot hold one caller to a smaller tolerance, so the limiter is now a GCRA of its own on tokio's clock, the arithmetic governor did; `governor` is gone from the tree.
- **The sync is never starved.** Once an origin's background requests have stood aside for a second without one let through, the next one stops yielding and keeps no reserve. A person's requests that trickle leave the sync the rest of the quota; ones that fill it still leave it about one request a second.
- **A pull request's view is read in two rounds, not three**, and **Conversation draws its own section** (two quick reads) when the page shows a view the mirror drew and is on that tab, instead of waiting for the whole view's slowest read, the landing's compare.

## Where hover prefetch and the write queue's flush go

- **A hover's prefetch is interactive.** It is a person's reach for a click: the read the press needs, started 80 ms early so the forge's round trip overlaps the reach. In the background lane it would queue behind the sync and land after the click, which is the wait it exists to remove. It is one read per hover and at most once a session, so it costs the sync little, and the one-second floor bounds what any stream of interactive reads can take from it.
- **A write a person pressed is interactive; the queue's tick is background.** A press flushes its own write at once (`submit_in_background`, on a task of its own), and the person watches the toast for it. The tick re-sends what is owed (a write that failed, one queued while a source was down) with nobody waiting on it, so it yields like the sync. *Flush now* from the queue panel is a press, and interactive.

## Other places a person's read waited behind the sync

Looked for, per adapter, past the token bucket:

- **Connection pools and concurrency caps:** none shared. Each `HttpClient` has its own reqwest pool, built per run, read or flush; no semaphore holds in-flight requests per host. The scheduler's `SYNC_CONCURRENCY` permits are runs, not reads.
- **The database:** a run holds a dedicated connection outside the app's pool (§10.6(c)), so a page's reads never wait for one.
- **A per-source lock:** the live reads build their own source (`LiveSources`), whose slot lock is held across a build only. The source's run lock (`pg_advisory_xact_lock`) is taken by a refresh: the reader's *Refresh* answers *the sync is running* at once (`Lock::Try`, 2026-11-12), and a landed write's read back waits for the run (`Lock::Wait`). That wait is not felt: the toast says *sent* when the write leaves the queue, and the page draws the write's projection from the moment it is queued. Left as it is.
- **Kuma's socket:** the sync does not use it. A Kuma run is one `GET /metrics`; the socket.io session (`KumaSocket`, one per instance and account) is shared by writes and a monitor's *Refresh* under its own lock, and its polls go through `HttpClient`, so they wait in the caller's lane.
- **The wasm host:** paced by the same limiter (#1631); now in the caller's lane and holding the sync off until its answer's head is in.
- **The server itself:** what the sync's requests already in flight cost the forge is not knobas's to reorder. Stopping new ones while a person's are in flight is what the lane can do, and does.

## Measured

The windowless backend (`examples/headless_ipc.rs`, optimized with debug assertions, so it reads the dev profile's credentials), before (`1f84766e`, ux9) and after, each run on a fresh APFS clone of a profile made from the saved scale database with Jira and Confluence synced from their instances as filled for this pass, against the live products on the WSL box (Gitea, Forgejo, Kuma up; Jira, Confluence and TeamCity brought up and filled for it). Per adapter, the felt waits were timed at the backend with the adapter's own source kept syncing (`testenv/perf/lane-probe.mjs`), four rounds in the order before, after, after, before, each on items no earlier round had read. The Gitea load is the scheduled run (`sync_now`), which on the scale estate fills the quota at ten requests a second; a backfill does too. Counted through `slow-forge.mjs` over 30 s, the backfills of Jira, Confluence and TeamCity send about two a second, under their quotas of 20, 5 and 5; theirs is the load for those three, their heaviest run. Another session's backend was syncing the same Gitea every five minutes.

**Settled, ms, each round** (a view answered from what knobas held is timed until its live read lands):

| Felt wait | before | after |
|---|---|---|
| Gitea, a file, no clone | 1,049 · 405 · 428 · 383 | 160 · 405 · 179 · 408 |
| Gitea, a commit's diff | 395 · 400 · 515 · 396 | 146 · 509 · 147 · 498 |
| Gitea, a branch's view (ahead/behind) | 1,481 · 1,607 · 1,475 · 3,606 | 523 · 1,090 · 483 · 995 |
| Gitea, Compare | 231 · 206 · 220 · 216 | 210 · 211 · 216 · 219 |
| Gitea, a pull request's whole view | 4,025 · 2,791 · 8,916 · 2,383 | 1,662 · 2,275 · 1,663 · 4,144 |
| Gitea, a pull request's Conversation section | 147 · 3,295 · 176 · 595 | 226 · 210 · 228 · 138 |
| Gitea, a pull request's Files | 696 · 2,297 · 1,007 · 601 | 799 · 614 · 794 · 1,219 |
| Gitea, *Test connection* | 205 · 197 | 184 · 179 |
| Gitea, a comment, press to sent | 222 · 319 | 221 · 214 |
| TeamCity, a run's Log | 56 · 210 | 54 · 46 |
| TeamCity, a run's Tests | 188 · 195 | 30 · 43 |
| TeamCity, a run's Artifacts | 598 · 197 | 22 · 14 |
| TeamCity, *Test connection* | 598 · 397 | 34 · 39 |
| Jira, a ticket (its statuses and its thread) | 43 · 18 · 41 · 18 | 46 · 18 · 43 · 26 |
| Jira, *Test connection* | 41 · 41 | 41 · 44 |
| Jira, a comment, press to sent | 108 · 113 | 119 · 112 |
| Confluence, a page | 49 · 41 · 61 · 46 | 83 · 21 · 81 · 15 |
| Confluence, *Test connection* | 38 · 58 | 48 · 40 |
| Kuma, a monitor's *Refresh* | 31 · 36 | 34 · 35 |
| Kuma, *Test connection* | 117 · 110 | 119 · 125 |

- **Gitea** is where a sync fills the quota, and where the lane's order shows: a branch's view from 1.5 s to 0.8 s at the median, a pull request's whole view from 3.4 s to 2.0 s.
- **TeamCity's** backfill leaves its quota room, but its server is busy with the run's requests: holding the sync's next ones while a person's is in flight takes a run's tabs and *Test connection* from 0.2 to 0.6 s to under 50 ms.
- **Jira and Confluence** never queued a person's read: their runs send about two requests a second, each a long page the server takes its time over, well under their quotas (twenty and five), and their servers answered the person's reads in tens of milliseconds beside them. **Kuma's** run is one request. Their numbers are the same before and after, which is what the lane promises where nothing waited.
- **What still waits:** a pull request's whole view, 1.7 to 4.1 s with the lane, is the forge's time under the sync's load, on its slowest read (the landing's compare of head and base); its Conversation no longer waits for it (below). A first open of a file, a diff or Files is 0.15 to 1.2 s, the forge's own answer while it serves the sync's requests already in flight.

**The inner pages in headless Chrome** (`just open-budget --real <port> --inner --syncing live-gitea`), first open with Gitea's scheduled run kept going, ms unless marked, before → after, at 1x and 4x. Two pairs: in the first the after pass ran first, in the second the before pass did, so a forge that kept an item it answered once favours each side in one pair. Another session's own open budget ran beside the second after pass.

| Page | first pair, 1x | 4x | second pair, 1x | 4x |
|---|---|---|---|---|
| repository landing | 6.0 s → 534 | 3.4 s → 526 | 1.2 s → 373 | 1.5 s → 468 |
| a deep folder | 1.0 s → 805 | 8.7 s → 5.7 s | 4.9 s → 859 | 5.3 s → 3.0 s |
| a small file | 177 → 389 | 416 → 419 | 364 → 165 | 827 → 954 |
| a large file | 452 → 1.1 s | 410 → 2.3 s | 420 → 173 | 1.7 s → 513 |
| a binary file | 381 → 221 | 1.4 s → 1 | 2.4 s → 160 | 2.8 s → 646 |
| a commit | 2.9 s → 1.2 s | 129 → 2.0 s | 89 → 1.3 s | 3.9 s → 385 |
| Compare | 260 → 1.6 s | 6.7 s → 218 | 438 → 850 | 298 → 1.2 s |
| *New pull request* | 239 → 434 | 5.7 s → 5.7 s | 2.6 s → 257 | 1.2 s → 2.7 s |
| a pull request's Files | 7.8 s → 498 | 874 → 532 | 1.7 s → 920 | 1.0 s → 399 |
| Files of a larger one | 5.0 s → 360 | 1.1 s → 650 | 1.4 s → 410 | 3.3 s → 423 |
| Conversation | never → 26 | never → 28 | 28 → 28 | 24 → 30 |

What knobas holds (Go to file, a README, Branches, Commits, a pull request's Overview and Commits, a run's Tests and Artifacts) opens within 157 ms on every first open of all four passes, and every return is within 200 ms. Over the two pairs, 25 first opens of what only the forge holds took a second or more before (15 of them 2.4 to 8.7 s), and 11 after (1.1 to 2.7 s, and 5.7 s twice at 4x). The return after every source's run re-reads a file at its branch from the forge: up to 7.9 s before, 3.4 s after. A single press is the forge's answer while it serves the run's requests already in flight, which moves with the item and the moment: the lane takes the queue out of it, not the forge.

**Lines that fail, and why.** Compare on the branch with the most changes is never useful within the probe's cap in any pass: its side panel waits on each branch's whole view (the rig's cap, as on 2026-11-12, and the forge's time). A run's Log is never useful in any pass and makes no read at all: the rig's item, not the lane. Conversation was never useful in the first before pass and was in the second: the backend emits no events, so a held view is never replaced in the page by its read behind, and whether the page's view held a conversation then depends on what the rig's earlier presses left. In the app the read behind lands in the window, which the probe above times at 2.4 to 8.9 s before and the section read at 0.14 to 0.8 s after.

## What reversing it costs

The lane is `on_lane` at four call sites (the run task, `execute_run`, the flush tick, the links after a run) and the wasm host's capture; removing them makes every request interactive, which is the old order. The reserve is `Rate::reserve`; returning it to zero keeps the lane and gives the sync the whole burst again. Going back to governor means losing the reserve.
