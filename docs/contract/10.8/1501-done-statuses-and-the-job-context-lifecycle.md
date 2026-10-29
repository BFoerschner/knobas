- **`crates/knobas-source/src/**` (through `knobas-spi-types`) and the IPC schema — issue #1501
  (2026-10-21): `KindPaths` gains `done_statuses`, and a ticket's job context is archived exactly
  while the ticket is done.** The lifecycle the grilling of 2026-10-21 ruled for #1427 (spec #1498,
  `docs/archive/2026-10-unattended-rulings.md`, "Archived follows the ticket's status, and only
  that"; "Done-like is a `done_statuses` name list beside `blocked_statuses`"). No migration, no
  event, no command added, removed or renamed, no argument changed, no `WriteOp`, nothing under
  `crates/knobas-http/**`, no new module in `commands/` or `ipc/`. Reserved for Björn, ruled by the
  implementer on Opus: the sign-off; the rulings are under `## #1501` in the same file.

  **`KindPaths` — additive.** One field, after `blocked_statuses` and in its shape:

  ```rust
  #[serde(default)] pub done_statuses: Vec<String>,
  ```

  the status names a source calls done-like, in its own spelling, matched case-insensitively against
  the declared `status_name`. Not in `payload::field::ALL` (a set of names, not a path, as
  `blocked_statuses` is not). The Jira adapter declares `Done`, `Closed` and `Resolved` on `ticket`;
  Confluence, Uptime Kuma, Gitea, TeamCity and the mock declare none, each with its reason beside
  its declaration.

  **External adapters.** The descriptor crosses the WIT boundary as its serde JSON (`descriptor:
  func() -> json`), so `adapter.wit` and `knobas:adapter@1.3.0` are unchanged: a component that
  sends no `done_statuses` decodes as one declaring none and archives nothing, and one that sends
  them is read by this host and ignored by an older one.

  **The IPC schema — additive.** Every `SourceDescriptor` on the wire (`list_adapters`, and an
  `InstalledAdapter`'s `template`) carries `done_statuses: Array<string>` on each `KindPaths`;
  `app/src/lib/ipc/generated/sources.ts` is regenerated. Nothing in the shell reads it.

  **`promote_context`** keeps its name, its one argument and its answer (`ContextRow`). It now
  answers with the context anchored on the entity **archived or not** (the live one first, else the
  newest archived one), where before an archived context was passed over and a second one minted; a
  context it mints for a ticket whose status is done-like is born with `archived_at` set. It reads
  the done-like sets from the lifecycle's registry (the process registry the sync engine shares),
  as `get_entity` and `list_entities` do, so it answers `not_ready` only while the database is
  coming up, as before. Two promotes of one anchor serialise on a
  transaction-scoped advisory lock (`pg_advisory_xact_lock(47, hashtext(anchor))`, the two-key
  space, which never meets a sync run's one-key lock), because migration `0010`'s
  `context_anchor_idx` covers only unarchived rows.

  **The sync run** (`knobas-sync`'s `run_locked`), after the saved views and inside the run's
  transaction, runs `knobas_core::context::follow_status` for its source with its own descriptor's
  paths: every context anchored on one of the source's live items is archived (`archived_at =
  now()`) when the item's status is done-like and it is live, and revived (`archived_at = null`)
  when the status is outside the set and it is archived. Revival takes the newest archived context
  per anchor and none where the anchor already has a live one. Off the mirror, every run, not gated
  on the declaration. Nothing else writes `archived_at`.

  **The one-time pass.** `knobas_app::sources::bring_up`, after the installed adapters load and
  before the scheduler starts, runs `knobas_core::context::archive_done` over every configured
  source's template declarations: it archives every live context whose anchor is done-like and
  never revives. Every bring-up runs it; after the first it finds nothing. A failure is logged and
  does not fail bring-up. No marker, so no migration.

  **`job_reach`** (`knobas_core::reach`) starts from the anchored context whether archived or not, so
  a done job still shows what its context held. The walk is `reach_walk!`, so the two other readers
  of it move with it: search's `reach:` filter (`knobas-search`'s `reach_ids`) and the job drawer's
  parent read (`link_sets.rs`'s `JOB_PARENT`). The inbox's alert rule (`held_by_any_context!`) and
  `list_contexts` still skip archived contexts, unchanged.

  Witnessed by `knobas-spi-types`' `the_field_names_the_sql_macros_use_are_the_serialized_ones`,
  `knobas-source`'s `done_statuses_round_trip_and_a_descriptor_without_them_declares_none`,
  `knobas-wasm-host`'s `convert::tests::a_components_done_like_set_decodes_and_one_without_it_declares_none`,
  the Jira crate's `a_ticket_declares_done_closed_and_resolved_as_done_like`, the Confluence and Kuma
  crates' "declared nowhere" tests, `knobas-core`'s `tests/it/contexts.rs` lifecycle tests
  (`a_context_first_made_for_a_done_ticket_is_born_archived`,
  `promoting_a_ticket_with_an_archived_context_returns_it_and_makes_nothing`,
  `racing_promotes_of_a_done_ticket_make_one_context`,
  `following_the_status_archives_on_done_and_revives_when_it_leaves`,
  `reviving_leaves_an_older_archived_context_behind_a_live_one`,
  `the_one_time_pass_archives_done_jobs_and_is_a_no_op_the_second_time`) and `tests/it/reach.rs`'s
  `an_archived_contexts_adds_are_still_reached`, `knobas-sync`'s `tests/it/job_contexts.rs`, and
  `knobas-app`'s `tests/it/job_lifecycle_ipc.rs` (promote, the alert rule, `open_alerts`, the open
  contexts, and the pass through `bring_up`).
