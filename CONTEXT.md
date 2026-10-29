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
A knobas-owned markdown document with `[[refs]]`; a first-class searchable kind, not an annotation on something else. (Amended 2026-09-11, #524: a note's **birth and death are each one line in the activity stream** — `created`, which names every withdrawable [link](#link) the note was born with by the id a later `unlinked` line carries, and `deleted`. An **edit is not one**, and neither is a `[[ref]]`: the editor's autosave fires every 700 ms of pause and the log is append-only, so a line per save could never be collapsed afterwards — Björn's ruling of 2026-09-05, #409, which is also why a [worklog](#worklog) draft reads a note's `updated_at` for an afternoon of typing instead. The birth line is what pairs the log's two halves for a link a [capture](#capture) drew; before it, withdrawing one left an `unlinked` line whose `linked` partner had never existed. Amended again 2026-09-11, #561: a note reaches a worklog draft through that `updated_at` stamp and through **nothing in the stream** — the draft's activity read offers no `note:` line at all, birth, death and a withdrawn born link alike — so one note is one checkbox and no bullet in a worklog comment says a note's uuid.)

**Capture**:
A [note](#note) made from the global shortcut while some other window has the focus, in a small window of its own, created on the first keystroke and never on an empty one. What it attaches is two ordinary [links](#link): `captured-in` to the [context](#context) the reader stood in, when there was one — the ticket-first shell reports none since #881 (rooms are retired), so today nothing — and `captured-from` to the foreground entity if there was one — the word as [Passive attribution](#passive-attribution) defines it, so an open pull request gives the pull request and an open [job](#job) its ticket. The in-app *New note* attaches the same two, by the same mechanism. Never a field on the note. (Ruled 2026-09-07, v1.5 grilling; spec §6)
_Avoid_: quick note, scratch, jot, inbox (that is the stream of things to answer)

**Checkout**:
A clone of a [repo](#kind) on this machine, found under the **clones root** — a directory setting knobas scans two levels deep, matching each clone's remote to a repo entity by host and owner/repo — or set by hand per repo as an override. Knobas-owned data about the local disk, never a field of the mirrored repo, and never written to: no checkout, no fetch (ADR-0016). The one exception is *Clone*, which clones a repository into a new folder directly under the clones root, `<clones root>/<repository name>`, and nothing else (ADR-0023). Where *Open in editor* and *open a terminal here* point. (Ruled 2026-09-07, v1.5 grilling; spec §5)
_Avoid_: clone (as a noun for the record — the clone is the directory, the checkout is what knobas knows about it), working copy, local path, worktree (a worktree is one more checkout, set by hand)

**Working tree state**:
Where a [checkout](#checkout)'s working tree stands right now: the branch it is on (or a detached head), its upstream and how many commits it is ahead and behind, and how many paths are changed, untracked or conflicted. Read by read-only `git status` when the checkout panel opens and when the window regains focus; never stored, never polled. The branch name the mirror holds is compared to it, never passed to git (ADR-0021). A branch detail whose repo's checkout is on that branch says *Checked out locally*.
_Avoid_: git status (the command, not the fact), dirty/clean as the whole state, local branch

## Sync

**Source**:
One configured external system (a Jira, a Gitea, a TeamCity…) that knobas syncs from.
_Avoid_: integration, provider, connector

**Adapter**:
The per-system implementation that speaks a source's API and emits its items.
_Avoid_: plugin, client

**External adapter**:
An [adapter](#adapter) installed from a repository that follows the [adapter standard](#adapter-standard), pinned to one release and updated only when the reader asks. Its opposite is a **built-in adapter**, compiled into knobas. Both are held to the same [role](#role) requirements. (Added 2026-10-18, custom-sources grilling.)
_Avoid_: plugin, extension, add-on

**Adapter standard**:
What a repository must contain and implement for knobas to install an [external adapter](#external-adapter) from it. (Added 2026-10-18, custom-sources grilling.)
_Avoid_: plugin standard, plugin API

**Source action**:
A write an adapter declares that belongs to no [role](#role)'s vocabulary, such as *Convert to draft*. The adapter declares its dialog. Knobas queues it like any other write without knowing what it does, and hands it back to the adapter. (Added 2026-10-18, custom-sources grilling.)
_Avoid_: custom op, plugin action

**Role**:
A domain knobas has a working surface for (code host, ticket system, CI, wiki, monitoring), stated as what a source must provide to fill it: the [kinds](#kind) it emits, the [declared paths](#declared-path) each kind carries, the live reads it answers, and the writes it may offer. A source may fill several roles (Gitea fills code host and ticket system). The shell draws a role's surface for every source that fills it, never for a named source. A source whose items fit none of these declares a **custom role** of its own, which is handled like any other role: its view opens when one of its items is opened. A custom role has no surface you can open by itself. (Added 2026-10-18, custom-sources grilling; ADR-0029.) (Amended 2026-10-19, architecture review: which sources **fill** a role is one answer, given by the [mirror](#mirror) to every surface that asks for the role; a surface asks for a role and never decides its fillers itself. An item whose source fills no role is on no role's surface, and stays live everywhere else.)
_Avoid_: capability (the descriptor's search/write/webhooks/import flags), type, category, frame (the shell's drawing of a role, not the role)

**Project**:
A source's own grouping of its items, where the source has one — a Jira project, a TeamCity project, a Confluence **space** (from M3.2). Per source and in the source's own word: Gitea's code has no such thing, and its repository is an entity kind rather than a grouping; only a Gitea issue, mirrored as a ticket, takes its repository as its project, because the ticket system needs one (ADR-0029, #1206). Scoping to one repo is what its [parent](#parent) relation is for. Where a source's projects nest (TeamCity's `Payments › Payouts`), a project takes in its subprojects: `project:Payments` and a link-set pattern naming it match what sits anywhere beneath it (#1019). The chain stays projects, never parents. Not a [Context](#context), which is a working set a person builds. (ADR-0010)
_Avoid_: container, space (as a synonym for project; it stays as Confluence's own word, ADR-0010), workspace, board (ADR-0009)

**Census**:
The whole-corpus report of every [project](#project) a source's live items show (`list_projects`), which is what the project pickers read. Deliberately not a window over the newest 200 items, which would silently lose a quiet project. (#208)

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
Where an adapter says one of those things lives in its own payloads — a status, a priority, an assignee, requested reviewers, a merged flag, a project, the commit a code record stands at, the commits a build ran on — carried per entity kind on its descriptor. A reader resolves the declaration; a kind that declares none [misses](#miss). (§10.8, issue #277)

**Parent**:
The entity you can open that an item sits directly inside: a repo for a branch, PR or commit; a build configuration for a build; the parent page for a page; the epic or parent issue for a ticket; the parent asset for an asset, and the exposing asset for a route (the field of ADR-0014, the same concept under the same word). For a synced item, the adapter declares it per kind as a [declared path](#declared-path), and it comes from the source. It is never drawn, so it is **not a [link](#link)**. It is something you can open, so it is **not a [project](#project)**. A parent need not be in the [mirror](#mirror): an epic outside the synced JQL is still the parent its children name. `under:<name>` scopes a search to everything beneath a parent, transitively. It matches the names the adapter declares for that parent kind exactly, ignoring case, and also returns the parent itself when it is mirrored. An asset is the one parent no adapter declares: it answers to its name, read from its own field, and `under:` returns the asset itself, because the estate holds it. A child's searchable text never contains its parent's name, because being inside something is a filter and not a word. The launcher row's path line shows the parent chain instead. A title may still name its parent where it means nothing without it (a TeamCity build, `Deploy #42`). (ADR-0018; ruled 2026-10-11, git-usability grilling)
_Avoid_: container, scope, holder, repository (as the name of this fact)

**State chip**:
What a row shows of where its item stands, for a kind whose adapter declares one: a pull request's `open`, `merged` or `closed`, a build's `running`, `queued` or outcome (`SUCCESS`, `FAILURE`). The adapter declares it as a [declared path](#declared-path) with a value map. Each value the source writes is drawn with a [tone](#tone), and a value the map does not name is a [miss](#miss), so the row shows no chip. It is drawn after the title in list rows and launcher rows. Not a ticket's status: a status is a workflow column you can move a ticket to, and a chip is only read. (#666)
_Avoid_: badge, status (that is a ticket's workflow word), state (alone; a monitor's state is its own thing)

**Tone**:
How loud a value is drawn, in five words the shell owns the colours of: `ok`, `warn`, `err`, `info`, `muted`. An adapter says which tone a value has and never which colour. Shared by the [state chip](#state-chip) and a [source view](#source-view)'s blocks. (#666, #661)
_Avoid_: colour, severity, level

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
A cap on how much of a kind a full sync emits per container (e.g. commits per repo). A budget makes its kind non-exhaustive. On an incremental run it bounds what one run reads, never the delta: what it leaves unread, the next run reads (spec #1618 ruling A6).
_Avoid_: limit, cap

**Sweep**:
The engine pass that tombstones items of an exhaustive kind that a full sync no longer emitted. A run handed a position sweeps a kind too when its adapter walked that kind whole anyway, as after a scope change, and says so (#2186).
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
4. The checkout read (`knobas_app::checkout::repo_of`, #499, spec #491 stories 27–28) — exempt from **both** halves, for the detail's reason applied to a panel that lives *inside* the detail: a [checkout](#checkout) is knobas' own observation about this disk, and the clone is still there after the source withdrew the repository or the reader turned its source off. A read through the view would draw *not in the mirror* inside a page the app can open, and would hide a path a person set by hand. Migration `0024`'s cascade is the same rule from the other side: a tombstoned repo keeps its entity row and therefore its override; only a purge takes it.

Those four, and a fifth needs a reason of its own and a line here. (Amended 2026-09-07, #448; amended 2026-09-08, #496, when the second and third readers were counted — the entry had claimed one since #204 shipped the detail's own exemption; amended 2026-09-08, #499, with the checkout read.)

**Watermark**:
A sync position that only advances as work completes. Its **ceiling** is the newest position the run *witnessed* at its start, which the watermark may never pass within that run. Witnessed, not the newest that exists: a ceiling too low costs a re-fetch, one too high loses work. A ceiling below where the watermark already stands holds it there. On TeamCity the case does not arise: an incremental run's opening probe starts at the watermark's own build, so its ceiling is never below the watermark (#2016; from #1626 until then, such a ceiling was ignored).

**Backfill**:
A deliberate full sync whose purpose is re-fetching unchanged items after the fetched payload widened. It runs with no stored position and keeps the one it ends on, so it is also the way out when an adapter refuses to sync on from a position the server no longer has (TeamCity's deleted newest build, #1477) or one too far behind for one run to catch up (TeamCity's 1,000-build cap, #1626). There is no separate cursor reset.
_Avoid_: refetch

**Refresh**:
Reading one item from its source and writing what the source holds into the [mirror](#mirror), outside any run. It moves no [watermark](#watermark) and sweeps nothing. A refresh never overlaps a run of its source: it waits for a run in flight to end and reads after it, so an older run cannot put back what the refresh read. It happens when a [write-back](#write-back) lands, by whichever path it was sent, on the item the write made (by the id the source answered) or wrote to (a comment refreshes the page it is on), and a run of the source follows it for what the write changed elsewhere. It also happens when the reader asks for it on the item's detail (*Refresh*). A source whose adapter cannot read one item has no refresh, and nor has an item of a kind its adapter does not read one by one (a TeamCity build configuration, a Gitea repository): its detail offers *Sync now* instead, and a landed write is followed by the run alone. For its one item it does what a run does for every item it brings in, links on arrival and a monitor's samples and alerts among them. An item the source no longer has is tombstoned, as a run's probe would. A refresh that fails changes nothing about the write, which has landed. Not a sync: catching a whole source up is a run, which a view's *Sync now* starts for every source that fills the view's [role](#role). (Added 2026-10-25, refresh-after-write grilling; ADR-0031.)
_Avoid_: single-item sync, resync, refetch

**Credential health**:
A source's authentication state as knobas last observed it, surfaced per source in the shell. Carries what the credential's last check *said went wrong*, not what an adapter had to say about the far end when it went right — that is a [Connection note](#connection-note).

**Discovered configuration**:
A configuration value an adapter learns from the instance itself during *Test connection*, rather than one the user types. It reaches the Add-source dialog on the connection report, keyed by the config property it belongs in, and the dialog fills that field **only when it is empty** — a value somebody typed is never replaced. Jira's Epic Link custom field id is the first: it is minted per instance, so an id copied from another server reads the wrong field rather than failing (#297). Not a secret, and never written by *Test connection* itself, which writes nothing. A saved source that leaves such a field empty gets it at its next sync instead, which records it under the same rule and re-reads the source as a backfill (#889).
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

**Backlink**:
A page whose body links to this page.
_Avoid_: pages that link here, inbound link

**Context**:
A working set of entities. Membership = explicit adds + direct links + one hop out; asset membership also counts through ancestors. The rule is fixed, not configurable — the operational reading (seed + direct + hop, computed over confirmed links only) is ADR-0008. (Amended 2026-10-21, the grilling of #1403, #1416, #1427 and #1464: since rooms went with #881 a context is **the working set behind a [job](#job)**, and nothing else. Opening a job promotes its ticket to one; the context is what routes an [alert](#alert) to the inbox, scopes the job's proposals and lists the open jobs; and a reader never sees one by name, adds to one, or makes one. Its one state is **archived**: a ticket's context is archived exactly while the ticket's status is in its source's declared **done-like** set, the same shape as the blocked-like set, and un-archived when the status leaves it — the sync run maintains it, opening the job never touches it, a context first made for a done ticket is born archived, and promoting a done ticket returns its archived context rather than making a second. A done job that is reopened is reading, not working; leaving Done in the source is what revives it. **Ad-hoc contexts are retired**: nothing creates one, *Not a ticket* and R-3.11 go, and time without a ticket is the ⌘T label timer.)
_Avoid_: workspace, project, room (retired with #881; a [job](#job) is the working view now) — and `project` stays on this list now that [Project](#project) is a term of its own: a project is a source's grouping, a context is a working set, and neither is the other. (ADR-0010)

**Job**:
The working view of one ticket, at `#/job/<key>`: the ticket's card, then sections over what the ticket reaches (Code, Builds, Assets, Monitors, Pages), a Conversation, a Map that draws Dependencies from the ticket, and a Connections drawer for the [links](#link) and [link sets](#link-set) that reach it. A job is drawn from the ticket and what it reaches; opening one promotes the ticket's [context](#context), the working set the job's alert routing, proposals and the open-jobs list stand on, which the reader never sees by name. A job is a ticket's: ad-hoc work has no job (retired 2026-10-21; its time is the ⌘T label timer). (Ruled 2026-10-15, ticket-first redesign D2; amended #1095: rooms retired with #881; amended 2026-10-21, the grilling of #1427: the earlier "needs no stored context" and the ad-hoc job at `#/job/<ctx id>` are withdrawn — the code promoted a context on every open all along, and no ad-hoc job page was ever built.)
_Avoid_: task, workspace

**Link set**:
A named group of repositories, pipelines, pages, assets and monitors, each a fixed item or a pattern, with rules that say which tickets it applies to (under an epic, sub-tasks of a ticket, in a project, or one ticket by hand). A [job](#job) shows its members beside its context's [members](#member), never as members: a set is read when the job is, never stored as links, and never walked through. (ADR-0025, 2026-10-15)
_Avoid_: template, bundle

**Member**:
An entity the membership rule reaches for a given context — computed at read time, never stored. A proposal never makes a member.
_Avoid_: item (that is the mirror's word)

**Ticket view**:
A saved way of looking at tickets in the Tickets view: a name, a group, which tickets (conditions stored as search text, always tickets only), and how they show (a [ticket board](#ticket-board) or a list, columns, lanes, what cards show). A **Jira view** is one the Jira source keeps for a board or a favourite filter; Jira decides which tickets it holds, and only how it shows is set in knobas. Not a [smart list](#smart-list), which spans every kind and has no layout. (Ruled 2026-10-15, ticket-first redesign D6)
_Avoid_: saved search, filter, board (alone)

**Smart list**:
A saved local query with a live count and change badge — built-ins plus any launcher search saved as a list. (Amended 2026-09-08, #506, spec #491 stories 56-58 and 60: the second half of that sentence is now built. A **built-in** is hand-written SQL in `knobas_search::lists`' registry and is a row nowhere; a **saved** one is a row in `knobas.smart_list` holding the raw box text and the name it was given. They are one kind everywhere it matters — one `list:<id>` namespace, one rail, one seen-stamp key, one shape on the wire — and differ in three: only a saved list can be renamed or deleted, only a saved list can come to **need attention**, and opening a saved one reports author coverage where opening a built-in reports none — a built-in is not a filter anybody wrote, so there is no author question of theirs to answer. Needing attention is what the rail says when today's grammar refuses the stored query, and it is a statement about the *grammar* and not about the list being empty — nothing was counted, so the count reads 0 and the badge is off, and the row does not open. A saved list stores **text and never a parse**, so the parser of the day is what judges it; and one cannot be created unless it runs, which is what makes *needs attention* mean the world changed under a list rather than that knobas wrote down something it never understood. **No migration ever rewrites a stored query to a newer grammar**, and there is no editor for one: a data migration that *upgraded* saved text would be a parse in disguise — it commits the migration's reading of what the reader wrote — and it would make *needs attention* a state no row can reach. The row stays as the reader wrote it; a refused one is deleted and the search saved again, which is what the rail's own sentence offers. Sharing one is #507, not this.) (Amended 2026-09-08, #533: at most **sixteen** saved lists. Sixteen is a **measured bound and not a feature** — every saved list is a count over the corpora on the same board read, so the rail's cost grows with how many there are, and the number is read off the perf gate's rail curve rather than chosen. It is **lowered by measurement and never raised by hand**, because the budget does not move; it was 64 until the gate measured that rail. Where the bound bites is *Save as list*: `create` counts the table and refuses the seventeenth, and **nothing else enforces it** — a restore is a schema dump, so a restored or merged database is held to the cap by nothing but the archive it came from. The number lives in `saved::MAX_SAVED_LISTS` and its reading in the perf gate's own record; this entry follows them and never the other way round. Ruled 2026-09-08 by the deputy on #533, `docs/archive/2026-09-v1-5-unattended-rulings.md`.) (Amended 2026-09-08, #507: sharing one is built — the [share export](#share-export)'s `smart_lists` part carries `knobas.smart_list`, and it carries the query verbatim, an archive being the third place after a migration and an editor where stored text could have been read and rewritten.) (Amended 2026-10-12, #653: at most **eight**, and `create` refuses the ninth. Held re-runs of the perf gate read the board at sixteen clearing the budget by less than the measurement's own spread, and #533's ruling steps the bound to eight when that happens. A database that already holds more keeps every list; the next *Save as list* is refused until it holds fewer than eight.)
_Avoid_: filter, saved search

## Acting on sources

**Write-back**:
An operation that changes data in a source (status, comment, approval, trigger, worklog, page, pausing a [monitor](#monitor)). Links, contexts, notes, and [blocks](#block) are never write-back; a [worklog](#worklog) is one.

**Account**:
A **second** credential stored beside a source's ordinary one, in the same keychain item: a username and password, optional, for a source whose write channel its ordinary credential cannot open. Uptime Kuma is the only source with one — its API key opens `/metrics` and nothing else, and pausing a monitor is a socket.io login — so a Kuma with only the key reads every monitor and offers no [write-back](#write-back), and adding the account is what turns pause and resume on with no other change anywhere. Not an *auth method*: the auth method is how a source authenticates its ordinary traffic, one per source, and offering the account as a rival would let a reader pick it instead of the key. Added from the Add-source form or, on a source that already exists, from the credential strip the sources view's **Account** button opens — the same strip *Re-enter* opens, under the word a *working* source deserves. (Added 2026-09-07, #452, spec #427 story 69.)
_Avoid_: second secret, write credential, admin login

**Section**:
A heading of any level, one to six, on a wiki [page](#kind), and everything under it until the next heading of any level: a section never holds a heading, so an `h4` under an `h2` is a section of its own. The unit knobas edits a page in: one section of prose at a time, never the whole page and never a fragment of one. A section holding a macro or a table — or one sitting *inside* a table, where an edit would cut across the cell it is in — **refuses** its edit and offers the source's own editor instead, because knobas reads those but cannot write them back. A section is edited as **text**, so lists and inline formatting inside it come back as plain paragraphs — that is not a refusal, and the surface says so before the edit is made rather than after. The edit that goes out still carries the whole page body — the source replaces the record — so everything outside the section is sent back exactly as it arrived. (Amended 2026-10-21, #1531: the entry said levels one to three and ended a section at the next heading of the same or higher level; the Confluence adapter's split (`spans` in `blocks.rs`) has always started a section at every heading, `h1` to `h6`, and ended it at the next heading of any level, and *Save section*, the Docs reader's edit affordances and the write queue's *Edit section …* name (#1520) all follow it. The glossary now says what the code does. The #286 contract entry's "level one to three" described the retired frontend `page-sections.ts` and is history.) (#286, #1531)
_Avoid_: block (that is a unit of [time](#block)), chunk, fragment

**Pending write**:
An edit queued because its source cannot currently accept it. The queue is a visible, inspectable list, not a count. Delivery is at least once — a write in flight when knobas stops may arrive twice; knobas re-sends rather than guess, and never merges or deduplicates what you wrote. (ADR-0012) A write that went out and got no answer is different: if a second arrival would duplicate it, knobas holds it and asks you to check the source (a [held write](#held-write), #1125).

**The word has two senses and both are load-bearing.** *This glossary's* sense is the whole open queue — pending, held and refused together — which is why the command that lists it is `pending_writes` and not `open_writes` (`docs/contract.md`, "Write-back": the name follows the glossary rather than the state column). The *state machine's* sense is narrow: `pending`, `held` and `refused` are three disjoint states of `knobas.write_queue`, and `write_queue::counts().pending` counts only the first. Neither is wrong and neither is being migrated to the other; read which one a surface means before changing it.

Which surface takes which: the write-queue **list** and its `pending_writes` command take the wide sense — they show everything knobas still owes. The launcher footer's *"N pending writes"* and `LauncherHome.pending_writes` take the narrow one (#212), because those are the writes that leave on their own, and a held or refused write is asking for a *decision* — which the status bar's badge, reading `write_queue_counts`, is where to ask for. Folding held into the footer would let it count down to zero with nothing sent.

**Held write**:
A queued write knobas will not send until the user acts, for one of three stated reasons: its target changed after it was queued (resolved by choosing between the two versions, shown side by side — there is no silent last-write-wins), its source was turned off (resolved by re-enabling the source), or knobas sent it and the source never answered, so it **may have arrived** (resolved by checking the source, then *Send again* or *Discard*; #1125). The third is only for a write a second arrival would duplicate — a comment, a create, a worklog, an approval, a review, a build, a transition; a write the source refuses on repeat, or one that sets what is already set, waits and goes on its own like any write whose source is down. A create held this way counts as **on its way**, not stopped: its page draws it queued (*Monitor queued*, *Ticket queued*) and offers no second create, and knobas refuses a second monitor of its name, until you send it again or discard it; a create held because its target changed was never sent, and is offered again (Björn, #1966). *Changed* means changed in what the write is about: a write that overwrites (an edit, a transition, a merge) holds on what it would overwrite; a write that adds (a comment, an approval, a review) holds only when the target's declared state moved — a pull request merged or closed — or it left the mirror, never because a reply or its own first comment moved the text (#752). A file commit's target is the file itself, which the mirror does not hold: it is held when the **source** says the file changed since it was read, with the version the source holds now beside the reader's (#822). The surface always says which; the reasons are never collapsed. (#204) In the state machine it has *left* `pending`; in this glossary's wider sense it is still a pending write. See the two senses above.

**Echo**:
What a page draws of a write it has queued, muted beside what the source holds: *queued* while the write is open, *sent* once it has landed, and gone when a read of the page shows the write itself. A write that is held, refused or withdrawn takes its echo with it. Every surface's echo keeps this one lifecycle, and the read it waits for comes when the write's [refresh](#refresh) ends or a run of its source does. (Ruling #775 named it; added 2026-10-25, refresh-after-write grilling.)
_Avoid_: optimistic copy, placeholder, ghost, queued mark

**Reachable transition**:
A status a ticket's workflow offers **from where the ticket stands right now**, read from the source when the detail opens and never stored — an answer about now, not a fact the mirror holds. The status select offers these and only these; when the read fails it falls back to the statuses the source's corpus has been seen to use and stays optimistic, as it was before the read existed. The write is unchanged: it still names the status, and the adapter still resolves and refuses by name. One read on the `Source` trait, implemented by every adapter, refused by name where a source has no workflow. (Ruled 2026-09-07, v1.5 grilling; booked by spec #272 and `docs/contract.md` #179)
_Avoid_: available transition, allowed status, workflow step, transition id (that is Jira's word and the adapter's business)

**Source view**:
What an [adapter](#adapter) draws for one of its own kinds when the detail opens: a live read, never stored, answered as sections of typed **blocks** from one fixed vocabulary — facts, text, entity list, diff, log, settings, actions, a directory's tree, a file. The shell renders the vocabulary and never learns which adapter drew it. A pull request's changed files, a commit's diff, a build's log and a build configuration's parameters are read this way, because none of them is in the [mirror](#mirror). A section may be **deferred**: its title and count come with the first read and its blocks when the reader opens it. A block may name a section that opens in its own tab: a tree's entries open directories and files in the *Files* tab, which keeps its place and a *Back*. A value a view lets a reader change names the [write-back](#write-back) op that changes it, and the change goes through the write queue; a view never writes. An **action** is a write a reader starts from the view (*Merge…*), drawn in the view's **toolbar** above its tabs (or, for one that belongs to a single section, in that section). The source declares the action's dialog (its fields, its sentence, whether it is primary or dangerous, and any values it fixes so one op can be two actions, *Approve…* and *Request changes…*) and whether it can be taken right now; one it cannot take yet is drawn disabled with the source's reason, and one that can never apply to this entity (*Merge…* on a merged pull request) is not drawn. The shell draws an action only where the source declares its op, and it too is queued. An adapter with no view for a kind answers nothing and the detail draws the generic payload projection; a read that fails draws the same projection under a line saying why. (ADR-0020, 2026-10-12; actions #667; toolbar and declared forms #702; fixed values #705; disabled or absent, the #701 ruling; tree and file #711)
_Avoid_: page (that is a wiki kind), template, layout, widget, panel (the detail is the panel; a view is what fills it)

**Diff**:
The line-by-line difference between two versions of a set of files, as a source view's diff block carries it: per file, its path (and old path when renamed), what happened to it (added, modified, deleted, renamed, or binary where git prints no lines), its added and removed line counts, and its hunks. Parsed from the unified diff text a git server answers, by one parser every git adapter shares. Capped per file and per view; a file over the cap keeps its counts and says it was cut. A pull request's diff also carries its **line threads**, and may take new line comments. (ADR-0020)
_Avoid_: patch (the text the server sends, before it is parsed), changeset, delta

**Pipeline**:
What a CI source runs: TeamCity's build configuration, a GitLab pipeline, a Jenkins job. The Builds view is drawn over pipelines and their [runs](#run), and an adapter declares their fields; the source's own word appears once, in the view's header. (ADR-0022, 2026-10-15)
_Avoid_: build configuration (TeamCity's word), job (that is a [job](#job))

**Run**:
One execution of a [pipeline](#pipeline): TeamCity's build, a GitLab pipeline run, a Jenkins build. It has a number, a state, a branch and commit, steps, a log, tests and artifacts. (ADR-0022, 2026-10-15)
_Avoid_: build (TeamCity's word; it also means the Builds view), execution

**Checks**:
Whether the commit a pull request, a branch or a commit stands at is green, from two places drawn apart. The source's own **commit statuses** (Gitea's combined status: a context, its state, a description, a page) are a block of the [source view](#source-view)'s *Overview*, live. The **builds of a commit** are the [mirror](#mirror)'s: the build rows whose declared revision (the commits a build ran on) is the sha the code row declares as its commit, both [declared paths](#declared-path), drawn by the shell as the detail's *Builds* line, between the view's toolbar and its tabs and under the header where there is no view, whether or not the view answered. Both are drawn failing first, and neither draws anything where nothing reported. (#710, #732)
_Avoid_: CI status (one source's word), pipeline (a [pipeline](#pipeline) is what runs, not whether a commit is green), check run (GitHub's)

**Line thread**:
The review conversation on one line of a [diff](#diff): the comments reviewers left there, oldest first, on one side — the file as it was (*old*, a removed line) or as it becomes (*new*). Drawn under its line. One whose line is no longer in the diff as it was written is **outdated**, the source's answer, and is drawn apart at the end of its file; one on a file the diff no longer holds is outdated too, and kept in a list at the end of the diff. A comment the reader has written and not yet sent with a review is **pending**: it is the reader's own, kept by the shell until a review carries it (*Submit review…*, or a verdict) or the reader discards it, and never the mirror's. (#709)
_Avoid_: inline comment, annotation, draft review (Gitea's `PENDING` review is a server-side draft, which knobas neither reads nor writes)

**Unclaimed write**:
Something a source made because knobas asked, that knobas has no record of. It happens in one window: a [pending write](#pending-write) withdrawn while it was in flight — one HTTP round trip wide — where the source takes the write and the row that would have recorded the delivery has already settled as withdrawn. It matters for the two ops that make something new and are not naturally idempotent, `create_ticket` and `create_page`: the ticket or the page is at the source, not in knobas' mirror until the next sync, and nothing links it to what asked for it. Withdrawing the write is not what created it and re-syncing does not adopt it — the withdrawn row holds no key to it, and a sync cannot tell it from anything a colleague made.

**knobas cannot warn you before the fact and does not pretend to.** At the moment you withdraw a write, nothing in the queue distinguishes one that is in flight from one that was never tried. What knobas does instead is say so afterwards, at the one moment it knows: an *unclaimed* line in the activity log, against the container the write was made under. Where that container is mirrored — a parent page — its history panel is where the line reads; a Jira project is not mirrored, so there the line is reached through the activity stream itself. It carries the source's own id for what it made when the source named one (a Confluence page's id; a Jira or Gitea ticket's key, since #1754) — otherwise it names the withdrawn write, which still holds what was asked for. Deleting the artefact is a person's job at the source. (#336, ADR-0012)
_Avoid_: orphan, leaked write, ghost ticket

**Inbox**:
The single actionable stream — mentions, review requests, failed builds, assignments, credential expiry, and from M4 [alerts](#alert) — with actions and snooze. (Amended 2026-10-21, #1571: a **failed sign-in** is a seventh category. It is raised for a source whose [credential health](#credential-health) says the sign-in fails until somebody acts, meaning the credential was rejected or none is stored (the shell's `signInFails`). It is raised **once per episode**: an episode is the run of consecutive failing verdicts, and the item is dated at its first one, so a retry rejected again neither moves the item nor undoes a *done*. Any other verdict ends the episode and the item with it, and the next failure is a new episode.)
_Avoid_: notifications, feed

**Desktop notification**:
What the operating system shows for **one new [inbox](#inbox) item**, when that item's category is switched on and the window is not focused; clicking it opens the item. Per category, all off until somebody says otherwise, and once per item. Always spelled in full: the bare word *notifications* is the inbox's forbidden synonym above, and the two must not collapse — the inbox is the stream that stays until it is answered, and this is one interruption about one line of it. Nothing that is not an inbox item is ever notified. (Amended 2026-09-07, #447: *opens the item* is one rule for all six categories and not one per surface — `addressOf` — because an [alert](#alert)'s way in is the Tree at the affected asset where every other category's is the item's detail, and the two doors had drifted apart. Its key is `alert:<monitor>`, so a monitor that goes down, recovers and goes down again is announced once a session, not once an outage.) (#290, spec #272, #427)
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
The standup's generated three lists — yesterday, today, blockers — drawn from the [mirror](#mirror) and the activity stream for the configured usernames, every line linking to the item it came from. *Yesterday* is the newest day before today that has any of your activity, at most seven days back. Mine only: it describes the person the sources were configured as, never a colleague. A day's work reaches it by three routes and no fourth: the [mirror](#mirror) for what the sources *attribute* to you — which is the source's own word for whose record it is, and on Jira that is the assignee, never a claim that you wrote it — the activity stream for the writes you made through knobas, and the local [worklog](#worklog) copy for the hours. *Today* adds the running timer's [target](#timer-target). *Blockers* are your items whose status their own source declares blocked-like plus the ones a confirmed link marks blocked by. (M3 grilling, 2026-09-02; built in #288) (Amended 2026-10-17, #932: a fourth list, *Infrastructure*, holds every monitor [alert](#alert) open at any point from the start of the last working day to the end of today. It is **not** mine only — a monitor has no owner — and it carries no restarts, which the mirror does not record.)
_Avoid_: report, summary, standup (that is the whole flow)

**Standup protocol**:
A [note](#note) — one per date, opened by its own address — holding attendees, per-person notes and action items. *Publish* creates a Confluence page from it under a configured parent and links note and page; the note stays the editable original. Not a kind of its own. One page per date: knobas queues one create however often *Publish* is pressed, and the redelivery it cannot see (ADR-0012) is refused by Confluence's own per-space title uniqueness rather than duplicated. (M3 grilling 2026-09-02; built in #289)
_Avoid_: minutes, protocol page (that is the published copy)

**Timesheet**:
The week under the day strip, Monday to Sunday with empty weekends collapsed: a row per [timer target](#timer-target) plus one for focused time no [block](#block) covers (nor a dismissed offer or the running timer, #900/#901; the Day tab draws the same time as *open* rows), and per day the time **tracked** (that day's manual blocks — on the no-target row, the uncovered focused time itself), **offered** (its passive ones, counted beside tracked and never inside it), **logged** ([worklogs](#worklog) whose write is pending or sent, since the number is about what you did rather than about sync timing) and **unlogged** (the difference, and on the no-target row the whole of it, because nothing can log time with no target). A worklog whose write is waiting on a person shows as **held**, never as logged and never as unlogged. Minute granularity and **no rounding**: knobas must never invent a rounding policy your Jira may not have. *Log all* makes one worklog per day and ticket from that day's unlogged manual blocks, and never touches a passive or ad-hoc-label one. (M3 grilling 2026-09-02, spec #272; built in #283)
_Avoid_: report, timecard, week view

**Passive attribution**:
The opt-in recording of which entity was in the foreground — the open detail, else the asset in the [Tree](#tree)'s pane, else the open [job](#job)'s ticket, else nothing — while the app window is focused, as passive [blocks](#block) derived from [observations](#observation). The open detail is the entity page on screen (a pull request, repository, run or page; an address the shell draws no page for is not one), any of its tabs being the same entity; the Tree's pane holds the asset its address selects (`#/asset/<id>`). A gap stays a gap until a person assigns it; nothing recorded this way reaches a source on its own. The Tree is a view in its own right and not a slide-over drawn over a job, so browsing the estate with nothing selected is a gap and never the last job's ticket. Observations are kept a month, and past the [observation horizon](#observation-horizon) knobas has no record of what was open at all. (#315, #337, #437)
_Avoid_: automatic tracking, activity tracking (that is the activity stream's word)

**Observation**:
One heartbeat's record of what was in the foreground at a single instant — an entity, or nothing — written only while [passive attribution](#passive-attribution) is on. It says what was open *then* and nothing about what was open in between, which is why a passive [block](#block) is derived from a run of observations and never from one. (#315, #344)
_Avoid_: beat. The heartbeat's tick is a beat and `BEAT_WINDOW_SECONDS` is its width, so the word survives as passive attribution's own shorthand in code and tests; *observation* is the record it leaves, the word this glossary uses, and the only one of the two a reader is ever shown.

**Observation horizon**:
The instant before which knobas has thrown its [observations](#observation) away: a fact about what was swept, not about how old a day is, so a profile nothing has ever been swept from has no horizon at all. A day reaching back past it is a day knobas has no record for, and what was open on it is **absent, not zero** — the day review and the timesheet say so rather than drawing it as a day with nothing on it. (#315, #337, #344)
_Avoid_: retention cutoff, thirty days ago (both name a date arithmetic can reach; the horizon is only ever what a sweep actually took)

## Estate

**Estate**:
Everything the Assets view holds — the whole tree of [assets](#asset), their [routes](#route) and the [monitors](#monitor) attached to them — the way the [mirror](#mirror) is the whole synced copy. Built by hand and by seed; from M4 it is the real test infrastructure (the Hetzner servers, the local Gitea and Uptime Kuma), never a fictional one. (Amended 2026-10-12, #634: that infrastructure is the development notebook, its OrbStack VM and Docker engine, and the containers on it — Gitea, Uptime Kuma, and the TeamCity, Jira and Confluence started on demand. The three Hetzner servers it named until then were decommissioned.)
_Avoid_: inventory, topology, infrastructure (that is what the estate models, not the model)

**Asset**:
A knobas-owned entity — a server, a container, a service, a database, a runtime — with a type, typed and custom properties, and a place in the estate's tree. Its place is its **parent**, a field of its own and never a [link](#link): the tree is structure, relations are links (ADR-0014). Never a mirrored [item](#item): a source may offer one through an [import](#import), and an accepted import makes an asset carrying an origin line, after which no sync overwrites what a person edited.
_Avoid_: resource, node, host (that is one type of asset)

**Import**:
Loading assets from outside — an estate file, or the file an [importer](#importer) produced — with a preview of what is already in the tree and what is new. What it makes are ordinary assets with an origin line; nothing imported is a mirrored item, and the file is data about a real estate, never a mock.
_Avoid_: sync (that is the mirror's word), seed (that is what the test environment does to a source)

**Importer**:
A producer of an estate file from a live system — hcloud, a Docker host — offered by the [Import](#import)'s chooser and previewed and applied by the same Import. **Not a [source](#source)** (ADR-0015): it mirrors nothing, syncs nothing, and appears nowhere sources do; its credential is its own, under the `importer:` keychain namespace. Its witness is the real estate: what it produces from the live system previews as *already in the tree* against the checked-in file. (Amended 2026-10-12, #634: for an importer whose live system the checked-in estate no longer holds — hcloud, since the Hetzner servers were decommissioned — the witness is its own round trip against the live system: produce, land what is new and apply it, produce again, and the last file previews as all known with no changes; the project is empty between runs, and a server is stood up on demand when the importer changes.) (Ruled 2026-09-07, v1.5 grilling; spec §3a) (Amended 2026-09-08, #508: **producer** is the wider word, and it is the one the code and the wire carry. Every importer is a producer of an estate file; the estate file a person picks off the disk is the one producer that is *not* an importer, because there is no live system on the other end of it and nothing for an [origin key](#origin-key) to name. #508's own words are *"declared per producer … and the file-import producer declares none"* and spec #491's are *"App-side producers in the assets module … The Import dialog's chooser selects the producer"*, so the chooser chooses between producers, `assets::PRODUCERS` is the registry, and `producer` is the argument the two Import commands take. **Importer** stays the word for the ones that read a live system, which is every producer but that one.)
_Avoid_: import adapter, asset adapter, asset source, connector

**Origin key**:
The property an [importer](#importer) sets that names the thing it read in the system's own terms — `hcloud_id` for a server, the docker context plus the container name for a container, since a container's id changes on every recreate and its name does not — and that the [Import](#import) matches on when the file's id is not one the tree holds. The second matching rule beside the id, and the only one. (Ruled 2026-09-07, v1.5 grilling) (Amended 2026-09-08, #510: **every part of a key is a property, including one that repeats a column.** The rule matches property bags on both sides, every part or nothing, so a container's key is `docker_context` and `container_name` — the second beside the entry's own name and not instead of it. The alternative was to let a key reach across to a parent's property or down to an entry's field, which is a second matching rule inside the second matching rule; this entry's own words are *"the property an importer sets"*, and #510 wrote both onto every container in `testenv/hetzner/estate.json` rather than widening them. `crates/knobas-core/tests/it/estate_file.rs` holds the repeated one equal to the name it repeats.)
_Avoid_: external id, foreign key, source id (that is a [source](#source)'s)

**Depends on this**:
What breaks if an [asset](#asset) goes down: its descendants, since they run inside it, and every asset linked to it by `depends-on` or `runs-on`, transitively over both — the panel in the asset pane that answers *blast radius* (spec §12.2). Routes whose target is the asset are listed beneath as *reachable via routes that would break* and not counted. `depends-on` and `runs-on` are load-bearing relations from here on, beside `monitored-by`; the rest of the vocabulary stays open. (Ruled 2026-09-07, v1.5 grilling) (Amended 2026-09-08, #505: read from the link's `to` end — the assets that break are the `from` ends of the `depends-on` and `runs-on` links pointing at the asset, per spec #491's *"toward the asset"* and the [Link](#link) entry's stored direction. So a container drawn `runs-on` at a machine is in the machine's answer and the machine is not in the container's: what breaks is what was standing on the thing that fell.) (Amended 2026-10-21, #1553: **the routes listed beneath are the ones whose target is the asset or anything in its answer**, not only the asset itself. A route landing on a container stops answering when the machine holding it goes down, and *what breaks* is the question; under the narrower reading no machine in `testenv/estate.json` listed a single route, since every route in it lands on a container — #432's argument for **Route**'s *reachable via*. A route landing on something that *holds* the asset is still not listed: that thing keeps answering. The Tree's panel and the asset editor's *Needed by* read the same list.)
_Avoid_: blast radius (that is the question, not the panel), dependents, impact, downstream

**Route**:
A knobas-owned entity an [asset](#asset) exposes: a URL or endpoint, with or without a target asset. An asset is *reachable via* the routes whose target is anywhere on its own containment path — landing on it, on something that holds it, or on something it holds. (Amended 2026-09-06, #432: this entry read *"land on it or on something that holds it"*, and the ticket's own criterion asked for the other direction as well — *"the container reads it under reachable-via, and so does the VM that holds the container"*. Both are true sentences about reachability and the real estate needs both: every route in `testenv/hetzner/estate.json` lands on a container, so under the narrower reading no server in it would read a single route.)
_Avoid_: URL (that is a route's property), ingress, endpoint (bare)

**Monitor**:
A mirrored [item](#item) of the Uptime Kuma [source](#source), kind `monitor`: one check as Uptime Kuma defines it, with its current state. Attached to an [asset](#asset) by a `monitored-by` [link](#link); never an asset itself, and never created or edited in the mirror by hand. A **monitor name** kept on an asset is not one of these — it is the Uptime Kuma name a monitor of that name will answer to, waiting to be mirrored, and until then it is all knobas has. (Added 2026-09-06, #439, from spec #427's *"a name the mirror does not hold yet is kept on the asset and resolved by the next import or the M4.1 sync"*.) (Amended 2026-09-07, #453: a name arrives two ways and is resolved two ways, and the entry originally said only the [import](#import)'s. *Create monitor for this asset* records one too — it is what joins the monitor to the asset when it arrives, which is why the pane records the name before it queues the write — and **every poll of a monitoring source** resolves whatever names the mirror can now answer to, which is the other half of the spec sentence above. A monitor **created** from knobas goes into no container of its own: a monitoring source has nothing above a monitor that knobas mirrors, so the create addresses the source itself, `<source>:monitors` — deliberately not called a *roster*, which is this glossary's word for the [Monitors](#monitors) tab's list of rows.)
_Avoid_: check (that is one heartbeat of a monitor), probe, healthcheck

**Sample**:
One reading of one [monitor](#monitor) at one poll — its state, its response time, and when it was taken — appended by the sync engine at the end of every run of a source that emits monitors, whether or not that run changed anything. Knobas-owned, in the backup and never in a [share export](#share-export), swept by a retention setting defaulting to ninety days. Its *state* is knobas' vocabulary and not the source's: **warn** is derived here, at sample time, from the global response-time threshold, so a threshold changed later decides the next poll and never redraws what is already recorded. (Added 2026-09-07, #443, from spec #427's *"one sample per poll per monitor kept in knobas … so that history outlives Kuma's one-day pruning"*.) (Amended 2026-10-18, faster-polling grilling: a run writes a sample for a monitor only when its *state* differs from that monitor's last sample, or when the last one is a minute old — so a poll is no longer always a sample, a response time alone never forces one, and the response-time history is at most one point a minute. Every state transition is still sampled on the run that sees it, which is what an [alert](#alert) opens and closes on, and a [gap](#monitors) still means a half hour with no sample at all.)
_Avoid_: heartbeat (that is Uptime Kuma's word for its own check, and knobas' word for the app's own beat — see [passive attribution](#passive-attribution)), datapoint, metric

**Alert**:
A [monitor](#monitor)'s transition to down or warn, open until the monitor recovers; at most one open per monitor. Every open alert shows in the Monitors view and in the Assets view's tree; it reaches the [inbox](#inbox) only when some [context](#context) holds the affected asset, directly or through an ancestor. **Ack** is knobas-local — Uptime Kuma has no ack — and clears the inbox item while the alert stays open. **Only a return to *up* closes one**: a [sample](#sample) reading `pending` or `maintenance` neither opens an alert nor closes one, because silencing a monitor is not fixing it and an alert that closed itself when somebody paused it would be knobas reporting a recovery nobody made. (Amended 2026-09-07, #444: `monitor_sample`'s state column allows two words beyond the down/warn/up vocabulary this entry was written in, and the #443 merge review left the reading of them open; this is it, and the health rollup takes the same one — both words colour nothing.) (Amended 2026-09-07, #446: *some context* means an **unarchived** one, which is the one place the alert's routing rule and `member_ids` differ — `member_ids` is asked about a context by name and answers about it whatever its state, while the inbox is asking whether anybody is still working on this. An archived context is one the reader put away, and an alert that went on interrupting them because of it would be the inbox failing its own promise; the retired switcher drew the same line.) (Amended 2026-09-08, #504: the [launcher](#launcher)'s *Open alerts on my jobs* [smart list](#smart-list) is the third surface that shows an open alert — an [asset](#asset) with one that some unarchived context holds — and, like the Monitors view and the Assets view, it keeps an **acked** alert. The inbox is the only surface an ack clears, which is why its count and the list's differ by exactly the acked ones; the routing rule is the context clause, and the ack is the inbox's own lifecycle and not part of it.)
_Avoid_: incident, notification (bare), problem (that is the rolled-up count on a closed branch)

## Export

**Backup export**:
Everything knobas owns, notes included; the mirror is excluded (it re-syncs).

**Share export**:
A curated export — links, assets, contexts and [source](#source) configurations by default, with notes and time off; every part toggleable. *Time* is the timer, [blocks](#block) and [worklogs](#worklog): personal the way a note is, and a colleague reading a link map has no use for somebody's hours. The archive is the backup's own format restricted to those tables, named `knobas-share-<stamp>.knobas` so retention never deletes it, and the ordinary restore reads it. Two things it cannot carry, both because `pg_dump` restricts by table and never by row (#454): **the time settings**, since `knobas.setting` is one table holding every feature's bookkeeping and the recipient keeps their own; and **only some titles**, since an entity row is an address and the whole address book travels — a note's title crosses with notes off, its body does not. No credential is ever in an archive, so a restored source configuration lands as *missing secret* — and at no position at all, because no archive carries the [mirror](#mirror) either: a restored source reads its system from the top, or the links the archive brought would stay ids with nothing behind them (#455). The settings section lists share exports apart from the backups, since the retention sentence is true of one and not the other. Smart lists are built-ins, so there is no toggle for them until a saved one exists. (Amended 2026-09-08, #506: a saved one can now exist, so that sentence's condition is met — and the toggle, with the archive's `smart_lists` part behind it, is **#507**'s. #506 adds no export of any kind and nothing in `commands::backup` moves for it.) (Amended 2026-09-08, #507: built. The parts gain **`smart_lists`** — `knobas.smart_list` and nothing else, the one part that brings no address book, since a saved list is not an [entity](#entity). **On by default**, with links: a saved list is a query over the link map, not a private note about it. The toggle is drawn only once a saved one exists, and while none does the dialog sends the part **off** rather than merely hiding it — `pg_dump` writes a table entry whether or not there are rows in it, so leaving it on would name a list table in the archive of a knobas that has never saved a search. A query crosses **exactly as it was written**: the rule the [smart list](#smart-list) entry states about migrations is the same rule for an export and an import, and a table list is the one shape that cannot rewrite anything. The cap is not in the archive either — `create` is what counts and refuses, so a restore brings every list the archive holds, however many that is against this build's constant, and the cap reasserts itself the next time somebody saves one.) (Amended 2026-10-16, #812: the parts gain **`ticket_views`**, `knobas.ticket_view` and nothing else, on by default and on the smart lists' terms: no address book, a tick only once a [ticket view](#ticket-view) exists and the part sent off while none does, and the text crosses exactly as it was saved.) (Amended 2026-10-17, #870: the parts gain **`link_sets`**, `knobas.link_set`, `link_set_member` and `link_set_rule`, on by default and on the ticket views' terms; no address book, since a set names its items by id with no foreign key, and not `link_set_hidden`, since *Hide here* is one reader's choice.) (#283, #427, #454, #455, #507, #812, #870)

## Surfaces

**Launcher**:
The ⌘K box; one search over everything, under 100 ms. Its text search, and every other full-text read of the [mirror](#mirror), runs under PostgreSQL's `english` configuration with no `unaccent`, over a German and English corpus, on purpose and with named gaps (ADR-0030).
_Avoid_: command palette, spotlight

**Coverage**:
Per query, which sources could answer a filter and which have nothing it can match — measured on the [corpus](#mirror) each source contributed, never on what matched. A source that *answered* and found nobody is saying something different from one whose items never name a person, and coverage is the name for that difference. (#141)
_Avoid_: capability (a source's `SourceDescriptor` declares those — a static claim about a source, where this is a measured one about a corpus, which is why #141 was ruled onto the response and not onto the descriptor), support

**Room**:
**Retired with #881; the app no longer draws one.** The hub view a switcher entry opened, with tiles, activity and a tray, narrowed by a [context](#context) (a *stored* room) or by nothing, a source, or a source plus [project](#project) (a *derived* room). The ticket-first shell has no switcher and no hub: the [job](#job) is the working view of one ticket, a [ticket view](#ticket-view) or the [Tickets view](#ticket-board) lists tickets, a [link set](#link-set) gathers what belongs together, and Dependencies draws the links. Do not use the word for any of these. The text is in git history at 0f6c1806. (#209, ADR-0010, #881)
_Avoid_: dashboard

**Tile**:
**Retired with #881; the app no longer draws one.** One of a [room](#room)'s boxes. Its successors are the sections of a [job](#job) (Code, Builds, Assets, Monitors, Pages), each opening the view of its own kind (Builds, Monitors, Docs), and not boxes in a hub. What a tile drew survives as read orders, not as a tile: the **attention** order (settled [tones](#tone) not drawn, the rest loudest first, each run newest first) and **latest per parent** (only the newest row of each declared [parent](#parent), so a configuration's health is its latest build), both over the [mirror](#mirror) in SQL and never a source. The full tile text is in git history at 0f6c1806. (#680, #712, #718, #881)
_Avoid_: card, widget, panel (as the name)

**Ticket board**:
A ticket view's tickets in columns, each column holding the statuses the view puts on it, optionally split into lanes. Inside the Tickets view its layout toggle says just *Board*, because the view qualifies it. It replaces the retired [mini board](#mini-board). (ADR-0024, 2026-10-15)
_Avoid_: kanban, board (alone, outside the Tickets view)

**Mini board**:
**Retired with #881; the app no longer draws one.** The Tickets tile's status-grouped rendering inside a [room](#room). The [ticket board](#ticket-board) replaced it. "Board" never stands alone: launcher board, assets board, [ticket board](#ticket-board), Jira board. (ADR-0009, ADR-0024, #210, #245, #881)
_Avoid_: kanban, board (unqualified)

**Maximised tile**:
**Retired with #881; the app no longer draws one.** A viewing gesture on a [room](#room) that made one [tile](#tile) fill the grid. Nothing replaced it: there is no grid to fill. (#250, #881)

**Tree**:
The Assets view, at `#/assets` and `#/asset/<id>`, reached from Dependencies' *Asset tree* button: the [estate](#estate) as Miller columns, one column per level. Named so that "board" never appears unqualified (ADR-0009). Its fixed pane offers **Create monitor for this asset** where a configured [source](#source) offers the op — prefilled from the asset's route or its hostname, and drawn nowhere else: a source with only its API key offers no create, and the pane then shows no control rather than a disabled one. (Amended 2026-09-07, #453, spec #427 story 70.) (Amended 2026-10-21, #1459: the Tree has no sibling tab; its *Monitors* tab named a roster that went with the rooms UI in #1087.)
_Avoid_: board, assets board, columns view

**Spine**:
A column of the [Tree](#tree) collapsed to a 30px strip, labelled with the [asset](#asset) of the reader's own path that runs through it — or, for a column their path does not reach, with the first asset in it — and re-expanded by clicking it. What collapses is decided by the width left beside the fixed pane and never by depth: the point is that a deep path does not push the pane off the screen. (#430, spec story 28)
_Avoid_: breadcrumb (that is a line of names, not a column), rail, collapsed column (as a name — that is what a spine *is*)

**Wire**:
A line the [Tree](#tree)'s pane draws from a [route](#route)'s row to the asset at the route's far end — the **target** for a route the asset exposes, the **exposer** for one it is reachable via, since the target of that one is the asset the reader is already standing in. It lands on that asset's row, or on the [spine](#spine) hiding the column that lists it. **Dashed when the far end is reached through something rather than pointed at**: a spine standing in for the row, a row or a column scrolled out of view so that the landing is where the row *would* be, or — on a *reachable via* row only, since an exposed route's wire points straight at its target — a route that arrives at an ancestor of this asset rather than at this asset. A far end that no open column lists draws nothing — the Tree opens the columns of one path, and an asset off that path is not on the surface to be pointed at. (#433, spec story 31)
_Avoid_: edge, arrow, connector, link (that is knobas' own relation between entities, and a wire is drawn from a field)

**Monitors**:
A section of its own, at `#/monitors`, with two tabs: *Monitors* and *Assets nothing watches* (`#/monitors/unmonitored`, R-11.3). *Monitors* lists every mirrored [monitor](#monitor) as one row — its state, its type, what it watches, a 24-hour bar drawn from the [samples](#sample), the last check, Kuma's uptime ratios, certificate days remaining, the assets it is attached to, and one click to its page in Uptime Kuma. Above the list, one chip per state with a count, and clicking one narrows the list. The bar's segment is **half an hour** and reads as the worst state sampled in it; a half hour with no sample is a **gap**, and a gap says knobas was not watching rather than that nothing was wrong. (#448, spec #427 story 68) Above the list, on the same tab, one **card** per open [alert](#alert) — the monitor that fell, what it watches, how long it has been open, whether somebody has acked it, with the ack and one step to the affected asset; the *Assets nothing watches* tab is a roster of its own: every [asset](#asset) with no confirmed `monitored-by` link to a monitor, narrowed by a type filter whose options are the types that roster holds. *Assets nothing watches* is a statement about **attachment and not about attention** — an asset whose only monitor Kuma has paused is not on it, because somebody wired a check to it and then silenced it, which the *Paused* chip on the *Monitors* tab already says. (Amended 2026-09-07, #449, from spec #427 story 68's other half.) The *Assets nothing watches* roster is also a **smart list** on the [launcher](#launcher)'s board, which keeps the name *Not monitored* — one rule with two readers, and the two are held to the same answer in the same order by `the_launchers_not_monitored_list_is_the_monitors_tabs_roster`, because the launcher's copy is hand-written SQL and no compiler sits between them. (Amended 2026-09-08, #504.) A row on the *Monitors* tab also carries **Pause** or **Resume** — one or the other, never both — when the source it came from has an [account](#account) beside its API key; a source with only the key shows neither, because pausing is a socket.io login the key cannot make. The button says *queued*, not *paused*: the write goes through the write queue, and the row catches up when a read finds the monitor gone from `/metrics`. (Amended 2026-09-07, #452, spec #427 story 69.) (Amended 2026-10-25, refresh-after-write grilling: that read is the [refresh](#refresh) that follows the landed write, not the next poll, and the button's [echo](#echo) says *sent* between the two.) (Amended 2026-10-21, #1459: Monitors left the Assets view for its own section, `#/monitors` (#864, R-11.3); `#/assets/monitors` lands there.) (Amended 2026-10-21, #1563: one name per roster, and the tab each is on — the *Monitors* tab's list of monitors and the *Assets nothing watches* tab's roster of assets; the body had still called the second "the **Not monitored** roster" below the alert cards, as the rooms UI drew it on one page. The launcher's *Not monitored* smart list is a separate reader and keeps its name.)
_Avoid_: uptime page, status page (that is Uptime Kuma's own published page), dashboard, health tab

**Type convention**:
The child [asset](#asset) types the built-in table records as usually held inside one — `AssetType::suggests`, drawn by the create dialog as *usual here* and by nothing else. An **ordering, never a filter**: the dialog offers every declared type whatever the conventions say, and `assets::create` accepts any type under any parent. Deliberately not called a *suggestion*, which is already the word for a machine-proposed [link](#link) that is accepted or dismissed; a type convention is neither proposed nor persisted. What the conventions answer to is the real estate — `testenv/estate.json` (`testenv/hetzner/estate.json` until #634, 2026-10-12) — and not a diagram (ADR-0013). (#429, spec story 17)
_Avoid_: suggestion (that is a link), rule, constraint, allowed types

**Tidewater**:
The fictional company whose dataset seeds demos, fixtures, and the test environment.
