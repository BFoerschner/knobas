- **The IPC schema, issue #74 (2026-08-30):** `SourceSummary` gains one field,
  `auth_kind: Option<knobas_source::AuthMethod>`, mirrored as `auth_kind: AuthMethod | null`.
  Granted under the **2026-08-29 delegation** (recorded in the #43 entry above) — an IPC
  addition, no migration, no milestone exit, no contract-battery clause.

  **Why it needed a ruling at all**, since the change is four lines: `SourceSummary` is what
  `list_sources`, `add_source` and `update_source` answer with, so it is the IPC schema, and
  §10.8 is single-writer. #36's implementer found the gap while building the sources view
  (PR #73), correctly declined to widen a frozen surface from inside a stream, and filed #74
  instead. This is that ruling.

  **What the field is, and what it deliberately is not.** It is the *kind* of credential —
  the same `AuthMethod` union `NewSource.auth_kind` and `SourceDraft.auth_kind` already submit,
  and not a fifth spelling of the same idea. `null` is the widening: every source the Add-source
  form can create authenticates by construction, but a *stored* row need not — the compiled-in
  mock reaches nothing and stores `auth_kind = 'none'` — and `AuthKind::from_db` maps a spelling
  it cannot read to that same `None` rather than guessing. Both reach the view as `null`, and
  both mean the one thing the view has to render: there is no credential kind to name.

  **It carries no secret and cannot.** The value lives in the OS keychain, no command reads one
  back (§2.2), and `sources_crud.rs`'s standing "no secret-shaped field" assertion runs over the
  serialized summary, so it covers this field the moment it exists.

  Additive on every axis: no existing field, command or event name changes meaning, nothing is
  removed, and every existing consumer of `SourceSummary` decodes unchanged. The column is
  already stored — `source_config.auth_kind` since `0001`, read into
  `knobas_sync::config::SourceConfigRow` since `0002` — so **no migration**; `0011` is still the
  next free number. Pinned by `sources_mirror.rs` (the exact key set, plus a test that the wire
  spelling is the input DTOs' union widened by `null` and not a tagged enum) and by
  `sources_crud.rs`, which walks *every* `AuthMethod` through the column and back out of both
  read paths — `crud::list`'s loop and `summarize` build the summary separately, so a field
  wired into one and forgotten in the other is a column that is right after adding a source and
  wrong after reopening the view.

  Ratified by the orchestrator as issue #74 itself, which specifies the field and its acceptance
  criteria. Ex-stream-D task 16 — the sources view's auth column naming the `AuthMethod` — lands
  with it, and the honest degradation PR #73 shipped in its place is retired.

