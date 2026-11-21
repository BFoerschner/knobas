# Tab opens a sheet of the row's actions, and the launcher writes the link itself, 2026-11-02

Björn's ruling in spec #2419 (*⌘K rethought*, its *Further Notes*: "the #1416 ruling
reversed"), built by #2426 and recorded here by its implementer as the ticket asks. It is his
ruling, not an agent's, and it binds as such.

## What it reverses

The 2026-10-21 ruling on #1416 (`docs/archive/2026-10-unattended-rulings.md`), built by #1500,
trimmed the launcher's Tab chain to two rows, *Link to ⟨key⟩* and *Start timer on ⟨title⟩*. The
link row showed only while the breadcrumb named a job, closed the launcher and opened the job's
own link dialog with the result preset; the launcher never wrote a link. *Change status*,
*Comment*, *Start work* and *Open* were left out.

## The ruling

**Tab opens a sheet.** Tab on a row lifts it under the box, drawn by the row component the
results use. The box, labelled *Actions*, empties and narrows the row's actions, every typed
letter in order, the tightest match first. Esc, Tab or Backspace on the empty box leave the
sheet with the search words back as they were. ⌘K while the launcher is open only gives the box
its focus back.

**The actions are a table per kind** (`app/src/lib/launcher/sheet.ts`), each an address of the
app or a write it has today:

- every entity: *Link to ⟨job⟩* first; *Copy key* on a ticket, *Copy title* on anything else, last;
  **Amended 2026-11-06 (UX polish, F-637, how it feels wins):** the link no longer comes first. What the kind does comes first, on a ticket *Work on this* then *Start timer*, then the link, so the positions hold still and `Tab`, `Enter` does not write a link.
- ticket: *Start timer* (on tickets only), *Work on this*;
- pull request: *Review the files*, *Conversation*, *Approve*, *Open in ⟨app⟩*;
- repository: *Pull requests*, *Branches*, *Open in ⟨app⟩*, *Clone*;
- branch: *Open in ⟨app⟩*; pipeline: *Run*; monitor: *Acknowledge*;
- inbox line: *Open*, *Re-run* where the source offers it, *Acknowledge* on an alert, *Done*;
- saved list: *Apply*, *Rename* (the box takes the new name), *Delete*;
- view: *Open*, *Copy address*; set: *Open*.

*Open in ⟨app⟩* is one row per application under Settings › Checkouts, the default first, through
the same `open_checkout` the repository page uses. The flows (*Add to a set*, *Snooze*, *Merge*,
*Create pull request*), *Favourite* and the action keys are spec #2419's later tickets
(#2427, #2428, #2429).

**The pair writes itself.** *Link to ⟨job⟩* is offered on every page the shell names a job for:
the ticket on screen on a job or ticket page, else the chosen job, so Tickets, Inbox and Settings
too. The relation is picked inline by ← and → from the job dialog's six, and Enter writes the
link through the dialog's own mapping (`drawn`, from the job to the row). The toast names both
ends and the relation. The launcher's hand-off to the job's dialog (`linkRequest`) is gone.

*Change status* and *Comment* stay with the ticket page: each needs a dialog, and nothing in the
sheet does.

## Why

The two-row chain made every act on a found thing a trip out of the box. The prototype on branch
`prototype-launcher-next` showed that the box can narrow a row's actions the way it narrows the
search, and that the link's relation fits on the row as chips, so the dialog the old ruling sent
the reader to added a step and nothing else.

## Cost of reversing

The table is one module and the sheet one mode of the box: going back to a chain is dropping rows
from `sheetActions`. Bringing back the hand-off would mean restoring `linkRequest` in
`tf/job/current.svelte.ts` and the job page's effect that took it.
