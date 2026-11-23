# The start compiles the code of the pages whose first open compiled the most (2026-11-23)

The ux30 end gate's Windows pass had first opens over the PC's line in knobas's own WebView2 window (Edge 157, Ryzen
5600X): a repository's landing usable at 217 to 236 ms at CPU 4x and 367 to 371 at 6x (the ux28 pass owed it too), a
run opened from a repository's Builds tab at 291 to 352 at 6x, a ticket and a Docs page drawn over 300 at 6x.

## Why

A trace of each first open in the window (`disabled-by-default-v8.compile`) showed the main thread compiling: 86 ms of
`V8.CompileCode` in a repository's first open at 4x, 35 in a Docs page's, 23 in a run's. The window's code cache works
(148 entries after a session) but holds only the functions compiled when it was made, at the end of the page's first
run. A page's code that first runs when the page is first opened is compiled at that press, on every start.

## What changed

- `vite-compile-hints.ts` puts Chromium's explicit compile hint, `//# allFunctionsCalledOnLoad`, as the first line of
  the start's chunk of the pages whose first open compiled the most (`src/lib/shell/compile-hints.ts`): a
  repository, a ticket, Tickets, a run and Builds, Docs. The script is then compiled whole as it loads, off the main
  thread while it streams, and the code cache holds all of it.
- `vite.config.ts` makes those modules a chunk of their own (`first-opens`, 700 kB) that the start still loads. Hinting
  every start chunk (2.5 MB) made each start 100 ms slower (page loaded to nav drawn 240 to 250 ms against 138 to 145),
  since every start then reads the whole compiled code back. Hinting only `first-opens` costs nothing measurable (137
  to 189 ms, median about 150, against 148 to 176 for the head).
- The chunk is split without its dependencies, or it pulls in most of the start (1.6 MB). Split that way, a shared
  module ran before one it used had initialised (`Cannot read properties of undefined (reading 'size')`, and the shell
  never drew), so modules run in source order whatever chunk holds them (`output.strictExecutionOrder`).

## Proof

`compile-hints.test.ts` (the hint on the start's `first-opens` chunk only, once, first line). In the window at 6x, one
run each against the head: a repository's first open usable 367 to 292 ms, a ticket's drawn 340 to 279.
