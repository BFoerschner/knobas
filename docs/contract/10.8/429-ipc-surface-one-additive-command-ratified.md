- **The IPC surface, issue #429 (2026-09-06): one additive command on the ratified `assets`
  module pair — `asset_types` — with `knobas_core::asset::AssetType` and its `TypedProperty`
  becoming wire shapes and gaining one field each side of the bridge, and one line on the Rust
  handler barrel.**

  ```rust
  #[tauri::command] pub fn asset_types() -> Vec<knobas_core::asset::AssetType>;
  ```

  Mirrored in `app/src/lib/ipc/assets.ts` as `interface AssetType` and `interface
  TypedProperty`, pinned by `assert_shape` in `commands/assets.rs` beside the six #428 added.

  **The type table grows its third column.** `AssetType` gains `suggests: &'static [&'static
  str]` — the child types conventionally suggested under one, which spec #427's *Types*
  paragraph named (*"id, monogram, display name, ordered typed-property schema, and the child
  types conventionally suggested"*) and #428 left out because a table of suggestions with no
  reader is untested prose. The reader is this ticket's create dialog. The lists are design
  §12.1's two chains plus the three pairs the **real** estate holds that the chains do not draw
  (a site under a site, a VM directly under a site, a database inside a container), and
  `knobas-core`'s `tests/estate_file.rs` asserts that every parent-and-child pair
  `testenv/hetzner/estate.json` actually uses is one the table suggests — so the conventions
  answer to an estate that exists (ADR-0013) rather than to a diagram. **A suggestion is never a
  constraint**: nothing enforces it, `assets::create` still takes any declared type under any
  parent, and the dialog offers all nineteen whatever is listed. *Superseded in part by the #493
  amendment in §9 above (2026-09-11): the dialog offers sixteen, `scenario`, `step` and `connector`
  having left the table in v1.5; the sentence stands as history.*

  **Why the table crosses the bridge rather than being copied.** Two things the frontend cannot
  answer on its own, and neither is a matter of taste. The first is *which types are usual here*.
  The second is **which kind an unfilled typed property takes**: `AssetProperty` hands the pane a
  declared key with `value: null`, there is no kind in a `null`, and `assets::edit` refuses a
  typed key given the wrong kind by name — so an editor without the schema would be guessing
  whether a service's `port` wants `8080` or `"8080"`. A nineteen-entry copy in TypeScript would
  also be the half that goes stale, and the ids travel in the share export. *Superseded in part by
  the #493 amendment in §9 above (2026-09-11): a copy in TypeScript would now be sixteen entries;
  the argument is untouched and the sentence stands as history.*

  **It takes no state, and answers without a pool.** The one command in this pair that does:
  the table is a `const`, so the answer is the same before bring-up as after it and a create
  dialog works on a cold start. It also answers **without a `Result`**, which `app::ping` and
  `app_status` already do — there is no read to fail and no argument to refuse, and a `Result`
  that is always `Ok` is an error branch every caller writes and no test can reach. That is why
  `tests/assets_ipc.rs`'s registration loop, whose marker is `not_ready`, cannot cover it and
  `the_type_table_answers_with_no_pool_and_carries_the_schema_and_the_suggestions` stands in its
  place, asserting the nineteen, the schema *in its declared order with its kinds*, and the
  suggestions. *Superseded in part by the #493 amendment in §9 above (2026-09-11): the assertion
  reads `16` since v1.5 dropped `scenario`, `step` and `connector`; the sentence stands as history.*

  **Which barrels were appended**: one line at the foot of `crates/knobas-app/src/lib.rs`'s
  `generate_handler!` list, after #428's group of six. `app/src/lib/ipc/index.ts` needs none —
  `export * from "./assets";` has been at its foot since #428. Neither barrel is rewritten.

  **What did not change.** No existing command, DTO field or event name changes meaning; the six
  #428 commands are called as they were, and the writes this ticket puts on the Tree are those
  six being *called*, not altered. No migration — the estate's table is `0017` and `0018` is
  still the next free number. No new event: a mutation writes an activity line and the shims
  announce it on the existing `activity:new`, which is #428's ratified arrangement and is what
  the pane's *re-read after every write* rides on. Nothing under `crates/knobas-source/src/**`,
  `crates/knobas-http/**` or `crates/knobas-app/src/{error,profile}.rs`; no settings key; no
  navigation contract growth — `#/assets/tree` and `#/asset/<id>` are #428's and the dialogs are
  not addresses.

  Ratified by the orchestrator as spec #427 and issue #429. Unlike #428's, **#429's acceptance
  criteria do not ask for this entry** -- they say "IPC tests already cover the commands", which
  reads as *no new command*. The entry is owed by the standing rule at the head of this section
  rather than by the ticket, and it is written because the ticket's own criteria cannot be met
  without the table on the wire: a create dialog needs `suggests` for *usual here*, and a pane
  editing a declared-but-unfilled property needs the kind, which no `AssetProperty` carries.
  **Björn keeps the gate for frozen contracts and this entry is flagged for his review.**

