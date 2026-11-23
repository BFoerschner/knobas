# Optical sizes for headings only (2026-11-23)

The ux30 end gate's Windows pass had first opens drawn late in WebView2 on Björn's PC: the list of every ticket 328 to
342 ms at CPU 6x, a Docs page 334 to 353, Builds 322 at full screen. Traces of each first open showed most of the
frame after the rows was layout, a third of it shaping text (`InlineNode::ShapeTextIncludingFirstLine`).

## Why

Inter 4.1 is variable on `wght` and `opsz` (14 to 32), and with `font-optical-sizing: auto` each size drawn asks for
an instance of its own. The 11 to 15 px of lists and text asked for five-odd instances per weight that draw the same
glyphs (the axis starts at 14), and WebView2 on Windows shapes and measures each instance's glyphs from cold on a
page's first open. With optical sizing off everywhere (an experiment, one run each at 6x): Docs page 327 to 254 ms
drawn, Builds 304 to 246, a ticket 353 to 314, the ticket list 349 to 320, the board of every ticket 349 to 318. Keys
were the same either way.

## What changed

Optical sizing is off for every element (`fonts.css`: `font-optical-sizing: var(--optical, none) !important`, so a
`font` shorthand, which resets it, does not turn it back on), and on wherever the heading face is set: each of the 47
rules with `font-family: var(--head)` sets `--optical: auto`, which the heading's words inherit.

What a reader sees: text and lists at 11 to 14 px draw exactly as before (they were at the axis' start already); 15 px
moves from optical size 15 to 14, one eighteenth of the axis, which is not visible; every heading and display figure
keeps its own optical size, the display cut at 24 px and up. Body text that Settings' sizes raise past 15 px now stays
in the text cut instead of drifting toward display, which suits running text. Reviewed through the frontend-design
skill before it went in.

## Proof

`optical.test.ts`: the universal rule is in `fonts.css`, and every rule that sets the heading face turns optical
sizing on (both failed before).
