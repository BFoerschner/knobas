- **`knobas_core::payload`, `crates/knobas-source/src/**` and the IPC schema — issue #894
  (2026-10-16): `KindPaths.comment_count` and two fields on `MiniBoardCard`.** The Tickets view's
  card draws "N comments" and the card and the preview draw the epic as `PAY-200 Payout
  reliability`. No new command, no migration, no grammar key, no `WriteOp`, no `commands/` or
  `ipc/` module.

  **The declarations** (`knobas_core::payload`, re-exported by `knobas_source`). `KindPaths` gains
  `comment_count: Vec<PayloadPath>`, `#[serde(default)]`, in `field::ALL` as
  `field::COMMENT_COUNT`: how many comments a ticket has, a whole JSON number. Jira declares it at
  `fields.comment.total`; `BASE_FIELDS` has fetched `comment` since M1 and the sync's comment
  completion rewrites `total` to the comments it holds, so no cursor bump. The contract battery
  weighs it as `Wanted::Integer`, as it weighs `estimate`. Read through the existing
  `declared_integer!`. Mirrored in `app/src/lib/ipc/sources.ts`.

  **The IPC schema.** `mini_board`'s `MiniBoardCard` gains `comment_count: Option<i64>` (through
  `declared_integer!`; `Some(0)` when the source says none, `None` on a miss) and
  `parent_title: Option<String>`: the title of the live mirrored item the card's direct parent
  names. The parent is the last position of `declared_parent_names!`, its first name; the item is
  the one at `<source_id>:<name>` of the parent's declared kind in the same source, and only if
  `declared_names_as_parent!` says that item answers to the name. An epic outside the mirror, or an
  item at that address that does not answer to the name, is `None` (ADR-0007). One primary-key
  lookup per kept card, in the same `CARDS` statement, so the board costs no extra round trip.
  Mirrored in `app/src/lib/ipc/entity.ts`; held by `entity_mirror.rs` and `sources_mirror.rs`;
  witnessed by `knobas-core`'s `tests/it/mini_board.rs`.
