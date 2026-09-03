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

**Project**:
A source's own grouping of its items, where the source has one — a Jira project, a TeamCity project, a Confluence **space** (from M3.2). Per source and in the source's own word: Gitea has no such thing, and its repository is an entity kind rather than a grouping. Not a [Context](#context), which is a working set a person builds. (ADR-0010)
_Avoid_: container, space (as a synonym for project; it stays as Confluence's own word, ADR-0010), workspace, board (ADR-0009)

**Census**:
The whole-corpus report of every [project](#project) a source's live items show (`list_projects`), which is what the switcher builds project rooms from. Deliberately not a room's own read — that scans the newest 200 items and is a window, not a census, so a quiet project would silently lose its room. (#208)

**Mirror**:
The local synced copy of every source's data, with provenance. Readers only ever see its live items. A count of the mirror is a corpus, never a run's [Upserted](#upserted).
_Avoid_: cache, index

**Item**:
One mirrored record of a given kind.

**Payload**:
The raw source record an item carries verbatim — where everything not normalized (title, body text, author, updated time) lives, in the source's own shape.
_Avoid_: raw data, blob

**Payload read**:
A read into source-shaped data outside the adapter that shaped it. Permitted only where it can [miss](#miss), in one named statement, with its failure direction pinned. (ADR-0007)

**Declared path**:
Where an adapter says one of those things lives in its own payloads — a status, a priority, an assignee, requested reviewers, a merged flag, a project — carried per entity kind on its descriptor. A reader resolves the declaration; a kind that declares none [misses](#miss). (§10.8, issue #277)

**Miss**:
A payload read finding no recognizable shape and contributing nothing — the one failure a payload read is allowed. Guessing is the forbidden alternative.

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

**Purge**:
Dropping a deleted source's items from the [Mirror](#mirror) and tombstoning the entities they named — what *Remove source and its items* asks for. Applied inside the delete, and applied again when a run that was in flight at delete time settles (#127). Not a [Sweep](#sweep): a sweep reconciles what a full sync no longer emitted; a purge carries out a user's deletion.
_Avoid_: sweep, cleanup, delete (entities are tombstoned, never deleted)

**Tombstone**:
Marking an item deleted-at-source while keeping the row. Tombstoned items leave every reader's view.
_Avoid_: delete, remove

**Live item**:
An item that is not tombstoned **and whose source is enabled** — the only thing any reader reads. Both halves are enforced by `sync.live_item`, never by a reader, so a reader cannot forget one. A source with no configuration row at all (`run_once` syncs unconfigured sources) is not "disabled": its items stay live. (#202)

**Watermark**:
A sync position that only advances as work completes. Its **ceiling** is the newest position the run *witnessed* at its start, which the watermark may never pass within that run. Witnessed, not the newest that exists: a ceiling too low costs a re-fetch, one too high loses work.

**Backfill**:
A deliberate full sync whose purpose is re-fetching unchanged items after the fetched payload widened.
_Avoid_: refetch

**Credential health**:
A source's authentication state as knobas last observed it, surfaced per source in the shell.

**Discovered configuration**:
A configuration value an adapter learns from the instance itself during *Test connection*, rather than one the user types. It reaches the Add-source dialog on the connection report, keyed by the config property it belongs in, and the dialog fills that field **only when it is empty** — a value somebody typed is never replaced. Jira's Epic Link custom field id is the first: it is minted per instance, so an id copied from another server reads the wrong field rather than failing (#297). Not a secret, and never written by *Test connection* itself, which writes nothing.
_Avoid_: auto-detected, probed

## Links and contexts

**Link**:
A knobas-owned, typed, bidirectional connection between two entities — one record shape for every pair. Bidirectional all the way down: one *pair* carries one active link per relation, whichever way round it was drawn, and the stored direction is what tells `blocks` from `blocked by`. Links live locally and are never written to a source.
_Avoid_: relation (that is a link's type), reference

**Suggestion**:
A machine-proposed link with a stated reason. Accepting makes a link; dismissing persists.
_Avoid_: recommendation

**Context**:
A working set of entities. Membership = explicit adds + direct links + one hop out; asset membership also counts through ancestors. The rule is fixed, not configurable — the operational reading (seed + direct + hop, computed over confirmed links only) is ADR-0008.
_Avoid_: workspace, project, room (that is its view) — and `project` stays on this list now that [Project](#project) is a term of its own: a project is a source's grouping, a context is a working set, and neither is the other. (ADR-0010)

**Member**:
An entity the membership rule reaches for a given context — computed at read time, never stored. A proposal never makes a member.
_Avoid_: item (that is the mirror's word)

**Smart list**:
A saved local query with a live count and change badge — built-ins plus any launcher search saved as a list.
_Avoid_: filter, saved search

## Acting on sources

**Write-back**:
An operation that changes data in a source (status, comment, approval, trigger, worklog, page). Links, contexts, notes, and [blocks](#block) are never write-back; a [worklog](#worklog) is one.

**Section**:
A heading of level one to three on a wiki [page](#kind), and everything under it until the next heading of the same or higher level. The unit knobas edits a page in: one section of prose at a time, never the whole page and never a fragment of one. A section holding a macro or a table — or one sitting *inside* a table, where an edit would cut across the cell it is in — **refuses** its edit and offers the source's own editor instead, because knobas reads those but cannot write them back. A section is edited as **text**, so sub-headings, lists and inline formatting inside it come back as plain paragraphs — that is not a refusal, and the surface says so before the edit is made rather than after. The edit that goes out still carries the whole page body — the source replaces the record — so everything outside the section is sent back exactly as it arrived. (#286)
_Avoid_: block (that is a unit of [time](#block)), chunk, fragment

**Pending write**:
An edit queued because its source cannot currently accept it. The queue is a visible, inspectable list, not a count. Delivery is at least once — a write in flight when knobas stops may arrive twice; knobas re-sends rather than guess, and never merges or deduplicates what you wrote. (ADR-0012)

**The word has two senses and both are load-bearing.** *This glossary's* sense is the whole open queue — pending, held and refused together — which is why the command that lists it is `pending_writes` and not `open_writes` (`docs/contract.md`, "Write-back": the name follows the glossary rather than the state column). The *state machine's* sense is narrow: `pending`, `held` and `refused` are three disjoint states of `knobas.write_queue`, and `write_queue::counts().pending` counts only the first. Neither is wrong and neither is being migrated to the other; read which one a surface means before changing it.

Which surface takes which: the write-queue **list** and its `pending_writes` command take the wide sense — they show everything knobas still owes. The launcher footer's *"N pending writes"* and `LauncherHome.pending_writes` take the narrow one (#212), because those are the writes that leave on their own, and a held or refused write is asking for a *decision* — which the status bar's badge, reading `write_queue_counts`, is where to ask for. Folding held into the footer would let it count down to zero with nothing sent.

**Held write**:
A queued write knobas will not send until the user acts, for one of two stated reasons: its target changed after it was queued (resolved by choosing between the two versions, shown side by side — there is no silent last-write-wins), or its source was turned off (resolved by re-enabling the source). The surface always says which; the two are never collapsed. (#204) In the state machine it has *left* `pending`; in this glossary's wider sense it is still a pending write. See the two senses above.

**Inbox**:
The single actionable stream — mentions, review requests, failed builds, assignments, credential expiry — with actions and snooze.
_Avoid_: notifications, feed

**Desktop notification**:
What the operating system shows for **one new [inbox](#inbox) item**, when that item's category is switched on and the window is not focused; clicking it opens the item. Per category, all off until somebody says otherwise, and once per item. Always spelled in full: the bare word *notifications* is the inbox's forbidden synonym above, and the two must not collapse — the inbox is the stream that stays until it is answered, and this is one interruption about one line of it. Nothing that is not an inbox item is ever notified. (#290, spec #272)
_Avoid_: notification (bare), alert, toast (that is the in-window message)

## Time

**Timer target**:
The one entity (ticket, page, note, repo, asset) or ad-hoc label a running timer is attributed to, and what a [block](#block) records. A [context](#context) is never a target: it is a set, and time on a set has nowhere to go; the ad-hoc label covers "worked across the SEPA context". Not the glossary's *context* — the two words were separated deliberately (ADR-0010).
_Avoid_: timer context, context

**Block**:
A knobas-owned stretch of time — start, end, [timer target](#timer-target) — and the unit everything about time is built from. **Manual** when the timer made it, **passive** when attribution recorded what was open. Blocks stay local and exportable; none is ever written to a source, and a passive block is never logged without a person saying so. A block remembers which [worklog](#worklog), if any, it was logged into, and which stored [context](#context), if any, the timer was started in — a fact about that moment, never about the context you are standing in when you look at it later.
_Avoid_: interval (that is a worklog's editable span), entry, session

**Worklog**:
The Jira record that one or more [blocks](#block) become when logged — a [write-back](#write-back) through the write queue like any other, with a local copy. Sending it changes nothing about the blocks. **Withdrawing its queued write does**: knobas never saw it land, so the copy goes with the write and the blocks are unlogged again, offered by *Log all* and editable in the day review. A worklog Jira has already answered for is not withdrawable that way. (#328) At-least-once like every write (ADR-0012): a worklog in flight when knobas stops may land twice, and knobas re-sends rather than guess.
_Avoid_: time entry, logged time (that is the timesheet's column, not the record)

**Digest**:
The standup's generated three lists — yesterday, today, blockers — drawn from the [mirror](#mirror) and the activity stream for the configured usernames, every line linking to the item it came from. *Yesterday* is the newest day before today that has any of your activity, at most seven days back. Mine only: it describes the person the sources were configured as, never a colleague. A day's work reaches it by three routes and no fourth: the [mirror](#mirror) for what the sources *attribute* to you — which is the source's own word for whose record it is, and on Jira that is the assignee, never a claim that you wrote it — the activity stream for the writes you made through knobas, and the local [worklog](#worklog) copy for the hours. *Today* adds the running timer's [target](#timer-target). *Blockers* are your items whose status their own source declares blocked-like plus the ones a confirmed link marks blocked by. (M3 grilling, 2026-09-02; built in #288)
_Avoid_: report, summary, standup (that is the whole flow)

**Standup protocol**:
A [note](#note) — one per date, opened by its own address — holding attendees, per-person notes and action items. *Publish* creates a Confluence page from it under a configured parent and links note and page; the note stays the editable original. Not a kind of its own.
_Avoid_: minutes, protocol page (that is the published copy)

**Timesheet**:
The week under the day strip, Monday to Sunday with empty weekends collapsed: a row per [timer target](#timer-target) plus one for focused time no [block](#block) covers, and per day the time **tracked** (that day's manual blocks — on the no-target row, the uncovered focused time itself), **offered** (its passive ones, counted beside tracked and never inside it), **logged** ([worklogs](#worklog) whose write is pending or sent, since the number is about what you did rather than about sync timing) and **unlogged** (the difference, and on the no-target row the whole of it, because nothing can log time with no target). A worklog whose write is waiting on a person shows as **held**, never as logged and never as unlogged. Minute granularity and **no rounding**: knobas must never invent a rounding policy your Jira may not have. *Log all* makes one worklog per day and ticket from that day's unlogged manual blocks, and never touches a passive or ad-hoc-label one. (M3 grilling 2026-09-02, spec #272; built in #283)
_Avoid_: report, timecard, week view

**Passive attribution**:
The opt-in recording of which entity was in the foreground — the open detail, else the room's anchor entity, else nothing — while the app window is focused, as passive [blocks](#block) derived from [observations](#observation). A gap stays a gap until a person assigns it; nothing recorded this way reaches a source on its own. Observations are kept a month, and past the [observation horizon](#observation-horizon) knobas has no record of what was open at all. (#315, #337)
_Avoid_: automatic tracking, activity tracking (that is the activity stream's word)

**Observation**:
One heartbeat's record of what was in the foreground at a single instant — an entity, or nothing — written only while [passive attribution](#passive-attribution) is on. It says what was open *then* and nothing about what was open in between, which is why a passive [block](#block) is derived from a run of observations and never from one.
_Avoid_: beat, heartbeat (the tick that leaves an observation, never the record it leaves — and only the record is ever spoken about to a reader)

**Observation horizon**:
The instant before which knobas has thrown its [observations](#observation) away: a fact about what was swept, not about how old a day is, so a profile nothing has ever been swept from has no horizon at all. A day reaching back past it is a day knobas has no record for, and what was open on it is **absent, not zero** — the day review and the timesheet say so rather than drawing it as a day with nothing on it. (#315, #337)
_Avoid_: retention cutoff, thirty days ago (both name a date arithmetic can reach; the horizon is only ever what a sweep actually took)

## Export

**Backup export**:
Everything knobas owns, notes included; the mirror is excluded (it re-syncs).

**Share export**:
A curated export — links, assets, contexts, smart lists — with notes and time excluded by default; every part toggleable. *Time* is the timer, [blocks](#block), [worklogs](#worklog) and the time settings: personal the way a note is, and a colleague reading a link map has no use for somebody's hours. The default is recorded; the export itself is M4. (#283)

## Surfaces

**Launcher**:
The ⌘K box; one search over everything, under 100 ms.
_Avoid_: command palette, spotlight

**Coverage**:
Per query, which sources could answer a filter and which have nothing it can match — measured on the [corpus](#mirror) each source contributed, never on what matched. A source that *answered* and found nobody is saying something different from one whose items never name a person, and coverage is the name for that difference. (#141)
_Avoid_: capability (a source's `SourceDescriptor` declares those — a static claim about a source, where this is a measured one about a corpus, which is why #141 was ruled onto the response and not onto the descriptor), support

**Room**:
The hub view a switcher entry opens — tiles, activity, tray — every tile handed the room's whole filter, of which context is one nullable dimension. A **stored** room is a [context](#context)'s view and narrows by it; a **derived** room — *All work*, one per [source](#source), and from M2.6 one per [project](#project) the [census](#census) shows — has no context at all, and narrows by nothing, by its source, or by source plus project respectively. (#209, ADR-0010) A derived room that stops existing under a reader announces itself and hands the address to *All work* — whether it goes while the reader stands in it or while they are elsewhere and come back to it (#241, #257) — while a dead address opened cold still falls back there silently (the silent fallback is #209).
_Avoid_: dashboard

**Mini board**:
The Tickets tile's status-grouped rendering — one group per status, in the source's own words as the mirror holds them. Two layouts, and the room chooses the **default**: **columns** side by side for a bounded room (a project room, a stored context), **stacked** one under another for an unbounded one (*All work*, a source room). A reader can **override** the default from the tile's header, per room, for the session — the choice sticks to the room until the app restarts, and choosing the room's default again clears it (#245). The demote-only backstop past six columns is unchanged and wins over both: columns is refused with a reason, and an override it demotes is kept for the moment the board fits again. Same groups, same order, same counts in both — the layout is how they are arranged, never what they are. "Board" never stands alone: launcher board, assets board, mini board. (ADR-0009, #210, #245)
_Avoid_: kanban, board (unqualified)

**Maximised tile**:
A viewing gesture on a [room](#room): one tile fills the tile grid for this visit, restored by its own button or by Escape, and never persisted — a room switch or a restart brings the grid back. (#250)

**Tidewater**:
The fictional company whose dataset seeds demos, fixtures, and the test environment.
