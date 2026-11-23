# 2026-11-21: A theme colours the whole window

**Asked (Björn, on PR #2479):** "I'd like for you to have another fable agent design a way to theme the whole
Knobas app. Maybe with ways to adapt it to the code-syntax theme and the imports, or just import vscode/etc. alone
just for the knobas theme without touching code, and i dunno, just make it all themable."

The judgment calls went to Fable under Björn's standing instruction. The architecture is ADR-0055.

## What was there

A theme coloured code: seventeen roles and a diff's four colours, two slots, seven built-ins (ADR-0051). The window's
own colours were two fixed sets in `tokens.css`. 81 colours stood outside them as literals, 45 of those a second copy
of the palette for a preview. 15 rules wrote text in a surface colour on a fill, which held only while the fill was
the ink. The page's module set the side after the first paint, so a window kept dark on a light system painted one
light frame.

## The rulings

1. **A theme colours things and nothing else.** No radius, shadow, density or font.
2. **The token names stay.** Every colour token reads a variable for its side with knobas's value behind it. 22 new
   tokens, among them `--on-fill`, `--primary`, `--link` and the yellow card's states. 77 literals became tokens or
   went. 4 stay, on a list with their reasons.
3. **The window's part of a theme is 51 colours a side, any of them set.** What a theme leaves out is derived in
   OKLCH from what it sets. Two colours fill the rest.
4. **One theme, two parts**, window colours and code colours, in one library.
5. **Four slots.** The code slot follows the window's until it is set apart.
6. **Import** reads a VS Code theme's `colors`, a Zed theme's `style`, a Base16 scheme and knobas's file, and then
   asks what to use it for: the app and code, the app only, or code only.
7. **Every text colour is checked on every surface it is drawn on**, at 4.5:1, and a part at 3:1. The fix moves
   lightness alone. A filled pair's fix moves the ground.
8. **States told apart** is a quiet line with a fix that may move a hue.
9. **A theme under 3:1 for the text one needs to find the way back asks first, and is kept only on a press.** Reset
   is a button, a launcher action and `knobas --default-theme` (the design's ⌘⇧⌫ was dropped, below).
10. **The first paint has the theme**, from a script in the page's head, and the bare window has its colour from the
    Rust side.
11. **Settings › Appearance › Theme** is the page. Appearance's *Theme* row is now *Mode*.

## Measured

Derived from knobas's panel and text colour alone, the other colours land a mean OKLab distance of 0.006 from
knobas's own. Of 65 VS Code themes, 50 arrive with a colour under its mark, 150 colours in all, and none fails
after *Fix all*. One is unreadable as published. 62 have two states closer than 0.08 for a colour-blind reader, as
knobas's own two themes do, at 0.018 and 0.034.

A switch is one restyle of the whole page and no layout: 17, 59 and 79 ms at 1x, 4x and 6x on a page of 2,854
elements in Chrome on Björn's PC, the same whether the theme is written as properties, an attribute or a sheet, and
the same for one side or both. 400 more custom properties on `<html>` change no restyle's time. A theme set by the
page's module missed the first paint in 5 of 5 loads at each speed. Set by a script in the head, in none.

## Not in this pass

A chart palette, since there is no chart with series. Fonts, radius or
density in a theme. Changing knobas's own state colours for colour-blind readers.

## Reversing it

With the four slot keys removed the window is knobas's, pixel for pixel but for the three changes of the token
step. The page removed leaves the tokens, which cost nothing at rest. The head's script removed brings back the one
light frame.

## Built (2026-11-22)

Steps 1 to 9 of the design, on `ux-polish`. Where the build differs from the design:

- The editor's list has 22 rows, as the design's own table has them; its text said 21.
- The Zed table was run against Zed's own One and Ayu families at the commit the notices pin. Every colour came from
  the key the table names, none of the five themes is unreadable as published, and none fails after *Fix all*. The
  guards are the design's spike's (`app-import.ts` says so), where its prose said "as above": so a Zed page can be
  lighter than its panel, as Zed draws it.
- The first-paint case is in `testenv/perf/theme-switch.mjs` beside the switch probe, and reads the first frame's
  computed page colour rather than a screenshot of it.
- *Put knobas's themes back* in the launcher is an action that runs where it is; its address is the Theme page.

## Björn's answers (2026-11-22)

Section 11, answered, and built:

1. **Code follows the window**, and can be set apart: as built.
2. **The yellow card is a theme colour.** Any theme may set it. One that sets none but sets its Attention gets a card
   of that hue at knobas's card's lightness and chroma, so the built-ins and imports bring their own; a theme that sets
   no Attention keeps knobas's yellow, and so do knobas's own themes.
3. **Colour-blind mode**, a switch in Settings › Appearance, off at first. On, it names each pair of states that reads
   the same under protanopia, deuteranopia or tritanopia, with the roles ("Attention and Failed read the same under
   deuteranopia"), for both sides' themes, under the Theme page's States, and in a toast when such a theme is put in a
   slot. The simulation is Machado, Oliveira and Fernandes (2009), *A Physiologically-based Model for Simulation of
   Color Vision Deficiency*, IEEE TVCG 15(6), at full severity; a pair reads the same under 0.08 in OKLab. Two
   built-ins, *knobas Light for colour blindness* and *knobas Dark for colour blindness*, carry the set *Tell them
   apart* computed: light Passed `#00604F`, Attention `#905C0C`, Failed `#9B0618`; dark Passed `#52D8BD`, Attention
   `#F2BD5B`.
4. **An import arrives as published, with *Fix all*:** as built.
5. **Themes are in backups.** The store is mirrored into one `knobas.setting` row (`themes.kept`), which the database's
   dump carries; a restore puts it back. A share export carries no setting, so none carries themes.
6. **JetBrains `.icls`** (code colours only): wanted, last in ux29 if time allows. Built last: the scheme's attributes are
   read onto the seventeen roles, the first one present winning and an empty one taking its base's (`ICLS` in
   `theme/themes.ts`). A scheme lists only what differs from Darcula or Default, so a role it leaves out draws in ink.
   Its side is its page colour, else its parent. Tested against Dracula's own JetBrains scheme.
7. **No reset key.** The button, the launcher action and `knobas --default-theme` stay.
8. **The built-ins keep their published code colours.** The nine are not lightened. On their own window a few fall
   under 4.5:1 and are marked; the editor's *Fix all* and each row's fix work on a built-in too, by making a copy with
   the fixed colours where the original drew.
9. **A selected row is a pair of theme colours of its own**, *Selected row* and *Text on a selected row*
   (`--sel-row`, `--on-sel-row`), split from *Pressed* (`--fill`, `--on-fill`, which pressed segments keep). Unset,
   it is the ink plate of today. Its row in the editor has three quick picks: *knobas (ink)*, *The theme's accent*, and
   for an import that had one, *The theme's own* (VS Code's `list.activeSelectionBackground` and `-Foreground`, Zed's
   `element.selected`), which the import offers and does not set. The pair is checked at 4.5:1 and joins the 3:1
   floor; a selected row's states (`--on-ink-*`) are worked out against it.

## The bare window on macOS (2026-11-22, the design's risk 4)

The main window has been built visible since 7c9c6f1a, with the title bar's side and the page colour from
`window-look.json`. On a Mac a restart still showed the wrong colour until the page's first paint, about 300 ms.
The cause was not the window: tao gives the NSWindow the kept colour and the app its side before the window is
ordered front. It was WKWebView, which paints a background of its own over the window until the page has painted.
That background follows the side: white on the light side, a dark grey on the dark, whatever the theme. wry turns it
off (WebKit's private `drawsBackground`) only under Tauri's `macos-private-api` feature. knobas does not use that
feature, because it would also clear the background of the sites' web views (`knobas-site`, the same wry), and a site
that sets no background of its own would show knobas's page through it.

So the main and capture windows are given a `WKWebViewConfiguration` with `drawsBackground` off before their web view
exists (`window_look::without_white`). The window's colour shows until the page paints, and the page then covers it in
the same colour. The sites' web views are unchanged.

Recorded on the Mac, knobas's window only (ScreenCaptureKit filtered to the app, about 120 frames a second, the app
held at launch until the stream ran), a restart on a copy of the scale database, the Mac on its dark side. The
capture shifts every colour a little, the window's and the page's alike, so the frames are compared with each other:

| Theme | Before | After |
| --- | --- | --- |
| Dracula (dark) | 3 frames with the title bar and page WebKit's dark grey, then the page that grey under the theme's title bar until the first paint at 858 ms (about 330 ms) | every frame from the first is the theme's page colour, title bar included |
| GitHub Light (light, on a dark Mac) | 3 frames all white, then a white page under the light title bar until 810 ms (about 320 ms) | every frame from the first is the theme's page colour, light title bar included |

No unit test covers the configuration itself: WebKit's configuration is made on the main thread, which libtest
never runs a test on, and a `harness = false` test binary is outside ADR-0017's layout. The side and colour a
kept look gives are tested (`a_kept_look_gives_the_window_its_side_and_colour`). Why a window built hidden and shown
later never loaded its page was not pursued, because the visible build now has no wrong frame.

## The bare window on Windows and Linux (2026-11-22, the ux29 boot check)

The ux29 Windows boot check found the page area wrong until the page's first paint on a dark system: white on a fresh profile, knobas Light's colour on a second start. The title bar was right. There were two causes. A look that follows the system was built with the light side's colour, and the dark one was set only once the window existed. WebView2 takes its background only when its controller is made, so a colour set later shows only after the page's first paint. A profile with no kept look got no colour at all, so WebView2 drew its white.

**The fix.** On Windows the system's side is read before the window is built, from `AppsUseLightTheme`, the setting WebView2's `prefers-color-scheme` and tao's title bar follow (`window_look::system_dark`). A profile with no kept look is built with knobas's own themes following the system (`WindowLook::knobas_own`, `#E4E8EC` and `#12171C`, checked against `tokens.css`). The same holds on Linux, where the side is still set after the build, as before. macOS is unchanged: a fresh profile keeps the system's window colour there, which was not checked in a Mac window.

**Recorded on Björn's PC.** knobas's window only (PrintWindow on its handle), the system on dark, Björn idle, debug builds of the same head cross-compiled on the Mac, a fresh scratch profile per start:

| Start | Before | After |
| --- | --- | --- |
| Fresh profile | white from the first frame (709 ms) to 1,267 ms, then `#12171c` | `#12171c` from the first frame (748 ms) |
| Second start of that profile (the page's own `window-look.json`, mode system) | `#e4e8ec` from 78 to 312 ms, then `#12171c` | `#12171c` from the first frame (67 ms) |
| A kept dark look (Dracula's `#21222c`) | `#21222c` from the first frame | the same |

The light side was not tested on Windows: that would have meant changing Björn's system theme. Linux under Xvfb (light, since there is no portal there): a fresh profile was GTK's `#f6f5f4` from 1,233 to 1,781 ms before the fix. After it, `#e4e8ec` showed from the first frame. The mode-follows-system test was the same before and after.

## The switch measured (2026-11-23, the ux30 end gate)

`testenv/perf/theme-switch.mjs` had never given a number. It stopped at "the app wrote no paint cache for Dracula"
because it moved between pages by changing only the address after `#`, which is a same-document navigation: no
module ran again, so the slot written to `localStorage` was never read. Its three pages were all the Tickets board, and
the cold loads were handed a cache encoded twice. The app wrote its cache all along. The probe now loads every page
afresh through `about:blank`, waits for the page to go idle, counts layout in both directions and holds it to the
line, and takes `--attach <port>` to drive knobas's own window.

Run to the end, the first paint met its line, and the switch did not:

- **A theme's selected text made every restyle dearer.** The rules `:root[data-sel~=…] ::selection` named every
  element's selection, so while one matched, Chromium resolved a selection style for each element on every restyle. A
  slot change to Dracula, and the light and dark toggle with Dracula on, cost about 1.8 times what they cost without
  it (Tickets at 4x: 40 ms against 23). The rules now name `<html>`'s own `::selection`, which Chromium's highlights
  inherit, and cost nothing measurable. An engine whose highlights do not inherit (WebKit) is found by one style read
  the first time a theme's selection is drawn (`theme/selection.ts`) and gets `data-sel-each`, which keeps the rules
  for every element there.
- **Tables were laid out again.** Chromium lays a `border-collapse: collapse` table out whenever a border's colour
  changes. The Markdown and Docs tables and the view editor's list now draw the same lines with separate borders
  (Docs, the list) or each cell's outline (Markdown, whose table is a block that scrolls), pixel for pixel at 1x and
  1.25x in both sides, merged cells included.
- **A fitted diagram was built again.** A diagram fitted into a kept room was given a new holder, frame and *Show at
  full size* when its drawing came back in the new colours; a drawing of the same size now swaps its picture.

**What the line counts (Fable's ruling, Björn away, 2026-11-23).** A slot change, like the toggle, also draws each
diagram again: its colours are baked into the drawing, so the sandboxed frame it stands in (the ruling of 2026-10-30 on
#2255) loads a new document, and that lays the frame's place in the page out (about 20 objects). The line is held in
two parts. The colours, the properties and `data-sel`, lay nothing out to the end of their frame, counted by CDP's
`LayoutCount`. The redraw that `data-paint` sets off, followed to its frames' loads, lays nothing out on a page with no
diagram, and on one with diagrams moves no frame or fence and shifts nothing. Its cost is reported beside the toggle's.
The time line stays the whole slot change against the toggle plus 10 %. A literal zero with diagrams would need them
drawn as an `<img>` in the page rather than in the frame, and even then a palette not drawn before lays Mermaid's
measuring box out in the window's document. That is open for Björn: it reverses the 2026-10-30 ruling for diagrams.

**Measured on the Mac** (headless Chrome 156, load 3 to 4, the parent `6a31a1af` build and the fixed one alternated,
seven runs a rate, medians; slot change against toggle in ms, then the colours' layouts in 14 switches):

| Page | Size | 1x before | 1x after | 4x before | 4x after | 6x before | 6x after |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Tickets | 1440 x 868 | 14 / 12, 0 | 10 / 10, 0 | 43 / 25, 0 | 24 / 24, 0 | 67 / 39, 0 | 38 / 41, 0 |
| Repository | 1440 x 868 | 6 / 5, 14 | 3 / 4, 0 | 18 / 14, 14 | 14 / 13, 0 | 26 / 20, 14 | 19 / 20, 0 |
| PR files | 1440 x 868 | 17 / 13, 0 | 13 / 16, 0 | 38 / 26, 0 | 27 / 28, 0 | 58 / 39, 0 | 41 / 42, 0 |
| Tickets | 2560 x 1440 @ 1.25 | 15 / 13, 0 | 6 / 6, 0 | 52 / 23, 0 | 24 / 24, 0 | 77 / 34, 0 | 36 / 36, 0 |
| Repository | 2560 x 1440 @ 1.25 | 13 / 8, 14 | 5 / 6, 0 | 28 / 22, 14 | 14 / 13, 0 | 42 / 32, 14 | 18 / 19, 0 |
| PR files | 2560 x 1440 @ 1.25 | 19 / 18, 0 | 18 / 19, 0 | 48 / 30, 0 | 27 / 28, 0 | 71 / 45, 0 | 41 / 42, 0 |

On the repository page, a redraw after the fix loads its two diagram frames, lays out about once (0.2 to 0.5 ms), moves
no box and shifts nothing; the toggle's costs the same. The first draw in Dracula is 31 to 32 layouts (about 1.1 ms at
1x), mostly Mermaid measuring its text. On the other two pages a redraw lays nothing out. The first frame of a cold
load had Dracula's page colour in 15 of 15 loads at each size, before and after. In WebKit (a windowless `WKWebView` on
this Mac) the check answers that highlights do not inherit, and with `data-sel-each` a deep element's selection is
Dracula's; with `<html>`'s rule alone it would be the system's.

## In knobas's own window on Windows (2026-11-23, the ux30 end gate)

WebView2 157 on Björn's PC (Ryzen 5 5600X, the window idling at about 170 Hz), the probes' build served into a release
window on a scratch profile, the PC otherwise quiet.

**A closed fold laid the page out on every restyle.** On the PR files page each slot change laid out once (14 in 14 at
every rate and both sizes), while the Mac's Chrome had none. A trace with invalidation tracking named the cause: for
each drawn, closed `<details>` Chromium takes its `details-content` slot out of layout and puts it back on any restyle
(even `data-sel` alone). Docs, Builds and Inbox did the same. One rule now hides a closed fold's content with
`display: none` instead of Chromium's `content-visibility: hidden` (`css/chrome.css`, held by `tf-css.test.ts`). With it
those four pages laid out 0 times in 10 restyles and 10 without, in either order, and their window screenshots were the
same with and without it. **Fable's ruling (Björn away):** keep the rule; what it gives up is that the webview's own find
(F3, or Ctrl+F while a dialog is open) no longer opens a closed fold, which agrees with knobas's find (Ctrl+F is
knobas's, `installFindKey`, and leaves a closed fold out by the design system's rule). Deleting the rule reverses it.
If a Chromium update changes how it hides the slot, `theme-switch.mjs` on PR files in WebView2 will count the layouts
again. After it, slot change against toggle in ms, 1440 x 868 then full screen (5120 x 1369), colours 0 layouts in 14
on every page and rate:

| Page | 1x | 4x | 6x |
| --- | --- | --- | --- |
| Tickets | 10 / 11, 11 / 11 | 52 / 53, 50 / 53 | 85 / 87, 81 / 82 |
| Repository | 4 / 4, 4 / 5 | 20 / 21, 21 / 22 | 35 / 35, 35 / 39 |
| PR files | 10 / 10, 10 / 10 | 49 / 54, 49 / 57 | 85 / 83, 82 / 94 |

The first frame of a cold load had Dracula's page colour in 5 of 5 loads at every rate and both sizes.

**The colour picker dragged (risk 9).** `testenv/perf/theme-picker.mjs` sends what the system's picker sends the page,
an `input` on the swatch's colour field, 170 times a second for 2 s. Each one was handed on to the editor, so the editor
re-read its colours, figures and preview for every pick, and the window was restyled once a frame. A swatch now hands on
one colour a frame, the last picked, as a preview: the editor's role list and preview follow it, and the window keeps
its colours until the drag ends (`change`), the change is kept (300 ms after the last pick), or anything flushes the
themes. This is the fallback the design named for risk 9; no cue was added, since the preview the person is watching
follows every frame and the window answers the release.
