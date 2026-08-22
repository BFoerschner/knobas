# Playground brief — "infinite depth" asset topology

A standalone idea page, separate from the rounds. One self-contained HTML file, vanilla JS/CSS, Google Fonts only, opens from `file://`, desktop proportions (1440×900, usable at 1100), **D1 Signal** design (see `../shared/designs/D1-signal.md`: graphite, hairlines, mono readings, amber only where something is wrong or selected; no gradients, no glow).

## What the page must let the user do
1. **Drill down without a bottom.** Every asset can hold assets. Click into a VM and see what it runs; click a container and see the service; the service may be a runtime hosting many apps; an app holds steps; a step holds connectors; a database server holds databases › schemas › tables. There is no fixed depth — the UI must work the same at level 2 and level 9.
2. **Always know where you are** (path / breadcrumb / zoom level) and get back up quickly.
3. **Create something anywhere**: at any level, add a new asset (type, name, one-line summary, status, properties). Suggest the usual child types but allow any type, including a custom one. Created assets persist in `localStorage` (a Reset button restores the example).
4. **See exposed routes from both ends.** A reverse proxy, a runtime or a service *exposes* routes (URL → target asset, or an endpoint with no target). Selecting the exposer shows its routes and where they land; selecting any asset shows what routes reach it — directly, or through an ancestor that holds it ("reached via Traefik → Flowrun prod, which holds this"). Let the user add a route to any asset.
5. Problems (down/warn) must be visible from above: a collapsed/unexpanded asset shows that something inside is broken.
6. `/` focuses search; matches are revealed wherever they are. `Esc` steps back (close dialog → deselect → go up). Keyboard focus visible; reduced motion respected. No `alert()`.

## Data
Use the example exactly as in `topology-explorer.html` (`SEED_NODES` and `SEED_ROUTES` arrays near the top of its `<script>`): 78 assets, 8 levels deep, 15 routes. Copy those two arrays verbatim (you may change the rendering, not the data). The page you build must NOT look like that file's nested boxes — the user rejected that presentation.

## Header comment (first lines of the file)
```html
<!--
knobas playground
example: E2 Semantic zoom
design: D1 Signal
thesis: <one line — how this way makes infinite depth feel natural>
thesis: <one line — the single signature interaction>
-->
```

## Report back (raw data)
path, line count, the two thesis lines, how drill-down / breadcrumb / create / routes are done, what you verified in headless Chrome, known gaps.
