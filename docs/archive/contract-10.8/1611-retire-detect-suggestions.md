- **The IPC command schema and the Rust handler barrel — issue #1611 (2026-10-22): the uncalled
  `detect_suggestions` command retires.** No frontend file outside the dev fake and tests called
  it. The room tray that called it on every finished sync went with #881, and since #1485 (PR
  #1597) the sync scheduler requests a pass in Rust after every run that ends OK
  (`suggestion_pass::SuggestOnSync`), with no IPC. A retirement, not an addition: nothing is added
  to either barrel, and no file is added to or removed from `commands/` or `app/src/lib/ipc/`. The
  handler list is append-only (§10.8's frozen list); its one deleted line is the exception this
  entry ratifies, as #1481's and #1578's entries ratified theirs.

  **What it supersedes.** Neither the entry files nor the sections of this contract are edited;
  these parts of them no longer hold:

  - #41's `detect_suggestions() -> u32`, the command that runs a detection pass and answers how
    many proposals it wrote; #745's "`detect_suggestions` keeps its name, its arguments (none) and
    its `u32` answer", its `SuggestionPass` *on `AppState` as `suggestions`*,
    `detect_suggestions_inner` and `Lifecycle::suggestions()`; and #771's "`detect_suggestions`
    still answers the number of proposals written". The pass, its queue, its own connection and
    its statement timeout (#745, migration `0031`) all stay. `SuggestionPass::run`, the waiting
    half of the queue, stays as the seam the tests drive.
  - #1485's ruling that bring-up hands the one pass to the scheduler **and** to `AppState`.
    Bring-up still builds it exactly once and hands it to the scheduler; `AppState` no longer
    holds it.
  - #1578's sentence that `knobas_core::mini_board::source_statuses` stays in `knobas-core`.
    #1612 removes it (not IPC surface; named here because #1578's entry said it stays). The
    board's `sources` field reads the same statement through `observed_statuses`.

  **The diff, by surface.**

  - `crates/knobas-app/src/lib.rs`'s handler list loses `commands::entity::detect_suggestions`.
    `AppState` loses its `suggestions` field, and bring-up passes the pass it builds to
    `sources::start` alone.
  - `commands/entity/suggestions.rs` loses `detect_suggestions` and `detect_suggestions_inner`;
    `commands/app.rs` loses `Lifecycle::suggestions`, whose one caller was the command.
  - `app/src/lib/ipc/entity.ts` loses `detectSuggestions`. No generated mirror type changes: the
    command answered a bare `u32`.
  - The fake IPC loses its `detect_suggestions` handler, and the census's suggestion test checks
    the three suggestion commands that remain; `residue.test.svelte.ts`' mock drops
    `detectSuggestions`.
  - The tests that ran a pass through `detect_suggestions_inner` (`suggestion_pass.rs`,
    `suggestions_ipc.rs`, `ui_pool_probe.rs`, `responsiveness_probe.rs`) call
    `SuggestionPass::run`, which answers the count as `u64`; `wiring.rs`' bring-up pin drops its
    `AppState` half.
  - `crates/knobas-core/src/mini_board.rs` loses `source_statuses` (#1612). Its test in
    `crates/knobas-core/tests/it/mini_board.rs` reads one source's offer through `mini_board::read`,
    and its prepared-statement test goes: the board's own one reads the same `status_paths`
    statement.
  - Prose only: the module docs of `suggestion_pass.rs` and `SuggestOnSync::new`'s doc, the
    module comment of `commands/entity/suggestions.rs` (four commands to three), one doc comment
    in `commands/entity/start_work.rs`, the bring-up comment in `lib.rs`, the fake's handler
    comment, `README.md` and `docs/diagrams/suggestion-pass-{light,dark}.svg` drop the manual pass.

  **What is not touched.** No migration, no event, no `WriteOp`, nothing under
  `crates/knobas-source/src/**` or `crates/knobas-http/**`, `crates/knobas-app/src/{error,
  profile}.rs` unchanged. `room_suggestions`, `accept_suggestion`, `dismiss_suggestion` and
  `proposal_evidence` stay as they are.

  Pinned by `fake-tauri.test.ts`'s census (every command the fake answers is one the barrel
  invokes, and every barrel command is answered or on its `UNANSWERED` record) and by
  `wiring.rs`' `bring_up_builds_one_suggestion_pass_for_the_scheduler`, and for #1612 by
  `knobas-core`'s `a_sources_offered_statuses_are_its_live_corpus_in_the_boards_order`. The
  rulings are in `docs/archive/2026-10-unattended-rulings.md` under `## #1611` and `## #1612`.
