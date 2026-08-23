# Round 3 brief — the integrated Signal view with Miller-column assets

One mockup: `mockups/round-3/signal-miller.html`. It is the round-2 shell (`mockups/round-2/base-signal.html`: context hub rooms + launcher box, D1 Signal) with assets done as **Miller columns** (the user picked `mockups/playground/E1-miller-columns.html`), and assets woven into everything else: linkable to tickets / PRs / builds / pages / notes / contexts from both ends, searchable, in rooms, in the inbox, on the timer.

All constraints from `agent-brief.md` apply (self-contained, vanilla, Google Fonts only, `⌘K`/`⌘T`/`Esc`, optimistic mutations, no `alert()`, no lorem). Read `dataset.md` (corrected time figures), `assets.md`, `designs/D1-signal.md`, `round-2-brief.md` (surfaces A1–A7 still apply; A1 is now Miller columns).

## Changes to the shell
- **Remove the amber ticker strip** at the top (the user does not like it). Keep the split-flap cells; keep the top strip (context tabs, search field, timer, inbox, Assets, sync monograms), status bar.
- Top strip gains **Assets** (opens the columns) with an open-alert count.

## The asset model (merge two sources)
1. **Containment tree** — copy `SEED_NODES` / `SEED_ROUTES` verbatim from `mockups/playground/topology-explorer.html` (78 assets, 8 levels, 15 routes). This is the hierarchy the columns browse. Any asset can hold assets; any asset can expose routes.
2. **From `assets.md`** — monitors, Flowrun run data / promote flow, suggested links, context membership, smart lists, new inbox items, new sources. Map its flat ids onto tree ids:
   `c-payout-prod→ct-payout-prod · c-ledger-prod→ct-ledger-prod · c-worker-prod→ct-worker-prod · c-payout-stage→ct-payout-stage · c-ledger-stage→ct-ledger-stage · c-pg-stage→ct-pg-stage · svc-payout→svc-payout-prod (stage: svc-payout-stage) · svc-ledger→svc-ledger-prod · svc-dashboard→svc-dash-prod · svc-traefik/rp-edge→rp-traefik · r-*→the same route ids · db-payments→dbn-prod (server db-prod) · db-payments-stage→dbn-stage · fr-dev/stage/prod→rt-fr-dev/stage/prod · sc-sepa→app-sepa-prod/stage/dev · sc-invoice→app-invoice-* · sc-refund→app-refund-dev · sc-customs→app-customs-*`.
   Monitors attach to: m-payouts→svc-payout-prod (+ route r-payouts) · m-ledger→svc-ledger-prod · m-dashboard→svc-dash-prod · m-flows-stage→rt-fr-stage · m-flows→rt-fr-prod · m-worker→ct-worker-prod · m-pg→db-prod · m-vm-pay→vm-pay-01 · m-cert→route r-payouts · m-pg-stage→ct-pg-stage.
   Context membership (tree ids): PAY-200 → vm-pay-01, ct-payout-prod, ct-worker-prod, dbn-prod, route r-payouts (+ their monitors) · PAY-231 → ct-payout-stage, vm-pay-stage-01, rt-fr-stage, app-sepa-stage, route r-payouts-stage · Staging DB configuration → ct-pg-stage, dbn-stage, vm-pay-stage-01.
3. **Work links (asset ↔ work item), confirmed**: app-sepa-prod & app-sepa-stage ↔ PAY-231, page *SEPA payout retry design* · app-refund-dev ↔ PAY-228 · svc-dash-prod ↔ PAY-240 · ct-payout-stage ↔ PR #142, build #1188 · svc-payout-prod/stage ↔ repo `payout-service` · svc-ledger-prod/stage ↔ repo `ledger-api` · ct-worker-prod ↔ note *SEPA retry investigation* (Mara noted the OOM) · dbn-stage ↔ the ad-hoc context *Staging DB configuration* and note *Credentials to rotate*. **Suggested**: vm-lowcode-01 → page *Payments architecture overview* (mentions "lowcode-01") · route r-dashboard → svc-dash-prod (host name match) · ct-payout-stage → build #1188 (tag 1.9.0-rc1) · app-refund-dev → PAY-228 (key in description) · ct-worker-prod → inbox alert / PAY-200 (alert in context).

## Surfaces
- **Assets view** (`#/assets`, top-strip *Assets*, `/assets` chip, `asset:` prefix): port E1's Miller columns into the shell's main area — columns, spines for the path, `L{n}` headers with `+`, rows with lamp · monogram · name · one-line · count · problem badge, `←→↑↓ Enter`, search reveal, wires from route rows, create at any level, persistence not required (mockup state only). Right pane = the asset detail.
- **Asset detail** (pane in the Assets view; also `#/asset/<id>` opening the columns at that path when reached from anywhere else): status flap + properties (typed + custom, add/edit) · **holds / held by** (path chips) · **exposes / reachable via** · **monitoring** (state, response, 24 h 48-segment bar, last checks, open alerts, Ack, *Create monitor*) · **linked work**: contexts, tickets, PRs, builds, repos, pages, notes — each a link, with *Link to…* (picker over all work items and assets, with relation label) and a *Suggested* strip (confirm/dismiss) · actions (*Add to context*, *Start timer on this*, *Open URL*, *Copy SSH*, *Restart container*, *Run now* / *View log* / *Promote* for scenarios per `assets.md`) · history.
- **Work items gain assets**: ticket / PR / build / page / note detail shows a **Linked assets** section (lamp, monogram, name, path) with *Link asset…* and *Open in Assets* (jumps the columns to it); the ticket's suggested-links strip includes asset suggestions.
- **Rooms**: an `ASSETS` tile listing the context's assets (lamp · name · short path · health) with problems first and *Open in Assets*; the ad-hoc "Staging DB configuration" room shows its DB/VM; *Add to context* from any asset; room inbox filter includes alerts on the context's assets.
- **Inbox**: the two new alert items (+ snoozed cert) with *Open asset* (→ columns at that asset) / *Ack* / *Restart container*.
- **Search** (launcher box): assets in results with their path under the name; `asset:` prefix; chips `type:` / `env:` / `health:`; Tab chain on an asset: *Open in Assets · Link to… · Add to context · Start timer · Create monitor*. Smart lists from `assets.md` (alerts in my contexts, drifted, certs expiring) plus **Assets with no monitor**.
- **Timer**: an asset can be the timer's context (ad-hoc label style); the day review then shows it.
- **Monitors** tab inside the Assets view (list with state filter) and **Flowrun** reached through the tree (runtime → scenarios) with the promote dialog from `assets.md`.
- **Sources**: Uptime Kuma + Flowrun configured; Proxmox / Docker host / Traefik available importers; *Import* preview.
- **Standup digest** gains an infra line ("flows-stage down since 12:40; payout-worker OOM-killed at 11:58 — restarted").

## Header comment
```html
<!--
knobas mockup
round: 3
paradigm: P4 Context hub + P1 Launcher box + Miller-column assets
design: D1 Signal
thesis: <one line — what the integrated view makes effortless>
thesis: <one line — how assets sit next to work items>
thesis: <one line — the single signature element (not the ticker)>
-->
```

## QA (agent-brief.md + round-2-brief.md, plus)
- From PAY-231 (ticket detail) → *Linked assets* → `payout-service` (stage) → columns open at dc › pve-01 › vm-pay-stage-01 › docker › payout-service › payout-service with the pane showing PR #142 and build #1188 under linked work.
- From the Assets view, link `vm-lowcode-01` to PAY-240 via *Link to…*; PAY-240's detail now lists it; the room's ASSETS tile for PAY-200 updates if PAY-240 is in it.
- Search `asset: flowrun` → results with paths; Tab → *Add to context* → appears in the room tile.
- Inbox → *payout-worker down* → *Open asset* → columns at the container; *Restart container* flips its lamp and clears the alert.
- `⌘T` while an asset is selected starts the timer on it; day review shows the block.
- No ticker element anywhere. Zero JS errors at 1440×900 and 1100 wide.

## Report
As in round-2-brief.md, plus: what was taken from E1 verbatim vs. adapted, and a list of every place assets now appear in the shell.
