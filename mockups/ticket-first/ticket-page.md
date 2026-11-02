# The ticket page and the job, one page

`ticket-page.html` beside this file. 2026-11-01, five passes in one day with Björn; this file keeps
the result and a short record of what was tried. The mockup links the app's own stylesheets
(`app/src/lib/tf/css/`) and the vendored fonts, so what it shows is what the port draws.

**Shipped.** Spec #2396 built this page (integration branch `spec-2396`, PR #2409). The page is
`app/src/lib/tf/job/TfJob.svelte` with its card `TfJobCard.svelte`, styled in `tf/css/job.css`;
`?fake-ipc` draws it at `#/ticket/<id>` (PAY-231 has every row). Since then this mockup is a
record: where the app and the mockup differ, the app and the docs decide (CONTEXT.md, **Ticket
page** and **Job**; the design system's Controls, Colour and Layout; the shell conventions'
address lines; `docs/decisions/2026-11-02-ticket-page-and-job-one-page.md`). Known differences:
the reporter is the source's username, as the assignee is (the fake keeps *Lena Hoffmann*); an
unassigned ticket's Assignee row reads *Unassigned* rather than going; *Try again* is a small
button, not a link; the width handle also takes ←/→ (16 px) and Enter to fit.

## What was wrong

The ticket page (#2332, spec #2227) put the Tickets preview's component on a 640px yellow card left
of an empty half window, at the preview's 20px title, with the Markdown reader's padding inside the
card, and without what the detail already holds (type, estimate, labels, the source's links, the
thread, the web address). Beside it, the job page carried a tab strip, an *Also on this job* bar, a
*12 connected* control, a signals line per panel and five identical panels in a grid.

## The design

One page for a ticket and the job, `#/ticket/<entity id>`. It is the job's two columns (`.tf-ws`,
job.css). What changes between a ticket you read and the ticket you are on is the card's mood.

**The card** (`.tf-jobcard.tf-ticketcard`, 320px, scrolls on its own):

- The card is the same in both moods. At its top the key at `--t-d2` with a copy button beside
  it; the title is the reading column's heading, not the card's. No
  dates: *created* and *updated* are not worth a line (Björn), and the thread carries the dates
  that matter. Under the title the status as the card's chip, the one fact that is a state. Then every
  other fact in one facts list (`dl.tf-facts`, the Tickets pane's), in Björn's order: Epic as a
  chip that links to its page, when the ticket has one; Reporter and Assignee, each an avatar
  and a name and nothing else; Type; Priority; Labels; Logged with its meter. The source is the *Open in <source>* button's;
  a key two sources hold gets its *also in* line under the key as the job card draws it. Then
  the links the source declared (*Blocks PAY-228 …*), the sections table, and at the foot one
  action. (The bar's *Facts: status line* keeps the status chip and
  *Story, High priority* as a line under the title instead of three rows, for comparison; the
  one list is the recommendation, since it is one shape from the title to the table.)
- A ticket you are reading sits on a sheet (`.tf-sheetcard`); its action is the decision, *Start
  work* (To Do, yours), *Resume work* (yours, in progress) or *Work on this* (anyone's), each of
  which pins the ticket. The button alone, no line under it; what it does is its tooltip. A done
  ticket has no button.
- The ticket you are on sits on the yellow, and that is the only difference; its action is the
  timer, *Stop and log* or *Start timer*, plain: the running time is the top bar's chip, which
  the card does not repeat. The nav has no Job entry any more: Tickets is the one entry, home
  (`#/job`) draws the pinned ticket's page, and the timer chip, the Tickets view and home all
  reach it. No hole, no 64px key, no status select, no *Time this
  week* (Time has it); a status move is the view's or the source's.

So yellow means exactly what the design system says, and nothing else gets it.

**The copy button** beside the key (`.tf-iconbtn.tf-sm`): one press puts the key and the title
on the clipboard, *PAY-231 Retry failed SEPA payouts*, for a commit, a branch or a chat; the
button turns `--ok` for a moment and the app's toast says *Copied*. **Long text on the card**: the
epic chip ends in an ellipsis with the whole name as its tooltip; nothing else on the card can
run long now that the title has left it. **The card's width is
yours**: a 6px handle on its right edge (`.tf-tkhandle`, a `separator`) drags it between 240px
and 720px, kept in browser storage (`knobas.ticket.side`), and a double click on the handle fits
it so nothing on the card wraps, up to the cap. The column is `--tk-side` on `.tf-ws`; below
900px the columns stack and the handle is gone.

**The rhythm.** Both columns start 24px under the breadcrumb. On the card: the key line, the
status chip close under it as one block, then 16px between the blocks (list, table, action). In the reading column: the title, 16px, the description; 28px between sections;
10px under a section heading; the thread's own 10px between comments.

**The card's two actions** sit under the sections table in the table's own shape, a second
bordered list of two rows with a chevron each: *Open in <source>* and *Link something to it*,
which opens the drawer. One box of places to go, one box of things to do, the same rows.

**The sections on the card** (`.tf-tktable`): the preview's bordered list, one row a link with a
chevron at its end. The name, the count in tabular figures, and at most one state with its dot,
the worst one, only where something asks for you (*Code 6 · 3 not pushed*, *Builds 7 · 1
failing*, *Docs 1*). The rollup's full words (`buildsRollup` and its siblings in job.ts) are the
row's tooltip. The row opens the section's view with *This job* set on its filter bar, which is
what the job page's *Open Code view* already does (`sessionFilter("code-repos").scope = "job"`,
then the view's address). A ticket with sub-tasks leads the list with *Sub-tasks 3 · 1 open*,
opening Tickets narrowed to them (`fields.parent` is synced, so the mirror knows them). Nothing
reached draws *Nothing connected yet.* (The same rows as plain lines and as tiles are in the
mockup bar for comparison.)

**Two facts the mirror already holds and the page did not show**, as the preview's facts list
(`dl.tf-facts`) so the labels line up: *Reported · Lena Hoffmann, 2026-08-12* with the reporter's
avatar as the assignee row has it (Jira's `fields.reporter`, who filed the ticket, and
`fields.created`, the date; both synced, neither projected by the descriptor yet, so the port adds
the two payload paths beside `assignee`'s) and *Logged · 2h 10m of 6h* (`timespent` against `timeoriginalestimate`), with a thin
meter under it in the week chart's language: logged in the darker fill, the rest of the estimate
in the track, the track `--err-bg` once logged passes the estimate. The row's rules: no estimate, the time alone and no meter
(*3h 40m*); an estimate and nothing logged, *0m of 2h* over an empty track; neither, no row. The
list is a 68px label column, 8px between rows, every row 20px tall whether it holds text, a chip
or an avatar, so the rhythm holds. With the *Logged by you*
switch the line reads *2h 10m of 6h, 1h 40m yours* and the meter splits: yours in full ink,
others' in the lighter fill. Yours comes from the synced `worklog` by author; the split is worth
it on a shared ticket and says nothing on one only you work on, so it could draw only when
somebody else has logged. The facts line is type and priority; the estimate lives on the logged
line.

**The body** (`.tf-work > .tf-wbody > .tf-tkread`, 880px, centred in the space beside the card;
the design system's left alignment stays everywhere else): the ticket's title as the page's
heading, whole, at the page-title step (`--t-3xl`, `wdth` 85, as `.tf-fhead` sets it), alone on its line; a long title wraps. The page's two actions live on the card instead (below),
and Refresh has no button: it is the keybind's. Then the description through `TfMarkdown` in a
resting panel at body size, filling the panel's width, with no heading of its own, the title
being it; then *Conversation*, the whole thread in the job's own rows
(`.tf-convo`, `.tf-msg`), no fold: the body is its own scroller, so a long thread costs a scroll
and nothing else. The job's *New since your last visit* line (`visitEntity`) sits where it falls,
and *mentions you* is where the thread says so. The heading is the word alone, no counts. The compose box
ends the page, one line until it is used.

**Attachments** (Björn: the list in the mirror, the bytes not): `attachment` joins the Jira sync's
`fields=` list and rides along in the payload like the rest, so every ticket's attachments are
known offline: name, size, author, date, the content and thumbnail URLs. Nothing else is stored.
The page draws them in two places:

- **Where the wiki put one.** `!name.png|thumbnail!` in the description becomes a figure
  (`.tf-tkfig`): the image at most the panel's width, nothing under it; the name is its `alt`. The bytes come
  live through the Rust side with the source's credential, since the webview's `default-src
  'self'` loads nothing from Jira, and are kept for the session, not the mirror.
- **An Attachments panel** between the description and the thread, folded by default: a
  `<details>` whose summary is the section heading and a count badge, *Attachments 4*, with a
  chevron after them (the panel header's open-link stroke) pointing right while it is closed and
  turning down when it is open, so a ticket with forty screenshots costs one line. Open, it is one
  row an attachment, the job's `.tf-r`, with a 56×40 preview for an image or the file's kind in mono (*PDF*, *CSV*), the
  name, who and when and how big, and two small ghost buttons: *Open*, which hands the URL to the
  browser (which has the Jira session), and *Download*, which saves through the credential
  (`DownloadTarget`).
- **When Jira cannot be reached**, the panel leads with the job's stale strip (`.tf-stale`):
  *Tidewater Jira cannot be reached, so previews and downloads wait. The names and links are the
  mirror's.* with *Try again*; image rows draw a dashed placeholder in the preview's place,
  *Download* is disabled, *Open* still works, and an inline figure is the Markdown component's own
  placeholder (`.tf-md-noimg`): *psp-409-trace.png, not reachable in Tidewater Jira right now.
  Open in Tidewater Jira.* A 403 reads the same with *not permitted*; a file over a size cap gets
  no preview and the two buttons.

**What Jira has that the page cannot draw** (the adapter asks `fields=` for eighteen names,
`sync.rs`; the wiki converter is `render/prose.ts`):

- **Inline images** need the converter rule for `!name.png|thumbnail!` above; until then the
  reference stays as text.
- **Check boxes.** Data Center's wiki has none; teams write `(/)` and `(x)`, which stay as text.
  The Markdown component draws `- [ ]` task lists already, so a converter rule for the two
  emoticons is cheap.
- **Colour and panels.** Dropped to plain text on purpose.
- **Not synced at all**: components, fix versions, due date, watchers, votes, the change history.
  Each is a `fields=` name away, and then a line on the card if it earns one.
- **Mentions** draw as `@account`, not the person's display name, since the mirror holds accounts.

**States**: a skeleton while the ticket is read, *The ticket has no description.*, *No comments
yet.*, *Nothing connected yet.*, and the existing not-found and failure empties. Below 900px the
columns stack as the job's do.

**Two rules for the yellow card**: it is a light surface in both themes, so its dots and its
toned chips take the light tone values (dark's `--warn` is the card's own yellow, its `--err` pink
on it; the job card's red chip has this problem today). And its primary button is the card's own
ink-on-yellow, as the clock's button is.

## What went, and why

- **The tab strip.** Map is Dependencies' *This job* opening under its own nav entry; the
  Conversation is on the page; one tab left is no strip.
- ***Also on this job*, the Connections count, the signals lines, the per-panel filter boxes.**
  Link sets and links are how rows arrive, not something the reader acts on. Managing them is the
  one *Link…* button, which opens the drawer; a proposed link is a chip on the row it concerns in
  the view.
- **The section panels in the body.** Their rows live in the views, which have the room and the
  filter bar. The card's lines say whether the click is worth it.
- ***Around this ticket* as counts without links.** The ruling (CONTEXT.md, **Ticket page**) existed
  because opening a section meant opening the job. Views scoped to a ticket remove the reason.

Tried and dropped the same day, kept out of the mockup: the sections as five section panels in a
grid beside the thread; as a sentence per row on the card; as name-and-count badges only; the card
as a header band across the page; as a small floating index card; as a right-hand rail. The band
and the rail gave up the card; the badges said too little; the sentences and the panels too much.

## Porting it

**CSS**: tickets.css drops its two `.tf-ticketpage` / `.tf-ticketcard` rules and takes the block
headed *Proposed additions* in `ticket-page.html`, minus the one line marked *Mockup only*. Every
other class is in the sheets already: `.tf-ws`, `.tf-jobcard`, `.tf-jobkey`, `.tf-jobtitle`,
`.tf-jobrow`, `.tf-jobdim`, `.tf-av`, `.tf-lab`, `.tf-dot`, `.tf-panel`,
`.tf-convo`, `.tf-msg`, `.tf-who`, `.tf-chip-mention`, `.tf-fold`, `.tf-compose`, `.tf-skel-lines`,
`.tf-btn` and its sizes.

**Component**: `TfJob.svelte` gains the unpinned mood and draws for `#/ticket/<id>`; `#/job/<id>`
pins, then goes there; `#/job` draws the pinned ticket. `TfTicket.svelte` and `TicketPreview`'s
`page` prop go; `TicketPreview` keeps the Tickets pane. The two share `ticketFacts`,
`previewAction`, `epicLine` and the rollups.

Reads, by part:

| Part | From |
|---|---|
| Key, title, status, type, priority, estimate | `detail.row`, `detail.status`, `factsLine(detail)` (job.ts) |
| Who, *(you)* | `detail.author`, `isMine(detail, sources)`, `initials()` (tickets.ts) |
| Epic line and its link | `parentOf(detail.row)` (sets/apply.ts), `entityAddress("ticket", parent)` |
| Labels | `detail.payload` as `mini_board` reads them, or a `labels` field on the detail |
| Source, updated | `detail.source.display_name`, `clockLabel(detail.row.updated_at)` |
| Linked | `detail.links` (relation, `other.title`), as `blockers()` reads them |
| The sections | the job's section reads and `buildsRollup`, `monitorsRollup`, … (job.ts); the worst signal's tone and text |
| Decision | `previewAction(card, timerOnIt)` to `jobHash` |
| Description | `proseMarkdown(detail.description, detail.markup)` into `TfMarkdown` |
| Open in <source> | `detail.web_url`, as `TfItem` draws it |
| Conversation | `ports.entityThread(id)`, `summary()` and `mentionCount()` (job/conversation.ts) |

**What the merge decides, for a spec**: one address (`#/ticket/<id>` draws the page, `#/job/<id>`
pins then goes there, `#/job` draws the pinned ticket); whether writes (status, comment, worklog)
are the pin's or any ticket's (my recommendation: any ticket's, since the write queue is per
entity, with the timer, *Start work*, home, *Link to* and the promoted context the pin's); and
that reading never pins. It reverses the two-page split of spec #2227 (#2332), so it is spec work:
grilling, then to-spec.

## Settled with Björn, 2026-11-01 (grilling, for to-spec)

One spec on an integration branch, nine sections: routing and the pin; the card; the reading
column; retiring the job page's extras; the adapter fields and the one-time Jira re-sync; the
attachment commands and panel; the converter rules; the width handle and copy button; the docs
and the design-system amendment.

- **Address.** `#/ticket/<id>` draws the page. `#/job/<id>` pins, then goes there. `#/job` draws
  the pinned ticket or the empty state. The Job nav entry goes. Esc as the ticket page's today;
  from home, nothing. The name *chosen job* stays in code and docs.
- **Pinning.** Reading never pins. *Start work* pins and runs the stepper at the top of the
  reading column; *Resume work* pins and starts the timer; *Work on this* pins. Pinned, the foot
  is *Stop and log* or *Start timer*. A done ticket has no button.
- **Writes.** Comments and links on any ticket. Worklogs the pin's. The status write leaves the
  page. Every open records a visit (*New since your last visit*).
- **Retired.** The tab strip, Map, the Conversation tab, *Also on this job*, the Connections
  count, the section panels with their signals and filters, *Time this week*, the hole. The
  drawer stays behind *Link something to it*; Dependencies keeps *This job*.
- **Rows.** Sections take the worst signal of the existing rollups; Code gets a rollup (open
  pull requests, branches with unpushed commits). Sub-tasks sets the Tickets search line to
  `under:<key>` and navigates. The card's width lives in browser storage.
- **Jira.** `attachment` joins the field list; one full re-sync when it lands. Reporter, created
  and time spent become typed detail fields with descriptor paths, additive, announced in the
  contract's ledger; Gitea maps what it has; an empty row is not drawn. *Logged by you* is the
  worklog by the source's username, drawn only when somebody else has logged. Previews through
  the credential, 5 MB cap, kept for the session; downloads to the OS Downloads folder as CI
  runs' are; *Open* through the browser. Converter rules for `!name.png|thumbnail!` and `(/)`,
  `(x)`. The dev fake's PAY-231 gets all of it.
- **Board and list.** Yellow is the pinned ticket's card and row, selected or not; selection is
  a two-pixel ink outline, and the lift (shadow) goes with selection. Elsewhere unchanged. The
  design system's first rule becomes: *Yellow means the ticket you are on: its page card, its
  card and row on the Tickets board and list, and the running timer. Nothing else gets yellow.
  Selection is an outline.* Plus: the yellow card is a light surface in both themes, so its
  dots and chips take the light tone values (fixes the job card's red chip in dark).
- **Out of scope.** The Tickets pane preview keeps its current drawing; lazy loading.
