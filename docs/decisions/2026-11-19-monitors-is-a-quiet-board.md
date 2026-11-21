# 2026-11-19: Monitors is a quiet board, and a poll draws only what it changed

**Asked (Björn, for ux21):** "look into the monitors page. it's so slow, for so little important data. why? it should
be one of the smoothest pages of all, I think the hiding and so on is so unnecessary. Make it perform better, use the
/frontend-design:frontend-design to redesign it so it still looks proper nice." And then: "Speed targets for the monitor
page is not just the draw speed, but also the fps. it should be near 240 like everything else too."

**How it was measured.** A new probe, `testenv/perf/monitors.mjs` (`just monitors-budget`, the perf README's *Monitors
at 239 Hz*): the first open, a return, the page idle, a beat landing (every source's run ends, as each Uptime Kuma poll
does, after a command that appends a sample to every monitor) and four more, a wheel scroll, a hover sweep at a 239 Hz
pointer's pace and a monitor opened and closed, each under its own Chrome trace, at CPU 1x, 4x and 6x, at 1440x868@1,
2560x1440@1.25 and 5120x1440@1. Headless Chrome on Björn's PC (the WSL box, his Ryzen 5 5600X), under the PC queue,
driving the app served from the Mac over the windowless backend (release build) on a copy of the saved scale database
(218 monitors), its secrets moved out. The saved database's samples are from the day it was made, so its bars were
empty; the copy was given a day of beats first (a sample a minute for 24 hours a monitor; 16 down since a spot of their
own in the last 6 hours, one in seven flapping, one in nine slow for an hour, a 40-minute gap at 03:00), 305,000 rows:

```sql
-- A day of beats: every monitor every 60 s for 24 h, up to now. Down monitors (state down in the mirror) went down at a
-- spot of their own in the last 6 h; one in seven flaps (a down minute now and then), one in nine is slow for an hour,
-- and every monitor has a 40-minute gap at 03:00 (knobas closed).
delete from knobas.monitor_sample;
with mons as (
  select i.entity_id, (i.payload::text like '%"down"%' or i.payload::text like '%: 0%') as d, row_number() over (order by i.entity_id) as n
    from sync.item i where i.kind = 'monitor'
), downs as (select entity_id from knobas.monitor_sample where false)
insert into knobas.monitor_sample (entity_id, taken_at, state, response_time_ms)
select m.entity_id, t,
  case
    when m.n % 13 = 0 and t > now() - make_interval(mins => (30 + (m.n * 17) % 330)::int) then 'down'
    when m.n % 7 = 0 and (extract(epoch from t)::bigint / 60 + m.n) % 97 = 0 then 'down'
    when m.n % 9 = 0 and t between now() - interval '9 hours' and now() - interval '8 hours' then 'warn'
    else 'up' end,
  (20 + (m.n * 7) % 180)::int
from mons m, generate_series(now() - interval '24 hours', now(), interval '60 seconds') as t
where not (t between date_trunc('day', now()) + interval '3 hours' and date_trunc('day', now()) + interval '3 hours 40 minutes');
```

## Why a page with so little data was slow

**Each Uptime Kuma poll, every minute, read the whole day of every monitor again, shipped it to the page, and the page
drew every row again.** The roster read every sample of the last 24 hours (305,000 rows for 218 monitors), thinned them
in Rust and sent 23,000 of them, 1.4 MB, to the page: 475 ms in the backend each time. The nav's badge read the same
day again on every sync's end to keep one sample a monitor (`distinct on`), 270 ms. The page then parsed the 1.4 MB,
bucketed every sample into 48 half-hours on the main thread, replaced every row object and drew every row it held again,
because every row's response time and its *read* time had moved by a millisecond. A beat cost 36 to 54 ms of main thread
at 1x and up to 218 ms at 6x, with tasks of 11 to 87 ms: at 239 Hz that is dozens of dropped frames a minute while the
page did nothing. On top of that the roster was a windowed table: each scroll swapped rows in and out (24 to 26 tasks
over a 239 Hz frame in a 5 s scroll at 1x), and each row held a hover group of *Hide*, *Pause* and *Refresh* measured for
room on every hover.

## What changed

**The backend sends what is drawn.** `0099` keeps `knobas.monitor_half_hour`, each monitor's worst state in each half
hour on the UTC grid, written by a trigger as each sample is written (whoever writes it), backfilled from the last two
days and pruned past two days by the retention tick. The roster reads 48 short rows a monitor and sends each as
`MonitorRow::bar`, 48 letters, with `down_since` for a down monitor (the first sample of its newest run of `down`, read
off the sample index for down monitors only), and each monitor's newest sample by one keyed read (`cross join lateral
... limit 1`). `monitor_roster` takes `withSamples` (additive: left out, the samples come as before); the page passes
`false`. The badge's `monitors_down` reads the newest sample the same way.

| Read (scale copy, a day of beats) | before | after |
|---|---|---|
| `monitor_roster`, backend | 475 ms, 1.42 MB | 29 ms, 124 KB |
| `monitors_down`, backend | 270 ms | 5 to 7 ms |
| a beat's reads at the page, round trip through the tunnel | roster 602 ms, badge 522 ms | roster 107 ms, badge 58 ms |

**A poll draws only what it changed.** Each read keeps every row whose drawn parts did not change as the object it was
(`keepDrawn`), and the list is keyed, so a kept row draws nothing; a row draws its response only while it is slow, so a
poll that moves every response by a millisecond draws nothing. A beat now draws one row again (the monitor that changed)
where it drew 30 to 44, and every row only when the half hour turns. A sync's end reads the roster, the open alerts, the
sources and their status, and *Assets nothing watches* only while that tab is shown.

**What was removed, and what deferral stayed.** The windowed list (`Windowed`, its spacer rows and its overscan), the
fold of hidden monitors (`sessionFold`, `hiddenFoldOpen`, the `<details>` and its summary row), the held row a mouse's
*Hide* left under the pointer (`row-hold`), the rows' hover groups of actions and the room measured for them, the strip's
mask on every row, and the day of samples over the bridge. Drawing every row whole was measured first: scrolling got
cheaper than the windowed table's, but a first open and a return at 6x doubled (241 and 213 ms), and opening a monitor took
299 ms at 6x. Two causes, each found by trace and taken out: each row's `contain` and `position` made all 218 rows
isolated pieces that every frame of a scroll laid out into layers again on the main thread (`Layerize`, 477 ms over a 2 s
scroll at 4x; 48 without), and the strips' masks did the same as effects. What stayed is `content-visibility: auto` on
**blocks of 16 rows**: every row is in the DOM and found by find-in-page, a block's room is its rows' exact height, so
nothing moves as one comes into view, and only the blocks near the viewport are styled, laid out and painted. On each row
it cost scrolls three times as much (each row a clip again); on blocks of 16 it halved the opens and kept the scroll.
The open monitor stands over the rows' end in a column of its own: when the roster narrowed to make room, opening one
drew every row on screen again.

**Hiding is kept, and costs nothing.** A hidden monitor's alerts still reach no inbox and raise no notification (#2447).
It is drawn after the roster, all of them, under one line (*2 hidden monitors. Their alerts don't reach the inbox or
notify you.*), dimmed, its bar grey, its state neutral, no *Ack*: nothing folds, so nothing redraws, reads again or moves.
*Hide* and *Unhide* are the open monitor's, one button that holds about two seconds after its press as *Acked* does
(F-351), so a double press is one hide. The press trip that walked a list of *Hide*s (`monitors-hide-walks`) and the
row-room trip (`monitors-row-room`) have nothing left to check and are gone; the others moved to the open monitor.

## The design (through `/frontend-design:frontend-design`)

A calm board for one developer who has it open all day: **what is wrong is the loud thing, and everything up stays
quiet**, inside the design system. One line under the filter bar counts what is wrong in the width that shouts (the job
key's narrow 800 Archivo at `--t-d1`): *16 down* in `--err`, *2 slow* in `--warn`, *202 up* in `--ink-2`; with nothing
wrong, one figure, *218 up*, in the ink. The roster is one sheet of rows of one height (38 px): the monitor and its type,
its state (a chip only where something is wrong, a quiet word for up and paused), when it went down or how slow it is,
the strip, the 30-day uptime, *Ack* on a down monitor, and what it watches at the row's end, where a wide window's room
goes, so at 5,120 px a monitor's facts still stand together. Down and slow rows carry their tone in a 3 px edge. The
strip's up half-hours are a pale `--ok` (42 % over `--sheet`), down and slow at full tone: two hundred healthy strips
read as one pale band and a red bar is the first thing seen. A row is pressed (or its name, from the keyboard) to open
the monitor over the rows' end, a floating panel in an end sidebar that drags: its name, its state, its strip at 5 px a
bar, its target in the code face, response, uptimes, certificate, assets and when it was read, then *Ack*, *Pause* or
*Resume*, *Hide* or *Unhide*, *Refresh*, and its page in Uptime Kuma. A press changes no address, so no breadcrumb
comes or goes, and `Esc` closes it as any panel on a page; `#/monitors/at/<id>` (an inbox alert's link) opens it too,
and its way back is the address's. The design system spec says this under *Monitors is a quiet board*.

## Before and after, on Björn's PC

Headless Chrome on the WSL box, the scale copy with a day of beats, one run a cell, the head as pushed. *Usable* is the
press to five monitors' rows drawn, or to the open monitor's name. *Main thread a frame* is the moment's main thread
over the frames it presented: under 4.2 ms holds a 239 Hz display.

**1440x868@1** (before → after, at 1x / 4x / 6x)

| Moment | What | 1x | 4x | 6x |
|---|---|---|---|---|
| first open | usable, ms | 55 → 41 | 101 → 72 | 117 → 84 |
| return | usable, ms | 38 → 32 | 95 → 66 | 114 → 66 |
| open a monitor | drawn, ms | none → 42 | none → 66 | none → 95 |
| a beat | longest task, ms | 11.2 → 11.5 (the same first read; 3.7 and 4.2 in other runs) | 51.8 → 12.1 | 62.8 → 18.9 |
| a beat | rows drawn again | 30 → 218 (the session's first read: the open drew the answer cache's roster, an hour old, and every bar caught up; 1 in every other run, and the probe now lands a beat before the one it measures) | 30 → 1 | 30 → 1 |
| scroll | main thread a frame, ms | 0.81 → 1.09 | 3.05 → 3.91 | 4.72 → 5.44 |
| scroll | frames dropped | 0 → 0 | 0 → 0 | 0 → 0 |
| hover | main thread a frame, ms | 1.87 → 2.13 | 6.4 → 6.95 | 11.89 → 10.64 |

**2560x1440@1.25** (before → after, at 1x / 4x / 6x)

| Moment | What | 1x | 4x | 6x |
|---|---|---|---|---|
| first open | usable, ms | 67 → 42 | 188 → 63 | 129 → 78 |
| return | usable, ms | 53 → 34 | 93 → 59 | 112 → 73 |
| open a monitor | drawn, ms | none → 32 | none → 86 | none → 96 |
| a beat | longest task, ms | 13.1 → 3.8 | 55.1 → 19.6 | 67 → 22.4 |
| a beat | rows drawn again | 44 → 1 | 44 → 1 | 44 → 1 |
| scroll | main thread a frame, ms | 0.96 → 1.74 | 3.72 → 6.6 | 5.98 → 7.49 |
| scroll | frames dropped | 0 → 0 | 0 → 0 | 2 → 1 |
| hover | main thread a frame, ms | 2.24 → 1.47 | 7.73 → 5.27 | 11.59 → 7.13 |

**5120x1440@1** (before → after, at 1x / 4x / 6x)

| Moment | What | 1x | 4x | 6x |
|---|---|---|---|---|
| first open | usable, ms | 49 → 53 | 99 → 60 | 132 → 75 |
| return | usable, ms | 42 → 53 | 92 → 54 | 139 → 63 |
| open a monitor | drawn, ms | none → 49 | none → 93 | none → 123 |
| a beat | longest task, ms | 12.5 → 3.7 | 54.2 → 42.7 | 87.3 → 35 |
| a beat | rows drawn again | 44 → 1 | 44 → 1 | 44 → 1 |
| scroll | main thread a frame, ms | 1.19 → 1.88 | 4.71 → 6.17 | 6.63 → 8.4 |
| scroll | frames dropped | 0 → 0 | 1 → 0 | 1 → 2 |
| hover | main thread a frame, ms | 2.27 → 2.68 | 7.84 → 9.58 | 11.96 → 14.65 |


At 1x every moment holds a 239 Hz frame on average. A beat's longest task is 3.7 to 4.2 ms in a poll (the first read of a
session, which brings every bar up to date, 11.5); the rest of a beat's long tasks are other pages' reads (the inbox's shelves, a kept pipeline's runs). The scroll is
composited at 60 frames a second with no frame dropped at 1x, its main thread a little above the windowed table's (1.1 to
1.9 against 0.8 to 1.2 ms a frame: the scroller's thumb is painted each frame, over more rows), with fewer long tasks at
1440 (4 against 24). An idle page does 0.4 ms of script in 3 s; its long tasks, where there are any, are V8 collecting
what the open left.

**The page's other budgets**, the same machine and copy, at 1440x868@1: `open.mjs` holds Monitors' first open, return
and synced return at every throttle (useful 16 to 23 ms, drawn 39 to 233 ms) and its late return (the page drawn anew
after keep-alive let it go) at 1x and 4x (drawn 40 and 156 ms); **at 6x the late return draws in 302 to 319 ms against
its 300**, where the windowed table drew 137: building all 218 rows at once is that much, its first rows on screen at
19 ms. It is the one cell over its line, left open rather than met by staging rows in after the first frame, which is the
deferral this pass took out. `keys.mjs` (new cases): a letter into the filter 19, 41 and 62 ms, a Backspace that brings
every monitor back 21, 77 and 165 ms, against 100, 100 and 200. `run.mjs`: the frame budget's Monitors lines all within
(the scroll 60, 59 and 55 frames a second, its longest task 28 ms at 4x and 42 at 6x; the filter's slowest key 82 ms at
4x and 135 at 6x). `moves.mjs`: Monitors and a monitor opened, first and return, at 1440x868@1 and 2560x1440@1.25, 1x
and 4x, on the scale copy and the fixture, with and without syncs: nothing moved.

**The strip is its runs.** The first redraw drew each strip as 96 colour stops, a `color-mix` in each up one, and a
roster drawn whole parsed and resolved 218 of them: the Backspace that brought every row back took 168 ms at 4x and 244
at 6x, and the late return 419 at 6x. Now a strip is one stretch of colour for each run of half-hours of one state (one
to five a row, most of the day), under one repeating layer that draws the gaps in the colour the row sits on, and the
pale up colour is mixed once (`--tf-mon-up`).

## Owed on Windows

Every number above is headless Chrome on Björn's machine, not WebView2 in knobas's window: the 239 Hz frame itself, the
scroll's thumb in WebView2, and the open monitor's panel and its edge at 125 %, from the Windows pass.
