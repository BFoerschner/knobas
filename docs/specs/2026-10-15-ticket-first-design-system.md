# Ticket-first design system

The design section of the ticket-first spec. The spec includes this file by reference. It writes down what `mockups/ticket-first/prototype-6.html` already draws, so every UI ticket builds against one system instead of reading CSS out of a 600 KB mockup. It adds no new look, except where [Controls](#controls) settles what the prototype drew inconsistently.

Acceptance for UI work is this file, the `#/controls` specimen (see [Controls](#controls)) and before/after screenshots of the real app at the same route, same theme, 1440 × 900. Prototype-6 is historical since 2026-10-31 (#2238): it decides nothing, and where the app and the prototype differ, this file decides. Its "acceptance is the match" rule in `requirements.md` is superseded.

It replaces `requirements.md` §16, which is a short version of the same content.

## What the design is

knobas is a work tool for one developer who has a dozen tools open behind it. The page is quiet: slate paper, hairline rules, dark ink, no decoration. One thing is loud, and that's the yellow card. It marks the ticket you are on, wherever that ticket appears. The job key sits on it in a very narrow, very heavy Archivo at 64 px. That key is the identity of the app, and everything else stays out of its way.

Rules that follow from that:

- **Yellow means "the ticket you are on".** It goes on the job card, the ticket page's card (`#/ticket/<id>`, spec #2227), the selected ticket on the board and list, and the running timer. Nothing else gets yellow. Highlights use `--mark`, which is a paler yellow and only appears behind matched search text.
- **The width axis does the shouting, not colour or size.** Big figures (the job key, counts, durations) use Archivo at width 62 and weight 800. Page titles use width 85. Body text stays at the normal width.
- **Tones carry state, and only state.** `ok`, `warn`, `err`, `info` and `muted` belong to the shell. Adapters never choose a colour. A source says "failed" and the shell draws it.
- **Structure means something.** A border separates things that are different. A shadow means the thing floats above the page. Labels are sentence case and never all caps. The prototype has no `text-transform: uppercase` anywhere, and the app keeps it that way.

## Colour

Tokens are the prototype's, unchanged. Light and dark come from the same names.

| Token | Light | Dark | Use |
|---|---|---|---|
| `--paper` | `#E4E8EC` | `#12171C` | Page background |
| `--sheet` | `#F7F8F9` | `#1A2027` | Panels, bars, cards, dialogs |
| `--sheet-2` | `#EEF1F4` | `#212932` | Inset areas, table heads, hovered rows |
| `--ink` | `#17212B` | `#E4E9EE` | Text, primary buttons, toast background |
| `--ink-2` | `#4B5663` | `#A0ABB7` | Secondary text |
| `--rule` | `#CBD2D9` | `#323C47` | Borders between things |
| `--rule-2` | `#DDE2E7` | `#28313A` | Row dividers inside a thing |
| `--card` | `#F2D157` | `#D9B941` | The current ticket (see above) |
| `--card-2` | `#E8C440` | `#C9A930` | Hovered or pressed card surface |
| `--card-ink` | `#2A230B` | `#1D1806` | Text on the card |
| `--card-ink-2` | `#5A4D1E` | `#3F3511` | Secondary text on the card |
| `--focus` | `#2F55D4` | `#8AA6FF` | Focus ring |
| `--mark` | `#F6E08F` | `#5C4E17` | Matched search text |
| `--ok` / `--ok-bg` | `#1F7444` / `#DDEFE4` | `#6FCF97` / `#1B3427` | Passed, merged, up |
| `--warn` / `--warn-bg` | `#8A5600` / `#F6EBD3` | `#E5B04D` / `#3A2E14` | Needs attention, stale |
| `--err` / `--err-bg` | `#B0262A` / `#F7DEDD` | `#FF8A80` / `#3F1F1F` | Failed, down, broken |
| `--info` / `--info-bg` | `#2F55D4` / `#DFE6FA` | `#8AA6FF` / `#1F2A4A` | Running, queued |
| `--muted` / `--muted-bg` | `#5F6975` / `#E6E9EC` | `#94A0AC` / `#262E37` | Unknown, no data, not monitored |
| `--shadow` | `0 1px 0 rgba(23,33,43,.06), 0 8px 24px -12px rgba(23,33,43,.25)` | `0 1px 0 rgba(0,0,0,.3), 0 10px 30px -12px rgba(0,0,0,.6)` | Floating things only |

### Contrast, measured

I computed WCAG ratios for every text pair the prototype uses. All text passes 4.5:1 in both themes. The tightest are light `--muted` on `--paper` at 4.53 and on `--muted-bg` at 4.58, so no lighter grey may be added for text.

One pair fails. The focus ring on the yellow card is 4.16:1 in light, which is fine for a non-text indicator, but **1.22:1 in dark**, so keyboard focus on the job card or the selected ticket is invisible there. The fix belongs in the system, not in a ticket: on any `--card` surface the focus ring is `--card-ink`. That's a change from the prototype, and the first ticket that draws a card names it in its PR.

## Type

Three families, each with one job.

- **Archivo** (variable, `wdth` 62–125, `wght` 400–800) for the whole interface.
- **Source Serif 4** (variable, optical size) is the reading face. It sets page bodies and titles in Docs and nowhere else.
- **JetBrains Mono** (400, 600) only for real code values: keys, shas, branch names, paths, logs and diffs. Never for a label or a number that's just a number. Durations and counts are Archivo with `font-variant-numeric: tabular-nums`.

Body text is Archivo 14 px / 1.45, weight 400. Weight 600 is the normal emphasis and covers most headings. 700 is for page titles and 800 for the narrow display figures. The prototype's single 650 becomes 600.

### Scale

The prototype uses about 30 sizes, many of them half-pixel neighbours (11.5, 12.5, 13.5) that nobody chose on purpose. The app uses these steps:

| Step | Size | Use | Prototype sizes it absorbs |
|---|---|---|---|
| `--t-2xs` | 11 px | Badges, axis ticks, meta in dense rows | 10, 10.5, 11 |
| `--t-xs` | 12 px | Secondary text in rows, chips, mono values | 11.5, 12 |
| `--t-sm` | 13 px | Rows, tables, controls | 12.5, 13 |
| `--t-md` | 14 px | Body | 13.5, 14, 14.5 |
| `--t-lg` | 15 px | Panel headings (`wdth` 88) | 15, 15.5 |
| `--t-xl` | 17 px | Drawer and dialog titles, launcher input | 16, 17, 18 |
| `--t-2xl` | 20 px | Job title, preview title | 19, 20, 21 |
| `--t-3xl` | 24 px | Page titles (`wdth` 85, 700) | 22, 23, 24, 25, 26 |
| `--t-d1` | 34 px | Display figure, small (`wdth` 62, 800) | 28, 30, 34 |
| `--t-d2` | 44 px | Display figure (`wdth` 62, 800) | 44, 48 (the job key below 900 px) |
| `--t-d3` | 64 px | The job key | 64 |

The reading face keeps its own sizes because it's a different face doing a different job: body 17.5 px / 1.65, section heads 25 px, page title 40 px (30 px below 900 px). The Docs reader sets no line length: its page runs the full width beside the tree and the outline (R-10.9, amended 2026-10-25, #2078).

A rounding difference of half a pixel to two pixels against prototype-6 is an expected difference and doesn't need to be named in a PR one by one. A PR states once that it uses the scale.

Width is set with `font-variation-settings: "wdth" N`. The prototype mixes this with `font-stretch` in a few places; the app uses only the first, so the value can't depend on how `@font-face` declares its stretch range.

## Shape, depth and spacing

Radii come in four steps. 3 px for chips, marks and the focus ring; 4 px for buttons, inputs and tags; 6 px for cards, panels and tiles (the prototype's 5 px tiles round up); 8 px for dialogs, menus and popovers (its 9 and 10 round down). Circles stay circles.

`--shadow` is only for things that float: the drawer, dialogs, menus, popovers, the combo-box list, and the selected ticket card, which lifts off the board. A resting panel has a border and no shadow. The prototype uses the shadow in exactly those nine places, so this is the rule it already follows.

Spacing follows the prototype's values, which already sit on a 2 px grid with 4, 6, 8, 10, 12 and 16 doing most of the work. Gaps inside a row are 6 to 8 px, padding inside a panel is 12 to 14 px, and gaps between panels are 16 px. There is no spacing token set beyond the four control tokens in [Controls](#controls); reviewers check the rest against the screenshot.

## Controls

Buttons and the controls that sit beside them follow one set of rules (#2238). The visual reference is the dev-only specimen page at `#/controls` (`app/src/lib/tf/controls/`), drawn with the real components and stylesheets; `just shot <slot> <dir> '#/controls'` captures it. The style gate (#2281) enforces the rules.

### Tokens

| Token | Value | Use |
|---|---|---|
| `--control-h` | 32 px | default button height |
| `--control-h-sm` | 26 px | small button height |
| `--control-h-lg` | 40 px | large button height |
| `--control-gap` | 6 px | the gap between neighbouring buttons |

They are on `:root` in `tokens.css`. A button takes a fixed `height` from its size token and horizontal padding only, so its height never depends on line-height or on its content: a `<kbd>` hint inside a button doesn't change its height. Every control's radius is 4 px, except the pill toggles, which are round.

### Three sizes, chosen by place

- **Default** (`.tf-btn`): page headers (`.tf-acts`), dialog footers and the top bar.
- **Small** (`.tf-btn.tf-sm`): everything inside a page body. Rows, section headings, panel toolbars, tab strips, side rails, segmented-control buttons and pill toggles.
- **Large** (`.tf-btn.tf-lg`): only the one decision a screen exists for (the link review's *Link them* / *Not related* / *Decide later*, the ticket preview's go button). No other button is large.

Three sizes is the complete set. A rule outside `chrome.css` doesn't set a button's height, padding, font size or radius.

- **Icon-only buttons** (`.tf-iconbtn`) are square at their row's height: `--control-h` by default, `--control-h-sm` with `.tf-sm` in a page body's row. The top bar's icon buttons are at the header height.
- **Form controls:** a `select` or `input` sitting in a row of buttons takes the row's control height.
- **Exempt:** the × removers inside chips and links (the view editor's ×, a link's ×) are part of the chip, not controls.

### Gap

Neighbouring buttons are `var(--control-gap)` apart, in headers, rows, dialog footers, strips, panel toolbars (the filter bar and its pill toggles), the large decision buttons and Settings alike.

### Header

The title block on the left and `.tf-acts` on the right, on the title line, top-aligned to it. Every page uses this layout; none puts its actions on a line of their own. The actions come in this order:

1. the primary;
2. the other actions;
3. "Open in <source>", always a ghost button;
4. *Sync now* / *Refresh*, always last.

A section heading that carries an action puts it on the right of the heading line at the small size. A page with no header (the job) ends the tab strip it draws with its small actions, centred on the strip.

### Dialog footer

Right-aligned, `var(--control-gap)` apart, at the default size. A ghost *Cancel* comes first, then the primary. An extra action (an edit dialog's *Delete*) goes on the far left. Every dialog has a primary.

### Danger

- `.tf-danger`: red text, at the size of its place. It marks a button that starts a destructive action (a header's or a row's *Delete*).
- `.tf-primary.tf-danger`: filled red. It marks the confirm button of a destructive dialog.

There is no third way to draw danger.

### The specimen

`#/controls` draws one example of every control and arrangement below, each found by its selector. `controls/TfControls.test.svelte.ts` reads this table, so a row added here without its example on the page fails.

| Control | Selector |
|---|---|
| Canonical header and its actions | `.tf-fhead .tf-acts` |
| Default button | `.tf-fhead .tf-acts .tf-btn:not(.tf-sm):not(.tf-lg)` |
| Primary | `.tf-fhead .tf-acts .tf-btn.tf-primary` |
| Ghost "Open in <source>" | `.tf-fhead .tf-acts .tf-btn.tf-ghost` |
| Section heading with a small action | `.tf-ext-h .tf-btn.tf-sm` |
| Tab strip with trailing small actions | `.tf-wtabs .tf-trail .tf-btn.tf-sm` |
| Panel toolbar with a select | `.tf-fbar select` |
| Segmented control | `.tf-fbar .tf-seg button` |
| Pill toggles | `.tf-fbar .tf-fchips button` |
| Row with small actions | `.tf-ext-row .tf-trail .tf-btn.tf-sm` |
| Icon-only button | `.tf-ext-row .tf-trail .tf-iconbtn` |
| Dialog footer with an extra action | `.tf-dialog footer .tf-btn.tf-first` |
| Dialog footer's ghost *Cancel* | `.tf-dialog footer .tf-btn.tf-ghost` |
| Danger, starting | `.tf-btn.tf-danger:not(.tf-primary)` |
| Danger, confirming | `.tf-btn.tf-primary.tf-danger` |
| Large decision with a `<kbd>` hint | `.tf-btn.tf-lg kbd` |

A strip's or a row's trailing group of actions is `.tf-trail`: pushed to the end, centred on the strip, `var(--control-gap)` apart.

### The style gate

`app/src/lib/tf/controls/style-gate.test.ts` reads every stylesheet and component under `app/src/` (a component's `<style>` blocks count) and fails when:

- **(a)** a rule outside `chrome.css` sets `height`, `min-height`, `padding`, `font-size` (or the `font` shorthand) or `border-radius` on a control: a selector whose subject names `.tf-btn`, `.tf-sm`, `.tf-lg`, `.tf-iconbtn` or `.tf-refresh`, or is a `button` inside `.tf-seg` or `.tf-fchips`;
- **(b)** the unported button classes `btn`, `tb-btn` or `x` appear in a stylesheet or in a component's `class`;
- **(c)** a row of buttons has a `gap` other than `var(--control-gap)`: an actions container (`.tf-acts`, any local `…acts` row with or without the hyphen, the bare `.acts`, `.tf-trail`, a dialog's `footer`; a `…facts` name is not one), or a row of controls under another name (the filter bar `.tf-fbar`, the pill toggles `.tf-fchips`, the large decision `.tf-decide`, any `…-btns` row);
- **(d)** an allowlist entry has no reason;
- **(e)** an allowlist entry matches nothing.

The allowlist is `app/src/lib/tf/controls/style-gate-exceptions/`, one `<ticket>.txt` per #2238 cleanup ticket. It holds the violations present when the gate landed, and each cleanup ticket deletes its own lines. New work doesn't add entries: it follows this section.

### Adding a control

A control this section doesn't name is added by amending this section, the `#/controls` specimen and the style gate in the same PR. It is never a rule local to one view.

## Layout

The design width is 1440 × 900. Every view works from 1100 px. There are two breakpoints, 1240 px (side rails and preview panes close) and 900 px (columns stack). The prototype's other breakpoints at 1000 and 1250 px belong to the retired global map and don't carry over.

The top bar and the filter bar stay put, and the content under them scrolls. A board scrolls sideways inside itself. The page never scrolls sideways. Content aligns left. The only centred blocks are empty states and the Docs home.

## Motion

Motion answers something the person did, and there's very little of it.

- A running build or timer pulses its dot (1.4 s, opacity to 35 %). That's the only motion nobody started.
- The Connections drawer slides in from the right (R-6.1), the Docs tree's drawer below 900 px from the left (R-10.9, #2084), and a drawer's scrim fades in 0.15 s.
- A toast rises 20 px into place.
- A dependency tile you navigated to flashes a focus-coloured ring once (1.6 s), so you can find it in the graph.
- Row actions fade in on hover and on focus within the row.

There are no entrance animations on views or sections. With `prefers-reduced-motion` everything above stops, and the dependency flash becomes a static 2 px ring.

Focus is always visible: a 2 px outline in `--focus`, offset 2 px. `--card-ink` replaces it on card surfaces (see Contrast).

## Words

Copy follows prototype-6 word for word, as `requirements.md` says. When a screen needs words the prototype doesn't have, they follow its voice. Sentence case. Buttons name what happens ("Log 1h 20m", not "Submit"), and the toast uses the same verb ("Logged 1h 20m"). Errors say what failed and what to do ("TeamCity sign-in is failing, so it waits there"). An empty view says what would fill it and offers the action.

## Getting it into the app

These are engineering constraints the prototype doesn't have. Each UI ticket that touches them cites this section.

- **Fonts are vendored, not linked.** The app's CSP is `default-src 'self'`, so Google Fonts can't load. The three families come from npm (`@fontsource-variable/archivo`, `@fontsource-variable/source-serif-4`, `@fontsource/jetbrains-mono`) and are bundled, like IBM Plex and Barlow are today. The Archivo import has to include the `wdth` axis; the default file from that package may carry only `wght`. The first slice-1 ticket checks this and pins the import. Without the axis the job key renders at normal width and the screenshot match fails visibly.
- **No inline styles.** `style-src 'self'` drops `style="…"` in a bundle, and `house-rules.test.ts` refuses them. Prototype-6 has 345 inline `style` attributes in its templates. Each becomes a class or a component prop with a class. A ticket that ports a view ports its inline styles too.
- **Two UIs live side by side until slice 5.** The new shell runs beside the rooms UI behind a setting. Their tokens collide by name (`--ok`, `--muted`, `--sans`, `--mono` exist in both with different values). The new tokens are set on the new shell's root element, not on `:root`, and that element sets its own `background: var(--paper)` and fills the window. The rooms UI's `app.css` stays untouched. When slice 5 removes the rooms UI, the tokens move to `:root` and `app.css` goes.
- **The new stylesheet is its own file.** It doesn't append to `app.css`, whose header asks later milestones to append. That rule was for extending the Signal design, and this one replaces it.
- **Theme.** Today's app is dark only. The new shell follows the OS, and the theme toggle and *Settings → Appearance* (R-13a.8) override it through `data-theme` on the shell root, with the same three-way selector the prototype uses (OS dark unless `data-theme="light"`, or forced `data-theme="dark"`).
- **Not a source.** Prototype-6 still contains the retired global map (`renderMap`, `renderGlobalMap` and its `.ma-*`, `.gm-*`, `.mn`, `.minsp` selectors). The job's Map tab is Dependencies only (D11/D12). Don't port those selectors.

The conventions the first shell tickets settled (class prefix, fixtures, mounting, addresses) are in [ticket-first-shell-conventions](2026-10-15-ticket-first-shell-conventions.md).
