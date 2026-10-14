# knobas: capability sheet for UI mockups

Written 2026-10-14 against `main` at `d228d9ee`. This lists what the app can **do**. It does not say how anything should look. A new mockup may arrange, merge, split or hide any of this. It should not show a function marked LATER as if it worked, and it should not drop a BUILT function without saying so.

Vocabulary is `CONTEXT.md`'s (room, tile, mini board, launcher, inbox, timer target, block, worklog, estate, Tree, spine, wire…). The data for the screens is still `dataset.md` + `assets.md` (Tidewater).

**Tags**
- **BUILT**: works in the app today, with UI.
- **BACKEND**: the IPC command exists and is tested; the UI doesn't expose it, or only exposes part of it.
- **LOW**: small work on plumbing that already exists. No new adapter. At most one new IPC command or `WriteOp` variant, which costs a §10.8 entry in `docs/contract.md`.
- **LATER**: needs real design or backend work (a new adapter, a new op family, a new model). A mockup can sketch it but must label it as future.

---

## 0. Constraints any layout must keep

These are decided, so a mockup changes them only by changing the spec.

| Constraint | Source |
|---|---|
| Every entity has one stable address (`#/ctx/<id>`, `#/<kind>/<id>`, `#/entity/<id>`, `#/asset/<id>`, `#/route/<id>`, `#/assets/tree`, `#/assets/monitors`, `#/inbox`, `#/inbox/ctx/<id>`, `#/time`, `#/time/<date>`, `#/standup`, `#/sources`, `#/settings`, `#/first-run`). Notifications, paste-URL chips and inbox rows deep-link to these addresses. | spec §2, `shell/router.svelte.ts` |
| Keyboard-complete: `⌘K` launcher, `⌘T` timer, `Esc` unwinds one layer at a time, focus always visible, reduced motion respected. | spec §2, §14 |
| Design target 1440×900, usable from 1100 px wide. | spec §2 |
| Values have a **tone** (`ok` `warn` `err` `info` `muted`) and the shell picks the colour. Adapters never pick colours. | `CONTEXT.md` Tone |
| A source's detail view is drawn from a **fixed block vocabulary**: facts, text, entity list, diff, log, settings, actions, tree, file. The UI renders blocks and never knows which adapter sent them. | ADR-0020 |
| Writes go through the visible **write queue** and are never shown as already done ("queued", not "paused"). | ADR-0012, Pending write |
| Links are local and never written to a source. Suggestions show their reason. | spec §5a |
| "Board" is never used alone: write *launcher board*, *mini board*, or *Tree*. | ADR-0009 |
| Contrast ≥ 4.5:1 on readings. | spec §14 |

---

## 1. Shell (always on screen)

| Function | Tag | Notes |
|---|---|---|
| Top strip: context switcher (tabs for recent rooms + full list) | BUILT | `ContextTabs`. Rooms: *All work*, one per source, one per project the census finds, and stored contexts. |
| Top strip: running timer with target and elapsed time; start/stop; `⌘T` picker | BUILT | `TimerPicker`. One global timer. |
| Top strip: inbox count | BUILT | |
| Top strip: Assets entry with open-alert count | BUILT | |
| Top strip: one sync monogram per source, highlighted on a credential problem | BUILT | `Monogram`, credential health |
| Status bar: DB size, FTS freshness, sync cadence/countdown, counts, write-queue badge (pending / held / refused), latest-change line | BUILT | `StatusBar`, `WriteQueue`, `latest-change` |
| Write queue list: inspect every queued write; *Flush now*; for a held write: *Apply anyway* / *Edit* / *Discard*, with both versions side by side | BUILT | `pending_writes`, `apply_held_write`, `amend_write`, `discard_write`, `flush_writes` |
| Toasts with an optional action button | BUILT | |
| Split-flap cells for values that change while you watch | BUILT | `Flap`; plain mono for static values |
| Booting / database-retry screen | BUILT | `Booting`, `retry_database` |
| Paste a Jira/Confluence/Gitea/TeamCity URL anywhere and it becomes an entity chip (a stale link explains itself) | BUILT | `resolve_url` |
| Navigation history ("what was I doing before lunch") | LOW | Can be read from the activity stream (`recent_activity`) plus the passive observations. |

## 2. Launcher (`⌘K`)

| Function | Tag | Notes |
|---|---|---|
| Full-text search over every source + notes + assets, grouped by kind, with source monogram and sync age per row | BUILT | `search`, < 100 ms target |
| Prefixes: `>` actions/navigation · `#` tickets · `@me` / `@name` · `/ji` `/gt` `/tc` `/cf` (source) · `t ` time · `note:` · `list:` · `asset:` · `?` help card | BUILT | `launcher/syntax.json` is the full list |
| Inline filters: `kind:`/`type:`, `source:`/`src:`/`in:`, `updated:today\|7d\|2w`, `is:mine`, `author:`/`owner:`/`by:`, `under:<parent>` (everything below an epic, repo, page, build config or asset) | BUILT | `filter_suggestions` autocompletes them |
| Filter chips (source, kind, `@me`, time) and **coverage**: which sources could answer this filter and which have nothing to match | BUILT | `coverage.ts` |
| Rows show a path line (parent chain) and a state chip (PR open/merged, build outcome) | BUILT | |
| Empty query = **launcher board**: smart lists with counts and change badges, contexts, inbox preview, today's time, recent items, pending-writes footer | BUILT | `launcher_home` |
| Built-in smart lists: *Changed today*, *My items*, *Mine, untouched 14 days*, *Just synced*, *Not monitored*, *Open alerts in my contexts*, *Certificates expiring* | BUILT | `knobas-search/src/lists.rs` |
| *Save this search as a list*, rename, delete (max 8 saved); a list the current grammar refuses shows *needs attention* | BUILT | `create/rename/delete_smart_list` |
| `Tab` action chain on a result: *Link to ⟨open entity⟩* › *Add to ⟨current context⟩* › *Start timer* | BUILT | Only actions that apply are shown. |
| Wider action chain: *Change status*, *Comment*, *Start work*, *Open in browser* straight from a result | LOW | The ops exist; the chain only needs entries. |
| Spec catalogue lists: *My tickets with a failing build*, *PRs waiting on me*, *PRs idle > 5 days*, *Blocked tickets*, *Unlogged time this week* | LOW | Each is hand-written SQL over data that already exists (links, declared state, blocks). |
| Local code grep across clones | LATER | v2 backlog |

## 3. Rooms (the hub view)

| Function | Tag | Notes |
|---|---|---|
| Room = tiles handed the room's filter. Stored (context) rooms and derived rooms (*All work*, per source, per project) | BUILT | `Room`, `list_projects` |
| **Tickets tile = mini board**: status groups in the source's own words; **columns** layout for bounded rooms, **stacked** for unbounded ones; per-room override for the session; falls back to stacked past 6 columns | BUILT | `mini_board`, `MiniBoard` |
| **Code tile**, attention first: PRs asking for my review › my PRs › other open PRs › my branches without a PR › … ; default branch hidden; header counts | BUILT | #712 |
| **Builds tile**: latest build per build configuration, failing first | BUILT | |
| Docs, Notes tiles; a tile of its own for any other kind (open kinds) | BUILT | |
| Assets tile (the context's assets, through ancestors) | BUILT | `context_assets`, `source_assets` |
| Maximise one tile (`Esc` restores it) | BUILT | |
| Room action bar (*New note*, …) | BUILT | `RoomBar` |
| **Suggestion tray** "Might belong here": proposed links with a reason, *Confirm* / *Dismiss* (dismissals are remembered) | BUILT | `room_suggestions`, `accept/dismiss_suggestion` |
| Create a context (ad-hoc label); promote any ticket/entity to a context | BUILT | `create_context`, `promote_context` |
| Rename a context; archive / unarchive a context | LOW | `knobas.context.archived_at` exists and readers already respect it. Only the command and the button are missing. |
| Remove an explicit member from a context | LOW | It's an `unlink` of the `in-context` link; just needs surfacing in the room. |
| Context handoff bundle (markdown of everything linked) | LOW | `context_members` + `get_entity`; render and save as a note or file. |
| Adaptive primary action (*Start work on X* / *Trigger build* / *New note* depending on the context's state) | LOW | All the actions exist. Picking which one leads is UI logic. |

## 4. Detail (any entity)

Generic for every kind. A source may add its own **source view** (tabs of blocks plus a toolbar of actions).

| Function | Tag | Notes |
|---|---|---|
| Header: title, kind, source, sync age, open in browser | BUILT | `get_entity` |
| Withdrawn / source-disabled banners (two different ones) | BUILT | |
| Generic payload projection for kinds without a view | BUILT | `PayloadView` |
| **Links panel**: confirmed links grouped by relation, both ends, with status readings; *Link to…* dialog (relation picked or typed, optional note); unlink | BUILT | `create_link`, `unlink` |
| Relations offered: related, blocks/blocked by, implements, documents, deploys, runs-on/hosts, exposes, depends-on/needed by, monitored-by, in-context, mentions, plus your own | BUILT | |
| History panel (activity stream filtered to the entity) | BUILT | `recent_activity(entityId)` |
| **Builds line** (checks): mirrored builds of the head commit, failing first | BUILT | #710 |
| **Checkout panel** (repos/branches/PRs): which local clone, working-tree state (branch, ahead/behind, changed/untracked/conflicted), *Open in editor*, *Open terminal*, set path by hand | BUILT | `entity_checkout`, `open_checkout`, ADR-0021 (read-only git) |
| Start timer on this entity | BUILT | |
| Bulk linking (multi-select results → *Link selected to…*) | LOW | `create_link` per pair. The work is in the selection UI. |
| Drag-to-link between columns or rooms | LOW | Same command. AssetsView explicitly leaves dragging out today. |

### 4a. Per source: what each detail can do

| Source / kind | Read | Write (all through the queue) | Tag |
|---|---|---|---|
| **Jira ticket** | fields, body, comments, parent/epic, mini board column | *Status* select (only transitions reachable right now), *Comment*, *Log work* (worklog), *Create ticket* | BUILT |
| Jira: assignee, priority, estimate edits | read only | needs new `WriteOp` variants | LOW–LATER (one op each, plus a live test against the real Jira) |
| **Confluence page** | rendered storage body by section | *Comment*, **section edit** (text only; sections with macros or tables refuse and offer *Open in browser*), *Create page* | BUILT |
| **Gitea repo** | Overview (README), Files tab (tree at a branch, file view with *Back*), Commits, branches | *New branch…*, *New pull request…* | BUILT |
| **Gitea branch** | ahead/behind vs base, Files, commits, checks, "checked out locally" | *Open pull request…* (or a link to the existing one), *Delete branch…* (spells out whether unmerged commits are lost) | BUILT |
| **Gitea PR** | Overview (commit statuses, mergeable), Conversation, Diff with **line threads** (outdated ones grouped apart), Commits | *Comment…*, *Approve…*, *Request changes…*, line comments kept pending until *Submit review…*, *Edit…* (title/body), close/reopen, *Merge…* (style, optionally delete branch), *Update branch…* (merge/rebase) | BUILT |
| **Gitea commit** | diff, checks | *New branch…* from here | BUILT |
| **TeamCity build config** | Overview, Parameters, Triggers, builds | *Trigger*, *Run…* (with parameters), *Set parameter* | BUILT |
| **TeamCity build** | Overview, Log, Changes, Tests, Artifacts | *Re-run* | BUILT |
| **Uptime Kuma monitor** | state, type, 24 h bar (half-hour segments, gaps shown as gaps), uptime, cert days, attached assets | *Pause* / *Resume* (only with the account credential), *Create monitor for this asset* | BUILT |
| Build failure → *Create ticket* prefilled | LOW | `create_ticket` exists; this is a prefill plus a button. |
| Disabled action with the source's reason as tooltip; actions that can never apply are not drawn | BUILT | #701 ruling |
| TeamCity: cancel/stop a running build, pin, tag | LATER | New ops. |
| Gitea: tags, releases, forks, repo settings, resolve conversations, suggested changes, syntax highlighting | LATER | Out of scope in #701 |

## 5. Start work

| Function | Tag | Notes |
|---|---|---|
| From a ticket: reviewable stepper. Branch named from the ticket → PR with title/description → link PR ↔ ticket → ticket to *In Progress* → optional timer. Each step can be retried, skipped, or amended | BUILT | `start_work_flow/run/retry/skip/amend`, `#/start-work/<key>` |
| Reverse flow: PR merged → ticket to *In Review* | BUILT | `follow_merges` |

## 6. Notes and capture

| Function | Tag | Notes |
|---|---|---|
| Markdown notes, autosaved; `[[PAY-231]]` refs render as entity chips; backlinks | BUILT | `create/save/delete_note`, `NoteView` |
| *New note* in a room links it `captured-in` the context and `captured-from` the open entity | BUILT | |
| **Quick capture**: global shortcut opens a small window while another app has focus; note created on first keystroke | BUILT | `CaptureWindow`, `capture_shortcut` |
| Reveal a captured note in the main window | BUILT | `reveal_note` |
| Personal templates (ticket, page, PR description) | LATER | v2 backlog |

## 7. Inbox

| Function | Tag | Notes |
|---|---|---|
| One stream. Categories: review request, mention, failed build, new assignment, credential expiry, monitor alert | BUILT | `inbox_items` |
| Two shelves: *stream* and *snoozed* | BUILT | |
| Per row: open, actions where the op exists (*Approve*, *Re-run*, *Comment*), *Snooze* with presets + a date picker, *Done* | BUILT | `inbox/actions.ts`. Deliberately no bulk action. |
| Filter to one context (`#/inbox/ctx/<id>`) | BUILT | |
| Desktop notification per new inbox item, per category opt-in, only when the window is unfocused; clicking it opens the item | BUILT | `notification_kinds`, `notify` |
| Ack an alert (knobas-local; the alert stays open until the monitor recovers) | BUILT | `ack_alert` |

## 8. Time

| Function | Tag | Notes |
|---|---|---|
| One timer; target = any entity or an ad-hoc label (never a context) | BUILT | `start/stop_timer`, `current_timer` |
| On stop: **worklog draft** with the interval (editable), activity in the interval as checkboxes, generated comment, *Log Nh to PAY-xxx* | BUILT | `worklog_draft`, `log_work` |
| On stop on a non-ticket target: *Log an ad-hoc block* with a suggested ticket and the reason for it | BUILT | `ad_hoc_block` |
| **Day review** (`#/time/<date>`): manual vs passive blocks, gaps → *Assign…*, edit/delete/create blocks | BUILT | `day_blocks`, `update/delete/create_block` |
| **Week timesheet**: per target and day, tracked / offered / logged / unlogged / held; no rounding | BUILT | `week_timesheet` |
| *Log all* with preview: one worklog per day and ticket from unlogged manual blocks | BUILT | `log_all_preview`, `log_all` |
| Passive attribution (opt-in): records what was in front while focused; days older than the observation horizon read "no record" | BUILT | `passive_attribution` |
| Drag/merge blocks in the day strip | LOW | `update_block` / `delete_block` already cover it; the work is the interaction. |

## 9. Standup

| Function | Tag | Notes |
|---|---|---|
| **Digest**: yesterday (last active day ≤ 7 back) / today (incl. timer target) / blockers; every line links to its item | BUILT | `standup_digest` |
| **Protocol** note per date: attendees, per-person notes, action items | BUILT | `standup_protocol` |
| *Publish to Confluence* under a configured parent page (asked for on first publish) | BUILT | `publish_standup_protocol`, `PublishTargetPicker` |
| Action item → *Create ticket*, with its status followed | BUILT | `create_action_item_ticket` |
| Infra line in the digest (outages, restarts from alerts) | LOW | Alerts and samples are stored; the digest only needs to read them. |

## 10. Assets (estate)

| Function | Tag | Notes |
|---|---|---|
| **Tree** tab: Miller columns, spines when space runs out, `←→↑↓ Enter`, search reveals the path, wires from route rows to their far end (dashed when reached through something) | BUILT | `asset_tree`, `AssetsView` |
| Fixed asset pane: properties (typed + custom; secrets masked with Show/Hide), parent path, holds, exposes, reachable via, monitoring, linked work, **Depends on this**, history | BUILT | `get_asset`, `depends_on_this` |
| Create at any level (16 types, "usual here" ordering), edit, **move** (re-parent), delete | BUILT | `create/edit/move/delete_asset`, `CreateDialog`, `MoveDialog` |
| Routes: create/edit/delete, URL with or without a target asset | BUILT | `RouteDialog` |
| Health rollup (own + monitors, worst descendant), "N problems inside" badges | BUILT | |
| Environment and owner, inherited from the nearest ancestor | BUILT | |
| **Import** with preview (already in tree / new): estate file, **hcloud** producer, **Docker host** producer | BUILT | `preview_estate_import`, `produce_estate_file`, `apply_estate_import` |
| **Monitors** tab: state chips with counts, roster rows (24 h bar, last check, uptime, cert days, attached assets, open in Kuma), open-alert cards with *Ack* and *Go to asset*, **Not monitored** roster with a type filter, *Pause*/*Resume* | BUILT | `monitor_roster`, `open_alerts`, `unmonitored_assets` |
| Actions on an asset: open URL, copy SSH, link, add to context, start timer, create monitor | BUILT | |
| Environment matrix / drift view (stage ≠ prod) | LATER | Post-v1. A view over the env property; there's no version data without a runtime adapter. |
| Restart container, low-code runtime run/promote | LATER | Needs a Docker-host write adapter / a runtime adapter (§12.4 deferred). Deep-link to Portainer/Proxmox instead. |

## 11. Sources, sync, diagnostics

| Function | Tag | Notes |
|---|---|---|
| Sources view: list with status, credential health, last run, item counts | BUILT | `list_sources`, `sync_status`, `credential_health` |
| **Add source**: form generated from the adapter's schema → *Test connection* (with discovered config filled into empty fields and a connection note) → save | BUILT | `list_adapters`, `test_source`, `add_source`, `SchemaForm` |
| Adapters: Jira DC, Confluence DC, Gitea, TeamCity, Uptime Kuma, mock (demo) | BUILT | |
| Re-enter secret; add the Kuma **Account** (second credential) | BUILT | `set_source_secret`, `ReenterSecret` |
| Sync now (with progress), sync all | BUILT | `sync_now_with_progress`, `sync_all` |
| Remove a source, with or without purging its items | BUILT | `delete_source` |
| Diagnostics: per-source run list (durations, upserted, errors), FTS state, *Re-index*, DB size | BUILT | `list_sync_runs`, `reindex_fts`, `db_stats` |
| **Edit an existing source's config** (URL, schedule, enabled, fields) | BACKEND | `update_source` exists, no UI calls it. |
| **Backfill** a source (full re-fetch after the payload widened) | BACKEND | `backfill_source` exists, no button. |
| First-run wizard → add first source → initial sync → launcher; or load the demo dataset | BUILT | `FirstRun`, `demo_load`, `complete_first_run` |
| New adapters (GitHub, GitLab, Jenkins, Proxmox …) | LATER | One crate each; certified against a real instance (ADR-0013). |

## 12. Settings

| Section | Tag | What's in it |
|---|---|---|
| Backup | BUILT | Backup now, schedule, retention, restore from file, **share export** with toggles (links, assets, contexts, sources, smart lists, notes off, time off) |
| Capture | BUILT | Global shortcut |
| Checkouts | BUILT | Clones root, editor/terminal command templates |
| Monitoring | BUILT | Response-time threshold (warn), sample retention |
| Notifications | BUILT | Desktop notification per inbox category |
| Passive attribution | BUILT | On/off |
| Standup | BUILT | Publish target |
| Logs | BUILT | Open the app's log folder |

---

## 13. Backend ready, UI missing (the quickest wins)

1. `update_source`: edit a source after it was added.
2. `backfill_source`: a *Backfill* button in diagnostics.
3. Context rename / archive / unarchive (the column exists; one command).
4. Remove an explicit context member (the unlink already exists).
5. Wider launcher `Tab` chain (status, comment, start work, open in browser).
6. The spec's remaining built-in smart lists (SQL over existing data).
7. Build failure → *Create ticket* prefill.
8. Bulk linking from launcher results.
9. Context handoff bundle as a note.
10. Infra line in the standup digest.
11. Navigation history from the activity stream.
