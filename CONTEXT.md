# knobas

A personal work cockpit: external work systems sync into one local database, and one search box, one link graph and one inbox sit on top of the mirror. This glossary is the canonical vocabulary. Code, plans and reviews use these words with exactly these meanings, and never the words under _Avoid_.

## Entities and kinds

**Entity**:
Anything with exactly one stable in-app address: a synced item, a context, a note, an asset.
_Avoid_: object, record

**Kind**:
The type of an entity: ticket, pr, build, page, repo, branch, commit, note, asset, route, monitor.
_Avoid_: type, category

**Note**:
A knobas-owned markdown document with `[[refs]]`, a searchable kind of its own and not an annotation on something else. Its birth and death are each one line in the activity stream; an edit is not a line, and neither is a `[[ref]]`, because autosave fires every 700 ms and the log is append-only. A [worklog](#worklog) draft reads a note's `updated_at` and nothing from the stream.

**Capture**:
A [note](#note) made from the global shortcut while another window has the focus, in a small window of its own, created on the first keystroke and never on an empty one. It attaches two ordinary [links](#link), `captured-in` to the [context](#context) the reader stood in and `captured-from` to the foreground entity, as [passive attribution](#passive-attribution) defines it. The in-app *New note* attaches the same two. Never a field on the note.
_Avoid_: quick note, scratch, jot, inbox (that is the stream of things to answer)

**Checkout**:
A clone of a [repo](#kind) on this machine, found under the **clones root** (a directory setting scanned two levels deep, each clone matched to a repo entity by host and owner/repo) or set by hand per repo. Knobas-owned data about the local disk, never a field of the mirrored repo, and never written to: no checkout, no ref moved (ADR-0016). The exceptions are *Clone* and *Clone all*, which clone into a new folder directly under the clones root (ADR-0023), and the fetch of a branch tip the mirror names into its object store, objects only, so *Files* and a compare can read it there (ADR-0041). *Files* and *New pull request* read a checkout that holds the mirror's tips first and check the source behind, in knobas's own process with no `git` started (ADR-0043); a *Files* tab that drew the source's tree while the checkout lacked the tip hands it to the checkout once the fetch lands at the same commit, and reads the checkout's tree a folder at a time, whole at any size (2026-11-13). Where *Open in editor* and *open a terminal here* point; "Open in" on a commit, a range, a pull request or a file at a line runs there too, with that object's checked git names (ADR-0034).
_Avoid_: clone (as a noun for the record: the clone is the directory, the checkout is what knobas knows about it), working copy, local path, worktree (one more checkout, set by hand)

**Application** (*Open in…*):
A program the reader named for *Open in…*, by a command per object: the repository's (required), and optionally a file's, a commit's, a range's and a pull request's. An application is offered on every *Open in…* for the objects it has a command for, and runs at the [checkout](#checkout) with no shell (ADR-0016, ADR-0034). Added and edited on its page in Settings › Checkouts and Open in, where *It will run* shows each command filled from a repository on this disk and *Try it* runs a saved copy (2026-11-13, `docs/decisions/2026-11-13-open-in-applications-get-a-page.md`). Knobas-owned settings, never synced.
_Avoid_: editor (an application may be a terminal, a file manager or a git client), tool (the *Diff tool* is git's own, not one of these), action, command (a program run and read, Settings › Commands)

**Working tree state**:
Where a [checkout](#checkout)'s working tree stands right now: its branch or detached head, its upstream and how far ahead and behind, and how many paths are changed, untracked or conflicted. Read by read-only `git status` when the checkout panel opens and when the window regains focus; never stored, never polled, and not run again where nothing git reads changed since its last answer (ADR-0021 as amended 2026-11-10; on Windows too since 2026-11-16). It stays git's own answer where a clone's commits are read in knobas's process (ADR-0043): what is changed in a working tree depends on the person's filters and settings. The branch name the mirror holds is compared to it, never passed to git (ADR-0021).
_Avoid_: git status (the command, not the fact), dirty/clean as the whole state, local branch

**Working tree** (as an end):
The files of a repository's [checkout](#checkout) as they stand on disk, uncommitted changes and the untracked files git does not ignore included, as one end of a comparison: written `~working-tree`, a name git refuses for a branch (ux26, ADR-0049). A head only, against a commit the checkout holds or a branch (then the merge base of its tip and `HEAD`). Always git's own answer, read again when the page is shown, on the window's focus and on a refresh, kept by the checkout's stamp so an untouched tree starts no git; never watched or polled. Ignored files are never listed, read or copied.
_Avoid_: local changes (the tab's two halves, against the index and `HEAD`), worktree (one more checkout), workspace

**Syntax role**:
What a run of code is to a reader (a keyword, a string, a key, a type), one of seventeen, carried by every token a highlighter answers and drawn by a class whose colour a [theme](#theme) sets (ADR-0051; `Role` in `render/roles.ts`). Never a colour and never an engine's own name for it.
_Avoid_: role alone (that is a source's [role](#role)), scope, token type, kind (that is an entity's)

**Theme**:
A name, a side (light or dark) and colours in two parts: the window's (any of its 51 colours, the rest [derived](#derived)) and code's (a colour for some [syntax roles](#syntax-role), with the diff's colours if it sets them; a role it leaves out draws in ink). A theme may hold either part or both (ADR-0055). Seven come with knobas, each with both; a person's own is a copy or an import, kept on this computer (Settings › Appearance › Theme and Code colours). Colours and nothing else: no font, radius or density.
_Avoid_: colour scheme, palette, skin

**Slot**:
Which [theme](#theme) draws for a side: *While light* and *While dark* for the window, and the same two for code, four in all (ADR-0055). A theme only fills a slot of its side.

**Colour-blind mode**:
A switch in Settings › Appearance that names each pair of state colours of the drawn themes that read the same under protanopia, deuteranopia or tritanopia (Machado, 2009), with their roles: *Attention and Failed read the same under deuteranopia*. Off until turned on; it warns and changes no colour.
_Avoid_: accessibility mode, high contrast

**Follows**:
A code [slot](#slot) that holds no theme of its own draws the window slot's theme of its side, or knobas's where that theme has no code colours. The start; *Set apart* keeps what it draws as its own.
_Avoid_: linked, synced

**Derived**:
A window colour worked out from the ones a [theme](#theme) sets, because it sets none (`app-derive.ts`); the editor labels it *auto*. knobas's own two themes are the empty set: every colour derived is knobas's own.
_Avoid_: default, computed, fallback (that is the stylesheet's value behind a token)

## Sync

**Source**:
One configured external system (a Jira, a Gitea, a TeamCity) that knobas syncs from.
_Avoid_: integration, provider, connector

**Adapter**:
The per-system implementation that speaks a source's API and emits its items.
_Avoid_: plugin, client

**External adapter**:
An [adapter](#adapter) installed from a repository that follows the [adapter standard](#adapter-standard), pinned to one release and updated only when the reader asks. Its opposite is a **built-in adapter**, compiled into knobas. Both are held to the same [role](#role) requirements.
_Avoid_: plugin, extension, add-on

**Adapter standard**:
What a repository must contain and implement for knobas to install an [external adapter](#external-adapter) from it.
_Avoid_: plugin standard, plugin API

**Source action**:
A write an adapter declares that belongs to no [role](#role)'s vocabulary, such as *Convert to draft*. The adapter declares its dialog; knobas queues it like any other write without knowing what it does and hands it back to the adapter.
_Avoid_: custom op, plugin action

**Role**:
A domain knobas has a working surface for (code host, ticket system, CI, wiki, monitoring, passwords), stated as what a source must provide to fill it: the [kinds](#kind) it emits, the [declared paths](#declared-path) each kind carries, the live reads it answers and the writes it may offer. A source may fill several roles. The shell draws a role's surface for every source that fills it, never for a named source; which sources fill a role is one answer, given by the [mirror](#mirror) to every surface that asks, and a surface never decides its fillers itself. An item whose source fills no role is on no role's surface and stays live everywhere else. A source whose items fit no built-in role declares a **custom role** of its own, whose view opens when one of its items is opened and which has no surface of its own (ADR-0029).
_Avoid_: capability (the descriptor's flags), type, category, frame (the shell's drawing of a role)

**Project**:
A source's own grouping of its items, in the source's own word: a Jira project, a TeamCity project, a Confluence space. Gitea's code has none; its repository is a kind, and only a Gitea issue mirrored as a ticket takes its repository as its project, because the ticket system needs one. Scoping to one repo is what the [parent](#parent) relation is for. Where projects nest, a project takes in its subprojects: `project:Payments` matches what sits anywhere beneath it. Not a [context](#context), which is a working set a person builds (ADR-0010).
_Avoid_: container, space (as a synonym; it stays as Confluence's own word), workspace, board

**Census**:
The whole-corpus report of every [project](#project) a source's live items show (`list_projects`), which the project pickers read. Not a window over the newest items, which would lose a quiet project.

**Mirror**:
The local synced copy of every source's data, with provenance. Readers see its [live items](#live-item); the readers that reach past them are listed under that term. A count of the mirror is a corpus, never a run's [Upserted](#upserted).
_Avoid_: cache, index

**By hand**:
A [source](#source) that syncs only when a person presses *Sync now*: no schedule, no run after a write, and no next run on its sources line ("by hand"). A password manager's sources start this way, because each sync may write a line under the person's name in the manager's audit log (ADR-0056). Settings › Sources › *Syncs* turns it on or off for any source.
_Avoid_: manual sync, paused (a paused source has a schedule it is not running), off

**Item**:
One mirrored record of a given kind.

**Payload**:
The raw source record an item carries verbatim, in the source's own shape, where everything not normalised (title, body text, author, updated time) lives.
_Avoid_: raw data, blob

**Payload read**:
A read into source-shaped data outside the adapter that shaped it. Permitted only where it can [miss](#miss), in one named statement, with its failure direction pinned (ADR-0007).

**Declared path**:
Where an adapter says a fact lives in its own payloads (a status, a priority, an assignee, requested reviewers, a merged flag, a project, the commit a code record stands at, the commits a build ran on), carried per entity kind on its descriptor. A reader resolves the declaration; a kind that declares none [misses](#miss).

**Parent**:
The entity you can open that an item sits directly inside: a repo for a branch, PR or commit; a build configuration for a build; the parent page for a page; the epic or parent issue for a ticket; the parent asset for an asset and the exposing asset for a route. An adapter declares it per kind as a [declared path](#declared-path). It is never drawn, so it is not a [link](#link); it is something you can open, so it is not a [project](#project). A parent need not be in the [mirror](#mirror). `under:<name>` scopes a search to everything beneath a parent, transitively, matching the declared names exactly and ignoring case, and returns the parent itself when it is mirrored. [Runs](#run) and commits still declare their parent, but the launcher leaves them out of every answer, so `under:` a build configuration answers with its [pipeline](#pipeline) alone and `under:` a repository answers without its commits. A name only left-out kinds sit beneath, such as a repository the mirror does not hold whose commits name it, answers with nothing and says so on a greyed-out token, as `kind:` does when it names a left-out kind and the answer is empty. An asset answers to its own name and `under:` returns it too. A child's searchable text never contains its parent's name; the launcher row's path line shows the chain instead. A title may still name its parent where it means nothing without it (ADR-0018).
_Avoid_: container, scope, holder, repository (as the name of this fact)

**State chip**:
What a row shows of where its item stands, for a kind whose adapter declares one: a pull request's `open`, `merged` or `closed`, a build's `running`, `queued` or outcome, a ticket's status. Declared as a [declared path](#declared-path) with a value map; each value is drawn with a [tone](#tone) or with none, and a value the map does not name is a [miss](#miss), so no chip. A value with no tone still matches: its chip draws the word in the neutral style and no later candidate is read. The map a reader sees is the adapter's with the reader's [tone overrides](#tone-override) applied. A ticket's state is its status where the adapter declares a tone for the word: on Jira, Done and Resolved are `ok`, In Progress and In Review `info`, Closed `muted`. The status is still the workflow column you move a ticket to. A status those names leave out takes the tone of its Jira status category, read after the name: in progress is `info` and done `ok`, and the chip still says the status; a to-do status, such as To Do, is a miss. The category colours a ticket and never archives its job. The launcher row draws the state as its monogram's tone, not as a chip; room rows, set dots and job sections read it like any other declared state.
_Avoid_: badge, status (the ticket's workflow word the state is read from, not this fact), state (alone)

**Tone**:
How loud a value is drawn, in five words the shell owns the colours of: `ok`, `warn`, `err`, `info`, `muted`. An adapter names a tone and never a colour, and a reader may set one per state word with a [tone override](#tone-override). The tone is the fact: what reads it (a build counted as failed for `err`, a row hidden as settled for `ok` and `muted`) follows the override, not only the colour.
_Avoid_: colour, severity, level

**Tone override**:
A reader's choice of [tone](#tone), or of no colour, for one state word of one entity kind: `FAILURE` on builds drawn `warn`, `Blocked` on tickets drawn `err`. Per kind, never per source or adapter, so it holds for every source of the kind. It replaces the tone of a word an adapter declares, and adds a word the mirror has seen at an open candidate, one whose words the instance invents (a Jira status name). No colour still matches and stops the walk over candidates. Clearing it restores the adapter's tone, or for a seen ticket status its Jira category's.
_Avoid_: custom colour, colour override, theme

**State colours**:
The reader's [tone overrides](#tone-override), seen per entity kind with every offered word: the words the adapters declare, then the words the mirror has seen at the kind's open candidates. Set in Settings › Appearance, read by every surface that draws a [state chip](#state-chip) and by the ⌘? legend, and never written into the stored parent declaration, so a change recomputes nothing.
_Avoid_: colour scheme, palette

**Miss**:
A payload read finding no recognisable shape and contributing nothing: the one failure a payload read is allowed. Guessing is the forbidden alternative.

**Upserted**:
The items one run wrote, new or changed. A per-run delta, not the size of the mirror.
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
A per-kind declaration that a full sync emits that kind's complete corpus. Only exhaustive kinds are swept; a budgeted kind is never exhaustive (ADR-0003).
_Avoid_: complete, full

**Budget**:
A cap on how much of a kind a full sync emits per container, such as commits per repo. A budget makes its kind non-exhaustive. On an incremental run it bounds what one run reads, never the delta: what it leaves unread, the next run reads.
_Avoid_: limit, cap

**Sweep**:
The engine pass that tombstones items of an exhaustive kind that a full sync no longer emitted. A run handed a position sweeps a kind too when its adapter walked that kind whole anyway, and says so.
_Avoid_: cleanup, purge, garbage collection

**Purge**:
Dropping a deleted source's items from the [mirror](#mirror) and tombstoning the entities they named, which is what *Remove source and its items* asks for. Applied inside the delete and again when a run in flight at delete time settles. Not a [sweep](#sweep): a sweep reconciles what a full sync no longer emitted; a purge carries out a deletion.
_Avoid_: sweep, cleanup, delete (entities are tombstoned, never deleted)

**Tombstone**:
Marking an item deleted-at-source while keeping the row. Tombstoned items leave every reader's view but the ones [Live item](#live-item) names.
_Avoid_: delete, remove

**Live item**:
An item that is not tombstoned and whose source is enabled, which is what a reader reads unless it is one of the four named here. Both halves are enforced by `sync.live_item`, never by a reader. A source with no configuration row is not disabled; its items stay live. The four readers that reach past the view, each reading `sync.item` for a stated reason: the Monitors roster (`assets::monitor_roster`, exempt from the tombstone half only, since a paused monitor is a tombstone and the roster must keep its Paused chip); the detail read (`get_entity`, exempt from both, since links and notes point at an entity whether its source withdrew it or was turned off, and the panel says which); the paste resolver (`resolve_url`, exempt from both, so a stale link opens what the detail can show); and the checkout read (`knobas_app::checkout::repo_of`, exempt from both, since the clone is still on this disk). A fifth needs a reason of its own and a line here.

**Watermark**:
A sync position that only advances as work completes. Its **ceiling** is the newest position the run witnessed at its start, which the watermark may never pass within that run. A ceiling below where the watermark already stands holds it there.

**Backfill**:
A deliberate full sync whose purpose is re-fetching unchanged items after the fetched payload widened. It runs with no stored position and keeps the one it ends on, so it is also the way out when an adapter refuses to sync on from a position the server no longer has or one too far behind to catch up. There is no separate cursor reset.
_Avoid_: refetch

**Refresh**:
Reading one item from its source and writing what the source holds into the [mirror](#mirror), outside any run. It moves no [watermark](#watermark) and sweeps nothing, with one exception: a source that reads an item only with what stands under it (a Gitea repository, branch or commit: its repository) walks that one [parent](#parent) as a run would, from where the stored position says it stood, and moves that parent's part of the position with its rows, since the source's change gates compare with the position (ADR-0047). A walk finds no position to start from, and the item gets *Sync now*, when a full sync is owed. It never overlaps a run of its source: a write's refresh waits for a run in flight and reads after it, and the reader's *Refresh* reads nothing while one is in flight and says the sync is running (2026-11-12), reads the page's own views again at once, and presses once more when that run ends (2026-11-20). It happens when a [write-back](#write-back) lands, on the item the write made or wrote to, with a run of the source following for what the write changed elsewhere; and when the reader asks on the item's detail (*Refresh*). A source whose adapter cannot read one item, or a kind it does not read one by one, has no refresh: its detail offers *Sync now* instead. For its one item it does what a run does for every item, links on arrival and a monitor's samples and alerts included; an item the source no longer has is tombstoned. A failed refresh changes nothing about the write, which has landed. Not a sync: catching a whole source up is a run (ADR-0031).
_Avoid_: single-item sync, resync, refetch

**Credential health**:
A source's authentication state as knobas last observed it, per source in the shell. It carries what the last check said went wrong, not what an adapter said about the far end when it went right; that is a [connection note](#connection-note).

**Discovered configuration**:
A configuration value an adapter learns from the instance during *Test connection* rather than one the user types. It reaches the Add-source dialog on the connection report and fills its field only when the field is empty; a typed value is never replaced. Jira's Epic Link custom field id is one. Not a secret, and never written by *Test connection* itself. A saved source that leaves such a field empty gets it at its next sync, which re-reads the source as a backfill.
_Avoid_: auto-detected, probed

**Connection note**:
The one line an adapter says about the far end that nothing else on the connection report says, shown wherever a test result is shown and never stored. Not [credential health](#credential-health), which a good sync may rewrite; a note is true of the far end as just found.
_Avoid_: detail (the field's name), diagnostic, warning

**Quota**:
How many requests a second, and how many at once, knobas sends one server: one per origin (scheme, host and port), shared by every source on it and by every request knobas makes there, a sync's, a live read's, a write's and an [external adapter](#external-adapter)'s alike; where two sources name different quotas, the stricter governs. The quota decides how many go, and a **lane** decides who goes next: a request a person waits for (a live read behind an open page or a pointer resting on a link, *Test connection*, a write they pressed) takes the next request the quota lets through ahead of any a sync or the write queue's tick has waiting. The sync sends nothing new while a person's request waits or is in flight, and leaves half the burst unspent, so a page's first requests go at once. It stands aside for at most a second without one of its own let through, and then a person's requests let it have the next one, so it is never stopped (2026-11-15). The time a sync request stands aside is not counted against how long it may wait, for at most five minutes more, so a person's reads never make a sync give up on the quota (2026-11-15). Neither lane changes how many requests the server sees (2026-11-13). The sync keeps at most two requests on a server at once, so a person's read meets two of its requests there, not a whole run's, and it goes on standing aside for 150 ms after a person's answer, so a read that needs a second round finds the server as the first left it (2026-11-15).
_Avoid_: rate limit (the code's word, `knobas-http`'s limiter), throttle, priority (the lane is about order, not a share of the quota)

## Links and contexts

**Link**:
A knobas-owned, typed, bidirectional connection between two entities, one record shape for every pair. One pair carries one active link per relation whichever way it was drawn, and the stored direction is what tells `blocks` from `blocked by`. Links live locally and are never written to a source.
_Avoid_: relation (a link's type), reference

**Suggestion**:
A machine-proposed link with a stated reason. Accepting makes a link; dismissing persists. The backend still makes them; the app has drawn none since 2026-11-05 (`docs/decisions/2026-11-05-cut-dependencies-and-proposed-links.md`).
_Avoid_: recommendation

**Backlink**:
A page whose body links to this page.
_Avoid_: pages that link here, inbound link

**Context**:
The working set behind a [job](#job), and nothing else. Membership is explicit adds plus direct links plus one hop out, computed over confirmed links only and never stored; asset membership also counts through ancestors (ADR-0008). Opening a ticket's [page](#ticket-page) promotes its ticket to one, in either mood. The context routes an [alert](#alert) to the inbox, scopes the job's proposals and lists the open jobs; a reader never sees one by name, adds to one or makes one. Its one state is **archived**: a ticket's context is archived exactly while the ticket's status is in its source's declared done-like set, maintained by the sync run, and un-archived when the status leaves it. Ad-hoc contexts are retired; time without a ticket is the ⌘T label timer.
_Avoid_: workspace, project (a source's grouping, ADR-0010), room (retired; a [job](#job) is the working view)

**Job**:
The ticket you are working on. A job is a ticket's; ad-hoc work has no job. The **chosen job** is the ticket last made the job on purpose: by its [ticket page](#ticket-page)'s foot (*Resume work*, *Work on this*), by running *Start work*'s steps (pressing *Start work* only shows them), by opening a `#/job/<entity id>` address (a bookmark, history, a link, the timer chip, the Tickets preview's *Resume work*), or by starting the timer on it. Reading a ticket never chooses it. `#/job/<entity id>` chooses the ticket and lands on its ticket page, which then sits on the yellow; home is a bare `#/job`, which draws the chosen job's ticket page; with none chosen, home is *All tickets*, where knobas opens, and a chosen job knobas no longer holds is forgotten there (2026-11-20). The choice survives a restart. The chosen job is what the timer, home, the launcher's *Link to* and a view's *This job* fall back to, and its card and row are yellow on the Tickets board and list. There is no job page apart from the ticket page and no Job entry in the nav (spec #2396).
_Avoid_: task, workspace

**Link set**:
A named group of repositories, pipelines, pages, assets and monitors, each a fixed item or a pattern, with rules saying which tickets it applies to (under an epic, sub-tasks of a ticket, in a project, or one ticket by hand). A [job](#job) shows a set's members beside its context's [members](#member), never as members: read when the job is, never stored as links, never walked through (ADR-0025).
_Avoid_: template, bundle

**Member**:
An entity the membership rule reaches for a given context, computed at read time and never stored. A proposal never makes a member.
_Avoid_: item (the mirror's word)

**Ticket view**:
A saved way of looking at tickets in the Tickets view: a name, a group, which tickets (conditions stored as search text, tickets only) and how they show (a [ticket board](#ticket-board) or a list, columns, lanes, card fields). A **Jira view** is one the Jira source keeps for a board or a favourite filter; Jira decides which tickets it holds, and only how it shows is set in knobas. Not a [smart list](#smart-list), which spans every kind and has no layout.
_Avoid_: saved search, filter, board (alone), ticket view for one ticket's page (that is the [ticket page](#ticket-page))

**Ticket page**:
One page for a ticket and the [job](#job), at `#/ticket/<entity id>`, under Tickets in the nav the way a run opens under Builds, its breadcrumb *Tickets › KEY*. On the left the card, one card in two moods: on a sheet while the ticket is only read, on the yellow while it is the chosen job, and the yellow is the only difference. The card holds the key with a copy button, the status, the facts list, the sections table (a row per section the ticket reaches, with its count and worst state, opening that section's view on *This job* for this ticket without choosing it; *Sub-tasks* first when the ticket has children), *Open in <source>*, *Link something to it*, and at its foot one action: the decision while reading (*Resume work* or *Work on this*, each choosing the ticket, or *Start work*, which shows the steps and chooses once they run), the timer (*Start timer* or *Stop and log*) once chosen. On the right the reading column: the title, the description with its inline images, the attachments folded to one line, and the whole conversation with its compose box. Every ticket link opens it; opening it chooses no job, and comments and links are writes on any ticket. It is the open detail for [passive attribution](#passive-attribution). Not a [ticket view](#ticket-view), and not the Tickets view's preview pane, which keeps its own drawing (spec #2396).
_Avoid_: ticket view, ticket detail, job page (one page with the ticket page since spec #2396)

**Smart list**:
A launcher query a reader saved with ⌘S: a row in `knobas.smart_list` holding the raw box text and its name. The only lists there are; knobas ships none. In the [launcher](#launcher) a saved list is kind `list` (monogram `LI`), found behind `#l` by its name or its text, its text as the path line; Enter puts that text back into the box, filters and words, and runs it, and the reader stays in the box. A list can be renamed and deleted, and comes to **need attention** when today's grammar refuses its stored text, an old `list:` or `/source` included: its row is coloured as a warning and says why, and Enter says so instead of applying it. A saved list stores text and never a parse; no migration ever rewrites a stored query, and there is no editor for one. At most eight saved lists, held in `saved::MAX_SAVED_LISTS`; `create` refuses the ninth and nothing else enforces it. The [share export](#share-export) carries saved lists verbatim.
_Avoid_: filter, saved search, built-in list

**Favourite**:
An entity or a [smart list](#smart-list) a reader keeps at the top of the [launcher](#launcher)'s empty box, set with *Favourite* and *Unfavourite* in the Tab sheet: a row in `knobas.favourite` holding the entity id or the list id and a position. The empty box draws them under one Favourites heading, lists and entities together in the order they were favourited, then Recent. Deleting a list forgets its favourite; an entity favourite whose entity is not known yet is kept and drawn once it is. The [share export](#share-export) carries them in the saved lists' part.
_Avoid_: bookmark, starred

**Key**:
A chord a reader binds in the [launcher](#launcher): a modifier and one letter or digit, read by the key's code and spelled `⌘⇧O` (⌘ ⌃ ⌥ ⇧, then the key). That spelling is what is stored; off the Mac it is drawn by the keys' names, `Ctrl+Shift+O` for `⌃⇧O` and `Win` for ⌘ (2026-11-14, `shell/platform.ts`). A [smart list](#smart-list)'s key (`knobas.list_key`, *Assign key…* in its Tab sheet) opens the launcher on the list from anywhere, launcher closed or not. An action's key (`knobas.action_key`, ⌘B on the action in the Tab sheet) is kept per kind and runs that action on the selected row of the kind, so one chord can mean *Open in VS Code* on a repository and *Approve* on a pull request. Bound in the key panel; the launcher's own chords (⌘K, ⌘S, ⌘B, ⌘D, ⌘?), the shell's (⌘T, ⌘R) and the system's editing and window chords are refused there. The [share export](#share-export) carries both tables in the saved lists' part.
_Avoid_: shortcut, hotkey, binding

## Acting on sources

**Write-back**:
An operation that changes data in a source: status, comment, approval, trigger, worklog, page, pausing a [monitor](#monitor). Links, contexts, notes and [blocks](#block) are never write-back; a [worklog](#worklog) is one.

**Compare**:
Where a [pull request](#kind) is proposed: *Into* is its base and *From* its head, any two branches of the repository, the default branch on either side (2026-11-13: a pull request from the default branch brings it into another). *From* may also be the [working tree](#working-tree-as-an-end) (ux26): then nothing is proposed, the form says where the checkout stands, and what it changes is the working tree against *Into*'s merge base with `HEAD`. What it changes and the commits it carries are the source's compare from *Into* to *From*. A branch's own view measures the branch against the default branch, so it speaks for the compare only while *Into* is the default and *From* is not.
_Avoid_: diff page (the diff between any two commits, at its own address), branch view

**Account**:
A second credential stored beside a source's ordinary one, in the same keychain item: a username and password, optional, for a source whose write channel its ordinary credential cannot open. Uptime Kuma is the only one: its API key opens `/metrics` and nothing else, so a Kuma with only the key reads every monitor and offers no [write-back](#write-back), and adding the account turns pause and resume on. Not an auth method, which is how a source authenticates its ordinary traffic. Added from the Add-source form or from the credential strip the sources view's **Account** button opens.
_Avoid_: second secret, write credential, admin login

**Direct write**:
A write to a password manager (add an entry, edit one, change its password) sent once when the person presses *Save* and answered before the dialog closes, never put in the write queue. If the source does not answer, nothing is kept and the dialog keeps what was typed. A direct write is not a [write-back](#write-back) in the queue's sense: it cannot wait, retry or be undone from the queue, because it may carry a [protected value](#protected-value) (ADR-0056).
_Avoid_: queued write, write op (for these), sync back

**Secrets**:
The surface (`#/secrets`) and navigation section for [secret entries](#secret-entry) and [secret folders](#secret-folder) from sources filling the `passwords` role (ADR-0056, #2515). Searched in the [launcher](#launcher) behind `#s`, which restricts to `password_entry` items only; `/` outside a field and `⌘/` anywhere on the Secrets page open the launcher pre-seeded with `#s `, while `⌘F` focuses the local filter. Legacy `#/passwords/...` addresses redirect to `#/secrets/...`, and internal database and SPI identifiers (`passwords`, `password_entry`, `password_folder`) stay unchanged.
_Avoid_: Passwords (former surface name), vault

**Protected value**:
A value a password manager hides behind a reveal: a password, a field the source marks protected, an earlier password, an attachment's bytes. Read live from the source, held in the app process for at most 30 seconds (the *hold*), and written to no table, file, log, cache or queue. A [code key](#code-key) is not a protected value in this sense: it is kept, in the system's password store. A note or a field the source sends openly is not one either: the [mirror](#mirror) holds it (ADR-0056, #2515).
_Avoid_: secret (now the [Secrets](#secrets) surface and its entries), credential (a source's own sign-in), sensitive data, password (for the general term)

**Code key**:
The key a [secret entry](#secret-entry)'s one-time codes are made from, read by a sync and kept in the system's password store, never in the [mirror](#mirror), so codes work offline and read nothing from the source. On Pleasant it is the entry's own `TOTPSecret` or one of four KeePass plugins' custom fields; an account the source does not let see it gets none (ADR-0056).
_Avoid_: TOTP secret (the vendor's word), seed, OTP

**Secret entry**:
The [kind](#kind) (`password_entry`) of one password manager record: its username, address, notes, the custom fields the source sends openly, its folder, its rules and rights, and never its [protected value](#protected-value) or [code key](#code-key). Its title stays in the [mirror](#mirror) and never reaches `knobas.entity`, so link lists, link sets, the timer and the standup never name it; it takes part in none of them (ADR-0056, #2515).
_Avoid_: password entry (former UI label), credential (Pleasant's word), login, item (the generic word)

**Secret folder**:
The [kind](#kind) (`password_folder`) of a password manager's folder: its name and parent, whether it asks a usage comment, whether the account may add to it. The [Secrets](#secrets) page's tree is drawn from these; the manager's own root is not emitted (ADR-0056, #2515).
_Avoid_: password folder (former UI label), group (Pleasant's word), vault, collection

**Section**:
A heading of any level on a wiki [page](#kind) and everything under it until the next heading of any level, so a section never holds a heading. The unit knobas edits a page in, one section of prose at a time. A section holding a macro or a table, or sitting inside a table, refuses its edit and offers the source's own editor. A section is edited as text, so lists and inline formatting come back as plain paragraphs, and the surface says so before the edit. The edit that goes out carries the whole page body, so everything outside the section is sent back as it arrived.
_Avoid_: block (a unit of [time](#block)), chunk, fragment

**Embed**:
A part of a wiki [page](#kind) that is HTML as its author wrote it, or an `http(s)` URL, which the Docs reader runs in an isolated frame that cannot reach knobas, its storage or its commands. Confluence's `html` and `iframe` macros and its widget connector are embeds. A placeholder draws in its place only when the source's *Run embedded HTML* setting is off, knobas is offline, or the frame fails to load; a placeholder names a part knobas does not draw, where an embed is drawn. A [section](#section) holding one refuses its edit (ADR-0040).
_Avoid_: macro (the source's word for many things), widget, iframe (one way an embed is drawn)

**Include**:
A part of a wiki [page](#kind) that draws another page, or only that page's excerpt, in a light frame reading "from *page title*" that opens it. The source names the target by space and title; knobas fills it from the [mirror](#mirror) when the page is read, never at sync, so it shows the target as the mirror holds it now, offline too. Its words are found in search on the target's page, not on the page that includes it. A placeholder that names the target and links to it draws instead only when knobas cannot fill it: the target is not mirrored, has no excerpt, sits more than three includes deep, or is already on the path. A [section](#section) holding one refuses its edit (ADR-0040).
_Avoid_: transclusion, embed (an [embed](#embed) runs HTML), excerpt (what an excerpt-include takes from its target)

**Collapsible**:
A titled part of a wiki [page](#kind) whose blocks the Docs reader draws closed, opening it when the reader arrives on a search hit or an anchor inside it. Confluence's `expand` is one. Its contents are drawn in full once opened and are in the page's search text, where a placeholder draws nothing of what it stands for. A [section](#section) holding one refuses its edit (ADR-0040).
_Avoid_: expand (Confluence's word), accordion, details

**Pending write**:
An edit queued because its source cannot currently accept it. The queue is a visible, inspectable list, not a count. Delivery is at least once: a write in flight when knobas stops may arrive twice, and knobas re-sends rather than guess and never merges or deduplicates what you wrote (ADR-0012). A write that went out and got no answer is different: if a second arrival would duplicate it, knobas holds it and asks you to check the source (a [held write](#held-write)).

The word has two senses and both are load-bearing. This glossary's sense is the whole open queue, pending, held and refused together, which is why the command that lists it is `pending_writes`. The state machine's sense is narrow: `pending`, `held` and `refused` are three disjoint states of `knobas.write_queue`, and `write_queue::counts().pending` counts only the first. The write-queue list takes the wide sense; the launcher footer's "N pending writes" takes the narrow one, since those are the writes that leave on their own, and the status bar's badge is where a held or refused write asks for a decision.

**Held write**:
A queued write knobas will not send until the user acts, for one of three stated reasons: its target changed after it was queued (resolved by choosing between the two versions side by side; there is no silent last-write-wins), its source was turned off (resolved by re-enabling it), or knobas sent it and the source never answered, so it may have arrived (resolved by checking the source, then *Send again* or *Discard*). The third is only for a write a second arrival would duplicate (a comment, a create, a worklog, an approval, a review, a build, a transition); a write the source refuses on repeat, or that sets what is already set, waits and goes on its own. A create held this way counts as on its way: its page draws it queued, offers no second create, and knobas refuses a second monitor of its name until you send or discard. *Changed* means changed in what the write is about: a write that overwrites holds on what it would overwrite; a move holds when the ticket's status moved, and an edit when a field it sets did, never on a status or value an earlier write of the same person's was going to set; a write that adds holds only when the target's declared state moved or it left the mirror. A file commit's target is the file itself, held when the source says the file changed since it was read. The surface always says which reason. In the state machine it has left `pending`; in the wide sense it is still a pending write.

**Echo**:
What a page draws of a write it has queued, muted beside what the source holds: *queued* while the write is open, *sent* once it has landed, gone when a read of the page shows the write itself. A write that is held, refused or withdrawn takes its echo with it. The read it waits for comes when the write's [refresh](#refresh) ends or a run of its source does. A ticket's move and its field edits are echoes too: the card stands in its new column, or shows its new value, with *Queued* then *Sent*, and an open one is drawn again from the queue after a restart.
_Avoid_: optimistic copy, placeholder, ghost, queued mark

**Note** (on a card or a field):
What a card or a field says of a write the source refused or knobas holds: the source's own sentence, drawn from the queue's open row, gone when that row is discarded or sent.
_Avoid_: error, warning, banner

**Reachable transition**:
A status a ticket's workflow offers from where the ticket stands right now, read from the source and never stored in the database. The window keeps each answer for the session, by the ticket as it stood, asks ahead when a card is rested on, selected or pressed, and asks again whenever a card is picked up; the status menu and the board's columns offer these and only these, and where no answer is held yet a drop is taken and the source decides. The write is unchanged: it names the status, and the adapter resolves and refuses by name. One read on the `Source` trait, refused by name where a source has no workflow.
_Avoid_: available transition, allowed status, workflow step, transition id (Jira's word)

**Ticket field**:
One thing of a ticket a person can change besides its status: its summary, assignee, priority, labels, or a field of the source's own. The source says per ticket which it lets a person change, in what kind of control and with which choices, and what it holds now; knobas draws the control where the value is read and sends the change as one write (ADR-0054). An adapter built before 1.6.0 of the adapter interface offers none, and its fields read as text.
_Avoid_: attribute, property, custom field (Jira's word for some of them)

**Source view**:
What an [adapter](#adapter) draws for one of its own kinds when the detail opens: a live read, never stored in the database, answered as sections of typed **blocks** from one fixed vocabulary (facts, text, entity list, diff, log, settings, actions, a directory's tree, a file). The shell renders the vocabulary and never learns which adapter drew it. A section may be **deferred**, its blocks read when the reader opens it; a block may name a section that opens in its own tab, which keeps its place and a *Back*. A value a view lets a reader change names the [write-back](#write-back) op that changes it, and the change goes through the write queue; a view never writes. An **action** is a write a reader starts from the view, drawn in the view's **toolbar** or in its one section; the source declares its dialog, any fixed values, and whether it can be taken now. One it cannot take yet is drawn disabled with the source's reason; one that can never apply is not drawn. An adapter with no view for a kind answers nothing and the detail draws the generic payload projection; a failed read draws the same under a line saying why (ADR-0020). A view the adapter says stands at a fixed commit is kept by the [read cache](#read-cache).
_Avoid_: page (a wiki kind), template, layout, widget, panel (the detail is the panel; a view fills it)

**Read cache**:
The app process's memory of [source views](#source-view) and of files' bytes (a README's image), keyed by what each stands at: a read at a whole commit sha is kept for the session and the same ask is answered from memory with no request to the source; anything else is read live. The adapter says which before the read. Bounded by bytes, least recently used out first; never written to the mirror's tables, and dropped with the built source it was read through when that source's address, configuration or credential changes (ADR-0032). A page's reads are also **held** between sessions in `sync.held_read`, beside the mirror and outside any backup: a page answers from what knobas last read, says from when, and reads the source behind it, so only a read never made waits (ADR-0042); and not every one of those: a view never read is drawn first from the mirror's record of the entity and of the entities under it, where the adapter can draw it (a pull request's overview; a repository's branches and its last commit), said to be as of the last sync. A compare's list of files between two branch names is held too, and its whole diff under the two commits it settled where it fits, so Compare without a clone draws a list read once at once, under the held line, and checks it behind (2026-11-13). Each open pull request's *Files* is held by the sync before anyone opens it, read after a run only when its merge base or head moved, so *Files changed* draws at once on its first open too; such a row is **speculative** until a page draws it, and leaves a full store first (2026-11-15).
_Avoid_: mirror (the database's copy, written by sync), store, snapshot

**Answer cache**:
The window's memory of the [mirror](#mirror)'s answers to a page's reads (`app/src/lib/ipc/answer-cache.ts`, 2026-11-12): the last answer to each read that is a function of the mirror and the settings, so a page drawn again, or opened for the first time once its reads were asked in the background (**warmed**: at start, the reads the last session made, and after a run, the ones it dropped), draws in the frame after the press. An answer goes the moment the mirror may have moved under it: a run that changed something drops what a source of its roles could change, and a write or a refresh drops everything (a ticket page's promotion only when it made the context; a visit keeps every read that never reads visits). Not the [read cache](#read-cache), which is the backend's memory of a source's views.
_Avoid_: read cache (the backend's, of source views), store, prefetch

**Held ahead**:
A section's page the shell opens hidden once the window is ready and the reader still, before its nav entry is pressed (`app/src/lib/tf/ahead.ts`, 2026-11-15): Tickets, Builds, Code, Monitors, Inbox and Docs, one at a time, each into a free room of *Pages kept open* behind every page held, so its first press shows a page already built. It is a kept page in every other way, reads what it draws and pauses while hidden; a page the reader leaves takes its room first.
_Avoid_: preloaded, prerendered, warmed (the [answer cache](#answer-cache) warms reads, not pages)

**Speed targets**:
What a page, a key and a scroll must do on the saved scale database in headless Chrome with the page's main thread slowed (Björn, 2026-11-14): a page usable within 100 ms of its press at CPU 1x, 200 ms at 4x and 300 ms at 6x, a key's rows drawn within 100, 200 and 200 ms, and at 6x a list scrolling with no visible stall. 6x stands for a slow work laptop and weighs as much as the other two. `just open-budget`, `just keys-budget` and `just frame-budget` hold all three (`testenv/perf/README.md`), and a pass that changes what the app draws runs them. A repository's and a pull request's inner pages are held at all three too (`just open-budget --inner`, since 2026-11-15), against the live forge or, on the Mac, a stand-in serving the scale estate's own repositories built again from its seed. The targets hold at every window size, full screen and at high DPI (since 2026-11-15): the budgets run 1440 x 868 at scale 1, and a 4K screen at 200 % and a 1440p screen at 125 % at 4x and 6x.
_Avoid_: performance budget (a budget is the file of lines that holds a target), fast enough

**Stored card**:
Each ticket's card as the database keeps it beside the [mirror](#mirror) (migration `0075`, `sync.item_card`; since 2026-11-15, `0089`, with its title, its date, its key and title lowered, and its project): every field a [ticket board](#ticket-board) draws, filters and cuts by, under the key of the declarations it was read with, written with the ticket and filled at start for what was there before. A source is **whole** (`sync.card_whole`) when every ticket of it has its card under the key and none is under another; a board whose sources are all whole reads the cards and nothing else, and one with a source that is not reads the mirror as before, walking the payload of a ticket with no card. The same answer either way.
_Avoid_: card cache, ticket snapshot, tile (the retired dashboard piece)

**Stored run**:
What Builds reads of a run, and a commit's own sha, as the database keeps them beside the [mirror](#mirror) (2026-11-15, migration `0088`, `sync.item_run`): a run's fifteen declared fields and a commit's declared sha, under the key of the declarations they were read with, written with the item and filled at start for what was there before. Builds' list, a pipeline's runs and a repository's Builds tab read a run's row where it has one and its payload where it has none, the same answer either way; a run's commit is found by the stored sha.
_Avoid_: run cache, build snapshot

**Stored tree**:
A branch's file list as the sync keeps it beside the [mirror](#mirror), for every branch of every repository (2026-11-14, migration `0085`): each directory once by its tree sha, as git keeps it, and each branch pointing at its root tree and the commit it was read at, so branches that share a directory share one copy. After each run the sync reads the branches whose head moved, only the directories it does not hold, behind a person's reads: default branches first, then pull requests' and the person's own, then the rest, a batch a run. A withdrawn branch drops its pointer in the sync's own transaction, and a directory no branch reaches goes. *Files* without a clone draws a branch's tree and *Go to file* from it at once where its pointer stands at the commit the page reads; a tree past 100,000 entries is not stored, and the page says so. A default branch's README is stored with it, its bytes by its blob sha (2026-11-15, migration `0090`), so a landing reads nothing from the forge. A clone still comes first ([ADR-0041](docs/adr/0041-knobas-reads-a-branchs-tree-at-its-checkout-and-fetches-its-tip-there.md)).
_Avoid_: file list cache, snapshot, held read (a page's read of the source, kept between sessions)

**Diff**:
The line-by-line difference between two versions of a set of files, as a source view's diff block carries it: per file its path and old path, what happened to it, its line counts and its hunks. Parsed from the unified diff text a git server answers, or that is read at a checkout holding both commits (ADR-0036; only there can whitespace be ignored), by one parser every git adapter shares. At a checkout that text is written in knobas's own process, the bytes `git diff` prints, and by git itself where a setting of the repository's would change them (ADR-0043). Capped per file and per view; a file over the cap keeps its counts and says it was cut. A range's diff may be read as its **file list** first (each file's paths, change and counts, no lines; every file, with no per-view cap) and each file's lines as the reader reaches it, a page at a time: the files and lines are those of the whole read. At a clone the list's **names** come first of all (each file's paths, change and modes, no counts, 2026-11-14), drawn while the counts are read and filled in place. A pull request's diff also carries its **line threads** and may take new line comments.
_Avoid_: patch (the text before it is parsed), changeset, delta

**Rendered comparison**:
A file of a [diff](#diff) drawn by a renderer on both sides, before and after, in place of its lines: *Lines | Rendered* in its header wherever a renderer can draw both sides (an image, Markdown, a diagram, a PDF, a page, a command renderer's output), *Rendered* in the diff's band for every such file, remembered (ux26). It stands in the **stage**, whose height comes from the kind of renderer and never from what lands in it, so a side that is late moves nothing. Each side is the file's bytes at its end, read at the clone first.
_Avoid_: preview, visual diff (it marks no pixel), side-by-side (that is split, the lines' layout)

**File pipeline**:
The ordered steps between a file's bytes and its renderer (base64 decode, gunzip, jq, pretty-print, …), each taking bytes or text and giving bytes or text. Run in the backend under a size cap and a timeout per step, and kept in the [read cache](#read-cache) by (sha, path, steps). In the viewer it is *Pipeline*, on the open file; in code `file_pipeline`, `PipeStep`, `PipeRun`. A **named pipeline** is one saved with a name, which a default per file pattern names by id (so editing it changes every pattern that uses it); a **command step** is one of the person's own programs as a step, under ADR-0033. It also runs on a value of a file (a **value pipeline**, ADR-0059), and has the steps a value is often wrapped in: hex decode, inflate, URL decode, unescape XML or HTML, JSON string, JWT, read as a character set.
_Avoid_: pipeline alone in code or docs (a CI [pipeline](#pipeline)), filter (jq's word for one step), transform

**Folder renderer**:
One of the person's own programs that takes a folder of a repository and makes a picture, text, Markdown, a page or a folder of files out of it, saved in Settings › Renderers and drawn on the Code page in the file's place (ADR-0044, 2026-11-16). It is given `{path}`, a copy of the folder knobas wrote at the commit shown (from the clone's objects, else the forge), and may write `{out}`; it never runs in the checkout. Which folders it takes is two glob lists: a file the folder holds, and the folder's name or path. What it drew is kept by the folder's content, so a revisit draws at once. In code `folder_render`, `FolderRenderer`.
_Avoid_: folder preview, folder plugin, renderer alone (a file's)

**Command** (*Run…*):
One of the person's own programs, saved in Settings › Commands, that is run once on a press on a file, a folder, the repository, a commit or a comparison, and whose output opens in a dialog (ADR-0060, 2026-11-24). It runs in the [checkout](#checkout) with no shell, as an [application](#application-open-in) does, and knobas waits for it and reads what it prints. It never stands in a file's place: a program that should is a renderer. What it is offered on is read from its placeholders. On a [generated folder](#generated-folder) it runs in the folder knobas kept, with its paths, and never in the checkout (2026-11-25). What it runs on is its target: a file, a folder, the repository, a commit, a comparison, files, or a generated file or folder (`OneOffTarget`, `made` and `made_files`). In code `oneoff`, `OneOff`.
_Avoid_: task, tool, action, script, linter (one use of it), renderer

**Finding**:
A place in a file of the checkout that a [command](#command-run)'s output said something about, with a severity and a rule where the program wrote them. Read from the lines, never from a report format. *Problems* lists them.
_Avoid_: diagnostic, issue (a forge's), error (one severity)

**Pick** (a folder's):
The [folder renderer](#folder-renderer) a person chose for one folder of one repository, kept for it (`render.folder_picks`, ux26): every place a folder's renderer is chosen writes it, and the folder gets it wherever it is drawn while that renderer is still saved and still takes the folder; else the first saved one that takes it. While several take a folder and none is picked, the Files tree's glyph asks which.
_Avoid_: default (a file pattern's renderer, Settings › Renderers), preference

**Generated folder**:
The files a [folder renderer](#folder-renderer) whose command holds `{outdir}` wrote in one run, kept by knobas as plain files under `<clones root>/knobas-rendered/` and drawn on the Code page in the real folder's place, each by the file renderers (ADR-0046, 2026-11-20). Links, `.git` folders and names Windows does not take are left out and said. Kept by the folder's content, a folder per run, never rewritten. In code `Made`, `folder_render::keep`, `folder_render::store`.
_Avoid_: output folder (the tool's own `{outdir}`, before it is kept), render cache, preview

**Generated compare**:
A [folder renderer](#folder-renderer) run on one folder at each side of a [diff](#diff), each side in a lane of its own, and what the two runs made compared: for a [generated folder](#generated-folder), only the generated files that differ (added, deleted or changed, sizes first, bytes only where sizes match), the rest counted; for a single output, its two drawings (ux26, ADR-0049). It stands in the diff's file list in place of the folder's real files while the band's *Rendered* is on (a renderer with a glob takes the folder; one with none only by its glyph), and only *Run* runs it. A folder with no changed file under it is never one. Where a side is the [working tree](#working-tree-as-an-end), a file that differs only in line endings is counted apart. In code `FolderCompare`, `folder_render_compare`, `MadeCompare`.
_Avoid_: folder diff (the real files' diff), rendered diff (a file's, a [rendered comparison](#rendered-comparison))

**API explorer**:
How an OpenAPI or Swagger file is drawn: a rail that lists its endpoints by tag, by path or by schema, a page for the one picked, and beside it a request a person can send. A spec is a file, so the explorer is its renderer, drawn wherever a file is (ux31, `docs/decisions/2026-11-22-an-api-spec-is-explored-sent-from-and-opened-in-a-client.md`). In code `oas` (`TfOpenApi`, `TfOas…`); a **rail**, a **place** (`get /pets`, `schemas/Pet`, `tags/pets`). Drawn small it is the **inline form** (`TfOasInline`): a Docs code block's *API*, a comparison's two sides.
_Avoid_: Swagger view, Try it out, API client

**XML view**:
How an XML file is drawn as a document: its own lines in the explorers' pane, with folds, addresses and paths that open, values that can be looked at, and a query (`docs/decisions/2026-11-23-an-xml-file-is-read-as-a-document.md`). It is the default for every XML file. In code `xml` (`TfDocView`, `tf-xv-`); with the JSON view to come, the **document view**, in code and prose only.
_Avoid_: XML viewer, tree view, Checked

**Schema explorer**:
How an XML Schema file is drawn: a rail that lists its elements as a tree, by element or by type, a page for the one picked, and beside it an XML example made from it. It reads the file with every file it reaches as one **schema set** (`docs/decisions/2026-11-23-an-xml-schema-is-explored-across-its-files-and-an-example-is-made-from-it.md`). In code `xsd` (`TfXsd…`, `tf-xsd-`).
_Avoid_: XSD viewer, schema browser, diagram

**Schema set**:
An XML Schema file and every file it reaches by include, import, redefine and override, read as one model. Each file is found in a fixed order, and says how. In code `SchemaSet`.
_Avoid_: project, workspace, closure

**Not found here**:
What a file a schema names is when no step finds it. It is said in place, never fetched by itself.
_Avoid_: missing, unresolved, broken

**Outline**:
A document read into columns of numbers, a row a node, in document order, in the document worker. What folds, what a query answers and where a value stands are rows of it. In code `XmlOutline`, `readXml`.
_Avoid_: DOM, tree, AST

**Value viewer**:
The file viewer opened on a value of a file (an element's text, an attribute), with the pipeline bar on top. A **value pipeline** is a [file pipeline](#file-pipeline) whose first step, `Take`, takes the value out of the file (ADR-0059). A chain **kept** for a field applies to every field whose names end the same way, in the files its pattern takes, and opens on a press. A person **looks at** a value.
_Avoid_: decoder, inspector, preview

**Zen mode**:
A file on the Code page shown alone, the repository's crumbs, header, tabs and tree hidden, for a renderer that wants the room (Markdown, PDF, JAR, an image, an API spec, an XML file, a diagram, a page). `Z` or the file header's *Zen*; `Esc` leaves it. A way of reading, never a setting (ux31, `app/src/lib/tf/zen.svelte.ts`).
_Avoid_: full screen, focus mode, fill the window

**Pipeline**:
What a CI source runs: TeamCity's build configuration, a GitLab pipeline, a Jenkins job. The Builds view is drawn over pipelines and their [runs](#run); the source's own word appears once, in the view's header (ADR-0022).
_Avoid_: build configuration (TeamCity's word), job (a [job](#job))

**Run**:
One execution of a [pipeline](#pipeline): TeamCity's build, a GitLab pipeline run, a Jenkins build. It has a number, a state, a branch and commit, steps, a log, tests and artifacts (ADR-0022). A [command](#command-run)'s run is one start of it on one thing; in prose that holds both, *a command's run* and *a build's run*.
_Avoid_: build (TeamCity's word; also the Builds view), execution

**Checks**:
Whether the commit a pull request, branch or commit stands at is green, from two places drawn apart. The source's own **commit statuses** are a live block of the [source view](#source-view)'s *Overview*. The **builds of a commit** are the [mirror](#mirror)'s: build rows whose declared revision is the sha the code row declares, drawn by the shell as the detail's *Builds* line. Both are drawn failing first, and neither draws anything where nothing reported.
_Avoid_: CI status, pipeline (what runs, not whether a commit is green), check run (GitHub's)

**Line thread**:
The review conversation on one line of a [diff](#diff), oldest first, on the old or the new side, drawn under its line. One whose line is no longer in the diff is **outdated** and drawn apart at the end of its file; one on a file the diff no longer holds is kept at the end of the diff. A comment the reader wrote and has not sent with a review is **pending**, kept by the shell until a review carries it or the reader discards it, never the mirror's.
_Avoid_: inline comment, annotation, draft review (Gitea's server-side draft)

**Unclaimed write**:
Something a source made because knobas asked, that knobas has no record of. It happens in one window: a [pending write](#pending-write) withdrawn while in flight, where the source takes the write and the row has already settled as withdrawn. It matters for `create_ticket` and `create_page`: the artefact is at the source and nothing links it to what asked for it. Knobas cannot warn before the fact and says so afterwards, with an *unclaimed* line in the activity log against the container the write was made under, carrying the source's own id for what it made where the source named one. Deleting the artefact is a person's job at the source (ADR-0012).
_Avoid_: orphan, leaked write, ghost ticket

**Inbox**:
The single actionable stream, with actions and snooze: mentions, review requests, failed builds, assignments, credential expiry, [alerts](#alert), and failed sign-ins. A failed sign-in is raised once per episode, the run of consecutive failing verdicts for a source whose [credential health](#credential-health) says the sign-in fails until somebody acts; any other verdict ends the episode and the item.
_Avoid_: notifications, feed

**Desktop notification**:
What the operating system shows for one new [inbox](#inbox) item, when that item's category is switched on and the window is not focused; clicking it opens the item. Per category, all off until somebody says otherwise, once per item, and one rule for every category (`addressOf`). An [alert](#alert) is keyed `alert:<monitor>`, so a monitor that flaps is announced once a session. Nothing that is not an inbox item is ever notified. Always spelled in full.
_Avoid_: notification (bare), alert, toast (the in-window message)

## Time

**Timer target**:
The one entity (ticket, page, note, repo, asset) or ad-hoc label a running timer is attributed to, and what a [block](#block) records. A [context](#context) is never a target: it is a set, and time on a set has nowhere to go.
_Avoid_: timer context, context

**Block**:
A knobas-owned stretch of time with a start, an end and a [timer target](#timer-target), the unit everything about time is built from. **Manual** when the timer made it, **passive** when attribution recorded what was open. Blocks stay local; none is written to a source, and a passive block is never logged without a person saying so. A block remembers which [worklog](#worklog) it was logged into and which [context](#context) the timer was started in.
_Avoid_: interval (a worklog's editable span), entry, session

**Worklog**:
The Jira record that one or more [blocks](#block) become when logged, a [write-back](#write-back) through the write queue with a local copy. Sending it changes nothing about the blocks; withdrawing its queued write unlogs them again. A worklog Jira has answered for is not withdrawable that way. At-least-once like every write (ADR-0012).
_Avoid_: time entry, logged time (the timesheet's column)

**Digest**:
The standup's generated lists, drawn from the [mirror](#mirror) and the activity stream for the configured usernames, every line linking to its item. *Yesterday* is the newest day before today with any of your activity, at most seven days back; *today* adds the running timer's [target](#timer-target); *blockers* are your items whose status their source declares blocked-like plus those a confirmed link marks blocked by; *infrastructure* is every monitor [alert](#alert) open since the start of the last working day, and is not mine only. The first three are mine only: the source's own attribution (on Jira the assignee), the writes made through knobas, and the local [worklog](#worklog) copy.
_Avoid_: report, summary, standup (the whole flow)

**Standup protocol**:
A [note](#note), one per date, holding attendees, per-person notes and action items. *Publish* creates a Confluence page from it under a configured parent and links note and page; the note stays the editable original. One page per date: a redelivery knobas cannot see is refused by Confluence's per-space title uniqueness rather than duplicated.
_Avoid_: minutes, protocol page (the published copy)

**Timesheet**:
The week under the day strip, Monday to Sunday with empty weekends collapsed: a row per [timer target](#timer-target) plus one for focused time no [block](#block) covers, and per day the time **tracked** (manual blocks), **offered** (passive ones, beside tracked and never inside it), **logged** ([worklogs](#worklog) pending or sent) and **unlogged**. A worklog waiting on a person shows as **held**. Minute granularity and no rounding: knobas never invents a rounding policy your Jira may not have. *Log all* makes one worklog per day and ticket from that day's unlogged manual blocks and never touches a passive or label one.
_Avoid_: report, timecard, week view

**Passive attribution**:
The opt-in recording of which entity was in the foreground (the open detail, else the asset in the [Tree](#tree)'s pane, else the chosen [job](#job)'s page at home, else nothing) while the window is focused, as passive [blocks](#block) derived from [observations](#observation). A gap stays a gap until a person assigns it; nothing recorded this way reaches a source on its own. Browsing the Tree with nothing selected is a gap. Observations are kept a month.
_Avoid_: automatic tracking, activity tracking (the activity stream's word)

**Observation**:
One heartbeat's record of what was in the foreground at a single instant, an entity or nothing, written only while [passive attribution](#passive-attribution) is on. It says nothing about what was open in between, which is why a passive [block](#block) is derived from a run of observations and never from one.
_Avoid_: beat (the heartbeat's tick, which survives as code shorthand; the reader is shown only *observation*)

**Observation horizon**:
The instant before which knobas has thrown its [observations](#observation) away: a fact about what was swept, so a profile nothing has been swept from has no horizon. A day past it is **absent, not zero**, and the day review and the timesheet say so.
_Avoid_: retention cutoff, thirty days ago (the horizon is only what a sweep took)

## Estate

**Estate**:
Everything the Assets view holds: the tree of [assets](#asset), their [routes](#route) and the [monitors](#monitor) attached to them. Built by hand and by seed, and it is the real test infrastructure (the development notebook, its VM and Docker engine, and the containers on it), never a fictional one.
_Avoid_: inventory, topology, infrastructure (what the estate models)

**Asset**:
A knobas-owned entity (a server, a container, a service, a database, a runtime) with a type, typed and custom properties, and a place in the estate's tree. Its place is its **parent**, a field of its own and never a [link](#link): the tree is structure, relations are links (ADR-0014). Never a mirrored [item](#item): an accepted [import](#import) makes an asset carrying an origin line, after which no sync overwrites what a person edited.
_Avoid_: resource, node, host (one type of asset)

**Import**:
Loading assets from outside, an estate file or the file an [importer](#importer) produced, with a preview of what is already in the tree and what is new. What it makes are ordinary assets with an origin line.
_Avoid_: sync (the mirror's word), seed (what the test environment does to a source)

**Importer**:
A producer of an estate file from a live system (hcloud, a Docker host), offered by the [Import](#import)'s chooser and applied by the same Import. Not a [source](#source) (ADR-0015): it mirrors nothing and appears nowhere sources do, and its credential lives under the `importer:` keychain namespace. **Producer** is the wider word the code and wire carry: every importer is a producer, and the estate file picked off the disk is the one producer that is not an importer. Its witness is the real system: what it produces previews as *already in the tree*.
_Avoid_: import adapter, asset adapter, asset source, connector

**Origin key**:
The property an [importer](#importer) sets that names the thing it read in the system's own terms (`hcloud_id` for a server; the docker context plus the container name for a container, since a container's id changes on every recreate), which the [Import](#import) matches on when the file's id is not one the tree holds. Every part of a key is a property, including one that repeats a column. The second matching rule beside the id, and the only one.
_Avoid_: external id, foreign key, source id (a [source](#source)'s)

**Depends on this**:
What breaks if an [asset](#asset) goes down: its descendants, and every asset linked to it by `depends-on` or `runs-on`, transitively over both, read from the link's `to` end. The panel in the asset pane that answers *blast radius*. Routes whose target is the asset or anything in its answer break too and are not counted: those not already in the pane's *Reachable via* are listed beneath as *other routes that would break*, and a line says the rest would.
_Avoid_: blast radius (the question, not the panel), dependents, impact, downstream

**Route**:
A knobas-owned entity an [asset](#asset) exposes: a URL or endpoint, with or without a target asset. An asset is *reachable via* the routes whose target is anywhere on its own containment path: on it, on something that holds it, or on something it holds.
_Avoid_: URL (a route's property), ingress, endpoint (bare)

**Monitor**:
A mirrored [item](#item) of the Uptime Kuma [source](#source), kind `monitor`: one check as Uptime Kuma defines it, with its current state. Attached to an [asset](#asset) by a `monitored-by` [link](#link); never an asset, never created or edited in the mirror by hand. A **monitor name** kept on an asset is the Uptime Kuma name a monitor will answer to; an import records one, so does *Create monitor for this asset*, and every poll resolves whatever names the mirror can now answer to. A monitor created from knobas addresses the source itself, `<source>:monitors`.
_Avoid_: check (one heartbeat of a monitor), probe, healthcheck

**Sample**:
One reading of one [monitor](#monitor) at one poll: its state, its response time and when it was taken, appended by the sync engine. A run writes a sample only when the state differs from the monitor's last sample or the last one is a minute old, so every state transition is sampled and the response-time history is at most one point a minute. Knobas-owned, in the backup and never in a [share export](#share-export), swept by a retention setting defaulting to ninety days. **warn** is derived at sample time from the global response-time threshold, so a threshold changed later never redraws what is recorded.
_Avoid_: heartbeat (Uptime Kuma's word, and the app's own beat), datapoint, metric

**Alert**:
A [monitor](#monitor)'s transition to down or warn, open until the monitor returns to up; at most one open per monitor. A [sample](#sample) reading `pending` or `maintenance` neither opens nor closes one. Every open alert shows in Monitors, in the Tree and in the launcher's *Open alerts on my jobs* list; it reaches the [inbox](#inbox) only when some unarchived [context](#context) holds the affected asset, directly or through an ancestor, and never while its monitor is hidden on [Monitors](#monitors). **Ack** is knobas-local and clears the inbox item while the alert stays open; the inbox is the only surface an ack clears. It can be taken back: the alert asks again, in the inbox too.
_Avoid_: incident, notification (bare), problem (the rolled-up count on a closed branch)

## Export

**Backup export**:
Everything knobas owns, notes included; the mirror is excluded, since it re-syncs.

**Share export**:
A curated export in the backup's own format restricted to tables, named `knobas-share-<stamp>.knobas` so retention never deletes it, read by the ordinary restore. Parts: links, assets, contexts, [source](#source) configurations, smart lists with the [favourites](#favourite) and [keys](#key), ticket views, link sets and [state colours](#state-colours) on by default; notes and time (the timer, [blocks](#block) and [worklogs](#worklog)) off. A part whose table is empty is sent off rather than hidden. Because `pg_dump` restricts by table and never by row, the time settings cannot cross and the whole address book of entity rows does. No credential is ever in an archive, so a restored source lands as *missing secret* and at no position, since no archive carries the [mirror](#mirror). Saved queries cross exactly as written. It never carries a [command](#command-run).

## Surfaces

**Site**:
A [source](#source)'s own website shown inside knobas: a native web view under the top bar, one per source, on a page at `#/site/<source id>`, offered once *Open its pages inside knobas* is on for the source. It can reach nothing of knobas, and knobas reads nothing of it but its address and its title. Going to another knobas page hides it and keeps its page alive; closing it is the bar's × or ⌘W, and asks first when the page holds unsaved changes. `Esc` is the site's, and a second `Esc` within half a second goes to the page knobas holds for where the site stands, else for the item it was opened at, never to what opened the site (ADR-0050; #2559).
_Avoid_: browser, tab, web view (the mechanism), embed (a page's own HTML in a frame), room (retired)

**Site session**:
What a [site](#site) keeps on this computer between runs: its cookies, storage and cache, one store per source, apart from knobas's own page and from every other source's. The person signs in there, on the site's own page; no [credential](#credential-health) of knobas's is sent to it. Not in a backup or a share export, removed with its source or by *Forget…*.
_Avoid_: profile (the demo or the real knobas), login (knobas's own to a source), cookie jar

**Allowed address**:
An origin a [site](#site) may show besides its source's own: a sign-in host the person said yes to when the site was sent there, or a second address of the same system. Per source, listed in Settings, never taken from a mirrored record. An address outside the list opens the browser. On a password manager's site it is also the only origin besides the source's own that anything may load from, unless the page on top is on an allowed origin, which loads as in a browser (ADR-0050, ux30).
_Avoid_: whitelist, trusted site, base URL (the source's own, which is always allowed)

**Web sign-in**:
A username and password saved for a [site](#site), kept in its source's keychain item beside the credential and typed into the site's own sign-in form: on *Fill sign-in*, and by itself once a page loads where the adapter says its sign-in form is. Only on the source's own origin, with "remember me" ticked; never a one-time code, never a provider's page. Settings shows the username alone (ADR-0050, ux29).
_Avoid_: account (Uptime Kuma's second API credential), saved password (says where, not what), auto-login

**Link handoff**:
Opening matching external URLs for an opted-in [source](#source) directly in knobas through the Firefox Native Messaging host (`dev.knobas.desktop`), and any mirrored item's URL through the OS `knobas://open?url=...` scheme, which someone asks for explicitly. Configured per source in Settings › Sources as *Open matching links in knobas*, defaulting to *Known items only* (`known_items`, mirrored items resolved through `resolve_url` and declared `UrlForm` patterns) with an optional *Whole source* (`whole_source`) mode that routes non-item URLs to the source's [site](#site) when *Open its pages inside knobas* is on. Every *Open in browser* action in knobas records a short-lived one-shot bypass in memory so Firefox lets that navigation load normally without looping back into knobas (ADR-0061). The Firefox side is the *knobas link handoff* extension in `extension/firefox/`, whose toolbar button pauses it.
_Avoid_: URL interceptor, redirector, protocol proxy

**Update**:
A newer knobas than the one running, offered on Settings › Updates and marked by a dot on the Settings cog. knobas
finds it in its own GitHub releases, downloads it on a press, checks it against its own signing keys, backs up, and
restarts into it through the ordinary quit. Never an older version (ADR-0052).
_Avoid_: upgrade, patch, auto-update (nothing installs unasked), adapter update (an [external adapter](#external-adapter)'s)

**Channel**:
Which versions knobas offers as an [update](#update): *Releases*, the finished ones and the default, or *Pre-release
builds*, the newest published release or pre-release, whichever is newer. A choice of the installed copy, kept in a
file beside the profile, not in a backup.
_Avoid_: track, ring, beta, nightly

**Pre-release build**:
A version published on GitHub as a pre-release, tagged `vX.Y.Z-<word><number>` (`v3.2.9-ux28`), older than the release
`X.Y.Z` it leads to. Björn's word for it. *Build* alone stays a CI source's.
_Avoid_: beta, nightly, build (a [run](#run))

**Launcher**:
The ⌘K box; one search over everything, under 100 ms. Its text search, and every full-text read of the [mirror](#mirror), runs under PostgreSQL's `english` configuration with no `unaccent`, on purpose and with named gaps (ADR-0030). A word's first characters, one to three letters or digits still being typed, are answered by names, under *Titles* and *Everything* alike: the rows a word of whose title or key begins with them, those whose name begins with them first (a title past its stop words, the text after a title's last `/`, a ticket's key, a pull request's number, the key cell), then the newest, a row a kind at a time until the page is full. A stop word answers as any other start (`s`, `in`, `the`). Such an answer counts no match, so its totals are the rows it holds. Under *Everything* the text is asked too where no name holds the characters, and for two or three characters where the names are few, filling no kind's ten rows and not the page: its rows follow the names in the room they leave. The ranking by words alone begins at the fourth character, at a finished word and at anything that is not all letters and digits (2026-11-15 and 2026-11-16, `docs/decisions/2026-11-15-one-letter-is-answered-by-names.md`, `docs/decisions/2026-11-16-two-letters-by-names-and-keys-wait-for-their-answer.md`), and holds while 2,000 rows or fewer hold the query. A query more rows hold is answered by the rows it names, ranked among themselves (the newest of them where it names more than 2,000 too), and under *Everything*, where they leave a kind room, by that kind's newest rows whose text holds it, not ranked; its totals are the rows it holds. At every length the page is dealt a row a kind at a time: every kind's first row, then every kind's second (2026-11-16, `docs/decisions/2026-11-16-a-word-many-rows-hold.md`). Between a key and its answer the list is what it was, the board or the answer before. A wait is said once it has lasted 400 ms: the rows kept are dimmed, and a list that drew nothing says what it waits for. Enter, the arrows, Tab and an action's chord pressed meanwhile wait for the answer and act on it. Beside PostgreSQL's words, every name is also read in memory by a picker, from two typed letters (2026-11-19, ADR-0045, `docs/decisions/2026-11-19-names-are-found-as-a-picker-finds-them.md`): a name that holds every typed word in one piece, in any order, is found by its parts (`proxy` finds `ADProxy`, `mapping service` finds `abcMappingService`), and from four letters one that holds the letters in order as an abbreviation does (`mapsvc`), with one typo where nothing else answered. These names join PostgreSQL's answer without moving a row of it, the loose ones last in their kind, and the matched letters are marked. `'word` is one piece, `^word` a name's start, `word$` its end, `!word` and `-word` leave a name out; a ticket's key is never read loosely. A ticket's key typed with a space or none (`pay 123`, `pay123`, `pay_123`, `pay.123`, any case) is read as the key (`PAY-123`) where the mirror's tickets are numbered under that project, so `top 10` stays two words unless TOP is one: the key is asked beside the words, its rows lead each kind and the tickets come first, the words' answer follows, and a part of the number lists the keys it begins. The Docs search and the Tickets view's and boards' filters read it too, a ticket view's text is counted so, the Tickets view marks the key whole, and the first key typed after a start is read (2026-11-20, `docs/decisions/2026-11-20-a-key-typed-with-a-space-is-the-key.md`). Under *Everything* a typed word is also read against the words the mirror holds (`sync.word`): one nobody wrote is asked again as the nearest written word, or two written words side by side, where the text found nothing; one somebody wrote also asks for the longer words it is part of, whose rows follow the text's. The Docs search reads the same way, kept to pages, and Code's repository filter and Docs' spaces filter rank what they hold by the same rules (`fuzzy_rank`), one to three letters kept anywhere in a row. Under *Everything* a repository is also found by its files' names and text ([files index](#files-index), ADR-0050): a typed word of four letters and more is read as the start of an identifier's part (`adproxy` and `proxy` find `ADProxyServer`), one of three letters as a whole word. A row found by its text and not by its name says where under its title, in one evidence line (the place, then the passage with the words marked, or a file's name and line), and opens there. Settings › Search says what *Everything* matches beyond names and keys.
_Avoid_: command palette, spotlight

**Files index**:
The names and the text of every mirrored repository's files, at its default branch's head, in a tantivy index in the profile (`<profile>/files-1/`, `knobas-files`, ADR-0050), read after each sync from a checkout on this machine where one holds the file, else from the forge. PostgreSQL keeps what it stands at (`sync.code_repo`, `sync.code_file`); bring-up compares the two. A backup does not carry it, as it carries nothing of `sync`: the next syncs read it again. What it keeps of file text is bounded in Settings › Search.
_Avoid_: code search, grep

**Text search**:
A repository's Files tab searching the text of its files, every line holding the words typed or a pattern, in the tree the tab shows: the branch's commit in a clone on this computer, the working tree as git lists it, or, where no clone holds the default branch, the [files index](#files-index)'s copy of it, which the answer names. Words typed apart match in the order typed on one line with anything between (`xy room` finds `xy_room` and `xyRoom`), a `"`-quoted phrase as typed (ADR-0062, amended 2026-11-27). Smart case unless *Aa*; *.\** for a pattern. *Files | Text* in the tree's search box, `Mod+Shift+F`, and *Search in repository* on selected code in a file or a diff (ADR-0062). Not the launcher's search, which finds a repository by its files and never a line in one branch.
_Avoid_: grep, code search, find in files (*Find in file* is ⌘F in one file)

**Section's place**:
The last address a reader stood on in a section of the nav, however deep (a Docs page or space, a ticket, a pull request at its tab, a run at its tab, a marked monitor, a link set, a source's page in Settings): where the section's nav entry, the gear or `⌘,` comes back to from another section, as the kept page that was left (2026-11-13, `docs/decisions/2026-11-13-a-nav-entry-comes-back-to-where-you-were.md`). Pressed inside the section, the entry goes to the section's own page as it was left instead (its tab, saved view, pipeline or search). Not a card row's narrowing and not home. A launcher row naming a section opens the section's own page, not its place.
_Avoid_: last page, history (Back steps through history; a place is one per section)

**Docs' home**:
Where Docs opens when nothing in it is chosen: a space's own page or one page, or, with none chosen, Docs' own page (`#/docs`). The Docs entry's first press, the entry pressed inside Docs and the launcher's *Docs* row go there; a [section's place](#sections-place) in Docs still wins on a return. Set from a space's or a page's right-click menu (*Open Docs here*) or Settings › Docs; one the mirror lost opens `#/docs` (2026-11-17, `docs/decisions/2026-11-17-where-docs-opens-and-the-spaces-it-hides.md`). Knobas-local, a `knobas.setting` row.
_Avoid_: start page, default page (`#/docs` is Docs' own page, which the crumb leads to)

**Hidden space**:
A Docs space taken off Docs' browsing: no tree in the rail's list, no tile on Docs' own page, last in the Space picker, and a row of the fold at the end of each list. Search still finds its pages, a link into it opens, and the sync keeps mirroring it. Knobas-local, a `knobas.setting` row a space; a backup carries it and a share export does not.

**Coverage**:
Per query, which sources could answer a filter and which have nothing it can match, measured on the corpus each source contributed and never on what matched. A source that answered and found nobody says something different from one whose items never name a person.
_Avoid_: capability (a static claim on the descriptor; this is a measured one about a corpus), support

**Room**:
Retired; the app no longer draws one. The hub view a switcher entry opened. The [job](#job) is the working view of one ticket, a [ticket view](#ticket-view) lists tickets, and a [link set](#link-set) gathers what belongs together. Do not use the word for any of these.
_Avoid_: dashboard

**Tile**:
Retired; the app no longer draws one. Its successors are the section rows on a [ticket page](#ticket-page)'s card and the views they open. What a tile drew survives as read orders: the **attention** order (settled [tones](#tone) not drawn, the rest loudest first, newest first) and **latest per parent** (only the newest row of each declared [parent](#parent)), both over the [mirror](#mirror) in SQL.
_Avoid_: card, widget, panel (as the name)

**Ticket board**:
A ticket view's tickets in columns, each column holding the statuses the view puts on it, optionally split into lanes. Inside the Tickets view its layout toggle says just *Board*, because the view qualifies it (ADR-0024). A card is moved by dragging it to a column, which lights where it can go; a column of several statuses asks which.
_Avoid_: kanban, board (alone, outside the Tickets view)

**Mini board**:
Retired; the app no longer draws one. The Tickets tile's status-grouped rendering inside a [room](#room), replaced by the [ticket board](#ticket-board). "Board" never stands alone: launcher board, assets board, ticket board, Jira board (ADR-0009).
_Avoid_: kanban, board (unqualified)

**Maximised tile**:
Retired; the app no longer draws one. A gesture that made one [tile](#tile) fill a [room](#room)'s grid.

**Tree**:
The Assets view, at `#/assets` and `#/asset/<id>`, the top bar's *Assets* after Monitors (Björn, 2026-11-19); a row, an alert or a search opens it at an asset too, and that page's breadcrumb leads to `#/assets`. It draws the [estate](#estate) as Miller columns, one column per level. Its fixed pane offers *Create monitor for this asset* where a configured [source](#source) offers the op, and shows no control at all where none does. It has no sibling tab.
_Avoid_: board, assets board, columns view

**Spine**:
A column of the [Tree](#tree) collapsed to a 30px strip, labelled with the [asset](#asset) of the reader's own path that runs through it, re-expanded by clicking it. What collapses is decided by the width left beside the fixed pane, never by depth.
_Avoid_: breadcrumb (a line of names), rail, collapsed column (as a name)

**Wire**:
A line the [Tree](#tree)'s pane draws from a [route](#route)'s row to the asset at the route's far end: the target for a route the asset exposes, the exposer for one it is reachable via. It lands on that asset's row or on the [spine](#spine) hiding its column, dashed when the far end is reached through something rather than pointed at. A far end no open column lists draws nothing, and one with a column or a [spine](#spine) drawn after it is its landing dot alone, since a line to it would cross that column's labels (2026-11-19).
_Avoid_: edge, arrow, connector, link (knobas' relation between entities)

**Monitors**:
A section of its own at `#/monitors`, with two tabs. *Monitors* lists every mirrored [monitor](#monitor) as one row (state, type, what it watches, a 24-hour bar from the [samples](#sample), last check, uptime ratios, certificate days, attached assets, a link to Uptime Kuma), one chip per state above it, and one card per open [alert](#alert). The bar's segment is half an hour, reading as the worst state sampled in it; a half hour with no sample is a **gap**, meaning knobas was not watching. A row pressed opens the monitor beside the roster (`#/monitors/at/<id>`), where its actions are: *Pause* or *Resume* only when its source has an [account](#account) (the button says *queued*, the write goes through the queue, and the [refresh](#refresh) after it lands catches the row up), *Hide*, and *Link an asset…* and *New asset for it…*, which draw its `monitored-by` link to an asset picked in the estate or made for it at the estate's top. A **hidden** monitor leaves the roster for the rows drawn after it under one line (*N hidden monitors*), all of them, nothing folded; every filter and scope applies to it as to the roster, and there the row is dimmed, its state drawn in no tone and no *Ack*, and its open monitor offers *Unhide*. Its [alerts](#alert) reach no [inbox](#inbox), so it raises no [desktop notification](#desktop-notification). Knobas-local and not a [write-back](#write-back): Uptime Kuma goes on checking it, and its alert still opens and closes. *Assets nothing watches* (`#/monitors/unmonitored`) is a roster of every [asset](#asset) with no confirmed `monitored-by` link, a statement about attachment and not attention.
_Avoid_: uptime page, status page (Uptime Kuma's own published page), dashboard, health tab

**Type convention**:
The child [asset](#asset) types the built-in table records as usually held inside one (`AssetType::suggests`), drawn by the create dialog as *usual here* and by nothing else. An ordering, never a filter: the dialog offers every type and `assets::create` accepts any type under any parent. Not a *suggestion*, which is a machine-proposed [link](#link). The conventions answer to the real estate in `testenv/estate.json`.
_Avoid_: suggestion (a link), rule, constraint, allowed types

**Tidewater**:
The fictional company whose dataset seeds demos, fixtures and the test environment.
