# The launcher flattens an answer once (2026-11-23)

The ux30 end gate's Windows pass had two of the launcher's keys under *Everything* over the PC's line at CPU 4x in
WebView2 on Björn's PC: a fourth letter 100.5 ms and a long word's last letter 105 ms (line 100).

## Why

A trace of the key in the window: the search answered at about 35 ms, and the task that drew its rows spent 27 ms in
`flatten` (`launcher/rows.ts`). `Session.rows` was a getter, so every read flattened the answer again, and the
launcher reads it about ten times in a key's update (its twins, the split, *Nothing found*, the preview, the scroll to
the selected row, the list itself).

## What changed

`Session.rows` is a derived value: flattened once for each answer, the board, the actions or the typed text that it
reads, and the same list on every read until one of them changes.

## Proof

`session.test.svelte.ts` "the rows are flattened once for an answer" failed before (a new list on each read) and
passes. In the window at 4x, median of seven presses: two letters 100.2 to 77 ms, four letters 104.5 to 78, five
letters 82.4 to 58, a long word's last letter 89.7 to 71.4.
