---
status: proposed
---

# A landed write reads back what it moves, and looks again for what the source does after it

Widens [ADR-0031](0031-refresh-is-an-optional-read-of-one-item-serialized-behind-the-run.md), with [ADR-0047](0047-a-refresh-under-a-parent-walks-it-as-a-run-and-moves-its-position.md)'s walks as the scoped read. Spec #2586, audit #2587. Björn, 2026-11-27: "whenever I do any action anywhere, I want to see the changes appear rather immediately, not just when they're synced at the next full sync." The rule below is the orchestrator's ruling from his words; he may redirect it, and this ADR stays *proposed* until the first ticket that builds it (#2592) lands it.

## Today

After a write lands, `after_landing` (`crates/knobas-app/src/sources/write_queue.rs:895`), the one place every send path reaches, does three things in order:

1. **Refresh one item** (`write_queue.rs:897`): the item the write made, when the op's receipt names one (`WriteOp::receipt_names_an_entity`, `crates/knobas-source/src/lib.rs:2078`), otherwise the write's target (`read_back`, `write_queue.rs:977`). The engine asks `Source::refresh` and, on `NotOffered`, `Source::refresh_under`, the walk of that item's [parent](../../CONTEXT.md#parent) (`crates/knobas-sync/src/refresh.rs:232`).
2. **Follow a run in flight** (`write_queue.rs:927`): where that item is a build the write started, it is read again until it finishes (`crates/knobas-app/src/sources/follow_run.rs:188`, ADR-0031's 2026-11-25 amendment).
3. **Run the write's source** (`write_queue.rs:928`, `Scheduler::sync_after_write`, `crates/knobas-sync/src/scheduler.rs:2160`): an incremental run of the whole source, waited for. Everything else the write moved *in that source* arrives through this run. It is slow where the source is large (a Gitea run takes minutes at Björn's size), a later write's read-back stands it aside (ADR-0031's 2026-11-24 amendment), a source that syncs only by hand gets no run at all (`scheduler.rs:2161`, ADR-0056), and **no other source is run**.

So the gaps are: effects that only the whole-source run reads; effects in another source, which nothing reads until that source's next scheduled run; and what a source does on its own seconds after the write, which nothing reads until the next run.

## Decision

1. **A write kind declares its effects, and after it lands knobas reads back the written item and each effect at once**, each at its own source and in its own place: a refresh by id or the walk of one parent (ADR-0047), never a run of a whole source. An effect is named in knobas' terms, not one adapter's, and resolved from the mirror: *the target's parent* (a pull request's repository, from its declared parent), *the items the target links to* (a pull request's tickets, in whatever source holds them), *the views that hold the target* (a Jira board). The effects are part of the op's delivery, stated beside `WriteOp::delivery` (`lib.rs:1790`) with no wildcard arm, as ADR-0006's 2026-10-19 amendment asks for anything the queue does per op. The run of the write's source still follows, as ADR-0031 says.
2. **What the source does later is looked for on a short, bounded schedule**: about 5 s, 15 s, 30 s, 60 s and 120 s after the landing. Each item stops once it has moved since the read-back. One schedule per scope at a time, and a new write in the scope restarts it. Reads use `Lock::Try` and the background lane after the first, as the follow of a run does; a refresh that finds the item unchanged costs the window nothing (ADR-0031's 2026-11-25 amendment). #2585 is the first case: code and builds after a branch change.
3. **An effect knobas cannot read back is said, not waited on in silence.** The write's toast or the page names it: "Shows after the next sync of *source*". That covers a kind its adapter does not refresh, a source that syncs only by hand, a source turned off, and a read that failed.
4. **Nothing waits on the read-back to draw.** The [echo](../../CONTEXT.md#echo) stays as today. The read-back replaces it in place, with no layout shift (`just moves`).

## The audit

What each `WriteOp` the queue sends moves, and what reads it back today. Every adapter that declares the op is listed with its write arm; an op no row lists for an adapter is refused by it (battery clause 5).

How it is read today:
- **R**: a refresh of the item by id (`Source::refresh`).
- **W**: the walk of the item's parent (`Source::refresh_under`, ADR-0047).
- **F**: followed until it finishes (`follow_run.rs:188`).
- **run**: only by the incremental run of the write's own source that follows the write (step 3 above).
- **live**: drawn from the source's live view (`Source::view`), read again whenever the page reads.
- **none**: nothing until the other source's next scheduled run.

Which item kinds each adapter can read alone:
- **Gitea**: `refresh` reads a pull request and an issue (`crates/knobas-source-gitea/src/refresh.rs:42`). A repository, branch or commit is `NotOffered` there (`refresh.rs:58`) and walks its repository instead (`crates/knobas-source-gitea/src/source.rs:361`, `crates/knobas-source-gitea/src/sync.rs:535`).
- **Jira**: `refresh` reads an issue (`crates/knobas-source-jira/src/source.rs:299`). A board or filter view (`board-N`, `filter-N`) is `NotOffered` (`source.rs:302`) and only a run reads it (`crates/knobas-source-jira/src/views.rs:1`).
- **Confluence**: `refresh` reads a page (`crates/knobas-source-confluence/src/source.rs:175`, `crates/knobas-source-confluence/src/sync.rs:696`).
- **TeamCity**: `refresh` reads a build (`crates/knobas-source-teamcity/src/source.rs:206`). A build configuration is `NotOffered` there (`source.rs:208`) and is walked for its newest builds instead, moving no watermark (`source.rs:220`, `crates/knobas-source-teamcity/src/sync.rs:1162`, #2564).
- **Uptime Kuma**: `refresh` reads a monitor from `/metrics`, and waits up to 30 s for a new monitor's first beat (`crates/knobas-source-kuma/src/source.rs:409`, `crates/knobas-source-kuma/src/refresh.rs:48`).
- **External (WASM)**: `refresh` works only where the component exports it, WIT 1.4.0 or later (`crates/knobas-wasm-host/src/source.rs:962`). `refresh_under` is not in the WIT world (ADR-0047, *Consequences*), so every parent is `NotOffered` (the trait default, `lib.rs:2217`).

The gap column names the gap group, filed as sub-issues of #2586.

| `WriteOp` (`lib.rs`) | Adapter (write arm) | Written item, read today | What else it moves, read today | What the source does on its own afterwards | Gap |
|---|---|---|---|---|---|
| `Comment` (:789) | Jira (`jira/src/source.rs:320`), Confluence (`confluence/src/source.rs:226`), Gitea (`gitea/src/source.rs:429`) | the ticket, page, pull request or issue: **R** | nothing | Jira automation answers or transitions; bots reply on a pull request: **run** | schedule |
| `Transition` (:799) | Jira (`jira/src/source.rs:324`), Gitea issue open/close (`gitea/src/source.rs:579`) | the ticket: **R**. A board column follows the ticket's status (`app/src/lib/tf/tickets/echo-board.ts:1`) | Jira: which boards and filters hold the ticket (a board's keys are its view item's): **run** | Jira automation moves a parent or assigns: **run** | Jira views; schedule |
| `CreateTicket` (:802) | Jira (`jira/src/source.rs:328`), Gitea (`gitea/src/source.rs:584`) | the new ticket, by its receipt: **R**. Its parent's children come from its own declared parent | Jira: the boards and filters that now hold it: **run** | as `Transition` | Jira views; schedule |
| `CreateBranch` (:812) | Gitea (`gitea/src/source.rs:395`) | the repository (the target): **W**, so the new branch shows | TeamCity builds of the branch: **none** | CI queues a build: **none** | #2585 |
| `CreatePullRequest` (:819) | Gitea (`gitea/src/source.rs:404`) | the new pull request, by its receipt: **R**. Its links to tickets are drawn on arrival | TeamCity builds of the pull request's ref: **none** | CI queues a build and reports a status (checks are **live**, `gitea/src/view/checks.rs:1`); bots comment: **run** | #2585; schedule |
| `Approve` (:831) | Gitea (`gitea/src/source.rs:434`) | the pull request (approvals, mergeability): **R** | nothing | an automatic merge once checks pass: **run** | schedule |
| `TriggerBuild` (:833), `RerunBuild` (:836), `RunBuild` (:1134) | TeamCity (`teamcity/src/source.rs:260`, `:264`, `:279`) | the new build, by its receipt: **R**, then **F** | the builds its snapshot dependencies queue in other configurations (`teamcity/src/chain.rs:1`): **run** catches them queued, and nothing follows them, so they stay *queued* until the next scheduled run | the chain runs; TeamCity reports commit statuses to Gitea (**live**) | build chain |
| `LogWork` (:864) | Jira (`jira/src/source.rs:354`) | the ticket (its worklog): **R** | nothing knobas mirrors elsewhere: no aggregate is read (`jira/src/sync.rs:133`) | nothing | none |
| `CreatePage` (:900) | Confluence (`confluence/src/source.rs:206`) | the new page, by its receipt: **R**. The parent's children come from its declared parent | nothing | nothing | none |
| `UpdatePage` (:927) | Confluence (`confluence/src/source.rs:217`) | the page: **R** | nothing | nothing | none |
| `PauseMonitor` (:948), `ResumeMonitor` (:964) | Uptime Kuma (`kuma/src/source.rs:340`, `:341`) | the monitor: **R** | nothing | the next beats: the following polls | none |
| `CreateMonitor` (:1000) | Uptime Kuma (`kuma/src/source.rs:349`) | the new monitor, by its receipt: **R**, waiting for its first beat. The assets it attaches to are linked in the same refresh (`knobas-sync/src/refresh.rs:8`) | nothing | nothing | Kuma API key only: say so |
| `MergePullRequest` (:1050) | Gitea (`gitea/src/source.rs:439`) | the pull request (*merged*): **R** | the base branch's head and the merge commit; the head branch when `delete_branch`; the issues the pull request closes: **run** (the repository walk exists but is not asked, since the pull request itself refreshes). The linked tickets in another source: **none**. TeamCity builds of the base: **none** | CI builds the base; Jira's development integration may transition the linked tickets; Gitea rechecks other pull requests' mergeability without bumping `updated` (no walk sees it; their pages read it **live**) | effects; #2585; schedule |
| `SetParameter` (:1091) | TeamCity (`teamcity/src/source.rs:268`) | the build configuration: `refresh` is `NotOffered`, so **W** (its newest builds); the parameter itself is **live** | nothing | nothing | none |
| `SubmitReview` (:1171) | Gitea (`gitea/src/source.rs:459`) | the pull request: **R** | nothing | an automatic merge; bots: **run** | schedule |
| `DeleteBranch` (:1198) | Gitea (`gitea/src/source.rs:491`) | the branch: `refresh` is `NotOffered`, so **W** on its repository, which tombstones it | the pull requests whose head it was: **W**, where Gitea moved them | nothing | #2585 (checkout side) |
| `EditPullRequest` (:1223) | Gitea (`gitea/src/source.rs:496`) | the pull request (title, body, open or closed): **R** | nothing | bots: **run** | schedule |
| `EditTicket` (:1252) | Jira (`jira/src/source.rs:375`), Gitea (`gitea/src/source.rs:560`) | the ticket: **R**. A changed parent's children follow, from the ticket's own declared parent | Jira: the boards and filters an assignee, label or field change moves it in or out of: **run** | Jira automation: **run** | Jira views; schedule |
| `UpdatePullRequestBranch` (:1289) | Gitea (`gitea/src/source.rs:454`) | the pull request (its head): **R** | the head branch's tip and its new commit: **run**. TeamCity builds of the head: **none** | CI builds the head: **none** | effects; #2585 |
| `CommitFile` (:1335) | Gitea (`gitea/src/source.rs:515`) | the new commit, by its receipt (`gitea/src/source.rs:536`): **W** on its repository, so the branch tip (and a `new_branch`) shows | TeamCity builds of the branch: **none** | CI builds: **none** | #2585 |
| `SourceAction` (:1378) | Gitea `add_label` on a pull request (`gitea/src/source.rs:540`); any external adapter's own actions | the target: **R** where its adapter refreshes it, else **run** | whatever the adapter's action does: knobas cannot know (`delivery`'s conservative arm, `lib.rs:1804`) | unknown | say so |
| `ReplyToThread` (:1400), `ResolveThread` (:1417), `RequestReviewers` (:1433) | Gitea (`gitea/src/source.rs:470`, `:477`, `:486`) | the pull request: **R** | nothing | the person asked reviews; bots: **run** | schedule |

Outside the queue:

| Write | Path | Written item, read today | What else it moves | Gap |
|---|---|---|---|---|
| Pleasant `CreateEntry`, `EditEntry`, `SetPassword` (`crates/knobas-source/src/passwords.rs:592`); `RequestAccess` is refused (`crates/knobas-source-pleasant/src/source.rs:594`) | a direct write (ADR-0056), `passwords_write` (`crates/knobas-app/src/commands/passwords.rs:546`), with no run after it | the entry, from one listing of its folder (`pleasant/src/source.rs:414`), written under the lock | an entry moved to another folder: its path waits for the next sync (`commands/passwords.rs:590`, `pleasant/src/source.rs:423`) | say so |
| Any write to an external (WASM) adapter | the queue, `crates/knobas-wasm-host/src/source.rs:950` | **R** from WIT 1.4.0, else **run** | every parent effect: `refresh_under` is not in the WIT | say so |
| Any write to a source that syncs only by hand | the queue | as its adapter | everything step 3 would have read: nothing until *Sync now* (`scheduler.rs:2161`) | say so |

## Gap groups

- **Effects**, #2592 (Rust): the declared effects and their read at once, rule 1. The rows marked *effects*, and the linked tickets of a merged pull request. Every later group builds on it.
- **#2585** (Rust and front): code and builds after a branch change, rule 2's first case. The rows marked *#2585*. Its triggers are the writes whose effects name a repository, so it is blocked by #2592.
- **Jira views**, #2593 (Rust): Jira reads a board or filter view by id, and a ticket write names the views that hold or may now hold the ticket as effects. Blocked by #2592.
- **Build chain**, #2594 (Rust): a build a write started brings its snapshot-dependency chain, each build read by id and followed until it finishes.
- **Schedule**, #2595 (Rust and front): rule 2 beyond branches. The written item and its linked items are read again on the bounded schedule, for bots, automation and automatic merges. Blocked by #2585 (the schedule) and #2592 (the links).
- **Say so**, #2596 (Rust and front): rule 3 for every row marked *say so*, and for any effect whose read answered `NotOffered` or failed. Blocked by #2592.

## What the rule cannot cover

- A change outside every source knobas reads: a deploy, a message in a chat, a system knobas has no adapter for.
- What a source does after the schedule's last read (a nightly automation, an approval hours later). The next run brings it. A build the write started is the exception: it is followed until it finishes.
- A push made outside knobas, or by a command run in the checkout (ADR-0060): knobas did not make the write, so it has no effects to read. A git sync that finds the branch head moved is #2585's second trigger.
- A source's change that no read by id or by parent can see: Gitea's recomputed mergeability of other pull requests, which leaves `updated` alone. Their pages read it live.
- An external adapter's parent effects, until the WIT world carries `refresh_under`. That growth is its own decision (ADR-0028). Until then rule 3 says so.

## Rejected

- **Run every source an effect lives in.** A run of a whole source is what the spec is moving away from: minutes at Björn's size, and it waits behind or stands aside for other writes.
- **Poll until something changes.** It is unbounded, and a write whose source never reacts would keep reading forever. The schedule ends at 120 s.
- **Effects per adapter.** It would make the SPI adapter-aware (ADR-0006). An effect is named in knobas' terms and resolved from the mirror's declared parents and links, so an external adapter's merge gets the same effects as Gitea's.
- **Write the expected effects into the mirror** (mark the base branch moved, the issues closed). The receipt carries an id and nothing else (ADR-0031), and a guess drawn as fact is worse than an echo.

## Consequences

- `WriteOp::delivery` grows an effects answer: a §10.8 change to `crates/knobas-source/src/**`, announced in the effects ticket's PR. The WIT does not change: an op crosses as JSON.
- A landed write costs more reads: one refresh or walk per effect at once, and up to five quiet reads per scope on the schedule. An unchanged read writes nothing and tells the window nothing.
- The glossary gains *effect* when this ADR is accepted.
