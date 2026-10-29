- **The IPC command schema — issue #1643 (2026-10-23): `open_checkout` loses its `command`
  argument, the program it starts inherits no `KNOBAS_*` variable, and `room_suggestions` and
  `list_entities` read at most 500 rows.** Spec #1620, ruling C4; findings IPC-2 and IPC-7 of the
  2026-10-22 review. The amendment to #844/#846's entry, whose `open_checkout` took a `command`
  that ran an unsaved template (*Try it*). No migration, no event, no new type, no new module in
  `commands/` or `ipc/`; the generated mirror (`app/src/lib/ipc/generated/`) does not change,
  since no mirrored type does.

  ```rust
  #[tauri::command] pub async fn open_checkout(.., entity_id: String,
      application: Option<String>, file: Option<String>, line: Option<u32>)
      -> Result<Vec<String>, IpcError>;
  ```

  - **The program is always a saved template (ADR-0016).** `open_checkout` runs the application
    `application` names, the default when `null`, from the `checkout.applications` row. *Try it*
    in *Add an application* saves the draft with `set_checkout_applications` and opens it by the
    id the answer gives it; from then on the dialog edits that entry, and closing it unsaved
    saves the list back as it was before the first try. `knobas_app::checkout::Run` is gone and
    `checkout::open::open` takes `application: Option<&str>` in its place.
  - **The child's environment.** The spawned program gets knobas's environment less every
    variable whose name starts with `KNOBAS_` (under `just dev`, `KNOBAS_DB_URL` and its
    password), as the git paths drop `GIT_*`. Everything else, `PATH` and `HOME` included, passes.
  - **The clamp.** `knobas_app::commands::ROW_CEILING` (500) is the ceiling `list_sync_runs`
    already read under, now named. `room_suggestions_inner` and `list_entities_inner` read
    `limit.min(ROW_CEILING)` rows; `total` is still the whole room's. The commands' arguments do
    not change. `entity.ts` declares the same `ROW_CEILING` (held equal by
    `commands::entity`'s `the_mirror_pages_by_the_backends_row_ceiling`), and its `listEntities`
    reads a `limit` past it in reads of the ceiling from `offset` on, up to `LIST_MOST` (1000)
    rows, keeping `total`, `tally` and `groups` from the first read: the code, repository and
    pull request views that ask for 1000 rows still get 1000. `room_suggestions` takes no
    offset, so the pull request page's read of 1000 proposals gets 500, as many as Review's own
    read (`REVIEW_LIMIT`).

  Mirrored in `app/src/lib/ipc/entity.ts`: `OpenAt` loses `command` and `openCheckout` sends
  none; `listEntities` pages past the ceiling (`entity.test.ts`). Answered by the fake IPC (`fake/checkouts.ts`, which no longer reads `command`), held by
  `fake-features.test.ts` (a `command` sent anyway is not run) and `fake-tauri.test.ts`'s
  argument census. Witnessed by `tests/it/checkout_ipc.rs`
  (`trying_a_command_runs_the_template_the_setting_holds`), `checkout::open`'s
  `a_spawned_program_inherits_no_knobas_variable`, `tests/it/suggestions_ipc.rs` and
  `tests/it/entity.rs` (`a_limit_past_the_ceiling_reads_five_hundred_*`), and
  `TfOpenIn.test.svelte.ts`.
