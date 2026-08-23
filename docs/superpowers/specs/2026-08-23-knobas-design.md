# knobas — design document (feature walkthrough)

Status of this document: **draft for review**, 2026-08-23. It collects every feature that has been decided, explored in a mockup, or proposed, so you can go through it and mark each one *keep / change / drop*. Nothing here is implemented in Rust yet.

Legend for the status tags:
- **Decided** — you said yes; it will be built as described.
- **In mockup** — exists in a clickable mockup; the behaviour is a proposal until you confirm it.
- **Proposed** — suggested, not yet in a mockup.
- **Open** — needs your answer; the question is stated.

Mockups referenced: round 1 `mockups/round-1/` (25 paradigm × design cells), round 2 `mockups/round-2/` (Signal shell + three asset ways), round 3 `mockups/round-3/signal-miller.html` (the integrated view), playground `mockups/playground/` (E1–E4 depth explorers).

---

## 1. What knobas is

A desktop app (Rust + Tauri) that puts the tools of a working day — Jira-like tickets, Confluence-like pages, Gitea-like repos, TeamCity-like pipelines, Uptime-Kuma-like monitoring, a Flowrun-like low-code runtime, and your own infrastructure — into **one window with one search, one set of links, one clock and one inbox**. It keeps a **local copy** of everything it syncs (Postgres with full-text search) so search is instant and can ask questions the source systems can't, and it writes back (comments, status changes, worklogs, builds, branches, pages) so you rarely open the originals.

Principles that came out of the rounds:
1. **Context first.** You always work *on something*; the app should know what, and everything should be one click from it. **Decided** (Context hub paradigm).
2. **One box.** Typed search with prefixes and filters reaches everything, including actions. **Decided** (Launcher box).
3. **Links are the product.** Ticket ↔ branch ↔ PR ↔ build ↔ page ↔ note ↔ asset ↔ monitor ↔ context. Made by hand anywhere, suggested automatically, confirmed in one click — and **stored locally, never written into the source systems**. **Decided 2026-08-23.** See §5a.
6. **Your data is a file.** Everything knobas owns (links, contexts, notes, assets, time, smart lists, source configs without secrets) is exportable and importable in plain formats. **Decided 2026-08-23.** See §14.
4. **Nothing is keyed to git branches** except git itself. Time and context follow what you have open, not HEAD. **Decided**.
5. **Quiet instrument panel.** Signal design: graphite, hairlines, mono readings, amber only where something moves or is wrong. **Decided** (no ticker strip — **Decided 2026-08-23**).

---

## 2. Shell

| Feature | Status | Notes |
|---|---|---|
| **Context switcher** in the top strip: tabs for recent contexts, dropdown with all + smart lists, *+ new* (ad-hoc label) | In mockup (R2 base, R3) | Contexts are epics, tickets, or ad-hoc labels ("Staging DB configuration"). |
| **Room** per context: tiles Tickets (mini board) · Code (branches/PRs/commits) · Builds · Docs · Notes · Time-in-this-context · Assets | In mockup | Every tile is filtered by the context. Tile grid changed in every R2 way; R3 settles it. |
| **Persistent top strip**: search field (`⌘K`), timer with context + elapsed, inbox count, Assets with open-alert count, sync monograms (one per source, 401 highlighted), Today / Day buttons | In mockup | The amber **ticker** from Signal is **removed**. |
| **Status bar**: DB size, FTS freshness, sync cadence, counts, pending writes, user, clock | In mockup | |
| **Detail slide-over** (right half of the room) for any entity; `Esc` unwinds | In mockup | |
| **Split-flap cells** for values that change (status, build state, timer minutes, inbox count, versions) | In mockup | Signal signature; keep or tone down? **Open** |
| Keyboard: `⌘K` search, `⌘T` timer, `Esc` unwind, visible focus, reduced-motion respected | Decided | |
| Window: desktop, 1440×900 design target, usable ≥ 1100 wide | Decided | |

---

## 3. Sources and sync

| Feature | Status | Notes |
|---|---|---|
| **One configuration per source**; sources are pluggable adapters with a declared capability set (search / write / webhooks / import) and version | Decided | Adapters seen so far: Jira, Confluence, Gitea, TeamCity, Uptime Kuma, Flowrun; listed as available: GitHub, GitLab, GitLab CI, Jenkins, generic git, Proxmox VE, Docker host, Traefik. |
| **Auth methods**: user + password, PAT, API token, (OAuth later) | Decided | Stored in the OS keychain. |
| **Add source** flow: type → URL → auth → *Test connection* → sync schedule → save | In mockup | |
| **Sync schedule** per source (default every 5 min); *Sync now* | In mockup | |
| **Credential health**: PAT expiry countdown, 401 detection → *Re-enter password*, reminder in inbox (snoozable) | In mockup | |
| **Offline / failed-source write queue**: edits made while a source is 401/offline queue as "pending writes", flush after re-auth; conflicts shown | In mockup | Conflict UI not designed yet. **Open**: merge strategy. |
| **Local database**: Postgres, full-text index over everything synced, provenance ("synced 4 min ago") on every item | Decided | **Open**: user-installed Postgres as a prerequisite vs. bundled/embedded. |
| **Import adapters** (Proxmox, Docker host, Traefik): preview what would be imported, imported assets stay editable | In mockup (R2) | |

---

## 4. Search (the launcher box)

| Feature | Status | Notes |
|---|---|---|
| Full-text search over all synced sources + local notes + assets, results grouped by type with source monogram and sync age | Decided | |
| **Prefixes**: `>` actions · `#` tickets · `@` people · `/` source · `t ` time · `note:` · `list:` · `asset:` · `?` help | In mockup | |
| **Filter chips**: per source, per type, `@me`, `today`, cross-source chips (e.g. *has failing build* — a join JQL can't express); asset chips `type:` `env:` `health:` | In mockup | |
| **Empty query** shows smart lists, contexts, inbox preview, today's time, recent items | In mockup | |
| **Tab → action chain** on a result (Change status › Comment › Link to… › Add to context › Start timer › Start work › Open) with breadcrumb chips | In mockup | |
| *Add to <context>* on every result | In mockup | |
| **Smart lists** = saved local queries with counts and change badges: *My tickets with a failing build*, *PRs waiting on me*, *PRs idle > 5 days*, *Pages I edited this week*, *Blocked tickets*, *Unlogged time this week*, *Assets with open alerts in my contexts*, *Drifted between stage and prod*, *Certificates expiring < 30 days*, *Assets with no monitor* | In mockup | **Open**: a query language for defining new ones (SQL-ish vs. a builder). |
| Local code search across cloned repos from the same box; open in editor / terminal | Proposed | |
| Paste a Jira/Confluence/Gitea URL anywhere → resolves to an entity chip | Proposed | |

---

## 5. Work items (tickets, PRs, builds, pages, notes, commits, branches, repos)

| Feature | Status | Notes |
|---|---|---|
| Detail views per type with inline edit: ticket status/priority/assignee/comment; PR approve/comment; build log excerpt, re-run, trigger with parameters; page section edit; note editor | In mockup | Page editing in-app is a stub in most mockups. **Open**: full Confluence editor vs. "edit in browser". |
| **Links panel** on every item: confirmed links grouped by type, with status readings (build failed, PR 1/2 approvals) | Decided | Details in §5a. |
| **Linked assets** section on work items; *Link asset…*; *Open in Assets* | In mockup (R3) | |
| Worklogs, estimate, time spent on tickets | In mockup | |
| Write-back targets: Jira (status, comment, worklog, create), Confluence (page create/edit, publish protocol), Gitea (branch, PR, comment, approve, create repo, pull/push), TeamCity (trigger, re-run), Uptime Kuma (create/pause/ack monitor), Flowrun (run, promote) | Decided | |

---

## 5a. Links — the core object

Links are what knobas adds that no source system has. They are **local**: created and stored in knobas, never pushed into Jira, Confluence, Gitea, TeamCity, Uptime Kuma or Flowrun (those systems keep whatever native links they already have; knobas reads those as *source links* and shows them alongside). **Decided 2026-08-23.**

| Feature | Status | Notes |
|---|---|---|
| **A link** = `from` (any entity) · `to` (any entity) · `relation` · `origin` (manual / suggested-confirmed / imported / source) · `created by` · `created at` · optional note | Decided | Entities are addressed by stable ids: source + key/URL for synced items (`jira:PAY-231`, `gitea:tidewater/payout-service#142`, `confluence:ENG/SEPA payout retry design`), local ids for notes/assets/contexts. |
| **Manual linking everywhere**: *Link to…* on every detail, in the Tab action chain of every search result, by drag where a view supports it (columns, rooms), by typing `[[…]]` in notes; relation picked from a list or typed | Decided | |
| **Relation types** (open list): related · blocks / blocked by · implements · documents · deploys · runs-on · hosts · exposes · depends-on · monitored-by · in-context · mentions; users can add their own | Decided | |
| **Suggestions**: detected from keys in commit messages / branch names / build parameters / page text / scenario descriptions, image tags matching builds, host names matching services, text similarity via FTS, and native links in the sources; each shows its reason; Confirm / Dismiss; dismissed pairs are remembered and not re-proposed | Decided | Suggestions are stored too, with their reason and state. |
| Links are **bidirectional in the UI** (every panel shows both ends), and **transitive for contexts** (a context's members are what it links to, plus what those link to one hop out — configurable) | In mockup | Hop depth: **Open** |
| Backlinks on notes and assets | In mockup | |
| Link history: who linked what when; unlink keeps a tombstone so an import can't resurrect it | Proposed | |
| Bulk linking: multi-select in search results / columns → *Link selected to…* | In mockup (R2 W1) | |

---

## 6. Notes

| Feature | Status | Notes |
|---|---|---|
| Local markdown notes; `[[PAY-231]]`-style links render as entity chips; backlinks panel | Decided | |
| *New note* pre-linked to the current context | In mockup | |
| Quick capture (global hotkey) with current context attached | Proposed | |
| Notes as timer context | Decided | |

---

## 7. Contexts

| Feature | Status | Notes |
|---|---|---|
| Kinds: **epic** (members = its tickets and everything linked), **ticket** (focused), **ad-hoc label** (notes, time, links, assets — no code required) | Decided | |
| Membership = linked items + explicit *Add to context*; rooms aggregate across sources | Decided | |
| Per-context: clock (time in this context), inbox filter ("3 here"), assets, notes | In mockup | |
| Switching context offers to switch the timer | In mockup | |
| Any ticket can be promoted to a context | In mockup | |
| Context handoff bundle (markdown of everything linked) | Proposed | |

---

## 8. Inbox

| Feature | Status | Notes |
|---|---|---|
| One queue: review requests, @mentions (Jira/Confluence), failed builds on your work, new assignments, blocked tickets, monitor alerts (down / warning / cert expiry), credential expiry | Decided | |
| Actions per item: Open · Reply · Review · Re-run · Comment on ticket · Ping · Restart container · Ack · Snooze (returns on a date) · Done | In mockup | |
| "In this context" filter; count in the top strip; acting on an item records a line in the activity stream | In mockup | |
| Desktop notification when a watched build/monitor changes | Proposed | |

---

## 9. Time tracking

| Feature | Status | Notes |
|---|---|---|
| **One global timer**, `⌘T`, visible everywhere, context = any entity (ticket, page, note, repo, **asset**) or an ad-hoc label | Decided | |
| **No git-branch awareness** | Decided | |
| **Worklog draft** on stop: interval (editable), target ticket (optional), activity in the interval as checkboxes (commits, PR comments, builds triggered, pages/notes edited), generated comment, *Log Nh to PAY-xxx* → Jira worklog + local copy | Decided | |
| **Passive attribution** (opt-in): app records which entity/label was open → **Day review** strip: passive vs. manual blocks, unattributed gaps → *Assign…*, ad-hoc blocks → *Log to a ticket… / Keep local*, *Log all* | Decided | Drag/merge of blocks: nice-to-have. |
| **Week timesheet**: tracked / logged / unlogged per day and per context | In mockup | |
| Per-context clocks; switching context prompts to move the timer | In mockup | |

---

## 10. Standup

| Feature | Status | Notes |
|---|---|---|
| **Digest** (yesterday / today / blockers) generated from commits, worklogs, transitions, comments, alerts; every line traceable to its source item | Decided | |
| **Standup protocol** document (attendees, per-person notes, action items); editable; *Save as note* / *Publish to Confluence* (ENG › Standup protocols › date); action item → *Create ticket* | Decided | |
| Infra line in the digest (outages, restarts) | In mockup (R3) | |

---

## 11. Actions

| Feature | Status | Notes |
|---|---|---|
| **Start work on a ticket**: branch named from the ticket → push → PR with ticket title/description → link PR back → ticket → In Progress → (optionally) start the timer; shown as a reviewable stepper | Decided | Reverse: PR merged → ticket → In Review. |
| Trigger build with parameters; re-run failed; view log | Decided | |
| Create ticket / page / repo / branch; pull / push | Decided | |
| Build failure → comment on ticket / create ticket / re-run | Proposed | |
| Runbook page → interactive checklist, progress stored locally | Proposed | |
| Open in editor (VS Code / JetBrains) / terminal at the checkout | Proposed | |

---

## 12. Assets (new in round 2 / 3)

### 12.1 Model
| Feature | Status | Notes |
|---|---|---|
| **Any asset can hold assets, without limit** (site › hypervisor › VM › engine › container › runtime › app › step › connector; DB server › database › schema › table). Types are open; conventions only suggest children | Decided (2026-08-23) | From the playground. |
| **Types** with monograms: site, hypervisor, VM, container engine, container, service, module, runtime, app/scenario, step, connector, database server, database, schema, table, reverse proxy, middleware, network, custom | In mockup | |
| **Typed properties** per type + **custom properties** (text / number / date / url / secret; secrets masked) | Decided | |
| **Routes**: any asset can *expose* routes (URL → target asset, or an endpoint with no target); an asset is *reachable via* routes that land on it or on something that holds it | Decided | Shown from both ends. |
| **Relations** beyond containment: depends-on, monitored-by, deployed-from (repo/build), documented-in (page), in-context, linked-to (ticket/PR/note) | Decided | |
| **Status** up / warn / down / none; problems roll up so a closed branch shows "N problems inside" | Decided | |
| Change history per asset | In mockup | |

### 12.2 UI
| Feature | Status | Notes |
|---|---|---|
| **Miller columns** as the Assets view: one column per level, older columns collapse to labelled spines, `←→↑↓ Enter`, search reveals the path, wires from route rows to their targets/spines, `+` on every column header | Decided (E1 chosen) | Rejected: nested boxes (topology-explorer), semantic zoom, expanding graph, drill-in stack; R2's inventory board, topology map, environment matrix remain as references. |
| Asset detail pane: properties, held-by path, holds, exposes, reachable via, monitoring, linked work, actions, history | In mockup (R3) | |
| Create at any level; import from a source (preview) | Decided | |
| *Link to…* from an asset to work items / contexts / other assets with relation label; suggested asset links with reasons | Decided | |
| Assets in the launcher (with path), in rooms (ASSETS tile), on the timer, in the inbox (alerts), in smart lists | Decided | |
| Actions on an asset: open URL, copy SSH, open in Proxmox / Portainer, restart container, create monitor, add to context, start timer | In mockup | Which of these write back for real: **Open** (restart container needs a Docker host adapter). |
| Environment matrix / drift view (stage ≠ prod) | In mockup (R2 W3) | Keep as a secondary view? **Open** |
| Blast radius ("what breaks if this goes down") | In mockup (R2 W2) | Worth keeping as an action on a VM? **Open** |

### 12.3 Monitoring (Uptime Kuma adapter)
**knobas does not own monitoring.** Uptime Kuma stays the system of record for monitors, checks, notifications and status pages; knobas syncs from it, attaches monitors to assets and contexts, and routes alerts into the inbox. **Decided 2026-08-23.**

| Feature | Status | Notes |
|---|---|---|
| Adapter reads monitors, current state, response time, uptime, heartbeat history and cert expiry from Uptime Kuma (socket.io API for full data; `/metrics` with an API key for cheap state polling) | Decided | Uptime Kuma has no official REST API; the adapter speaks the socket.io protocol the web UI uses (as `uptime-kuma-api` does). |
| Monitor types shown as Uptime Kuma defines them (HTTP, TCP, ping, Docker, cert expiry, …); states up / down / pending / warning (response-time threshold is knobas-side) | Decided | |
| Monitors attach to assets (and through them to contexts); alerts → inbox with Open asset / Ack / Snooze / Restart; Ack marks the alert on the asset | Decided | |
| Write-back limited to what Uptime Kuma exposes: *Create monitor for this asset*, pause/resume, (ack is knobas-local — Uptime Kuma has no ack); deep link to the monitor and to the status page | In mockup | |
| Smart list: assets with no monitor | In mockup | |

### 12.4 Low-code runtime (Flowrun-style adapter)
| Feature | Status | Notes |
|---|---|---|
| Source adapter syncs runtime **instances** per environment (dev/stage/prod: version, host, health) and their **scenarios** (per-env version, trigger, last run, runs today, log) into assets under the runtime | Decided | |
| Actions: *Run now*, *View log*, **Promote** scenario between environments with a review dialog (version diff, linked ticket, stage run status, compatibility) and optimistic result | Decided | |
| Scenarios link to tickets/pages and to the container/VM they run on; steps and connectors are assets below them | Decided | |
| Drift between environments surfaced (smart list, matrix view) | In mockup | |

---

## 13. Secondary features (proposed, not yet in a mockup)

- Desktop notifications (build finished, monitor changed, PAT expiring).
- Quick capture hotkey; paste-URL → entity chip.
- Local code grep across clones; open in editor / terminal.
- Runbook → checklist; build-failure → ticket/comment shortcuts.
- Context handoff bundle (markdown).
- Conflict resolution UI for queued writes.
- Personal templates (ticket, page, PR description).
- Navigation history ("what was I doing before lunch") feeding the day review.

---

## 14. Non-functional

| Topic | Decision / proposal |
|---|---|
| Platform | Tauri 2 desktop app; Rust backend; frontend framework chosen to port the winning mockup (vanilla → Svelte/Solid/React all viable). **Open**. |
| Storage | Postgres (local), full-text search via its FTS; one schema for entities, links, notes, assets, worklogs, activity; per-source raw payload kept for re-mapping. |
| Secrets | OS keychain via Tauri; never in the DB; masked in the UI. |
| Performance | Local search < 100 ms for ~100 k items; sync incremental per adapter; UI never blocks on a source. |
| Offline | Everything read works offline; writes queue. |
| **Export / import** | Everything knobas owns is exportable and importable: links (with relation, origin, reason, dismissed state), contexts and memberships, notes (markdown files with front-matter), the asset tree with properties/routes/relations (YAML or JSON), worklogs and day blocks, smart lists, source configurations **without secrets**. Format: a folder of plain files (`links.jsonl`, `contexts.yaml`, `assets.yaml`, `notes/*.md`, `time.jsonl`, `lists.yaml`, `sources.yaml`) that diffs well and can live in git; a single `.knobas.zip` wraps it. Entity references use the stable ids above so an export re-imports on another machine or after a re-sync. Import = merge by id with a preview (added / changed / conflicting), never a blind overwrite. Synced source data is *not* part of the export (it is re-synced) except as an optional snapshot. **Decided 2026-08-23.** |
| Accessibility | Keyboard-complete, visible focus, reduced motion, contrast ≥ 4.5:1 on readings. |
| Privacy | Passive time attribution is opt-in and local only. |

## 15. Architecture direction (to validate in the implementation plan)

Rust workspace: `knobas-core` (entities, links, contexts, notes, time), `knobas-db` (sqlx + Postgres, FTS, migrations), `knobas-source-*` (one crate per adapter behind a `Source` trait: config schema, auth, incremental sync, search mapping, write-back, capabilities), `knobas-assets` (tree, routes, relations, monitors), `knobas-git` (gix/git2: clone, pull, push, branch), `knobas-app` (Tauri commands, events, keychain). Frontend talks to commands only; all sync runs in the backend on a schedule.

---

## 16. Open questions (answer inline or in chat)

1. Postgres: require a local install, or bundle/embed?
2. Split-flap cells: keep as the Signal signature, or reduce to the timer and build state only?
3. Smart lists: how should you define new ones — a small query language, a builder, or both?
4. Page editing: in-app editor for Confluence, or "open in browser" plus section-level edits only?
5. Which asset actions must really write back in v1 (restart container, create monitor, promote scenario)?
6. Keep the environment-matrix and blast-radius views as secondary asset views, or drop them?
7. Conflict handling for queued writes: last-write-wins with a diff, or always ask?
8. ~~Monitoring: Uptime Kuma only for v1, or also a Prometheus/Alertmanager adapter?~~ **Answered 2026-08-23: Uptime Kuma is the monitoring system; knobas syncs from it and does not own monitoring.** A Grafana/Alertmanager adapter is a later option, not v1.
9. Frontend framework preference (Svelte / Solid / React / keep vanilla)?
10. Anything from §13 that should move up into v1?
11. Context membership: direct links only, or one hop out (ticket's PR's build counts)? Configurable per context?
12. Export: should a shared export (to a colleague) include notes by default, or links/assets/contexts only?

## Appendix — mockup map

- Round 1: `mockups/round-1/index.html` — 5 paradigms × 5 designs; chosen: P4 Context hub + P1 Launcher, D1 Signal (D3 Patchbay liked for later).
- Round 2: `mockups/round-2/index.html` — base shell + W1 Inventory, W2 Topology, W3 Environment matrix.
- Playground: `mockups/playground/` — topology-explorer (rejected), E1 Miller columns (chosen), E2 Semantic zoom, E3 Expanding graph, E4 Drill-in stack.
- Round 3: `mockups/round-3/signal-miller.html` — the integrated view this document describes.
- Briefs and data: `mockups/shared/` (dataset, assets, screens, paradigms, designs, round briefs).
