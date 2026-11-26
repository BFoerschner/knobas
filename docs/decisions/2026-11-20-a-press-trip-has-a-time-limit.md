# A press trip has a time limit (2026-11-20)

PR #2479, ux26. Two things about the press trips (`testenv/ui-checks/trips.mjs`).

## `scroller-select-run` and the bundled Inter

**Seen.** From `d859dad5` the strict trip `scroller-select-run` failed: a drag across a Builds run's title,
"Payout_IntegrationTests #1187", selected nothing. `fc2b6205` (the one-line pipeline header) was the first suspect. It was not
the cause: the trip drags across the run's title, which that commit did not change.

**Cause.** The faces knobas brings (`2026-11-20-the-faces-knobas-brings.md`), in the same range. Inter at 700 and heavier,
from 17 px up with kerning on, keeps Chrome from starting a selection on a drag across "t_I". The bundled-fonts pass found
it and fixed the app in `6e6b9edc`: headings and a Docs page's bold take Inter's 600, which selects at every size. The
trip was right and is unchanged. On `df1cbe48` it passes and selects "out_Integr".

## A stuck trip fails by name, and the run goes on

**Seen.** Twice that day the machine watch took a Mac `trips.mjs --strict` run for hung: 22 and 23 minutes with no output and
flat CPU (bundled fonts at 15:15, renderer columns at 19:04). Neither was hung. Both finished on their own a few minutes
later (316 trips in 1,559 s at 15:17; 320 in 1,549 s at 19:07), and each printed nothing because its output was piped
through `tail`. A full strict run takes about 26 minutes on the Mac with two workers. Still, a trip that never ends would
hold the machine for ever, and nothing would say which trip it was.

**Taken.** `trip-runner.mjs` walks the queue. Each trip may take at most `--trip-limit` seconds (180 by default; the longest
real trips take 77 s on the loaded Mac). A trip past its limit fails, and the failure names it. Its Chrome is stopped and a
fresh one takes the rest of the queue. Every line also goes to `<out dir>/trips.log` as it happens, and so does a progress
line each minute naming each worker's trip and how long it has been on it, so a run piped through `tail` can still be
watched. `trip-runner-test.sh` (in `just witness-unit`) runs the runner against stand-in browsers that pass, fail, throw or
never answer.
