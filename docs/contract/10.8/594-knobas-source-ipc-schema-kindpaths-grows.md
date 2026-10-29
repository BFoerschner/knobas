- **`crates/knobas-source/src/**` and the IPC schema, issue #594 (2026-10-11): `KindPaths` grows
  a declared parent.** Spec #591 and ADR-0018 make an item's **parent** (`CONTEXT.md`) a fact the
  adapter declares per kind, as a declared path, the way it declares a status or a project. The
  spec names this change as the one frozen-contract touch of the `under:` work, and #594 asks for
  this entry by name. **Björn signed it off on #594 on 2026-10-11**
  ([comment](https://github.com/BFoerschner/knobas/issues/594#issuecomment-6112829636)):
  *"merge already gets my sign off, it's fine"*.

  **The field.** `KindPaths::parent: Vec<ParentPath>`, `#[serde(default)]` like every field
  beside it, so a descriptor serialized by an older peer decodes as declaring no parent, and a
  kind that declares none misses. `ParentPath` is new, in `knobas_core::payload` beside
  `ListPath` and for the same reason #277 put the others there (the readers are in core, which
  may not depend on the SPI), and re-exported from `knobas-source` with the other three:

  ```rust
  pub struct ParentPath {
      pub kind: String,             // the parent's entity kind: repo, build_config, page, ticket
      pub at: PayloadPath,          // one object (the direct parent), or an array of objects
                                    // (the chain, outermost first, direct parent last)
      #[serde(default)]
      pub names: Vec<PayloadPath>,  // inside each parent object, the names it answers to;
                                    // the first is the one a path line shows
  }
  ```

  The ticket names two parts: where the parent lives, and the names it answers to. Those are
  `at` and `names`. **`kind` is a third part**, and it is there because the spec needs it from
  the declaration: a parent need not be in the mirror (ADR-0018's first consequence), yet #601's
  `under:` suggestions carry their kind (`payout-service · repo`) for parents "named by children
  but not mirrored". Only the child's declaration can say what such a parent is. Adding it now
  saves a second §10.8 touch later. The `kind` part and the two-shape `at` were ratified as within
  #591's sanctioned change by the deputy's ruling of 2026-10-11 on #594
  (`docs/archive/2026-10-unattended-rulings.md`); flagged for Björn's review. **`at` takes two
  shapes** for the same reason: the spec takes Confluence's transitive chain "from the existing
  `ancestors` payload (#284)", an array, while Gitea, TeamCity and Jira name one object. Both are
  said in one field rather than two, and an `at` that resolves to neither shape is a miss. The
  ruling's condition on the two shapes: the branch on shape lives in exactly one SQL macro and one
  Rust resolver in `knobas_core::payload`, held together by the agreement test, and no reader
  spells it inline. Like every other field it is a **candidate list**, the first whose `at`
  resolves winning.

  **The field-name pin.** `payload::field::PARENT = "parent"` joins `field::ALL`, so
  `the_field_names_the_sql_macros_use_are_the_serialized_ones` covers it: a rename now turns that
  test red instead of turning every `under:` read into a silent miss.

  **No adapter declares a parent yet, and behaviour is unchanged.** Every `KindPaths` literal in
  the tree already ends in `..KindPaths::default()` or names its kind's fields through a
  constructor that does, so nothing else needed editing. Each adapter's declaration is its own
  ticket (#595 Gitea, #597 TeamCity, #598 Jira, #599 Confluence), and the `under:` filter, its SQL
  and the path line are #595 and #596.

  **What this does not do, so nobody reads the omission as covered.** No SQL macro reads the
  field and no Rust resolver walks it. The first reader is #595's, and it should land both halves
  under the agreement test `crates/knobas-core/tests/it/payload_paths.rs` already holds the
  others to. The **contract battery's clause 6 does not inspect `parent`** either: with no
  adapter declaring one, no corpus here could witness the check. #595, the first adapter to
  declare a parent, owes the clause's type and misspelling checks for it (`at` leads to an object
  or an array of objects, each `names` path to a string). `ancestor_path_read!`'s doc still says
  `KindPaths` has no slot shaped like an ancestor path. That read expires into this field when
  #596 and #599 move the path line onto it, and its doc changes with them. *Superseded in part by
  the note on #596's entry below (2026-10-12): after #599 the read is still the path line's
  fallback, and its retirement is a ticket of its own.*
  *Expired, issue #646 (2026-10-12): that ticket retired the read. `ancestor_path_read!` and
  its doc are gone, and the path line is the declared parent chain alone (see #596's entry).*

  **The IPC touch.** No command, no event, no module. `payload_paths` already rides inside the
  `SourceDescriptor` that `list_adapters` returns, so `app/src/lib/ipc/sources.ts` gains the
  `parent` key on `KindPaths` and a `ParentPath` interface beside `ListPath`. The barrel is
  untouched. `crates/knobas-app/tests/it/sources_mirror.rs` pins both: `KindPaths`' key set gains
  `parent`, and `the_parent_path_shape_matches_its_typescript_mirror` pins `ParentPath`'s keys,
  read off the type's own serialization because no template carries one yet.

  **No migration.** A declaration is compiled-in data handed to a read (#277's entry), not a
  stored row.

  **Tests:** `knobas-core`'s
  `a_descriptor_from_before_parents_decodes_as_declaring_no_parent` (a `KindPaths` with no
  `parent` key decodes, with an empty parent, equal to the default for its kind) and
  `a_declared_parent_travels_as_kind_at_and_names` (the wire spelling, both directions);
  `the_field_names_the_sql_macros_use_are_the_serialized_ones`, now covering `parent`; and the two
  mirror pins above.

