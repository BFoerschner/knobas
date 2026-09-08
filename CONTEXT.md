# knobas

A personal work cockpit: external work systems sync into one local database, and one search box, one link graph, and one inbox sit on top of the mirror. This glossary is the canonical vocabulary — code, plans, and reviews use these words with exactly these meanings.

## Entities and kinds

**Entity**:
Anything with exactly one stable in-app address — a synced item, a context, a note, an asset.
_Avoid_: object, record

**Kind**:
The type of an entity: ticket, pr, build, page, repo, branch, commit — note from M2; asset, route and monitor from M4.
_Avoid_: type, category

**Note**:
A knobas-owned markdown document with `[[refs]]`; a first-class searchable kind, not an annotation on something else.

**Capture**:
A [note](#note) made from the global shortcut while some other window has the focus, in a small window of its own, created on the first keystroke and never on an empty one. What it attaches is two ordinary [links](#link): `captured-in` to the [context](#context) of the last **stored** [room](#room) the reader stood in — a derived room has no context, so nothing then — and `captured-from` to the foreground entity if there was one — the word as [Passive attribution](#passive-attribution) defines it, so a promoted room with nothing open gives its anchor. The in-app *New note* attaches the same two, by the same mechanism. Never a field on the note. (Ruled 2026-09-07, v1.5 grilling; spec §6)
_Avoid_: quick note, scratch, jot, inbox (that is the stream of things to answer)

**Checkout**:
A clone of a [repo](#kind) on this machine, found under the **clones root** — a directory setting knobas scans two levels deep, matching each clone's remote to a repo entity by host and owner/repo — or set by hand per repo as an override. Knobas-owned data about the local disk, never a field of the mirrored repo, and never written to: no clone, no checkout, no fetch (ADR-0016). Where *Open in editor* and *open a terminal here* point. (Ruled 2026-09-07, v1.5 grilling; spec §5)
_Avoid_: clone (as a noun for the record — the clone is the directory, the checkout is what knobas knows about it), working copy, local path, worktree (a worktree is one more checkout, set by hand)

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
The local synced copy of every source's data, with provenance. Readers see its [live items](#live-item); the readers that reach past them are named under [Live item](#live-item), and each says why. A count of the mirror is a corpus, never a run's [Upserted](#upserted).
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
Marking an item deleted-at-source while keeping the row. Tombstoned items leave every reader's view but the ones [Live item](#live-item) names.
_Avoid_: delete, remove

**Live item**:
An item that is not tombstoned **and whose source is enabled** — what a reader reads unless it is one of the four named below. Both halves are enforced by `sync.live_item`, never by a reader, so a reader that goes through the view cannot forget one, and a reader that does not has to say so here. A source with no configuration row at all (`run_once` syncs unconfigured sources) is not "disabled": its items stay live. (#202) **Four readers reach past the view, and this is the list of them** — each reads `sync.item` and each carries its reason:

1. The [Monitors](#monitors) tab's roster (`assets::monitor_roster`, #448) — exempt from the **tombstone half only**, keeping the enabled clause itself, because a *paused* monitor **is** a tombstone: Uptime Kuma drops it from `/metrics` and the adapter tombstones it, so a roster that inherited the view would silently lose the Paused chip and the monitor's history with it.
2. The detail read (`get_entity`'s `DETAIL`, §5a, #204, migration `0012`) — exempt from **both** halves, because [links](#link) and [notes](#note) point at an entity whether its source withdrew it or the reader turned its source off, and the panel they open has to draw it and say which. Neither dropped clause is a hole: each leaves its own marker in its place, `deleted_at` for the withdrawn banner and the derived enabled flag for the turned-off one, and the two are different banners because the remedies differ.
3. The paste resolver (`resolve_url`, #496, spec #491 story 15) — exempt from **both** halves, for the detail's reason and one of its own: it answers a URL with an entity's id and nothing else, and the entity then opens through the detail read above. So it has to reach exactly what that read can show, or a stale link would be told *Not in the mirror* about an entity the app can open — which is the opposite of the story, *"a stale link explains itself"*.
4. The checkout read (`knobas_app::checkout::repo_of`, #499, spec #491 stories 27–28) — exempt from **both** halves, for the detail's reason applied to a panel that lives *inside* the detail: a [checkout](#checkout) is knobas' own observation about this disk, and the clone is still there after the source withdrew the repository or the reader turned its source off. A read through the view would draw *not in the local index* inside a page the app can open, and would hide a path a person set by hand. Migration `0024`'s cascade is the same rule from the other side: a tombstoned repo keeps its entity row and therefore its override; only a purge takes it.

Those four, and a fifth needs a reason of its own and a line here. (Amended 2026-09-07, #448; amended 2026-09-08, #496, when the second and third readers were counted — the entry had claimed one since #204 shipped the detail's own exemption; amended 2026-09-08, #499, with the checkout read.)

**Watermark**:
A sync position that only advances as work completes. Its **ceiling** is the newest position the run *witnessed* at its start, which the watermark may never pass within that run. Witnessed, not the newest that exists: a ceiling too low costs a re-fetch, one too high loses work.

**Backfill**:
A deliberate full sync whose purpose is re-fetching unchanged items after the fetched payload widened.
_Avoid_: refetch

**Credential health**:
A source's authentication state as knobas last observed it, surfaced per source in the shell. Carries what the credential's last check *said went wrong*, not what an adapter had to say about the far end when it went right — that is a [Connection note](#connection-note).

**Discovered configuration**:
A configuration value an adapter learns from the instance itself during *Test connection*, rather than one the user types. It reaches the Add-source dialog on the connection report, keyed by the config property it belongs in, and the dialog fills that field **only when it is empty** — a value somebody typed is never replaced. Jira's Epic Link custom field id is the first: it is minted per instance, so an id copied from another server reads the wrong field rather than failing (#297). Not a secret, and never written by *Test connection* itself, which writes nothing.
_Avoid_: auto-detected, probed

**Connection note**:
The one line an adapter says about the far end that nothing else on the connection report already says — Jira's Epic Link clause ("found but not configured: epic membership is not mirrored") is the first. It belongs to the moment of *Test connection*: shown wherever a test result is shown, on a draft or on a saved source, and never stored. Not [Credential health](#credential-health), which a good sync may rewrite; a note is true of the far end as just found, not of the credential. (#326)
_Avoid_: detail (the field's name, not the term), diagnostic, warning

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
An operation that changes data in a source (status, comment, approval, trigger, worklog, page, pausing a [monitor](#monitor)). Links, contexts, notes, and [blocks](#block) are never write-back; a [worklog](#worklog) is one.

**Account**:
A **second** credential stored beside a source's ordinary one, in the same keychain item: a username and password, optional, for a source whose write channel its ordinary credential cannot open. Uptime Kuma is the only source with one — its API key opens `/metrics` and nothing else, and pausing a monitor is a socket.io login — so a Kuma with only the key reads every monitor and offers no [write-back](#write-back), and adding the account is what turns pause and resume on with no other change anywhere. Not an *auth method*: the auth method is how a source authenticates its ordinary traffic, one per source, and offering the account as a rival would let a reader pick it instead of the key. Added from the Add-source form or, on a source that already exists, from the credential strip the sources view's **Account** button opens — the same strip *Re-enter* opens, under the word a *working* source deserves. (Added 2026-09-07, #452, spec #427 story 69.)
_Avoid_: second secret, write credential, admin login

**Section**:
A heading of level one to three on a wiki [page](#kind), and everything under it until the next heading of the same or higher level. The unit knobas edits a page in: one section of prose at a time, never the whole page and never a fragment of one. A section holding a macro or a table — or one sitting *inside* a table, where an edit would cut across the cell it is in — **refuses** its edit and offers the source's own editor instead, because knobas reads those but cannot write them back. A section is edited as **text**, so sub-headings, lists and inline formatting inside it come back as plain paragraphs — that is not a refusal, and the surface says so before the edit is made rather than after. The edit that goes out still carries the whole page body — the source replaces the record — so everything outside the section is sent back exactly as it arrived. (#286)
_Avoid_: block (that is a unit of [time](#block)), chunk, fragment

**Pending write**:
An edit queued because its source cannot currently accept it. The queue is a visible, inspectable list, not a count. Delivery is at least once — a write in flight when knobas stops may arrive twice; knobas re-sends rather than guess, and never merges or deduplicates what you wrote. (ADR-0012)

**The word has two senses and both are load-bearing.** *This glossary's* sense is the whole open queue — pending, held and refused together — which is why the command that lists it is `pending_writes` and not `open_writes` (`docs/contract.md`, "Write-back": the name follows the glossary rather than the state column). The *state machine's* sense is narrow: `pending`, `held` and `refused` are three disjoint states of `knobas.write_queue`, and `write_queue::counts().pending` counts only the first. Neither is wrong and neither is being migrated to the other; read which one a surface means before changing it.

Which surface takes which: the write-queue **list** and its `pending_writes` command take the wide sense — they show everything knobas still owes. The launcher footer's *"N pending writes"* and `LauncherHome.pending_writes` take the narrow one (#212), because those are the writes that leave on their own, and a held or refused write is asking for a *decision* — which the status bar's badge, reading `write_queue_counts`, is where to ask for. Folding held into the footer would let it count down to zero with nothing sent.

**Held write**:
A queued write knobas will not send until the user acts, for one of two stated reasons: its target changed after it was queued (resolved by choosing between the two versions, shown side by side — there is no silent last-write-wins), or its source was turned off (resolved by re-enabling the source). The surface always says which; the two are never collapsed. (#204) In the state machine it has *left* `pending`; in this glossary's wider sense it is still a pending write. See the two senses above.

**Reachable transition**:
A status a ticket's workflow offers **from where the ticket stands right now**, read from the source when the detail opens and never stored — an answer about now, not a fact the mirror holds. The status select offers these and only these; when the read fails it falls back to the statuses the source's corpus has been seen to use and stays optimistic, as it was before the read existed. The write is unchanged: it still names the status, and the adapter still resolves and refuses by name. One read on the `Source` trait, implemented by every adapter, refused by name where a source has no workflow. (Ruled 2026-09-07, v1.5 grilling; booked by spec #272 and `docs/contract.md` #179)
_Avoid_: available transition, allowed status, workflow step, transition id (that is Jira's word and the adapter's business)

**Unclaimed write**:
Something a source made because knobas asked, that knobas has no record of. It happens in one window: a [pending write](#pending-write) withdrawn while it was in flight — one HTTP round trip wide — where the source takes the write and the row that would have recorded the delivery has already settled as withdrawn. It matters for the two ops that make something new and are not naturally idempotent, `create_ticket` and `create_page`: the ticket or the page is at the source, not in knobas' mirror until the next sync, and nothing links it to what asked for it. Withdrawing the write is not what created it and re-syncing does not adopt it — knobas has no key on it and cannot tell it from anything a colleague made.

**knobas cannot warn you before the fact and does not pretend to.** At the moment you withdraw a write, nothing in the queue distinguishes one that is in flight from one that was never tried. What knobas does instead is say so afterwards, at the one moment it knows: an *unclaimed* line in the activity log, against the container the write was made under. Where that container is mirrored — a parent page — its history panel is where the line reads; a Jira project is not mirrored, so there the line is reached through the activity stream itself. It carries the source's own id for what it made when the source named one (a Confluence page does; a Jira ticket does not, by design) — otherwise it names the withdrawn write, which still holds what was asked for. Deleting the artefact is a person's job at the source. (#336, ADR-0012)
_Avoid_: orphan, leaked write, ghost ticket

**Inbox**:
The single actionable stream — mentions, review requests, failed builds, assignments, credential expiry, and from M4 [alerts](#alert) — with actions and snooze.
_Avoid_: notifications, feed

**Desktop notification**:
What the operating system shows for **one new [inbox](#inbox) item**, when that item's category is switched on and the window is not focused; clicking it opens the item. Per category, all off until somebody says otherwise, and once per item. Always spelled in full: the bare word *notifications* is the inbox's forbidden synonym above, and the two must not collapse — the inbox is the stream that stays until it is answered, and this is one interruption about one line of it. Nothing that is not an inbox item is ever notified. (Amended 2026-09-07, #447: *opens the item* is one rule for all six categories and not one per surface — `addressOf` — because an [alert](#alert)'s way in is the Tree at the affected asset where every other category's is a room detail, and the two doors had drifted apart. Its key is `alert:<monitor>`, so a monitor that goes down, recovers and goes down again is announced once a session, not once an outage.) (#290, spec #272, #427)
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
A [note](#note) — one per date, opened by its own address — holding attendees, per-person notes and action items. *Publish* creates a Confluence page from it under a configured parent and links note and page; the note stays the editable original. Not a kind of its own. One page per date: knobas queues one create however often *Publish* is pressed, and the redelivery it cannot see (ADR-0012) is refused by Confluence's own per-space title uniqueness rather than duplicated. (M3 grilling 2026-09-02; built in #289)
_Avoid_: minutes, protocol page (that is the published copy)

**Timesheet**:
The week under the day strip, Monday to Sunday with empty weekends collapsed: a row per [timer target](#timer-target) plus one for focused time no [block](#block) covers, and per day the time **tracked** (that day's manual blocks — on the no-target row, the uncovered focused time itself), **offered** (its passive ones, counted beside tracked and never inside it), **logged** ([worklogs](#worklog) whose write is pending or sent, since the number is about what you did rather than about sync timing) and **unlogged** (the difference, and on the no-target row the whole of it, because nothing can log time with no target). A worklog whose write is waiting on a person shows as **held**, never as logged and never as unlogged. Minute granularity and **no rounding**: knobas must never invent a rounding policy your Jira may not have. *Log all* makes one worklog per day and ticket from that day's unlogged manual blocks, and never touches a passive or ad-hoc-label one. (M3 grilling 2026-09-02, spec #272; built in #283)
_Avoid_: report, timecard, week view

**Passive attribution**:
The opt-in recording of which entity was in the foreground — the open detail, else the asset in the [Tree](#tree)'s pane, else the room's anchor entity, else nothing — while the app window is focused, as passive [blocks](#block) derived from [observations](#observation). A gap stays a gap until a person assigns it; nothing recorded this way reaches a source on its own. The Tree is a view in its own right and not a slide-over drawn over a room, so browsing the estate with nothing selected is a gap and never the last room's anchor. Observations are kept a month, and past the [observation horizon](#observation-horizon) knobas has no record of what was open at all. (#315, #337, #437)
_Avoid_: automatic tracking, activity tracking (that is the activity stream's word)

**Observation**:
One heartbeat's record of what was in the foreground at a single instant — an entity, or nothing — written only while [passive attribution](#passive-attribution) is on. It says what was open *then* and nothing about what was open in between, which is why a passive [block](#block) is derived from a run of observations and never from one. (#315, #344)
_Avoid_: beat. The heartbeat's tick is a beat and `BEAT_WINDOW_SECONDS` is its width, so the word survives as passive attribution's own shorthand in code and tests; *observation* is the record it leaves, the word this glossary uses, and the only one of the two a reader is ever shown.

**Observation horizon**:
The instant before which knobas has thrown its [observations](#observation) away: a fact about what was swept, not about how old a day is, so a profile nothing has ever been swept from has no horizon at all. A day reaching back past it is a day knobas has no record for, and what was open on it is **absent, not zero** — the day review and the timesheet say so rather than drawing it as a day with nothing on it. (#315, #337, #344)
_Avoid_: retention cutoff, thirty days ago (both name a date arithmetic can reach; the horizon is only ever what a sweep actually took)

## Estate

**Estate**:
Everything the Assets view holds — the whole tree of [assets](#asset), their [routes](#route) and the [monitors](#monitor) attached to them — the way the [mirror](#mirror) is the whole synced copy. Built by hand and by seed; from M4 it is the real test infrastructure (the Hetzner servers, the local Gitea and Uptime Kuma), never a fictional one.
_Avoid_: inventory, topology, infrastructure (that is what the estate models, not the model)

**Asset**:
A knobas-owned entity — a server, a container, a service, a database, a runtime — with a type, typed and custom properties, and a place in the estate's tree. Its place is its **parent**, a field of its own and never a [link](#link): the tree is structure, relations are links (ADR-0014). Never a mirrored [item](#item): a source may offer one through an [import](#import), and an accepted import makes an asset carrying an origin line, after which no sync overwrites what a person edited.
_Avoid_: resource, node, host (that is one type of asset)

**Import**:
Loading assets from outside — an estate file, or the file an [importer](#importer) produced — with a preview of what is already in the tree and what is new. What it makes are ordinary assets with an origin line; nothing imported is a mirrored item, and the file is data about a real estate, never a mock.
_Avoid_: sync (that is the mirror's word), seed (that is what the test environment does to a source)

**Importer**:
A producer of an estate file from a live system — hcloud, a Docker host — offered by the [Import](#import)'s chooser and previewed and applied by the same Import. **Not a [source](#source)** (ADR-0015): it mirrors nothing, syncs nothing, and appears nowhere sources do; its credential is its own, under the `importer:` keychain namespace. Its witness is the real estate: what it produces from the live system previews as *already in the tree* against the checked-in file. (Ruled 2026-09-07, v1.5 grilling; spec §3a)
_Avoid_: import adapter, asset adapter, asset source, connector

**Origin key**:
The property an [importer](#importer) sets that names the thing it read in the system's own terms — `hcloud_id` for a server, the docker context plus the container name for a container, since a container's id changes on every recreate and its name does not — and that the [Import](#import) matches on when the file's id is not one the tree holds. The second matching rule beside the id, and the only one. (Ruled 2026-09-07, v1.5 grilling)
_Avoid_: external id, foreign key, source id (that is a [source](#source)'s)

**Depends on this**:
What breaks if an [asset](#asset) goes down: its descendants, since they run inside it, and every asset linked to it by `depends-on` or `runs-on`, transitively over both — the panel in the asset pane that answers *blast radius* (spec §12.2). Routes whose target is the asset are listed beneath as *reachable via routes that would break* and not counted. `depends-on` and `runs-on` are load-bearing relations from here on, beside `monitored-by`; the rest of the vocabulary stays open. (Ruled 2026-09-07, v1.5 grilling)
_Avoid_: blast radius (that is the question, not the panel), dependents, impact, downstream

**Route**:
A knobas-owned entity an [asset](#asset) exposes: a URL or endpoint, with or without a target asset. An asset is *reachable via* the routes whose target is anywhere on its own containment path — landing on it, on something that holds it, or on something it holds. (Amended 2026-09-06, #432: this entry read *"land on it or on something that holds it"*, and the ticket's own criterion asked for the other direction as well — *"the container reads it under reachable-via, and so does the VM that holds the container"*. Both are true sentences about reachability and the real estate needs both: every route in `testenv/hetzner/estate.json` lands on a container, so under the narrower reading no server in it would read a single route.)
_Avoid_: URL (that is a route's property), ingress, endpoint (bare)

**Monitor**:
A mirrored [item](#item) of the Uptime Kuma [source](#source), kind `monitor`: one check as Uptime Kuma defines it, with its current state. Attached to an [asset](#asset) by a `monitored-by` [link](#link); never an asset itself, and never created or edited in the mirror by hand. A **monitor name** kept on an asset is not one of these — it is the Uptime Kuma name a monitor of that name will answer to, waiting to be mirrored, and until then it is all knobas has. (Added 2026-09-06, #439, from spec #427's *"a name the mirror does not hold yet is kept on the asset and resolved by the next import or the M4.1 sync"*.) (Amended 2026-09-07, #453: a name arrives two ways and is resolved two ways, and the entry originally said only the [import](#import)'s. *Create monitor for this asset* records one too — it is what joins the monitor to the asset when it arrives, which is why the pane records the name before it queues the write — and **every poll of a monitoring source** resolves whatever names the mirror can now answer to, which is the other half of the spec sentence above. A monitor **created** from knobas goes into no container of its own: a monitoring source has nothing above a monitor that knobas mirrors, so the create addresses the source itself, `<source>:monitors` — deliberately not called a *roster*, which is this glossary's word for the [Monitors](#monitors) tab's list of rows.)
_Avoid_: check (that is one heartbeat of a monitor), probe, healthcheck

**Sample**:
One reading of one [monitor](#monitor) at one poll — its state, its response time, and when it was taken — appended by the sync engine at the end of every run of a source that emits monitors, whether or not that run changed anything. Knobas-owned, in the backup and never in a [share export](#share-export), swept by a retention setting defaulting to ninety days. Its *state* is knobas' vocabulary and not the source's: **warn** is derived here, at sample time, from the global response-time threshold, so a threshold changed later decides the next poll and never redraws what is already recorded. (Added 2026-09-07, #443, from spec #427's *"one sample per poll per monitor kept in knobas … so that history outlives Kuma's one-day pruning"*.)
_Avoid_: heartbeat (that is Uptime Kuma's word for its own check, and knobas' word for the app's own beat — see [passive attribution](#passive-attribution)), datapoint, metric

**Alert**:
A [monitor](#monitor)'s transition to down or warn, open until the monitor recovers; at most one open per monitor. Every open alert shows in the Assets view and the top strip; it reaches the [inbox](#inbox) only when some [context](#context) holds the affected asset, directly or through an ancestor. **Ack** is knobas-local — Uptime Kuma has no ack — and clears the inbox item while the alert stays open. **Only a return to *up* closes one**: a [sample](#sample) reading `pending` or `maintenance` neither opens an alert nor closes one, because silencing a monitor is not fixing it and an alert that closed itself when somebody paused it would be knobas reporting a recovery nobody made. (Amended 2026-09-07, #444: `monitor_sample`'s state column allows two words beyond the down/warn/up vocabulary this entry was written in, and the #443 merge review left the reading of them open; this is it, and the health rollup takes the same one — both words colour nothing.) (Amended 2026-09-07, #446: *some context* means an **unarchived** one, which is the one place the alert's routing rule and `member_ids` differ — `member_ids` is asked about a context by name and answers about it whatever its state, while the inbox is asking whether anybody is still working on this. An archived context is one the reader put away, and an alert that went on interrupting them because of it would be the inbox failing its own promise; the switcher draws the same line.) (Amended 2026-09-08, #504: the [launcher](#launcher)'s *Open alerts in my contexts* [smart list](#smart-list) is the third surface that shows an open alert — an [asset](#asset) with one that some unarchived context holds — and, like the Assets view and the top strip, it keeps an **acked** alert. The inbox is the only surface an ack clears, which is why its count and the list's differ by exactly the acked ones; the routing rule is the context clause, and the ack is the inbox's own lifecycle and not part of it.)
_Avoid_: incident, notification (bare), problem (that is the rolled-up count on a closed branch)

## Export

**Backup export**:
Everything knobas owns, notes included; the mirror is excluded (it re-syncs).

**Share export**:
A curated export — links, assets, contexts and [source](#source) configurations by default, with notes and time off; every part toggleable. *Time* is the timer, [blocks](#block) and [worklogs](#worklog): personal the way a note is, and a colleague reading a link map has no use for somebody's hours. The archive is the backup's own format restricted to those tables, named `knobas-share-<stamp>.knobas` so retention never deletes it, and the ordinary restore reads it. Two things it cannot carry, both because `pg_dump` restricts by table and never by row (#454): **the time settings**, since `knobas.setting` is one table holding every feature's bookkeeping and the recipient keeps their own; and **only some titles**, since an entity row is an address and the whole address book travels — a note's title crosses with notes off, its body does not. No credential is ever in an archive, so a restored source configuration lands as *missing secret* — and at no position at all, because no archive carries the [mirror](#mirror) either: a restored source reads its system from the top, or the links the archive brought would stay ids with nothing behind them (#455). The settings section lists share exports apart from the backups, since the retention sentence is true of one and not the other. Smart lists are built-ins, so there is no toggle for them until a saved one exists. (#283, #427, #454, #455)

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

**Tree**:
The Assets view's first tab: the [estate](#estate) as Miller columns, one column per level. Its sibling tab is *Monitors*. Named so that "board" never appears unqualified (ADR-0009). Its fixed pane offers **Create monitor for this asset** where a configured [source](#source) offers the op — prefilled from the asset's route or its hostname, and drawn nowhere else: a source with only its API key offers no create, and the pane then shows no control rather than a disabled one. (Amended 2026-09-07, #453, spec #427 story 70.)
_Avoid_: board, assets board, columns view

**Spine**:
A column of the [Tree](#tree) collapsed to a 30px strip, labelled with the [asset](#asset) of the reader's own path that runs through it — or, for a column their path does not reach, with the first asset in it — and re-expanded by clicking it. What collapses is decided by the width left beside the fixed pane and never by depth: the point is that a deep path does not push the pane off the screen. (#430, spec story 28)
_Avoid_: breadcrumb (that is a line of names, not a column), rail, collapsed column (as a name — that is what a spine *is*)

**Wire**:
A line the [Tree](#tree)'s pane draws from a [route](#route)'s row to the asset at the route's far end — the **target** for a route the asset exposes, the **exposer** for one it is reachable via, since the target of that one is the asset the reader is already standing in. It lands on that asset's row, or on the [spine](#spine) hiding the column that lists it. **Dashed when the far end is reached through something rather than pointed at**: a spine standing in for the row, a row or a column scrolled out of view so that the landing is where the row *would* be, or — on a *reachable via* row only, since an exposed route's wire points straight at its target — a route that arrives at an ancestor of this asset rather than at this asset. A far end that no open column lists draws nothing — the Tree opens the columns of one path, and an asset off that path is not on the surface to be pointed at. (#433, spec story 31)
_Avoid_: edge, arrow, connector, link (that is knobas' own relation between entities, and a wire is drawn from a field)

**Monitors**:
The [Assets](#asset) view's second tab, at `#/assets/monitors`: every mirrored [monitor](#monitor) as one row — its state, its type, what it watches, a 24-hour bar drawn from the [samples](#sample), the last check, Kuma's uptime ratios, certificate days remaining, the assets it is attached to, and one click to its page in Uptime Kuma. Above the list, one chip per state with a count, and clicking one narrows the list. Its sibling tab is *Tree*. The bar's segment is **half an hour** and reads as the worst state sampled in it; a half hour with no sample is a **gap**, and a gap says knobas was not watching rather than that nothing was wrong. (#448, spec #427 story 68) Above the roster, one **card** per open [alert](#alert) — the monitor that fell, what it watches, how long it has been open, whether somebody has acked it, with the ack and one step to the affected asset; below it, the **Not monitored** roster: every [asset](#asset) with no confirmed `monitored-by` link to a monitor, narrowed by a type filter whose options are the types that roster holds. *Not monitored* is a statement about **attachment and not about attention** — an asset whose only monitor Kuma has paused is not on it, because somebody wired a check to it and then silenced it, which the *Paused* chip on the roster above already says. (Amended 2026-09-07, #449, from spec #427 story 68's other half.) The same roster is also a **smart list** on the [launcher](#launcher)'s board, *Not monitored* — one rule with two readers, and the two are held to the same answer in the same order by `the_launchers_not_monitored_list_is_the_monitors_tabs_roster`, because the launcher's copy is hand-written SQL and no compiler sits between them. (Amended 2026-09-08, #504.) A roster row also carries **Pause** or **Resume** — one or the other, never both — when the source it came from has an [account](#account) beside its API key; a source with only the key shows neither, because pausing is a socket.io login the key cannot make. The button says *queued*, not *paused*: the write goes through the write queue, and the roster catches up when the next poll finds the monitor gone from `/metrics`. (Amended 2026-09-07, #452, spec #427 story 69.)
_Avoid_: uptime page, status page (that is Uptime Kuma's own published page), dashboard, health tab

**Type convention**:
The child [asset](#asset) types the built-in table records as usually held inside one — `AssetType::suggests`, drawn by the create dialog as *usual here* and by nothing else. An **ordering, never a filter**: the dialog offers every declared type whatever the conventions say, and `assets::create` accepts any type under any parent. Deliberately not called a *suggestion*, which is already the word for a machine-proposed [link](#link) that is accepted or dismissed; a type convention is neither proposed nor persisted. What the conventions answer to is the real estate — `testenv/hetzner/estate.json` — and not a diagram (ADR-0013). (#429, spec story 17)
_Avoid_: suggestion (that is a link), rule, constraint, allowed types

**Tidewater**:
The fictional company whose dataset seeds demos, fixtures, and the test environment.
