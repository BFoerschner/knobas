- **The IPC schema of `list_sources` (and of every command answering a `SourceSummary`) and the
  `sources.ts` mirror — issue #1196 (2026-10-18): the source list carries each source's role
  claims, and the shell picks each surface's sources by them.** No migration, no new command, no
  `commands/` or `ipc/` module, no `WriteOp`, no `KindPaths` field, no change to
  `crates/knobas-app/src/error.rs`, no change to any role's minimum. Parent spec #1192 (ADR-0029,
  glossary **Role**).

  **The field.** `knobas_app::sources::SourceSummary` gains `roles: Vec<RoleClaim>`, taken from the
  adapter's descriptor template as `kinds` is (#1193's `SourceDescriptor::roles`), on both read
  paths: `list_sources` and the summary `add_source` and `update_source` answer. Empty for an
  adapter that claims nothing (the mock) and for a stored source whose adapter is no longer
  compiled in. Mirrored in `app/src/lib/ipc/sources.ts` as `roles?: RoleClaim[]`: always sent,
  optional in the mirror only so that stand-in summaries written before this entry still type-check
  (a summary without it claims nothing). Held by `sources_mirror.rs`'s `SourceSummary` shape test
  and `sources_crud.rs::the_source_list_carries_the_role_claims_of_each_sources_adapter`.

  **What the shell does with it.** `TfShell.svelte` hands every `list_sources` answer to
  `tf/roles.svelte.ts`. Code draws the rows of the sources claiming `code-host`, Builds `ci`, Docs
  `wiki` and Monitors `monitoring`, whatever their adapter kind; the job page's stale-builds
  warning names the sources claiming `ci` (it read a `build` kind before). Until the list has been
  read, and for a row whose source the list does not name, nothing is narrowed. Tickets is #1200.
