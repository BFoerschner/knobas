# A ticket view's ⋯ is drawn, not a character (2026-11-23)

Found while the ux30 end gate's Windows pass looked into the list of every ticket's first open (241 ms drawn at CPU 4x
in WebView2 on Björn's PC, line 200).

## Why

A trace of the first open with the `fonts` category: 38 `FontCache::FallbackFontForCharacter` calls and four system
font files mapped (Cambria Math twice, Segoe UI, Times New Roman), about 50 ms at 4x. Asking the window which system
fonts each page's text drew in (`CSS.getPlatformFontsForNode` over every text node of Tickets, Builds, a run, an
asset, Monitors, Code, Docs and the inbox): the top bar's ▶ in Segoe UI Symbol on every page, read once at the start,
and on Tickets alone the views' menus, `⋯` (U+22EF), which Inter does not hold. Blink went through Segoe UI and Times
New Roman before Cambria Math had it.

## What changed

The ⋯ of a ticket view's menu, in the views pane and on the view's header, is `TfMoreIcon`: three dots drawn in the
text's colour on its middle, an em wide, as `TfXIcon` draws the ✕ for the same reason (the card-drag gate, earlier
today) and as the file header's *More* already drew its dots. Its button keeps its label (*More for …*).

## Proof

`TfMoreIcon.test.ts` (no component draws U+22EF as text) failed before (`tf/tickets/ViewMenu.svelte`) and passes. It
did not move the probe's numbers: alternated with the build before it in the window (five runs at 6x, three at 4x), the
first opens of the board and the list of every ticket drew the same either way, since in a probe's order an earlier
page of the same window has already found the font. It saves the search on the first Tickets page a window draws.
