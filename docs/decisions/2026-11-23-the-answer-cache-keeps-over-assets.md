# The answer cache keeps its answers over Assets (2026-11-23)

The ux30 end gate's Windows pass, in knobas's own WebView2 window on Björn's PC (Ryzen 5600X), had every page opened
from another section in its `late` round over the PC's line: Builds drawn at 218 to 257 ms at CPU 4x and 342 to 397 ms
at 6x, the list of every ticket 104 to 126 ms at 1x, Monitors 420 to 452 at 6x. The Mac's own gate had Builds late and
a run opened from a repository's Builds tab late over its line too (109 and 102 ms at 1x).

## Why

`open.mjs`'s `late` round visits every section before it presses the place. Logging every command the window sent
over that tour (in the window, over CDP) found three the answer cache (`ipc/answer-cache.ts`) knows neither as a read
nor as quiet: `asset_types`, and `asset_tree` twice, all from Assets. An unknown command drops every kept answer, as a
write does. So after Assets, Builds read its whole pipeline list again (about 60 ms of backend on the PC) before it
could ask for the selected pipeline's runs, the ticket list read its views and then its board, and Monitors its
roster.

## What changed

`asset_tree` (one Miller column) and `depends_on_this` (what stops when an asset does) are kept reads: both are
functions of the mirror, neither carries a visit, so a visit keeps them too. `asset_types` is quiet: it reads a table
compiled into the app. Nothing else in the tour drops answers (`timer_heartbeat` drops the time reads only, as before).

## Proof

`answer-cache.test.ts` "drops nothing on the estate's reads, and keeps them over a visit" failed before (`docs_outline`
asked twice) and passes. In the window at 6x (one run each, 1440 x 868, Björn idle), with the front end served into the
release build: Builds late 342 to 217 ms drawn, the ticket list late 259 to 164, the board of every ticket 244 to 209,
an asset holding 300 containers on its return 246 to 181 (its columns are now kept too).
