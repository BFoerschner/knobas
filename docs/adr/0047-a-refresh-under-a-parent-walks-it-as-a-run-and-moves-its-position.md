---
status: accepted
---

# A refresh under a parent walks it as a run would, and moves that parent's position

Amends [ADR-0031](0031-refresh-is-an-optional-read-of-one-item-serialized-behind-the-run.md). Decided 2026-11-20 by the ux25 pass's one decision agent (Fable), for Björn: "is it possible, when I am on a repository, to refresh only it and not everything when pressing cmd+r? Because right now it takes minutes until it does." Gitea read no repository, branch or commit by id, so their pages offered *Sync now*, and ⌘R on a repository ran the whole source: every repository of the estate, 58 at his size.

*Amended 2026-11-24, the pr-state fix (`docs/decisions/2026-11-24-a-pull-requests-state-shows-at-once.md`, Fable's ruling 2): the repository page walks its repository by itself, once its landing has drawn and at each moment it reads its lists again, at most once in 30 s, in the background lane (`refresh_entity`'s `behind`, additive), so a pull request merged or closed at the forge shows on *Pull requests* without a run. A walk that emitted nothing is `RefreshOutcome::Unchanged`: nothing written, no per-item work, and no `item:refreshed` unless a write is being read back, so a page's walk with no news drops none of the window's answers; the reader is told *Refreshed* as before. The rows a walk brings land in their places while the tab stays shown.*

*Amended 2026-11-27, #2564 (Björn: option A): ⌘R on a repository's *Builds* tab also finds new runs of that repository's pipelines. TeamCity answers `refresh_under` for a build configuration (`teamcity:buildType:<id>`): two reads of that configuration alone, `/app/rest/builds?locator=buildType:(id:<id>),state:…,sinceBuild:(id:<watermark>),count:N` for the finished builds and for the queued and running ones, N = 25 (fewer when `builds_per_config` is lower). It answers the builds above the stored watermark, which are what the next run emits anyway, and **moves no watermark** (`cursor: None`): TeamCity's watermark is server-wide, the walk saw one configuration, and its change gate is the watermark alone, so rows written past an unmoved position are re-read and written again by the next run, never judged against a stale position. This is the one walk that writes rows past its position; the rule below ("the parent's own position moves with its rows") stands for every walk whose source compares with a per-parent position. No position, or one taken under another scope, is `NotOffered`, as below. The build configuration declares `refreshes: true`, so its pipeline page draws *Refresh* (the walk) and keeps the Builds view's *Sync now* after it. The tab walks its pipelines one source's at a time (each refresh takes the source's lock only when free), then reads its runs in flight by id, then its answers; it syncs no source. The Rejected line on TeamCity's build configuration below is superseded.*

**A source may read an item together with what stands under it (`Source::refresh_under`, an optional SPI method, `NotOffered` by default): the run's walk of that one [parent](../../CONTEXT.md#parent), from where the stored position says it stood.** Gitea answers it for a repository, a branch and a commit, each walking its repository: the record, the branches, the pull requests and issues down to their watermarks, the commits of the branches that moved. Those three kinds declare `refreshes: true`, so their pages draw *Refresh* and ⌘R reads one repository.

- **Asked when `refresh` answers `NotOffered`**, in the same lock and transaction, the stored cursor read inside the lock as a run reads it.
- **The parent's own position moves with its rows.** The answer carries the whole cursor with that parent's part moved (`None` when the walk emitted nothing, as an idle run returns its cursor). Every other parent's part stays. Not an economy: Gitea's change gates compare the forge with the position, not with the mirror, so rows written past an unmoved position are judged against a forge that may since have gone back to it. A branch the walk tombstoned and the forge restored at the same head would stay tombstoned; a push the walk mirrored and a force-push undid would leave the mirror on the dropped head.
- **No position, no walk.** A cursor that does not parse, names no parent, or holds one the scope no longer admits owes a full sync and its sweep; a position naming one repository would make the next run incremental and cancel both. `NotOffered`, and the app runs the source.
- **No sweep, no `walked_whole`, no "synced" line.** A parent the source no longer has comes back as its tombstone item, and its part leaves the position.
- **Never beside a run.** The reader's refresh takes the lock only when it is free, as before. A run's batches are written in its open transaction, so a walk beside it would wait on its row locks or deadlock it, and the run's end writes its whole cursor over the walk's. A press while a run goes reads the page's own views again at once (they are the source's answers, not the mirror) and presses once more when that run ends, while the page still shows the item.
- **A landed write on a repository, branch or commit** takes the same path under the waiting lock, before the run that follows it.

## Rejected

- *Leave the position where it was* (the glossary's "moves no watermark"). Re-emitting is idempotent, but the change gates above make it wrong, not only slower.
- *Walk beside a run, accepting its older payload over the walk's until the next run.* Unsafe beyond staleness: row locks and the run's cursor write.
- *Wait behind the run.* Minutes of *Refreshing…*, which the 2026-11-12 ruling removed.
- *A narrower read for Code's list* (the repository listing alone). A second partial write across many parents, for a page nobody asked about; the list keeps its *Sync now*. Why an incremental Gitea run takes over a minute, holding the lock across the network, is its own ticket.
- *TeamCity's build configuration.* Its run is three queries and one server-wide watermark, with no per-configuration position to move; its page keeps *Sync now*. (Superseded 2026-11-27, #2564: the configuration is walked for its newest builds and moves no watermark; see the amendment above.)

## Consequences

- The `Source` trait (a frozen surface) gains `refresh_under` and `UnderAnswer`; the WIT world is untouched, so external adapters answer `NotOffered`.
- Contract battery clause 13 accepts either read per kind, and holds the walk to clause 2's rule: from the position a run just left on an unchanged source it emits nothing and moves nothing.
- Gitea reads the repository by name (`GET /repos/{owner}/{repo}`), where a run fingerprints the listing's row; Gitea 1.27.3 answers both with the same record (measured on the live testenv, 2026-11-20), so the change gate sees no change.
- A stored tree for a branch the walk moved is the next run's to store: until then *Files* reads the forge for that branch.
