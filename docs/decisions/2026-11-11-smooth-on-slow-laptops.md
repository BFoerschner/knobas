# 2026-11-11: Smooth on slow laptops

**Asked (Björn, on PR #2479):** "I want you to start fixing all these issues so the app is smooth on even slow laptops."

**Found.** Two measuring passes of the same day, one on his Windows PC in WebView2 with the main thread slowed four and six times to stand in for his work laptop, one on the Mac in WKWebView at 120 Hz. On Windows the big lists moved 20 to 51 pictures a second at 4x and Monitors 5 to 7, while the Mac hid it. Two causes were our CSS, three our code:

1. **A `:has()` before a subject made Chromium restyle the page on every row.** `.tf-panel:has(> header) > :not(header)` and its two siblings in `job.css` turned Chromium's one invalidation set for `:has()` into "the whole subtree": each row a list drew restyled 4,098 elements on Monitors, 21 ms a frame. With those gone, `:has()` rules anchored on page-sized elements (`body:has(.tf-modal)`, `.tf-board-wrap:has(.tf-lane + .tf-lane)`, `.tf-tbl:has(th.tf-c-epic)`, `.tf-prfiles:has(.tf-dtool)`, the asset tree's `.tree:has(.gone, .solo)`) still applied that set to every cell under them: 2,646 table cells a pass on Monitors.
2. **No scroller had a background.** Chromium below a 150% display scale keeps LCD text, so it scrolls a box on its compositor only when the box's own background is opaque. Every knobas scroller was transparent, so every scroll frame was a whole main-thread frame.
3. **`Windowed` changed rows on most frames** while a list scrolled, which cost WKWebView frames at 120 Hz and Chromium a main-thread frame each.
4. **Single long tasks**: a hidden page read `scrollTop` of all of its elements (92 ms on the largest Confluence page at 4x), that page rebuilt its 216 KB body on every return and built it in one task on a first visit, and the Docs reading pane's wheel listener was not passive.

## Rulings

1. **A panel that opens with its heading is marked `tf-headed`** (`.tf-panel tf-headed`), and the box rules are written against the class. `style-cost.test.ts` checks that every panel whose first child is a `header` has it and no other does.
2. **No `:has()` before a subject that names no class, id, tag or attribute**, and a `:has()` before a subject stands only on a row-sized element, listed with its reason in `style-cost.test.ts`. A page-sized element gets a class from its markup: `tf-lane-one` and `tf-lanes` on the board's scroller (by the lanes drawn), `tf-over-modal` on the toasts (by the dialogs open, `modals.svelte.ts`), `tf-has-epic` and `tf-has-labels` on the Tickets list's table, `tf-with-tool` on a pull request's files, `alone` on the asset tree.
3. **Every rule that lets a box scroll gives it an opaque background**, the token of the surface it sits on, so nothing looks different. A scroller drawn on more than one surface (a Markdown table, a code view, a picker, a hunk) paints `var(--under)`: `.tf` sets it to `--paper`, and each surface that holds such a scroller (a panel, a dialog, a comment, the ticket preview, a file view, a diff, a PDF, a PDF diff's tinted rows) sets it to its own colour. `style-cost.test.ts` refuses a scroller rule without one; `testenv/perf/scrollers.mjs` lists every scroller on the app's pages with its own colour and the colour under it.
4. **`Windowed` holds its drawn rows while the view stays inside them** with `margin` pixels to spare (half the overscan unless a list says otherwise), then draws the view and its overscan again. A scroll changes rows a few at a time every few hundred pixels.
5. **A hidden page and a return read the place from the boxes that scrolled** (`scrolled.ts`, one capturing `scroll` listener), not from every element. A Docs page read again keeps its drawn body where it reads the same, and a long page draws its body in pieces, all of it at once where something needs the whole body (a section's address, a find, a resume, a mention).
6. **Every wheel listener is passive.**
7. **A monitor's 24-hour strip is one element**, its bars a background painted through the CSSOM with a mask for the
   rounded ends, and its title names the bar under the pointer. 48 elements a row made a filter key that drew every
   row restyle thousands.
8. **A board's card past its column's first twelve is not laid out while off the screen** (`content-visibility:
   auto`, at about a two-line card's height until it has drawn): with *Show more* pressed a board holds over a thousand
   cards, and each filter key laid them all out again. The first page is left alone: with every card skipped, a card on
   screen at the first draw changed height, which `board-rows-still` caught.
9. **Big read results are held raw** (`$state.raw`) where they are replaced whole: the Docs page and Monitors' roster,
   as Code's pages and the Docs tree already were. A deep proxy made drawing them read every field through one.
10. **The frame budget is measured, not remembered**: `just frame-budget` (`testenv/perf/README.md`), on a build whose
   Svelte runtime is the release's (`KNOBAS_PERF=1`), since development mode's bookkeeping made the measured app's
   keys several times dearer than the shipped one's.

**Tried and not kept.** A hidden kept page as `content-visibility: hidden` instead of `display: none`, so Chromium
would keep its layout for the return: layout still answers inside such a subtree, so code that reads "no box" as "a
hidden page" (⌘F's box finder, a windowed list's view) would have seen hidden pages.
