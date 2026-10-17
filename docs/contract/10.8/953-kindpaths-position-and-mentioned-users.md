- **`knobas_core::payload`, `crates/knobas-source/src/**` and the order of `docs_tree`'s answer
  — issue #953 (2026-10-17): `KindPaths.position`, and the Confluence sync names every mention.**
  No new command, no migration, no grammar key, no `WriteOp`, no `commands/` or `ipc/` module, no
  new IPC field.

  **The declarations** (`knobas_core::payload`, re-exported by `knobas_source`). `KindPaths` gains
  `position: Vec<PayloadPath>`, `#[serde(default)]`, in `field::ALL` as `field::POSITION`: where
  a page sits among its siblings in the source's own order, a whole JSON number, lowest first. The
  contract battery weighs it as `Wanted::Integer`, as it weighs `estimate`. Read through the
  existing `declared_integer!`. Mirrored in `app/src/lib/ipc/sources.ts`; held by
  `sources_mirror.rs`.

  **`docs_tree`'s order.** `DocSpace.pages` was ordered by title. It is now ordered by the declared
  `position`, nulls last, then by title, then by entity id; a parent's children are read out of
  that list in list order, so each sibling set is in the source's order. The shape is unchanged.
  Witnessed by `docs_ipc.rs`'s `children_are_in_the_sources_order_and_by_title_where_it_has_none`.

  **The Confluence adapter** (not frozen; recorded because it fills the two). The sync copies
  `extensions.position` to the payload's root `position` where it is a whole number (Confluence
  answers `"none"` for title order), and the descriptor declares `position` there. It also asks
  `GET /rest/api/user?key=…` for every `ri:userkey` a page's body or comments mention that the
  record does not name, once per key per run, and keeps the users found at the payload's
  `mentioned_users`, where the Docs projection reads them. A `404` is a miss; any other failure
  stops the lookups for the run and never fails the sync. The adapter's cursor version goes from 2
  to 3 (merge-manager, as #860 did), so the first run after the upgrade is one full sync that
  gives every mirrored page its `position` and `mentioned_users`; without it, a page would gain
  them only when next edited. The mirror keeps only `userKey`, `username` and `displayName` of
  each user record. Witnessed offline by `sync.rs`'s fake and, against
  the real container, `confluence_live.rs`'s
  `a_mention_of_anyone_resolves_and_children_keep_confluences_order`.
