# D1 — Signal

**Identity:** a dark instrument panel — the departure board at an airport, the status wall of a CI room. Everything is a reading: status, elapsed time, sync age, build step. Calm graphite, one amber *signal* hue that is reserved for things that are changing or need attention, so when amber appears it means something.

## Tokens
- Background `#15171A` graphite · Panel `#1D2024` · Raised `#24282D` · Hairline `#2E3339`
- Text `#E7E9EC` · Muted `#8B929A` · Faint `#5A6169`
- **Signal amber `#FFB020`** — running, attention, active timer, unread, pending. Use it on maybe 3 % of the screen.
- Failed `#FF6A5C` (restrained) · Success `#7FD1A1` (low saturation, used small) · Link/selection `#9CC3FF` (cool, quiet)
- Source identity: **not** by color — by a 2-letter monogram cell (`JI` `CF` `GT` `TC` `NT`) in the mono face, like an airline code.

## Type (Google Fonts)
- Display / headings: **Barlow Condensed** 500–600, uppercase for section labels with +0.08em tracking
- Body: **IBM Plex Sans** 400/500
- Data, keys, times, counts: **IBM Plex Mono** — every PAY-231, every 0:34, every "4 min ago" is mono and tabular.

## Shape & density
- 2 px radii at most. Hairline dividers, not boxes. Dense rows 28–32 px. Cells align to a strict column grid like a board.
- Icons: thin-stroke inline SVG, 14–16 px.

## Signature
**Split-flap status cells.** Status values (In Progress, FAILED, running, 0:34) live in flap cells: a fixed-width mono cell with a horizontal split line; when a value changes (status dropdown, timer tick, build state, toast) the cell flips (CSS 3D rotateX on the top half, ~350 ms, disabled under reduced-motion). The timer's minutes flip once per minute; the elapsed counter can tick seconds without flipping. Use it for the timer, build status, ticket status, inbox count. Also: a thin amber *marquee* line at the very top that shows the latest change ("#1188 · step 3/5 · cargo test") like a ticker, if the paradigm has a top strip.

## Motion
Flap animation only; everything else is instant or 120 ms opacity. No glows, no blur.

## Avoid
Neon/acid green, glassmorphism, gradients, rounded cards, colored source badges, the generic "dark dashboard with bright accent" look — the amber must stay rare and the data must feel like a board, not a SaaS.
