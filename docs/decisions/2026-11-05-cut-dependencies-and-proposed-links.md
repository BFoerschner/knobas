# Dependencies and Proposed links leave the app, their backend stays, 2026-11-05

Björn ruled this on 2026-11-05, in the request for this PR. It binds as Björn's ruling, not an agent's.

## The ruling

**The pages go.** Dependencies (`#/deps`, `#/deps/job/<key>`, the retired `#/map`) and Proposed links (`#/review`, `#/review/<link id>`) have no page, no nav entry and no row in the launcher's `>` menu. Those addresses now draw *Nothing lives at …*. The folders `app/src/lib/tf/deps/` and `app/src/lib/tf/review/` are deleted, with their styles.

**What only opened them goes too, as a consequence.** Björn named the pages, not these. The top bar's proposals badge, the Connections drawer's *N proposed* box and the *proposed link, judge it* chip on a pull request's Commits tab all led to the Proposed links page, so they are gone along with the reads behind them. A link accepted from a proposal keeps showing its reason in the links panel, because by then it is a link.

**The backend stays, unchanged.** No crate, migration, IPC command or generated mirror changes. The wrappers in `app/src/lib/ipc/` and the `?fake-ipc` handlers stay as well. The suggestion pass still runs when a sync ends, so proposals keep collecting with nothing drawing them, and Dependencies' tables keep their rows. Link sets' picker still calls the `dependencies` read (`tf/sets/ports.ts`).

**Monitors does not change.** Björn, 2026-11-05, on the PR's first version, which had added a button to Monitors and moved the asset tree under it. Monitors' page, its styles and its tests match what they were before this PR.

The asset tree is neither of the two pages and stays. Where it lives now is an agent's ruling, recorded in the PR.

## Why

Björn: "I want to concentrate on the core of knobas and what it does well." Björn may want both back, which is why the backend stays.

## Left as it was

`mockups/ticket-first/requirements.md` keeps R-6.3, R-6.4 and R-11.10 to R-11.23 as the record of what the pages were. They no longer describe the app. The Controls specimen keeps its large decision row (`.tf-decide`), whose words came from Proposed links, because the design system names that control.

## Cost of reversing

Reverting this PR's squash commit brings back the pages, their tests and their styles, and needs no migration. The risk grows with time: while no page calls these commands, only `crates/knobas-app/tests/it/` exercises them (`dependencies_ipc.rs`, `suggestions_ipc.rs`), so a later backend change could leave the restored pages needing repair.
