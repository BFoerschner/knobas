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
| ⌘K again, or Tab, on a row | The row lifts under the box and the panel becomes its actions. Esc goes back to the results. Typing goes back too. |
| On a ticket, page, PR… | *Link to ⟨job⟩* with the relation picked by ←→ or a click, Enter writes it. *Add to a set…* (pages, repos, pipelines, assets, monitors) or *Show a set on ⟨key⟩…* (tickets) opens a second level listing the sets. *Start timer*, *Work on this* (tickets), *Copy key* / *Copy title*. |
| On a view or a set | *Open*, *Copy address*; on a set with a chosen job, *Show on ⟨job⟩*. |
| On a saved smart list (empty box) | *Rename*, *Delete*, as the shipped Tab chain. |
| `doc:sepa` | Pages only, path and excerpt, and a last row *Search Docs for “sepa”* that opens `#/docs`. |

The footer always names what the second ⌘K would act on.

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
| `doc:` is rewritten to `kind:page` in the frontend (ruling P2 says the backend parses). | One `Prefix::Doc` in `crates/knobas-search/src/query.rs`, a row in `syntax.json`, a chip label in `Chips.svelte`. |
| Views and Sets are built from `ticket_views` and `link_sets` in the component and prepended to the answer. | `knobas-search` indexes both tables as kinds `view` and `set` with monograms `VW` and `LS`; `entityAddress` learns `#/tickets/<id>` and `#/sets/<id>`. |
| The *Search Docs for …* row stores its words in `sessionStorage` and opens `#/docs`; the Docs page ignores them. | A `#/docs?q=` form, or the search field reading a one-shot hand-off, whichever the Docs page prefers. |
| The sheet's actions are a fixed list per kind. | Adapter-declared source actions would join the list under a rule. |
| Value suggestions and pasted-URL resolving are left out. | Nothing: they stay as shipped. |

## Rulings this reverses

The 2026-10-21 ruling on #1416 (`docs/archive/2026-10-unattended-rulings.md`)
trimmed the chain to two rows and sent the relation to the job's dialog.
Picking the relation inline and *Work on this* go against it; the ruling
itself notes that restoring a row is a prop and a row. *Change status* and
*Comment* stay with the ticket page: each needs a dialog, and the point of
the sheet is that nothing in it does.

## Files

- `LauncherNext.svelte`: the component, mounted by `App.svelte` under `?launcher-next` (dev only, dynamic import).
- `../../shell/dev/launcher-next-search.ts`: free text over the fixture, used by `fake/launcher.ts` under the same flag.
- `../../shell/dev/fake/docs.ts`: `fakeDocPages()`, the one export added.
