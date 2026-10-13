- **`crates/knobas-source/src/**`, the IPC schema and what every row carries — issue #666
  (2026-10-12): `KindPaths` declares a state, and `EntityRow` carries its chip.** Spec #660
  stories 15–16: room and launcher rows for pull requests and builds show a coloured state chip,
  and a room scoped to one source stops repeating the source id. The spec names "the `state`
  declared path" among the frozen-surface changes Björn's delegation of 2026-10-12 signs off
  (#660, *Further Notes*); each still carries its entry, and this is that entry.

  **`Tone`, in `knobas_core::tone`.** `ok | warn | err | info | muted`, serialized lowercase.
  #661 adds the same five words for the source-view vocabulary and was not on `main` when this
  landed, so the ticket's instruction applied: its own enum, the same five words, recorded here.
  It lives in `knobas-core` rather than `knobas-source` because `KindPaths` is in core and core
  may not depend on the SPI (the reason #277 and #594 put the path types there).
  `knobas-source` re-exports it as `knobas_source::Tone`, beside `StatePath`, `StateValue` and
  `StateChip`, so #661's block vocabulary names `knobas_source::Tone` and there is one enum.
  The frontend's mirror is `Tone` in `app/src/lib/ipc/entity.ts`.

  **The field.** `KindPaths::state: Vec<StatePath>`, `#[serde(default)]` like every field beside
  it, so an older descriptor decodes as declaring no state. `payload::field::STATE = "state"`
  joins `field::ALL`, so `the_field_names_the_sql_macros_use_are_the_serialized_ones` covers it.

  ```rust
  pub struct StatePath {
      pub at: PayloadPath,             // leads to a string or a boolean
      #[serde(default)]
      pub values: Vec<StateValue>,     // the value -> tone map
  }
  pub struct StateValue {
      pub value: serde_json::Value,    // a JSON string ("open") or boolean (true); typed
      pub tone: Tone,
      #[serde(default)]
      pub text: Option<String>,        // what the chip says instead of the value ("merged")
  }
  pub struct StateChip { pub text: String, pub tone: Tone }   // the resolved chip
  ```

  **The rule.** Candidates in declaration order; a candidate's value is the trimmed, non-blank
  string or the boolean its `at` leads to, and it must be one its own `values` names, string for
  string and boolean for boolean, or the candidate misses and the next is tried. The chip's text
  is the entry's `text`, else the value. All candidates missing is `null`, ADR-0007's miss, and
  the failure direction is toward no chip, never a wrong one. **The value is typed** because a
  value map keyed by text would read a source writing the word `"true"` as a merged flag, the
  refusal `resolve_flag` makes. `payload::resolve_state` is the Rust side and
  `knobas_core::declared_state!` the SQL side; `crates/knobas-core/tests/it/payload_paths.rs`
  holds them to one answer (`a_state_chip_is_the_first_mapped_value_in_sql_and_in_rust`,
  `a_state_chip_misses_in_both_and_never_guesses`).

  **The declarations.** Gitea `pr`: `merged` (`true` → `merged`, ok), then `state` (`open` info,
  `closed` muted); the flag comes first because Gitea says `closed` of a merged pull request
  too. TeamCity `build`: `state` (`running`, `queued` → info), then `status` (`SUCCESS` ok,
  `FAILURE` err, `UNKNOWN` muted); a finished build's `state` is `finished`, which the first
  candidate does not map. No other kind declares one.

  **Contract battery clause 6 inspects `state`.** Clause 2's type check reads "a string or a
  boolean" for each candidate's `at`, clause 3 weighs the candidates in one slot, and clause 4
  is a kind declaring no state drawing no chip. One more, which clause 3 cannot see: a kind that
  declares a state, and that the corpus populates, must draw a chip on at least one item, so a
  value map naming nothing the source writes (`FAILED` for `FAILURE`) is caught. Pinned by
  `rejects_a_state_no_item_draws_a_chip_for` and `rejects_a_state_at_that_leads_to_an_object`;
  the conforming test adapter now declares a state, so `accepts_a_conforming_adapter` runs the
  clause's happy path.

  **The IPC touch: `EntityRow.state: StateChip | null`.** On both Rust `EntityRow`s
  (`knobas_app::commands::entity` and `knobas_search::types`, `#[serde(default)]` on the
  latter, as for `path`), pinned to one wire shape by
  `commands::search::tests::the_two_entity_rows_are_one_wire_shape`, and on the TypeScript
  `EntityRow` and `SearchHit`. No command, event, module or barrel moves, and no migration: the
  declaration is compiled-in data bound into the reads (#277's entry). The value is
  `declared_state!` expanded beside `path_line_read!` at every place a row is read: the four room
  statements and the detail statement in `commands/entity.rs`, the search statement's mirror
  corpus (`Corpus::state`; `null::jsonb` for notes, assets and routes), the board's newest rows
  and the built-in lists (`null::jsonb` for the estate lists). `sql::tests`' closed literal list
  gains the macro's keys (`state`, `values`, `value`, `text`, `tone`), its type comparand
  `boolean` and the empty path `{}`. `app/src/lib/ipc/sources.ts` gains `state` on `KindPaths`
  and `StatePath` / `StateValue` interfaces; `sources_mirror.rs` pins them, and
  `entity_mirror.rs` pins `StateChip` and the `Tone` union.

  **The shell.** `StateChip.svelte` draws the chip after the title in a room row
  (`EntityLine`) and a launcher row (`Row`, which the board and results both draw). A room whose
  `sources` filter names exactly one source (a source room, a project room) drops the source
  column (`.row.g4w`); *All work* and context rooms, whose filters name no source, keep it,
  and the row's provenance tooltip names the source in the column's place. The mini board draws
  tickets, which declare no state, and is unchanged.

  **Witness.** Real PostgreSQL: `knobas-app`'s
  `entity::a_rooms_lines_carry_the_state_their_adapter_declares` (merged `merged`/ok, open
  `open`/info, failed build `FAILURE`/err, a ticket `null`, through all four room statements and
  the detail) and `knobas-search`'s `tests/it/state_chip.rs` (search hit, board, built-in list).
  The real products: `just gitea-live` (`the_shapes_the_fake_only_assumes_are_certified_here`
  reads `merged`/ok, `open`/info and `closed`/muted off the seeded Gitea) and
  `just teamcity-live-seeded` (`a_synced_build_carries_its_state_chip`: build 1187
  `FAILURE`/err, 412 `SUCCESS`/ok, every seeded build draws one), both on 2026-10-12.

