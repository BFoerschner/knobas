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
A clone of a [repo](#kind) on this machine, found under the **clones root** (a directory setting scanned two levels deep, each clone matched to a repo entity by host and owner/repo) or set by hand per repo. Knobas-owned data about the local disk, never a field of the mirrored repo, and never written to: no checkout, no fetch (ADR-0016). The one exception is *Clone*, which clones into a new folder directly under the clones root (ADR-0023). Where *Open in editor* and *open a terminal here* point; "Open in" on a commit, a range, a pull request or a file at a line runs there too, with that object's checked git names (ADR-0034).
_Avoid_: clone (as a noun for the record: the clone is the directory, the checkout is what knobas knows about it), working copy, local path, worktree (one more checkout, set by hand)

**Working tree state**:
Where a [checkout](#checkout)'s working tree stands right now: its branch or detached head, its upstream and how far ahead and behind, and how many paths are changed, untracked or conflicted. Read by read-only `git status` when the checkout panel opens and when the window regains focus; never stored, never polled. The branch name the mirror holds is compared to it, never passed to git (ADR-0021).
_Avoid_: git status (the command, not the fact), dirty/clean as the whole state, local branch

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
A domain knobas has a working surface for (code host, ticket system, CI, wiki, monitoring), stated as what a source must provide to fill it: the [kinds](#kind) it emits, the [declared paths](#declared-path) each kind carries, the live reads it answers and the writes it may offer. A source may fill several roles. The shell draws a role's surface for every source that fills it, never for a named source; which sources fill a role is one answer, given by the [mirror](#mirror) to every surface that asks, and a surface never decides its fillers itself. An item whose source fills no role is on no role's surface and stays live everywhere else. A source whose items fit no built-in role declares a **custom role** of its own, whose view opens when one of its items is opened and which has no surface of its own (ADR-0029).
_Avoid_: capability (the descriptor's flags), type, category, frame (the shell's drawing of a role)

**Project**:
A source's own grouping of its items, in the source's own word: a Jira project, a TeamCity project, a Confluence space. Gitea's code has none; its repository is a kind, and only a Gitea issue mirrored as a ticket takes its repository as its project, because the ticket system needs one. Scoping to one repo is what the [parent](#parent) relation is for. Where projects nest, a project takes in its subprojects: `project:Payments` matches what sits anywhere beneath it. Not a [context](#context), which is a working set a person builds (ADR-0010).
_Avoid_: container, space (as a synonym; it stays as Confluence's own word), workspace, board

**Census**:
The whole-corpus report of every [project](#project) a source's live items show (`list_projects`), which the project pickers read. Not a window over the newest items, which would lose a quiet project.

**Mirror**:
The local synced copy of every source's data, with provenance. Readers see its [live items](#live-item); the readers that reach past them are listed under that term. A count of the mirror is a corpus, never a run's [Upserted](#upserted).
_Avoid_: cache, index

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
The entity you can open that an item sits directly inside: a repo for a branch, PR or commit; a build configuration for a build; the parent page for a page; the epic or parent issue for a ticket; the parent asset for an asset and the exposing asset for a route. An adapter declares it per kind as a [declared path](#declared-path). It is never drawn, so it is not a [link](#link); it is something you can open, so it is not a [project](#project). A parent need not be in the [mirror](#mirror). `under:<name>` scopes a search to everything beneath a parent, transitively, matching the declared names exactly and ignoring case, and returns the parent itself when it is mirrored. An asset answers to its own name and `under:` returns it too. A child's searchable text never contains its parent's name; the launcher row's path line shows the chain instead. A title may still name its parent where it means nothing without it (ADR-0018).
_Avoid_: container, scope, holder, repository (as the name of this fact)

**State chip**:
What a row shows of where its item stands, for a kind whose adapter declares one: a pull request's `open`, `merged` or `closed`, a build's `running`, `queued` or outcome. Declared as a [declared path](#declared-path) with a value map; each value is drawn with a [tone](#tone), and a value the map does not name is a [miss](#miss), so no chip. Not a ticket's status, which is a workflow column you can move a ticket to.
_Avoid_: badge, status (a ticket's workflow word), state (alone)

**Tone**:
How loud a value is drawn, in five words the shell owns the colours of: `ok`, `warn`, `err`, `info`, `muted`. An adapter names a tone and never a colour.
_Avoid_: colour, severity, level

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
Reading one item from its source and writing what the source holds into the [mirror](#mirror), outside any run. It moves no [watermark](#watermark) and sweeps nothing. It never overlaps a run of its source: it waits for a run in flight and reads after it. It happens when a [write-back](#write-back) lands, on the item the write made or wrote to, with a run of the source following for what the write changed elsewhere; and when the reader asks on the item's detail (*Refresh*). A source whose adapter cannot read one item, or a kind it does not read one by one, has no refresh: its detail offers *Sync now* instead. For its one item it does what a run does for every item, links on arrival and a monitor's samples and alerts included; an item the source no longer has is tombstoned. A failed refresh changes nothing about the write, which has landed. Not a sync: catching a whole source up is a run (ADR-0031).
_Avoid_: single-item sync, resync, refetch

**Credential health**:
A source's authentication state as knobas last observed it, per source in the shell. It carries what the last check said went wrong, not what an adapter said about the far end when it went right; that is a [connection note](#connection-note).

**Discovered configuration**:
A configuration value an adapter learns from the instance during *Test connection* rather than one the user types. It reaches the Add-source dialog on the connection report and fills its field only when the field is empty; a typed value is never replaced. Jira's Epic Link custom field id is one. Not a secret, and never written by *Test connection* itself. A saved source that leaves such a field empty gets it at its next sync, which re-reads the source as a backfill.
_Avoid_: auto-detected, probed

**Connection note**:
The one line an adapter says about the far end that nothing else on the connection report says, shown wherever a test result is shown and never stored. Not [credential health](#credential-health), which a good sync may rewrite; a note is true of the far end as just found.
_Avoid_: detail (the field's name), diagnostic, warning

## Links and contexts

**Link**:
A knobas-owned, typed, bidirectional connection between two entities, one record shape for every pair. One pair carries one active link per relation whichever way it was drawn, and the stored direction is what tells `blocks` from `blocked by`. Links live locally and are never written to a source.
_Avoid_: relation (a link's type), reference

**Suggestion**:
A machine-proposed link with a stated reason. Accepting makes a link; dismissing persists.
_Avoid_: recommendation

**Backlink**:
A page whose body links to this page.
_Avoid_: pages that link here, inbound link

**Context**:
The working set behind a [job](#job), and nothing else. Membership is explicit adds plus direct links plus one hop out, computed over confirmed links only and never stored; asset membership also counts through ancestors (ADR-0008). Opening a job promotes its ticket to one. The context routes an [alert](#alert) to the inbox, scopes the job's proposals and lists the open jobs; a reader never sees one by name, adds to one or makes one. Its one state is **archived**: a ticket's context is archived exactly while the ticket's status is in its source's declared done-like set, maintained by the sync run, and un-archived when the status leaves it. Ad-hoc contexts are retired; time without a ticket is the ⌘T label timer.
_Avoid_: workspace, project (a source's grouping, ADR-0010), room (retired; a [job](#job) is the working view)

**Job**:
The working view of one ticket, at `#/job/<key>`: the ticket's card, sections over what the ticket reaches (Code, Builds, Assets, Monitors, Pages), a Conversation, a Map drawing Dependencies, and a Connections drawer for the [links](#link) and [link sets](#link-set) that reach it. Opening one promotes the ticket's [context](#context). A job is a ticket's; ad-hoc work has no job.
_Avoid_: task, workspace

**Link set**:
A named group of repositories, pipelines, pages, assets and monitors, each a fixed item or a pattern, with rules saying which tickets it applies to (under an epic, sub-tasks of a ticket, in a project, or one ticket by hand). A [job](#job) shows a set's members beside its context's [members](#member), never as members: read when the job is, never stored as links, never walked through (ADR-0025).
_Avoid_: template, bundle

**Member**:
An entity the membership rule reaches for a given context, computed at read time and never stored. A proposal never makes a member.
_Avoid_: item (the mirror's word)

**Ticket view**:
A saved way of looking at tickets in the Tickets view: a name, a group, which tickets (conditions stored as search text, tickets only) and how they show (a [ticket board](#ticket-board) or a list, columns, lanes, card fields). A **Jira view** is one the Jira source keeps for a board or a favourite filter; Jira decides which tickets it holds, and only how it shows is set in knobas. Not a [smart list](#smart-list), which spans every kind and has no layout.
_Avoid_: saved search, filter, board (alone)

**Smart list**:
A saved local query with a live count and change badge. A **built-in** is hand-written SQL in `knobas_search::lists`' registry; a **saved** one is a row in `knobas.smart_list` holding the raw box text and its name. One `list:<id>` namespace, one rail, one shape on the wire. Only a saved list can be renamed, deleted or come to **need attention**, which is what the rail says when today's grammar refuses the stored query: nothing was counted, the count reads 0 and the row does not open. A saved list stores text and never a parse; no migration ever rewrites a stored query, and there is no editor for one. At most eight saved lists, held in `saved::MAX_SAVED_LISTS`, a measured bound read off the perf gate, lowered by measurement and never raised by hand; `create` refuses the ninth and nothing else enforces it. The [share export](#share-export) carries saved lists verbatim.
_Avoid_: filter, saved search

## Acting on sources

**Write-back**:
An operation that changes data in a source: status, comment, approval, trigger, worklog, page, pausing a [monitor](#monitor). Links, contexts, notes and [blocks](#block) are never write-back; a [worklog](#worklog) is one.

**Account**:
A second credential stored beside a source's ordinary one, in the same keychain item: a username and password, optional, for a source whose write channel its ordinary credential cannot open. Uptime Kuma is the only one: its API key opens `/metrics` and nothing else, so a Kuma with only the key reads every monitor and offers no [write-back](#write-back), and adding the account turns pause and resume on. Not an auth method, which is how a source authenticates its ordinary traffic. Added from the Add-source form or from the credential strip the sources view's **Account** button opens.
_Avoid_: second secret, write credential, admin login

**Section**:
A heading of any level on a wiki [page](#kind) and everything under it until the next heading of any level, so a section never holds a heading. The unit knobas edits a page in, one section of prose at a time. A section holding a macro or a table, or sitting inside a table, refuses its edit and offers the source's own editor. A section is edited as text, so lists and inline formatting come back as plain paragraphs, and the surface says so before the edit. The edit that goes out carries the whole page body, so everything outside the section is sent back as it arrived.
_Avoid_: block (a unit of [time](#block)), chunk, fragment

**Pending write**:
An edit queued because its source cannot currently accept it. The queue is a visible, inspectable list, not a count. Delivery is at least once: a write in flight when knobas stops may arrive twice, and knobas re-sends rather than guess and never merges or deduplicates what you wrote (ADR-0012). A write that went out and got no answer is different: if a second arrival would duplicate it, knobas holds it and asks you to check the source (a [held write](#held-write)).

The word has two senses and both are load-bearing. This glossary's sense is the whole open queue, pending, held and refused together, which is why the command that lists it is `pending_writes`. The state machine's sense is narrow: `pending`, `held` and `refused` are three disjoint states of `knobas.write_queue`, and `write_queue::counts().pending` counts only the first. The write-queue list takes the wide sense; the launcher footer's "N pending writes" takes the narrow one, since those are the writes that leave on their own, and the status bar's badge is where a held or refused write asks for a decision.

**Held write**:
A queued write knobas will not send until the user acts, for one of three stated reasons: its target changed after it was queued (resolved by choosing between the two versions side by side; there is no silent last-write-wins), its source was turned off (resolved by re-enabling it), or knobas sent it and the source never answered, so it may have arrived (resolved by checking the source, then *Send again* or *Discard*). The third is only for a write a second arrival would duplicate (a comment, a create, a worklog, an approval, a review, a build, a transition); a write the source refuses on repeat, or that sets what is already set, waits and goes on its own. A create held this way counts as on its way: its page draws it queued, offers no second create, and knobas refuses a second monitor of its name until you send or discard. *Changed* means changed in what the write is about: a write that overwrites holds on what it would overwrite; a write that adds holds only when the target's declared state moved or it left the mirror. A file commit's target is the file itself, held when the source says the file changed since it was read. The surface always says which reason. In the state machine it has left `pending`; in the wide sense it is still a pending write.

**Echo**:
What a page draws of a write it has queued, muted beside what the source holds: *queued* while the write is open, *sent* once it has landed, gone when a read of the page shows the write itself. A write that is held, refused or withdrawn takes its echo with it. The read it waits for comes when the write's [refresh](#refresh) ends or a run of its source does.
_Avoid_: optimistic copy, placeholder, ghost, queued mark

**Reachable transition**:
A status a ticket's workflow offers from where the ticket stands right now, read from the source when the detail opens and never stored. The status select offers these and only these; when the read fails it falls back to the statuses the source's corpus has been seen to use and stays optimistic. The write is unchanged: it names the status, and the adapter resolves and refuses by name. One read on the `Source` trait, refused by name where a source has no workflow.
_Avoid_: available transition, allowed status, workflow step, transition id (Jira's word)

**Source view**:
What an [adapter](#adapter) draws for one of its own kinds when the detail opens: a live read, never stored in the database, answered as sections of typed **blocks** from one fixed vocabulary (facts, text, entity list, diff, log, settings, actions, a directory's tree, a file). The shell renders the vocabulary and never learns which adapter drew it. A section may be **deferred**, its blocks read when the reader opens it; a block may name a section that opens in its own tab, which keeps its place and a *Back*. A value a view lets a reader change names the [write-back](#write-back) op that changes it, and the change goes through the write queue; a view never writes. An **action** is a write a reader starts from the view, drawn in the view's **toolbar** or in its one section; the source declares its dialog, any fixed values, and whether it can be taken now. One it cannot take yet is drawn disabled with the source's reason; one that can never apply is not drawn. An adapter with no view for a kind answers nothing and the detail draws the generic payload projection; a failed read draws the same under a line saying why (ADR-0020). A view the adapter says stands at a fixed commit is kept by the [read cache](#read-cache).
_Avoid_: page (a wiki kind), template, layout, widget, panel (the detail is the panel; a view fills it)

**Read cache**:
The app process's memory of [source views](#source-view) and of files' bytes (a README's image), keyed by what each stands at: a read at a whole commit sha is kept for the session and the same ask is answered from memory with no request to the source; anything else is read live. The adapter says which before the read. Bounded by bytes, least recently used out first; never written to the database or to disk, and dropped with the built source it was read through when that source's address, configuration or credential changes (ADR-0032).
_Avoid_: mirror (the database's copy, written by sync), store, snapshot

**Diff**:
The line-by-line difference between two versions of a set of files, as a source view's diff block carries it: per file its path and old path, what happened to it, its line counts and its hunks. Parsed from the unified diff text a git server answers, by one parser every git adapter shares. Capped per file and per view; a file over the cap keeps its counts and says it was cut. A pull request's diff also carries its **line threads** and may take new line comments.
_Avoid_: patch (the text before it is parsed), changeset, delta

**File pipeline**:
The ordered steps between a file's bytes and its renderer (base64 decode, gunzip, jq, pretty-print, …), each taking bytes or text and giving bytes or text. Run in the backend under a size cap and a timeout per step, and kept in the [read cache](#read-cache) by (sha, path, steps). In the viewer it is *Pipeline*, on the open file; in code `file_pipeline`, `PipeStep`, `PipeRun`. A **named pipeline** is one saved with a name, which a default per file pattern names by id (so editing it changes every pattern that uses it); a **command step** is one of the person's own programs as a step, under ADR-0033.
_Avoid_: pipeline alone in code or docs (a CI [pipeline](#pipeline)), filter (jq's word for one step), transform

**Pipeline**:
What a CI source runs: TeamCity's build configuration, a GitLab pipeline, a Jenkins job. The Builds view is drawn over pipelines and their [runs](#run); the source's own word appears once, in the view's header (ADR-0022).
_Avoid_: build configuration (TeamCity's word), job (a [job](#job))

**Run**:
One execution of a [pipeline](#pipeline): TeamCity's build, a GitLab pipeline run, a Jenkins build. It has a number, a state, a branch and commit, steps, a log, tests and artifacts (ADR-0022).
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
The opt-in recording of which entity was in the foreground (the open detail, else the asset in the [Tree](#tree)'s pane, else the open [job](#job)'s ticket, else nothing) while the window is focused, as passive [blocks](#block) derived from [observations](#observation). A gap stays a gap until a person assigns it; nothing recorded this way reaches a source on its own. Browsing the Tree with nothing selected is a gap. Observations are kept a month.
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
What breaks if an [asset](#asset) goes down: its descendants, and every asset linked to it by `depends-on` or `runs-on`, transitively over both, read from the link's `to` end. The panel in the asset pane that answers *blast radius*. Routes whose target is the asset or anything in its answer are listed beneath as *reachable via routes that would break* and not counted.
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
A [monitor](#monitor)'s transition to down or warn, open until the monitor returns to up; at most one open per monitor. A [sample](#sample) reading `pending` or `maintenance` neither opens nor closes one. Every open alert shows in Monitors, in the Tree and in the launcher's *Open alerts on my jobs* list; it reaches the [inbox](#inbox) only when some unarchived [context](#context) holds the affected asset, directly or through an ancestor. **Ack** is knobas-local and clears the inbox item while the alert stays open; the inbox is the only surface an ack clears.
_Avoid_: incident, notification (bare), problem (the rolled-up count on a closed branch)

## Export

**Backup export**:
Everything knobas owns, notes included; the mirror is excluded, since it re-syncs.

**Share export**:
A curated export in the backup's own format restricted to tables, named `knobas-share-<stamp>.knobas` so retention never deletes it, read by the ordinary restore. Parts: links, assets, contexts, [source](#source) configurations, smart lists, ticket views and link sets on by default; notes and time (the timer, [blocks](#block) and [worklogs](#worklog)) off. A part whose table is empty is sent off rather than hidden. Because `pg_dump` restricts by table and never by row, the time settings cannot cross and the whole address book of entity rows does. No credential is ever in an archive, so a restored source lands as *missing secret* and at no position, since no archive carries the [mirror](#mirror). Saved queries cross exactly as written.

## Surfaces

**Launcher**:
The ⌘K box; one search over everything, under 100 ms. Its text search, and every full-text read of the [mirror](#mirror), runs under PostgreSQL's `english` configuration with no `unaccent`, on purpose and with named gaps (ADR-0030).
_Avoid_: command palette, spotlight

**Coverage**:
Per query, which sources could answer a filter and which have nothing it can match, measured on the corpus each source contributed and never on what matched. A source that answered and found nobody says something different from one whose items never name a person.
_Avoid_: capability (a static claim on the descriptor; this is a measured one about a corpus), support

**Room**:
Retired; the app no longer draws one. The hub view a switcher entry opened. The [job](#job) is the working view of one ticket, a [ticket view](#ticket-view) lists tickets, a [link set](#link-set) gathers what belongs together, and Dependencies draws the links. Do not use the word for any of these.
_Avoid_: dashboard

**Tile**:
Retired; the app no longer draws one. Its successors are the sections of a [job](#job). What a tile drew survives as read orders: the **attention** order (settled [tones](#tone) not drawn, the rest loudest first, newest first) and **latest per parent** (only the newest row of each declared [parent](#parent)), both over the [mirror](#mirror) in SQL.
_Avoid_: card, widget, panel (as the name)

**Ticket board**:
A ticket view's tickets in columns, each column holding the statuses the view puts on it, optionally split into lanes. Inside the Tickets view its layout toggle says just *Board*, because the view qualifies it (ADR-0024).
_Avoid_: kanban, board (alone, outside the Tickets view)

**Mini board**:
Retired; the app no longer draws one. The Tickets tile's status-grouped rendering inside a [room](#room), replaced by the [ticket board](#ticket-board). "Board" never stands alone: launcher board, assets board, ticket board, Jira board (ADR-0009).
_Avoid_: kanban, board (unqualified)

**Maximised tile**:
Retired; the app no longer draws one. A gesture that made one [tile](#tile) fill a [room](#room)'s grid.

**Tree**:
The Assets view, at `#/assets` and `#/asset/<id>`, reached from Dependencies' *Asset tree* button: the [estate](#estate) as Miller columns, one column per level. Its fixed pane offers *Create monitor for this asset* where a configured [source](#source) offers the op, and shows no control at all where none does. It has no sibling tab.
_Avoid_: board, assets board, columns view

**Spine**:
A column of the [Tree](#tree) collapsed to a 30px strip, labelled with the [asset](#asset) of the reader's own path that runs through it, re-expanded by clicking it. What collapses is decided by the width left beside the fixed pane, never by depth.
_Avoid_: breadcrumb (a line of names), rail, collapsed column (as a name)

**Wire**:
A line the [Tree](#tree)'s pane draws from a [route](#route)'s row to the asset at the route's far end: the target for a route the asset exposes, the exposer for one it is reachable via. It lands on that asset's row or on the [spine](#spine) hiding its column, dashed when the far end is reached through something rather than pointed at. A far end no open column lists draws nothing.
_Avoid_: edge, arrow, connector, link (knobas' relation between entities)

**Monitors**:
A section of its own at `#/monitors`, with two tabs. *Monitors* lists every mirrored [monitor](#monitor) as one row (state, type, what it watches, a 24-hour bar from the [samples](#sample), last check, uptime ratios, certificate days, attached assets, a link to Uptime Kuma), one chip per state above it, and one card per open [alert](#alert). The bar's segment is half an hour, reading as the worst state sampled in it; a half hour with no sample is a **gap**, meaning knobas was not watching. A row carries *Pause* or *Resume* only when its source has an [account](#account); the button says *queued*, the write goes through the queue, and the [refresh](#refresh) after it lands catches the row up. *Assets nothing watches* (`#/monitors/unmonitored`) is a roster of every [asset](#asset) with no confirmed `monitored-by` link, a statement about attachment and not attention; the [launcher](#launcher)'s *Not monitored* smart list is the same rule with a second reader, held to the same answer by `the_launchers_not_monitored_list_is_the_monitors_tabs_roster`.
_Avoid_: uptime page, status page (Uptime Kuma's own published page), dashboard, health tab

**Type convention**:
The child [asset](#asset) types the built-in table records as usually held inside one (`AssetType::suggests`), drawn by the create dialog as *usual here* and by nothing else. An ordering, never a filter: the dialog offers every type and `assets::create` accepts any type under any parent. Not a *suggestion*, which is a machine-proposed [link](#link). The conventions answer to the real estate in `testenv/estate.json`.
_Avoid_: suggestion (a link), rule, constraint, allowed types

**Tidewater**:
The fictional company whose dataset seeds demos, fixtures and the test environment.
