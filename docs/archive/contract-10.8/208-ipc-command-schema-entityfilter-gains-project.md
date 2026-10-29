- **The IPC command schema, issue #208 (2026-09-01):** `EntityFilter` gains `project:
  Option<String>`, and `mini_board` gains a fourth argument carrying it — the first of M2.6's
  **two** frozen-surface touches, both ratified in advance by the spec (#188) Björn approved:
  "Two §10.8 entries follow: the room filter grows a project dimension, and an additive command
  reports the projects a source's corpus shows" (ADR-0010's own closing consequence). Written
  with the implementing PR per the #175/#177 pattern.

  **The exact shape**, on the existing input DTO and the existing command:

  ```rust
  // knobas_app::commands::entity
  pub struct EntityFilter { /* … */ pub project: Option<String> }
  #[tauri::command] pub async fn mini_board(.., ctx_id: Option<String>, sources: Vec<String>,
                                            project: Option<String>) -> Result<MiniBoard, IpcError>;
  ```

  **It narrows *within* `sources`, and that is the whole of why it is a second dimension rather
  than a replacement for the first.** A project key is unique only inside its own source, so two
  sources against two Jira instances can both show a `PAY`; a filter carrying the key alone would
  union them into one room and leak one source's work into another's. A project room therefore
  sets both (`{sources: [source_id], project: key, context: null}`), and `None` is unscoped in the
  same "empty means unfiltered" sense `sources`, `kinds` and `context` already have — bound as a
  **nullable parameter** (`$7::text is null or …`), the discipline this module's room statements
  record, so the SQL stays static and nothing concatenates a value into it.

  **All four room statements honour it, including the two that reach past the tombstone filter.**
  `LIVE_UPDATED`, `LIVE_TITLE`, `ALL_UPDATED` and `ALL_TITLE` take the same predicate: a filter
  honoured by two of the four would be a room that changes meaning the moment a caller asks to see
  withdrawn work (§5a), which is a difference no reader could attribute. The two live statements
  gained the `i` alias the two `include_deleted` ones already had; no column, ordering, parameter
  or `count(*) over ()` changed, and `every_statement_names_its_columns_and_no_two_are_the_same`
  still holds.

  **Because a room hands its filter to every tile, this is the whole of the narrowing.** The
  room's own kinds-and-count read, the mini board and everything else in the room scope the same
  way, with no per-tile special case — spec #188's story 2. The mini board takes it as a *third
  IPC argument, nullable* — the "fourth argument" this entry opens with counts the Rust
  signature, whose first is the `State` handle — rather than inside a filter object, because
  that command already takes its two narrowings apart (#177's entry above, which this leaves
  otherwise untouched).

  **This supersedes one sentence of the #177 entry above** — "`app/src/lib/shell/contexts.ts` has
  two populations of room" — true when written: since #209 the derived population also holds one
  room per project the census shows, narrowing by `sources` *and* `project` at once. That entry's
  "exactly one of the two is ever narrowing" still holds of the two it names — `context` joins
  neither. The old entry is left as history rather than rewritten, the same treatment #53's entry
  gives the two #52 sentences it supersedes.

  **Two readers deliberately not narrowed, recorded so neither is discovered as a bug.**
  `MiniBoard::sources` — the ticket detail's status select (#179) — stays the statuses a ticket's
  whole *source corpus* shows: a project room with nothing finished still has to be able to offer
  *Done*, and narrowing the offer by the room would make a move available only where it had
  already been made. The **suggestion tray** is likewise unnarrowed, per spec #188: its read is
  keyed on sources and context and a proposal belongs to a room when *either* end does, which for
  projects is genuinely ambiguous — a suggestion linking an `INT` ticket to an `ERP` one belongs
  to both project rooms or to neither. A project room shows its source's tray.

  **The value is a payload read outside an adapter (ADR-0007), and its failure direction is
  stated where the read is.** `knobas_core::project_key_read!` is the one statement — Jira's
  `fields.project.key` and TeamCity's `buildType.projectId`, both at a *type-checked* path, so a
  record spelled some other way misses rather than opening a room headed `{"id":3}`. Direction: a
  record with no readable project key belongs to **no** project room and is still in *All work*
  and its source's room — absence, never a wrong room, and no "No project" room, which ADR-0010
  refuses because nothing is hidden when two other rooms still hold the item. Four tests, one per
  way to miss (absent, blank, whitespace-only, not-a-string), plus the board's own
  `a_ticket_with_no_readable_project_is_on_no_project_rooms_board`, which asserts both halves of
  that sentence.

  **The mirror**, appended to the existing `app/src/lib/ipc/entity.ts`: `EntityFilter.project:
  string | null`, and `miniBoard` widened to `Pick<EntityFilter, "sources" | "context" |
  "project">` — the room's own filter object, so a caller cannot narrow by one dimension and
  forget another. Both are pinned by `the_entity_filter_shape_matches_its_typescript_mirror` in
  `crates/knobas-app/tests/entity_mirror.rs`, whose fixture exercises `project` as `null` per that
  file's rule, and whose round trip is what makes a Rust-only field visible.

  **Which barrels were appended: neither, and that is correct.** `list_entities` and `mini_board`
  were already registered in `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, and
  `app/src/lib/ipc/index.ts` already re-exports `./entity` wholesale, so a widened DTO and a
  widened argument list reach the frontend without either barrel being touched. The append is the
  second entry's.

  **What did not change.** **No migration — `0013` is still the next free number** — and no
  normalized project model: contract §4.1's four-field normalization stands, and the value has
  been arriving in the payload since M1 because the Jira sync has requested `project` in its base
  field list from the start. No new module on either side of the bridge: the filter field and the
  predicate are in `commands/entity.rs`, the reads are `knobas_core::{mini_board, project}`, and
  `knobas-core` is not in the frozen list. Nothing under `crates/knobas-source/src/**` — the
  Source SPI, its DTOs and the contract battery are untouched, so ADR-0006 stands, and no adapter
  declares a thing. No new event (`contexts:changed` is what already says the switcher's rooms
  moved). `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are untouched.
  No **existing** field changes meaning: `EntityFilter.project` is additive and `None`-by-absence,
  and every caller that shipped before this passes `null`.

  Ratified by the orchestrator as spec #188 and issue #208, whose acceptance criteria specify the
  dimension, its tests and this entry.

