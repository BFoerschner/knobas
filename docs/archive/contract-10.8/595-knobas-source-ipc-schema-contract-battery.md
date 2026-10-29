- **`crates/knobas-source/src/**` and the IPC schema, issue #595 (2026-10-11): contract battery
  clause 6 inspects `parent`, and `FilterDimension` gains `under`.** The tracer bullet of spec #591:
  Gitea declares the repo as the parent of `branch`, `pr` and `commit`, and the launcher scopes by it
  with `under:<name>`. #594's entry above said this ticket owes clause 6's checks for `parent` and the
  first reader of the field. **Ratified by the deputy's ruling of 2026-10-11 on #594**
  ([comment](https://github.com/BFoerschner/knobas/issues/594#issuecomment-6113150556), *"#595 owes its own §10.8
  entry, ratified by this ruling and flagged for Björn's review, as #498's entry was for clause 8"*);
  **flagged for Björn's review. Björn's sign-off on #594 does not cover it.**

  **`crates/knobas-source/src/contract.rs` — clause 6 extended to `parent`, by the rules it already
  applies to every other declared field.** A change to what a battery clause asserts, which #146's
  ruling puts at Björn's gate:

  1. **A declared parent's `kind` is one of the adapter's own `entity_kinds`**, the check the ruling
     added. An `under:` suggestion (#601) says what a parent is from this word alone, since the
     parent need not be mirrored (ADR-0018), so a kind the adapter does not emit is a label nothing
     can open. Checked on the descriptor, like clause 6's first rule.
  2. **`at` is not the empty path**, which addresses the record itself: an item is never its own
     parent. Also on the descriptor.
  3. **Type check:** where `at` leads anywhere on an item, it leads to an object or an array of
     objects, and each `names` path inside a parent object leads to a string. Anything else fails,
     naming the item and its payload.
  4. **Misspelling check:** where nothing of a kind resolved, no record names a key it does not have,
     asked of the corpus as for every field. `at` and the names get one evidence slot each, for the
     reason `reviewers`' array and entry do: a parent that is found must not excuse a misspelled name
     inside it.
  5. **Clause 4's counterpart:** a kind that declares no parent resolves none.
  6. **A parent kind's own records answer to the names declared for it**, added by the deputy's
     second ruling of 2026-10-11 on #595 (below). For each declared parent `kind`, on that kind's own
     records in the corpus, every `names` path read from the record's root leads to a string (type
     check), and where none resolved, no record names a key it does not have (misspelling check).
     It has an evidence slot per parent kind of its own, so the children's parent objects resolving a
     name cannot excuse the parent's records lacking it.
     Like a field's candidates, a parent's names share one evidence slot, on the children's parent
     objects and on the parent's own records alike. So a name no record carries is accepted whenever
     another name of the same parent resolves, even though `under:` can never reach the parent by
     that name. Each adapter's live suite is what witnesses that every declared name resolves
     (deputy's ruling of 2026-10-12 on #595).

  The two #301 tolerances hold for `parent` unchanged, rule 6 included: a kind the corpus never
  populates is asked nothing (a Jira epic outside the JQL), and a field's candidates share one
  evidence slot (a parent kind's names share theirs). Nothing declared a parent before this
  ticket, so the extension changes no existing adapter's certificate. Witnessed by seven rejecting
  cases in `contract.rs`'s tests, one per rule that can be broken by a declaration or a record (a
  kind it does not emit, `at` onto a string, `at` onto an array of strings, `at` at a key the records
  lack, a name key the parent objects lack, a parent kind's records lacking the name their children
  give them, and that name not being a string there), and by the conforming adapter, which now
  declares a parent its corpus resolves, on the children and on the parent's own records. Against
  the real container, `passes_the_contract_battery_against_the_real_container` in
  `just gitea-live` runs the extended clause on Gitea's own records, a pull request's `base.repo`
  among them.

  **The read, and the ruling's condition on it.** The two shapes of `ParentPath.at` are told apart
  in exactly one SQL macro, `knobas_core::declared_parents!`, and one Rust resolver,
  `knobas_core::payload::resolve_parents`. `declared_parent_names!` is built on the macro and is what
  every `under:` statement reads; no reader spells the branch inline.
  `crates/knobas-core/tests/it/payload_paths.rs` holds the two to one answer across both shapes, the
  misses (an `at` onto a string or onto nothing, the empty path), an empty array (no ancestors, an
  answer and not a miss), and candidate order (the first `at` that resolves wins). A **second read
  pair** sits beside them and is held by the same test: `declared_names_as_parent!` and
  `resolve_names_as_parent`, the names a *mirrored* parent answers to, which `under:` needs to
  return the parent itself. It reads the children's declared `names` off the parent's own record,
  from the payload's root, which holds because a source names a parent in the parent's own record
  shape (a pull request's `base.repo` is a repository, a Confluence ancestor a page, a Jira
  `fields.parent` an issue). It does not walk `at`, so the shape branch is still in one place.
  `knobas-core` is not on the frozen list; this paragraph is here because the ruling's condition is.
  The pair stands by the deputy's second ruling of 2026-10-11 on #595
  ([comment](https://github.com/BFoerschner/knobas/issues/595#issuecomment-6113509745);
  `docs/archive/2026-10-unattended-rulings.md`), which reads #594's "every `under:` read goes
  through these two" as every read of a child's parent objects. It adds no descriptor field, but it
  puts one obligation on the already-frozen `ParentPath.names`: the names must also resolve on the
  parent kind's own records. The ruling's condition is that the battery certifies that obligation,
  which is clause 6's rule 6 above. **Flagged for Björn's review** with the rest of this entry.

  **The IPC schema — one enum variant, no new command.** `knobas_search::FilterDimension` gains
  `Under` (`"under"`), and the union in `app/src/lib/ipc/search.ts` gains the member. Spec #591
  says `under:` "takes part in coverage the way `author:` does: a source with no kind declaring a
  parent is reported as having nothing the filter can match", and #141's entry above built the report
  for exactly this: *"A second dimension appends an entry and a variant; nothing existing moves."*
  The spec calls the descriptor change "the one frozen-contract touch", and this variant is a second
  one the spec's own coverage requirement brings with it. Ratified by the deputy's ruling of
  2026-10-11 on #595 as required by that ticket's coverage criterion
  (`docs/archive/2026-10-unattended-rulings.md`); flagged for Björn's review.
  The answer is read off the declaration, not measured on the corpus: `answered` for a source
  declaring a parent for any kind, `no_values` otherwise, and the same "which sources put rows in
  this query's corpus" probe decides who is listed, so one round trip serves both dimensions.
  *(Replaced 2026-10-12 by Björn's ruling on #642: the answer is measured on the corpus. See the
  #642 entry below; who is listed is unchanged.)*
  `the_response_shape_matches_its_typescript_mirror` now reads the union's own line for both
  members, since `"under"` appears in the mirror's comments. The launcher draws the gap
  (`underGaps`, `Results.svelte`) the way it draws the author one.

  **What is not frozen and changed with it, so nobody hunts for an entry.** `SearchFilters` is
  untouched: `under:` has no chip, lives in the text, and a saved list stores it as text, so it rides
  on `Parsed` and `EffectiveFilters`, neither of them on the wire. `Searcher::with_payload_paths`
  hands the compiled-in templates to the search, and `Vocabulary::declaring` pairs them with the
  sources it already read, so `under:` costs no extra round trip to find its declarations. The
  unknown-token check is one statement, only for a query that used `under:`. Gitea's stored
  `branch` and `commit` payloads gain a `repository` object, `{ "full_name", "name" }` (spelled as
  `base.repo` is, full name first), and the branch tombstone carries the same object rather than the
  bare string it carried. That is ADR-0018's sanctioned departure from §3a's verbatim payload, and
  `crates/knobas-source-gitea/**` is not frozen.

  **No migration**, no command, no event, neither barrel.

  **Not done here, by ticket.** The path line is #596, the other adapters' parents #597 to #599,
  assets #600 (a corpus with no declared parent excludes itself from an `under:` query until then,
  as a corpus with no author does from `author:`), and suggestions #601.

