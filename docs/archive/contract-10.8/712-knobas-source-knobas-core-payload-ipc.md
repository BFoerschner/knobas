- **`crates/knobas-source/src/**`, `knobas_core::payload` and the IPC schema — issue #712
  (2026-10-13): `KindPaths.head`, and `EntityPage.groups`, the Code tile's "what needs me".**
  Spec #701, workflow 1, under its delegation of 2026-10-13; #712 asks for this entry if a
  declaration or a row shape changes, and both do.

  **The declaration.** `KindPaths` gains `head: Vec<PayloadPath>` (`#[serde(default)]`, so an
  older peer's descriptor decodes as declaring none; `field::HEAD`, in `field::ALL`): the branch a
  pull request is opened from, by the name the branch's own record is titled with. Gitea declares
  it on `pr` at `head.ref`, which `map::branch_item`'s title (`branch.name`) matches. The contract
  battery weighs it as text like the other scalar paths. Mirrored in `app/src/lib/ipc/sources.ts`.
  No author path is declared: §4.1's normalized `author` is already a pull request's opener and a
  branch's head-commit author for Gitea, and the standup compares exactly that.

  **The row shape.** `EntityPage` gains `groups: Option<Vec<GroupTally>>`, `GroupTally { group:
  AttentionGroup, count }`, `AttentionGroup` one of `review_requested`, `my_pull_requests`,
  `open_pull_requests`, `my_branches`, `branches`, `other_branches` (snake case). `Some` only under
  `attention`, in a room where some drawn row's source has a configured username; the non-empty
  groups in draw order, the rows arriving group by group (so a page from offset 0 is cut by the
  counts). `EntityRow` is unchanged, so `knobas_search::EntityRow` stays its wire twin. Mirrored in
  `app/src/lib/ipc/entity.ts` as `GroupTally` and `AttentionGroup`.

  **The rule** (`LIVE_ATTENTION`, one statement, still unnamed). "Me" is the row's **own
  source's** `config.username`, trimmed, blank as none, compared case-sensitively as the inbox and standup
  compare (they union every enabled source's; a Gitea author is compared with the Gitea username
  only). After the settled rows are dropped: a `pr` whose declared reviewers include me (1), a
  `pr` I authored (2), any other `pr` (3), a `branch` I last committed to that no open `pr` of
  the same source names as `head` under the same declared parent path line (4), a `branch` whose
  source has no username (5, `branches`; ruled 2026-10-13 on #712, so a source without identity
  keeps its branches drawn in a mixed room), anything else (6). The repo is matched by the path
  line (Gitea's `owner/name`), not by the name set `sync.item_parent` keeps, whose bare `name` is
  shared across owners. A miss on either side (no `head`, no parent) matches nothing, so the
  branch stays unproposed and drawn (ADR-0007). A pull request whose state misses counts as
  open, as the tile draws it, so the branch it names is drawn as that pull request. A row whose
  source has no username is 3 or 5. A room where no row has one is not grouped
  (`groups` `None`, and the order is exactly #680's). The open pull requests are read across the
  live corpus, not the room, so a context holding a branch without its pull request does not call
  the branch unproposed. Whether I reviewed since the last push is not asked: the mirror holds
  neither reviews nor push times. `pr` and `branch` are knobas' own words (`0001_init.sql`), as
  the inbox's review rule already names `pr`. `LIVE_LATEST` draws `null` groups; `tile_page!`
  orders by group first, `nulls first`, which leaves every ungrouped read's order as it was.

  **What is not touched.** No `EntityOrder` member, no `EntityFilter` field, no command, no
  migration, no module.

  **The tile.** `Tile.svelte` draws each group under a muted `.lab` header with its count
  (`.tile-g`, `app.css`), drops a group with no row on the page, and draws `other_branches`
  only maximised; a tile those alone fill says how many it keeps back. The header reads *2 to
  review · 1 mine · 5 open · 3 branches*: `my_branches` and `branches` share the branches
  segment, and other branches are not counted.

  Pinned by: `tests/it/entity.rs`'s
  `the_code_tile_draws_what_needs_me_first_in_four_groups` and
  `a_code_tile_with_no_identity_draws_todays_order` and
  `a_source_without_identity_keeps_its_branches_drawn_in_a_mixed_room`; `entity_mirror.rs`'s
  `the_entity_page_shape_matches_its_typescript_mirror` and `sources_mirror.rs`'s `KindPaths`
  keys; `knobas-core`'s `the_field_names_the_sql_macros_use_are_the_serialized_ones`; six new
  tests in `Tile.test.svelte.ts`. Witnessed against the real server by `just gitea-live`'s
  `the_shapes_the_fake_only_assumes_are_certified_here`: the seeded *SEPA retry* pull request's
  declared head names exactly one mirrored branch of its repo, and every seeded branch has an
  author.

