# P1 — Launcher

**Thesis:** the app *is* a search box. Like Raycast/Alfred/Spotlight: you type, you get mixed results from the local index across every source, you act on them without leaving the box. Every other surface is a *mode* of the same box. Minimal chrome; the window can be compact and grow when detail is open.

```
┌──────────────────────────────────────────────────────────────────────┐
│ ◔ 0:34 PAY-231        ● jira ● wiki ● git ✖ teamcity        ✉ 6  ⚙  │  ← one thin persistent strip
├──────────────────────────────────────────────────────────────────────┤
│  ▌ sepa retry                                                  ⌘K    │  ← the box
├──────────────────────────────────────────────────────────────────────┤
│  TICKET   PAY-231  Retry failed SEPA payouts     In Progress  · 4m   │
│  PAGE     SEPA payout retry design               edited 10:40 · 4m   │
│  PR       #142 SEPA retry with exponential backoff   1/2 ✓  · 2m     │
│  COMMIT   a41f2c  PAY-231 backoff jitter                   · 2m      │
│  BUILD    #1187 Payout_IntegrationTests          FAILED    · 38m ✖   │
│  NOTE     SEPA retry investigation               11:20               │
│  ACTION   › Start timer on PAY-231   › Comment on PAY-231            │
├──────────────────────────────────────────────────────────────────────┤
│  PAY-231 · Retry failed SEPA payouts                   [Tab → actions]│  ← selected result expands
│  In Progress · High · Mara · est 2d · 6h30 spent                     │     (inline below, or a
│  Links: ⎇ feature/PAY-231… · PR #142 · #1187 ✖ · #1188 ◌ · page · note│      split pane — your call)
│  Suggested: commit a41f2c mentions PAY-231   [Confirm] [Dismiss]     │
└──────────────────────────────────────────────────────────────────────┘
```

## Where the eight surfaces live
1. **Search + smart lists** — the box itself. Empty query shows: smart lists with counts, inbox preview, today's time, recent items. Typed query → mixed results grouped by type. Prefix syntax is the power feature: `>` actions, `#` tickets, `@` people, `/` source filter, `t ` time, `?` help. Results list is keyboard-first (↑↓ ↵ Tab).
2. **Entity detail + suggestions** — selected result expands (inline accordion or split pane). `Tab` on a result opens its action list (Change status, Comment, Link to…, Start timer, Start work, Open in browser).
3. **Notes** — `> notes` or `note:` prefix; editor takes over the result area; `[[` triggers an entity picker inside the editor.
4. **Sources** — `> sources`. A settings mode: list, add-source wizard as a step-by-step sequence of the same box ("Type?", "URL?", "Auth?", "Test connection…").
5. **Actions** — `> start work on PAY-240` shows a checklist that ticks off live; `> trigger Payout_IntegrationTests` asks for parameters as chips.
6. **Inbox** — `> inbox` mode; each item has inline actions; counter in the strip.
7. **Time** — timer pill in the strip; `⌘T` or `> stop timer` opens the worklog draft as a box mode; `> day review` shows the strip of blocks; `> timesheet`.
8. **Standup** — `> standup` shows digest then protocol as an editable document mode with *Save as note* / *Publish*.

## Signature interaction
Chaining: result → Tab → action → parameters → result, all in the box, with a breadcrumb of the chain ("PAY-231 › Comment"). Show at least one chain.

## Don't
- Don't add a sidebar or tabs bar; if you need navigation, it's a mode switcher inside the box (segmented pills are fine).
- Don't hide the timer/inbox/sync strip in any mode.
