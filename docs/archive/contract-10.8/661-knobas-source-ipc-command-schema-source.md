- **`crates/knobas-source/src/**` and the IPC command schema — issue #661 (2026-10-12): the source
  view, one trait method, a frozen block vocabulary, a shared diff parser, battery clause 9 and one
  IPC command.** ADR-0020 is the argument; spec #660 names each surface, and Björn's delegation of
  2026-10-12 recorded there signs them off (*"You can also introduce own views per source"*). This
  is the list of what froze.

  **`crates/knobas-source/src/**` — four additions, all additive:**

  1. `Source::view(&self, entity: &str, section: Option<&str>) -> Result<Option<view::SourceView>,
     SourceError>`, **with no default implementation** (ADR-0006's forcing function, as for
     `reachable_transitions`). `entity` is an `EntityRef` in string form. `section: None` reads the
     whole view, a deferred section as its `id`, `title` and `count` with no blocks; `Some(id)`
     reads that one section, and the answer is a view holding exactly it, not deferred; an id the
     kind's view lacks is `SourceError::Protocol`. `Ok(None)` is *this adapter draws nothing for
     this kind*; a failed read is an error and never `None`. Every implementation in the tree
     answers it: the six adapters (Gitea draws `commit`; Jira, Confluence, Kuma, TeamCity and the
     mock answer `Ok(None)` for every kind), `knobas_sync::progress::Observed` (delegating, as for
     `reachable_transitions`), the battery's `TestSource`, and twenty-six test fakes (`Ok(None)`).
  2. **`knobas_source::view`, the block vocabulary.** Plain serde data; every string is source
     text, interpolated and never rendered (ADR-0020 rule 5). Field for field:
     - `SourceView { sections: Vec<ViewSection> }`
     - `ViewSection { id: String, title: String, count: Option<u64>, deferred: bool, blocks:
       Vec<Block> }` — `id` unique within one view.
     - `Block`, internally tagged on `type` in snake_case: `Facts { facts: Vec<Fact> }`,
       `Text { text: String, markdown: bool }`, `Entities { title: Option<String>, rows:
       Vec<ViewRow> }`, `Diff { files: Vec<DiffFile>, truncated: bool }`, `Log { text: String,
       truncated: bool }`, `Settings { title: Option<String>, entity: String, write_op: String,
       values: Vec<Setting> }`.
     - `Fact { label: String, value: String, tone: Option<Tone> }`. `Tone` is
       `knobas_core::tone::Tone` (`ok | warn | err | info | muted`, lowercase), re-exported as
       `knobas_source::Tone` and from `view`: one enum, shared with #666's state chip, whose
       entry above (PR #671) added it; this entry adds no second one.
     - `ViewRow { entity_id: Option<String>, title: String, meta: Option<String>, tone:
       Option<Tone>, chip: Option<String>, url: Option<String> }` — named `ViewRow` and not the
       spec's `EntityRef`, which is `knobas_core`'s type.
     - `Setting { name: String, value: String, inherited: bool, secret: bool, editable: bool,
       description: Option<String> }` — a `secret` one's `value` is empty; the adapter never sends
       it.
     Adding a block, a field or a tone is a §10.8 entry, as a `WriteOp` variant is.
  3. **`knobas_source::diff`, the unified-diff parser every git adapter shares**, and its types
     (re-exported from `view`): `DiffFile { path: String, old_path: Option<String>, change:
     DiffChange, additions: u32, deletions: u32, hunks: Vec<Hunk>, truncated: bool }`,
     `DiffChange = added | modified | deleted | renamed | binary`, `Hunk { header: String, lines:
     Vec<DiffLine> }`, `DiffLine { kind: DiffLineKind, old: Option<u32>, new: Option<u32>, text:
     String }`, `DiffLineKind = context | add | del`. `parse(&str) -> Vec<DiffFile>` and
     `parse_view(&str) -> (Vec<DiffFile>, bool)` apply the caps, `MAX_LINES_PER_FILE = 1_500` and `MAX_LINES_PER_VIEW =
     6_000`: a file over either keeps the lines before the cut, its **full** counts and
     `truncated: true`; `parse_view`'s `bool`, which is `Block::Diff.truncated`, says the
     per-view cap cut something. Hunk lines are
     read by the counts the `@@` header gives, so `--- comment` inside a hunk is a line;
     `\ No newline at end of file` is no line; git's quoted paths are unquoted.
  4. `contract::battery` **clause 9**: for every item the full sync emitted, `view(id, None)` must
     answer `Ok(_)`; a drawn view's section ids are non-blank and unique; and each deferred
     section's own read answers exactly that section, not deferred. Run against a live backend this
     makes one read per emitted entity, and no write.

  **The IPC schema — one new command, the vocabulary as its answer:**

  * `commands::entity::entity_view(entity_id: String, section: Option<String>) ->
    Option<SourceView>`, routed like `reachable_transitions` (no `source_id`: the namespace is the
    source). `sources::crud::view` builds the adapter through the same `adapter_for` that
    `reachable_transitions` now shares, and does one thing to the answer: an `Entities` row keeps
    its `entity_id` only where `sync.item` holds that entity (ADR-0020 rule 7), so a row the mirror
    lacks is plain text. `crates/knobas-app/src/lib.rs`'s handler list gains the command.
  * `app/src/lib/ipc/entity.ts` gains `entityView(entityId, section = null)` and the TypeScript
    mirror of the vocabulary: `SourceView`, `ViewSection`, `ViewBlock` (Rust's `Block`, renamed on
    this side because the barrel already exports `time.ts`'s `Block`), `Fact`, `Tone`, `ViewRow`,
    `Setting`, `DiffFile`, `DiffChange`, `Hunk`, `DiffLine`. `app/src/lib/ipc/index.ts` is
    unchanged (`./entity` is already re-exported).
  * **No event, no migration.** A view is live and never stored (rule 1); nothing in Postgres, the
    keychain envelope, `knobas-http` or `crates/knobas-app/src/{error,profile}.rs` changes.

  Pinned by: `knobas_source`'s `view::tests::every_block_round_trips` and
  `the_wire_shape_is_a_union_on_type`; the `diff::tests` table over real `git diff` output (modify,
  add, delete, rename with and without edits, binary, binary added, no newline at EOF, function
  context, a header look-alike, a quoted path) and both caps from both sides; the four clause-9
  battery cases (`accepts_an_adapter_that_draws_no_view`,
  `rejects_an_adapter_whose_view_of_its_own_entity_fails`,
  `rejects_a_view_whose_section_ids_repeat`,
  `rejects_a_deferred_section_that_does_not_answer_its_own_read`) and every adapter's own
  `passes_the_contract_battery`; `knobas-source-gitea`'s `tests/it/view.rs`; `knobas-app`'s
  `tests/it/entity_view.rs`; `wiring.rs`'s `every_command_is_in_the_handler_list`; and, the
  witness ADR-0013 asks for, `live_gitea.rs`'s
  `a_seeded_commits_view_carries_the_diff_the_seed_pushed` and the live battery's clause 9 against
  the real Gitea.

