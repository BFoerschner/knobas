- **`crates/knobas-source/src/**` — issue #2030 (2026-10-24): `SourceView::refuse_writes`, one
  additive method on the view DTO.** Spec #1192, story 10. No field, no block, no `WriteOp`, no
  change to the `Source` trait or the contract battery, no IPC command, event, DTO field or
  barrel line, no migration; the wire shape of `SourceView` is unchanged, and no generated
  TypeScript type moves.

  `refuse_writes(&mut self, refused: impl FnMut(&str) -> Option<String>)` asks `refused` about each
  write the view offers, in drawing order, and draws each one it answers a reason for as one the
  reader cannot take: an action (the toolbar's or a `Block::Actions`'s) gets `disabled` set to the
  reason, replacing any the adapter drew; a `Block::Settings` whose `write_op` is refused has every
  value's `editable` cleared; a `Block::Diff` whose `commentable` op is refused loses
  `commentable`, since `Commenting` carries no reason. Its match over `Block` has no wildcard arm.
  It is for an adapter that learns what its **credential** may write after drawing what the
  entity offers (ADR-0020 rule 4: a token can be replaced, so the write is drawn disabled with its
  reason rather than left out). Gitea is its one caller (`crates/knobas-source-gitea/src/view/grants.rs`).
  Witnessed by `view::tests::a_refused_write_is_disabled_wherever_the_view_offers_it`.
