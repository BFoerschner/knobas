# knobas — design document (feature walkthrough)

Status of this document: **draft for review**, 2026-08-23, **revised 2026-08-24**. It collects every feature that has been decided, explored in a mockup, or proposed, so you can go through it and mark each one *keep / change / drop*. Nothing here is implemented in Rust yet.

## Revision 2026-08-24 (Claude) — what changed, veto per line

Per your go-ahead ("add or modify features as you see fit"), this revision was made autonomously. Everything new is tagged **Rec 08-24** (= recommended, not user-decided) so you can scan and override. Summary:

1. **Open questions §16 all answered with recommendations** (embedded Postgres, flaps only on live values, smart lists = saved searches first, Confluence section-edit only, v1 write-back list, matrix/blast-radius post-v1, conflicts ask-with-diff, Svelte 5, backlog promotions, 1-hop membership, export defaults).
2. **Rules the round-3 mockup implements but the doc never stated** are now written down: entity addressing (§2), health rollup / env / owner inheritance (§12.1), alert routing + ack semantics (§12.3), Monitors & Flowrun as first-class tabs (§12.2), launcher details (§4).
3. **Three mockup design flaws fixed in the spec, not carried over**: links get one uniform record for all pairs incl. relation (§5a); environment becomes a stored/inherited property, not id-string guessing (§12.1); asset search must match ancestor paths (§4).
4. **New: §2a Activity stream** (first-class — it already feeds standup, day review, and inbox history), **§14a first-run + demo seed mode + mock source** (the Tidewater dataset becomes the dev/test fixture — this is what lets parallel agents build UI without live sources), **diagnostics view** (§3).
5. §13 backlog re-tiered; §15 architecture extended (sync engine, mock source crate, IPC contract-first).

Companion documents: `docs/roadmap.md` (MVP, milestones, parallel workstreams); task-level tracking is GitHub Issues (the executed superpowers-era plan files are in git history).

Legend for the status tags:
- **Decided** — you said yes; it will be built as described.
- **In mockup** — exists in a clickable mockup; the behaviour is a proposal until you confirm it.
- **Proposed** — suggested, not yet in a mockup.
- **Open** — needs your answer; the question is stated. *(As of 08-24 nothing carries this tag — every open question got a recommendation, tagged as below.)*
- **Rec 08-24** — recommended by Claude on 2026-08-24 under your "as you see fit" delegation; treated as decided for planning, but yours to override on review. **Ratified 2026-08-27 (Björn) — all twelve §16 entries, with two amendments recorded in §16.**

Mockups referenced: round 1 `mockups/round-1/` (25 paradigm × design cells), round 2 `mockups/round-2/` (Signal shell + three asset ways), round 3 `mockups/round-3/signal-miller.html` (the integrated view), playground `mockups/playground/` (E1–E4 depth explorers).

---

## 1. What knobas is

A desktop app (Rust + Tauri) that puts the tools of a working day — Jira-like tickets, Confluence-like pages, Gitea-like repos, TeamCity-like pipelines, Uptime-Kuma-like monitoring, a Flowrun-like low-code runtime, and your own infrastructure — into **one window with one search, one set of links, one clock and one inbox**. It keeps a **local copy** of everything it syncs (Postgres with full-text search) so search is instant and can ask questions the source systems can't, and it writes back (comments, status changes, worklogs, builds, branches, pages) so you rarely open the originals.

Principles that came out of the rounds:
1. **Context first.** You always work *on something*; the app should know what, and everything should be one click from it. **Decided** (Context hub paradigm).
2. **One box.** Typed search with prefixes and filters reaches everything, including actions. **Decided** (Launcher box).
3. **Links are the product.** Ticket ↔ branch ↔ PR ↔ build ↔ page ↔ note ↔ asset ↔ monitor ↔ context. Made by hand anywhere, suggested automatically, confirmed in one click — and **stored locally, never written into the source systems**. **Decided 2026-08-23.** See §5a.
4. **Nothing is keyed to git branches** except git itself. Time and context follow what you have open, not HEAD. **Decided**.
5. **Quiet instrument panel.** Signal design: graphite, hairlines, mono readings, amber only where something moves or is wrong. **Decided** (no ticker strip — **Decided 2026-08-23**).
6. **Postgres is the one store.** Everything — the synced copy of every source *and* everything knobas owns (links, contexts, notes, assets, time, smart lists, source configs minus secrets) — lives in one local Postgres database. Export and import are a dump and restore of that database's contents, nothing more. **Decided 2026-08-23.** See §14.

---

## 2. Shell

| Feature | Status | Notes |
|---|---|---|
| **Context switcher** in the top strip: tabs for recent contexts, dropdown with all + smart lists, *+ new* (ad-hoc label) | In mockup (R2 base, R3) | Contexts are epics, tickets, or ad-hoc labels ("Staging DB configuration"). The switcher also lists derived rooms: *All work*, one per configured source, and from **M2.6** one per project a source's corpus shows, under that source's own room — see the §13 addendum (2026-08-31) and ADR-0010. |
| **Room** per switcher entry: tiles Tickets (mini board) · Code (branches/PRs/commits) · Builds · Docs · Notes · Time-in-this-context · Assets | In mockup | Every tile is handed the room's whole filter, of which context is one nullable dimension — a derived room (*All work*, a source room, a project room) has none. Tile grid changed in every R2 way; R3 settles it. The Tickets tile shipped in M1 as a flat recency list; the mini-board promise is fulfilled by milestone **M2.5** — see the §13 addendum (2026-08-30). Milestone **M2.6** then gives the mini board two layouts and lets the room choose between them — see the §13 addendum (2026-08-31). |
| **Persistent top strip**: search field (`⌘K`), timer with context + elapsed, inbox count, Assets with open-alert count, sync monograms (one per source, 401 highlighted), Today / Day buttons | In mockup | The amber **ticker** from Signal is **removed**. |
| **Status bar**: DB size, FTS freshness, sync cadence, counts, pending writes, user, clock | In mockup | |
| **Detail slide-over** (right half of the room) for any entity; `Esc` unwinds | In mockup | **Two detail idioms are intentional** (R3): slide-over for work items; assets use a fixed right pane inside the Miller view so the column path stays visible. **Rec 08-24: keep both.** |
| **Entity addressing**: every entity has exactly one stable in-app address (`#/ctx/<id>`, `#/ticket/<key>`, `#/asset/<id>`, `#/route/<id>`, `#/monitor/<id>`, `#/inbox`, `#/time`, `#/standup`, `#/sources`, `#/settings`, `#/assets/{board\|monitors\|flowrun}`, `#/start-work/<key>`); opening an asset address re-opens the columns at its path | In mockup (R3) | Was implemented but never specified. **Rec 08-24: adopt as the navigation contract** — it's also what paste-URL chips, notifications, and inbox items deep-link to. Branch/repo get addresses too (dead ends in R3). |
| **Adaptive room action bar**: the primary action follows the context's state (*Start work on X* / *Trigger build* / *New note*) | In mockup (R3) | |
| **Suggestion tray** "Might belong here" per room: work-item + asset suggestions with reasons, Confirm / Dismiss | In mockup (R3) | Feeds from the §5a suggestion store. |
| **Toasts** with optional action button (*Fix now*, *Open PR #145*); status bar carries a "latest change" one-liner (the ticker's quiet replacement) | In mockup (R3) | |
| **Split-flap cells** | In mockup | **Rec 08-24: flaps only for values that change while you watch** — timer, sync countdown, build/monitor state transitions, inbox/alert counts. Static readings (versions, ids, dates) are plain mono. Disabled under reduced motion. Cuts port cost, keeps the signature. |
| Keyboard: `⌘K` search, `⌘T` timer, `Esc` unwind, visible focus, reduced-motion respected | Decided | `Esc` unwind ladder as in R3 (chain → query → launcher → overlay → detail → column step → room). `⌘T` on the assets board times the selected asset. |
| Window: desktop, 1440×900 design target, usable ≥ 1100 wide | Decided | |

---

## 2a. Activity stream (Rec 08-24 — promoted to first-class)

A local, append-only log of (a) every action taken through knobas (status change, comment, link created, worklog sent, build triggered, restart, ack …) and (b) notable synced events on your work (build finished, PR approved, monitor changed). Each line: when, actor, verb, entity refs, origin (user / sync).

Why first-class: two already-decided features are views over exactly this data — the day review's passive attribution, and the inbox's "acting on an item records a line" — and a third, the standup digest ("built from 3 commits, 2 worklogs …"), reads it together with the mirror. **Corrected 2026-09-03 (#272):** the digest is drawn from the mirror and the activity stream for the configured usernames; sync writes no per-event lines, so the stream alone cannot carry it (the `CONTEXT.md` **Digest** term). The mockup writes such lines ad-hoc; the real app should have one table and one writer. Also gives the status-bar "latest change" line and, later, "what was I doing before lunch" (§13) for free.

---

## 3. Sources and sync

| Feature | Status | Notes |
|---|---|---|
| **One configuration per source**; sources are pluggable adapters with a declared capability set (search / write / webhooks / import) and version | Decided | Adapters seen so far: Jira, Confluence, Gitea, TeamCity, Uptime Kuma, Flowrun; listed as available: GitHub, GitLab, GitLab CI, Jenkins, generic git, Proxmox VE, Docker host, Traefik. Extensibility guarantees: §3a. |
| **Deployment flavor per source: `datacenter` (self-hosted) is the primary target** — Björn's real Jira/Confluence are self-hosted. Jira adapter speaks **DC REST v2** (`/rest/api/2/search`, `startAt` pagination — the Cloud `/search` removal never happened on DC); Confluence adapter speaks **DC REST v1** (`/rest/api/content` + CQL; the Cloud v2 API doesn't exist on DC). `cloud` is a config flavor to add later | Decided (Björn 08-24) | Contract sources vendored in `testenv/specs/` (official Jira DC WADL; **Atlassian publishes no machine-readable Confluence DC spec** — its contract is the official docs + validation against the real `atlassian/confluence` container, available with free dev licenses behind an opt-in compose profile). Pin WADL + container tags to the real instance versions once known. |
| **Auth methods**: user + password, PAT (DC: Bearer, supported since Jira 8.14 / Confluence 7.9), API token, (OAuth later) | Decided | Stored in the OS keychain. |
| **Add source** flow: type → URL → auth → *Test connection* → sync schedule → save | In mockup | |
| **Sync schedule** per source (default every 5 min); *Sync now* | In mockup | |
| **Credential health**: PAT expiry countdown, 401 detection → *Re-enter password*, reminder in inbox (snoozable) | In mockup | |
| **Offline / failed-source write queue**: edits made while a source is 401/offline queue as "pending writes", flush after re-auth; conflicts shown | In mockup | **Rec 08-24 (answers Q7):** the queue is inspectable (a list, not just a count — R3 only had the integer). Before flushing, the adapter re-reads the target; if it changed since the edit was queued, the write is held and shown as both-versions-side-by-side → you pick (apply anyway / discard / edit). No silent last-write-wins in v1; a per-source "just apply my version" toggle can come later. |
| **Work adapters vs asset-import adapters** are listed separately in Sources (Jira/Gitea/… vs Proxmox/Docker host/Traefik); asset adapters show configured / "not configured — import creates editable assets" | In mockup (R3) | |
| **Diagnostics** (Rec 08-24): per-source sync log with errors, last-run durations, item counts, FTS index state, re-index button, DB size | Proposed | The status bar shows the summary; this is where you look when a sync misbehaves. Cheap to build, saves debugging pain later. |
| **Local database**: one Postgres database holds the synced copy of every source (with provenance, "synced 4 min ago") **and** all knobas-owned data (links, contexts, notes, assets, time, smart lists, source configs); full-text index over all of it | Decided | Owned tables and synced tables are separate schemas (`knobas` / `sync`) so a dump can include or exclude the cache. **Rec 08-24 (answers Q1): embedded Postgres via the `postgresql_embedded` crate** (v0.21, pinned PG 18.6, native arm64) — downloads once on first run (~13 MB), `initdb` 2.6 s once, starts in ~0.12 s, idles at ~21 MB, ships `pg_dump`/`pg_restore`/`pg_upgrade` (export/import comes free). TCP on 127.0.0.1 (the macOS socket-path length limit bites under `~/Library/Application Support`). A settings field accepts an existing Postgres URL for anyone who already runs one. Proven in production by Retrom (Tauri 2 + postgresql_embedded). |
| **Import adapters** (Proxmox, Docker host, Traefik): preview what would be imported, imported assets stay editable | In mockup (R2) | |

---

## 3a. Adapter SPI and extensibility (Decided — Björn 08-24: "just make it extensible")

The promise: **adding a new source later — another document management system, another ticket system — is one new adapter and zero changes to knobas core, search, or UI.** What makes that true:

| Guarantee | How |
|---|---|
| **Self-describing adapters** | An adapter's descriptor declares everything the app needs to host it: its config schema (the *Add source* form is **generated** from it, not hand-built per adapter), auth methods, capability set (search / write / webhooks / import), and the **entity kinds it emits with display metadata** (label, plural, monogram) — so a new source's items get grouped, chipped, and labeled in the launcher without touching core. |
| **One generic sync pipeline** | Anything that emits `SyncItem`s lands in `sync.item` and automatically gets: Postgres FTS search, launcher grouping + filter chips, per-row provenance ("synced N min ago"), linkability (§5a), context membership, inbox eligibility, and smart-list reachability. Search does not know adapter names — it knows `sync.item`. |
| **Open kinds, generic detail view** | Kind strings are open (like asset types). Known kinds get their tailored detail views; an **unknown kind gets a generic detail view** — title, metadata fields projected from the raw payload, body text, the links panel, and actions derived from the adapter's declared capabilities. A new ticket system is browsable on day one; a bespoke detail view is optional polish later. |
| **Compile-time plugins now, out-of-process later** | v1 plugins are Rust crates: one crate implementing `Source` + one registry line. The SPI is deliberately **transport-agnostic** — every type crossing it (`SourceDescriptor`, `SyncItem`, `Cursor`, `WriteOp`) is plain serde-serializable data, and sync streams through a sink — so a v2 can host adapters **out of process** (JSON-RPC over stdio, MCP-style, any language) or as WASM without changing the model. No dynamic-library ABI risk now, no dead end later. |
| **Raw payload kept** | `sync.item.payload` stores the source's raw record, so a later, smarter mapping (or a new detail view) can re-project existing data without re-syncing. |

---

## 4. Search (the launcher box)

| Feature | Status | Notes |
|---|---|---|
| Full-text search over all synced sources + local notes + assets, results grouped by type with source monogram and sync age | Decided | |
| **Prefixes**: `>` actions · `#` tickets · `@` people · `/` source · `t ` time · `note:` · `list:` · `asset:` · `?` help | In mockup | |
| **Filter chips**: per source, per type, `@me`, `today`, cross-source chips (e.g. *has failing build* — a join JQL can't express); asset chips `type:` `env:` `health:` | In mockup | |
| **Source aliases + inline key:value filters**: `/ji` `/gt` `/tc` `/cf` `/nt` `/as` (and long forms), `type:` `env:` `health:` `owner:` typed inline as an alternative to chips | In mockup (R3) | |
| **Per-row provenance**: every result shows its sync age ("synced 4 min ago", "local · always current", "read at import"); asset rows are double-height with their path underneath; footer reads "local index · N pending writes" | In mockup (R3) | |
| **"Do it here" rows**: contextual actions appended to results (Start/Stop timer on the current ticket, Comment on it); `>` with empty query doubles as the app's navigation menu | In mockup (R3) | |
| **Asset search matches ancestor path names** (searching "pve-02" finds the containers under it), in both the launcher and the board filter | Decided (E1) | **R3 gap to fix in the real app**: the corpus indexed only the asset's own fields, and the board filter was dead code. The implementation must index the path. |
| **Empty query** shows smart lists, contexts, inbox preview, today's time, recent items | In mockup | |
| **Tab → action chain** on a result (Change status › Comment › Link to… › Add to context › Start timer › Start work › Open) with breadcrumb chips | In mockup | |
| *Add to <context>* on every result | In mockup | |
| **Smart lists** = saved local queries with counts and change badges: *My tickets with a failing build*, *PRs waiting on me*, *PRs idle > 5 days*, *Pages I edited this week*, *Blocked tickets*, *Unlogged time this week*, *Assets with open alerts in my contexts*, *Drifted between stage and prod*, *Certificates expiring < 30 days*, *Assets with no monitor* | In mockup | **Rec 08-24 (answers Q3):** v1 = the built-in lists above (hand-written SQL inside knobas) **plus "Save this search as a list"** — any launcher query with its chips/prefixes becomes a smart list. That covers most needs at near-zero design cost. A real query language (SQL-ish over the schema) is v2; a visual builder only if the language proves too hostile. |
| Local code search across cloned repos from the same box; open in editor / terminal | Proposed | |
| Paste a Jira/Confluence/Gitea URL anywhere → resolves to an entity chip | Proposed | |

---

## 5. Work items (tickets, PRs, builds, pages, notes, commits, branches, repos)

| Feature | Status | Notes |
|---|---|---|
| Detail views per type with inline edit: ticket status/priority/assignee/comment; PR approve/comment; build log excerpt, re-run, trigger with parameters; page section edit; note editor | In mockup | **Rec 08-24 (answers Q4):** Confluence editing in v1 = **create page from template** (standup protocol needs it), **comment**, and **section-level text edit** (round-trip one storage-format section, refuse sections with macros/tables and offer *Open in browser* instead). No full editor — the storage-format round-trip risk isn't worth it while search/links are the product. |
| Branch and repo get detail views too (R3 left them as dead-end chips): branch → commits/PR/build state + *Open in editor*; repo → branches, clone state, *Clone…* | Rec 08-24 | Small, but link targets must all be openable. |
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
| **One table for every pair** — work↔work, asset↔work, asset↔asset, context membership, note `[[refs]]` are all rows of the same record above | Rec 08-24 | **Explicit spec fix**: R3 stored these in five different shapes and work↔work links had no relation at all. The real app has one `link` table; every link carries a relation (`related` as the default); backlinks are a query, not a scan. |
| **Inverse labels** per relation (runs-on↔hosts, exposes↔exposed by, depends-on↔needed by, monitored-by↔monitors, deployed-from↔deploys, documented-in↔documents, blocks↔blocked by) so each end reads naturally | In mockup (R3) | |
| **Linking an asset to a ticket auto-adds the asset to that ticket's contexts** | In mockup (R3) | Was implemented but unstated. **Rec 08-24: keep** — it's why the room's asset tile fills itself. Shown as origin `implied`, removable. |
| A suggestion can also propose a **route target** ("host name alone — confirm it"); confirming writes the route's target | In mockup (R3) | |
| **Manual linking everywhere**: *Link to…* on every detail, in the Tab action chain of every search result, by drag where a view supports it (columns, rooms), by typing `[[…]]` in notes; relation picked from a list or typed | Decided | |
| **Relation types** (open list): related · blocks / blocked by · implements · documents · deploys · runs-on · hosts · exposes · depends-on · monitored-by · in-context · mentions; users can add their own | Decided | |
| **Suggestions**: detected from keys in commit messages / branch names / build parameters / page text / scenario descriptions, image tags matching builds, host names matching services, text similarity via FTS, and native links in the sources; each shows its reason; Confirm / Dismiss; dismissed pairs are remembered and not re-proposed | Decided | Suggestions are stored too, with their reason and state. |
| Links are **bidirectional in the UI** (every panel shows both ends), and **transitive for contexts** (a context's members are what it links to, plus what those link to one hop out) | In mockup | **Rec 08-24 (answers Q11):** fixed rule in v1 — explicit adds + direct links + **one hop** (a member ticket's PRs, their builds). Not configurable until real use shows the need; R3 never actually implemented a general rule, so nothing is lost. Asset membership also counts through ancestors (a context holding a VM holds its containers for alert routing). |
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
| Actions per item: Open · Reply · Review · Re-run · Comment on ticket · Ping · Restart container · Ack · Snooze (returns on a date) · Done | In mockup | Snooze gets a real date picker with presets (tomorrow / next Monday / after the PAT expires) — R3 hard-coded the date. |
| "In this context" filter; count in the top strip; **every** inbox action records a line in the §2a activity stream | In mockup | R3 only logged some actions; the real app logs all of them (one writer). |
| Desktop notification when a watched build/monitor changes | Proposed | |

---

## 9. Time tracking

| Feature | Status | Notes |
|---|---|---|
| **One global timer**, `⌘T`, visible everywhere, context = any entity (ticket, page, note, repo, **asset**) or an ad-hoc label | Decided | |
| **No git-branch awareness** | Decided | |
| **Worklog draft** on stop: interval (editable), target ticket (optional), activity in the interval as checkboxes (commits, PR comments, builds triggered, pages/notes edited), generated comment, *Log Nh to PAY-xxx* → Jira worklog + local copy | Decided | |
| **Passive attribution** (opt-in): app records which entity/label was open → **Day review** strip: passive vs. manual blocks, unattributed gaps → *Assign…*, ad-hoc blocks → *Log to a ticket… / Keep local*, *Log all* | Decided | Drag/merge of blocks: nice-to-have. |
| **Worklog interval** = the day's blocks for that context concatenated ("09:40–11:50 + 13:58–14:32"), editable; stopping the timer on a non-ticket entity opens **"Log an ad-hoc block"** with a *suggested* ticket + reason instead of the worklog draft | In mockup (R3) | |
| **Week timesheet**: tracked / logged / unlogged per day and per context; rows can be an asset or "no context — app open, no entity" | In mockup | *Log all* must distribute per-day correctly (R3 zeroed rows wholesale — mockup shortcut, not the spec). |
| Per-context clocks; switching context prompts to move the timer | In mockup | |

**Addendum 2026-09-03 (M3 grilling, spec #272; built in M3.1 Time against the real Jira, ADR-0013).** The vocabulary is the `CONTEXT.md` **Time** section, and it corrects the table's wording: the timer's *timer target* is one entity or an ad-hoc label, never a stored context ("context = any entity" above predates ADR-0010's separation of the two words); a *block* is the local unit of time, manual when the timer made it and passive when attribution recorded what was open, never written to a source, read-only once logged; a *worklog* is the Jira record blocks become through the write queue, at-least-once per ADR-0012, with a local copy carrying the block ids it covers; *passive attribution* is opt-in and records the open detail, else the room's anchor entity, else nothing, only while the window is focused, and no passive block is ever logged without a person saying so. The day review strip and the week timesheet share one address, opened by the top strip's *Today*; *Log all* makes one worklog per day and ticket from unlogged manual blocks and never touches passive or label blocks. The last row's per-context clocks are out: there is one timer, and starting it on another target stops the current one with its draft first.

---

## 10. Standup

| Feature | Status | Notes |
|---|---|---|
| **Digest** (yesterday / today / blockers) generated from commits, worklogs, transitions, comments, alerts; every line traceable to its source item | Decided | |
| **Standup protocol** document (attendees, per-person notes, action items); editable; *Save as note* / *Publish to Confluence* (ENG › Standup protocols › date); action item → *Create ticket* | Decided | |
| Infra line in the digest (outages, restarts) | In mockup (R3) | |

**Addendum 2026-09-03 (M3 grilling, spec #272; built in M3.3 Daily flow against the real Confluence, ADR-0013).** The *digest* (`CONTEXT.md` **Digest**) is drawn from the mirror and the activity stream for the configured usernames, not from a per-event log written by sync (§2a, corrected): *yesterday* is the newest day before today with any of your activity, at most seven days back; *today* includes the running timer's target; *blockers* are your tickets in the adapter's declared blocked-like statuses plus tickets a link marks as blocked by; mine only, every line linking to the item it came from. The *standup protocol* (`CONTEXT.md` **Standup protocol**) is a note per date, not a kind of its own, so the table's *Save as note* is moot; *Publish to Confluence* creates a page under a configured parent (`standup.publish_target`, asked for on first publish) and links note and page, the note staying the editable original. The infra line waits for M4.

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
| **Status** up / warn / down / none; problems roll up so a closed branch shows "N problems inside" | Decided | **Rollup rule (R3, now spec):** an asset's health = worst of its own status and its monitors' states (down > warn > up > none); its *effective* status also takes the worst descendant; a paused monitor reads as none. |
| **Environment** (`dev`/`stage`/`prod`/`shared`) is a **stored property, inherited from the nearest ancestor that sets it**; import adapters set it where the source knows it | Rec 08-24 | **Spec fix**: R3 inferred env by pattern-matching id substrings — accident-prone. Same inheritance rule for **owner**. |
| **Routes and monitors are addressable entities** in their own right: searchable, linkable, chip-able, with their own `#/route/<id>` / `#/monitor/<id>` addresses (a monitor can attach to several assets and to routes) | In mockup (R3) | Was implemented but unstated; matters for the schema. |
| Change history per asset: **every mutation appends a line** (old → new for property edits); sync-originated lines marked as such; imported assets get an "imported from X" origin line | In mockup | Backed by the §2a activity stream, filtered to the asset. |

### 12.2 UI
| Feature | Status | Notes |
|---|---|---|
| **Miller columns** as the Assets view: one column per level, older columns collapse to labelled spines, `←→↑↓ Enter`, search reveals the path, wires from route rows to their targets/spines, `+` on every column header | Decided (E1 chosen) | Rejected: nested boxes (topology-explorer), semantic zoom, expanding graph, drill-in stack; R2's inventory board, topology map, environment matrix remain as references. Column rows carry two badges (linked-work count; amber/red "N problems inside"); wires are dashed when the target is reached through an ancestor. "Search reveals the path" must actually be wired up (dead code in R3). |
| **Assets view has three tabs: Board · Monitors · Flowrun** (counts in the tab labels) | In mockup (R3) | The doc previously treated monitors/Flowrun as detail sections only. **Monitors tab**: state filter chips with counts, per-monitor 24 h bar + last checks + uptime/cert days, Pause/Resume, open-alert cards, "Not monitored" roster, status-page dialog. **Flowrun tab**: instances per env, scenario matrix (dev/stage/prod version cells), Run now / View log / Promote. |
| Asset detail pane: properties, held-by path, holds, exposes, reachable via, monitoring, linked work, actions, history | In mockup (R3) | Property display: per-type schema orders known keys first, custom keys after; secrets masked with Show/Hide, stored in the keychain. |
| Create at any level (type conventions suggest children — "usual here: …", any type allowed); import from a source (preview: already-in-tree vs. new; imported assets stay editable and custom properties/links/routes survive the next sync) | Decided | |
| *Link to…* from an asset to work items / contexts / other assets with relation label; suggested asset links with reasons | Decided | |
| Assets in the launcher (with path), in rooms (ASSETS tile), on the timer, in the inbox (alerts), in smart lists | Decided | |
| Actions on an asset: open URL, copy SSH, open in Proxmox / Portainer, restart container, create monitor, add to context, start timer | In mockup | **Rec 08-24 (answers Q5):** v1 write-backs = open URL / copy SSH (no adapter needed), **create + pause monitor** (Uptime Kuma), **Run now / View log / Promote** (Flowrun — it's a decided first-class flow). **Restart container ships only with the Docker-host adapter** (v2); until then the button deep-links to Portainer/Proxmox instead. Nothing pretends to write back without an adapter behind it. |
| Environment matrix / drift view (stage ≠ prod) | In mockup (R2 W3) | **Rec 08-24 (answers Q6):** keep — but post-v1 and rebuilt as a *view over the same data* (env property + Flowrun versions). The `drift` smart list covers the need until then. |
| Blast radius ("what breaks if this goes down") | In mockup (R2 W2) | **Rec 08-24 (answers Q6):** don't keep the dedicated view. Instead a **"Depends on this" panel** in the asset pane (transitive closure over depends-on/runs-on/routes), which is 90 % of the value for 10 % of the work. |

### 12.3 Monitoring (Uptime Kuma adapter)
**knobas does not own monitoring.** Uptime Kuma stays the system of record for monitors, checks, notifications and status pages; knobas syncs from it, attaches monitors to assets and contexts, and routes alerts into the inbox. **Decided 2026-08-23.**

| Feature | Status | Notes |
|---|---|---|
| Adapter reads monitors, current state, response time, uptime, heartbeat history and cert expiry from Uptime Kuma | Decided | **Rec 08-24 — updated for Uptime Kuma v2 (stable since 2025-10, now 2.5.x; 1.x is EOL; still no REST API):** primary channel is **polling `/metrics` with an API key** — v2 added a `monitor_id` label plus uptime-ratio and response-time metrics, so plain `reqwest` covers state/telemetry. **v2 prunes raw heartbeats to ~24 h**, so knobas appends samples into its own Postgres timeseries from day one (it will quickly hold more history than Kuma). Config detail (intervals, paused, groups) via the `kuma-client` crate (socket.io) on a slow cadence; `rust_socketio` itself is stalled since 2024, so socket.io stays a secondary, replaceable channel. |
| Monitor types shown as Uptime Kuma defines them (HTTP, TCP, ping, Docker, cert expiry, …); states up / down / pending / warning (response-time threshold is knobas-side) | Decided | |
| Monitors attach to assets (and through them to contexts); alerts → inbox with Open asset / Ack / Snooze / Restart; Ack marks the alert on the asset | Decided | **Alert routing rule (R3, now spec):** an alert reaches the *inbox* only when some context holds the affected asset (directly or via an ancestor or via the context's monitors); all open alerts always show in the Assets views and the top-strip count. **Ack** is knobas-local: clears the inbox item, writes history, the alert stays open until the monitor recovers; a restart that recovers the monitor auto-resolves it. |
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

## 13. Secondary features (Rec 08-24: tiered — answers Q10)

**Pulled into v1** (no longer backlog):
- Conflict resolution UI for queued writes — required by the §3 write-queue decision (ask-with-diff).

**M2.5 — the mini board (addendum 2026-08-30):**
- The Room's Tickets tile renders the **mini board** the §2 Shell table promised: status columns over the context's live ticket items, column names in the source's own words, plus a status select in the ticket detail round-tripped through the write queue. M1 shipped the tile as a flat recency list; the mini-board rendering is tiered as its own milestone, **M2.5**, which opens only at M2 exit (Björn's gate) and closes before M3 starts. Spec: GitHub issue #175. Vocabulary: ADR-0009 — the feature is the *mini board*; "board" never appears unqualified.

**M2.6 — project rooms and the mini board's two layouts (addendum 2026-08-31):**
- A **project** — a source's own grouping of its items, in the source's own word, per the `CONTEXT.md` glossary entry and ADR-0010 — becomes a scoping dimension a room can be built on. The context switcher grows a room for every project a source's corpus shows, listed directly under that source's own room, and standing in one narrows every tile in the room rather than the Tickets tile alone.
- The **mini board** is drawn in **two layouts, and the room chooses**. A bounded room — a project room, a stored context — draws the **column layout** M2.5 shipped: statuses side by side, each headed by its name and count. A definitionally unbounded room — *All work*, a source room — draws the **stacked layout**: the same status groups one under another, each with its heading and count, the tile scrolling down instead of sideways. Both layouts draw the same statuses in the same order, with the same counts and the same cards, and the terminal "No status" group is last in both; the layout is how the groups are arranged, never which groups there are. A **demote-only backstop at six columns** sends a bounded room whose mini board turns out to span two workflows to the stacked layout rather than to a horizontal scrollbar, and it never runs the other way, so a room's layout does not flip back and forth as its contents change through the day.
- Both halves are tiered as one milestone, **M2.6**, which opens only at M2.5 exit (Björn's gate) and closes before M3 starts. Spec: GitHub issue #188. Decision records: ADR-0010 (a project is a scoping dimension, in the source's own word) and ADR-0009 (the feature is the *mini board*; "board" never appears unqualified).

**v1.5 — first releases after the MVP** (each is small and rides on existing plumbing):
- Desktop notifications (build finished, monitor changed, PAT expiring) — Tauri notification plugin over the same events the inbox consumes.
- Open in editor (VS Code / JetBrains) / terminal at the checkout — completes the ticket→branch flow.
- Paste a Jira/Confluence/Gitea URL anywhere → entity chip — the resolver is a lookup on ids the sync already stores.
- Quick capture hotkey (global) with current context attached.

**v2 backlog:**
- Local code grep across clones from the launcher.
- Runbook page → interactive checklist; build-failure → create-ticket shortcut (→ comment exists in v1).
- Context handoff bundle (markdown of everything linked).
- Personal templates (ticket, page, PR description).
- Navigation history ("what was I doing before lunch") — derivable from the §2a activity stream when wanted.

---

## 14. Non-functional

| Topic | Decision / proposal |
|---|---|
| Platform | Tauri 2 desktop app; Rust backend. **Rec 08-24 (answers Q9): Svelte 5 (runes) + Vite, no SvelteKit.** Reasons: runes are fine-grained signals so the mockup's "mutate this when that changes" logic maps ~1:1; `mount()` allows porting region-by-region as islands; the mockup's CSS carries over wholesale as a global stylesheet; smallest runtime; Bits UI provides Svelte-5-native keyboard/dialog/command primitives. React's runtime+StrictMode friction and Solid's 2.0-at-RC timing both lose for this app. |
| Storage | Postgres (local), full-text search via its FTS; one schema for entities, links, notes, assets, worklogs, activity; per-source raw payload kept for re-mapping. |
| Secrets | OS keychain via Tauri; never in the DB; masked in the UI. |
| Performance | Local search < 100 ms for ~100 k items; sync incremental per adapter; UI never blocks on a source. |
| Offline | Everything read works offline; writes queue. |
| **Export / import** | Export = a dump of the knobas database contents; import = restoring it. One archive file (`*.knobas`, a compressed logical dump of the `knobas` schema — links with relation/origin/reason/dismissed state, contexts and memberships, notes, the asset tree with properties/routes/relations, worklogs and day blocks, smart lists, source configurations **without secrets**; the `sync` schema optional since it re-syncs). Entity references use the stable ids from §5a so a dump restores on another machine or after a re-sync. Restore into an empty database is the primary path; restore into a populated one merges by id with a preview (added / changed / conflicting) rather than overwriting. Scheduled automatic exports as backups. Plain-file formats (markdown notes, YAML assets) are at most a secondary *view* of the same data, not the source of truth. **Decided 2026-08-23.** **Rec 08-24 (answers Q12):** defaults — *Backup* export: everything in `knobas` incl. notes, `sync` excluded (it re-syncs; keeps archives small). *Share with a colleague* export: links/assets/contexts/smart lists, **notes excluded** by default (they're personal), each toggleable in the export dialog. Needs a small settings surface (export now / schedule / restore) — no mockup yet, plain dialogs are fine. |
| Accessibility | Keyboard-complete, visible focus, reduced motion, contrast ≥ 4.5:1 on readings. |
| Privacy | Passive time attribution is opt-in and local only. |

## 14a. First run, demo mode, and the mock source (Rec 08-24 — new)

| Feature | Status | Notes |
|---|---|---|
| **First-run wizard**: initialize the database → add the first source (the §3 flow) → initial sync with progress → land in the launcher | Rec 08-24 | The doc had no onboarding story; an empty cockpit with no guidance would be the first thing you ever see. |
| **Demo seed mode** (`knobas --demo` or a hidden setting): loads the Tidewater Freight dataset (`mockups/shared/dataset.md` + `assets.md`, made machine-readable) into a scratch database | Rec 08-24 | Three jobs: (1) every UI workstream develops against identical, rich data without live credentials — this is what makes parallel agents possible; (2) golden data for integration tests; (3) safe screenshots/demos. |
| **Mock source adapter** (`knobas-source-mock`): a full `Source` implementation serving the fictional dataset, including simulated failures (401, timeout) and write-back | Rec 08-24 | Doubles as the contract test suite for the `Source` trait: every real adapter runs the same test battery the mock defines. |
| **Local test environment** (Docker, this machine): `testenv/docker-compose.yml` with real Gitea + real Uptime Kuma containers seeded with the Tidewater content, plus `knobas-mockd` — a stateful HTTP mock server implementing Jira and TeamCity API subsets (the Confluence half planned here was never built and will not be: ADR-0013). ~~**No real Jira/Confluence/TeamCity instance exists during development**: the official OpenAPI documents (fetched + checksum-pinned in `testenv/specs/`) are the only ground truth, and mockd validates both its responses and the adapters' requests against them~~ | Decided (Björn 08-24); **amended 2026-09-03 (ADR-0013)** | **`knobas-mockd` is deprecated.** A mock certifies nothing: the real containers — Gitea, the seeded TeamCity (`--profile real-teamcity`), the seeded Jira and Confluence (`--profile real-atlassian`) — are the witness for every adapter and every write path, each with a `just <system>-live` recipe, and no acceptance or exit criterion is met against a mock. mockd stays frozen until the live suites assert what its tests assert, then is deleted; Flowrun's stub is the single named exception. `knobas-source-mock` at the trait layer (UI/sync tests, `--demo`) is unaffected. Details: `docs/agents/working-model.md`, test strategy by layer. |

---

## 15. Architecture direction (to validate in the implementation plan)

Rust workspace: `knobas-core` (entities, links, contexts, notes, time, activity), `knobas-db` (sqlx + Postgres, FTS, migrations), **`knobas-source`** (the SPI: the `Source` trait, capability/cursor/event types, and the contract-test battery every adapter must pass), `knobas-source-*` (one crate per adapter implementing it: config schema, auth, incremental sync, search mapping, write-back, capabilities), **`knobas-source-mock`** (the §14a fictional-dataset adapter), **`knobas-sync`** (the scheduler: per-source cursors, incremental runs, backoff, the write queue and its conflict checks, events out), `knobas-assets` (tree, routes, relations, monitors), `knobas-git` (gix/git2: clone, pull, push, branch), `knobas-app` (Tauri commands, events, keychain). Frontend talks to commands only; all sync runs in the backend on a schedule.

**Contract-first (Rec 08-24, this is what enables parallel work):** three interfaces get frozen before fan-out — (1) the `Source` trait, (2) the DB schema (`knobas` + `sync`), (3) the Tauri command/event API as a typed IPC schema shared with the frontend (generated TS types). Adapter agents build against the trait + contract tests; frontend agents build against the IPC schema + the mock source; neither waits for the other.

**Amended 2026-09-03 (ADR-0013).** Adapter agents still *build* against the trait and the contract battery, but they are *certified* against the real system: a live suite against the seeded container, its output in the PR body, re-run by the merge-manager before the squash. `knobas-mockd` is no longer part of this picture: deprecated, frozen, deleted once the live suites cover its assertions. `knobas-source-mock` stays as the frontend's backend.

---

## 16. Open questions — all answered with recommendations 2026-08-24 (Claude); **ratified 2026-08-27 (Björn), all twelve, with two amendments** (the notification tier in 10, and this tier mapping: **v1 = end of M4** on the roadmap; v1.5/v2 follow it)

1. **Postgres: embedded** via `postgresql_embedded`, download-on-first-run, PG 18.6 pinned, TCP on 127.0.0.1; "use existing Postgres URL" as a setting. Not a user-installed prerequisite; not a Tauri sidecar (macOS notarization of external binaries is a known open Tauri bug — the crate's extract-to-home approach sidesteps it). → §3.
2. **Split-flaps: only on values that change while you watch** (timer, sync countdown, build/monitor transitions, inbox/alert counts); everything else plain mono. → §2.
3. **Smart lists: built-ins + "save this search as a list" in v1**; query language v2. → §4.
4. **Page editing: create-from-template + comment + macro-free section edits in v1**; *Open in browser* for everything else. No full editor. → §5.
5. **v1 asset write-backs: create/pause monitor (Uptime Kuma) and Run/Log/Promote (Flowrun)**; restart-container waits for a Docker-host adapter (v2), deep-link to Portainer/Proxmox until then. → §12.2.
6. **Environment matrix: keep, post-v1, as a view over the env property + Flowrun versions. Blast radius: fold into a "Depends on this" panel** in the asset pane instead of a dedicated view. → §12.2.
7. **Queued-write conflicts: re-read before flush; if the target changed, hold and ask with both versions side by side.** No silent last-write-wins in v1. The queue is a visible list. → §3.
8. ~~Monitoring~~ **Answered 2026-08-23: Uptime Kuma is the monitoring system.** 08-24 note: adapter plan updated for Kuma **v2** (`/metrics` primary, own timeseries, `kuma-client` for config). → §12.3.
9. **Frontend: Svelte 5 + Vite** (no SvelteKit); port region-by-region, keep the mockup CSS global. → §14.
10. **Backlog promotions:** conflict UI → v1 (follows from 7); desktop notifications → **M3** (amended 2026-08-27 — they pair with the daily flow, and M2's inbox is in-app only); open-in-editor/terminal, paste-URL→chip, quick capture → v1.5; rest stays v2. → §13.
11. **Context membership: explicit adds + direct links + one hop, fixed rule, not configurable in v1.** Asset membership counts through ancestors. → §5a.
12. **Export defaults: backup = all of `knobas` incl. notes, no `sync`; share = links/assets/contexts/smart lists, no notes; every part toggleable.** → §14.

## Appendix — mockup map

- Round 1: `mockups/round-1/index.html` — 5 paradigms × 5 designs; chosen: P4 Context hub + P1 Launcher, D1 Signal (D3 Patchbay liked for later).
- Round 2: `mockups/round-2/index.html` — base shell + W1 Inventory, W2 Topology, W3 Environment matrix.
- Playground: `mockups/playground/` — topology-explorer (rejected), E1 Miller columns (chosen), E2 Semantic zoom, E3 Expanding graph, E4 Drill-in stack.
- Round 3: `mockups/round-3/signal-miller.html` — the integrated view this document describes.
- Briefs and data: `mockups/shared/` (dataset, assets, screens, paradigms, designs, round briefs).
