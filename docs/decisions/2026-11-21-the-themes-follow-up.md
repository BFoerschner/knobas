# 2026-11-21: The themes follow-up: press trips, moves through a change of highlighter, and what they found

The ux27 pass, PR #2479. What the themes build (`2026-11-21-a-theme-is-yours-and-code-is-coloured-by-two-engines.md`)
left owed, two small fixes from the sweep's list, and the working tree's Commits lines.

## Press trips for Code colours

A new kind in `testenv/ui-checks/trips.mjs`, `press-result`: real presses (a file chooser answered with a checked-in
file through `Page.setInterceptFileChooserDialog`), then a check that must hold within 4 s. Nine trips on Settings ›
Appearance › Code colours:

- `hl-radio-tanstack`, `-shiki`, `-pierre` by pointer and `hl-radio-key` by an arrow key: the radio and
  `knobas.highlighter`, the preview's five jobs asked again of the new engine (the worker's `open` messages), its words
  and every line's box the same, and its colours new. The three engines draw the preview differently, so a choice that
  recoloured nothing fails.
- `theme-duplicate`: a copy kept, in the light slot and the editor, its 21 swatches buttons.
- `theme-import-vscode` (`testenv/ui-checks/themes/vscode-fixture.json`, JSONC as VS Code writes it) and
  `theme-import-zed` (a family of two): kept, the roles read, the toast, the editor on the first.
- `theme-slot`: the light slot's select by pointer, ↓ and Enter; GitHub Light kept and drawn on `<html>`.
- `theme-swatch`: one's own theme's keyword typed as `#C8B6E2` in its swatch's hex field; the row's colour, its contrast
  (about 1.9:1), its warning and fix, the kept theme and the preview's keyword.

With their presses left out, all nine fail on their first check.

## `just moves` through a change of highlighter

`testenv/perf/moves.mjs` gains places that open a code area, change the highlighter on Code colours by its radio
(Shiki, the slowest, then TanStack Highlight on the return) and open the area again: a file, a commit's diff with
unchanged lines full, quiet and grey and split, a pull request's diff, Compare on the working tree and a Docs code
block, each with its code lines held to their boxes (a place's new `track`), and the preview with each radio pressed in
view. At 1x and 4x, 1440x868@1 and 2560x1440@1.25 on the WSL box: everything held but a pull request's diff.

**Found and fixed.** Returning to a pull request's diff drew *Unified*, *Split* and *Rendered* at the toolbar's left
edge for a frame, then 242 px right (241 at 1.25), at every size and throttle, after any leave, not only a change of
highlighter. Keep-alive hides the page; the commit picker's `ResizeObserver` read its width as 0 and wrote
`--tf-pick-w: 0px`, and the return drew without the picker's room until the next measure. A picker with no box now
keeps the room it had (`TfPrFiles.svelte`, `pickRoom`). After it, the 15 places (the existing pull-request ones
included) recorded 108 opens with nothing moved.

## Smaller

- The CSP comments in `crates/knobas-app/src/lib.rs` named Shiki as the highlight worker's engine and D2 as a worker.
  The worker now imports the chosen highlighter: Pierre Highlights compiles WebAssembly (`'wasm-unsafe-eval'`), TanStack
  Highlight and Shiki (on its JavaScript regex engine) need no eval. D2 left the workers in #2381.
- `just check-front` reads `VITEST_WORKERS` (4 when unset), so the Mac's rule of 2 is `VITEST_WORKERS=2 just
  check-front`.

## `settings-renderers-add-default`

The branch box agent saw this long-paths case fail at 5120x1440 and 2048x1152@1.25; the sweep (`604f391b`) had it
passing. At `92bcfb58` it passes at all four sizes: alone and in the whole run (204 of 204 screens) on the WSL box, and
alone on the Mac. The sweep is right and the row needs no change.

## The working tree's Commits lines

Built as `2026-11-21-the-working-tree-is-a-choice-in-the-branch-box.md` says under *Built later the same day*, and run
once in the app's own window on a real clone: a scratch HOME, live Gitea as its source, `tidewater/ledger-api` cloned by
the app's *Clone*, a branch `feat/local-only` with one commit and two uncommitted files made in the clone, *Working tree*
picked from the branch box's native menu by real macOS input. Commits drew *Uncommitted changes, 2 files changed*,
`f33a1bb Note the local-only branch` with *Not pushed*, then the forge's commits, under *feat/local-only is not on Gitea
(live) yet*, and *New pull request* was off with *Push feat/local-only to Gitea (live) first*.
