# Required surfaces — every mockup must contain all eight

Your paradigm brief says *where* each surface lives (full screen, pane, overlay, block, command…). Whatever the form, each must be reachable by click and show the dataset content listed here. "Clickable" means: navigation works, selecting an item shows its detail, and mutating buttons produce an optimistic in-page result (state change, toast, inserted row) — never `alert()`.

## 1. Search + smart lists
- Query **"sepa retry"** against the local full-text index; mixed results across all sources (ticket, page, PR, commits, build, note), each with its source and "synced N min ago".
- Filter by source / type / people / date; make at least one filter visibly cross-source (e.g. *has failing build*).
- Pinned **smart lists** with counts and change badges, including *My tickets with a failing build* (call out that JQL can't express it).
- `Cmd/Ctrl+K` focuses/opens search from anywhere.

## 2. Entity detail + link suggestions
- **PAY-231** as the canonical example: summary, status (inline change), priority, assignee, description, comments (add one inline), worklogs, estimate/time spent.
- Linked items: branch, PR #142 (with check status), builds #1187 failed / #1188 running, page *SEPA payout retry design*, note *SEPA retry investigation*. Clicking any linked item navigates to its detail (at least PR #142, build #1187, the page and the note must have a detail view too — lighter than the ticket is fine).
- **Suggested links** strip: the four proposals from the dataset, each with its reason and *Confirm* / *Dismiss*.
- *Link to…* action (search-as-you-type picker).

## 3. Notes
- Note list + editor. Open *SEPA retry investigation*; `[[PAY-231]]`-style links render as entity chips; backlinks panel shows who links here.
- *New note* creates one (with the current context pre-linked).

## 4. Sources
- List of the four configured sources with type, URL, auth method, sync state, counts; TeamCity shows the **401** error with *Re-enter password*; Jira shows **PAT expires in 12 days**.
- *Add source* flow: pick type → URL → auth method (user+password / PAT / API token) → *Test connection* (shows a result) → sync schedule → save. Each source type is a pluggable module; make that visible (icon, "adapter v1.2", capabilities like search / write / webhooks).
- Sync now, sync schedule, local DB size.

## 5. Actions
- **Start work on PAY-240** → a reviewable one-action flow: create branch `feature/PAY-240-payout-dashboard-latency` in `payout-service`, push, open PR with the ticket title/description, link PR back to the ticket, transition PAY-240 → In Progress, (optionally) start the timer. Show each step completing.
- Pipeline: **Trigger Payout_IntegrationTests** with parameters (branch, env, clean checkout); view #1187 log excerpt; *Re-run failed*.
- Create ticket, create page (form → success with new key/page). Git: pull / push / new branch / create repo (any reasonable subset, at least pull/push and create repo).

## 6. Inbox
- The six items from the dataset, newest first, with their actions (Review / Reply / Open / Re-run / Snooze / Done); one snoozed item. Acting on an item removes or updates it in place.
- Inbox count is visible from every surface.

## 7. Time
- **Persistent timer** visible everywhere: context (PAY-231) + elapsed (0:34). Click → stop / switch context. Context may be any entity **or an ad-hoc label** (show "Staging DB configuration"). `Cmd/Ctrl+T` toggles. No git-branch awareness anywhere.
- **Worklog draft** dialog on stop: interval, target ticket (editable, optional), activity checkboxes, generated comment, *Log to PAY-231*.
- **Day review**: today's blocks as a horizontal strip (08:30–14:32), passive vs manual distinguished, the **unattributed** block highlighted with *Assign…*, the ad-hoc label block with *Log to a ticket… / Keep local*. Merge/drag is nice-to-have; assign must be clickable.
- **Week timesheet**: Mon–Fri totals, logged vs unlogged (4h 45m), *Log all*.

## 8. Standup
- Generated **digest** (yesterday / today / blockers) from the dataset, each line traceable to its source item (hover/click shows the commit, worklog, transition…).
- **Standup protocol** document for 2026-08-22 (attendees, per-person notes, action items) — editable text, *Save as note*, *Publish to Confluence* (shows target ENG › Standup protocols › 2026-08-22 and a success state).
- Action items can become tickets (*Create ticket* on an action item).
