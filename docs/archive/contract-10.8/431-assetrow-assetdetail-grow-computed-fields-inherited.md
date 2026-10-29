- **`AssetRow` and `AssetDetail` grow the computed fields, issue #431 (2026-09-06):** inherited
  environment and owner with their source, effective health, and the "N problems inside" count.
  Ratified in advance by the spec (#427) Björn approved — "an asset and route read with computed
  effective environment, owner, health and reachable-via" — and by the entry above, which named
  #431 as the ticket that adds them. Written with the implementing PR, per #428's own pattern.

  **No migration, and `0018` is still the next free number.** Nothing here is stored. Both
  computations are answers about a *path*, and a stored copy of either would be a second writer of
  `parent_id` — the column ADR-0014 made the whole tree — that could disagree with it the first
  time a move failed halfway. **What the four writers write is unchanged**; what changed for them
  is the row they answer *with*, because `assets::one` now goes through `rows_of` like every other
  read, so a created or edited asset comes back carrying its rollup rather than three fields the
  caller would have to re-read for. `a_move_carries_the_inherited_value_and_the_rollup_with_it` is
  the witness that a re-parenting moves both answers with no edit to either — which is what
  "computed at read time" has to mean if it means anything.

  **Three fields on `AssetRow`, two on `AssetDetail`, and one new shape.**

  ```rust
  pub struct AssetRow { /* … as #428 froze it … */
      pub health: AssetStatus,      // worst of `status` and every descendant's
      pub inside: AssetStatus,      // worst status strictly underneath
      pub problems_inside: i64,     // descendants carrying warn or down
  }

  pub struct Inherited<T> { pub value: T, pub source_id: String, pub source_name: String }

  pub struct AssetDetail { /* … */
      pub effective_environment: Option<Inherited<Environment>>,
      pub effective_owner: Option<Inherited<String>>,
  }
  ```

  Mirrored in `app/src/lib/ipc/assets.ts` as the same names, with `Inherited<T>` **generic** there
  — one interface carrying an `Environment` in one field and a `string` in the other, rather than
  two concrete copies, so a third inherited field later grows nothing. `commands::assets`' mirror
  test finds it by the header text `export interface Inherited<T> {`, which is what
  `mirror::assert_shape` matches on, and asserts the field list in both directions like every
  other shape on this surface. **No existing field changes meaning**: `AssetRow.status`,
  `AssetRow.environment` and `AssetRow.owner` are still the values set *on that asset*, and the new
  fields sit beside them rather than replacing them. What the **pane** draws of each is the
  effective value, under a new *In force* group; the stored value is what the *note* beside it
  reports on (*set here* against *inherited from `hel1`*), and editing either is #429's surface.

  **`Inherited` carries a whole source, not an `inherited: bool`.** Story 10 is *"see whether a
  shown environment or owner is set here or inherited from which ancestor, so that I know where to
  change it"* — a boolean answers the first half and leaves the reader hunting up the path for the
  second. `source_id` equal to the asset's own id is *set here*; the pane compares rather than
  being told the same fact twice, and `tree.ts`'s `sourceOf` is the one place that comparison
  lives.

  **`inside` is a third field and not something the badge derives from `health`.** They answer
  different questions the moment an asset is worse than what it holds: a `down` VM holding one
  `warn` container has `health = down` and `inside = warn`, and story 32's badge counts and colours
  what is *inside*, so it is amber on a red row. Pinned in both languages — `assets_ipc.rs`'s
  `a_column_row_reports_its_effective_health_and_what_is_wrong_inside` and `tree.test.ts`'s badge
  test both carry that row, and a badge coloured from `health` fails both.

  **One statement, `assets::ROLLUP`, run once per read.** It is a `with recursive` seeded from the
  ids the read is about to answer with, and it is a *whole literal* rather than a fragment spliced
  into the three column statements `CHILDREN`/`ANCESTORS`/`ONE` — which is what that module's
  "three statements rather than one spliced constant" rule is for, and which keeps spec #427's
  ordering (**down over warn over up over none**) in one place instead of three. The price is one
  extra round trip **per statement**: one for a column, three for the pane's read, which already
  issues three (the asset, its ancestors, and what it holds) and now pairs each with a rollup.
  `AssetStatus::severity` is that ordering in Rust, smaller-is-worse so
  that "the worst of a set" is a plain `min`, and `the_rollup_ranks_the_statuses_the_way_rust_does`
  reads the `case` arms back out of the SQL string so the two cannot drift. **No depth cap**, for
  `move_to`'s reason: `parent_id` has one writer and it refuses cycles before it writes, with
  `asset_no_self_parent_chk` under it.

  **The `none` ordering is read literally and asserted.** `up` is *worse* than `none`, so an asset
  nobody has rated reads as `up` when something under it is up — the pane prints that as
  *"Health up · own status none"*. That is spec #427's list read as written and it is what makes a
  branch of healthy things read as healthy, but it is the one consequence of the ordering a reader
  might not have pictured, so it is pinned twice rather than left to follow: in Rust by
  `the_rollup_ranks_the_statuses_the_way_rust_does` (`Up.severity() < None.severity()`, and the
  same four `case` arms read back out of the SQL string), and over a real database by the *spare*
  branch at the foot of `a_column_row_reports_its_effective_health_and_what_is_wrong_inside` — an
  unrated site holding one `up` VM, which is the shallowest fixture that can tell the two orderings
  apart, since a leaf's own `up` reads `up` under either.

  **What did not change.** No command, no argument, no event, no settings key, no migration, no
  `Kind`, no reserved namespace. `crates/knobas-source/**`, `crates/knobas-http/**` and
  `crates/knobas-app/src/{error,profile}.rs` are absent from the diff. `knobas_search::corpus`
  is untouched — the rollup is not searchable and nothing indexes it. The launcher, the room tiles
  and the backup export need no change: the export dumps the whole `knobas` schema and there is no
  new column in it. Monitors are the half of story 37 still missing from `health`'s *own* term, and
  they are M4.1's; `AssetsView`'s module header says so in place.

  Ratified by the orchestrator as spec #427 and issue #431, whose acceptance criteria specify the
  inherited reads, the rollup, the badge and the tests. **Björn keeps the gate for frozen contracts
  and this entry is flagged for his review.**

