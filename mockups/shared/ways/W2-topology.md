# W2 — Topology

**Thesis:** the infrastructure as a map. An SVG diagram shows VMs as host boxes containing their containers and services, the edge VM with Traefik fanning routes (URLs) to services, the databases, and the Flowrun instances as stacked boxes holding scenario chips. Monitors are status lamps on the nodes. Dependencies are visible, so "what breaks if vm-pay-01 goes down" is a click.

```
┌─ CONTEXT [PAY-200 ▾] …   ◔ 0:34  ✉ 8   Assets ─────────────────────────────────────────────┐
│ ▌ asset:                     layers: [prod] [stage] [dev]  [monitors] [scenarios]   ⌘K      │
├──────────────────────────────────────────────────────────────────────┬──────────────────────┤
│  ┌ vm-edge-01 ─────────────┐                                         │ vm-pay-01            │
│  │ traefik 3.1  ● UP       │                                         │ VM · prod · 10.20.4.11│
│  └──┬──────┬──────┬────────┘                                         │ ▮ UP  ping 0.4 ms     │
│  payouts  ledger  dashboard ⚠ 8.4 s   flows  flows-stage ✖           │ hosts 3 · routed 3    │
│     │        │        │                 │        │                    │ ──────────────────── │
│  ┌ vm-pay-01 ──────────────────┐   ┌ vm-lowcode-01 ────────────┐     │ Properties …         │
│  │ [payout-service 1.8.2 ●]    │   │ [flowrun-prod 4.1.7 ●]    │     │ Relations …          │
│  │ [ledger-api 2.3.0 ●]        │   │   sepa v12 · invoice v7…  │     │ Monitoring ▮▮▮▮▮▮▮▮  │
│  │ [payout-worker ✖ exited]    │   │ [flowrun-stage 4.2.1 ✖]   │     │ Blast radius: 5      │
│  └──────────┬──────────────────┘   └───────────────────────────┘     │ [Show what breaks]   │
│        ┌ vm-db-01 ───┐      ┌ vm-pay-stage-01 ─────────────────┐     │                      │
│        │ pg-payments ●│      │ [payout-service 1.9.0-rc1 ●] …   │     │                      │
│  legend ● up ⚠ warn ✖ down   ── runs-on/routes-to   ╌╌ suggested     │                      │
└──────────────────────────────────────────────────────────────────────┴──────────────────────┘
```

## How the surfaces land
- **A1** — the map. Hand-laid layout (~22 nodes from `assets.md`): host boxes with nested container/service chips, routes as labelled edges from Traefik to their targets, DB nodes, Flowrun instance boxes with scenario chips. **Layers** (prod / stage / dev / monitors / scenarios) toggle visibility; env can alternatively be swimlanes — your call, say which. Pan (drag background) + zoom (+/−/fit). Search dims non-matching nodes.
- **A2** — clicking a node opens the right side card (same card component as the room's detail panel). Includes **Blast radius**: *Show what breaks* lights every node that depends on this one (transitively, via runs-on / routes-to / depends-on) in amber and lists them; this is the signature moment.
- **A3** — *+ New asset* opens a dialog; the created node appears on the map (placed near its runs-on host if given). *Import from source* previews nodes to add.
- **A4** — **drag a node onto another** → relation-type popover (runs-on / routes-to / depends-on / monitored-by …) → edge drawn. Suggested links are dashed edges plus rows in the card; Confirm makes them solid.
- **A5** — monitors are lamps on their asset nodes; a *Monitors* layer shows them as small satellite nodes with response time; the monitoring list is a panel (top-bar button) with the state filter; alerts in the inbox; *Create monitor* from the card.
- **A6** — Flowrun instance boxes hold scenario chips with version; click a chip → scenario card (per-env versions, runs, log); *Promote* from the card opens the review dialog, then the prod chip flips to v13.
- **A7** — the room's ASSETS tile is a **mini-map** of that context's assets (same renderer, reduced) with health lamps; clicking opens the full map focused on those nodes. `asset:` search results list nodes and pan to them on Enter.

## Signature (Signal)
Departure-board lamps on nodes (flap cells for state), hairline edges, a strict grid layout so it reads like a wiring diagram on a dark panel, and **blast radius** as the one amber moment. No glow, no physics.

## Don't
- Don't make the map decorative — select, filter, link and blast radius must work.
- Don't exceed ~25 nodes; legibility over completeness (scenarios can collapse into a count).
