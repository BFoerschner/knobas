# knobas — session handoff (2026-08-24)

Context for picking up work on **knobas**, a Rust + Tauri desktop app being designed via clickable HTML mockups before any app code is written. Repo: `/Users/dev/Projects/knobas`, branch `main`, everything committed.

## 1. What knobas is

A personal work cockpit that unifies Jira (tickets), Confluence (pages), Gitea (git), TeamCity (pipelines), **Uptime Kuma (monitoring — it stays the system of record; knobas only syncs from it)**, a low-code scenario runtime (fictional name "Flowrun": instances per dev/stage/prod running versioned scenarios), and the user's own infrastructure assets. It keeps a local copy of everything in **one Postgres database** (with FTS) so search is instant and cross-source, writes back through each system's API, and adds what no source has: **links, contexts, notes, time tracking, an inbox, standup generation, and an asset tree**.

The user: Björn (git author "Björn Förschner"), a developer juggling these systems daily; pain points are bad source-system search (JQL/Confluence) and credential juggling.

## 2. Hard decisions (do not re-litigate)

1. **Process**: no Rust/app code until the UI is chosen from mockup rounds. Clickable self-contained HTML mockups with shared fictional data, built by parallel agents, user picks, repeat.
2. **Paradigm**: Context hub (rooms per work context) + the Launcher box (prefix search `>` `#` `@` `/`, filter chips, Tab action chains) merged into one shell.
3. **Design**: D1 "Signal" (dark graphite, hairlines, mono readings, amber only for attention, split-flap cells). D3 "Patchbay" liked, may return later. **The amber ticker strip is removed** (user dislikes it).
4. **Assets**: Miller columns (Finder-style, chosen from 5 depth-explorer prototypes) over an **infinitely nestable** asset tree (site › hypervisor › VM › engine › container › runtime › app › step › connector; DB › database › schema › table). Types are open; create anywhere; any asset can expose routes (URL → target); routes visible from both ends ("exposes" / "reachable via", including through ancestors).
5. **Links are THE core feature**: manual (*Link to…* everywhere) + suggested (with reasons, dismissals remembered), **stored locally only — never written into the source systems**.
6. **Postgres is the one store**: synced copies (`sync` schema) + everything knobas owns (`knobas` schema: links, contexts, notes, assets, time, smart lists, source configs minus secrets). **Export/import = dump/restore of that DB's contents** (one archive; merge-by-id with preview when restoring into a populated DB; plain files at most a secondary view).
7. **Time tracking**: manual timer + passive suggestions based on what's open in the app; **no git-branch awareness ever** (only git has branches; much work isn't code). Timer context = any entity (incl. assets) or an ad-hoc label. Worklog draft assembled from interval activity as checkboxes → Jira worklog. Day review with unattributed blocks; week timesheet.
8. **First-class features**: attention inbox, ticket→branch→PR one-action flow, smart lists (cross-source queries JQL can't do), auto-link suggestions, standup digest + protocol (publishable to Confluence).
9. **Monitoring belongs to Uptime Kuma** (no official REST API — adapter speaks its socket.io protocol; `/metrics` for cheap polling; write-back only create/pause; "ack" is knobas-local).

## 3. Repo layout

```
mockups/shared/          briefs: dataset.md (fictional company "Tidewater Freight", Mara Lindqvist,
                         PAY-231 SEPA-retry storyline; "today" = Fri 2026-08-22 14:32, fictional calendar),
                         assets.md, screens.md (8 surfaces), agent-brief.md, round-2-brief.md,
                         round-3-brief.md, paradigms/P1-P5, designs/D1-D5, ways/W1-W3
mockups/round-1/         25 mockups (5 paradigms × 5 designs) + index.html (5×5 matrix)
mockups/round-2/         base-signal.html (shell: context hub + launcher, corrected time figures)
                         + W1-inventory / W2-topology / W3-environment-matrix + index.html
mockups/playground/      brief.md, topology-explorer.html (nested boxes — REJECTED),
                         E1-miller-columns (CHOSEN), E2-semantic-zoom, E3-expanding-graph, E4-drill-in-stack
mockups/round-3/         signal-miller.html — THE current reference mockup (4277 lines):
                         shell + Miller-column assets integrated everywhere, no ticker
mockups/build-index.mjs  regenerates a round's index.html (matrix or flat list) from file header comments
docs/superpowers/specs/2026-08-23-knobas-design.md   THE design document (feature walkthrough)
HANDOFF.md               this file
```

Every mockup: single self-contained HTML, vanilla JS/CSS, Google Fonts only, opens from `file://`, `⌘K` search / `⌘T` timer / `Esc` unwind, optimistic mutations, no `alert()`, shared dataset verbatim. Header comment (paradigm/design/way + 3 thesis lines) feeds the index generator.

## 4. The design document

`docs/superpowers/specs/2026-08-23-knobas-design.md` — feature walkthrough with status tags (**Decided / In mockup / Proposed / Open**): shell, sources/sync, search, work items, **§5a links (core)**, notes, contexts, inbox, time, standup, actions, **§12 assets (model / Miller UI / Uptime Kuma monitoring / Flowrun)**, backlog, non-functional (incl. export/import), architecture direction (Tauri 2 + Rust workspace: knobas-core / knobas-db (sqlx, Postgres FTS) / knobas-source-* behind one `Source` trait / knobas-assets / knobas-git / knobas-app; OS keychain for secrets).

**Open questions awaiting the user** (§16): 1 Postgres installed vs. bundled (now the first real architecture decision); 2 keep split-flaps everywhere or tone down; 3 smart-list query language vs. builder; 4 Confluence editing depth; 5 which asset actions write back in v1; 6 keep environment-matrix / blast-radius as secondary views; 7 write-queue conflict strategy; 9 frontend framework (mockups are vanilla; Svelte/Solid/React all port); 10 backlog promotions; 11 context membership hop depth; 12 export defaults (notes included? `sync` schema included?). (8, monitoring, is answered.)

## 5. Where things stand / immediate next steps

- User was last reviewing `round-3/signal-miller.html` and the design doc; both delivered. No outstanding build tasks. Round-3 known gaps (from its builder, acceptable so far): created objects are session-only; asset detail is a fixed pane (not the slide-over) so the column path stays visible; search doesn't match ancestor names in asset paths.
- **Next**: user answers the open questions / gives round-3 feedback → update the design doc → then the superpowers flow: user reviews spec → `superpowers:writing-plans` → implementation plan → build the Tauri app. Further mockup rounds only if the user asks (e.g. re-applying Patchbay once features settle).
- Time-figure note: `dataset.md` was corrected (Fri 5h 12m; tracked 34h 37m; logged 29h 25m; unlogged 5h 12m; #1187→PAY-231 is confirmed, #412→PAY-228 is the 4th suggestion). Round-1 mockups and some round-2 ways still show the old numbers — harmless, don't fix retroactively.

## 6. Process facts and gotchas

- **Mockup fan-out**: parallel `general-purpose` agents on **Opus** (user explicitly chose Opus for agents after session limits killed a Fable wave; session model is now Opus 5 too), one file per agent, in per-stream git worktrees under `.worktrees/` (ignored) for rounds — orchestrator commits/merges (agents never run git; 5 agents sharing a worktree would collide on index.lock). Playground files were written straight into the checkout. Harness cap: 20 concurrent subagents. **User watches usage limits — never launch more agents than agreed, ask before scaling.**
- Each agent: invoke `frontend-design:frontend-design` skill first, read the briefs, build one file, QA in headless Chrome ("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", `--headless=new`; agents should use their own `--user-data-dir`/port — parallel agents have collided), report raw data. Superpowers plugin is installed (brainstorming was used; architectural path; next artifact per its flow = implementation plan via `writing-plans`).
- **Git commits are GPG-signed**; the pinentry prompt sometimes expires — if signing fails, tell the user to run the commit themselves (`! git commit …`) or wait for their go-ahead; they said "you can commit now" once cached. Commit style: short imperative subject, no Claude attribution used in this repo so far.
- Memory files exist at `/Users/dev/.claude/projects/-Users-dev-Projects-knobas/memory/` (process, no-branch-time-tracking, links-local-exportable) — keep them current on new decisions.
- User communicates tersely, redirects fast, plays devil's advocate (asked "why not Grafana?" — answer: Grafana/Backstage fit slices but not the local-index + write-back + desktop core; monitoring delegated to Uptime Kuma instead). Present options with a recommendation; don't over-ask.
