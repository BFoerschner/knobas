# P2 — Workbench

**Thesis:** an IDE for your work. Three panes plus a status bar, like VS Code / JetBrains: everything is a tree on the left, a tabbed document in the center, metadata and links on the right. Dense, resizable, keyboard-driven, made for having six things open at once.

```
┌─┬──────────────┬───────────────────────────────────────┬──────────────┐
│⌕│ SMART LISTS  │ PAY-231 ×│ PR #142 ×│ SEPA design ×│ + │ INSPECTOR    │
│✉│ ▸ Failing… 1 │───────────────────────────────────────│ Links        │
│◔│ ▸ PRs on me 1│ PAY-231  Retry failed SEPA payouts    │  ⎇ feature/… │
│≡│ SOURCES      │ [In Progress ▾] High · Mara · est 2d  │  PR #142 1/2 │
│⚙│ ▾ Jira · PAY │                                       │  #1187 ✖     │
│ │   PAY-231    │ Description…                          │  #1188 ◌     │
│ │   PAY-228    │ Comments…  [Add comment]              │  page, note  │
│ │ ▾ Gitea      │                                       │ Suggested    │
│ │   payout-svc │ Worklogs   3h yesterday               │  a41f2c ✓ ✗  │
│ │ ▸ Confluence │                                       │ Sync 4m ago  │
│ │ ▸ TeamCity ✖ │                                       │              │
│ │ NOTES        │                                       │              │
├─┴──────────────┴───────────────────────────────────────┴──────────────┤
│ ◔ 0:34 PAY-231   ● ● ● ✖ teamcity 401   ✉ 6   0 pending   ⌘K ⌘T       │  ← status bar
└───────────────────────────────────────────────────────────────────────┘
```

## Where the eight surfaces live
1. **Search + smart lists** — activity-bar icon opens a search panel in the left pane (query, filters, results); smart lists are the top section of the tree. `⌘K` is also a command palette overlay (commands + entities).
2. **Entity detail + suggestions** — center tab per entity (ticket, PR, build, page, note, repo each a tab type with its own layout); right inspector: Links, Suggested links (confirm/dismiss), Worklogs, Activity, Sync provenance.
3. **Notes** — NOTES section in the tree; note opens as a center tab with an editor; backlinks in the inspector.
4. **Sources** — activity-bar gear → Sources opens as a center tab (settings document): list + add-source form; the tree shows each source as a root with its sync badge (TeamCity ✖ 401).
5. **Actions** — a *Start work on PAY-240* button on the ticket tab opens a run panel (bottom panel, like a terminal/output pane) that streams the steps. Trigger build from the build-config tab; create ticket/page from the tree's `+`.
6. **Inbox** — activity-bar icon with badge; opens as the left pane content; double-click opens the item's tab.
7. **Time** — timer in the status bar (click → popover: stop / switch context / ad-hoc label). Stopping opens the worklog draft as a modal. *Day review* and *Timesheet* are center tabs (the day review a horizontal strip editor).
8. **Standup** — center tab "Standup 2026-08-22": digest on top, protocol document below, *Save as note* / *Publish*.

## Signature interaction
Tabs + bottom panel: run the *Start work* flow and watch its steps log into the bottom panel while the ticket tab updates its status live. Split view (two tabs side by side) is a nice-to-have.

## Don't
- Don't make it airy; density is the point. Resizable panes (drag handles) if cheap.
- Don't drop the status bar or the inspector.
