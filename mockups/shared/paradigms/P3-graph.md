# P3 — Graph

**Thesis:** the home screen is a knowledge-graph canvas. Entities are nodes colored by source, confirmed links are edges, suggested links are dashed edges. You see the shape of your work — which ticket pulls which branch, build, page and note — and make connections by dragging. Search filters and highlights the graph; detail opens beside it.

```
┌──────────────────────────────────────────────────────────────────────┐
│ ⌕ sepa retry          [all ▾] [my work]        ◔ 0:34 PAY-231  ✉ 6 ⚙ │
├───┬─────────────────────────────────────────────────┬────────────────┤
│ ⌂ │                (page) SEPA design               │ PAY-231        │
│ ✉ │                 ╱        │                      │ Retry failed…  │
│ ◔ │   (note)───(PAY-231)──(PR #142)──(#1187 ✖)      │ In Progress ▾  │
│ ≡ │              │  ╲ ╌╌╌╌╌╌╌╌╌╌╌(a41f2c) ← dashed  │ High · Mara    │
│ ⚙ │          (PAY-200 epic)     (#1188 ◌)            │ ─────────────  │
│   │              │                                   │ Links (6)      │
│   │          (PAY-228)──(PR #139)──(runbook page)    │ Suggested (2)  │
│   │              ╲╌╌╌(OPS-77)                        │  a41f2c ✓ ✗    │
│   │   legend: ● jira ● confluence ● gitea ● teamcity │ Comments       │
│   │           ● note   ── link  ╌╌ suggested         │ [Start timer]  │
└───┴─────────────────────────────────────────────────┴────────────────┘
```

## Where the eight surfaces live
1. **Search + smart lists** — search bar over the canvas: typing dims non-matching nodes and lists matches in a dropdown; smart lists are *lenses* (chips) that re-filter the graph ("My tickets with a failing build" lights PAY-231 + #1187).
2. **Entity detail + suggestions** — select a node → side card (right) with full detail; suggested links are dashed edges on the canvas *and* rows in the card; confirm turns the edge solid.
3. **Notes** — note nodes on the graph; opening one shows the editor in the side card (or a wider drawer); `[[` links create edges.
4. **Sources** — gear → overlay/page: sources as the graph's "layers" (toggle visibility per source, sync state, add source).
5. **Actions** — context menu on a node (right-click or ⋯): *Start work* on PAY-240 animates new nodes (branch, PR) appearing and linking; *Trigger build* on a build-config node; *Create ticket* adds a node.
6. **Inbox** — overlay list from the top bar; items point at their node (clicking an item pans/highlights it).
7. **Time** — the active-context node has a pulsing ring; timer in the top bar; stopping opens the worklog draft; day review is a horizontal strip overlay at the bottom whose blocks are colored by the node they belong to; timesheet as a panel.
8. **Standup** — overlay document (digest + protocol); each digest line highlights its node on hover.

## Implementation guidance
- SVG canvas with **precomputed node positions** (hand-laid layout from the dataset); no physics simulation required. Pan/zoom nice-to-have. Node drag to link (drop on another node → "Link PAY-240 → page?" confirm) is the signature — make at least one drag-to-link work with mouse events.
- Keep the graph small and legible: ~14 nodes from the dataset, not a hairball.

## Don't
- Don't make the graph decorative — selection, filtering and linking must actually work.
- Don't bury the detail: the side card must show the full PAY-231 content.
