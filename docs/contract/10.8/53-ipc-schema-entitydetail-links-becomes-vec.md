- **IPC schema**, issue #53 (2026-08-28): `EntityDetail.links` becomes
  `Vec<knobas_core::link::LinkEntry>`, where a `LinkEntry` is the link record plus a `LinkEnd` --
  the end the reader is *not* on (`entity_id`, `kind`, `title`, `deleted_at`). Two new DTOs, both
  riding inside `EntityDetail` rather than crossing on their own, the same precedent as that
  entry's `LinkRow.note`. No new command; the read side was already `get_entity`.

  **This supersedes two sentences of the #52 entry above**, both true when written: "`EntityDetail`
  keeps its shape" (it does not -- `links` changes element type) and its reference to `links_of`,
  which no longer exists. `entries_of` **replaced** it rather than joining it, so there is one read
  with one store battery behind it. The old entry is left as history rather than rewritten, the
  same treatment §9 gives the superseded TeamCity locator table.

  **The wire shape is nested, deliberately:** `{link, other}`, produced by `#[sqlx(flatten)]` on
  the query side and *not* `#[serde(flatten)]` on the wire. Flattening it into one bag would
  collide `id` -- the link's and the other end's -- so a later "tidy-up" that flattens it is a bug,
  not a simplification.

  The hydration reads `knobas.entity`, **not** `sync.live_item`: that is the whole mechanism by
  which a link to an entity withdrawn upstream still resolves and is marked, instead of dangling.
  A test pins it from both ends against a genuinely tombstoned fixture row.

