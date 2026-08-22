# P4 — Context hub

**Thesis:** you always work *in a context* — an epic, a ticket, an incident, or an ad-hoc thing like "Staging DB configuration". Pick the context and the app becomes a room with everything related, pulled from every source: tickets, branches/PRs, builds, pages, notes, time, and the inbox items that belong here. Switching context is the primary navigation, and each context has its own clock.

```
┌──────────────────────────────────────────────────────────────────────┐
│ CONTEXT  [PAY-200 Payout reliability ▾]  PAY-231 · Staging DB · +new │
│ ◔ 0:34 in PAY-231      ⌕ ⌘K       ✉ 6 (3 here)     ● ● ● ✖ teamcity │
├─────────────────────────────┬────────────────────────────────────────┤
│ TICKETS (board)             │ CODE                                   │
│ To Do     In Prog   Review  │ ⎇ feature/PAY-231-sepa-retry  +3       │
│ PAY-240   PAY-231   PAY-228 │ PR #142  1/2 ✓  ✖ #1187  ◌ #1188       │
│ PAY-236             ⛔OPS-77│ PR #139  merged                        │
├─────────────────────────────┼────────────────────────────────────────┤
│ BUILDS                      │ DOCS                                   │
│ #1188 ◌ running  #1187 ✖    │ SEPA payout retry design (edited 10:40)│
│ #412 ✓ staging              │ Ledger reconciliation runbook          │
├─────────────────────────────┼────────────────────────────────────────┤
│ NOTES                       │ TIME IN THIS CONTEXT                   │
│ SEPA retry investigation    │ today 2h 44m · week 9h 30m · unlogged  │
│ Standup 2026-08-21          │ [Log today]                            │
└─────────────────────────────┴────────────────────────────────────────┘
```

## Where the eight surfaces live
1. **Search + smart lists** — `⌘K` overlay searches everywhere; results offer "open" or "add to this context". Smart lists are global lists in the context switcher dropdown *and* per-context filters. The room for an epic is essentially the smart list "everything linked to PAY-200".
2. **Entity detail + suggestions** — clicking a card opens a detail panel inside the room (slide-over or the room's right half); suggested links appear as a "Might belong here" tray at the bottom of the room with Confirm/Dismiss.
3. **Notes** — NOTES tile: notes linked to this context; new note is pre-linked to the context.
4. **Sources** — global settings page (gear): list + add source.
5. **Actions** — room toolbar: *Start work* (on a To Do card), *Trigger build*, *New ticket in this epic*, *New page*; the *Start work on PAY-240* flow runs as a stepper inside the room.
6. **Inbox** — global inbox from the top bar, with a "in this context" filter (3 of 6 items belong to PAY-200).
7. **Time** — each context has its own timer; the top bar shows the running one; switching context asks "Switch timer to Staging DB configuration?". Day review is a global view (top bar ◔) where blocks are colored by context; timesheet per context and total.
8. **Standup** — global "Today" view: digest + protocol, grouped by context.

## Signature interaction
Context switching: change the dropdown from PAY-200 to "Staging DB configuration" and watch the whole room swap (and the timer prompt). Show at least three contexts: PAY-200 (epic, rich), PAY-231 (ticket, focused), "Staging DB configuration" (ad-hoc, mostly notes + time, no code).

## Don't
- Don't make it a generic dashboard of widgets; every tile must be filtered by the active context.
- Don't forget the ad-hoc context — it's the proof that not all work is code.
