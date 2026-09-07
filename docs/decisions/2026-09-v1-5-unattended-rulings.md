# Unattended v1.5 rulings — from 2026-09-07, Fable as deputy

Björn, 2026-09-07: *"let a fable subagent decide whenever there's a decision to be made since I won't be there."* The `/v15-next` loop dispatches one Fable subagent — the **deputy** — per fork, in the shape of the 2026-08-29 batch (`2026-08-29-unattended-batch-rulings.md`): each ruling is posted as a comment on its issue, beginning `Ruling (deputy for Björn, <date>):`, and the PR that acts on it appends the same four parts here, in the order ruled. The deputy first looks for an answer that already exists — the ticket, spec #491, `CONTEXT.md`, the ADRs, the decision records, the grilling's rulings — and quotes it; only an open fork gets a new ruling.

What counts as a fork, and what does not, is the loop's *Decisions in Björn's absence* section. A ruling here binds the milestone; Björn reverses one with a follow-up ticket, and a ruling that settles something architectural gets its ADR through the implementer, per this directory's README.

---

## #493 — what "a row carrying a dropped type reads as `custom`" means in code

Ruled 2026-09-07. Comment:
<https://github.com/BFoerschner/knobas/issues/493#issuecomment-5575724964>

**The fork:** criterion 3 says an asset row whose type is `scenario` "reads as `custom` through the
asset read and the Tree"; spec #491 says the same in story 4 and in its Implementation Decisions.
But `row_of` and `unmonitored_assets` in `crates/knobas-app/src/assets/mod.rs` already have a
deliberate rule for a type the table has lost — `type_label` is the raw id and `monogram` is
`"??"`, documented in code, on the wire in `app/src/lib/ipc/assets.ts`, and asserted in five
frontend tests. Option A: take the words literally and fall back to `asset::find("custom")`, so a
`scenario` row draws "Custom" / `CU` and the `??` path dies. Option B: read the sentence as the
property-level statement it is, change no code, and meet the criterion with a test that inserts a
`scenario` row and reads it back through the asset read and the Tree.

**Ruling:** option B. No read-path code changes. The `??` rule stands as it is, and a row carrying
`scenario`, `step` or `connector` after this PR is exactly what #439's rule was built for: a row
naming a type this build does not carry. The criterion is discharged by a test in
`knobas_app::assets` that inserts a row with `type_id = 'scenario'` directly (the create door
refuses it, which the test may also assert) and reads it back, asserting all of: the row comes back
from the asset read; `type_id` is `"scenario"`, `type_label` is `"scenario"`, `monogram` is `"??"`;
`properties_of` returns every stored key as `custom: true` and no declared property; `assets::edit`
accepts a name edit and a property edit of any kind on it, refusing nothing on the type's account.
The Tree half of the criterion is the existing `??` assertions in `tree.test.ts`,
`AssetsView.test.svelte.ts` and `editing.test.ts` — the four tests that used `flowrun-scenario` as
their unknown type should use a word that is not one of the three dropped ids, so that "an id this
build never had" and "an id this build dropped" stay two named cases, the second of them the new
test. Two pieces of prose follow from this, both inside the ticket's existing scope: the new test's
doc comment states the rule in the words below, and the spec §12.1 edit the ticket already requires
gains one sentence: *a row whose type the table no longer carries is kept, drawn with the `??`
chip, and edits like a custom asset; the create dialog does not offer the type.* No glossary entry
and no ADR: this restates a rule the contract already records, it does not decide a new one. The PR
body names criterion 3 as read here and links this comment.

**Reasoning:** the answer already exists in the contract and the ruling is to quote it, not
re-decide it. The §10.8 entry for #428 in `docs/contract.md` says: "`type_id` is open text because
the interesting half of a type cannot be written in SQL … `assets::create` is the one door and
refuses a type it does not know." Nothing in the contract says a lost type is *relabelled*; the
pinned `AssetRow` and `UnmonitoredAsset` shapes carry `type_label` and `monogram` as fields, and
the value for a lost type is the code's rule — "A type the table has lost is drawn as `??` rather
than refused: the row exists, a reader has to be able to see it and move it" (`row_of`, since
83aba352), mirrored on the wire as "`\"??\"` for a type this build lost". Spec #491's sentence has
a job, and it is the migration question: "the type id is not a database constraint, so a row
carrying a dropped type reads as `custom` at read time and no migration is needed"; story 4's *so
that* clause is about the create dialog ("so that the create dialog offers only types some real
thing has"), not about the chip. The grilling memory records the same ruling in the same breath:
"dropped types read as `custom`, no migration." What "reads as custom" is true of, in the code as
it stands, is the property model: `properties_of` gives a type `find` does not know no declared
keys, so every stored key comes back `custom: true`, and `vet_against_schema` returns `Ok(())` for
a type with no schema — a dropped-type row already carries only custom properties and is edited
under custom's rules. That is the reading that keeps the frozen surface smallest: spec #491's
stream map lists Stream 1's frozen-surface touches as "none (the type table is code, not a
constraint)", and option A would change the observable answer of `list_assets`, `get_asset` and
`unmonitored_assets` — the asset IPC module the roadmap names as an M4.0 frozen-surface touch — for
every row the table loses, which is a §10.8 entry and Björn's gate, not a paperwork stream's. It
also keeps the witness real: with B, a `scenario` row from a pre-v1.5 database or an old estate
file is still identifiable as what it was, which is the property #439 bought and the reason a
reader can find and retype it; with A the row would silently claim to be a type somebody chose. The
implementer's brief lesson applies as stated — a criterion that would quietly undo an unmentioned,
documented, five-times-tested design is loose prose, and neither the ticket nor the spec mentions
the `??` rule once.

**If you disagree, the cost of reversing this is:** low in code, higher in paperwork. Reversing to A
is the four lines the implementer counted in `row_of` and `unmonitored_assets`, the wire comment in
`assets.ts`, the five frontend tests, and the new test's assertions — plus what A owes that B does
not: a §10.8 entry for a behaviour change on the asset IPC module, a contract line saying a lost
type reads as `custom` rather than `??`, and Björn's gate on the frozen touch. The one sentence
added to spec §12.1 is rewritten, not deleted. Nothing ruled here moves a wire value or a
migration, so there is nothing to unwind in a database.
