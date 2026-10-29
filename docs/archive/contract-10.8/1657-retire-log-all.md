- **The IPC command schema and the Rust handler barrel — issue #1657 (2026-10-23): the uncalled
  `log_all` command retires, with the bulk path behind it.** Ruling D2 of
  `docs/decisions/2026-10-review-planning-rulings.md`. No frontend file outside the barrel, the
  dev fake and its test called it. The *Log all…* dialog (`TfLogAll.svelte`, #828) has always
  asked `log_all_preview` for the rows and sent each ticked row through `log_work` with its
  comment, so two paths logged a week with different failure rules and only one of them could be
  reached. A retirement, not an addition: nothing is added to either barrel, and no file is added
  to or removed from `commands/` or `app/src/lib/ipc/`. The handler list is append-only (§10.8's
  frozen list); its one deleted line is the exception this entry ratifies, as #1481's, #1578's
  and #1611's entries ratified theirs.

  **What it supersedes.** Neither the entry files nor the sections of this contract are edited;
  these parts of them no longer hold:

  - #283's `log_all(days: Vec<DayWindow>) -> Vec<Worklog>`, appended to the `commands::time::`
    group and to `app/src/lib/ipc/time.ts`, and its sentence that `log_all` **re-derives** its
    own work rather than being handed the plan back. The rule still holds of the path that is
    left: `log_work` takes no block ids and re-derives the covered blocks from the day it is
    given. #283's exclusions (`UNLOGGED_BLOCKS`: manual, on a ticket, not yet logged) still hold
    of `log_all_preview`, which reads them through `worklog::unlogged`. #283's empty comment no
    longer holds: each row goes with the comment the reader left in the dialog.
  - #775's `log_all` half: `time::week::log_all_in_background`, the sentence that the checks
    `log_within` made still run before the answer, and "`time::week::log_all` keeps the awaited
    send". `log_work` and `time::worklog::log` / `log_in_background` are unchanged.
  - #896's "`log_work`, `log_all` and the week's reads still see closed blocks only" holds of
    `log_work` and the reads; there is no `log_all` left to see anything.

  **The diff, by surface.**

  - `crates/knobas-app/src/lib.rs`'s handler list loses `commands::time::log_all`.
  - `commands/time.rs` loses the `log_all` command. Its two mirror tests
    (`the_mirror_invokes_the_commands_by_their_registered_names`,
    `the_mirror_sends_the_argument_names_tauri_expects`) drop their `log_all` rows, and
    `log_all_preview`'s doc names `log_work` as what the dialog sends.
  - `knobas_app::time::week` loses `log_all`, `log_all_in_background` and `log_all_with`;
    `knobas_app::time::worklog` loses `log_within`, whose one caller was `log_all_with`. `plan`,
    `vet`, `DayWindow` and `PlannedWorklog` stay. Doc comments only: `PlannedWorklog`'s (so the
    regenerated `app/src/lib/ipc/generated/time.ts` changes in that comment and nowhere else),
    `worklog::commit`'s, the `time::week` module's and its line in `time/mod.rs`.
  - `app/src/lib/ipc/time.ts` loses `logAll`; `logAllPreview`'s doc names `logWork`. No import
    changes: `Worklog` is still `logWork`'s answer.
  - The fake IPC loses its `log_all` handler (`fake/time.ts`) and its row in
    `fake-answers.ts`. `fake-features.test.ts`' week test stops logging through it; the test
    beside it, *Log all's dialog path*, already empties the plan through `log_work`.
  - `crates/knobas-app/tests/it/week_ipc.rs`' three *Log all* tests send the plan the way the
    dialog does, one `time::worklog::log` per planned row, through a `log_the_plan` helper; its
    module doc says the week's tests send the plan that way. `fake-features.test.ts`' week test
    is retitled *the week and Log all's plan*.
  - Prose only: `mockups/shared/capabilities.md`'s *Log all* row names `log_work` in place of
    `log_all`, and a comment in `the_mirror_sends_the_argument_names_tauri_expects` reads "on
    both" for "on all three".

  **What is not touched.** No migration, no event, no `WriteOp`, no DTO. `TfLogAll.svelte` and
  its tests are unchanged. Nothing under `crates/knobas-source/src/**` or `crates/knobas-http/**`,
  `crates/knobas-app/src/{error,profile}.rs` unchanged. `log_all_preview`, `log_work` and
  `worklog_draft` stay as they are.

  Pinned by `fake-tauri.test.ts`'s census (every command the fake answers is one the barrel
  invokes, and every barrel command is answered or on its `UNANSWERED` record, and
  `fake-answers.ts` types exactly the barrel's commands), by `commands/time.rs`' two mirror tests
  and by `ipc_mirror` (the generated barrel is current). The rulings are in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1657`.
