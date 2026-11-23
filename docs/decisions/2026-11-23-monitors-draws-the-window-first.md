# Monitors draws the window's rows first, the rest a frame or two later (2026-11-23)

The ux30 end gate's Windows pass had Monitors opened from another section (`late`) drawn at 239 / 276 ms at CPU 4x and
420 / 452 at 6x in WebView2 on Björn's PC, where the line is 200 and 300.

## Why

With *Pages kept open* at its 5, a tour of every section lets Monitors go, so the next press mounts it again. A trace
in the window at 6x: the roster came from the answer cache at once (after `2026-11-23-the-answer-cache-keeps-over-assets.md`),
then one task built all 218 rows (`monitorBlock`, `monitorRow`: 150 ms of script) and the next styled and laid them
out (180 ms).

## What changed

The roster was already drawn in blocks of 16 rows, each block a box laid out only near the window
(`content-visibility: auto`) with its rows' exact room. On a fresh mount the page now builds the rows of the first four
blocks (64 rows, more than a 5120 x 1369 window holds), and three more blocks on each frame after, until every row is
drawn: a block waiting for its rows holds their room (`tf-mon-later`), so a block filling moves nothing, and a block
once drawn stays drawn (a poll, a typed word or a filter never empties one). There is still no windowing: every row is
in the document a few frames after the first draw, as the monitors-page pass wanted.

## Proof

`TfMonitors.test.svelte.ts` "the first blocks draw at once and the rest a frame or two later" (304 monitors: 4 blocks
drawn, 15 holding 16 rows' room, every row after the fill, the blocks the same elements) failed before (304 rows at
once).
