# P5 — Journal

**Thesis:** notes-first. Your day is a document: a journal page per day where the activity stream (commits, builds, comments, page edits, ticket transitions) is interleaved with your own notes, and remote entities are embedded as live blocks you can edit in place. Time is just the vertical axis; worklogs and the standup digest are blocks of the same page. Think Obsidian/Notion/Logseq daily notes, but the blocks are alive.

```
┌────────────┬──────────────────────────────────────────────┬──────────┐
│ Aug 2026   │ Friday 22 August 2026          ◔ 0:34 PAY-231│ LINKS    │
│ ‹ 18 19 20 │                                              │ PAY-231  │
│   21 [22]  │ ▸ Standup digest (generated)  [edit][publish]│ PR #142  │
│ NOTES      │   Yesterday · Today · Blockers …             │ #1187 ✖  │
│ SEPA retry │                                              │ page     │
│ Standup 21 │ 08:30  inbox · 6 items   [open inbox]        │ BACKLINKS│
│ Credentials│ 08:45  ▣ PAY-228  In Review  ▸ blocked OPS-77│ Standup… │
│ SMART LISTS│ 09:40  ▣ PAY-231  In Progress ▾  [+ comment] │ TODAY    │
│ Failing… 1 │ 10:02  ⎇ a41f2c PAY-231 backoff jitter       │ 5h28 trk │
│ PRs on me 1│ 10:10  ⚠ #1187 failed  attempts==5 (6) [rerun]│ 0 logged │
│ Blocked 1  │ 10:40  ✎ edited SEPA design › Backoff policy │ [log 2h44]│
│            │ 11:20  ✎ my note: off-by-one, start at 1…    │          │
│            │ 11:50  ░ unattributed 30 min  [assign]       │          │
│ ⚙ sources  │ 13:10  ◔ Staging DB configuration  48 min    │          │
│ ✉ inbox 6  │ 13:58  ◔ PAY-231 running…                    │          │
└────────────┴──────────────────────────────────────────────┴──────────┘
```

## Where the eight surfaces live
1. **Search + smart lists** — `⌘K` overlay; results can be *inserted as a block* into today's page or opened as their own page. Smart lists in the left sidebar with counts; a list opens as a page of embedded blocks.
2. **Entity detail + suggestions** — an entity block expands inline (▸) to the full detail (status dropdown, comments, links); "open as page" gives it a full page with the same editor. Suggested links appear as ghost blocks ("a41f2c mentions PAY-231 — link?") in the stream.
3. **Notes** — native: any text you type on the day page or a named note; `[[` picker; backlinks in the right pane.
4. **Sources** — sidebar ⚙ → a settings page (list + add source) in the same document style.
5. **Actions** — slash commands in the editor: `/start work PAY-240` inserts a live stepper block; `/trigger build`, `/new ticket`, `/new page` insert forms that become entity blocks on success.
6. **Inbox** — an "unread" block at the top of today's page (collapsible) and a sidebar entry; acting on an item records a line in the stream.
7. **Time** — the stream *is* the day review: blocks have durations, passive vs manual styling, the unattributed block offers *Assign*; stopping the timer inserts the worklog draft as a block with checkboxes; the right pane shows today's tracked/logged totals; *Timesheet* is a weekly page.
8. **Standup** — the first block of the day page: generated digest, then the protocol document below it; *Save as note* / *Publish to Confluence*.

## Signature interaction
Editing a live block edits the remote object: change PAY-231's status inside its block and the sidebar/links update, with a tiny "saved to Jira" confirmation. Show one worklog draft block being logged in place.

## Don't
- Don't turn it into a chat timeline; it's an editable document with time on the side.
- Don't make the activity lines noisy — group where possible, keep your own notes visually primary.
