# Ticket-first shell: conventions so far

How code for the new shell (the TF milestones) is written, as the first tickets already did it. This file records conventions only. It decides nothing new, and the [design system](2026-10-15-ticket-first-design-system.md), the ADRs and spec #808 win wherever they differ. Each line names where it was ruled: the entry in `docs/decisions/2026-10-unattended-rulings.md` and the PR. A PR that departs from a line says why in its Rulings.

Not yet recorded: the conventions of #826 (PR #899 was open when this was written).

## Styles and fonts

- Every class in `tf.css` starts with `tf-`, and `tf-css.test.ts` holds it. `app.css` styles bare names globally (`.btn`, `.toast`, `.timer`) and those would leak. (#809, PR #885)
- Tokens live on the new shell's root, not on `:root`, and the shell has its own stylesheet. (Design system, "Getting it into the app")
- Durations and clocks are Archivo with tabular figures, not mono; mono is for code values. Where the design system and prototype-6 differ, the design system decides. (#809, PR #885; reused by #816, PR #898)
- A dialog of its own width passes `className` to `tf/settings/TfDialog.svelte` (the view editor's `tf-vw-dialog`), never a second dialog shell. (#814)
- A selected card is `aria-current="true"` with the `tf-card` class, so the focus ring follows the design system's `--card-ink` rule. Not `aria-selected` on a button. (#810, PR #895)
- A component that draws something else on a narrow window reads the stylesheet's own query: one exported constant (the Docs reader's `READER_NARROW`) handed to Svelte's `MediaQuery`, the same text in the `@media` rule, and a `tf-css.test.ts` check that the rule exists. Written as `max-width`, so the test setup's `matchMedia` stub (every query false) draws the wide layout. (#2081)
- A sticky side column (a tree, an outline, a rail, a file list) is no taller than the window and scrolls on its own: `max-height: calc(100vh - <px>)` with `overflow: auto` (or `overflow-y`), as `.tf-ftree` first did. `tf-css.test.ts` checks every `position: sticky` rule except the one-row bars it names in `STICKY_BARS`; a new bar joins that list. (#2091)
- A drawer (the Connections drawer, the Docs tree below 900px) registers as a `drawer` on the shell's `layers` while open, so `Esc` closes it wherever focus is, and focus goes back to what opened it when it closes. (#820, #2084)

## Mounting and the shell switch

- The switch between the rooms UI and the new shell is `localStorage` `knobas.shell`, read at startup; switching reloads. A dev build also takes `?shell=tickets`. No command, so a share export does not carry it. (#809, PR #885)
- Rooms-styled dialogs mounted outside `.tf` and kept their own tokens until a ticket redrew each. (#809, PR #885) All are ported now: the worklog draft (#826), the timer picker and the ad-hoc dialog (#1432) and the ⌘K launcher (#1422) mount in `TfShell`'s `dialogs`, as the write queue (#1425) does. Only the shell's `Modal` still draws with `unported.css`'s `.scrim`/`.dlg`.
- Under the new shell `Esc` on a full view with no job goes to Tickets. (#809, PR #885)
- A setting the shell and a page both change (the colour theme) is one store under `tf/`, passed to pages on the shell's context (`TfPageContext`), so a test can hand both the same instance. (#831)

## Routes and addresses

- A route not built yet draws an empty page that says so (the Job page before #816; the Map and Conversation tabs "not built yet", until #874 and #819). (#810, PR #895; #816, PR #898)
- The job's address is `#/job/<entity id>` (`#/job/jira%3APAY-231`); an ad-hoc context is `#/job/<ctx id>`. A bare `#/job/<key>`, typed or bookmarked, is read under every source, the first ticket winning. (#810, PR #895; #816, PR #898; #1476)
- Every link the shell builds to a job is `jobHash(<entity id>)` or goes through `itemHash`/`entityAddress`, never the bare key, so a key two sources hold opens the ticket it was drawn from. The one exception hands an address back rather than building one: the breadcrumb's job chip and the top bar's Job entry go to the segment the job was opened at (`jobAt`). (#1676, #1977)
- A ticket view's address is `#/tickets/<id>`, the id a slug of its name made when it is first saved and kept through a rename; `#/tickets` is *All tickets*. (#812)
- The Time view's addresses are `#/time` and `#/time/<date>` for the Day, and `#/time/week` and `#/time/week/<date>` for the Week containing that date, and `#/time/standup` and `#/time/standup/<date>` for that day's Standup, which is where a protocol note opens. (#827; #828; #1504)
- A Code row opens `#/pr/<entity id>`, `#/repo/<entity id>`, `#/branch/<entity id>` or `#/commit/<entity id>`. The address carries the entity id, because a pull request's number is unique only inside its repository. `branch` and `commit` light the Code nav, and their page puts the repository page in their place: a branch its Branches tab (`#/repo/<repo entity id>/branches`), a commit `#/repo/<repo entity id>/commits/<commit entity id>`, the repository read from the entity's declared parent. (#816, PR #898; #1440)
- Monitors' addresses are `#/monitors` (*Monitors*) and `#/monitors/unmonitored` (*Assets nothing watches*), two tabs of one page. (#864)
- Code's addresses are `#/code` and `#/code/repos` (*Repositories*) and `#/code/prs` (*Pull requests*); *New pull request* opens `#/repo/<repo entity id>/compare/<branch>`. (#837)
- The repository page is `#/repo/<repo entity id>/<tab>` (`files`, `commits`, `branches`, `prs`, `local`); `files/<path>` opens a file, its path encoded segment by segment so its slashes stay, and `commits/<commit entity id>` a commit's diff under the Commits tab. The tab row is `REPO_TABS` in `tf/repo/repo.ts`. (#838)
- The pull request page is `#/pr/<entity id>` (Overview) and `#/pr/<entity id>/<tab>` (`conversation`, `diff`, `commits`); the tab row is `PR_TABS` in `tf/pr/pr.ts`, and every tab draws from the pull request's one whole `entity_view`. (#841)
- A Builds row opens `#/build/<entity id>`, a Pages row `#/page/<entity id>`, an Assets row `#/asset/<asset id>`, a Monitors row `#/monitors`. (#817)
- Under Dependencies, `#/deps` (and the retired `#/map`) draws Dependencies, and `#/assets`, `#/asset/<id>` and `#/route/<id>` draw the Asset tree, each matching on `head`. (#871)
- `#/deps/job/<key>` and the job's Map tab (`#/job/<key>/map`) draw Dependencies from that ticket: the `dependencies` read kept to what `job_reach` and `job_pipelines` answer (`deps/reach.ts`), in the view that opens from a ticket. While a job is open, *This job <key>* / *Everything* switches openings; on the Map tab *This job* stays on the tab. (#874)
- Dependencies opens in a saved view (`dep_views`): the top bar's default, its draft held for the session in `tf/deps/views.svelte.ts`; the reader's gestures (Find, the trace, folds opened by hand) hold for the page. (#873)
- A source's settings page is `#/settings/source/<id>` (`sourceHash` in `tf/settings/settings.ts`); `#/settings/source/<id>/add/<list field>/<value>` is that page with the value added to a list field of its config, unsaved, for the reader to save (a Jira view's *Sync WEB too*). (#879)
- An item whose kind no surface draws (a custom role's, ADR-0029) opens `#/item/<entity id>` (`tf/item/`), which draws the source's block view under the shell's breadcrumb (`tf/TfCrumbs.svelte`, named by the kind's label, #1408), draws the view's actions as its toolbar and an `actions` block in place through `tf/TfEntityAction.svelte` (#1407), and lights no nav entry; `itemHash` sends every such kind there. (#1201)
- A note opens `#/note/<note id>` (`tf/note/`), a read-only page that lights no nav entry. While notes are hidden (D10) the page replaces its address with `#/settings/notes`, and a note titled by the standup protocol rule (`protocolDayOf` in `standup/protocol.ts`) opens `#/time/standup/<date>` instead, through `itemHash` when the producer holds the title and through the page when it holds only the id. (#1504)
- Link sets' addresses are `#/sets` (the list) and `#/sets/<set id>` (its editor), the id a slug of the name the set was first saved under, kept through a rename, as a ticket view's. (#866)
- Every job section is `tf/job/TfSection.svelte` reading `list_entities` with `reach: <ticket id>`; a row from the parent (or, from TF5, a link set) carries a `.tf-via` label naming it ("Parent PAY-231"). (#817) A row from a link set is labelled with the set's name, through `originLabel` (`tf/job/job.ts`). (#866)

## Data, fixtures and screenshots

- A list of items from everywhere names each item's job as its own ticket, else the first open job (a live ticket context, the current one first) whose `job_reach` holds it; nothing reaches it, *no job*. (#834, PR #958)
- Which job a row belongs to is `tf/job/open-jobs.ts` (`readOpenJobs`, `jobIndex`), one read for every list that names a job. (#837)
- The shell reads declared paths only, never a payload (ADR-0007). A field a card needs becomes a declared path, not a payload read. (#810, PR #895; #816, PR #898)
- A surface drawn for a role draws the rows of the sources that claim it, through `tf/roles.svelte.ts` (`sourceRoles.keep(<role>, rows)`), which the shell fills from every `list_sources` answer; never by adapter kind or by which kinds a source has. Until the list is read, and for a source it does not name, nothing is narrowed. (#1196)
- A surface whose read the server counts (Tickets' board) narrows the read, not the rows: it passes `sourceRoles.scope(<role>)` as the read's sources, which is `null` (read every source) while the list is unread or every listed source claims the role, and reads nothing when no listed source does, because an empty `sources` means every source to the read. Once it narrows, a source the list does not name drops too: the read takes an inclusion list only. (#1200)
- A source fills a role when it claims it and the list carries no `role_misses` entry for it (`fills` in `tf/roles.svelte.ts`); a surface draws only fillers, and `scope` asks for fillers only. The shell reads the list again when a run finishes, and Settings › Sources names each miss's role and path on the source's row and page. (#1199)
- A read of a frame's kinds (Builds' runs) asks for the kinds the adapters claiming the role declare the frame on, `kindRegistry.frameKinds(<role>, <frame>)`, never a kind spelled in the shell. While the registry is unread the job's Builds section reads `build`, as before; with no claimed run kind it reads nothing, because an empty `kinds` means every kind. The job's sections and *Around this ticket* keep their rows to the role's sources as the surfaces do. (#1230)
- A `?fake-ipc` feature module that emits an event (a clone's `clone:progress`) exports `attach(bridge)`, which the installer calls with the installed bridge. (#846)
- A `?fake-ipc` feature that adds rows to another feature's read registers a function in an array that module exports (`SET_REACH` in `fake/job-reach.ts`), so the read's module imports nothing from it and no import cycle runs through `corpus.ts`. (#866)
- `?fake-ipc` fixtures grow with the ticket that needs them: enough rows to compare with prototype-6 (eleven more PAY tickets in #810; a small link graph and the workflow in #816). Where the fixture diverges from the mock adapter, the PR says so. (PR #895; PR #898)
- The `?fake-ipc` fixture is one module per feature under `app/src/lib/shell/dev/fake/`, found by a glob; a ticket adds or edits its feature's module only, except that a row a command returns goes in the module that answers the command (the Week's refused worklog in `fake/write-queue.ts`, #828). A new screen registers itself in its folder's `page.ts` (`definePage`, `tf/page-entry.ts`) and does not edit `TfPage.svelte`. (#908, PR #916)
- A `?fake-ipc` row's state chip, path and source ride in its payload as `chip`, `path` and `source`, which `mirrorRow` reads; the mock adapter declares none of them. (#817)
- A repository's files, branches, commits and a commit's diff are read through its source's view (`entity_view` sections), so an uncloned repository reads like a cloned one; a diff is drawn by `tf/code/TfDiff.svelte`. (#838)
- A page that reads a source's view section by block shape draws a muted line where the source put it, never as the list, check, commit or message it reads. A muted line is an untitled `entities` block whose every row is muted with no entity and no chip, as Gitea's `view::muted_line` writes one (comments or checks the token could not read). A list's own last note stays in its list, and *No comments yet.* or *No commits.* is not drawn beside a line. `tf/pr/pr.ts`'s `linesOf` reads it. A page reading a section's list takes every `entities` block in order (`tf/repo/repo.ts`'s `viewRows`, the pull request's *Commits*), never the first whatever it holds; the pull request's checks are its overview's first list that is not a line. (#2065)
- What Code draws beside a mirrored row (an author, *Mine*, a repository's teams and language) is `code_facts`, asked once for the rows a list holds; a view row with no entity id has none, and a *Mine* chip leaves it out. (#966, #973, #986)
- A pull request's toolbar write is drawn only where its view offers it, and opens `tf/TfEntityAction.svelte` (lifted from the pull request page by #1407, so an item's source actions share it), which draws the action's declared fields and sentence and queues what `detail/view/actions.ts` builds; a write whose dialog is a later ticket's is drawn `aria-disabled` with *Not built in the new shell yet*. (#841)
- A pull request's review and merge dialogs (`tf/pr/TfPrReview.svelte`, `TfPrMerge.svelte`) draw the choices the source's actions declare: a verdict per `submit_review` action, the merge's `style` options, flag and message. The shell gives an SPI value its own words (`merge` is *Create a merge commit*), and the payload is still what `detail/view/actions.ts` builds. (#842, #843)
- Knobas-local state that holds only for one head of a pull request (*Viewed*) is a `knobas.setting` row keyed by entity id, storing the head sha the mirror declares (`KindPaths.commit`). Its commands read that head themselves, so a push resets the state. (#842)
- Whether a row offers a write (a branch's *Open PR*, *Delete…*) is its view's toolbar action: offered and enabled, or drawn disabled with the source's reason as its title; never a reading of a fact. (#839)
- A source's markup reaches the shell only as a declared block list (`knobas_source::docs`, ADR-0022): the adapter projects its mirrored payload with a plain function, and the shell draws the blocks as text, never as markup. (#859)
- Screenshots under `?fake-ipc` are compared with prototype-6 at the same route, theme and 1440 x 900. (Design system, intro)
- A read whose answer is sentences (a proposal's reason and evidence) is drawn as the backend wrote it; the view adds prototype-6's headings and no words of its own, and leaves out a heading with nothing under it. (#836)
- What a docs search needs beyond the words (a section's heading, a comment's author, the pages a
  body links to) rides in `body_text` as marked lines the adapter writes at sync
  (`knobas_source::docs::indexed`); the shell never reads the payload for it. (#860, #861)
- A Docs write in the source's markup (a section edit, a new page's body) is the adapter's
  (`DocWrites` in the registry); the shell sends plain text. (#862)
- A form generated from an adapter's `config_schema` is `tf/settings/TfSchemaForm.svelte` (settings rows or a dialog grid) over `sources/schema-form`. A source's config is saved as the stored config read again at the press with only the reader's changes over it (`tf/settings/source-config.ts`), never a copy read when the page opened: the sync writes into it too. (#903, #904, #1021, PR #1033)
- Anything exported as Markdown goes through the one converter, `knobas_source::docs::markdown`, which reads the Docs frame's own `DocBlock`s (one vocabulary, #952); a link to another page is `page_file_name(title)`, the name the exporter writes. (#832)

## Copy and display

- Copy follows prototype-6 word for word unless the PR says why. A source's words are not in the design: nothing is invented for a field no source declares (the card's comment count waited for #894). (Design system, "Words"; #810, PR #895)
- A count badge is drawn only after its read has answered and only above zero. (#809, PR #885)
- An unreachable source is plain text in the status bar; only a credential failure gets the err tone and *Re-enter password*. (#809, PR #885)
- An estimate reads in hours and minutes (`est. 6h`), never days. (#816, PR #898)
- An instant read against now, a length, an age and an entity's key are spelled by `lib/time/format.ts`, never in place: an instant `clockLabel` (`today 11:48`, `yesterday 16:05`, `2026-08-18`), a line saying when something was written or taken `stampLabel` (an older day keeps its clock, `2026-08-12 09:14`: a comment, the last backup, a sync run), a timesheet length `minutesLabel` (`2h 15m`), a run's length `secondsLabel` (`3 m 12 s`), an age `ageLabel` (`38 min`, as in *data is 38 min old* and *synced 4 min ago*), an entity's key `keyOf`. On a clock or a stamp an older day is the ISO date, never a month name; a bare date (a sprint's end, the notes line) is not a clock. A timesheet block's clock within its day is `time/day.ts`'s `clockReading`. A page that needs another form adds a named variant there. (#1678; #1986)
- Initials come from the account's spelling (`mara.lindqvist` is ML). (#810, PR #895)
- A sentence built from a backend reason capitalises only knobas' own opening word; a source's word (an account, an asset path) keeps the source's spelling. (#834, PR #958)

## Tickets

- The *Board / List*, *Lanes* and list sort hold for the session as the page's module `$state`, and are never written to storage or a command; a restart forgets them. They are kept per ticket view, over what that view saved. (#813; #812)
- A list header that sorts is a `th` carrying `aria-sort` with a button inside; CSS draws ▴ or ▾ on the sorted one. A column that does not sort is a plain `th`. (#813)
- A signal is drawn through `tickets/Signals.svelte`: a `tf-dot` in its tone, then its words. Cards and list rows draw the read's signals and nothing else. (#815)
- A saved view's words name its ticket system, `saved_view_source.system` (*Jira decides which tickets*, *Open in Jira*), never the word Jira; a group is the source's when one of its views has a `saved_view_source`, never by its name; Settings' *Boards and filters* names the adapter's declaration (`savedViewSystem`). `lib/shell/house-rules.test.ts` keeps Jira out of what `tf/tickets/` and `TfBoardsAndFilters.svelte` draw. (#2032)
- A control that needs a role's optional read (*Make an editable copy* needs `translate_query`) is drawn only where the source's claim names that read (`sourceRoles.answers`); while the claims are unread it is not drawn. The surface itself still shows. (#1208)

## Writes and undo

- A write that no command can take back (a dismissal, an accepted proposal) is held for the toast's 6.5 s and then sent; the toast's *Undo* cancels it, and the toast on screen is the sign a write is still held. Quitting, closing or reloading the window inside those 6.5 s loses the decision: nothing is written, and the proposal is back next time. Moving to another view does not; the write still goes. (#835)
- A write a page queues is drawn at once where it lands (a branch *queued*, a branch *delete queued*, a file's commit as a banner), held for the session as module `$state` and dropped once the source's read shows it; never stored. The toast reads `Queued: <what>.` with *Show queue*, from the one helper `shell/queued-toast.ts`; while the source's sign-in fails it adds `<Source> sign-in is failing, so it waits there.` (R-15.1). It never promises the next sync: a write is sent the moment it is queued, paused sync or not. (#839, #845, #1438)
- The pages share one echo module, `tf/queued.svelte.ts` (`echoWrite`, `settleEchoes`), keyed by entity and the write's id: the job's picked status and comment, a Docs section's text, comment and new page, a pull request's chips and comment, a run queued on a pipeline from its page or a run page (#1449), a pipeline's parameter change, a run's filed ticket, Standup's filed tickets, Monitors' create, pause and resume, and the repository's branches made and deleted and file commits (#2103-#2105). An echo goes when a read shows the write landed (a status, state, value, blob or page version that moved from where the write found it, a thread holding the comment's words, a listing or tree holding the name or title, the roster holding the monitor) and stays gone if the read moves back. What no read shows (a pull request's edit, update, review or source action, a secret parameter) goes on the first live read asked once it is *sent*. An echo is followed: held or refused, it goes and the toast says why with *Edit and resend*, which reopens the editor with what was typed. (#1424, #1542, #2103)
- Every echo (CONTEXT.md) keeps one lifecycle in that module: its chip reads *queued* while the write is open and *sent* once it left the queue as sent, never *queued* after, both in the `tf-info` tone (a branch's *delete queued* too); it goes on the read that shows the write. A surface tags its echoes with its name and source, and `watchEchoes` re-reads it when `item:refreshed` names an echo's write or entity or a run of an echo's source ends; the surface supplies only its `settleEchoes` check. A refresh naming an echo's write makes it *sent* before the re-read. A surface that already re-reads on its own source's run ends (a pipeline) passes `runEnds: false`. Where the echo is a word in a longer chip or line, the word changes and the rest stays: *Ticket queued* / *Ticket sent*, *Monitor queued* / *Monitor sent*, *delete queued* / *delete sent*, *merge queued* / *merge sent*, a commit banner's *is queued* / *was sent*; a run started here reads *queued* / *sent* in place of its number, with no state chip. A monitor's pause or resume draws its echo in place of the button until the roster reads the new state. (#2102, #2103-#2105)
- While its echo is drawn, a queued write holds back the writes it makes stale: drawn `aria-disabled` with the echo's chip as its title, opening nothing (a branch's queued delete drops its row's actions; a pull request's queued merge holds *Merge…*, *Update branch*, *Edit* and *Close*, a queued close or reopen *Merge…* and the state change). A source's own actions are never held. (#839, #1441)

- A button that re-reads an open write rather than sending it again never says *Retry*: a queued start-work step's says *Check again*, and a failed step's keeps *Retry*. *Retry* is for repeating what failed. Beside a write that may already have landed, it reads as an invitation to send twice. (#1998)

- Code the reader has to trust (an external adapter's install or update) is checked before anything is written. An install always asks, and an update asks when what it may reach, claim or use differs from what is installed. The confirm names the roles it claims, every host it may reach and its limits, and the write is held to what was checked (the artifact's hash, its hosts and its limits), so a release that moved in between is refused rather than installed unseen. (#2031)

- Deleting a note is the one irreversible write sent at once: a confirm dialog asks first, and the toast "Deleted <title>" offers no *Undo*, because the `deleted` activity line is final (#1464's ruling, #1504).

## Checkouts

- Every place a checkout is offered draws `tf/openin/TfOpenIn.svelte`: *Open in… ▾* when the repository is on this machine, *Clone* (and *Cloning… N %*, *Stop*) when it is not. Its applications are one store, `tf/openin/apps.svelte.ts`, and running clones another, `tf/openin/clones.svelte.ts`; a page reads its checkouts again when `clones.finished` moves. (#844, #846)
- A table row that opens its item on click ignores a click inside `.tf-openin` or `.tf-modal`, so the control and its dialogs are not the row. (#844)

## Filter keys

- A *This job* scope names the job's key after its label, in mono (`FilterScope.tag`), as prototype-6's `jobk`. (#835)
- A job section's *Open … view* turns that view's *This job* on: `TfSection`'s `onopen` writes the view's `sessionFilter` scope before the link is followed, and the view reads it from there. (#863)
- A view's *This job* reads `job_reach` once for the job's ticket and filters its own list, search answers included; a reason heading is the reach's origin: *Linked to <key>*, *Parent <key>*, or *Via <first step's title>*. (#863)
- Filter keys on the new shell's search use the launcher's grammar, evaluated on the server; a negation is `-key:value` on the keys only, and a row whose field is absent fails the key and passes its negation. (#811, PR #890; #810, PR #895)

## Builds

- A full route carries its first segment as `head`, so a section with two address heads (`builds`
  and `build`) draws each; a page matches on `section` and branches on `head`. (#848)
- A pipeline's address is `#/builds/<entity id>`; the Builds view reads declared frame fields only
  (`pipelines`, `pipeline_runs`), and the source's word is its configured name, once, as the
  header line over its pipelines. (#848)
- A tree's indent is a class per level (`tf-d1` … `tf-d8` in the pipeline list, 14 px a level),
  never an inline style or a container padding, so a selected row spans the list. (#849)
- Knobas-local per-pipeline state the reader sets (pins) is a `knobas.setting` row of entity ids,
  read back on `pipelines`; *This job* on Builds is `job_pipelines` by the ticket's entity id. (#849,
  #850)
- A pipeline page's tab is the address's next segment: `#/builds/<entity id>/params`,
  `/triggers`; Runs is the pipeline's own address. (#851)
- What a frame needs and the mirror does not hold is a live read on `Source` with a default
  `Ok(None)`, answered in a frame shape the shell draws (`knobas_source::pipeline`), never a
  payload. (#851)
- A geometry that depends on data (a chart's bar, a duration bar) is SVG attributes, since an
  inline style is refused; a tip is the SVG `<title>`. (#851)
- Knobas-local per-pipeline state with fields and a typed name (Run presets) is a table of its own
  with a case- and space-blind unique name; a save of an existing name replaces it. (#852)
- Starting a run goes through the one Run… dialog (`tf/builds/TfRunDialog.svelte`), which hands
  the page a `RunRequest`; the page queues it and draws it *queued*. The default branch is sent as
  no branch. (#852)
- A run queued from the pipeline page or a run page is an echo on the pipeline in the shared echo
  module; the pipeline page draws it *queued*, then *sent*, at the top of its runs until a read holds a newer run
  a person started on its branch (`runLanded`). The run page draws no queued row. (#1449)
- The pipeline and run pages re-read when a sync of their source ends (`sync:state` with `running`
  false, `listenSynced`): the runs, and on the run page the view, the log and the open tab, over
  what is drawn and keeping the reader's tab, filter and dialog. No polling between syncs. (#1450)
- A click anywhere on a pipeline page's run row opens the run; the number is the keyboard's link.
  Every day's runs table shares one `<colgroup>` and `table-layout: fixed`, so the columns line up
  down the page. (#1452)
- A run's address is `#/build/<pipeline entity id>/<run entity id>`, and its tab is the next
  segment (`log`, `tests`, `artifacts`, `changes`, `params`); Overview is the run's own address.
  `#/build/<run entity id>` alone moves to the full address. The run page draws from the run's row
  and its source view (`entity_view`), with no command of its own. (#853)
- A run page panel that needs a read beyond the run's row and view is its own component with its
  own read, injectable for its test (`TfRunChain` reads `run_chain`); a pipeline's chain rides on
  its settings (`PipelineSettings.chain`). (#857)
- Every *Create ticket* opens the shell's one create-ticket dialog, `tf/TfCreateTicket.svelte`.
  A ticket filed from an entity's page (a failed run's *Create ticket*) is queued with
  `submit_write`'s `link_to` naming that entity, so the backend links it once it arrives; the page
  draws its linked tickets from its confirmed links in place of *Create ticket*. Standup's action
  item keeps its own command, `create_action_item_ticket`, which links the ticket to the note.
  (#1451)
- A run's live reads (`run_log`, `run_tests`, `test_history`, `run_artifacts`, `artifact_preview`)
  route by the run's namespace with no mirror check, as `entity_view` does, and answer frame shapes
  (`knobas_source::run`). A download from a source is one command (`download_run_file`) into a new
  file in the downloads folder, named by the adapter's suggestion cut to its last segment, never
  the source's URL opened in a browser (D8). (#854, #855, #856)
- A run's steps are `run_log`'s steps; the run page reads the log once and hands that read to the
  Log tab (again for a run in flight). A step no read names is not drawn. (#1003)
- How a run was started is said one way everywhere, `triggerLabel` over the row's declared
  `trigger`, `started_by` and `revision`; *you* is the row's `started_by_me`, never a username the
  frontend compares. (#961)
- The run page reads a run's own facts from its row, never from the view's facts by their labels:
  *Agent* is `agent`, *Now* is `current_step`, *Commit* is `revision` (short, `shortSha`) opening
  `commit_id`. The view supplies only what has no declared field: tabs, artifacts, changes,
  parameters. (#1004)
