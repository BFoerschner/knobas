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
| `sepa` | Results as today, with two new groups first: **Views** (saved ticket views, by name, group or conditions) and **Sets** (link sets, by name or description). |
| A letter and a space | `p` pull requests, `r` repos, `b` builds, `m` monitors, `d` pages, `i` inbox, `l` saved lists, `t` time (as shipped). `#` tickets, `@` people, `>` go to, `?` help stay. `/` for a source is gone. |
| ⌘K again, or Tab, on a row | The row lifts under the box, the box empties and the panel becomes its actions. Typing in the box narrows them (fuzzy, in order of the letters). Esc, ⌘K, Tab or Backspace on the empty box go back to the results with the search words back in the box. Never out of the dialog. |
| ⌘K while a click took the focus | Gives the box its focus back and nothing else. The next ⌘K opens the sheet. |
| `l`, `l gitea` | Your saved lists, all of them or by name. Enter applies the list's filters in the box and keeps you there. ⌘K on one: *Apply*, *Rename* (the box takes the new name, Enter keeps it), *Delete*. |
| `i`, `i review` | The inbox, newest first, or the lines holding the words. Enter opens what the line is about. ⌘K: *Re-run* (a failed build), *Acknowledge* (an alert), *Snooze until tomorrow 9:00*, *Done*. |
| ⌘S on any query | Saves it as a list; the footer says so. No button. |
| `d sepa` | Pages only, path and excerpt, and a last row *Search Docs for "sepa"* that opens `#/docs`. |
| Empty box | The board: inbox, today, recent. No lists block, no sources strip, no mirror footnote. |

The footer always names what the second ⌘K would act on.

### The sheet, per kind

The pair comes first on every entity: *Link to ⟨job⟩* with the relation
inline, *Add to a set…* (pages, repos, pipelines, assets, monitors) or
*Show a set on ⟨key⟩…* (tickets), then *Start timer*. Then the kind's own,
each a real address or a real write of the app today:

| Kind | Actions |
|---|---|
| ticket | *Work on this* (`#/job/<id>`), *Copy key* |
| pull request | *Review the files* (the Files changed tab), *Conversation*, *Approve* (a queued write), *Open in editor* (the checkout at its branch), *Copy title* |
| repo | *Create pull request* (the compare page, where a request is drawn from a branch), *Pull requests*, *Branches*, *Open in editor*, *Clone* (ADR-0023), *Copy title* |
| build | *Re-run* (a queued write), *Copy title* |
| monitor | *Acknowledge alert*, *Copy title* |
| commit, branch | *Open in editor* (ADR-0034), *Copy title* |
| page, asset, note | the pair and *Copy title* |
| view | *Open*, *Copy address* |
| set | *Open*, *Show on ⟨job⟩* |
| saved list | *Apply*, *Rename*, *Delete* |
| inbox line | *Open*, *Re-run* / *Acknowledge* where they apply, *Snooze until tomorrow 9:00*, *Done* |

*Change status* and *Comment* stay with the ticket page: each needs a
dialog, and nothing in the sheet does. A source action an adapter declares
(spec, *Source action*) would join the kind's rows under the same rule.

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
| `p r b m d ` are rewritten to `kind:` filters, `l ` and `i ` are answered in the frontend from the board's saved lists and the shell's inbox store (ruling P2 says the backend parses). | One `Prefix` each in `crates/knobas-search/src/query.rs`, rows in `syntax.json`, chip labels in `Chips.svelte`; `?` does not list them yet. `/` is removed from the grammar or left undocumented. |
| Views and Sets are built from `ticket_views` and `link_sets` in the component and prepended to the answer. | `knobas-search` indexes both tables as kinds `view` and `set` with monograms `VW` and `LS`; `entityAddress` learns `#/tickets/<id>` and `#/sets/<id>`. |
| The *Search Docs for …* row stores its words in `sessionStorage` and opens `#/docs`; the Docs page ignores them. | A `#/docs?q=` form, or the search field reading a one-shot hand-off, whichever the Docs page prefers. |
| The sheet's actions are a fixed list per kind, in the component. | A table per kind in `launcher/actions.ts`, and adapter-declared source actions joining it. |
| Value suggestions and pasted-URL resolving are left out. | Nothing: they stay as shipped. |
| `BoardNext.svelte` is `Board.svelte` without the lists block, the rename field, the row buttons and the source strip. | Delete those from `Board.svelte`; built-in smart lists go, or move behind `l `. |

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
