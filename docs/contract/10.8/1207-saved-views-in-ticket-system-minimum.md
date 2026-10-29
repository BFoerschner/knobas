- **`knobas_core::payload` (a `Frame` variant), `crates/knobas-source/src/**` (the ticket-system
  minimum), the IPC schema of `list_adapters` and the `sources.ts` mirror — issue #1207
  (2026-10-19): saved views come from any ticket-system source.** No migration, no new command,
  no `commands/` or `ipc/` module, no `WriteOp`, one `KindPaths` field (`system_name`), no change to
  `crates/knobas-wasm-host/wit/adapter.wit` (`knobas:adapter@1.0.0` is untouched), no change to
  `crates/knobas-app/src/error.rs`. Parent spec #1192 (ADR-0029). Ratified by the implementer on
  Opus under Björn's 2026-10-18 reserved-steps rule; the ruling is in
  `docs/archive/2026-10-unattended-rulings.md`.

  **The frame.** `knobas_core::payload::Frame` gains `SavedView`, on the wire `"saved_view"` (the
  other two stay `"pipeline"` and `"run"`). A kind whose `KindPaths.frame` is `saved_view` is a
  ticket system's saved view: Jira's `jira_view` (its boards and favourite filters) declares it.
  Mirrored in `app/src/lib/ipc/sources.ts` as `Frame = "pipeline" | "run" | "saved_view"`. A
  descriptor from a peer that predates it never sends the word, and an external adapter declares
  it in its descriptor JSON, so no WIT change.

  **The minimum.** `knobas_source::role::Minimum` gains `optional_kinds`: kinds a source filling
  the role may emit beside the required ones, held to their paths when emitted and skipped when
  not. The ticket-system minimum's is one saved view (frame `saved_view`), with no required
  paths. Every other role's is empty. The five required-kind rows of the 1193 entry are
  unchanged.

  **The projection's shape is unchanged.** `knobas_sync::jira_views` is now
  `knobas_sync::saved_views`, keyed on the kinds that declare the frame (`saved_views::declared`)
  instead of the kind id `jira_view`, so the module names no adapter. It writes the same tables
  (`sync.jira_view`, `sync.jira_view_member`, `knobas.ticket_view`), in the same columns, under
  the names migration `0044` gave them. Two values become source-neutral: the group is the
  descriptor's `name` followed by `boards`, or `filters` for a view whose payload `kind` is
  `filter`, and a payload `kind` that is neither `board` nor `filter` is a board, as a missing one
  already was, rather than a run failed on `jira_view_ref_kind_chk`.

  **The system's name (`KindPaths.system_name`), added by the merge-manager.** The group's first
  word is the saved-view kind's declared `system_name`, else the descriptor's `name`. An
  instance's descriptor carries the name a person gave the source (`JiraSource::descriptor` sets
  it from `display_name`), so a source named *Jira EU* would have listed its views under *Jira EU
  boards*, outside the Tickets pane's `JIRA_GROUPS`. Jira's `jira_view` declares `system_name:
  "Jira"`, so every Jira source's views read *Jira boards* and *Jira filters* as before.
  `Option<String>`, `#[serde(default)]`, `null` on the wire when undeclared; mirrored in
  `sources.ts` as `system_name: string | null`. It crosses the WIT boundary inside the descriptor's
  JSON, so `knobas:adapter@1.0.0` is still untouched.
