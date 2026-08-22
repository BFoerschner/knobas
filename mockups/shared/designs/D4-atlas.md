# D4 — Atlas

**Identity:** a navigator's atlas at night. Deep indigo, faint contour lines, parchment-toned text, gold and teal accents. Entities are places, links are routes, navigation is an itinerary. Quiet, literary, precise — a chart table, not a dashboard.

## Tokens
- Background `#111827` → use `#121A2C` indigo · Panel `#182238` · Raised `#1F2B45` · Contour/hairline `#27344F`
- Text parchment `#EAE4D3` · Muted `#A5AEC2` · Faint `#6C778F`
- **Gold `#D4A857`** — primary action, active, the running timer, selection
- **Teal `#3FB8AF`** — links, routes, confirmed connections
- Failed `#E2735B` (coral) · Success `#8CC8A0` (small) · Suggested route: teal dashed at 50 %
- Source identity: small-caps text labels (JIRA · WIKI · GIT · CI · NOTE) in muted parchment, with a tiny map-symbol glyph per source (inline SVG: a pin, a book, a fork, a beacon, a flag).

## Type (Google Fonts)
- Display / headings: **Newsreader** 500 (use `font-variant: small-caps` for labels and eyebrows; Newsreader has true small caps via `font-feature-settings: "smcp"`)
- Body & UI: **Source Sans 3** 400/600
- Keys, coordinates, times: **DM Mono** 400 — treat times like coordinates (`14:32`, `0:34`).

## Shape & density
- 3 px radii. Panels separated by hairlines in `#27344F`. Background has a subtle contour-line pattern (inline SVG or repeating radial gradients at ~6 % opacity) — visible, not loud. Medium density, generous line-height for reading.
- Selected item gets a gold left rule, like a route marker.

## Signature
**Routes and itineraries.** Relationships are rendered as map routes: dashed teal paths with small waypoint dots, drawn between linked entities on the detail view (an SVG "route strip": PAY-231 ● ─ ─ ● PR #142 ─ ─ ● #1187 ─ ─ ● page). Navigation history / breadcrumb is an *itinerary*: "PAY-231 → PR #142 → #1187", with the time spent at each stop if the timer was running (this is where the day review lives naturally: the day as a journey with legs). A small compass rose (inline SVG) marks the current context in the persistent strip; confirming a suggested link draws the route in (stroke-dashoffset animation, 400 ms).

## Motion
Route draw-in, 150 ms fades. No parallax, no glow.

## Avoid
Literal world maps, photos, sepia filters, Cormorant/Playfair headline clichés, the "dark + single neon accent" look — gold and teal are both present and both muted.
