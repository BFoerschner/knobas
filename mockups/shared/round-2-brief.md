# Round 2 brief — asset management on the Signal shell

Round 1 settled the shell: **Context hub** (rooms per work context, context switcher, tiles, inbox, time, standup, sources) with the **Launcher box** for search (prefix syntax `>` `#` `@` `/`, filter chips incl. cross-source ones, `Tab` opens an action chain) — all in the **D1 Signal** design. That shell exists as `mockups/round-2/base-signal.html`. Round 2 adds **asset management** to it in three different *ways* (W1 Inventory, W2 Topology, W3 Environment matrix). You build one way.

All constraints in `agent-brief.md` still apply (single self-contained file, vanilla JS/CSS, Google Fonts only, desktop proportions, `⌘K`/`⌘T`/`Esc`, optimistic in-page mutations, no `alert()`, no lorem, dataset verbatim, no git-branch time attribution). Read `dataset.md` **and** `assets.md`; use the corrected time figures already present in `dataset.md`.

## How to start
1. Invoke `frontend-design:frontend-design`. Plan the asset surfaces as Signal: graphite, hairlines, mono readings, amber only where something is moving or wrong, split-flap cells for state that changes (health, version, run status).
2. Read `mockups/round-2/base-signal.html` end to end. **Copy it** to your output path and extend it — keep its rooms, search box, inbox, time, standup and sources working. You may refactor its internals, but every existing surface must remain reachable and the dataset entities must stay intact.
3. Add the asset model from `assets.md` to the data layer, then build surfaces A1–A7 below the way your way brief says.

## Required asset surfaces
- **A1 Asset overview** — the way itself (your brief). Reachable from the top strip (`Assets`) and from search (`asset:` prefix and an `/assets` chip). Filters: type, env, health, owner. Counts per type visible.
- **A2 Asset detail** — for *every* asset: typed properties per type schema; **custom properties** with *Add property* (key, type text/number/date/url/secret, value; secrets masked); relations grouped by relation type, each a link to the other asset; **monitoring block** (state, response, 24 h bar of 48 segments, last 5 checks, open alerts, *Create monitor for this asset*); linked work (contexts, tickets, PR/repo/build, pages, notes) with *Add to context* and *Link to…*; actions (*Open URL*, *Copy SSH* `ssh mara@10.20.4.11`, *Open in Proxmox* / *Open in Portainer* deep links, *Restart container* → optimistic state flip); change history.
- **A3 Create asset** — type picker → that type's template properties → optional custom properties → relations → *Create*. Also *Import from source* (Proxmox VE / Docker host / Traefik / Flowrun) that shows a preview of what would be imported and a *Configure source* path.
- **A4 Link assets** — *Link to…* with a **relation type** picker; where your way supports it, drag one asset onto another → relation type popover. **Suggested asset links** (the four in `assets.md`) with reason + Confirm/Dismiss, visible both in the overview and on the asset.
- **A5 Monitoring** — monitor list (state, type, response, uptime 30 d, cert days) with a state filter; alerts as inbox items (*Open asset* / *Ack* / *Snooze*, *Restart container* for the worker); *Create monitor* (type, target, interval) optimistic; *Pause monitor*; status page link. Ack removes the inbox item and marks the alert acknowledged on the asset.
- **A6 Flowrun** — instances dev/stage/prod (version, host, URL, health) and scenarios with per-env versions, trigger, last run, runs today; *Run now* (queues run, optimistic), *View log* (the excerpt from `assets.md`); **Promote** `sepa-payout-export` **v13 stage → prod** via the review dialog described in `assets.md`; scenarios link to tickets/pages and to the instance/VM they run on.
- **A7 Context integration** — `ASSETS` tile in every room with that context's assets and their health (membership in `assets.md`); the ad-hoc "Staging DB configuration" room shows its DB/VM; *Add to context* from any asset; the inbox "in this context" filter includes alerts on the context's assets; search chips `type:` `env:` `health:` on asset results; smart lists **Assets with open alerts in my contexts**, **Drifted between stage and prod**, **Certificates expiring < 30 days**; the two new sources (Uptime Kuma, Flowrun) in Sources with sync state, plus Proxmox / Docker host / Traefik as available adapters.

## Header comment (the index is generated from it — exact format)
```html
<!--
knobas mockup
round: 2
way: W2 Topology
design: D1 Signal
thesis: <one line — what this way makes effortless about assets>
thesis: <one line — how it fits the context hub + launcher shell>
thesis: <one line — the single signature element>
-->
```

## QA (in addition to agent-brief.md)
- Chain: PAY-231 room → ASSETS tile → `payout-service` (stage) container → `vm-pay-stage-01` → its monitors → `flows-stage down` alert in the inbox → Ack.
- Create a VM with one custom property; it appears in the overview and in search. Link it to `traefik` with relation *depends-on*; the relation shows on both assets.
- Promote `sepa-payout-export` v13 stage → prod: dialog → optimistic result → history line → prod version cell updated wherever it is displayed.
- `asset: stage`, `health:down`, `/assets` chip and the type/env/health chips work in the search box.
- Base surfaces still work: rooms switch, `⌘T` opens the worklog draft, standup and time screens render.

## Report
As in agent-brief.md, plus: which base functions you reused vs. replaced, and any place where the asset model forced a change to the shell.
