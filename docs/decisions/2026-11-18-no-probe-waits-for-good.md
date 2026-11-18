# 2026-11-18: No timing probe waits for good

**Asked (Björn, through the orchestrator, urgent):** in the ux18 release gate `keys.mjs` hung for 27 minutes at
1920x1080@2. The page was drawn and idle on Tickets, and a DevTools call never answered (most likely `fresh()` waiting
for a page's load event at the start of the next case). No probe had a watchdog.

## What changed

- **Every wait has a deadline** (`testenv/perf/cdp.mjs`, `LIMITS`; `testenv/perf/README.md`, *No probe waits for
  good*, has the table and the reasons). A command gets 30 s. A trace's end, a screenshot, a heap collection, a profile's
  stop or a synthesized scroll gets 120 s. An awaited evaluate gets its own `timeout` plus 15 s, an event 60 s, the
  debug port's page list and the socket 15 s, a launched Chrome's port 30 s and a probe's `fetch` 120 s. Each limit sits
  far past the slowest healthy run (6x at 5120x1440@2 on the scale database), so it only ever ends a call that would not
  have answered. A socket that closes fails every waiting call at once. A stall's message names the probe, size,
  throttle, case and call.
- **A case runs under `runCase()`**, with a wall-clock limit of 10 minutes (the longest case takes about two). On a
  stall the session moves to a fresh page and the old one is closed. When the Chrome the probe launched has died, a new
  Chrome is started. The calls that set the page up are made again, and the case runs once more. A second stall makes
  the case failed-to-run: it is reported, counted over its line in every check (`keys`, `open`, `moves`, `budget.mjs`,
  `compare-moves --budget`, the others' exit codes), and the probe goes on with the next case. Once an attempt is given
  up its calls fail at once, and so do the later calls of an attempt that stalled. So a probe that catches the error
  cannot wait out every later deadline.
- **Wired:** keys, open, moves, run, zoom, fonts, folder-render, state-colours, runs-read, docs-choices, openapi,
  right-click, git-starts, folders, stored-trees, synced-file, compare-open and compare-moves. A soak's lap is not tried
  again (a fresh page would not read what the session grew): the soak ends failed-to-run. `compare.mjs` keeps its own
  recovery (a fresh Chrome for the next step). The single-shot probes (`profile`, `moment`, `invalidations`, `long`,
  `scrollers`, the request counters) have no cases. A stall there ends the run with the message, never a hang.
- **Tests:** `testenv/perf/test/fake-chrome.mjs` stands in for Chrome. It leaves a chosen call or event unanswered,
  dies on one, never opens its socket or never writes its port. `watchdog.test.mjs` (`watchdog-test.sh`, in `just
  witness-unit`) runs at a hundredth of the limits: 21 tests, about 12 s. They cover each deadline, a retry, a twice-stalled
  case, a dead Chrome, the wall-clock limit, a caught stall, and 11 probes run whole against it.

## Measured (the Mac, under the Mac queue, scale database copy, release `headless_ipc`)

- **The hang did not reproduce.** keys.mjs at 1920x1080@2 at 4x ran every case (14) in 5.5 minutes with no stall
  and no retry. Four cases were over at a load of 15 to 33. Those four, old (ecc33edd) and new back to back, at
  1920x1080@2 4x, median rows of five presses:
  tickets-letter 117.6 / 95.0 ms, tickets-not-priority 84.7 / 93.9, everything-four-letters 99.5 / 89.8,
  docs-spaces-letter 17.1 / 18.0. The difference is noise; the watchdog adds no work to a healthy call.
- **The fixture, old / new at 4x:** open inbox first useful 20.5 / 27.7 ms and tickets-board drawn 117.6 / 116.1; moves
  tickets, tickets-list and inbox moved nothing on both; run tickets-list scroll 60 / 59.8 fps, code-filter slowest key
  43.5 / 47.3 ms, `#/inbox` first frame 35.2 / 51.0 (one sample each).
- **Real Chrome's recovery:** an evaluate that never settled was retried on a fresh page that kept the replayed device
  scale (2) and width (1920). A Chrome killed during a case was started again on a new port and the case ran, in 0.7 s.
