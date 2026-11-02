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
| ⌘? | From any search: the box empties and narrows the filters (one word each), Enter adds the picked one to the search that was there, the keys sit at the bottom. `#r` then `#l` becomes `#rl`. |
| `sepa` | Results as today, with two new groups first: **Views** (saved ticket views, by name, group or conditions) and **Sets** (link sets, by name or description). |
| `#t`, `#p`, `#r`, `#b`, `#m`, `#d`, `#l`, `#i` | Tickets, pull requests, repositories, builds, monitors, pages, your saved lists, the inbox. Letters combine: `#rl sepa` is repositories and lists. A key is `#t PAY-231`; a bare `#PAY-231` is no lookup any more. `>` go to, `@me` and `t ` time stay; `/` for a source is gone. |
| Tab on a row | The row lifts under the box, the box empties and the panel becomes its actions. Typing in the box narrows them (fuzzy, in order of the letters). Esc, Tab or Backspace on the empty box go back to the results with the search words back in the box. Never out of the dialog. The footer says just "Tab actions". |
| ⌘K while open | Gives the box its focus back, wherever a click left it. Nothing else: Tab is the one way into the actions. |
| An action that asks something | A **flow**: the sheet shows the step's choices, the box narrows them, Enter picks, the next step follows, the last pick runs the action. Esc or Backspace on the empty box step back. |
| `#l`, `#l gitea` | Your saved lists. Enter applies the list's filters in the box and keeps you there. Tab on one: *Apply*, *Rename* (the box takes the new name, Enter keeps it), *Delete*. |
| `#i`, `#i review` | The inbox, newest first. Enter opens what the line is about. Tab: *Re-run* (a failed build), *Acknowledge* (an alert), *Snooze…* (a flow: tomorrow or next Monday), *Done*. |
| ⌘S on any query | Saves it as a list; the footer says so. No button. |
| `#d sepa` | Pages only, path and excerpt, and a last row *Search Docs for "sepa"* that opens `#/docs`. |

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
| build | *Re-run* (a queued write), *Copy title* |
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
| Value suggestions and pasted-URL resolving are left out. | Nothing: they stay as shipped. |
| `BoardNext.svelte` is the recent items alone. | `Board.svelte` loses the smart lists, the inbox, today's time and the source strip; built-in smart lists go, or move behind `#l`. The box loses its ⌘K badge and its footnote. |

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
