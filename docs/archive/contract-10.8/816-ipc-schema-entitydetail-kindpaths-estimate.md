- **`knobas_core::payload`, `crates/knobas-source/src/**` and the IPC schema — issue #816
  (2026-10-15): `KindPaths.estimate` and three fields on `EntityDetail`.** The job card (R-4.2)
  shows a ticket's type, priority and estimate. No new command, no migration, no grammar key, no
  `WriteOp`, no `commands/` or `ipc/` module.

  **The declarations** (`knobas_core::payload`, re-exported by `knobas_source`). `KindPaths` gains
  `estimate: Vec<PayloadPath>`, `#[serde(default)]`, in `field::ALL` as `field::ESTIMATE`: a
  ticket's original estimate **in seconds**, a whole JSON number. Jira declares it at
  `fields.timeoriginalestimate`, which `BASE_FIELDS` has fetched since M1, so no cursor bump. The
  contract battery weighs it as a new `Wanted::Integer` (a JSON integer, or nothing). A source that
  keeps hours or a phrase declares no estimate rather than one in another unit. One macro,
  `knobas_core::declared_integer!`: the first candidate whose value is a JSON number written as an
  integer of at most 18 digits, as `bigint`; anything else is a miss. Mirrored in
  `app/src/lib/ipc/sources.ts`.

  **The IPC schema.** `get_entity`'s `EntityDetail` gains `issue_type: Option<String>`,
  `priority: Option<String>` (both through `declared_string!` over the existing `issue_type` and
  `priority` paths) and `estimate_seconds: Option<i64>` (through `declared_integer!`). Each is
  `None` on a miss, never a default. Read in the same `DETAIL` statement as `status`, so opening an
  entity costs no extra round trip. Mirrored in `app/src/lib/ipc/entity.ts`; held by
  `entity_mirror.rs` and `sources_mirror.rs`; witnessed by `tests/it/job_ipc.rs`.
