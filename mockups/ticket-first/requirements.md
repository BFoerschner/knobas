# Ticket-first UI: requirements

Written 2026-10-14 against `main` at `0681cc12`. This document says what the app has to do so the real UI matches the ticket-first prototypes. Read it next to the prototypes:

- `prototype-1.html` has the flow: pick a ticket, work on its job, open full views.
- `prototype-2.html` adds scale: filters on every page, Docs as a view, long comment threads.
- `prototype-3.html` adds link sets and a full Code view.
- `prototype-4.html` reworks Builds: pipelines on the left, their runs on the right, and a run's log, tests and artifacts.
- `prototype-5.html` makes Docs one reader with the space picker and full-text search at the top left, separates assets from monitors, and drops notes. It carries the BUILT/LOW/NEW tags in the UI.
- `prototype-6.html` (2026-10-15) is prototype 5 drawn as if every function existed: no tags in the UI, every ruling of section 0 applied (the job's Map tab is Dependencies only, the global map is gone), and Time (section 13) and Settings (section 13a) drawn in full. **It is the reference.** Where it differs from the earlier ones, it wins.
- `docs-reader.html` (2026-10-25, #2078) redraws the Docs reader alone: a full-width page, an outline-only margin, the page's context at its foot, and a tree drawer on narrow windows. For the reader (R-10.9, R-10.10, R-10.15) it wins over prototype-6.

The capability sheet `mockups/shared/capabilities.md` lists what the backend already does. Every requirement below carries a tag:

- **BUILT**: the backend and some UI exist today. The work is the new UI.
- **BACKEND**: the IPC command exists, and no UI calls it yet.
- **LOW**: small plumbing, at most one migration or one IPC command or one `WriteOp` variant, each with its §10.8 entry.
- **NEW**: real backend work. The size is given where it is known.
- **LATER**: drawn in the prototype as future. Do not build it in this round.

IDs (`R-3.4`) are stable. Refer to them in tickets and PRs.

**Scope rules for the build (Björn, 2026-10-15):**

- **Match prototype-6; leave performance for later.** No ticket in this round optimises a read, adds a summary or paged read, or measures a new budget. Every read is written the straightforward way. This defers, among others, the paged summary read of section 11d at estate scale, any tuning of the *This job* scope (R-2.9), and perf-gate work for ticket-view counts (D6). They come back as their own tickets after the round.
- **Two guardrails stay.** The existing perf gate in `just check` stays green and unchanged; a change that trips it is fixed, not waved through. And each screen has to be usable on testenv's real data, judged by eye during the desktop check. A screen too slow to use there gets one follow-up ticket, not a redesign inside the ticket that found it.
- **Acceptance is the match.** Each UI ticket names the prototype-6 route or routes it builds, for example `#/time/week`. Its PR shows a screenshot of the real app at that route next to prototype-6 at the same route, in the same theme and at 1440 × 900. A difference is either fixed or named in the PR with its reason, such as data the real sources do not have. Copy follows prototype-6 word for word unless the PR says why not.

---

## 0. Decisions needed before building

These came up while drawing the prototypes. Each one blocks a group of requirements.

| # | Question | Blocks | Recommendation |
|---|---|---|---|
| D1 | **Ruled 2026-10-15: amend ADR-0009 so *Board* is allowed for the ticket layout.** The UI says **Board** for a ticket layout. ADR-0009 forbids the word unqualified. Björn waived section 0 of the capability sheet for the mockups. Does that waiver carry into the app? | R-3.* | Rename the layout toggle *Columns / List* in the app, or amend ADR-0009. Pick one; the screens work either way. |
| D2 | **Ruled 2026-10-15: add *Job* to `CONTEXT.md`.** **Job** is a new word. It is a ticket's working view: a context promoted from the ticket (`promote_context`), shown with the job card. Add it to `CONTEXT.md`, or reuse *room*? | All | Add *Job* to the glossary as "the room of a promoted ticket context". The address stays `#/ctx/<id>`, and `#/job/<key>` resolves to it. |
| D3 | **Ruled 2026-10-15: a new ADR, as recommended.** **Link sets** (section 7) add a read term to the job beside ADR-0008's fixed membership rule. | R-7.* | A new ADR. It should state that sets are a separate term, computed at read time and never stored as links, and that the membership walk still never passes through a set. |
| D4 | **Ruled 2026-10-15: Gitea's contents API through the write queue (`CommitFile`).** **Small code edits** (R-8.6) need one new `WriteOp` that commits a file through Gitea. Editing the local checkout instead would break ADR-0016 and ADR-0021. | R-8.6 | Gitea's contents API, through the write queue. Never write to the checkout. |
| D5 | **Ruled 2026-10-15: amend ADR-0021 for `git diff`.** **Local uncommitted diff** (R-8.9) needs `git diff` beside `git status`. ADR-0021 allows only `status` today. It already names `log` and `rev-list` as allowed later on the same terms. | R-8.9 | Amend ADR-0021 to allow `git diff --no-ext-diff --no-color` with the same flags, timeout and argument rule. |
| D6 | **Ruled 2026-10-15: ticket views are their own term, not smart lists, and have no cap.** Smart lists span every kind (pages, assets, pull requests), so a ticket-only layout does not belong on them. They stay as they are on the launcher board, cap and all. | R-3.2, R-3.3 | Done. A **ticket view** is its own knobas-local table (R-3.3). |
| D7 | **Ruled 2026-10-15: the shell draws the frame; one ADR with D9.** The Builds view is drawn over a generic **pipeline** and **run**, so a GitLab CI or Jenkins adapter can fill it later. ADR-0020 says a source draws the views of its own kinds. Does the shell own this view, or does each CI adapter draw it? | R-9.* | The shell owns the frame: list, history chart, runs table, steps bar, log viewer, tests, artifacts, run dialog. The adapter declares the fields (R-9.12). Source-only details stay in blocks inside the frame. Record it as an ADR next to ADR-0020. *Pipeline* and *run* join `CONTEXT.md`; the source's word appears once, in the header. |
| D8 | **Ruled 2026-10-15: downloads go through knobas.** **Artifact and log downloads** today are TeamCity URLs opened in the browser, which needs a TeamCity browser session. The prototype downloads through knobas with the stored credential. | R-9.9, R-9.10 | One IPC command that streams a source file to `~/Downloads`. It reads from the source and writes nothing to it, so no `WriteOp` is needed. |
| D9 | **Ruled 2026-10-15: the shell draws the frame; one ADR with D7.** Confluence may give way to a **Markdown-based** wiki. How does the Docs view stay valid? | R-10.* | The shell owns the Docs view over a generic docs source (R-10.18): spaces, a tree, pages as blocks, comments, versions. A folder of `.md` files in a Gitea repo is the cheapest second source, because edits reuse `CommitFile` (D4). Record it with D7 as one ADR: "the shell draws Builds and Docs over declared kinds". |
| D10 | **Ruled 2026-10-15: hide for one release, export first.** The redesign drops **knobas notes** and **quick capture**, which are BUILT. Hide them, keep them reachable from ⌘K only, or remove them along with their IPC commands? Existing notes need an export or a move first. | R-17 | Keep the data and the commands. Hide the UI and the capture shortcut behind a setting for one release. Offer *Export notes as Markdown* (the converter of R-10.17) before anything is deleted. On, notes are back in search and open on their own page, and the capture shortcut works again (#1504). |
| D11 | **Ruled 2026-10-15: only the job's Map tab, and it is Dependencies scoped to the ticket. The global map is retired (D13).** Both now exist: the job's **Map tab** (R-4.11) and the **global map** (R-4.27). Keep both, or fold one into the other? Originally: should it be a view of its own across jobs ("everything on vm-pay-01"), or both? | R-4.11 to R-4.18 | Try the job's Map first; its *What breaks if this goes down* already answers the cross-job question from one asset. A global map can reuse the same model later. |
| D12 | **Ruled 2026-10-15: Dependencies only. Maps 1 to 3 and 5 to 7 are dropped; section 4b and 4c stay as a record.** The job's Map now has seven drawings of the same graph (section 4c). Which ones stay? | R-4.29 to R-4.36 | Keep Map 1 for the full picture. Pick at most two of the others as tabs, and turn one or two more into a panel or a section of the Work tab. Trails (Map 2) answers the most common question, so it is the strongest default. |
| D13 | **Ruled 2026-10-15: yes.** Dependencies (section 11d) is the asset page and takes the top-bar places of *Assets* and *Map*. The Tree (Miller columns, BUILT) stays reachable as *Asset tree*; the global map (R-4.27) is retired. | R-11.10 to R-11.24 | Done in the prototype. |
| D14 | **Ruled 2026-10-15: layers are one shared set, and each name is unique.** A layer's name always means the same contents, in every view. Moving a kind to another layer changes it everywhere. | R-11.13, R-11.16 | Done in the prototype. Order is shared as well, so a name never sits in two places. A view keeps only which layers show, how each one folds and groups, the filters and the tile size. |
| D15 | **Ruled 2026-10-15: yes.** Use the built-in type labels everywhere (*Hypervisor*, not the mockup's *Host*). | R-11.12 | Done in Dependencies; the older drawings still read the mockup data. |
| D17 | **Ruled 2026-10-15: boards on the synced projects plus favourite filters.** Jira boards and filters come in **with the Jira source**, with no import step (R-3.20). Still open: which ones by default? | R-3.20 | Every board whose filter reaches a project the source syncs, plus the person's favourite filters. *Every board I can see* is a setting, because on a large Jira it is hundreds of boards about projects knobas does not sync. The groups are named *Jira boards* and *Jira filters*, which uses the source's word with a qualifier, so ADR-0009 allows it. |
| D16 | **Ruled 2026-10-15: knobas clones.** *Clone* clones the repository into the clones root set in Settings › Checkouts. This reverses part of ADR-0016 ("no clone, no checkout, no fetch"), which needs an amendment, as ADR-0021 amended it for `git status`. | R-8.25 | Amend ADR-0016 for exactly one writing command, `git clone` into a new folder under the clones root, on the terms in R-8.25. No checkout, fetch or pull on an existing clone. The argument rule stays for everything else. |

---

## 1. Shell

- **R-1.1** The top bar holds, from left: the app name; the main navigation (Tickets, Job, Inbox, Code, Builds, Docs, Dependencies, Monitors, Link sets, Proposed links, Time); *Search* (`⌘K`); the timer; Settings (a cog, `⌘,`); the theme toggle. BUILT parts: launcher, timer, inbox count. NEW in UI: the nav itself.
- **R-1.2** *Job* in the nav shows the key of the job you were last in, or the timer's target if you were in none. It opens that job.
- **R-1.3** Count badges: Inbox (live items), Builds (configurations whose latest build failed, err tone), Monitors (monitors down, err tone), Proposed links (undecided proposals). BUILT data (`inbox_items`, `monitor_roster`, `room_suggestions`).
- **R-1.4** The timer chip shows the target key and the elapsed time, ticking every second. A click stops it and opens the worklog draft (R-13.1). With no timer running, a click starts one on the current job. `⌘T` does the same. BUILT (`current_timer`, `start_timer`, `stop_timer`).
- **R-1.5** The status bar lists per-source sync age, and states a credential failure in err tone with a *Re-enter password* action. It shows the write-queue count, which opens the queue list. BUILT (`StatusBar`, `WriteQueue`, `credential_health`).
- **R-1.6** Every full view opened from a job shows a breadcrumb that starts with the job key on the job-card yellow. `Esc` returns to the job. With no job, the breadcrumb starts at the view's own name.
- **R-1.7** Light and dark themes come from the same tokens (section 16). The toggle overrides the OS setting for the session.
- **R-1.8** Toasts name what happened, in the same words as the button ("Queued: re-run #1187"), and may carry one action (*Undo*, *Show queue*, *Open it*). BUILT.

## 2. The filter bar (one component, every page)

Every list view has the same filter bar. That way nobody has to open the global search to narrow a list.

- **R-2.1** Parts, left to right:
  - a text field (placeholder names what it filters, e.g. "Filter by configuration, branch or build number");
  - a scope toggle *This job / Mine / All*, where the view has a meaning for each;
  - chips for fields with few values (status, type, state), multi-select;
  - dropdowns for fields with many values (repo, project, epic, assignee, space);
  - the result count on the right, "8 of 75 tickets", or "75 tickets" when nothing filters.
- **R-2.2** Typing filters on every keystroke. Only the results re-render. Focus and the caret stay in the field.
- **R-2.3** Matches are highlighted in the results (`<mark>`). Every word typed has to appear somewhere in the row, in any order, ignoring case.
- **R-2.4** `/` focuses the page's filter. In a filter field, the first `Esc` clears it and the second leaves it. `Esc` in any other text field (editor, comment) does not clear it.
- **R-2.5** *This job* appears only when there is a current job. It is **off by default** (Björn, 2026-10-15: "if I want it to"). A job section's *Open … view* link opens the view with *This job* on. After that the choice is kept per view (R-2.6). When *This job* finds nothing, the empty state offers a one-click *Search all* that switches to *All*.
- **R-2.8** **One meaning of *This job* everywhere: what the ticket reaches.** It is the same walk Dependencies draws from a ticket (R-4.18, R-4.17's rule that from a VM, proxy or host the walk only goes further down). That covers the ticket's own links, its link sets, its parent, and what those lead to. Each row it keeps carries a small reason: "linked to PAY-231", "from the link set Payout stack", "via PR #142", "watches vm-pay-01, via payout-service". Per view:
  - **Code, Repositories:** repositories the ticket reaches through its PRs, branches, direct *works in* links and link sets. New: *This job* was not offered here before.
  - **Code, Pull requests:** the PRs linked to the ticket, plus any PR the walk reaches. Before, only direct links counted.
  - **Monitors:** every monitor watching an asset the ticket reaches, the VM under a container included. Before, only the job's own list counted.
  - **Docs:** a *This job / All pages* switch above the space picker. With *This job* on, the left column lists the ticket's pages grouped by reason ("Linked to PAY-231", "From the link set Payout stack"). Search stays inside them ("3 pages in PAY-231's pages", with *Search all pages*), and the space picker hides.
  - **Dependencies:** the same *This job / Everything* switch, in place of the old sentence. From the top bar it goes to `#/deps/job/<key>`; from the job's Map tab, *Everything* goes to `#/deps`.
  - **Builds** keeps *My job's branches* (R-9.*), which is about runs, not about reach.
- **R-2.9** **Backend for R-2.8.** The reach is the R-4.18 read. Each view asks it once for the ids and filters its own list, so there is no new query per view.
  - LOW: expose the reach as a set of entity ids with the first step of each path, which is what the reason label needs.
  - For the server-side filters of R-2.7 (pages, PRs), the search grammar needs one filter, `reach:<ticket>`, beside `under:`. That is NEW but small: the same walk, as a filter.
- **R-2.6** Filter state is kept per view for the session.
- **R-2.7** The filters run over data the view already holds. Lists that can exceed a few hundred rows (tickets, pages, commits) need a server-side filter. Use the search grammar (`search`, `filter_suggestions`) with the view's base query. BUILT grammar. LOW for each view's statement.

## 3. Tickets

- **R-3.1** Three panes: views on the left, the ticket area in the middle, the selected ticket's preview on the right.
- **R-3.2** Views are listed on the left in groups. Your own groups come first (*Mine*, *Team*, *Epics*, *Projects*, and any group you type), then *Jira boards* and *Jira filters* (section 3b). Each view shows its count. *Find a view* filters the list by name and by query. Your own views are **ticket views** (D6), a term of their own: always tickets only, unlike smart lists, which span every kind. The search grammar and its filters are BUILT; the views themselves are NEW, small (R-3.3).
- **R-3.3** Each view keeps its layout (*Board* or *List*, see D1), its columns, lanes, card fields and list columns (R-3.15, R-3.16). The *Board / List* and *Lanes* switches above the tickets override them for the session. NEW, small: one migration for a knobas-local `ticket_view` table (name, group, position, the conditions as search text, layout, columns, lanes, card fields, list columns, and for a Jira view its board or filter id), list, save and delete IPC commands, each with a §10.8 entry, and the views in the share export beside smart lists. The text is the source of truth: the editor parses it into conditions when it opens and writes new text on save, and a text the grammar refuses opens as text with a line saying which part it could not read.
- **R-3.4** Board layout: the view's columns (R-3.15). A new view starts with one column per status, in the source's own words (`mini_board`). A column shows up to 8 cards, then *Show N more*. *Done* shows 3. While a filter has text, the server still filters the whole mirror and counts every match per column, but a column sends and draws at most 50 cards; its header shows the column's total, and *Show N more* reads the rest, as an unfiltered column does (Björn, 2026-10-17, #1024). BUILT data.
- **R-3.5** Lanes: *None*, *Epic*, *Assignee*, *Priority*, *Issue type*, *Label* (a ticket's first label). Each lane has its own columns and a count. Lanes sort by size. UI only: lanes are not stored anywhere but the view. The server groups them (#939, #1098): a lane's column shows and reads like a board's column (8, *Done* 3, 50 while the filter has words, then *Show N more*), and the board reads its ten biggest lanes, then *Show N more lanes* reads ten more.
- **R-3.6** List layout: Key, Summary (epic under it), Status, Priority, Signals, Assignee, Updated. Every header sorts on click and shows the direction. Key and Status never wrap.
- **R-3.7** A ticket card shows key, type, priority, summary, signals, the comment count when there are comments, and the assignee initials. The selected card turns job-card yellow.
- **R-3.8** **Signals** are short tone-coloured facts: *build failed*, *PR #142 open*, *blocked by OPS-77*, *review asked of you*, *dashboard 8.4 s*. LOW: a reader over links plus declared state. The spec catalogue lists already need most of it (`My tickets with a failing build`, `Blocked tickets`).
- **R-3.9** The preview shows key, title, status, assignee, priority, epic, updated, and the description. Then *Around this ticket*, one line per section: "Builds: 7 configurations, 1 failing or down". Then one primary button whose label follows the ticket's state: *Start work* (To Do, mine), *Resume work* (in progress, mine), *Open job* (anything else). `Enter` presses it.
- **R-3.10** Keyboard: `j`/`k` or arrows move the selection through the visible cards in reading order. `Enter` opens the job. A double click opens it too.
- ~~**R-3.11** Ad-hoc work without a ticket (an ad-hoc context like *Staging DB configuration*) is listed under *Not a ticket* at the bottom of the views.~~ retired 2026-10-21; ad-hoc time is the ⌘T label timer.

### 3a. Making and changing views (2026-10-15)

Before this, the views were fixed and *Save this filter as a view* did nothing.

- **R-3.12** Where views are made and changed: *+ New view* at the top of the list; *Edit view* in the view's header; *Save as a view* at the end of the filter bar, which starts a new view from the current view's conditions plus the filter's chips, dropdowns and text; a ⋯ menu on each row and in the header with *Edit view*, *Duplicate* and *Delete* (with *Undo* in the toast). UI.
- **R-3.13** The editor is one dialog: the form on the left, a live preview on the right. The preview shows how many tickets the view holds now, in large figures, and the board or list as it will look, redrawn on every change. It calls out a column over its limit and tickets whose status is on no column. *Create view* stays off until the view has a name. The footer says "Kept in knobas. Jira never sees it." UI.
- **R-3.14** *Which tickets* is a list of conditions. Each one is a field, *is* or *is not*, and one or more values, which mean any of them. Every condition has to hold. Below them is a field for words in the key or summary. Fields: Project, Status, Issue type, Priority, Assignee (*Me* first), Epic, Label, Sprint (the active one), Signal, Updated, Assigned to me. Under the conditions, the editor shows the same view as a search query; tokens the grammar does not parse yet have a dotted underline.
  - BUILT grammar: `is:mine`, `under:`, `updated:`.
  - NEW, small: `project:`, `status:`, `issuetype:`, `priority:`, `assignee:`, `label:`, `sprint:current`, `has:` (`failing-build`, `blocker`, `open-pr`, `review-for-me`), `assigned:`. `type:` already means the entity kind, so Jira's issue type needs its own key. All of them read fields the Jira sync already stores in the raw payload (`BASE_FIELDS` in `knobas-source-jira/src/sync.rs` asks for status, issuetype, priority, assignee, project, labels and resolution). `has:` needs the signals of R-3.8.
  - LOW: the sprint is a custom field. The adapter discovers its id the way `discover.rs` finds Epic Link, and adds it to the field list.
- **R-3.15** Board columns: a name, the statuses on it, and an optional limit (*At most*). Each status sits on at most one column, and a column can hold several. Columns are ordered with ↑ and ↓, added, removed, or reset with *One column per status*. A column over its limit shows its count as "5 of 4" in err tone. Tickets in a status on no column do not show, as on a Jira board, and the editor says so. UI plus R-3.3's column.
- **R-3.16** *Cards show*: issue type and priority, signals, comment count, assignee, epic, labels. *List columns*: Key (always), Summary, Status, Priority, Signals, Assignee, Updated, Epic, Labels. UI plus R-3.3's column.

### 3b. Views from Jira boards and filters (2026-10-15)

Björn: the boards come in with the Jira source, with no import step.

- **R-3.20** At every sync the Jira source reads the boards and filters it is set up for, and keeps one view for each, under *Jira boards* and *Jira filters*. A new board shows *new* until it is opened. One deleted in Jira disappears at the next sync. Sources › Jira › *Boards and filters* chooses which: boards on the projects this source syncs (the default, D17), only boards built on my favourite filters (Jira DC has no favourite-board read, so the choice follows the board's saved filter, #1096), every board I can see, or none; and the person's favourite filters, on or off. The view list ends with a line that says where they come from, with a *Choose which* link to that setting.
- **R-3.21** Jira decides which tickets a Jira view holds. At each sync the adapter runs the board's query and asks for keys only. The query is the board's saved filter, plus a Kanban board's sub-filter, plus `sprint in openSprints()` for a Scrum board. knobas stores the keys in Jira's order. Nothing is translated, so ScriptRunner functions, components, fix versions and anything else knobas does not sync keep working, because Jira answers them.
- **R-3.22** A Jira view's header says where it comes from: board or filter, its type, the active sprint and its end date, the JQL, when it was last read, and *Open in Jira*. A Scrum view follows the active sprint and moves to the next one when the sprint closes.
- **R-3.23** Tickets on a board that knobas does not sync, such as a project outside the source (WEB), are the keys Jira returned minus the keys knobas has. A warn line names the count and the project and offers *Sync WEB too*, which opens Sources › Jira with the project added.
- **R-3.24** Columns come from the board's configuration (names, the statuses on each, the max limits) and follow it at every sync. Lanes, card fields and list columns are set in knobas under *View settings*, because Jira's public REST API does not expose how a board's swimlanes are set up. Quick filters become chips in the view's filter bar, labelled *Jira quick filters*. Like in Jira, several quick filters must all hold.
- **R-3.25** Which tickets a Jira view holds, and its columns, can only be changed in Jira. *Make an editable copy* (also offered when *Save as a view* runs on a Jira view) turns it into a view of your own. The JQL parts knobas can test become conditions, and the editor lists the parts it left out and why, for example "`component = on-call` (knobas does not sync components)". *Hide from the list* keeps the view syncing. Hidden views are one line at the bottom of the list, each with *Show*.
- **R-3.26** Backend for R-3.20 to R-3.25. NEW, medium, all in `knobas-source-jira`, read-only, so no `WriteOp`:
  - Reads: `GET /rest/agile/1.0/board` (paged, `projectKeyOrId` once per synced project), `/rest/agile/1.0/board/{id}/configuration` (columns with status ids, column limits, the filter id, a Kanban sub-query), `/rest/agile/1.0/board/{id}/sprint?state=active`, `/rest/api/2/filter/{id}`, `/rest/api/2/filter/favourite`, and `/rest/api/2/status` for status names. Quick filters come from `/rest/agile/1.0/board/{id}/quickfilter` if the Data Center version has that endpoint. Check it against testenv Jira, and leave quick filters out if it is missing.
  - One keys-only search per view per sync (`fields=key`, paged like the main search). That is one request per 1 000 tickets per view. With the default set of 5 to 10 views it adds little to a sync.
  - One migration: a `jira_view` table (source, board or filter id, kind, name, JQL, columns, sprint, last read) and `jira_view_member` (view, entity, rank). The display settings are columns of R-3.3's `ticket_view` row. §10.8 entries for the migration and the IPC: the list of Jira views, their display settings, and hiding a view. One `update_view` command could cover both kinds.
  - A live suite against testenv Jira. It needs a seeded Scrum board with an active sprint, a Kanban board with a sub-filter and a column limit, a favourite filter with a clause knobas cannot translate, and a board that reaches a project the source does not sync.

## 4. Job

The job is where work happens. It is the room of the context promoted from the ticket (D2). Opening a job promotes the ticket if it has no context yet. BUILT (`promote_context`, `room`).

- **R-4.1** Layout: the job card is a fixed 320 px column on the left, in job-card yellow. The work area is on the right with tabs *Work* and *Conversation*.
- **R-4.2** The job card shows, top to bottom:
  - *All tickets* (`Esc`);
  - the key in large condensed type;
  - the summary;
  - a status select with only the transitions reachable now (BUILT);
  - type, priority, estimate, and a blocked chip;
  - the clock, with *Stop and log* while the timer runs on this ticket and *Start timer* otherwise;
  - *What it asks for* (the description);
  - *Conversation*, a summary (R-5.1);
  - *Time this week*, a five-day bar strip with logged and unlogged time (BUILT `week_timesheet`).
- **R-4.3** The *Connections* button sits at the right of the tab bar: "26 connected, 3 proposed", shortcut `L`. It opens the drawer (R-6.1).
- **R-4.4** **Also on this job**: one line of chips naming every link set that applies, with the reason ("Payout stack, via epic PAY-200"), and *Apply a link set…*. Hidden while the Conversation tab is open. Depends on section 7.
- **R-4.5** Removed 2026-10-14. There is no worklist at the top of the job; what needs you shows in each section's rollup and in the Inbox.
- **R-4.6** Five sections: **Code**, **Builds**, **Assets**, **Monitors**, **Pages**. Assets and Monitors are separate: an asset is a thing in the estate, and a monitor is a check that watches one (section 11). Each section:
  - has a title and an *Open … view* link to the full view, already set to *This job*;
  - starts with a one-line rollup, e.g. "7 configurations: 1 failing, 1 running, 5 passing" or "2 repos: payout-service, psp-client, 1 open PR";
  - lists rows sorted by what asks for you, then newest first, and shows at most 4;
  - past 4 rows, has a footer with an in-section filter ("Filter 8 builds"), *Show all N*, and *All in … view*;
  - tags a row that comes from a link set or a parent with a small label naming it ("Payout stack", "Parent PAY-231").
- **R-4.7** Code shows the local checkout line (path, branch, changed count, *Open in…* (R-8.22); BUILT `entity_checkout`, `open_checkout`), then PRs, branches, commits and repos. Builds shows a staleness warning when the source's sync failed, with the fix action ("TeamCity data is 38 min old: sign-in failing. Re-enter password").
- **R-4.8** A blocked ticket shows a red banner above the sections, naming the blocker with a button to open it.
- **R-4.9** A ticket in To Do and assigned to you shows **Start work** in place of *Next*. It is a stepper:
  1. create branch, with a repo dropdown listing every repo and the suggested one first;
  2. open the PR;
  3. link the PR to the ticket;
  4. move the ticket;
  5. start the timer.

  Each step has an editable value, a state (*ready*, *queued*, *done*), and can be skipped or retried. If another timer runs, step 5 says so, and the worklog draft for the old timer opens first. BUILT (`start_work_flow`, `run`, `retry`, `skip`, `amend`).
- **R-4.10** Rows link to the item's full view, never to a half-screen panel.

### 4b. Map (`#/job/<key>/map`), an example to react to

Drawn on 2026-10-15 because the question "how do I see the systems this depends on?" had no answer yet. Treat it as a proposal, not a decision (D11).

- **R-4.11** A third tab on the job, *Map*. It gives the whole width to the map; the job card hides and the toolbar names the job. Since 2026-10-15 the tab opens Dependencies for the ticket (section 11d); this map is *1 Columns* under *Earlier drawings*.
- **R-4.12** **Layout**: columns from the ticket to the hardware.
  - The columns are: Ticket, Changes and pages, Repositories, Builds, Entry points (routes), Runs as (containers, services, databases), Runs on (VMs, the proxy), Hosts.
  - Nodes are small cards with **two lines only**: the kind (with one short state on the right: *failed*, *running*, *check down*, *warning*, *2 checks*, *no check*) and the name. The tone colours the left edge. Everything else is in the tooltip and in the panel.
  - Monitors are never nodes of their own. They are the check state on the asset they watch, and *no check* when nothing watches it.
  - Each column is ordered by where its neighbours sit, to keep lines short. The map zooms to fit (`−`, *Fit*, `+`).
- **R-4.13** **Edges are relations knobas already has**:
  - links from the ticket (implements, documents);
  - a PR's repo, and a build's repo;
  - build chains (R-9.13), and deploys from a deploy pipeline to what it deploys;
  - a route's target and an asset's parent ("runs on", "served by"). Routes are their own entity in knobas: a URL, the asset that exposes it (the reverse proxy) and an optional target asset. They are created by hand or by an estate-file import. There is no Traefik importer, so the map draws only routes somebody recorded;
  - an asset's `depends-on` ("uses").

  Lines through a link set are dotted. Proposed links are dashed in info blue and carry their reason ("proposed: image tag 1.9.0-rc1 matches"). Hovering an edge names the relation.
- **R-4.14** **Hover** a node to light every path through it and dim the rest. **Click** it for a panel with:
  - who watches it (or *Create monitor*);
  - **Why it is on this map**: the chain of relations from the ticket, one per line;
  - its connections in both directions, each a link to that node;
  - *Open*, *Who else uses this*, and *What breaks if this goes down* (for assets, repos and builds).
- **R-4.15** **Who else uses this** adds up to five other tickets to the left column, dashed, joined by dotted "also uses" lines. They come through links, link sets and parent assets; yours sort first.
- **R-4.16** **What breaks if this goes down** marks the node and everything that depends on it in red. The panel lists what it takes down on this job and every other ticket using it ("12 other tickets, 5 of them yours"), with *Put them on the map*.
- **R-4.17** The toolbar also has:
  - *Find on the map*;
  - layer chips: *Pages*, *Builds*, *VMs and proxy*, *Hosts* (off by default), *Proposed links*;
  - *N with a problem*, which traces every failing build, down container or warning back to the ticket;
  - *N not monitored*, which outlines every asset no check watches. The full list, with *Create monitor*, is the Monitors page's *Assets nothing watches* tab (R-11.3).

  A node that the ticket cannot reach is dashed and says "not reached from the ticket". In the example that is the flows-stage route: it is on the job through a link set, but only the shared proxy connects it to the work. Links count both ways for this test, except that from a VM, proxy or host the walk only goes further down.
- **R-4.19** **Grouping keeps a big map on one screen.** Nodes of one kind in one column fold into one box ("3 pages", "3 builds") when they lead nowhere further, at least three of them qualify, and none has a problem. Problems never fold. Other tickets brought in by *Who else uses this* always fold into one box ("12 other tickets"). Clicking a group lists its members, with *Show them one by one*.
- **R-4.20** **Focus here** (in a node's panel) shows only what is within two steps of that node, plus the ticket. Each further focus adds to a trail at the top ("Focused on payout-service › Payout_Build"). A trail chip steps back, and *Show everything* leaves.
- **R-4.21** **Where columns come from.** Each asset type declares its layer: entry point, runs as, runs on or hardware. The map places a node by its type's layer, so a new built-in type needs one more field and no map change. A *custom* asset takes the layer of its parent plus one. Today the types are fixed in code (`knobas_core::asset`: site, hypervisor, vm, container_engine, container, service, module, runtime, database_server, database, schema, table, reverse_proxy, middleware, network, custom). Their ids travel in the share export and never change. Defining your own types is LATER: a type is code because it carries a property schema and a monogram.
- **R-4.22** **Lanes** (View settings › Lanes) split the map into horizontal bands.
  - **Environment** gives *Work and builds*, *Stage*, *Prod*, *Dev*, *Shared*. Assets go by their env. Deploy pipelines go by their target (`*_Deploy_Staging` to Stage, `*_Deploy_Prod` to Prod).
  - **Repository** gives one band per repository, plus *The ticket and its pages* on top and *Shared infrastructure* at the bottom. A PR, build or container goes to the repository it belongs to or was deployed from; a route follows its target.
- **R-4.23** **Summary** (View settings › Detail, or zooming below 70 %) puts builds inside their repository and containers and databases inside their VM. The box then says what it holds and what is wrong: "6 builds, 1 failing", "2 containers, 1 down". Its panel lists the members. A bar above the map offers *Show every item*.
- **R-4.24** **Only problems** keeps the paths between the start and every failing build, down container or warning, and drops the rest. A bar says "13 of 32 shown" with *Show everything*. View settings › *When a map opens* can make this the default.
- **R-4.25** **Columns can move and hide.**
  - A column header shows `‹` `›` and `×` on hover. View settings lists every column with a checkbox and `↑` `↓`.
  - The start's column cannot be hidden.
  - A hidden column is **bridged**: its neighbours connect directly with a "via <name>" line, drawn dash-dot, so no path breaks.
- **R-4.26** **Saved views** keep the column order, hidden columns, lanes, detail, pages, proposed links and only-problems.
  - *View: <name> ▾* lists *Default*, every view that applies here with its scope, and *Save the current view…*.
  - A view is saved for one of: only this job, every ticket under its epic, every ticket with one of its link sets, the map of one asset or repository, or every map.
  - A map opens in its most specific saved view (job, then epic, then link set). Views that apply to every map are offered, never applied by themselves.
  - Stored in knobas, never in a source.
- **R-4.27** **The global map** (`#/map/<entity>`, *Map* in the top bar) starts from any asset, repository or pipeline instead of a ticket. A search field and a few shortcuts (vm-pay-01, traefik, vm-db-01, payout-service, ledger-api, Payout_Build) pick the start.
  - From an asset, it brings in what runs on it, what it runs on, the routes and deploy pipelines that lead to it, and every ticket that uses it.
  - From a repository, it brings in its builds, open PRs and their tickets, and what its pipelines deploy.

  Every node's panel on any map offers *Its own map*. Everything else (groups, focus, lanes, summary, problems, views) works the same.
- **R-4.28** **A ticket can point at a repository without a PR.** A link from the ticket to a repository (relation *works in*) draws as a line from the ticket straight to the Repositories column. It also shows in the job's Code section as "repository, linked to PAY-231 directly". A branch without a PR does the same. BUILT: it is an ordinary link.
- **R-4.18** **Backend**:
  - LOW: a read that returns the job's nodes and edges in one call. It is the context membership read (ADR-0008) plus the relations listed in R-4.13. Layout happens in the UI.
  - LOW: *Who else uses this* is the reverse of membership, the tickets whose job reaches a given entity.
  - NEW, from R-9.13: build chains.
  - LOW: deploy targets, which are a link today or a proposal from the image tag (as in the example).
  - LOW: a `layer` on each built-in asset type (R-4.21).
  - LOW: saved map views, which are knobas-local settings with a scope; one table and its §10.8 entry.
  - The global map is the same read started from an entity instead of a ticket.

### 4c. Other ways to draw the map (`#/job/<key>/map/<n>`), inspiration, 2026-10-15

Björn found Map 1 hard to use: lines cross, names get cut off, and a problem does not say why it is on the job. Six more drawings of the same graph sit as tabs *Map 1* to *Map 7* above the map. Each one starts from a different question. They share one model: the full graph of R-4.18 and one walk out from the ticket (the walk follows R-4.17, so from a VM, proxy or host it only goes further down). Clicking anything in any of them opens one details panel. It shows the trail from the ticket, who watches it, what on the job depends on it, other tickets that use it, and *Open*, *Show on Map 1* and *What breaks if it goes down*. They are meant to be argued with, not built as they are (D12).

- **R-4.29** **Tabs.** A strip above the map names each drawing and says in one line what it tries. Map 1 is R-4.11 to R-4.28 unchanged. The details panel and the selection carry over between tabs.
- **R-4.30** **Map 2, Trails. Problems first.** A headline counts the problems that reach the work ("3 problems reach your work"), then one card per problem, worst first, nearest first. Each card draws a trail. A trail is a line of stops from the ticket to the broken thing, with the relation written over each hop. A run of two or more stops of one kind folds into one stop ("4 builds, Payout_Build … Payout_Deploy_Prod"). The card says how many steps away it is, what on the job depends on it, and how many other tickets use it, with *Open*, *Re-run* for a build, and *Show on Map 1*. A problem the walk cannot reach gets its own heading, and its card says that only shared infrastructure connects it. Below the cards: *Nobody watches these* with *Create monitor*, *Pages that ask something of you*, and a folded *Everything else is fine: N things*.
- **R-4.31** **Map 3, Outline.** The walk from the ticket as a tree. Each row reads as "<relation> <kind> <name>", hanging off the row above, with the env and state on the right. Reverse relations get their own word ("reached at", "deployed by"). A folded branch says what it holds: "2 problems inside" in red, or "3 inside, all fine". A search field and the filters *Everything*, *Problems*, *No check*, *Running* and *Asks you* keep the matching rows and the rows above them. Keys: `↑` `↓` move, `→` unfolds, `←` folds or goes to the parent, `Enter` opens the details, `/` finds. Things the walk cannot reach are listed under the tree.
- **R-4.32** **Map 4, Layers. No lines until you point.** Six bands top to bottom: *Your work*, *Code*, *Builds and deploys* (a chain in the order it runs), *Entry points*, *Running*, *Machines*. Each band starts with its name and "1 with a problem" or "6, all fine". Tiles are coloured by health: a broken tile is solid red, a tile with no check is hatched. Pointing at a tile or selecting it draws the lines to everything it touches and fades the rest. A health bar on top counts everything by state; clicking a segment lights up only those tiles.
- **R-4.33** **Map 5, Distance.** The ticket in the middle. Each ring is one step further out, and things the walk cannot reach sit on a dashed outer ring. Only problems are named, plus the way in to whatever you point at; *Show every name* names everything. The side lists *Closest problems first* with the number of steps in large type. The reasoning: a problem one or two rings out is in your own work, and further out it is in what your work runs on.
- **R-4.34** **Map 6, Ask.** A list of questions, each answered as a sentence whose names open the details, followed by the trails that prove it:
  - What is broken that touches my work?
  - Where does my code run? (per environment: container, machine, address)
  - What does nobody watch? (the ones that take traffic first)
  - How is this connected to my ticket?
  - What breaks if this goes down? (on this job, plus other tickets and how many are yours)
  - What happened here today? (builds and outages as one timeline)
  - Who else works on these systems? (other tickets ranked by how much they share)

  A search field below the questions finds a thing on the job and offers *How it connects* and *If it goes down* for it.
- **R-4.35** **Map 7, Where it runs.** On top, the road from the branch: the build chain as stops, each with run number, state, time and the branch it ran on. A sentence follows: "Your branch reached stage at 12:04. Prod still runs main from #642, yesterday 05:49." Below are columns *Stage*, *Prod* and *Shared by both*. Each service is a stack: its addresses, "routes to", the container with what deployed it, the database it uses, "runs on" the VM, and "runs on" the host. A proposed deploy link shows as a sentence with *Review*. An address on the job with nothing of yours behind it sits apart in a dashed box.
- **R-4.36** **Backend.** Maps 2 to 6 need nothing beyond R-4.18: the same read, walked in the UI. Map 7 needs two things more:
  - LOW: the branch each deploy run built from, which the build sync already stores;
  - NEW, small: the image tag a container runs, so the stack can say which version is where. Today the tag only appears inside a proposal's evidence (suggestion s2). The Docker or Portainer side has to report it, or the deploy run has to declare it.

## 5. Conversation

- **R-5.1** The conversation is the ticket's comments (#2163, `docs/decisions/2026-10-26-conversation-comments-only.md`). The job card's conversation summary: "20 comments, 2 new since 09:00, 2 mention you", the latest comment, and *Read the conversation*.
- **R-5.2** The conversation is the ticket's comments: the Conversation tab shows them all, oldest first, and no status change or PR event (#2163). It has the filter bar:
  - text;
  - person chips (everyone who commented);
  - *Mentions me*;
  - *New since 09:00*.
- **R-5.3** With no filter, the thread folds its old part behind "Show 17 earlier entries, from 2026-08-12". It keeps the last 6 entries and everything new. A blue rule marks "New since your last visit". New comments have a blue edge. Comments that mention you have a yellow edge, a *mentions you* chip, and the `@name` in blue.
- **R-5.4** A comment box at the end. *Comment* goes through the write queue. BUILT (`Comment` op).
- **R-5.5** Data: Jira comments are BUILT (mirrored). Two pieces are LOW. *Last visit* needs a per-entity "last opened" time, which can come from the activity stream or passive observations. *Mentions me* is a match of the account's display name in the comment body, as a named payload read (ADR-0007).

## 6. Connections drawer and proposed links

- **R-6.1** The drawer slides in from the right and does not move the page. `L` opens it, and `Esc` or a click outside closes it. Top row: a filter field, then *Link to…*, *Apply a link set…*, *Save as a link set…*.
- **R-6.2** Groups:
  - **Linked on PAY-231**, links drawn on the ticket. Each row has its section label and *also in Payout stack* when a set holds it too. *Unlink* appears on hover, with *Undo*. BUILT (`create_link`, `unlink`).
  - **From Payout stack, via epic PAY-200**, one group per set or parent. *Hide here* hides the row on this ticket only, with *Undo*. *Edit set* links to the editor. NEW (section 7).
  - **Hidden on PAY-231**, each row with *Show again*.
  - The proposals box: "3 proposed for PAY-231", *Judge them one by one*.
- **R-6.3** The **Proposed links** view is a queue on the left and the current proposal on the right. The queue filter has text, *This job / All*, strength chips (*Strong*, *Indirect*, *Weak*) and *Decided*. Each queue row shows a strength dot, the proposal and its target.
- **R-6.4** A proposal shows both ends as cards, with the relation as an editable dropdown between them under a strength label. Then *Why it was proposed*: the reason in one sentence, the evidence with the matching text marked, *Also found* (what supports it) and *Speaks against it*. Buttons: *Link them* (`Y`), *Not related* (`N`, remembered), *Decide later* (`S`). After a decision the next proposal opens, and the toast offers *Undo*. BUILT (`room_suggestions`, `accept_suggestion`, `dismiss_suggestion`). LOW: the evidence and counter-evidence text, which are a richer reason payload than today's one-line reason.

## 7. Link sets

A ticket should get most of its connections without anyone linking them one by one. A **link set** is a named group of repos, build configurations, pages, assets and monitors. A **rule** says which tickets show it.

- **R-7.1** Rules have four kinds:
  - every ticket under epic X;
  - every sub-task of ticket X;
  - every ticket in project X;
  - one ticket, applied by hand.

  A set can have any number of rules.
- **R-7.2** A member is either an explicit item ("payout-service", "PSP error codes") or a **pattern**, a saved filter that picks up new items as they appear ("Payout_* and PspClient_* in TeamCity project Payouts", "Pages under OPS › Runbooks"). A pattern shows "(8 match now)" and expands to the list.
- **R-7.3** A global switch, **Sub-tasks show their parent ticket's connections**, on by default. With it on, a sub-task's job shows its parent's own links, labelled "Parent PAY-231". No set is needed.
- **R-7.4** The job reads its own links, then the members of every set whose rule matches, then the parent's links. Duplicates show once, with the first origin. Nothing is copied onto the ticket. Editing a set changes every job at once. A new ticket under the epic shows the set the moment it syncs.
- **R-7.5** *Hide here* stores a per-ticket exclusion. It never edits the set.
- **R-7.6** **Link sets view** (`#/sets`):
  - the intro sentence, *New link set*, and the filter bar (text over names and members, chips *Epic / Parent / Project / By hand*);
  - the sub-task switch (R-7.3);
  - a table: name and description, what is in it (counts per kind), what it applies to (one line per rule), tickets now ("39 (10 yours)").
- **R-7.7** **Set editor** (`#/sets/<id>`):
  - the name and description, editable in place, and *Delete*;
  - one panel per kind (repositories, build configurations, pages, assets, monitors), each with *Add…* (a filterable multi-select picker over every item of that kind, where items already in the set show "already in") and, for builds and pages, *Add a pattern…*;
  - the pattern dialog: a project or space dropdown, a name glob or parent page, and a live "Matches now: N" list;
  - *Applies to*, one row per rule with a kind dropdown, a value dropdown, a live ticket count and *Remove*, plus *Add a rule*;
  - *Shows on N tickets now*: the count, how many are yours, and the keys as links (yours in yellow), first 40 then "and N more".
- **R-7.8** *Apply a link set…* (from the job or the drawer) lists the sets with a filter, with *Apply* on each. Sets that already apply are marked "already applies". Applying adds a by-hand rule, and the toast says how many items now show. *Undo* is available.
- **R-7.9** *Save as a link set…* (drawer) builds a set from the ticket's own links:
  - a name;
  - a checklist of the ticket's repos, build configurations, pages, assets and monitors, all ticked; PRs, branches and commits are not offered, and the dialog says why;
  - *Apply it to*: only this ticket, every ticket under its epic (with today's count), every sub-task, or every ticket in its project.

  This is the fast path. Wire one ticket up by hand, then save it as the epic's set.
- **R-7.10** A hint card on the Link sets view proposes adding an item you linked by hand to several tickets under one epic: "You linked Dashboard_Build by hand to PAY-240 and 2 more tickets under PAY-200. Add it to Payout stack?" LOW, a query over links.
- **R-7.11** Backend. NEW, small, with no adapter work and no `WriteOp`:
  - **One migration** with four knobas-owned tables:
    - `link_set` (id, name, description);
    - `link_set_member` (set, kind `repo | pipeline | page | asset | monitor`, entity id **or** a stored query in the search grammar);
    - `link_set_rule` (set, kind `epic | parent | project | manual`, value);
    - `link_set_hidden` (ticket entity, member entity).
  - **Rule matching** uses facts the database already keeps. Epic and parent come from the declared parent and `sync.item_parent` (ADR-0018, ADR-0019; `under:` is already transitive). Project comes from the declared project path (ADR-0010).
  - **Patterns** reuse the smart-list grammar (`source:`, `kind:`, `under:`). A name glob is not in the grammar today; adding it is LOW.
  - **The job read** is the existing context-membership read (ADR-0008) plus one union term for matching sets and one for the parent. It is computed at read time, like membership. Sets never feed the membership walk, the suggestion tray, or another context. That is ADR D3.
  - **About six IPC commands**: list, get, save and delete a set; preview a rule's tickets; hide or unhide on a ticket. Each needs a §10.8 entry.
  - **Export and import** (spec §5a): sets join the share export beside contexts and smart lists.

## 8. Code

### 8a. Code view (`#/code`)

- **R-8.1** Two tabs, *Repositories* first (the default) and then *Pull requests*, each with its count and its own filter bar. There is no cross-repo branch list; branches live in each repository's Branches tab (R-8.8), and a job's branch rows open that tab.
  - **Repositories**: scope *Mine / All*, chips *Cloned here*, *Has open PRs*, dropdowns *Team*, *Language*. Columns: name (with the job key chip if the job uses it), team, language, open PRs, local checkout, last push. A row opens the repository.
  - **Pull requests**: scope, chips *Open / Merged / Closed*, *Waiting on me*, *Idle > 5 days*, dropdowns *Repo*, *Team*. Grouped as *Asking for your review*, *Yours*, *Open, others*, *Merged or closed*. BUILT data (#712 attention order).
- **R-8.2** *New pull request* in the header opens the compare page for the current job's branch.

### 8b. Repository (`#/repo/<name>`)

- **R-8.3** The header shows `tidewater/<name>`, then team, language, last push, and the checkout path and state when cloned. Actions: a **Branch** dropdown that sets the branch for every tab, *New branch…*, *New pull request*, *Open in…* (R-8.22).
- **R-8.4** Tabs: *Files*, *Commits*, *Branches* (count), *Pull requests* (open count), *Local changes* (a warn count when the working tree has changes).
- **R-8.5** **Files**:
  - a tree on the left, folders as labels, files indented; a file changed in the local checkout is marked `M`;
  - the file on the right with line numbers in a monospace face;
  - header actions *Edit*, *History*, *Open in…* with the file (R-8.22);
  - a warn banner when the checkout has an uncommitted change to this file, with *See local changes*.

  Works for any repo, cloned or not, because it reads through Gitea. BUILT (Files tab, `contents_at`, `raw_file`). LATER: syntax highlighting (#701).
- **R-8.6** **Edit**:
  - the file becomes an editor with line numbers, and a banner says "The edit is committed to `<branch>` on Gitea. Your local checkout is never written to.";
  - *Commit change…* opens a dialog with the diff of the edit, a message prefilled with the job key, and a target: *Commit to `<branch>`* (disabled for a protected branch, with the reason) or *Commit to a new branch and open a pull request* (with a branch name field);
  - when the checkout is on the same branch, the dialog warns: "After this commit it is 1 behind; pull in your terminal before you push";
  - the commit goes through the write queue, and the file shows a banner that the commit is queued until Gitea has it;
  - *new branch* goes on to the compare page.

  NEW (D4): one `WriteOp` variant, e.g. `CommitFile { repo, branch, new_branch: Option, path, content, message, base_sha }`, on Gitea's `PUT /repos/{owner}/{repo}/contents/{path}`. That costs a §10.8 entry under ADR-0006 and a live test against the real Gitea (ADR-0013). A `sha` mismatch, meaning someone changed the file since, becomes a **held** write with both versions side by side. That is the queue's existing held state (*Apply anyway / Edit / Discard*).
- **R-8.7** **Commits**: the branch's commits with a filter (text, *Mine*), each row with sha, message, author and time. A row opens the commit: its diff per file and *New branch from here…*. BUILT (`commits`, `commit_diff`).
- **R-8.8** **Branches**: a filter (text, *Mine*, *No PR*, *Merged*), then main (marked *default*) and every branch. Each row shows ahead and behind main (or *merged*), its PR with state, author and updated time. Actions:
  - *Open PR*, when the branch has no PR and is not merged;
  - *Delete…*, which opens a dialog that says either "every commit is in main, nothing is lost" or "N commits will be lost". It warns when an open PR comes from the branch, and says the local checkout keeps its copy.

  *New branch…* asks for a name (prefilled `feature/<job key>-`), a *from* branch or commit, and *Link to <job>*. BUILT (`CreateBranch`, `DeleteBranch`, `compare_as`, `count_commits_not_in`).
- **R-8.9** **Local changes** (clones only):
  - the checkout line (path, branch, ahead and behind, *Open in…*);
  - the diff of uncommitted changes per file, marked "modified, not staged";
  - the line "knobas never commits, stashes or pushes here".

  Not cloned: an empty state that says how to clone it and offers *Set a path by hand*. BUILT: working tree state (ADR-0021). LOW (D5): the uncommitted diff.
- **R-8.10** **Pull requests** (repo tab): the repo's PRs with a filter (text, state chips).

### 8c. Compare and create a pull request (`#/repo/<name>/compare/<head>`)

- **R-8.11** A form:
  - *Into* (default main) and *From* (changing it reloads the page);
  - *Title*, prefilled `<key>: <summary>` when the branch name holds a ticket key;
  - *Description*, prefilled with the ticket's description and `Closes <key>.`;
  - *Link the PR to <key> in knobas*, ticked by default.
- **R-8.12** Under the form, *What it changes* shows the full diff against the base. Beside it, the list of commits.
- **R-8.13** When a PR from that branch is already open, the page says so and offers *Open PR #N* instead of *Create*.
- **R-8.14** *Create pull request* goes through the write queue, then lands on the repo's PR list. BUILT (`CreatePullRequest`, `compare_diff`). LATER: asking reviewers at creation.

### 8d. Pull request (`#/pr/<n>`)

- **R-8.15** The header shows state, head into base, and author. Actions:
  - *Review…*, or *Submit review (N pending)* in warn colour while line comments wait;
  - *Update branch*, *Edit*, *Close*;
  - *Merge…* (primary).

  Tabs: *Overview*, *Conversation* (count), *Files changed* (count), *Commits* (count).
- **R-8.16** Overview lists the checks (row opens the build) and the description. The side panel *Ready to merge?* lists approvals "1 of 2", checks and conflicts, each with a tone dot. When the repo is cloned, it also shows the local checkout.
- **R-8.17** **Files changed**:
  - a file list on the left with a filter, each file with `+N −N` and a tick when viewed; a click scrolls to the file;
  - on the right, one diff per file, in hunks with three lines of context and a `⋯` row for each gap;
  - a *Viewed* checkbox per file, which collapses it;
  - existing line threads under their line, outdated ones grouped apart (BUILT);
  - a `+` button on hover on every line, which opens an inline box; *Add to review* keeps the comment **pending**, drawn with a warn edge and a *pending* chip, until the review goes out.

  BUILT (`pull_diff`, `pull_review_comments`, `SubmitReview`). LOW: *Viewed*, which is knobas-local state per PR head.
- **R-8.18** *Submit review* opens a dialog with a summary, *Comment / Approve / Request changes* (each with a one-line meaning), and the pending line comments that go with it. Submitting goes through the queue. BUILT.
- **R-8.19** *Merge…* opens a dialog:
  - the three checks with tone dots; when Gitea's branch protection blocks the merge, a warn box lists each reason ("needs 2 approvals, has 1", "check #1187 failed") and *Merge* is disabled with the reason as its tooltip;
  - the style: merge commit, rebase, or squash (squash has an editable message);
  - *Delete <branch> after merging*;
  - when the PR is linked to a ticket, a line that it moves to In Review after the merge.

  BUILT (`MergePullRequest`, `follow_merges`, branch protection read).
- **R-8.20** Commits tab: the PR's commits. A commit that has a proposed link shows *proposed link, judge it*.

### 8e. Open in… (your own applications)

- **R-8.21** **Applications** are a list in Settings › Checkouts, managed from any Open in… menu. Each one has:
  - a name, e.g. "MyGitViewer";
  - a command, e.g. `MyGitViewer {path}`;
  - an optional command for a file, e.g. `code --goto {file}:{line}`.

  Shipped as the first entries on macOS: VS Code, Terminal, Fork, IntelliJ IDEA. They are ordinary entries and can be edited or removed. One application is the default.
- **R-8.22** **The Open in… button** is one button, "Open in… ▾", and the whole of it opens the menu (Björn, 2026-10-15: a split button whose list hid behind a small arrow was hard to hit).
  - The menu lists every application with its command filled in for this repository, the default first and marked *(default)*, then *Add an application…* and *Manage applications…*.
  - The default has focus when the menu opens, so `Enter` runs it: the default still costs one click and one key.
  - The menu works with arrow keys, and `Esc` closes it.
  - It appears on the repository header, the file view (where the file command is used if there is one), every cloned row of the Repositories list, the job's Code checkout line, the Local changes tab and the PR's checkout panel.
  - A repository that is not cloned shows **Clone** in its place (R-8.25), never a disabled button.
  - Starting an application says so in a toast with the command it ran.
- **R-8.23** **Add an application** dialog:
  - fields for name, command, and command for a file;
  - the placeholders as buttons that insert at the cursor: `{path}` the checkout folder, `{file}` the open file inside it, `{line}` the current line or 1, `{name}` the repository name;
  - *It will run*, a live preview on a real checkout;
  - warnings when the command has no `{path}`, when `{file}` or `{line}` sit in the repository command, and when it holds shell characters (`|`, `;`, `&`, `>`), which are passed through as plain text;
  - *Try it on <repo>*, *Cancel*, and *Add application*.

  *Manage applications* lists them with the default as a radio button, *↑* to reorder, and *Edit…*. *Remove* is in the edit dialog.
- **R-8.24** **The rules, from ADR-0016.**
  - Every placeholder is filled from this disk only: the checkout path the clones-root scan found, or the path a person set, and file paths inside it. `{name}` is the name of that checkout's folder.
  - Branch names, titles, URLs and every other synced value are never passed to a program, so there is no `{branch}` placeholder.
  - The command is split into arguments and run without a shell, and a path with spaces stays one argument.

- **R-8.25** **Clone** (D16). A repository that is not on this machine shows **Clone** where *Open in…* would be. That covers every Repositories row (the *Local checkout* column says "not on this machine"), the repository header, the Local changes tab, and the job's Code checkout line.
  - One click clones it into `<clones root>/<repository name>`. The tooltip names the folder first: "Clones into ~/code/booking-web".
  - While it runs, the button reads "Cloning… 52 %", with **Stop** beside it. Stopping removes the half-made folder and says so: "Stopped cloning auth-gateway. Nothing was left in ~/code."
  - When it is done, the button becomes *Open in…*, the *Local checkout* column shows the branch, and a toast says "Cloned auth-gateway into ~/code/auth-gateway, on main" with *Open in VS Code* (the default application).
  - With no clones root set, *Clone* first asks for one ("Where should clones go?", prefilled `~/code`) and saves it to Settings › Checkouts.
  - *It is already cloned somewhere else…* stays on the repository page, to set a path by hand.

  **Errors**, each said in place on the row, with what to do:
  - the folder already exists: if it is a clone of this repository it is simply adopted; otherwise "~/code/booking-web exists and is not a clone of this repository. Rename it, or set a path by hand";
  - authentication failed: "git could not sign in to git.tidewater.internal. knobas uses your own git credentials (ssh agent or credential helper); check them in a terminal", with *Try again*;
  - the network or the server failed: git's last line, with *Try again*;
  - the disk is full.

  **The rules this keeps, since a value from the mirror now reaches a program** (ADR-0016's concern):
  - git runs **without a shell**, as an argument list, with `--` before the URL and the folder.
  - The URL is **built**, not taken: the Gitea source's configured address (typed by a person) plus the repository's full name. The name must match `^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$`, and anything else is refused. The source's `web_url` and other mirrored URLs are never used.
  - Only `https` and `ssh` transports are allowed (`-c protocol.allow=never -c protocol.https.allow=always -c protocol.ssh.allow=always`), so no `ext::` or `file://`. No submodules are cloned.
  - The target is always a **new** folder directly under the clones root, named by the checked repository name. knobas never writes into an existing folder.
  - knobas passes **no credentials**. git uses the person's own ssh agent or credential helper, so knobas holds nothing new.
  - Clone is the only writing git command. knobas still never checks out, fetches or pulls in an existing clone.

  **Backend:**
  - NEW, small: a `clone_repo` command that runs in the background with progress events (git's `--progress` output, parsed) and can be cancelled. It needs a §10.8 entry and the ADR-0016 amendment (D16).
  - The checkout scan already finds the result. `clone_command`, the text kept for showing what will run, becomes the tooltip's detail.
  - Tests: a live test against the testenv Gitea (clone, refuse an existing folder, refuse a bad name, cancel) plus unit tests on the argument list. It needs no desktop witness, because nothing is opened.

  Backend for R-8.21 to R-8.24: LOW. The editor and terminal command templates exist (BUILT, Settings › Checkouts, `open_checkout`). This turns two fixed templates into a list with names, a default and an optional file command. That is one migration or settings change and one change to the `open_checkout` arguments, which takes a template id and an optional file and line, with §10.8 entries. The desktop witness of ADR-0016 covers it as it covers the editor today.

## 9. Builds (`#/builds`, `#/builds/<pipeline>`, `#/build/<pipeline>/<run>`)

The layout follows TeamCity's: pipelines on the left, and the selected pipeline's runs on the right. It differs in three ways:
- one click from a pipeline to its runs, and one more to a run, with the pipeline list always in view;
- the run page opens on *what went wrong*, not on a log;
- the frame is written against a generic pipeline and run (D7), so another CI system fills the same screens.

### 9a. Pipeline list (left, always visible)

- **R-9.1** At the top: a scope toggle *This job <key> / All* (shown when there is a current job, *This job* by default), a filter field ("Filter 59 pipelines", `/` focuses it, `Esc` clears it), and chips *Failing* and *Running*.
- **R-9.2** **This job** keeps only the pipelines relevant to the job. Each shows why, in a short second line:
  - *your branch*: it has a run on one of the job's branches. This is found automatically, with nothing to set up. It compares the run's branch with the branches the job is connected to;
  - *PR check*: it checks a PR linked to the job (BUILT, the checks line, #710);
  - the name of a link set that holds it, e.g. *Payout stack* (section 7).

  A line at the top says "10 of 59 pipelines are relevant to PAY-231" and offers *Show all*. When nothing is relevant, the empty state offers *Search all pipelines*.
- **R-9.3** **All** shows *Pinned* first (your own list, *Pin* and *Unpin* on the pipeline header; LOW, knobas-local), then the source's **project tree as deep as it goes**. TeamCity projects nest to any depth (Root › Payments › Payouts › Deploy):
  - each project is a fold with its total count and *N failing* for everything below it, and inside it come its own pipelines first, then its subprojects, indented one step per level;
  - a project that holds no pipelines and only one subproject folds into one row with its child, e.g. "Payments › Payouts", so a deep tree does not cost a click per level;
  - a fold opens by itself when something below it fails, holds the selected pipeline, or matches a filter. Otherwise it remembers what you last did with it.

  *This job* uses the same tree, pruned to the relevant pipelines.
- **R-9.4a** A row has no status dot. It shows the name (in err tone when its latest run failed), the time of the latest run, and a strip of the last 12 runs as small coloured blocks, newest right. A hover on the row gives the full project path. The selected pipeline is drawn inverted. The list scrolls on its own and keeps the selection in view.
- **R-9.4b** Backend for the tree. LOW in the TeamCity adapter:
  - read `/app/rest/projects` with `parentProjectId` once per sync;
  - declare each build configuration's project path as a declared path, so the path line and `under:` see every level;
  - make the `project_ids` sync filter include subprojects; today it matches the exact project id, so a configuration in a subproject of a listed project is not synced.

### 9b. Pipeline (`#/builds/<pipeline>`)

- **R-9.4** The header holds:
  - the name, and one line that says what it is in the source's word ("TeamCity build configuration in project Payouts"), which repo it builds (as a link to the repo view), and what triggers it;
  - actions: **Run…** (primary), a one-click **Run on <job branch>** when the current job has a branch this pipeline builds (*Run on main* otherwise), *Pin*, *Open in TeamCity*.

  When the source's sync is failing, a warn line says the data's age and that runs you start wait in the queue, with *Re-enter password*.
- **R-9.5** **History chart**, one bar per run for the last 30:
  - height is duration, colour is the result (passed, failed, running), oldest left, with a legend;
  - each bar has a hover and focus tooltip (number, result, branch, duration, when), and a click opens the run;
  - beside it, three facts: *Passed* ("26 of the last 34"), *Usual duration* (the median), *Last passing* (a link).
- **R-9.6** Tabs *Runs* (count), *Parameters*, *Triggers*.
  - **Runs** has the filter bar: text over number, branch, commit and person; scope *My job's branches / Started by me / All*; chips *Failed / Running / Queued / Passed*; a *Branch* dropdown. Runs are grouped by day (*Today*, *Yesterday*, date). A row shows the number and result chip (no dot), branch (with the job chip when it is the job's branch), what started it ("VCS change a41f2c", "Schedule, nightly", a person), the test summary ("1 failed, 41 passed"), duration with a bar relative to the slowest run, start time, and *Log* and *Run again…*. A click opens the run. BUILT data: mirrored builds per configuration. LOW: history older than the sync budget, fetched on scroll.
  - **Parameters** lists each default with its kind and *Change…*, which writes the default through the queue. BUILT (`SetParameter`).
  - **Triggers** lists the triggers, read only. BUILT.

### 9c. Run… dialog

- **R-9.7** One dialog for every way of starting a run:
  - **Start from**: *Defaults*, *#N settings* (when opened from a run) or a saved preset; picking one fills the form;
  - **Branch**: a combobox with type-ahead over the branches of the repo the pipeline builds. Main is marked *default* and the job's branch *your job's branch*. Arrow keys and `Enter` work. Any typed name is allowed;
  - **Parameters**: a typed field per declared parameter (a select for a fixed list, text otherwise, with a hint for what empty means). A field that differs from its default is marked *changed*. *Add a parameter* adds a free name and value row;
  - **Source options**, collapsed under the source's name ("TeamCity options"): clean checkout, put first in the queue. LOW, since `run_build` carries only branch and parameters today;
  - **Save these settings as a preset named …** (LOW, knobas-local per pipeline);
  - a warn line when the source's sign-in is failing: the run waits in the write queue;
  - the footer summarises what will run: "On feature/PAY-231-sepa-retry, with test.filter=sepa::retry". Then *Cancel* and *Run*.

  *Run* queues the write. The run appears at the top of the list as **queued** at once, never as started. BUILT (`RunBuild { branch, parameters }`, `TriggerBuild`). LOW: parameter kinds and options read from the configuration's parameter specs.
- **R-9.8** *Re-run* (same settings) and *Run again with changes…* (the dialog, prefilled from that run) sit on every finished run. BUILT (`RerunBuild`).

### 9d. Run (`#/build/<pipeline>/<run>`)

- **R-9.9** The header shows the pipeline and number with the result chip, then branch, what started it, when, and how long it took. Actions: *Re-run*, *Run again with changes…*, *Open in TeamCity*. A running run shows *Stop*, disabled with "needs a new write" (LATER).
- **R-9.10** Under the header, a **steps bar**: one segment per step, width by duration, colour by result (running in info, not started in grey), the step name inside, and a tooltip with the duration. A click opens the Log tab at that step.
- **R-9.11** Tabs:
  - **Overview** leads with the problem. For a failed run, the first failing test with its assertion and file:line, then *Show it in the log*, *Open the test file* (repo view), *Create ticket* (LOW). One line puts it in context: "Also failed in #1185 on this branch. #1186 on main passed". Then two panels. *Run* lists branch, commit (a link to the commit view), started by, agent, started, and took with the difference to the previous run on the branch. *Artifacts* lists the first four with *Download*. A running run says when it was last seen.
  - **Log**:
    - the log split into its steps as collapsible sections, each with duration and line count; failed and running steps start open, passing ones closed;
    - line numbers, and timestamps that can be toggled;
    - error lines in err tone, warnings in warn;
    - a sticky toolbar: *Find in log* with "2 of 5", previous and next (`Enter` and `Shift+Enter`); chips *Errors and warnings only*, *Wrap lines*, *Timestamps*; *Jump to the first error*; *Download log*;
    - a search opens every step that holds a match.

    BUILT: the last lines (`log_tail`). LOW: the whole log, streamed with a cap, and the step boundaries.
  - **Tests**: the filter bar (text, *Failed / Passed*, *Module*), failed first. A failed test expands to its assertion and location, *Show in log*, and how often it failed in the last 10 runs. BUILT (`testOccurrences`). LOW: the per-test history line.
  - **Artifacts**: a tree with folders that fold, each with its file count and *Download folder*. Files show size, *Preview* for text files (inline, with line numbers) and *Download*. *Download all as zip* sits under the tree. BUILT: top-level list. LOW: folders, preview, and downloads through knobas (D8).
  - **Changes**: the commits in the run, each opening the commit view. BUILT.
  - **Parameters**: each parameter with its value in this run and the default, plus the branch. *Run again with these…* opens the dialog. BUILT (Parameters tab).

### 9e. The generic shape a CI adapter declares (D7)

- **R-9.13** **Build chains** are the other kind of nesting: in TeamCity, a configuration can wait on others through snapshot dependencies (Payout_Build › Payout_Docker › Payout_Deploy_Staging › Payout_Deploy_Prod).
  - The pipeline header names the chain and the pipeline's step in it ("Step 2 of the chain …").
  - A run of a chained pipeline shows a *Chain on <branch>* panel. It has one box per step with that step's run on the same branch and its result, and the current step is outlined. A box opens that run, or the pipeline when the step has not run on the branch.
  - The panel says that a run of a later step runs the steps before it too.

  NEW, small: read `snapshot-dependencies` per build configuration and the dependency builds per build. Read only; no new write.
- **R-9.12** For the frame above to work with any CI system, an adapter declares two kinds and their fields as declared paths (ADR-0018 style):
  - **pipeline** (TeamCity: build configuration; GitLab: pipeline schedule or project pipeline; Jenkins: job). Fields: name, group (project or folder), repo it builds, default branch, parameter specs (name, kind, choices, default), and triggers as text.
  - **run** (TeamCity: build; GitLab: pipeline; Jenkins: build). Fields: number, state with a tone map (the existing state chip, #666), branch, commit, started by, started, duration, agent, steps (name, state, duration), test summary.
  - **Reads**: log (a range of lines), tests, artifacts (a tree, and a download stream), changes.
  - **Writes**: `run_build`, `rerun_build`, `trigger_build`, `set_parameter`, which exist. `stop_run` is LATER.

  TeamCity fills all of it today, except the parts marked LOW above. Other adapters are LATER and are certified against a real instance (ADR-0013).

## 10. Docs (`#/docs`, `#/docs/space/<key>`, `#/docs/space/<source>:<key>`, `#/page/<id>`, `#/page/<id>/<section>`)

The job of this view is that nobody has to open Confluence. Search comes first, because Confluence's own search is the main reason people avoid it. The second job is reading and small edits, done well enough that the browser tab stays closed. `prototype-5.html` is the reference.

### 10a. One view: the reader, with space and search at the top left

There is no separate Docs home and no separate results page. `#/docs` opens the reader on the page you read last. `#/docs/space/<key>` picks that space and opens its first page. Where two sources hold the key, the space's address names its source the way an entity id does, `#/docs/space/<source>:<key>`, and a bare key opens the first source's space (2026-10-21, #1455, the shape #1476 gave a job).

- **R-10.1** The **left pane** starts with two controls, at the very top left:
  - a **space picker**: *All spaces (84)*, then every space with its page count. It follows the open page's space until you pick one;
  - a **search field** that names its scope ("Search Engineering", "Search all spaces"). `/` focuses it from anywhere in Docs;
  - one line of syntax under it: `"…"` phrase, `-word` leave out, `by:`, `updated:7d`.
- **R-10.2** **With the field empty**, the pane shows:
  - **For you**: pages that mention you or ask something of you, each with the reason in warn tone;
  - **the page tree** of the picked space, where folders fold and a folder's title is itself a link. With *All spaces* picked, every space is one more fold level.

  *New page* sits at the bottom of the pane.
- **R-10.3** **With text in the field**, the same pane shows the hits instead of the tree, updated on every keystroke.
  - A header line: "5 pages in all spaces, 1 ms". In a single space it also offers *Search all spaces*.
  - Each hit has the title in the reading face with matches marked, the space (when searching all) and the path, and a snippet. The snippet is the best sentence, cut around the first match and prefixed with its section or "Comment by Priya". Then the match count, who edited it, and when.
  - The open page's hit is marked with a bar on the left.
- **R-10.4** **Query syntax**: every word must appear, with simple stemming ("reviews" finds "review"); `"…"` is a phrase; `-word` and `-"phrase"` leave out; `by:priya` and `updated:7d|30d|1y|today` filter. The space comes from the picker. BUILT: `websearch_to_tsquery('english', …)`, and the `updated:` and `author:` filters. LOW: the space picker as the docs scope of the project filter.
- **R-10.5** **A hit opens in the reader** with the query carried over: every match marked, the find bar at the top ("Matches for retry, 1 of 4", previous and next), and the outline counting matches per section. `↑` `↓` in the search field open the next or previous hit. `Enter` opens the selected one. Clearing the field brings the tree back and drops the highlighting. The reader is the preview: there is no second preview pane.
- **R-10.6** **Data**. BUILT: page bodies and comments are in `body_text` (#287), and snippets come from `snippet.rs` (`ts_headline`). LOW: the section name beside a snippet.
- **R-10.7** *(merged into R-10.3)*
- **R-10.8** The empty result explains the AND rule and, in a single space, offers *Search all spaces*.

### 10b. Reader (`#/page/<id>`)

- **R-10.9** Two panes (amended 2026-10-25, #2078; `docs-reader.html` is the reference):
  - **left**: the space picker, search, and the tree or the hits (R-10.1 to R-10.3), 300 px wide;
  - **right**: the page, at full width, with nothing capped: running text, panels, comments, code, tables and the section editor all fill it. The page's outline is a sticky column on its right (R-10.15).

  Below 1240 px the outline folds above the page, closed, and the tree narrows to 220 px. Below 900 px the tree becomes a drawer that slides over the page: a *Pages* button at the start of the breadcrumb row opens it, and so do `/` and a label chip.
- **R-10.10** **Header** (amended 2026-10-25, #2078): the breadcrumb (job chip, Docs, space, parent), the title, then the meta line: who edited it when, *Created by* who and when, the version, the reading time, and labels. A label is a button that searches for it. The actions row, in order: *Link to <job>* (only when the page is opened from a job), *Link to…*, *New page here*, *Copy link* (copies the knobas address and the Confluence URL), *Open in Confluence*, and *More ▾*, which holds *Export as Markdown* and is where later actions go. The header has no *Comment* button: commenting happens in the comment box at the page foot (R-10.14).
- **R-10.11** **The body** is drawn from a fixed set of blocks. The interface uses Archivo; the page body uses Source Serif 4 at 17.5 px with 1.65 line height.
  - headings (h2) with an anchor link and *Edit section* on hover;
  - paragraphs, lists, inline code;
  - code blocks with *Copy* and the language;
  - tables in the interface face;
  - info, warning and note panels;
  - **Jira references as live chips**: key, summary and the ticket's current status from the mirror, opening its job;
  - links to other pages as in-app links;
  - `@mentions`, with yours on the job-card yellow;
  - any macro knobas does not draw becomes an honest placeholder: "This part is a Confluence Roadmap macro, which knobas does not draw", with *Open this part in Confluence*.

  BUILT: the storage body by section. LOW: the panel, code, table, Jira-issue and user-mention macros mapped to blocks, as a stated subset of the storage format.
- **R-10.12** **Find in page**. Arriving from a search keeps the query: a yellow bar at the top says "Matches for <query>" with an editable field, "1 of 6", previous and next (`Enter` and `Shift+Enter`), and ✕, which stops the highlighting. The hits stay in the left pane. Every match is marked. The outline shows the match count per section.
- **R-10.13** **Edit section**: a section of only text, lists and panels turns into a text box in place, with *Save section* (through the write queue) and *Cancel*. A section with a table, code or a macro shows the button disabled, with the reason. BUILT (`UpdatePage`, the text-only rule).
- **R-10.14** **Comments** at the end of the page, with the composer. BUILT (`Comment`).
- **R-10.15** **The margin** right of the page holds only the outline, headed *On this page* (amended 2026-10-25, #2078). The outline sticks while the page scrolls, follows your scrolling, and shows the match count per section while the find bar is open (R-10.12). Below 1240 px the same outline is a closed fold between the actions row and the first section, showing the number of sections, and the total match count while the find bar is open.

  The rest of the page's context sits at the **page foot**, after the body and before the comments, in this order, each left out when empty:
  - *Work it belongs to*, the tickets the page references or is linked to, with key, title and status, as an open list;
  - *Linked from*, the page's backlinks, grouped by space. Each space is a fold, closed every time a page opens, showing the space key, its name and its count, and opening to a list of pages. The heading counts the pages, "50+" when the reader's limit of 50 is reached;
  - *Attachments*, once built.

  The page's history is the last edit and *Created by* in the meta line (R-10.10); there is no History section.

  BUILT: backlinks, a link extraction from the storage body at sync time, and the version fields behind *Created by*. Comparing versions is LATER. Attachments are not synced (LATER).
- **R-10.16** **New page**: space, parent (*Under*, every page of the space), title, *Link it to <job>*, and a plain-text body where `##` starts a section and `-` starts a list. It goes through the write queue. BUILT (`CreatePage`).
- **R-10.17** **Export as Markdown** downloads the page as `.md`: headings, lists, code fences, tables, panels as quotes, page links as relative `.md` links, and macros as HTML comments. LOW, but it is the same converter the move away from Confluence needs (D9), so it is worth building early.

### 10c. Beyond Confluence (D9)

- **R-10.18** Nothing in these screens is Confluence-only except the macro placeholder and the *Open in Confluence* button. A docs source declares:
  - spaces;
  - a page tree (parent as a declared path, ADR-0018);
  - a page's body as the block list of R-10.11;
  - comments, versions, labels;
  - the writes `create_page`, `update_page` (by section) and `comment`.

  A **Markdown source** maps onto it directly:
  - a folder of `.md` files in a Gitea repository, where folders are the tree and a section edit is a commit through the `CommitFile` op of R-8.6;
  - or a Markdown wiki with an API (Outline, Wiki.js, BookStack).

  Search, the reader and the preview need no change.

## 11. Assets and monitors, apart (`#/assets`, `#/asset/<id>`, `#/monitors`)

They are two different things. An **asset** is a thing in the estate: a host, VM, container, service, route, database or reverse proxy, knobas-owned (ADR-0014). A **monitor** is a check in Uptime Kuma that watches one asset. An asset can have no monitor, one or several. A monitor always names the asset it watches. Each gets its own view and its own section on a job, and link sets hold them as separate kinds (*Assets*, *Monitors*).

### 11a. Assets (`#/assets`)

- **R-11.1** Two panes. **Left**, the estate as a tree, deepest first under its parent: host › VM › container › database, edge VM › reverse proxy › routes. Above the tree:
  - the scope *This job / All*, where *This job* is the job's own assets and its link sets;
  - a filter over name, id, IP, image and every property;
  - type chips and env chips, plus *With a problem*.

  A row shows the name (monospace), the type, *down* or *warning* when one of its own monitors says so, *no monitor* (dashed) when nothing watches it, *N problems inside* when a descendant has one, and the job chip. Rows that only lead to a match are drawn dimmed. *New asset* and *Import…* sit under the tree. BUILT (`asset_tree`, `CreateDialog`, the importers).
- **R-11.2** **Right**, the selected asset:
  - the breadcrumb of its parents, then the name, type, env, owner and id;
  - actions: *Open URL* (routes), *Copy SSH* (VMs), *Link to…*, *Create monitor* (when nothing watches it), *Edit…*;
  - a **health line**: *Down*, *Warning*, *Healthy* or *Not monitored*, with the reason and the problems inside;
  - **Properties**, typed then custom, with secrets masked behind *Show*;
  - **Holds**, its children with their state;
  - **Monitors watching it**, each with its 24-hour bar, linking to Monitors, or *Create monitor* when there is none;
  - on the side: **Work it belongs to** (linked, or through a link set), **Depends on this** ("If vm-pay-01 goes, so do …"), **History**.

  BUILT (`get_asset`, `depends_on_this`, the health rollup, custom properties).

### 11b. Monitors (`#/monitors`)

- **R-11.3** Two tabs: *Monitors* and *Assets nothing watches*.
  - **Monitors** has the filter bar (text over monitor, asset and type; scope *This job / My jobs / All*; state chips *Down / Warning / Up / Paused*; a *Type* dropdown). Rows are sorted down, warning, paused, up. Columns: monitor and type, state chip, 24-hour bar, response, 30-day uptime, **Watches** (the asset's name and type, linking to the asset), and *Ack* on down monitors plus *Pause* or *Resume*. BUILT (`monitor_roster`, `ack_alert`, `PauseMonitor`, `ResumeMonitor`).
  - **Assets nothing watches** lists containers, services, routes, databases and VMs without a monitor, each with *Create monitor*. BUILT (`unmonitored_assets`).
- **R-11.4** **Create monitor** is the same dialog from both views. It is prefilled from the asset: HTTP for a route or service with its URL plus `/health`, TCP for a database, Docker for a container, Ping for a VM. It goes through the write queue and is attached to the asset. BUILT (`CreateMonitor`).

### 11c. On the job and in link sets

- **R-11.5** The job's **Assets** section lists its assets: own links, link sets and the parent. The rollup reads "9 assets in stage, prod, 2 with a problem". Each row gives type, env, parent and, when a monitor complains, its state. The **Monitors** section lists the job's monitors with their bars. The rollup reads "7 watched: 2 down, 1 warning, 4 fine". Both have the in-section filter and *Open Assets* or *Open Monitors*.
- **R-11.6** A link set has *Assets* and *Monitors* as two kinds, each with its own picker. Payout stack, for example, holds the prod payout containers, vm-pay-01 and the payouts route as assets, and the payouts health, cert and worker checks as monitors.

### 11d. Dependencies (`#/deps`, and the job's Map tab), 2026-10-15

Björn liked Map 4 (Layers) best and asked why it could not be the asset page. Dependencies is that view, grown up. It replaces *Assets* and *Map* in the top bar (D13). The job's *Map* tab opens it for the ticket; Map 1 to 7 stay behind *Earlier drawings* in the same tab.

- **R-11.10** **Two openings, two defaults.** From a ticket's Map tab it shows only what the ticket reaches (the R-4.18 read), in the view set as *opens from a ticket*. From the top bar it shows everything knobas knows, in the view set as *opens from the top bar*. *Which view opens first…* in the view menu sets both. From a ticket, *Show everything* goes to the top-bar opening. From the top bar while a job is open, *Only PAY-231* goes back.
- **R-11.11** **Layers, top to bottom.** Each layer is a band. On the left: its name, a one-line summary ("17 services, 1 reverse proxy"), what is wrong ("2 broken, 7 warning", or "all fine"), and a thin health bar. On the right: its items. The default scheme is:
  - *Work*: tickets, pull requests, pages;
  - *Code*: repositories;
  - *Delivery*: pipelines;
  - *Entry points*: routes;
  - *Services*: service, reverse proxy, middleware, runtime, module, custom;
  - *Containers*: container engine, container;
  - *Data*: database server, database, schema, table;
  - *Machines*: VM;
  - *Hardware and network*: hypervisor, site, network.

  A reverse proxy runs on a VM like any service, so it sits in *Services*, not *Machines*.
- **R-11.12** **Tiles.** A tile shows the type's monogram (the built-in two letters; TK, PR, PG, RE, PL and RO for the non-asset kinds), the name, and one state word. A broken tile is solid red; a warning is amber; a running pipeline or open PR is outlined blue; an asset no monitor watches is hatched. Above 30 items in a layer the tiles shrink to one line with a state dot. The state word uses the built-in type labels (D15).
- **R-11.13** **Layers are one shared set with unique names** (D14). *Layers…*, or *Change the layers* under the rail, opens a panel in three parts:
  - **The layers, top to bottom**, the same in every view. You can add, rename, remove and reorder them, and each row lists the kinds it holds. A name must be unique, ignoring case and spaces at either end. A rename to a name already in use is refused in place: "There is already a layer called Services. Names are unique, so a name always means the same things." A new layer gets the first free name ("New layer", "New layer 2") with the name field focused. Removing a layer removes it from every view, and its kinds go to no layer, which the toast says.
  - **What goes where**, also shared: every kind (the 16 asset types plus ticket, pull request, page, repository, pipeline and route) sits in exactly one layer or in none, with its count.
  - **In the view <name>**, the only part a view keeps: for each layer, show or hide, folded by default, and group by (none, environment, owner or team, what it runs on, project, status); plus the tile size and the item count above which healthy groups fold.

  Changes to the shared parts apply at once and are kept straight away, with a toast ending "Every view uses it". Changes to the view part mark the view *changed* until it is saved (R-11.16). Items whose kind is in no layer are named under the bands ("6 items are in no layer: Network. Place them in Layers…"), never dropped silently.
- **R-11.14** **The rail** on the left lists every layer with a checkbox, its count, and a red count of problems. Clicking a name scrolls to the band.
- **R-11.15** **Pointing and clicking.**
  - Pointing at a tile (or focusing it) draws lines to everything it touches, up and down, and fades the rest. A line to something folded ends at the fold.
  - Click or `Enter` opens it: a ticket opens its job, a PR its page, a repository its repository page, a pipeline its Builds page, a page the reader. An asset opens its editor (R-11.19) and a route opens the route editor (R-11.21).
  - `⌖` on a tile, or `T`, shows only what it touches, with *Show everything* to leave.
  - The arrow keys move between tiles.
- **R-11.16** **Views** (*View: <name> ▾*). A view keeps, per shared layer, whether it shows, folds and how it groups, plus the environment filter, *Problems and what they touch*, and tile size. It never holds its own layers (D14). The menu shows each view's "6 of 9 layers". The menu offers:
  - every view, with which opening it is the default for;
  - *Save changes* and *Discard changes*, shown when the current view has changed (the button reads "changed");
  - *Save as a new view…*, which can make the new view a default;
  - *Rename* and *Delete*;
  - *Which view opens first…*.

  Built-in views: *Everything* (Work hidden), *This ticket* (no grouping), *Runtime by environment*, *Code and delivery*. Views are stored in knobas.
- **R-11.17** **Filters** in the bar: *Find* (dims what does not match, opens the folds that hold a match, counts the matches, `Enter` jumps to the next), environment chips, and *Problems and what they touch* with the problem count.
- **R-11.18** **Lots of items without confusion.** These rules apply together:
  - Problems never fold.
  - In a layer above the fold count, a group with no problem folds into a chip ("Booking 24", with a small health bar). Groups with problems stay open, problems first.
  - In such a layer, an open group shows its problems and a few others, then "37 more, all fine". While *Find* has text, it shows its first 24 matches too, and its *more* counts the matches it holds ("37 more, 12 found").
  - A layer can fold to its summary line.
  - A search opens whatever holds a match: layer folds and chips open. A group keeps its *more*, so every match is drawn as a tile or counted on its group's *more*, which *Find*'s `Enter` opens on the next match. A short query can match half the estate, and drawing each match would break the 100 ms keystroke budget (#2214).
  - Counts use thousands separators.

  The prototype's *Try it with about 1 700 more made-up items* shows 2 106 items this way.
- **R-11.19** **The asset editor** (a dialog over the view). It holds:
  - the name, as an editable title;
  - type and health as text (the type does not change after creation);
  - *Inside*, a picker that moves the asset with everything it holds;
  - environment and owner, each with *Inherit (prod from vm-pay-01)*;
  - own status;
  - the type's typed properties in its order, then custom properties, then *Add a property*;
  - *Depends on*, with *Add what it depends on…*;
  - *Needed by*, with a count of what is inside it;
  - *Watched by*, with *Create monitor*;
  - linked work.

  The footer has *Delete…* (two clicks; disabled with the reason while it holds assets or exposes routes, see R-11.19a), *Open in the asset tree*, *Cancel* and *Save*.
- **R-11.19a** **Checked against the code (2026-10-15).** Two things the mockup guessed, and one it did not mention:
  - **Delete** (`knobas_app::assets::delete`): knobas refuses while the asset **holds any asset** and also while it **still exposes routes** ("still exposes 2 route(s), delete them first"). Routes that **land on** it do not block: their target is cleared, and each route gets its own history line. The asset's entity is **tombstoned**, so links to it stay visible and are marked as deleted; its history is kept. The mockup first said "its links and history go too", which was wrong. It now disables *Delete…* with the reason when the asset holds assets or exposes routes, and the confirm says how many routes will land on nothing and that links stay, marked deleted. To build: the dialog shows the backend's `conflict` text in place when the state changed underneath.
  - **Save** (`edit_asset`): one call takes a **list of edits** (name, status, environment, owner, property, monitors). knobas applies them in order, in **one transaction**, with one history line each, and an edit that changes nothing writes no line. So *Save* is **one** `edit_asset` call carrying only the fields that changed, never one call per field. Two parts of the dialog are **separate calls** and do not share that transaction: *Inside* is `move_asset` (refused if it would make a cycle), and *Add what it depends on* is a link create. To build: send the edit list first, then the move, then the links. If a later step fails, keep the dialog open, say which part did not save, and keep the parts that did. The type cannot be edited, which the dialog already reflects.
  - **Create** (`create_asset`): a hand-created asset sets **no environment and no owner**; it inherits both from what it sits inside (#431). The mockup copies the parent's values into the new row for display only. To build: leave both unset, and show "Inherit (prod from vm-pay-01)" in the editor, as R-11.19 already describes. Linking the new asset to the ticket is a second call (a link create), after the asset exists.
- **R-11.20** **Adding.** *+ Add* on each asset layer, and *Add ▾* in the bar (Asset, Route, Import…, and *Link to PAY-231…* from a ticket). New asset asks for a name, a type and a place, as `CreateDialog` does:
  - the layer's types come first ("In this layer"), then any type;
  - *Inside* shows the parent's usual children ("Usual inside vm-pay-01: Container engine, Service, Database server, Reverse proxy. Any type is allowed.");
  - from a ticket, *Link it to PAY-231* is ticked by default.

  After *Create*, the new tile is scrolled to and flashes once, and a toast offers *Edit it*.
- **R-11.21** **Routes.** The route dialog asks for:
  - the address (it must carry a scheme);
  - a name (from the address if left empty);
  - *Exposed by*, which is fixed once the route exists;
  - *Lands on*, where "Nothing knobas knows" is a real choice;
  - who can reach it.

  From a ticket it also offers to link the route. Edit uses the same dialog with *Delete…*.
- **R-11.22** **Not in this round:** dragging to create a relation or to move between layers, multi-select and bulk move, and a minimap. They are worth having once the layout settles.
- **R-11.23** **Backend.**
  - BUILT: create, edit (name, status, environment, owner, property, monitors), move and delete asset; create, edit and delete route; links for *depends-on*, *runs-on*, *exposes* and the ticket link; `depends_on_this` for *Needed by*; import; create monitor; the asset type table with monograms, typed properties and *usual here*.
  - LOW: the shared layers and their kinds, plus the saved views (per-layer show, fold and group settings) and the two defaults. All are knobas-local settings: two small tables (layers with a unique name, views with their per-layer settings) and their §10.8 entries. A unique index on the lowercased, trimmed layer name enforces D14. This replaces R-4.26's map views.
  - LOW: one read for the top-bar opening that returns every asset, route, repository, pipeline, open PR and linked ticket with health and the R-4.13 relations. That is the job-map read (R-4.18) without the ticket filter.
  - NEW, needed above a few thousand items: a **summary read** with counts by layer, group and health, and a paged read of one group's items, so the view never loads the whole estate to draw folded chips. The fold rules of R-11.18 already fit this: a folded chip needs only counts.
  - UI only: the editor as a dialog (today the same edits live in the Tree's pane), lines on pointing, search, and folding.
- **R-11.24** **What it changes elsewhere.** R-4.11 to R-4.28 (Map 1) and section 4c stay as earlier drawings until D11 and D12 are ruled. R-4.21's `layer` per asset type is no longer needed (D14). R-4.27, the global map, is replaced by the top-bar opening (D13).

## 12. Inbox (`#/inbox`)

- **R-12.1** The filter: text, scope *This job / All*, chips *Review / Mention / Build / Assignment / Monitor*.
- **R-12.2** A row shows the time, the item in one sentence (tone dot for failures), the actions the op allows (*Review*, *Open log*, *Re-run*, *Ack*, *Snooze*, *Done*), and a second line with the kind and the job it belongs to, as a link. BUILT (`inbox_items`, `inbox/actions.ts`).
- **R-12.3** The snoozed items are one line under the list, with their return dates.

## 13. Time

- **R-13.1** Stopping the timer opens the worklog draft:
  - the duration, editable, with the intervals it came from;
  - the activity in those intervals as checkboxes;
  - the generated comment;
  - *Log 2h 44m to <key>* and *Discard*.

  Logging goes through the queue. BUILT (`worklog_draft`, `log_work`).
- **R-13.2** Starting a timer on another ticket while one runs opens the draft for the running one first.
- **R-13.3** *Stop, log later* stops the timer and keeps the time in the day, unlogged. *Keep running* closes the draft. The draft lists what knobas saw in the interval with its time and source (`worklog_draft` candidates), and the comment follows the ticked boxes until it is edited. BUILT.
- **R-13.4** **Time view** (`#/time`, `#/time/<date>`, `#/time/week`, `#/time/standup`), three tabs, *Log all…* in the header. Ported from `DayReview`, `WeekTimesheet` and `StandupView`; all BUILT reads and writes.
  - **Day:** five figures (tracked, logged, to log, offered by knobas, open with the app in front), a strip from 08:00 to 18:00 with a now line, then one row per block with its state and actions. Manual blocks are solid, logged ones green, the running timer yellow, passive blocks hatched (*offered*), focused time with no target dashed (*open*). A passive block offers *Add to <suggested ticket>*, *Assign…*, *Dismiss*. A label block offers *Log to <suggested ticket>* (`ad_hoc_block`). A block opens *Edit* (from, to, target, *Delete block*). A day past the observation horizon says "No record".
  - **Week:** a row per target and a column per day, Monday first, weekend shaded, minutes not rounded. Each cell shows tracked time, a bar split into logged, held and unlogged, and what is left to log. The no-target row has no bar. A held worklog gets a warn line naming why, with *Open the queue*. Day headers and cells open that day.
  - ***Log all…*** previews one worklog per day and ticket, each with a tick and an editable comment, and leaves out the running timer and the no-target time, saying so.
  - **Standup:** the digest (yesterday, today, blockers, infrastructure, every line a link, *Copy as text*) beside the protocol (who was there, notes per person, action items with *Create ticket* and the ticket's status, *Publish to Confluence* under the configured page). The infrastructure line is the LOW item of capability sheet section 9.
- **R-13.5** The job card's *Time this week* heading opens the week.

## 13a. Settings (`#/settings/<section>`, `#/settings/source/<id>`), 2026-10-15

A cog in the top bar beside the theme toggle, and `⌘,`. *Sources* in the status bar and every *Re-enter password* go to the source's page. Sections on the left, one page each:

- **R-13a.1 Sources:** a row per source with state, address, what it holds and the last sync; *Add a source* (pick the adapter, fill the generated form, *Test connection*, *Add source*); *Sync all*. BUILT (`list_sources`, `add_source`, `test_source`, `sync_all`).
- **R-13a.2 A source's page:** a credential failure on top with *Re-enter password*; Connection (address and user editable, password in the keychain with *Re-enter…*, a Kuma *Account*, what *Test connection* found); What it syncs (for Jira, projects or a JQL query; the schedule; a switch to pause); for Jira, *Boards and filters* (R-3.20, D17) with every Jira view, its ticket count, unsynced tickets and *Hide*/*Show*; *Sync runs*; Maintenance (*Backfill*, *Remove…* asking whether to keep what it synced). Editing is the BACKEND `update_source`, *Backfill* is the BACKEND `backfill_source`; the rest is BUILT.
- **R-13a.3 Checkouts and Open in…:** the clones root, the git found, and the *Open in…* applications (R-8.21 to R-8.24) with *Make default* and *Edit applications…*.
- **R-13a.4 Time:** passive blocks on or off, how long they are kept, the week's rules, where worklogs go.
- **R-13a.5 Notifications**, **Standup** (publish target), **Monitoring** (slow-above threshold, sample retention): BUILT sections, redrawn.
- **R-13a.6 Notes and capture (D10):** *Show notes and quick capture*, off; on, notes are back in search and open on their own page, and the capture shortcut works again (#1504); the shortcut, inactive while hidden, with *Change…* to set or clear it (BUILT `set_capture_shortcut`, kept from today's Capture section, #1086); the count; *Export notes as Markdown*; *Move to Confluence…*.
- **R-13a.7 Backup and export:** last backup, schedule, how many to keep, *Back up now*, *Restore from a file…*; *Share with a colleague* with ticks (links, link sets, ticket views, contexts, assets and routes, sources without passwords, smart lists; notes and time off) and *Export…*, *Import…*.
- **R-13a.8 Appearance** (theme: like the system, light, dark), **Keyboard** (the shortcuts of section 14, read-only), **Logs and database** (size, version, *Open the log folder*, *Re-index search*).

## 14. Keyboard

| Key | Where | Does |
|---|---|---|
| `⌘K` | anywhere | Search everything |
| `⌘T` | anywhere | Stop or start the timer |
| `⌘,` | anywhere | Settings |
| `⌘[` `⌘]` `⌘←` `⌘→`, the mouse's back and forward buttons | anywhere but a text field | Back and forward through the addresses (BUILT, #693; kept in the new shell, #1086) |
| `/` | any list view | Focus the page filter |
| `Esc` | anywhere | Unwinds one layer: dialog, drawer, filter text, filter focus, sub-tab, full view to job, job to tickets |
| `j` `k` `↑` `↓` | tickets | Move the selection |
| `Enter` | tickets | Open the job |
| `L` | job | Connections drawer |
| `Enter`, `Shift+Enter` | build log search | Next and previous match |
| `Y` `N` `S` | proposed links | Link, not related, decide later |

Focus is always visible (2 px outline in the focus colour). Reduced motion turns off the pulse and the drawer slide.

## 15. Writes

- **R-15.1** Every write goes through the write queue and is drawn as *queued* until the source confirms it: status, comment, worklog, branch, PR, review, merge, file commit, re-run. A write to a source whose credential fails says so in its toast ("TeamCity sign-in is failing, so it waits there"). BUILT (ADR-0012).
- **R-15.2** Links, link sets, hides, *Viewed* marks and *Done* marks on job items are knobas-local and never reach a source. Every dialog that creates one says so in one line.

## 16. Visual design

Superseded by `docs/specs/2026-10-15-ticket-first-design-system.md` (2026-10-15), which holds the full token set, the type scale, the contrast check and the porting constraints. The short version below stays for reference.

Taken from the prototypes. Tones stay `ok / warn / err / info / muted` (shell-owned). Adapters never pick colours.

| Token | Light | Dark | Use |
|---|---|---|---|
| `--paper` | `#E4E8EC` | `#12171C` | page background |
| `--sheet` | `#F7F8F9` | `#1A2027` | panels, bars |
| `--ink` | `#17212B` | `#E4E9EE` | text, primary buttons |
| `--ink-2` | `#4B5663` | `#A0ABB7` | secondary text |
| `--rule` | `#CBD2D9` | `#323C47` | borders |
| `--card` | `#F2D157` | `#D9B941` | the job card, the selected ticket, the timer, the job chip |
| `--card-ink` | `#2A230B` | `#1D1806` | text on the card |
| `ok / warn / err / info` | `#1F7444 / #8A5600 / #B0262A / #2F55D4` | `#6FCF97 / #E5B04D / #FF8A80 / #8AA6FF` | tones |

- Type: **Archivo** (variable, width axis) for the interface. **Source Serif 4** is the reading face for document bodies and titles in Docs, nowhere else. The job key is Archivo at width 62, weight 800, 64 px. **JetBrains Mono** is only for real code values: keys, shas, branch names, paths, logs and diffs.
- Yellow means "the ticket you are on". Do not use it for anything else.
- Contrast is 4.5:1 or better on every reading in both themes.
- Design width is 1440 × 900, and every view is usable from 1100 px. Below 900 px, columns stack. The board scrolls sideways, and the page never does.

## 17. Not in this round

**Removed on purpose (2026-10-14): knobas notes.** Notes are BUILT today: markdown notes with `[[refs]]`, backlinks, the Notes tile, *New note*, and quick capture from the global shortcut (`create/save/delete_note`, `CaptureWindow`, `capture_shortcut`). This UI has no place for them. Pages are the only documents, and conversations live on the ticket. Deciding what happens to existing notes and to the capture shortcut is D10.

Syntax highlighting, reviewers at PR creation, stopping a running build, a live-following log, CI adapters other than TeamCity, the estate tree and environment matrix, a redesign of Time and Standup, Sources and Settings beyond today's screens, and any write into a local checkout.

## 18. Backend work, summed up

| Area | Tag | What |
|---|---|---|
| Shell, tickets, job, filters, full views | BUILT + UI | Mostly new Svelte over existing reads. The filter bar is one component (section 2). |
| Signals, conversation *new* and *mentions* | LOW | Readers over links, declared state, activity stream; one named payload read for mentions. |
| Ticket views (R-3.3) | NEW, small | One `ticket_view` table and three IPC commands; smart lists are untouched (D6). |
| Making views from conditions (section 3a) | NEW, small | Nine grammar keys over fields already in the payload (R-3.14); sprint field discovery (LOW). |
| Jira boards and filters as views (section 3b) | NEW, medium | Agile REST reads, one keys-only search per view per sync, one migration, a live suite against testenv Jira (R-3.26). No `WriteOp`. |
| Link sets | NEW, small | One migration (4 tables), about 6 IPC commands, a read-time union term, one ADR (D3). No adapter, no `WriteOp`. |
| Pattern name globs | LOW | One grammar addition. |
| Uncommitted diff | LOW | ADR-0021 amendment (D5). |
| *Viewed* per PR file | LOW | knobas-local state. |
| File edit through Gitea | NEW, one op | `CommitFile` `WriteOp`, §10.8 entry, live Gitea test, held-write path on `sha` mismatch (D4). |
| Clone into the clones root | NEW, small | `clone_repo` with progress and cancel, the ADR-0016 amendment, a live test against testenv Gitea (D16, R-8.25). |
| Open in… with your own applications | LOW | The two command templates become a named list with a default and a file command; `open_checkout` takes a template id, file and line (R-8.21 to R-8.24). |
| Everything else in section 8 | BUILT | Branch create and delete, PR create, review, merge, update, compare, files, commits. |
| Builds frame over a generic pipeline and run | NEW, small | One ADR next to ADR-0020 (D7). The declared fields of R-9.12 for TeamCity, most of which the adapter already reads. |
| Whole log with step boundaries | LOW | Stream `downloadBuildLog.html` with a cap, in place of the tail. |
| Artifact folders, preview, downloads through knobas | LOW | The artifact children endpoint, plus one IPC command that streams to `~/Downloads` (D8). |
| Run options (clean checkout, first in queue), parameter specs | LOW | Two optional fields on `run_build` (§10.8), and a parse of the parameter spec. |
| Nested project tree, `project_ids` including subprojects | LOW | `/app/rest/projects` with `parentProjectId`, a declared project path (R-9.4b). |
| Build chains | NEW, small | Read snapshot dependencies per configuration and per build (R-9.13). |
| *This job* pipelines | LOW | A reader: runs on the job's branches, PR checks, link sets. |
| Assets and Monitors as two views, two job sections, two link-set kinds | BUILT + UI | The data already is two things (`asset`, monitors with `attached assets`); this only stops the UI from merging them. Link-set members gain an `assets` kind (in the R-7.11 migration). |
| The job's Map | LOW | One read for nodes and edges (membership plus known relations), its reverse for *who else uses this*; chains from R-9.13. |
| Dependencies view (section 11d) | LOW, one NEW at scale | Saved views and defaults (one knobas-local table); one read for everything; a summary read with paged groups once an estate passes a few thousand items. Asset and route create, edit, move and delete are BUILT. |
| Other map drawings (Maps 2 to 7) | UI, plus one NEW for Map 7 | The same read walked in the UI. Map 7 also wants the image tag a container runs (R-4.36). |
| Pins and presets | LOW | knobas-local state per pipeline. |
| Build history beyond the sync budget | LOW | Fetch older runs on scroll. |
| Run, re-run, parameters, tests, changes, top-level artifacts | BUILT | `RunBuild`, `RerunBuild`, `TriggerBuild`, `SetParameter`, the build view tabs. |
| Docs full-text search with snippets | BUILT | Bodies and comments are in `body_text`; `websearch_to_tsquery` and `ts_headline` already run. |
| Snippet section, *Found in* facet, `space:` alias | LOW | Keep the section heading beside the body text; one facet; one grammar alias. |
| Storage macros as blocks (panels, code, tables, Jira issue, mention) | LOW | A stated subset of the storage format; everything else is the honest placeholder. |
| Backlinks | LOW | Extract page links from the storage body at sync time. |
| Export as Markdown | LOW | The block-to-Markdown converter, shared with a later Markdown source. |
| Version compare, attachments, blog posts | LATER | Not synced today. |
| A Markdown docs source | LATER | A folder of `.md` in Gitea first (reuses `CommitFile`), or a wiki API (D9). |
