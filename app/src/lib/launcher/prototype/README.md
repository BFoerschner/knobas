# ⌘K rethought: a clickdummy

**Throwaway.** This directory lives on the `prototype-launcher-next` branch and
never on `main`. It answers one question: does the launcher below feel right,
so that a spec can be written from it?

## Run it

```sh
env -C app npx vite --port 5311 --strictPort
open "http://localhost:5311/?fake-ipc&launcher-next#/"
```

Press ⌘K. Everything under `?fake-ipc` is the usual fixture; `launcher-next`
swaps the launcher for `LauncherNext.svelte` and lets the fixture answer free
text (`shell/dev/launcher-next-search.ts`).

For a screenshot: `&launcher-open=sepa` opens with that query, and
`&launcher-sheet=1` opens the action sheet on the row at that index once
results land. End the address with `#/job/mock:PAY-231` to have a chosen job.

## What it does

| Try | What happens |
|---|---|
| Empty box | The recent items, nothing else. The box says only "⌘? for help". |
| Opened by a page's search keys | `/` or ⌘/ opens the box with that section's filter already in it (⌘K stays the global search and opens empty): `#t ` on Tickets, `#rc ` on Code, `#p ` on Builds, `#d ` on Docs, `#m ` on Monitors, `#i ` on the Inbox. Delete the token to search everything. The pages' own search fields are gone; this is their search. |
| ⌘? | From any search: the box empties and narrows the filters (one word each), Enter adds the picked one to the search that was there, the keys sit at the bottom. `#r` then `#l` becomes `#rl`. |
| `sepa` | Results as today, with two new groups first: **Views** (saved ticket views, by name, group or conditions) and **Sets** (link sets, by name or description). |
| `#t`, `#c`, `#r`, `#p`, `#m`, `#d`, `#l`, `#i` | Tickets, pull requests (c for changes), repositories, pipelines, monitors, pages, your saved lists, the inbox. Letters combine: `#rl sepa` is repositories and lists. A key is `#t PAY-231`; a bare `#PAY-231` is no lookup any more. `>` go to, `@me` and `t ` time stay; `/` for a source is gone. |
| Tab on a row | The row lifts under the box, the box empties and the panel becomes its actions. Typing in the box narrows them (fuzzy, in order of the letters). Esc, Tab or Backspace on the empty box go back to the results with the search words back in the box. Never out of the dialog. The footer says just "Tab actions". |
| ⌘K while open | Gives the box its focus back, wherever a click left it. Nothing else: Tab is the one way into the actions. |
| An action that asks something | A **flow**: the sheet shows the step's choices, the box narrows them, Enter picks, the next step follows, the last pick runs the action. Esc or Backspace on the empty box step back. |
| `#l`, `#l gitea` | Your saved lists. Enter puts the list's own text back into the box, filters and words, and runs it; you stay in the box. Tab on one: *Apply*, *Rename* (the box takes the new name, Enter keeps it), *Favourite*, *Delete*, *Assign key…*. |
| `#i`, `#i review` | The inbox, newest first. Enter opens what the line is about. Tab: *Re-run* (a failed build), *Acknowledge* (an alert), *Snooze…* (a flow: tomorrow or next Monday), *Done*. |
| ⌘S on any query | The box asks for the list's name, the query prefilled, with a highlighted **Name** label in front of the box that is not part of the text; Enter saves, Esc keeps searching. The same label names every other thing the box asks for: Filter, Actions, Keys, New name, a flow's step. |
| A key on a saved list | Tab on the list, *Assign key…*: a panel shows the keys as you press them, large; Enter or OK keeps them, Backspace clears, Esc leaves things as they were. From then on that chord opens the list from anywhere, launcher closed or not. |
| A key on an action | Tab on a row, select the action, ⌘B: the same panel, the keys shown as you press them, Enter or OK keeps them. From then on that chord runs the action on whatever row of that **kind** is selected, without opening the actions. The same chord can mean another action on another kind: ⌘⇧O is *Open in VS Code* on repositories and *Approve* on pull requests if you bind it so. The row shows its chord. |
| *Favourite* on any entity or saved list | Puts it at the top of the empty box under one **Favourites** heading, lists and items alike, then the recents; *Unfavourite* takes it out. |
| `#d sepa` | Pages only, and a last row *Search Docs for "sepa"* that opens `#/docs`. |
| **Titles** / **Everything** (⌘D, or the chip at the right of the box) | *Titles* finds only what is on screen: rows whose title or key column holds the words, so `pay-210` finds PAY-210. *Everything* searches bodies too, the dialog grows, and the body splits: results at the left, and at the right where the words appear in the selected row. A page shows the Docs area's own hits by section; a pipeline shows the lines of its latest run log; a ticket, pull request or note shows its text from its view. Words found only in a run log add the pipeline to the answer. A click on an excerpt opens the row. |

### The sheet, per kind

The pair comes first on every entity: *Link to ⟨job⟩* with the relation
inline, *Add to a set…* (pages, repos, pipelines, assets, monitors) or
*Show a set on ⟨key⟩…* (tickets), both flows. *Start timer* is a ticket's
row alone, and starting it makes the ticket the job (the shell does that
already). Then the kind's own, each a real address or a real write of the
app today:

| Kind | Actions |
|---|---|
| ticket | *Start timer*, *Work on this* (`#/job/<id>`), *Copy key* |
| pull request | *Review the files* (the Files changed tab), *Conversation*, *Approve* (a queued write), *Merge…* (a flow: merge commit, squash or rebase, then a queued `MergePullRequest`), *Open in ⟨app⟩* (one row per application under Settings › Checkouts, the default first), *Copy title* |
| repo | *Create pull request…* (a flow: the branch with the change, then the branch it goes into, from the repository's Branches view; then the compare page), *Pull requests*, *Branches*, *Open in ⟨app⟩*, *Clone* (ADR-0023), *Copy title* |
| pipeline | *Run* (a queued `TriggerBuild`), *Copy title* |
| monitor | *Acknowledge alert*, *Copy title* |
| commit, branch | *Open in ⟨app⟩* (ADR-0034), *Copy title* |
| page, asset, note | the pair and *Copy title* |
| view | *Open*, *Copy address* |
| set | *Open*, *Show on ⟨job⟩* |
| saved list | *Apply*, *Rename*, *Delete* |
| inbox line | *Open*, *Re-run* / *Acknowledge* where they apply, *Snooze…*, *Done* |

*Change status* and *Comment* stay with the ticket page: each needs a
dialog, and nothing in the sheet does. A source action an adapter declares
(spec, *Source action*) would join the kind's rows under the same rule, and
one with fields would be a flow.

Where a flow fits elsewhere: *Link to ⟨job⟩* could become one (relation,
then an optional note) instead of the inline chips; *Re-run* on a pipeline
could pick a run preset; *Change status* could pick the transition the
source offers and then open the ticket page's dialog with it set.

### The rows

Round 9, "less is more": `RowNext.svelte` and `ResultsNext.svelte` draw
the rows; the shipped `Row.svelte` and `Results.svelte` would take their
shape.

- Three cells like a table: the kind's monogram, a key column, the title.
- The monogram is coloured by the state's tone, the same tones the chips use: a merged pull request, a failed pipeline, a monitor that is down are told by colour alone. No chip, no state word.
- The key column is something a person reads: a ticket's key in the mono face; otherwise where the thing sits, a pull request's repository, a repository's owner (its own name is the title), a page's space, a pipeline's project, a monitor's type, a view's group, an inbox line's time; the kind's word when nothing better is known.
- Tickets are coloured by status too: done green, in progress or in review blue, closed grey, to do plain. The ⌘? card ends with a legend per kind.
- No second line, no provenance, no "no secret", no counts on the headings or in the footer. What a second line said is in the title or not worth the row's height; a repository's path is its name.
- Builds are their configurations: `#b` finds pipelines, coloured by their last run. Runs and commits are not in the answer at all; they are reached from the pipeline and the repository.

## What is real and what is a shortcut

Real: `TfDialog`, `QueryBox`, `Chips`, `Results`, `Row`, `Board`, `Help`,
`Session`, the `>` palette, the `?` card, *Save as list*, and every write.
`create_link` goes through `drawn()` exactly as the job's link dialog does;
`save_link_set` re-saves the set with one member or one manual rule added.
Against the real backend (`just dev`) the same component works with real
views, sets and links; only the fixture's free-text search is dev-only.

Shortcuts, each marked `PROTOTYPE` in the code:

| Shortcut | The real change |
|---|---|
| `#t #p #r #b #m #d` are rewritten to `kind:` filters, `#l` and `#i` are answered in the frontend from the board's saved lists and the shell's inbox store (ruling P2 says the backend parses). | `#x` tokens in `crates/knobas-search/src/query.rs` beside `#KEY`, rows in `syntax.json`, chip labels in `Chips.svelte`. `/` is removed from the grammar or left undocumented. |
| Views and Sets are built from `ticket_views` and `link_sets` in the component and prepended to the answer. | `knobas-search` indexes both tables as kinds `view` and `set` with monograms `VW` and `LS`; `entityAddress` learns `#/tickets/<id>` and `#/sets/<id>`. |
| The *Search Docs for …* row stores its words in `sessionStorage` and opens `#/docs`; the Docs page ignores them. | A `#/docs?q=` form, or the search field reading a one-shot hand-off, whichever the Docs page prefers. |
| *Create pull request…* opens the compare page with the head branch in its address and stores the base in `sessionStorage`; the page picks its own base. | `compare/<head>/<base>` in the address, or the page reading the hand-off. |
| *Merge…* is offered on every pull request. | The row follows the source's declared `merge_pull_request` action, as the PR page's button does, and shows its `disabled` reason. |
| ⌘? draws a card written in the component, as an overlay the box narrows. | Rows in `syntax.json`, which the Rust test pins to the parser; the overlay and the merge of `#` letters in `Launcher.svelte`. |
| The sheet's actions are a fixed list per kind, in the component. | A table per kind in `launcher/actions.ts`, and adapter-declared source actions joining it. |
| *Titles* is a client-side filter on the engine's answer; *Everything* merges `docs_search` and reads each pipeline's latest run log through the IPC, kept in memory. | A `titles` flag on the query; run logs and page sections indexed in `knobas-search` with the match's place in the hit, so the preview is one read. |
| The preview reads a ticket's, pull request's or note's text from its `entity_view` and marks the words itself. | The engine's `snippet` grows to every match with its section, or the view read is kept and the marking moves to the backend. |
| The pages' own search fields are hidden by a `:global` rule and `/` is taken in the capture phase. | Remove the fields and their `/` handlers from the Docs, Builds and Inbox pages; the top bar's search button opens the box with the section's filter. |
| Value suggestions and pasted-URL resolving are left out. | Nothing: they stay as shipped. |
| Favourites, list keys and action keys live in `localStorage`. | A table each: a favourite is an entity id with a position; a key is a chord with a list id or a kind and an action id. The chords a list binds have to be registered with the shell's own keys so they work with the launcher closed. |
| `BoardNext.svelte` is the recent items alone, favourites first. | `Board.svelte` loses the smart lists, the inbox, today's time and the source strip; built-in smart lists go, or move behind `#l`. The box loses its ⌘K badge and its footnote. |

## Rulings this reverses

The 2026-10-21 ruling on #1416 (`docs/archive/2026-10-unattended-rulings.md`)
trimmed the chain to two rows and sent the relation to the job's dialog.
Picking the relation inline and *Work on this* go against it; the ruling
itself notes that restoring a row is a prop and a row. *Change status* and
*Comment* stay with the ticket page: each needs a dialog, and the point of
the sheet is that nothing in it does.

## Files

- `LauncherNext.svelte`: the component, mounted by `App.svelte` under `?launcher-next` (dev only, dynamic import).
- `BoardNext.svelte`: the board without lists.
- `../../shell/dev/launcher-next-search.ts`: free text over the fixture, used by `fake/launcher.ts` under the same flag.
- `../../shell/dev/fake/docs.ts`: `fakeDocPages()`, the one export added.
