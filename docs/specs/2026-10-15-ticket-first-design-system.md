# Ticket-first design system

The design section of the ticket-first spec. The spec includes this file by reference. It writes down what `mockups/ticket-first/prototype-6.html` already draws, so every UI ticket builds against one system instead of reading CSS out of a 600 KB mockup. It adds no new look, except where [Controls](#controls) settles what the prototype drew inconsistently.

Acceptance for UI work is this file, the `#/controls` specimen (see [Controls](#controls)) and before/after screenshots of the real app at the same route, same theme, 1440 × 900. Prototype-6 is historical since 2026-10-31 (#2238): it decides nothing, and where the app and the prototype differ, this file decides. Its "acceptance is the match" rule in `requirements.md` is superseded.

It replaces `requirements.md` §16, which is a short version of the same content.

**How it feels wins.** When a rule here and how the app feels to use disagree, how it feels wins, and the rule gets amended (Björn, 2026-11-06).

## What the design is

knobas is a work tool for one developer who has a dozen tools open behind it. The page is quiet: slate paper, hairline rules, dark ink, no decoration. One thing is loud, and that's the yellow card. It marks the ticket you are on, wherever that ticket appears. The job key sits on it in a very narrow, very heavy Archivo at 64 px. That key is the identity of the app, and everything else stays out of its way.

Rules that follow from that:

- **Yellow means the ticket you are on: its page card, its card and row on the Tickets board and list, and the running timer. Nothing else gets yellow. Selection is an outline.** On the board and list the chosen job's card and row are yellow whether or not they are selected; the selected card or row carries a two-pixel ink outline (`--card-ink` on the yellow, with a second ring in `--ink` in dark, where `--card-ink` melts into the page), and the selected card keeps the lift (spec #2396, #2398). Highlights use `--mark`, which is a paler yellow and appears behind matched search text and what mentions you. A chip that names the job is neutral.
- **The yellow card is a light surface in both themes.** Its dots and chips take the light tone values, so a failing build reads on it in dark too: dark's `--warn` is the card's own yellow and its `--err` is pink on it. Every yellow card (the ticket page's, the Tickets board's card and the list's row) re-declares the five tones, their tints and `--mark` at their light values (`job.css`), so the blocked chip, every dot and a matched word follow without a rule of their own (spec #2396, #2400).
- **The width axis does the shouting, not colour or size.** Big figures (the job key, counts, durations) use Archivo at width 62 and weight 800. Page titles use width 85. Body text stays at the normal width.
- **Tones carry state, and only state.** `ok`, `warn`, `err`, `info` and `muted` belong to the shell. Adapters never choose a colour. A source says "failed" and the shell draws it.
- **Structure means something.** A border separates things that are different. A shadow means the thing floats above the page. Labels are sentence case and never all caps. The prototype has no `text-transform: uppercase` anywhere, and the app keeps it that way.

## Colour

Tokens are the prototype's, unchanged, except `--field`, which it did not have. Light and dark come from the same names.

| Token | Light | Dark | Use |
|---|---|---|---|
| `--paper` | `#E4E8EC` | `#12171C` | Page background |
| `--sheet` | `#F7F8F9` | `#1A2027` | Panels, bars, cards, dialogs |
| `--sheet-2` | `#EEF1F4` | `#212932` | Inset areas, table heads, hovered rows |
| `--field` | `#FFFFFF` | `#12171C` | What you type into: every text field, search box and textarea, and the selects beside them. Never `--paper`, which reads as disabled on a sheet |
| `--ink` | `#17212B` | `#E4E9EE` | Text, primary buttons, toast background |
| `--ink-2` | `#4B5663` | `#A0ABB7` | Secondary text |
| `--rule` | `#CBD2D9` | `#323C47` | Borders between things |
| `--rule-2` | `#DDE2E7` | `#28313A` | Row dividers inside a thing |
| `--card` | `#F2D157` | `#D9B941` | The current ticket (see above) |
| `--card-2` | `#E8C440` | `#C9A930` | Hovered or pressed card surface |
| `--card-ink` | `#2A230B` | `#1D1806` | Text on the card |
| `--card-ink-2` | `#5A4D1E` | `#3F3511` | Secondary text on the card |
| `--focus` | `#2F55D4` | `#8AA6FF` | Focus ring |
| `--mark` | `#F6E08F` | `#5C4E17` | Matched search text, and what mentions you |
| `--fill` | `--ink` | `#A9B3BE` | Whatever is filled to say it is on: a primary, a pressed segment, a rail's selected row. In dark it sits below the yellow card's luminance (.44 against .50), so it never outshouts it. A tracked block on Time is this fill too |
| `--ok` / `--ok-bg` | `#1F7444` / `#DDEFE4` | `#6FCF97` / `#1B3427` | Passed, merged, up |
| `--warn` / `--warn-bg` | `#8A5600` / `#F6EBD3` | `#E5B04D` / `#3A2E14` | Needs attention, stale |
| `--err` / `--err-bg` | `#B0262A` / `#F7DEDD` | `#FF8A80` / `#3F1F1F` | Failed, down, broken |
| `--info` / `--info-bg` | `#2F55D4` / `#DFE6FA` | `#8AA6FF` / `#1F2A4A` | Running, queued |
| `--muted` / `--muted-bg` | `#5F6975` / `#E6E9EC` | `#94A0AC` / `#262E37` | Unknown, no data, not monitored |
| `--on-ink-ok` / `-warn` / `-err` / `-info` / `-muted` | `#6FCF97` / `#E5B04D` / `#FF8A80` / `#8AA6FF` / `#94A0AC` | `#0F3F24` / `#4A2E00` / `#6E1014` / `#17297A` / `#2C333B` | The tones on `--fill` (a rail's selected row). Dark's five clear 4.5:1 on the fill, at 5.62 to 6.04 |
| `--shadow` | `0 1px 0 rgba(23,33,43,.06), 0 8px 24px -12px rgba(23,33,43,.25)` | `0 1px 0 rgba(0,0,0,.3), 0 10px 30px -12px rgba(0,0,0,.6)` | Floating things only |

The ticket page's card has two moods and one markup (spec #2396). While the ticket is only read it sits on `--sheet` with `--ink` (`.tf-sheetcard`); once it is the chosen job it sits on `--card` with `--card-ink` (`.tf-card`), and the class is the only difference. The yellow follows the first rule above: the page card, the chosen job's card and row on the Tickets board and list, and the running timer. A selected card or row that is not the chosen job stays on `--sheet` with its outline in `--ink`.

### Contrast, measured

I computed WCAG ratios for every text pair the prototype uses. All text passes 4.5:1 in both themes. The tightest are light `--muted` on `--paper` at 4.53 and on `--muted-bg` at 4.58, so no lighter grey may be added for text.

One pair fails. The focus ring on the yellow card is 4.16:1 in light, which is fine for a non-text indicator, but **1.22:1 in dark**, so keyboard focus on the ticket page's yellow card or the chosen job's card on the board is invisible there. The fix belongs in the system, not in a ticket: on any `--card` surface the focus ring is `--card-ink`. That's a change from the prototype, and the first ticket that draws a card names it in its PR.

The ring inside a `--fill` surface (a rail's selected row, a pressed segment) fails the same way: `--focus` on the fill is 1.10:1 in dark and 2.62:1 in light. There the ring is `--paper`, at 8.48:1 and 13.23:1. Amended 2026-11-05.

## Type

Three families, each with one job.

- **Archivo** (variable, `wdth` 62–125, `wght` 400–800) for the whole interface.
- **Source Serif 4** (variable, optical size) is the reading face. It sets page bodies and titles in Docs and nowhere else.
- **JetBrains Mono** (400, 600) only for real code values: ticket keys, shas, branch names, repository names, file paths, logs and diffs. Never for a label or a number that's just a number. Durations, counts and a number with a hash (`#1187`, `PR #142`) are Archivo with `font-variant-numeric: tabular-nums`. An asset's or a monitor's name is a name, and its path in a dialog is words: Archivo, in every place. A ticket key is mono in every place it can be: a chip, a table, a title, a crumb, *Back to*, a rule's wording. A label inherits no mono from the code around it: a diff's *Rendered / Lines*, a tree row's *not staged* and *Clear the filter* are Archivo.

`TfKey` draws a code value, so this is one component and not a class applied by hand. A line that holds keys among words (a signal's *blocked by OPS-77*, an epic's *PAY-200 Payout reliability*) goes through `TfKeyed`, which draws each key with it. It is 600 where it leads a row, a card or a pane (`lead`) and 400 in a sentence. As a `chip` it holds the chip's size, and it names the job without yellow. A breadcrumb and *Back to* find a ticket key by themselves (`isKey`), and a crumb that is another code value, such as a repository's name, says so with `code`. A dialog's title given as text goes through `TfKeyed` by itself. A key in a button takes the button's weight. The launcher is the one exception to a code value in mono: its key cell is, its titles are not (#1422, #2421). A native `select` has one face, so a select of branches is mono and a select that mixes keys and words is not.

Body text is Archivo 14 px / 1.45, weight 400. Weight 500 is for buttons and for a row's title, the name you scan a list by: `--t-md` where the row has a second line, `--t-sm` in a dense table. Weight 600 is the normal emphasis and covers most headings. 700 is for page titles and 800 for the narrow display figures. The prototype's single 650 becomes 600.

Archivo's word space is narrow, so at interface sizes words run together. The whole interface sets `letter-spacing: .006em; word-spacing: .06em` once, on `body`. Code surfaces and the Docs reading face reset both to `normal`: code has to keep its columns, and Source Serif already has its own fit. A placeholder is `--muted`, the lightest grey that passes 4.5:1 on every field fill.

### Heading steps

One step per level, on every page and in every column of it:

| Level | Style |
|---|---|
| Page title | `--t-3xl`, 700, `wdth` 85 (a code value as a title holds to `--t-2xl`) |
| Dialog or drawer title | `--t-xl` |
| Panel or section heading, side rails' included | `--t-lg`, 700, `wdth` 88 |
| A group's label inside a panel, a rail or a section | `--t-xs`, 600, `--ink-2` |

A panel's heading goes above its box, with the panel's action on the same line, and what follows it is the box. `.tf-panel > header` draws this, on every page. A panel with no heading is a plain box.

Markdown a file or a comment holds keeps its own order above its body: h1 `--t-2xl`, h2 `--t-xl`, h3 `--t-lg`, h4 `--t-md`.

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

The reading face keeps its own sizes because it's a different face doing a different job: body 17.5 px / 1.65, section heads 25 px, page title 40 px (30 px below 900 px). The Docs reader sets no line length: its page runs the full width beside the tree and the outline (R-10.9, amended 2026-10-25, #2078). Its own width decides when the outline folds, not the window's: where the header's actions would stop fitting one row beside it, or the reading column would drop under 660 px. Only where the row would not fit even with the outline folded does *Sync now* move into *More*, so no button wraps alone. *Refresh page* keeps its name at every width.

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
- **Large** (`.tf-btn.tf-lg`): only the one decision a screen exists for (the specimen's *Link them* / *Not related* / *Decide later*, the ticket preview's go button, the ticket page card's foot, and *Run all steps* while Start work's steps are unrun, when the foot steps aside; once a step has run the foot is back and the stepper's button is default size). No other button is large.

Three sizes is the complete set. A rule outside `chrome.css` doesn't set a button's height, padding, font size or radius.

- **Icon-only buttons** (`.tf-iconbtn`) are square at their row's height: `--control-h` by default, `--control-h-sm` with `.tf-sm` in a page body's row. The top bar's icon buttons are at the header height.
- **Form controls:** a `select` or `input` sitting in a row of buttons takes the row's control height from its token, with horizontal padding only. The filter bar is a page body's toolbar, so its search (`.tf-fsearch`) and its selects (`.tf-fsel select`) are `--control-h-sm`, as the segmented control and the pill toggles beside them are.
- **Pill toggles** (`.tf-fchips`, and Standup's people are one): off is outlined; on is ringed in `--ink` on `--sheet-2`, not filled. Several can be on at once, and a row of filled pills outweighed the one action the page exists for (reviewers, Rounds 7 and 8, on Standup's *Publish*). Only a primary, a pressed segment and a rail's selected row are filled, and a segmented control has exactly one on. Amended 2026-11-05. A pill's class is `.tf-fchips` alone, and the gate refuses another.
- **Chip buttons** (`.tf-chipbtn`): a button that is part of a chip's text, drawn as that text. The State colours chip's word, which is its grip, and its *Reset* are the two (spec #2463). No border, background or padding, and the chip's own font; `chrome.css` draws it, and a view adds no size to it. Text-only styling (an underline, a weight) is the view's.
- **Exempt:** the × removers inside chips and links (the view editor's ×, a link's ×) are part of the chip, not controls.

### Labels

- **An ellipsis** ends a label when the press asks for more before anything happens: a dialog, a confirm, a menu that needs a choice, a panel that opens in the row (*Merge…*, *Edit…*, *Close…*, *Update branch…*, *Add label…*, *Move…*, *Import…*, *Delete…*, *Add a source…*, *Add a block…*, *Stop and log…*, *Save as a view…*, *Snooze…*). A press that acts at once has none (*Refresh*, *New link set*, *Add a rule*, a *Delete view* with *Undo* on its toast), and neither does a link or a card's row that goes to another place, where the asking happens if it does (*Open Settings › Sources*, *Link something to it*). It is the one character `…`, never three dots, and never in a dialog's title or a section's name.
- **No `+`** in a label. The verb says it adds.
- **`Open in…`** with its caret is a menu of this machine's applications, and only that. **`Open in <source>`** is a link to the thing on its source, whole, with no caret. Both are ghost buttons wherever they stand: a header at the default size, a body at the small one. Where a header has no room, `Open in <source>` is its glyph with the name as its title and label. It is never shortened to words that read as the menu.
- **`Refresh`** reads the one thing on the page; **`Sync now`** runs the sources behind the view. They stand together only where a page is both, and then *Refresh* names its thing: *Refresh run*, *Refresh pipeline*, *Refresh page*.

### Disabled

No fill and `--muted` text: a disabled primary is on `--sheet-2` with a `--rule` border, a disabled ghost has a transparent border and a title that says why, and any other disabled button has no fill at all. In both themes any fill means it can be pressed. A border on a disabled ghost made it look more pressable than a working one, because a working ghost has none (Amended 2026-11-05). The rule is one rule in `chrome.css`. A control that works and opens the reasons (*Merge…* when the source blocks it) is enabled, drawn as a default button, not as a disabled primary. 

**The enabled rule.** A primary is disabled until there is something to do (a pattern with neither a project nor a name, a move to where the asset already is, a link set with no name or nothing ticked), and a line says what is missing: a tooltip alone is not enough. In a dialog the line is `TfDialog`'s `why`, which draws it in the footer just left of *Cancel*; a primary that is off only while its send is out needs none. Outside a dialog it is the same markup, `<p class="tf-dwhy" role="status">`: under the primary on a form that is a page (Compare), beside the buttons on a settings row (*Export…*). The gate refuses any other. Needed fields are marked in words, a `.tf-req` *required* in the label, and nothing is preselected among things the reader chooses between: *Apply a link set…* picks nothing until a row is picked or a filter is typed, when the top match is picked so `Enter` applies it (#2289), and *Add a pattern…* on pages waits for a space. A list with one choice has nothing to choose, so that choice is taken. *New asset* takes the type when the parent's conventions name exactly one, because that is what goes there and a new one stays a name and `Enter`; where they name several or none it waits for a pick. *Save as a link set…* is the one exception, by Björn's ruling (2026-11-06, R-7.9): its ticks start on what the ticket already links and its rule on the ticket's epic when it has one, because it saves what is there and removes nothing. **What removes or replaces data is never preselected, whatever the source declares** (Björn, 2026-11-06): Restore's and Import's lists run newest first with nothing picked, so the pick is one press and no press restores by itself, *Remove…* starts on *Keep what it synced*, never on *Delete*, *Merge…* never starts with its *Delete the branch* ticked, and *Update branch…* starts on merge and never on rebase, which rewrites history. A source's action starts on the choice it names. Where it names none the choice starts unpicked, unless it has one option, because the first of a list is a wrong guess one key away (*Add label…*, *Request…*). *Merge…* keeps its first style, because that is the source's own order, it removes nothing, and the dialog is already the decision.

**The press rule.** After a press, the control or a toast says what happened. A control that can't act is disabled with its reason, and it doesn't open a dialog that can't finish, unless the dialog is where the reasons are (*Merge…* when the source blocks it). A write's toast says *Queued: …* with *Show queue*, and *Sent: …* in place once the write has gone, because a queue it no longer sits in is nothing to offer; a write whose effect shows somewhere else offers that instead (*Show in runs*, the `where` of `toastQueued`, F-350). Withdrawn in the queue the toast goes, and held or refused it stays while a second toast says why. *Run all steps* counts several writes in one toast and stays as it is, because the stepper's own rows say how each went. A failed *Refresh* is a banner with its own small *Try again* on each page that has one. *Retry waiting writes* is off with *Nothing is waiting.* when nothing is, reads *Retrying…* while it runs, and toasts what is left. *Run all steps* reads *Waiting on 3 writes* once it ran, *Step 2 failed* where the flow stopped, or *All steps done*, and is off, with *Show queue* beside it and the card's foot back with the timer. A press that takes the chosen job from another ticket says so with the way back (*On PAY-240 now*, *Back to PAY-231*). *Start work* chooses nothing: it shows the steps on the card's sheet with *Cancel*, so `Esc` and *Cancel* leave everything as it was, and the run is what chooses, with its toast naming the job it replaces (amended 2026-11-06, F-130; spec #2396 had the press choose). A toggle shows what is drawn, so a choice that can't be drawn here is disabled with a title and its key says the same: in a diff column under 900 px *Unified* is pressed, *Split* is off and `s` toasts why, and the choice comes back in a wider window.

**A press keeps your place** (`focus-keep.ts`, which the shell installs once). When the control that had focus goes away or is disabled, focus goes to the same control in that row, else the same control in the row that took its place, or the one before at the end of a list, else that row's first control, else the page's heading. It stays out of an open dialog, which gives focus back itself, and out of a return to a place, which is `return-place.ts`'s. A page that moves focus itself on the same press has got there first. A control that is busy for a moment uses `aria-disabled`, so focus stays on it (*Sync now*). A move made by key with no place to return to focuses the new page's heading (`tabindex="-1"`, no ring), so the next `Tab` starts on the page and not back at the top bar (F-052).

**The removal rule.** A press that removes something either confirms in a dialog or acts at once with *Undo* on its toast. Repeats collapse into one toast, whatever raised them: `push` treats the same words in the same tone as that toast again, on a new time, and two *Undo*s become one that takes back both (F-630). `undoable(text, undo)` is the short way to write a press with its way back, and `tally`, in `shell/toasts.svelte.ts`, is for toasts that count what they hold. A press with no reverse command behind it uses `heldWrite(text, send, undo)`: the page hides the thing at once and the write goes when the toast ends, so *Undo* only has to put it back, and a quit inside those seconds sends nothing (F-339). Each collapsed *Undo* puts back all of them: a set editor's members, patterns and rules, a row of a settings list, the Inbox's answers. In the set editor focus moves to the row that took the removed one's place, else to the panel's way of adding one. What removes files or can't be rebuilt confirms: a pull request's worktree does, and so does a route's *Delete route…*. *Undo* puts back what was there or it is not offered: the asset pane's and the Connections drawer's *Unlink* have it for a link drawn by hand, and *Unlink…* confirms for a link from anywhere else, because `create_link` records whatever it draws as drawn by hand. The Inbox's *Snooze* and *Done* follow it, undoing by a snooze until now, which clears both in the backend. *Ack* is knobas's own, so it follows it too: *Undo* on its toast, and on Monitors *Acked* is a press that takes it back, so the alert asks again. Its toast also says the alert stays open until the monitor recovers. An *Undo* or an *Ack* that reaches an alert already closed says so in one sentence on the Inbox, Monitors and the launcher (*payout-api has already left the inbox. It was resolved at its source.*).

### Dialogs

One pattern, `TfDialog`: the title line with its × on the right (every dialog but the launcher, which has its own), an optional sub line, the body, the footer. A title says what the dialog does to what (*Delete Payout stack?*, *Move knobas-gitea*, *Link SEPA payout retry design*, *Add a route to knobas-gitea*), with no ellipsis, and the body says what follows without asking again. A route has one verb, from its opener to its primary (*Add a route…*, *Add route*). A delete's primary names the kind (*Delete link set*, *Delete asset*). A list the dialog is about keeps its height in a short window, and the body scrolls: a body's children don't shrink. **Arrival focus:** the first field, of any kind, else a control in the body that isn't a button or a link (a radio, a tick), else the ×. It is never a button in the body, so `Enter` straight after opening never presses what the dialog is asking about (*Send mine anyway*, F-035). `Enter` on a radio presses the footer's primary when it can be pressed, unless that primary is danger or marked `data-no-enter` (*Merge*), so a keyboard reaches a harmless write without tabbing past *Cancel* (F-240, Key table, amended 2026-11-06). `Enter` on a tick sends nothing. A field in a closed fold is not where it goes; a dialog whose first question arrives after it opens takes focus when it does. A control removed while it holds focus hands it back to the dialog by the same order, and a pick that takes its box away moves focus to the primary it made ready. **The worklog draft after a stop:** `Esc` and the × keep the timer running, as *Cancel, keep running* does (Björn, 2026-11-06, reversing #896's *Stop, log later*): leaving a question unanswered changes nothing. **The scrim:** a dialog with a footer ignores a click on it and focus stays where it was, because typed work is not lost to a click beside the sheet. A viewer has no footer, and the launcher's only names keys: those two close (`scrimCloses`). A combo box with no list drawn passes `Esc` on, so one press closes the dialog. *Change…* on a settings row opens a dialog, not an inline field that moves the rows below it. A credential with nothing stored is entered, not re-entered: *Enter token…*, *Enter the Uptime Kuma token*, with no word of where one is kept. A toast does not sit on a dialog's footer: however short the window, a dialog leaves 52 px under it, and while one is open the newest toast stands there and the older ones wait.

A row in the launcher takes the selection only when the pointer really moves, never because it appeared under a still one, and new words start on the first row. Its jump records the page it was pressed over, so the bar and `Esc` come back from it, a top page included; a card row's narrowing still ends (overrules #2229's "the launcher gets none"). Tab's actions hold one order from row to row: what the kind does first, on a ticket *Work on this* and *Start timer*, then the pair with the chosen job, then *Open in*, *Favourite* and *Copy* (overrules the 2026-11-02 ruling's "Link first", because a chosen job shifted every position and `Tab`, `Enter` wrote a link). The launcher's box shows only what the reader typed. `/`, ⌘/ and the top bar's button open it on the section's filter, which the query holds and the box draws as a chip named for the section (*Code*), with an × and Backspace on the empty box as its ways off, and no copy of its kinds in the bar under the box. `Esc` takes the words and then closes, so an opening with no words closes in one press wherever it was opened. A place of the app whose name starts with the words is a row under *Go to*, above the answer and selected first; Settings' sections go by their own names, and what a place holds counts only where nothing else answered. Over an answer a filter means the reader is narrowing, so it offers no place; where it finds nothing, the place its words name is offered above *Search everything*, because a dead end one key from the page is no use (F-042). An empty answer's line sits under those rows and says what it searched (*Nothing in Code matches asset*), and *Search everything* is a row the arrows reach. The footer names what `↵` does in the mode the box is in, and draws no band when a mode has its own legend. A kind's tag is `kindTag`'s, as the asset tree's is: *Tk*, *Pr*, *Mo*. Every row keeps one column model, so a *Go to* row's words start where a result's title does. A mode has one name and one place: `>` is *Go to* on its chip, its heading and the help, and a prefix's chip stands in front of the box where a section's does, so it pushes the box's words and never the list; the bar under the box holds only what narrows the search. A filter's words read in sentence case, and the colour legend's words go through `stateWord`.

### Forms

One grid, `.tf-formgrid`, in a dialog or on a page. A short control has its label beside it, `--t-sm` and `--ink-2`, on the one label column (`--tf-label`, 132 px; a dialog of long names widens the token). A wide field (a textarea, a list, a radio group, or anything marked `.tf-fwide`) takes the whole row, its label above it and 4 px away. A sentence, a tick and a banner take the whole row too. This trades height for room: a dialog with a wide field is taller.

A list of options (`.tf-picklist`) rests unfilled, and fills on hover, on focus and under the arrow keys. Nothing looks chosen until it is: the picked one shows a tick and an `--ink` ring. This is the one way a pick is drawn, on a picked ticket (`.tf-chosen`, `TfTick`), a radio card and a radio row, where the radio turns into the tick, and a route's *Lands on*. There is no fill and no blue, because blue means focus only. A lone recent ticket therefore takes one more click, and `Enter` still takes the first.

A time field is short wherever it stands, 112 px at the start of its column. New view is a builder with its own layout, accepted (it keeps the pill rule). A needed field is marked `.tf-req` wherever the primary waits for it.

### Layers

**A menu and the Connections drawer draw in one host at the shell's root, never inside a page.** The host (`.tf-floats`, `floats.ts`) sits inside `.tf`, so the tokens and delegated events hold, and it makes no stacking context, so a page's `overflow`, sticky band or `z-index` can't reach what is in it. A dialog fills the window from where it was opened and hides with its page, and Docs' tree is that page's own rail sliding out. The order is a drawer at 21, a dialog at 30, a menu at 35 (so a menu a dialog opened is over it), a toast at 40. A combo box's list is part of its field and stays beside it. Hiding a page doesn't reach the host, so what floats closes on a move to another address: a menu, and the drawer.

A menu is `TfMenu`, the one `role="menu"`, and the gate refuses another. It is placed from its opener: under it, or over it when it fits only there or there is more room, with the opener's edge as its own, inside the window, and scrolling when it is taller than the room. It follows its opener through a scroll and a resize. It is on the layer stack as a dialog is, so the page's keys wait. `↑`, `↓`, `Home` and `End` move between the items that can be pressed. `Esc`, `Tab` and a pick close it and focus its opener, so a dialog a pick opens returns there. A click anywhere else closes it and leaves focus where the click put it. A move to another address closes it, so it never stands over a kept page. A row's actions hold on while the menu one of them opened is open.

An editor drawn in the page (a line comment, a row turned into a field, the write queue's *Edit…*) is `{@attach inlineEditor(close)}` on its root (`inline-editor.ts`). That owns focus in, a `panel` layer so `Esc` reaches it, and focus back, also to an opener a redraw replaced (`back`). Inside a dialog its layer is a `dialog`, because the ladder reads dialogs before panels and one press must close the editor, not what holds it. It also owns ⌘Enter (`send`, else its form or primary), and `escCloses` makes one `Esc` close it from inside a field, for a caller whose `close` keeps the words (`drafts.svelte.ts`). A diff's comment box with nothing typed closes on one `Esc`, because stepping out of an empty field first only asks for a second press. With words in it `Esc` blurs and then closes as the ladder has it, and the words stay for that line until *Cancel* throws them away. A file's pipeline bar puts its steps by on *Close* or `Esc`, and *Pipeline* brings them back; *Reset* is what drops them, and the bar stays open for new ones.

A toast's time stops while the pointer or focus is on it, and it keeps at least 2 s once they leave, so an *Undo* being reached for stays. Pressing a toast's button gives focus back to what held it, so a dialog under it keeps `Tab`.

### Keys

One table, so a key learned on one page holds on the next (amended 2026-11-06). It replaces "`Enter` on a radio sends in Apply and Save only" and *Pipeline*'s `Esc` that drops an unsaved chain.

- **`Esc` never loses typed words.** A filter with text clears and keeps focus, and an empty box falls through to whatever `Esc` would do without it (`filterKeys`, `filter-keys.ts`; the gate refuses a box that narrows or finds without it). A find bar closes and returns focus. An editor or a dialog with words closes and keeps the draft (`drafts.svelte.ts`, for the session, keyed so words never land on another page or section), and *Cancel* is the explicit discard.
- **⌘Enter sends in every composer** (`inlineEditor`, or `sendKey` and `isSendKey` in `inline-editor.ts`).
- **`Enter` on a radio sends** unless the dialog's primary is danger or *Merge*.

A shared layout rule names what it lays out. The gate refuses `*` as the subject of a rule that sets a width, a height, `overflow` or `position`: the page's `100vw` once reached the drawer through one. `.tf-keptpage` holds that width now.

### Job scope

One control, `TfScope`, as wide as its words on every page: *This job* with its count as a badge, *Mine* where the page has one, then *All*. The badge is a dash until the read answers, so the control keeps its width. Its default is *All* on every page, and a ticket card's row turns *This job* on, on each list the section holds, Builds' rail and runs alike. It does not repeat the key: the top bar names the job (*Start PAY-231*), and a page a row narrowed to another ticket says so in its bar. The button's title names the ticket. The Inbox draws no control when no job is chosen, because a lone *All* chooses nothing. Amended 2026-11-05: Builds' rail used to default to *This job* (#850); one default means one answer to "why is this page showing fewer".

### Gap

Neighbouring buttons are `var(--control-gap)` apart, in headers, rows, dialog footers, strips, panel toolbars (the filter bar and its pill toggles), the large decision buttons and Settings alike.

### Header

The title block on the left and `.tf-acts` on the right, top-aligned, while both fit. What fits decides this, not the window's width. The block is the title and its subline together and asks for at least 360 px, so a long subline wraps inside it and stays with the title it describes. When the actions don't fit beside that they wrap as one group under the block: title, meta, actions. A group never wraps a button at a time, and a subline can't run under the actions on one row. Amended 2026-11-06, reversing Round 10's subline under the actions. A page's tab strip holds one height across its tabs, so a tab that doesn't offer an action keeps that action's room unseen and out of the Tab order, as Compare does for *New pull request* and the branch picker. The actions come in this order:

1. the primary;
2. the other actions;
3. "Open in <source>", always a ghost button;
4. *Sync now* / *Refresh*, always last.

A section heading that carries an action puts it on the right of the heading line at the small size. A dialog that shows a thing and decides nothing (the file viewer, `TfFileViewer`, #2415) is a viewer: its action is on its title line at the default size, in `TfDialog`'s `actions`, and it has no footer. The action, a 1 px rule and the Close are one cluster (`.tf-dctl`), each control the default height, centred on the title block; *Download* carries its tray glyph at the Close's stroke. The ticket page has no header: its key, status and actions are on its card ([below](#the-ticket-pages-card)), and its title heads the reading column.

### The ticket page's card

The card (`TfJobCard.svelte`, `job.css`) is the same markup in both moods (spec #2396).

- **Two lists of rows.** Under the facts list the card holds two bordered lists in one shape (`.tf-tktable`). The first (`.tf-tksecs`) is the sections: a row per section, each a link holding the name, the count in tabular figures, at most one state with its dot (full ink for err and warn) and a chevron. A row counts what its landing shows. Code lands on two tabs, so its count is repositories and pull requests, a branch or a commit counting only where nothing else is reached, and where it holds both the row's words divide it (*2 repos, 1 open PR*). The second (`.tf-tkacts`) is the card's two actions, *Open in <source>* and *Link something to it*: the same row with no count and no state. A row is a link or a button drawn as a row, not a `.tf-btn`, so it takes no size. The Tickets preview's *Around this ticket* is the same list, with the same words and chevron, and each of its rows opens what the card's row opens.
- **The copy button** beside the key is `.tf-iconbtn.tf-sm`.
- **The foot** is one `.tf-btn.tf-primary.tf-lg`: the card's ink on the yellow, the app's `--fill` on a sheet, as the Tickets preview's same button is, so it stays below the yellow in dark. The decision while reading, the timer once chosen. No line under it. It stays pinned on a window too short for the card, and while rows pass under it a hairline on its top edge ends them.
- **The width handle** is a 6 px `separator` on the card's right edge (`.tf-tkhandle`), focusable, that shows as the edge darkening under the pointer. Dragging sets the card between 240 and 720 px, ←/→ step 16 px, and a double click or Enter fits the card so nothing on it wraps. The width is browser state (`knobas.ticket.side`, default 320 px). Below 900 px the handle is gone.
- **The fold.** The reading column's Attachments panel is a native `<details class="tf-tkfold">`, closed by default. Its summary is the heading, a count badge and a chevron (`.tf-tkmark`) that points right while closed and turns down when open, in 120 ms. An attachment row is one button drawn as the row, not a `.tf-btn`, that opens the attachment preview; the row has no *Open* or *Download* (#2415).

### Dialog footer

Right-aligned, `var(--control-gap)` apart, at the default size. A ghost *Cancel* comes first, then the primary. An extra action (an edit dialog's *Delete*, `.tf-first`) goes on the far left, two of them side by side, and what the primary waits for (`why`) stands just left of *Cancel*. Every dialog has a primary, except a viewer ([Header](#header)), which has no footer.

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
| Panel toolbar's search field | `.tf-fbar .tf-fsearch input` |
| Segmented control | `.tf-fbar .tf-seg button` |
| Pill toggles | `.tf-fbar .tf-fchips button` |
| Row with small actions | `.tf-ext-row .tf-trail .tf-btn.tf-sm` |
| Icon-only button | `.tf-ext-row .tf-trail .tf-iconbtn` |
| A chip's own buttons | `.tf-chip .tf-chipbtn` |
| Dialog footer with an extra action | `.tf-dialog footer .tf-btn.tf-first` |
| A viewer's title-line action | `.tf-dialog .tf-dhead .tf-acts .tf-btn` |
| Dialog footer's ghost *Cancel* | `.tf-dialog footer .tf-btn.tf-ghost` |
| What a disabled primary waits for | `.tf-dialog footer .tf-dwhy` |
| Danger, starting | `.tf-btn.tf-danger:not(.tf-primary)` |
| Danger, confirming | `.tf-btn.tf-primary.tf-danger` |
| Large decision with a `<kbd>` hint | `.tf-btn.tf-lg kbd` |
| A form's short field | `.tf-formgrid > input` |
| A form's wide field | `.tf-formgrid > textarea` |
| A panel's heading, above its box | `.tf-panel > header` |
| A code value | `.tf-key` |
| A state's chip | `.tf-chip.tf-err` |
| A row's fading group | `.tf-rowend .tf-rowacts` |
| A row's value, which hides only where the group covers it | `.tf-rowval`, `.tf-covered` |
| A menu's opener (`TfMenu`) | `.tf-r button[aria-haspopup="menu"]` |

A strip's or a row's trailing group of actions is `.tf-trail`: pushed to the end, centred on the strip, `var(--control-gap)` apart.

### The style gate

`app/src/lib/tf/controls/style-gate.test.ts` reads every stylesheet and component under `app/src/` (a component's `<style>` blocks count) and fails when:

- **(a)** a rule outside `chrome.css` sets `height`, `min-height`, `padding`, `font-size` (or the `font` shorthand) or `border-radius` on a control, or a disabled control's `opacity` or `cursor` (the disabled look is one rule): a selector whose subject names `.tf-btn`, `.tf-sm`, `.tf-lg`, `.tf-iconbtn`, `.tf-chipbtn` or `.tf-refresh`, or is a `button` inside `.tf-seg` or `.tf-fchips`;
- **(b)** the unported button classes `btn`, `tb-btn` or `x` appear in a stylesheet or in a component's `class`;
- **(c)** a row of buttons has a `gap` other than `var(--control-gap)`: an actions container (`.tf-acts`, any local `…acts` row with or without the hyphen, the bare `.acts`, `.tf-trail`, a dialog's `footer`; a `…facts` name is not one), or a row of controls under another name (the filter bar `.tf-fbar`, the pill toggles `.tf-fchips`, the large decision `.tf-decide`, any `…-btns` row, a dialog's title-line cluster `.tf-dctl`);
- **(f)** a field in a row of buttons (`.tf-fsearch`, or a `select` or `input` inside a row of buttons or a `.tf-fsel`) sets a literal `height` or `min-height`, or vertical padding: it takes `height: var(--control-h…)`;
- **(g)** an actions container outside a page header (`.tf-fhead`, `.tf-stg-h`) is pushed to the end and laid out by hand (`margin-left: auto` with `display: flex`): a strip's or a row's trailing group is `.tf-trail`;
- **(h)** a `font-size` or a `font` shorthand's size is not a `--t-*` token, apart from the reading face and code (a rule that sets `--serif` or `--mono`), the Docs title and the PDF's text layer;
- **(i)** a pill toggle is drawn outside `.tf-fchips`: a class with a `border-radius` of 9 px or more that also answers `aria-pressed`;
- **(j)** a grid's label column is a literal `132px`: it is `var(--tf-label)`;
- **(k)** `*` is the subject of a rule that sets a `width`, `height`, `overflow` or `position`: a shared layout rule names what it lays out ([Layers](#layers));
- **(l)** `role="menu"` appears outside `TfMenu.svelte`: a menu is `TfMenu` ([Layers](#layers));
- **(m)** a component draws the missing line by hand inside a `TfDialog` (it is `why`), outside one in any markup but `<p class="tf-dwhy" role="status">`, or says *(needed)* (a needed field is a `.tf-req` *required*);
- **(n)** a `TfDialog`'s footer has a primary disabled for more than a send in flight (`busy`, `sending`, `writing`, `saving` and the like), and the dialog passes no `why`;
- **(d)** an allowlist entry has no reason;
- **(e)** an allowlist entry matches nothing.

The gate still reads an allowlist at `app/src/lib/tf/controls/style-gate-exceptions/`, one `<ticket>.txt` per ticket, which rules (d) and (e) check. It held the violations present when the gate landed; the #2238 cleanup tickets deleted every line, and the directory with them. New work doesn't bring it back: it follows this section.

### Adding a control

A control this section doesn't name is added by amending this section, the `#/controls` specimen and the style gate in the same PR. It is never a rule local to one view.

## Layout

The design width is 1440 × 900. Every view works from 1100 px. There are two breakpoints, 1240 px (preview panes close and side rails narrow) and 900 px (columns stack). A page whose own width decides something, such as a table shedding columns or a header shortening a label, uses a container query instead. Two rules read the window at a step of their own, and say so where they are written: the status bar's age at 1150 px and Monitors' columns at 1040 px. The prototype's other breakpoints at 1000 and 1250 px belong to the retired global map and don't carry over.

On a 240 px rail (901 to 1240 px) Builds' pipeline name has the row, because it is what the rail is scanned by. The time goes short (*11:45*, *Thu 13:05*, *3 Nov*, the whole of it in its title) and shares the ticks' line. A failed tick stands 2 px taller than the others, so it differs by more than hue, which on a selected row's fill is nearly all it has.

A row on Time's Day holds its actions in three slots, so the eye runs down a column: what the row is for, the other way to do it, the row's own upkeep (*Edit…*, *Dismiss*). The group goes whole: the list's own width first moves the state chip under the title, then the slots under the title, and never wraps a button at a time. *Other ticket…* stands beside a button that names a ticket. *Dismiss* has no reverse command, so it holds its write for the toast's life (`heldWrite`), as *Delete block* does; what gives a block to a ticket takes `undoable`. The strip's dashed block says *Assign…* and a press on it opens that dialog, and the now line keeps a block's label to its left, so it cuts no word. A cell on Week is a way into its day: it tints under the pointer, and the day it opens has `Esc` and *Back to* to the week. A row that names none has nothing for it to be other than, so its one action is *Assign…*, the dialog's own word. Amended 2026-11-06.

A source that can't sync says so with the same `.tf-stale` band and fix on each page drawn from it (a pipeline, a run, Monitors), and a page's header then says nothing of when it synced. A credential with none stored reads *Enter token…* there too.

Standup's protocol stays editable once published, on the date's one page. One press says where it stands, in one place: *Publish to Confluence*, *Queued*, *Sent*, and *Publish again* once the note has moved on from what the queue last carried to the page, which the read works out, so leaving the tab loses nothing. A line under the heading says the same in words, and its three usual sentences share one box, so the first keystroke after a publish moves nothing under the caret. *Publish again* sends the whole note against the version knobas last wrote, never the mirror's, so it can't delete an edit made in Confluence: a page the mirror shows as edited since says so and offers *Open the published page* instead, and one the mirror hasn't seen yet comes back refused, which the line draws.

Below 900 px a stacked side rail folds behind one bar (`TfRailBar`) that names what is open, and a link picked in it folds it again, so the page's title stays near the top: Tickets' views, Builds' pipelines, Settings' sections and Docs' tree. An open fold is a panel on the layer stack, so `Esc` folds it before it leaves the page and focus returns to the bar (F-619). A diff's file list does the same by its own width, below 860 px, at any window size, since Compare has less room than a pull request in one window. Docs' tree opens as a drawer from that same bar, because the page does not reflow under it, and as a drawer it closes on `Esc` first too. A row keeps its state's words stacked: a dot alone says nothing. A ticket's card, stacked, is its key, status and title, then *Details and connections* folded, then its foot, in the card's flow.

No page's scroller reserves room for its bar. Content is as wide as the scroller's box with a bar or without one, and the bar takes the last 8 px of the page's right padding, so a bar appearing moves nothing. For `.tf-main` the page is `100vw` wide. The inner scroller of a viewport-high page (Tickets' board, Builds' right side, Docs' page, a ticket's reading column, Settings' section column) is `TfScroller`: a host that is a size container, the scroller, and one body at `100cqw`, with a clip around the body so that what passes under the bar is no overflow to scroll to. A band ends at the bar while there is something to scroll, and 14 px, not 22, sits between content and bar. A rail's own list is not a page's scroller and may keep its room. Amended 2026-11-05, after `overflow-y: scroll` and `scrollbar-gutter: stable` each cost something else. A selected tab differs by its ink and its rule, not its weight, and a filter bar's count holds its width. A table whose rows change under the reader has fixed columns.

Settings is viewport-high: its crumb and its rail stay in view, and only the section's column scrolls. What fits in that column decides, not the window: below 720 px a row's label goes above its control and State colours' six columns become three, so a word is not split. A control's text sits on one line with its label's first, apart from a switch and a textarea, which have none and start at the top. An address that names no section or no source keeps the rail, lights nothing, and says so under *Settings* in the one nothing-here state. **A source's page holds what is typed** (Björn's plan, F-617): hash history can't be held for a *Discard?* prompt, so Connection and the adapter's config keep their edits per source for the session, across a leave and a return, with *Not saved* and *Reset* beside that section's own *Save connection* or *Save sync settings*, and *Changes not saved* on its row in Sources. *Reset* has *Undo*. A draft can sit unsaved until restart. *Every* and *Syncing* save as they change and are never part of one. A number field that takes a different value from the one typed says so under it, naming both (*Takes 1 to 365, so 0 became 1*). A renderer, a command step and a named pipeline are added and edited in a dialog, as a source is. A pipeline's *Add step* offers *Add a command step…*: in that dialog it opens the step's over it and the new step joins the pipeline, and on a file it goes to Settings › Renderers, where they are made.

The top bar's nav keeps its width above 1240 px while the search box gives up its own, so no entry slides out of view. From 1240 px down the search is its icon and the nav tightens; Inbox and Monitors keep their counts because a count is a state to act on. The bar always names the job, because the job scope relies on it: below 900 px *Start* keeps the job's key and the search gives up `⌘/` first.

The status bar names at most two healthy sources; past that they are one line that says how many synced and how many need a look, then the oldest age, and its title and Settings › Sources keep every reading. The line only takes the width the rest leave: its age drops out of sight before anything is cut, at 1150 px and below it goes when a failure is showing, and a failure gives up words only when there is nothing else to give. A failure's fix and the write queue never shrink.

From 1240 px down the Tickets preview is closed, so one click on a card or row opens the ticket's page. Above that, one click fills the preview and a double click opens the page.

A section's own page has no breadcrumb bar, unless a ticket card's row opened it narrowed to that ticket, or a ticket's dialog opened it to come back from (*Manage link sets*, which opens the Connections drawer again on the way back, where the press was made), when the bar says *Back to PAY-231* and `Esc` goes there: the nav lights the section and the page's title names it, so a bar reading that name again adds nothing. A deeper page draws its path as before. All six rows of a ticket's card land this way, Docs' home and Sub-tasks included. Sub-tasks names the narrowing in its title (*Sub-tasks of PAY-231*), with nothing typed into the search.

*Back to <name>* `Esc` is a link that does what the key does, drawn as the crumbs are. With no opener, `Esc` goes to the crumb above the page, as the page says it (a pull request's repository, a run's pipeline by name, an asset's holder). A section's own tabs and a pipeline picked on Builds have no parent, because their parent would be a sibling: `#/builds` reopens the pipeline you were on, and a pipeline's crumb is one crumb at both of its addresses, with nothing above it linked. Settings and the asset tree are the top pages `Esc` leaves, because neither has a nav entry: you visit Settings by its cog and the tree by a row or a search, and each says where it came from. The tree's `←` walks up its columns, and its `Esc` follows the bar like any page's. A walk through the columns keeps how the tree was opened. A jump to another asset (a link in the pane such as *inherited from*, an alert's row, the tree's search) is an opening: the bar says *Back to knobas-gitea*, and `Esc` returns to the asset being read.

**The return contract.** The bar and the key agree whatever has focus. The whole of *Back to* is underlined, a key or a repository's name included, since it is one link to the thing it names (F-053). *Enter token…* and *Re-enter password…* open their prompt over the page you are on and change no address, so *Cancel* is back where you were; Settings' own *Enter…* opens it on the source's page, and the `…/reenter` address still does if opened raw. A hint names the real target, by the name that page gives itself (*Back to Payout_Lint*, *Back to Engineering*, *Back to Payout stack*). Back restores the list's scroll and focuses the row you opened (`return-place.ts`), and a step through history does the same. The way back is a chain: a tab switch keeps the page's opener, and returning to the opener brings that page's own way back with it, so one trip ends with one *Back to PAY-231* whichever tabs it went through. A row's narrowing belongs to the trip, not the section: a detail in another section keeps it, the list comes back narrowed, and `⌘[` and `⌘]` narrow an entry as it was. Back on the ticket, or by a nav entry or the launcher, the narrowing ends and each filter bar is back on the scope it had. The window's title is the page's own name as the page reads it (*knobas-gitea · knobas*). A row or link that opens a place for one thing marks that thing and draws the bar: Monitors at one monitor is `#/monitors/at/<id>`, its row marked and brought into view whatever the filter says, and a card's *Assets* row marks the ticket's assets in the tree's columns, a dot on what holds them, with a line under the bar naming them all. An open panel or inline editor closes first on `Esc`.

**Section memory.** A nav entry returns to the section as you left it, tab, saved view and filter, as Builds does with its pipeline, minus any card row's narrowing. It opens nothing, so `Esc` there goes nowhere. A place deeper than a tab (a link set, a Docs space, a pull request) is not what an entry comes back to: the entry leads to the section's own page.

**One "nothing here" state** (`TfNothingHere`) for an address that names nothing, of any kind: centred, one sentence with no id and no word of the app's insides (*knobas does not hold this pull request. It may not have synced yet.*), one action. A bar above it keeps the path to the kind (*Code › Pull request*), never an empty crumb. A section's address with a tab it does not have says so under the section's title, and a pull request's or a repository's under its own header and tabs, with none lit. Neither is drawn as its first tab. Its words say what is missing rather than speak of an address, since the window shows none. A run's address that names only a pipeline is that pipeline's page. A read that failed for another reason is the same block with the reason and *Try again*.

The asset tree (`#/assets`, `#/asset/<id>`) is viewport-high: a crumb bar drawn at both addresses, the canonical header with one line at both (the title names the asset), a filter band with its open alerts' fold at the band's end (folded to start with, then the reader's fold, which a selection does not change; its list opens under the band, each row's path in its title), then the Miller columns scrolling sideways beside a fixed pane. So the columns start at one height whether anything is wrong or not. An empty estate has no filter. So the first click moves no row: a column is 220 px at both addresses, and what the strip leaves stays blank, because a last column that took the room shrank under the pointer. An estate with nothing in it and a failed read have no pane. A wire runs behind a row's words and tag, which stand on the row's own fill, so it never reads as a strikethrough. A column is one `Tab` stop, the row on the path or else its first, and the arrows walk the column the press was made in; `Enter` on a row `Tab` reached is what a click on it does. A picker's level in *Move…* and *Add a route…* is one stop too, and *Move…*, which has no field, arrives on its list. A row's name stays whole: what a ticket reaches and what holds it are a small dot and ring beside the name, because the line under the bar names the ticket. The pane doesn't repeat the path, which the columns draw; it is the title of the pane's heading. A long value takes the pane's width under its label and breaks at spaces, backticks in it draw as code, and it edits in a box that shows it all with the caret at the end. An inline editor in the pane (*Rename*, a property, *Add a property*) takes focus with its text selected, has a *Cancel*, closes first on `Esc` wherever focus is, and gives focus back to what opened it; *Rename* waits for a name, with the line. A rule waiting for its value in a set's *Applies to* does the same: it takes focus on the value, and one `Esc` puts it away, back on *Add a rule*. *New link set* saves nothing until the first change, which makes the set and moves the address in place, so leaving at once leaves no row behind. Its name and description are edited in place, and pointing at them draws a field's outline, not a link's underline. A route's *Lands on* opens on what the asset holds, or on its neighbours when it holds nothing, and picking a row that holds nothing keeps the list. A dialog's filter and the list under it read as one control: `↓` goes into the list, `↑` from the top comes back, and `Enter` on a picked row of *Apply a link set…* or *Save as a link set…* sends it, as focus can only get there by a press. A *Depends on this* row draws only the part of its path it does not share with the selected asset, the whole path in its title. It has no nav entry and nothing lights for it, because `docs/decisions/2026-11-05-cut-dependencies-and-proposed-links.md` keeps it from moving under Monitors. Its crumb starts at *Asset tree*. In a column the selected asset is the ink pill and the assets above it on the path a paler one, so the path reads and one row says where you are. An asset's kind is a two-letter tag written like an element's symbol, in sentence case and in Archivo (*Si*, *Hv*, *Vm*, *Ce*, *Ct*), with the word itself in the tag's title and heading the pane.

A filter bar is a full-width band under the page's tabs, on every page. Its search takes what the row leaves, from 140 to 360 px, so a placeholder is short enough to fit at 140. A group of pills stays whole: the row wraps between groups. The bar's last things are one group (`.tf-ftail`): its selects, or its last pills where it has none, then the count at the row's end, so the count never wraps alone. The count holds the room its longest text needs and no more. *Save as a view…* is in the Tickets header's actions, and only while a filter is on, because with none there is nothing to save.

A key in mono carries no underline until pointed at. A link made of words has an underline, in a sentence, on its own or in a banner, with one exception: the name of a row that opens as a whole (a table's `.tf-row`, a list row that is one link) has none until pointed at, because the row answers the pointer. A link in a row that does not open, or a second link in one that does, keeps it.

A diff shows split only while its own column has 900 px, and says so where it shows unified instead, because the choice is one for the whole app. Half a row has no room to scroll in, so a split line wraps. A unified line scrolls sideways inside its file.

The top bar and the filter bar stay put, and the content under them scrolls. A board scrolls sideways inside itself, its bar pinned at the pane's foot with lanes or without, and every column is 232 px wide on every board, so a card keeps its shape when a filter drops a column or lanes go on. The page never scrolls sideways. Content aligns left. The only centred blocks are empty states, the Docs home and the ticket page's reading column.

Monitors' columns stay fixed, whatever a filter leaves: the action column is 72 px whether or not a row holds *Ack*. They run Monitor, State, Watches, Last 24 h, Response, 30 days, because a row's actions stand in place of its last values on hover and Watches holds links, which have to stay in reach. The state chip holds the word (*Down*), and how long is plain `--t-xs` text: under the chip at every width, so a down row is two lines. A pause's or resume's *Queued* and *Sent* stand beside the state chip, because they say the state is changing, and the row keeps its height as they come and go; *Pause* stays where it was, turned off, so focus and the pointer keep their place. State takes its room from Monitor. A slow monitor's time is its Response. Below 1041 px Response and 30 days leave their columns and stay in the row, under the bars, and on hover the actions take that line only, so the bar under the pointer keeps its title. Column heads stay under the filter bar above 900 px, and scroll with the rows at 900 px and below.

A run's steps bar draws each step as wide as its time and, where the bar has room for every name, at least as wide as its name; with no room the short names stay whole and the longest give up their ends. A step's name that is a command line is code, there and in the Log tab's step heads.

The Docs home is `#/docs` (Björn, 2026-11-05): the tree on the left and, beside it, a centred column at most 980 px wide with the canonical header (*Docs*, how many spaces and pages, *Sync now*). It lists what the tree read holds, on panels in this order: *Continue reading* (the page read last) beside *For you*, the pages the chosen job reaches, then a tile per space with its count and first four pages. The tiles head themselves: no *Spaces* heading stands over them, because at its size it read as one more tile head and the header's sub line already counts the spaces. A tile takes its pages by level, the top of the space first, then the level below, so a space whose top holds one index page fills its tile. With *This job* on, the job's pages come first. A page is listed once, in the first panel that holds it, and its line says what else it is: on the job's panel *read last* and *mentions you*, on the others *on this job* and *mentions you*. A tile skips what the panels hold, so its four may not be its literal first four, and its count stays whole, as does the job's, which says how many are listed above. Panels in a row end on one line. A tile has one link, its whole head row with the chevron a ticket card's rows have, which opens the space's own page, `#/docs/space/<key>`: every page it holds, indented, a page that holds others heavier with more room above it. The Space picker goes there too. It opens no page, so it records no visit, and the rail's own *For you* steps aside while it is showing. Page titles on it keep the reading face. A page's place reads as the crumb has it through one function (`pagePlace`), the space by name, and a page at the top of a space says the space alone: *Engineering › Payments*, *Payments team*. Within one space's search the space is left out. Docs' crumb and *Back to Docs* lead here from any page. Empty, it says whether a source is missing or has not synced, and the one *Sync now* sits under that sentence, not in the header, at a page body's small size, as *Open Settings › Sources* is. With nothing held the rail offers no *This job*, and while the first read is out neither the scope nor the Space picker is drawn, so neither can contradict what comes. A search's address (`#/docs/search/<words>`) opens no page: this column stays until a hit is opened, under a *Docs › Search* crumb, and says there how many pages match (*12 pages match payout, listed on the left*) or that none does. On arriving at the address the first hit takes focus, unless the reader is in a field or a dialog, and the arrow keys walk the hits. Opening a hit keeps the search in the rail, and *Back to Search* returns to it, focus on the hit that was opened. Leaving by anything else ends it.

A page's foot lists up to six *Linked from* pages outright, each saying its space where they come from more than one, because a closed fold around one link only added a click. Past six they go by space, each a closed fold. *On this page* marks a section once its heading is in the top third of the column, with its body in view under it, and an entry pressed scrolls there, pressed twice too. The current match in the find bar is ringed in `--ink`. A comment on its way counts in *Comments*.

An Inbox row leads with its subject (*knobas-jira is down*, *Payout CSV export: jonas asked for your review*). What would bury it, an asset's path or a build's status, goes on the row's second line. The sentence is the row's link, with no underline at rest, and a click on the row that made no selection follows it, so the row has no *Open…* button. A button is only for what a link can't do: *Ack*, *Re-run* and a credential's fix (F-132). **A row that answers a press stays selectable** (F-142), so nothing stretches over its words, and a drag that starts on the sentence selects as one anywhere else does. `j` and `k` walk the rows, as on Tickets, and Tab goes through a row's words before its actions (F-143). Below 900 px all of a row's actions stand on one line at its foot, showing (F-146).

**A row leaving under a pointer holds its place** (Björn's plan, round 12, F-138). After a mouse press removes a row, it keeps its height with its contents gone until the pointer leaves it, and then the list closes up, so a second press where the pointer rests lands on nothing. A key press closes at once and moves focus to the same action on the next row. This gives up pressing *Done* down a list with a still pointer, which was never safe, and it replaces the idea of a fixed slot for *Done*, which left *Ack* floating away from the edge at rest.

**Start work's *Skip* before the first run only marks the step** (F-137), with *Include* to take the mark back, and nothing is sent or chosen until *Run all steps*. `start_work_skip` settles a step and runs the flow on, so one step can sit out the run: the first, or a move the credential refuses, and then the first's *Skip* is off and says why. `Esc` does what *Cancel* does while nothing has run, when the steps opened on the ticket's own page (F-139); from the Tickets preview it goes back to the list. A press that runs the steps and starts the timer raises one toast, ending *Timer running.* (F-145).

**The stop dialog's left button is *Stop, don't log*** (F-140). It was *Discard*, set apart in danger's colour, though a stop makes a block and the time stays in the day, unlogged, as with *Stop, log later*. Both toasts say how much time stayed and offer *Undo*, which starts the clock again, and *Open the day*.

A row of the Connections drawer draws its state's dot and word, as the ticket card's rows do, and its action stands in that place on hover and focus.

The ticket page is two columns that each scroll on their own (spec #2396): the card at `--tk-side` (320 px by default, 240 to 720 px by its handle) and the reading column, at most 880 px wide and centred in the space beside the card. Below 900 px they stack.

## Motion

Motion answers something the person did, and there's very little of it.

- A running build or timer pulses its dot (1.4 s, opacity to 35 %). That's the only motion nobody started.
- The Connections drawer, under 480 px wide and with a dialog's title line, slides in from the right (R-6.1), the Docs tree's drawer below 900 px from the left (R-10.9, #2084), and a drawer's scrim fades in 0.15 s.
- A toast rises 20 px into place.
- A route's wire on the asset tree fades in over 160 ms when a selection brings it.
- A row's repeated actions appear over the end of the row, and no word is cut. They fade in on hover or when keyboard focus is inside the group (`.tf-rowacts:has(:focus-visible)`), and focus on the row's own link reveals nothing, so `Esc` back to a row shows the row as it was (F-046). At rest they hold no room: `.tf-rowend` is the end of the row, and the group hangs to its left, with no plate or gradient. A value (`.tf-rowval`) hides only if the group covers its words: `row-room.ts` measures on the way into the row and marks it `.tf-covered`. A partly covered value hides whole, and a value with room beside the group stays (F-248, F-342, amended 2026-11-06). Pointing at a covered value still hides it. The column holds no link, so nothing that can be pressed goes out of sight. Where nothing hovers they stand in the row. An action only some rows offer because of their state (*Ack*, *Open PR*) stands in `.tf-rowend`, not in the group, and stays. The Inbox has no value column to replace, so from 901 px its words keep the group's room on their right and a long title wraps a line earlier at rest; stacked, the time's line is a small button's height and holds the group. Nothing moves on hover.

There are no entrance animations on views or sections. With `prefers-reduced-motion` everything above stops.

Focus is always visible: a 2 px outline in `--focus`, offset 2 px. Inside a box that clips (the nav, a tab strip, a segmented control, a rail, a card's lists) it is offset -2 px, or the box would cut its sides. `--card-ink` replaces it on card surfaces and `--paper` on a fill (see Contrast).

A rail's selected row is an inset ink pill on every page (Tickets, Settings, Builds, Docs), with its tones re-declared for the ink (`--on-ink-*`). A list row that says why it asks for you (Docs' *For you*) does not repeat it. A status tag on the pill drops its tint, which is for the sheet, and keeps its tone.

A diff's rail and its files run in one order, the tree's. The pill follows the scroll. A pick or a key holds it until the reader scrolls, a pick opens a collapsed file, and at the scroll's end the last file is marked, since its head may never reach the top.

## Words

Copy follows prototype-6 word for word, as `requirements.md` says. When a screen needs words the prototype doesn't have, they follow its voice. Sentence case. A state's chip is `TfStateChip`, whose word comes from one function (`state-word.ts`): whatever a source writes, *FAILURE*, *In Progress* or *merged*, draws as *Failed*, *In progress* and *Merged*, and a chip reads what the filters read. A ticket's status goes through the same function wherever it is drawn: a column head, the list, the preview, the card. It changes what is drawn and not what is stored, so a filter still matches the source's word. It takes words of state only and never a name, so a column the reader named in their own view keeps their words. Monitors' pills are the chips' six words: *Down*, *Slow*, *Up*, *Paused*, *Pending*, *Maintenance*, and a pill keeps the rows whose chip reads as it does. A length has one spelling, `time/format.ts`: *38s*, *4m 12s*, *2h 10m*, on a run, a step, a worklog and a timeout alike. Milliseconds keep their *42 ms*, a measure and not a length of work. An age or a cadence is said in a sentence and keeps its words: *synced 4 min ago*, *every 15 min*. A word the app stores is not a word to show: an asset's type reads *Container engine*, not `container_engine`, a monitor's kind *HTTP*, a relation *Depends on*, and a dot says what the same thing's chip says elsewhere. Two state words side by side join with an arrow, *To do → In progress*: in sentence case, *To do to In progress* stumbles. Buttons name what happens ("Log 1h 20m", not "Submit", and "Start timer", not "No timer"), and the toast uses the same verb ("Logged 1h 20m"). Errors say what failed and what to do ("TeamCity sign-in is failing, so it waits there"). An empty view says what would fill it and offers the action. One person reads one way on a page. Where a source keeps a name beside the account, the name is drawn, with the avatar made from it: *Mara Lindqvist (you)* on a ticket's card, *You* where the reader leads a line. Filters, lanes and *mine* stay on the account.

**A row's date climbs one ladder**, through `clockLabel` in `time/format.ts`: the time (*10:05*, or *today 10:05* where it stands alone), *yesterday 10:05*, the weekday for the week past (*Wed 14:32*), then one date form (*4 Nov*, with the year when it isn't this one). A row under a date heading says the time alone (`timeLabel`), and the heading is *Today*, *Yesterday* or *Wed 4 Nov*. Ages in sentences stay ages. So one run can still read *10:05* in a list and *2 h ago* in a pull request's prose.

## Getting it into the app

These are engineering constraints the prototype doesn't have. Each UI ticket that touches them cites this section.

- **Fonts are vendored, not linked.** The app's CSP is `default-src 'self'`, so Google Fonts can't load. The three families come from npm (`@fontsource-variable/archivo`, `@fontsource-variable/source-serif-4`, `@fontsource/jetbrains-mono`) and are bundled, like IBM Plex and Barlow are today. The Archivo import has to include the `wdth` axis; the default file from that package may carry only `wght`. The first slice-1 ticket checks this and pins the import. Without the axis the job key renders at normal width and the screenshot match fails visibly.
- **No inline styles.** `style-src 'self'` drops `style="…"` in a bundle, and `house-rules.test.ts` refuses them. Prototype-6 has 345 inline `style` attributes in its templates. Each becomes a class or a component prop with a class. A ticket that ports a view ports its inline styles too.
- **Two UIs live side by side until slice 5.** The new shell runs beside the rooms UI behind a setting. Their tokens collide by name (`--ok`, `--muted`, `--sans`, `--mono` exist in both with different values). The new tokens are set on the new shell's root element, not on `:root`, and that element sets its own `background: var(--paper)` and fills the window. The rooms UI's `app.css` stays untouched. When slice 5 removes the rooms UI, the tokens move to `:root` and `app.css` goes.
- **The new stylesheet is its own file.** It doesn't append to `app.css`, whose header asks later milestones to append. That rule was for extending the Signal design, and this one replaces it.
- **Theme.** Today's app is dark only. The new shell follows the OS, and the theme toggle and *Settings → Appearance* (R-13a.8) override it through `data-theme` on the shell root, with the same three-way selector the prototype uses (OS dark unless `data-theme="light"`, or forced `data-theme="dark"`).
- **Not a source.** Prototype-6 still contains the retired global map (`renderMap`, `renderGlobalMap` and its `.ma-*`, `.gm-*`, `.mn`, `.minsp` selectors). Dependencies replaced it (D11/D12) and left the app on 2026-11-05. Don't port those selectors.

The conventions the first shell tickets settled (class prefix, fixtures, mounting, addresses) are in [ticket-first-shell-conventions](2026-10-15-ticket-first-shell-conventions.md).
