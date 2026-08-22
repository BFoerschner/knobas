# W1 — Inventory

**Thesis:** a typed inventory you can scan at speed. Assets are rows on a board; the left rail is a type list with counts; the board can be grouped by type or by host (a tree: VM › containers › services › routes); each type has its own columns. Built for the day there are 300 assets, not 20.

```
┌─ CONTEXT [PAY-200 ▾]  PAY-231 · Staging DB   ◔ 0:34  ✉ 8  ● ● ● ✖ ● ●  Assets ─────────────┐
│ ▌ asset: /assets  env:prod  health:⚠                                              ⌘K      │
├────────────┬─────────────────────────────────────────────────────────────────────────────┤
│ ALL     27 │ group by [type] [host]      filter: env ▾  health ▾  owner ▾     + New asset  │
│ VM       6 │ VM        HOST             IP          SIZE          ENV    HEALTH   WIRED   │
│ Container6 │ vm-pay-01                  10.20.4.11  8c·32G·200G   prod   ▮ UP     CT CT CT│
│ Service  4 │   ├ ct  payout-service     :8080  1.8.2              prod   ▮ UP     SV RT   │
│ Route    6 │   ├ ct  ledger-api         :8090  2.3.0              prod   ▮ UP     SV RT   │
│ Database 2 │   └ ct  payout-worker      —      1.8.2              prod   ▮ DOWN   MO      │  ← amber flap
│ Runtime  3 │ vm-pay-stage-01            10.20.5.11  4c·16G·120G   stage  ▮ UP            │
│ Scenario 4 │   ├ ct  payout-service     :8080  1.9.0-rc1          stage  ▮ UP     BD SV   │
│ Monitor 10 │ …                                                                            │
│────────────│──────────────────────────────────────────────────────────────────────────────│
│ SUGGESTED 4│ ▸ r-dashboard → svc-dashboard  host name matches    [Confirm] [Dismiss]       │
└────────────┴─────────────────────────────────────────────────────────────────────────────┘
```

## How the surfaces land
- **A1** — the board above. Type rail (counts, click = filter), group-by toggle (type / host), per-type column sets (VM: host, IP, size, hypervisor, env, health, wired; Container: name, image:tag, ports, runs-on, state, env, health; Route: URL, target, cert, visibility; Scenario: name, dev/stage/prod versions, trigger, last run…). Dense rows (28–32 px), sticky header, sortable columns, keyboard ↑↓ + Enter.
- **A2** — selecting a row opens a detail panel on the right (room-style slide-over, ~480 px); the board keeps its scroll position. Properties as a two-column ledger; custom properties inline-editable; relations as monogram chains with relation labels; monitoring block with the 48-segment bar; history as a dated list.
- **A3** — *+ New asset* inserts an **inline row form** at the top of the board (type select first, then that type's columns become inputs); *Create* turns it into a real row with a flap "NEW" that settles. *Import from source* is a dialog with a preview table.
- **A4** — `Link to…` in the detail panel (relation type + target search). Multi-select rows (checkbox column) → bulk action bar: *Link selected to…*, *Add to context*, *Set owner*. Suggested links as a collapsible strip under the board.
- **A5** — Monitor is a type in the rail; the monitor rows show state / response / uptime bar / cert days; *Create monitor* pre-filled from the selected asset; alerts in the inbox.
- **A6** — Runtime and Scenario are types in the rail; the scenario rows show the three env version cells side by side with drift marked; *Promote* from the row's action menu opens the review dialog; *Run now*, *View log* likewise.
- **A7** — the room's ASSETS tile is a mini version of the board (same row component, fewer columns); `asset:` search results reuse the row component.

## Signature (Signal)
Health, state and version are **split-flap cells** that flip on change (restart container → DOWN → RUNNING; promote → v12 → v13). The *wired* column is a chain of 2-letter monograms (VM CT SV RT DB MO FR SC) like airline codes; hovering one highlights its row. Amber only on warning/down rows and on the ticker.

## Don't
- Don't turn it into cards; it is a board.
- Don't hide the type rail or the counts — they're the orientation device.
