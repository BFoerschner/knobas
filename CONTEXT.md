# knobas

A personal work cockpit: external work systems sync into one local database, and one search box, one link graph, and one inbox sit on top of the mirror. This glossary is the canonical vocabulary — code, plans, and reviews use these words with exactly these meanings.

## Entities and kinds

**Entity**:
Anything with exactly one stable in-app address — a synced item, a context, a note, an asset.
_Avoid_: object, record

**Kind**:
The type of an entity: ticket, pr, build, page, repo, branch, commit — and note from M2.
_Avoid_: type, category

**Note**:
A knobas-owned markdown document with `[[refs]]`; a first-class searchable kind, not an annotation on something else.

## Sync

**Source**:
One configured external system (a Jira, a Gitea, a TeamCity…) that knobas syncs from.
_Avoid_: integration, provider, connector

**Adapter**:
The per-system implementation that speaks a source's API and emits its items.
_Avoid_: plugin, client

**Mirror**:
The local synced copy of every source's data, with provenance. Readers only ever see its live items. A count of the mirror is a corpus, never a run's [Upserted](#upserted).
_Avoid_: cache, index

**Item**:
One mirrored record of a given kind.

**Upserted**:
The items one run wrote, new or changed. A per-run delta, not the size of the mirror: a run that writes nothing over a full mirror upserted zero.
_Avoid_: synced, mirrored (both name the corpus)

**Full sync**:
A cursor-less run; the source is re-read from the top.
_Avoid_: initial sync, resync

**Incremental sync**:
A run from a cursor, emitting only what changed since.

**Cursor**:
The adapter-owned position that makes the next run incremental.
_Avoid_: checkpoint, offset

**Exhaustive**:
A per-kind declaration that a full sync emits that kind's complete corpus. Only exhaustive kinds are swept; a budgeted kind is never exhaustive. (ADR-0003)
_Avoid_: complete, full

**Budget**:
A cap on how much of a kind a full sync emits per container (e.g. commits per repo). A budget makes its kind non-exhaustive.
_Avoid_: limit, cap

**Sweep**:
The engine pass that tombstones items of an exhaustive kind that a full sync no longer emitted.
_Avoid_: cleanup, purge, garbage collection

**Tombstone**:
Marking an item deleted-at-source while keeping the row. Tombstoned items leave every reader's view.
_Avoid_: delete, remove

**Live item**:
An item that is not tombstoned — the only thing any reader reads.

**Watermark**:
A sync position that only advances as work completes. Its **ceiling** is the newest position recorded at run start, which the watermark may never pass within that run.

**Backfill**:
A deliberate full sync whose purpose is re-fetching unchanged items after the fetched payload widened.
_Avoid_: refetch

**Credential health**:
A source's authentication state as knobas last observed it, surfaced per source in the shell.

## Links and contexts

**Link**:
A knobas-owned, typed, bidirectional connection between two entities — one record shape for every pair. Links live locally and are never written to a source.
_Avoid_: relation (that is a link's type), reference

**Suggestion**:
A machine-proposed link with a stated reason. Accepting makes a link; dismissing persists.
_Avoid_: recommendation

**Context**:
A working set of entities. Membership = explicit adds + direct links + one hop out; asset membership also counts through ancestors. The rule is fixed, not configurable.
_Avoid_: workspace, project, room (that is its view)

**Smart list**:
A saved local query with a live count and change badge — built-ins plus any launcher search saved as a list.
_Avoid_: filter, saved search

## Acting on sources

**Write-back**:
An operation that changes data in a source (status, comment, approval, trigger). Links, contexts, notes, and time are never write-back.

**Pending write**:
An edit queued because its source cannot currently accept it. The queue is a visible, inspectable list, not a count.

**Held write**:
A pending write whose target changed after it was queued. It flushes only after the user chooses, both versions shown side by side — there is no silent last-write-wins.

**Inbox**:
The single actionable stream — mentions, review requests, failed builds, assignments, credential expiry — with actions and snooze.
_Avoid_: notifications, feed

## Export

**Backup export**:
Everything knobas owns, notes included; the mirror is excluded (it re-syncs).

**Share export**:
A curated export — links, assets, contexts, smart lists — with notes excluded by default; every part toggleable.

## Surfaces

**Launcher**:
The ⌘K box; one search over everything, under 100 ms.
_Avoid_: command palette, spotlight

**Room**:
A context's hub view — tiles, activity, tray.
_Avoid_: dashboard

**Tidewater**:
The fictional company whose dataset seeds demos, fixtures, and the test environment.
