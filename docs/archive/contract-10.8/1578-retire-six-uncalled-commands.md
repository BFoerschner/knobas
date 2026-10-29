- **The IPC command schema and the Rust handler barrel — issue #1578 (2026-10-21): six uncalled
  commands retire, `context_assets`, `source_assets`, `recent_activity`, `context_members`,
  `source_statuses` and `smart_list_items`, with the `MemberAsset` DTO.** No frontend file
  outside the dev fake, the generated mirror and tests called any of them: the rooms UI that
  drew the Assets tile, the History panel and the status-bar stream went with #1087 (PR
  `dda9a9e3`), and the job card that replaced the ticket detail shows the current status when
  the workflow cannot be read rather than asking for a fallback offer. A retirement, not an
  addition: nothing is added to either barrel, and no file is added to or removed from
  `commands/` or `app/src/lib/ipc/`. The handler list is append-only (§10.8's frozen list); its
  six deleted lines are the exception this entry ratifies, as #1481's entry ratified its four.

  **What it supersedes.** Neither the entry files nor the sections of this contract are edited;
  these parts of them no longer hold:

  - §2.4's `smart_list_items(id: String, limit: u32) -> SearchResponse`. A smart list opens
    the way the launcher has always opened one, by searching for `list:<id>`: `search` reaches
    the same `Searcher::list_response`, answers in the same shape and clears the same badge.
    `knobas_search::Searcher::smart_list_items` stays, as the engine's own entry to that path.
  - §2.5's `recent_activity(limit: u32, entity_id: Option<String>) -> Vec<ActivityRow>`, with
    the sentence that made the detail's history panel and the status bar one call, and stream
    D's D5 grant of the `entity_id` argument. The detail's history rides on `get_entity`
    (`EntityDetail.activity`) and the estate pane's on `get_asset`, both through
    `knobas_core::activity::recent`, which stays. #819's paragraph *What `recent_activity`
    answers* still holds of that read: `visited` lines stay out of it, pinned by
    `a_visit_is_not_in_the_recent_activity`.
  - #47's `context_members(ctx_id) -> Vec<String>`. Membership is still
    `knobas_core::context::member_ids`' one statement, read by the context scope on
    `list_entities`, by `mini_board` and by the tray's proposal scope; #177's and #208's
    placement notes that name `context_members` describe where their lines went, not a command
    that must stay.
  - #434's `context_assets(ctx_id) -> Vec<MemberAsset>` and `MemberAsset { asset, path }`, and
    #435's `source_assets(source_id) -> Vec<MemberAsset>`. #435's two computed fields on the
    estate's shapes stay. Membership through ancestors (ADR-0008's clause) is pinned in
    `knobas-core/tests/it/contexts.rs`, and the two exit witnesses (#440, #455) now read it
    through `member_ids`.
  - #760's `source_statuses(source_id) -> Vec<String>` and its mirror `sourceStatuses`.
    `EntityDetail.status` stays. `knobas_core::mini_board::source_statuses` stays in that crate
    and has no caller in the app.

  **The diff, by surface.**

  - `crates/knobas-app/src/lib.rs`'s handler list loses six lines: `commands::entity::
    context_members`, `::source_statuses`, `::recent_activity`, `commands::search::
    smart_list_items`, `commands::assets::context_assets` and `::source_assets`.
  - `commands/assets.rs` loses its two tile commands; `commands/entity/contexts.rs` loses
    `context_members`; `commands/entity/board.rs` loses `source_statuses`;
    `commands/entity/lookups.rs` loses `recent_activity` and `recent_activity_inner`;
    `commands/search.rs` loses `smart_list_items` and `smart_list_items_inner`.
  - `knobas_app::assets` loses `MemberAsset`, `in_context`, `monitored_by`, `tile_rows` and the
    two tile statements `MEMBER_ASSETS` and `MONITORED_ASSETS`; `ipc_mirror.rs` stops exporting
    `MemberAsset`, and the regenerated `app/src/lib/ipc/generated/assets.ts` no longer has it.
  - `app/src/lib/ipc/assets.ts` loses `contextAssets`, `sourceAssets` and the `MemberAsset`
    import and re-export; `app/src/lib/ipc/entity.ts` loses `recentActivity`, `contextMembers`
    and `sourceStatuses` (its `ActivityRow` re-export stays, for `EntityDetail.activity`);
    `app/src/lib/ipc/search.ts` loses `smartListItems`.
  - The fake IPC loses the five handlers it answered (`source_assets` it never answered: it
    stood on the census's `UNANSWERED` record, which loses that row). `fake/activity.ts`
    answered only `recent_activity`; its log lines move into `fake/entity.ts`, their one
    reader, and the file goes.

  **What is not touched.** No migration, no event, no `WriteOp`, nothing under
  `crates/knobas-source/src/**` or `crates/knobas-http/**`, `crates/knobas-app/src/{error,
  profile}.rs` unchanged, nothing in `knobas-core` or `knobas-search`. `detect_suggestions` was
  #1485's. Since PR #1597 the sync scheduler requests the pass in Rust, and the command still
  has no frontend caller, which is #1611's to rule.

  Pinned by `fake-tauri.test.ts`'s census (every command the fake answers is one the barrel
  invokes, and every barrel command is answered or on its `UNANSWERED` record) and `ipc_mirror` (the generated barrel is current). The rulings are in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1578`.
