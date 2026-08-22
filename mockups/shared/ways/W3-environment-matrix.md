# W3 — Environment matrix

**Thesis:** what runs where. Rows are the things you deploy (services, scenarios, routes, databases); columns are environments (dev · stage · prod); every cell is that thing's instance in that environment — version, host, health lamp, URL, last deploy/run. Drift between columns is the headline; promoting is a gesture across columns. Hosts (VMs, containers) are the "where" behind each cell.

```
┌─ CONTEXT [PAY-200 ▾] …   ◔ 0:34  ✉ 8   Assets ─────────────────────────────────────────────┐
│ ▌ asset:                       rows: [services] [scenarios] [routes] [databases] [hosts]    │
├──────────────────────┬──────────────────┬────────────────────┬──────────────────────────────┤
│                      │ DEV              │ STAGE              │ PROD                         │
│ SERVICES             │                  │                    │                              │
│ payout-service       │ —                │ 1.9.0-rc1  ● UP    │ 1.8.2  ● UP        ← drift ─ │
│   where              │                  │ vm-pay-stage-01 ct │ vm-pay-01 ct · payouts.…     │
│ ledger-api           │ —                │ 2.3.0  ● UP        │ 2.3.0  ● UP                  │
│ payout-dashboard     │ —                │ —                  │ 0.9.4  ⚠ 8.4 s  PAY-240      │
│ SCENARIOS (Flowrun)  │ flowrun-dev 4.2.1│ flowrun-stage ✖    │ flowrun-prod 4.1.7           │
│ sepa-payout-export   │ v14  ● ok        │ v13  ✖ failed 12:40│ v12  ● ok 13:00   [Promote →]│
│ invoice-sync         │ v7               │ v7                 │ v7  ● ok 14:00               │
│ refund-reconciliation│ v3  ● ok         │ —  [Deploy →]      │ —                            │
│ ROUTES               │                  │ payouts-stage.…    │ payouts.…  cert 9 d ⚠        │
│ DATABASES            │                  │ pg-payments-stage ●│ pg-payments ●                │
│ HOSTS                │ vm-lowcode-dev-01│ vm-pay-stage-01 …  │ vm-pay-01 · vm-db-01 · …     │
└──────────────────────┴──────────────────┴────────────────────┴──────────────────────────────┘
```

## How the surfaces land
- **A1** — the matrix. Row groups (services / scenarios / routes / databases / hosts) collapsible; column headers show the environment's instance state (flowrun-stage ✖). Each cell: version flap, health lamp, a second line with host and URL. **Drift**: when stage ≠ prod (or dev ≠ stage) a thin amber rule connects the differing cells and a *drift* badge appears on the row. Filters: health, owner, "only drifted". Row groups also expandable into their hosts ("where").
- **A2** — clicking a cell opens the detail panel for that instance (container / scenario-in-env / route); clicking the row label opens the logical asset (service / scenario) showing all environments. Same panel component as the room.
- **A3** — *+ New asset* dialog with type picker; new services/scenarios appear as rows, new VMs under Hosts. *Import from source* previews rows.
- **A4** — *Link to…* with relation type from the panel; **drag a cell onto a host row** sets runs-on. Suggested links in a strip below the matrix and on the asset.
- **A5** — health lamps per cell come from monitors; a *Monitoring* toggle expands each cell to show its monitors' response and 24 h bar; monitor list panel with state filter; alerts in the inbox; *Create monitor* from a cell.
- **A6** — scenarios live natively in the matrix. **Promote**: press *Promote →* on a cell (or drag the cell to the next column) → review dialog → the target cell flips to the new version, a "queued run #8813" line appears, history updated. *Deploy →* for rows missing in an env. *Run now* / *View log* on the cell menu.
- **A7** — the room's ASSETS tile is a **reduced matrix** (only that context's rows; PAY-231 shows the stage column emphasised); `asset:` search results open the matrix scrolled to the row with the cell highlighted.

## Signature (Signal)
Version cells are **split-flap cells that flip on promote**; drift is a thin amber rule between cells; the three columns read like a runway board (DEV · STAGE · PROD as board headers in condensed caps). Everything else hairlines and mono.

## Don't
- Don't turn the matrix into cards; cells are compact board cells.
- Don't hide hosts entirely — "where does it run" must be one expansion away.
