# Unattended rulings — from 2026-10-11, Opus as deputy

The `/v15-next` loop now runs over every `ready-for-agent` issue, not one milestone, and its deputy is Opus. The shape is the 2026-08-29 batch's (`2026-08-29-unattended-batch-rulings.md`) and the v1.5 milestone's (`2026-09-v1-5-unattended-rulings.md`): each ruling is posted as a comment on its issue, beginning `Ruling (deputy for Björn, <date>):`, and the PR that acts on it appends the same four parts here, in the order ruled. The deputy first looks for an answer that already exists — the ticket, its spec, `CONTEXT.md`, the ADRs, the decision records — and quotes it; only an open fork gets a new ruling. Björn reverses one with a follow-up ticket, and a ruling that settles something architectural gets its ADR through the implementer, per this directory's README.

---

## #593 — `repo` directly before `pr`, or directly after it

Ruled 2026-10-11. Comment:
<https://github.com/BFoerschner/knobas/issues/593#issuecomment-6113041302>

**The fork:** #593 says "`repo` moves to directly after `pr`, ahead of `branch` and `commit`". The
same ticket also says "**a parent kind always comes before its child kinds**". A PR sits in a repo,
so the literal sentence breaks the rule for the repo→PR pair. PR #602 follows the rule and puts
`repo` directly *before* `pr`: `ticket, asset, repo, pr, build_config, build, page, note, commit,
branch, route, monitor`. Should `repo` go where the sentence says, or where the rule says?

**Ruling:** follow the rule. `repo` goes directly before `pr`, as PR #602 has it. This was already
decided, and the ruling only confirms it.

**Reasoning:** the answer is written down in three places, and all three agree:

- ADR-0018 (accepted): "a commit, branch or PR sits in a repo … In the fixed group order, a parent
  kind always comes before its child kinds."
- `CONTEXT.md`, **Parent**: "The entity you can open that an item sits directly inside: a repo for
  a branch, PR or commit; …"
- #591, user story 12: "As a developer, I want the repo group to come before the branch, PR and
  commit groups, so that the thing I navigate *to* is above the things I scroll *through*."

Story 12 names the PR group outright. "Directly after `pr`" is a slip in wording: #591's
Implementation Decisions has the same slip, and #593 copied it from there. The same bullet in #591
opens with the governing rule, and the ADR states that rule as a consequence of the decision. The
sentence describes one move, taking `repo` out of last place and putting it above the kinds that
sit in it, and one slot before `pr` does that. The ticket's scope and its five acceptance criteria
are unchanged, and #602 meets the first criterion's stricter reading too (repo above PR as well as
above branch and commit). `GROUP_ORDER` is not a §10.8 frozen surface (`docs/contract.md` does not
mention group order). Both of #602's tests are witnessed: mutant M2, the ticket's literal wording,
fails the unit rule test and the real-PostgreSQL `Searcher` test.

No glossary entry or ADR changes. The only text that disagrees is the bullet in spec #591; if
Björn wants the record clean, that bullet becomes "`repo` moves to directly before `pr`". That is
an edit to the spec issue and does not block #602.

**If you disagree, the cost of reversing this is:** trivial in the code: swap two entries in
`GROUP_ORDER` and drop the `("repo", "pr")` pair from the `PARENT_KINDS` test table. But the result
would contradict ADR-0018 and story 12, so a reversal means amending ADR-0018 (for example, "a
parent kind comes before its child kinds, except that a PR may precede its repo") and rewriting
story 12. It cannot be done in a comment.

## #592 — a route's URL: its body, or part of its name

Ruled 2026-10-11. Comment:
<https://github.com/BFoerschner/knobas/issues/592#issuecomment-6113047688>

**The fork:** migration `0018` weights a route's name and its URL both at A, so a route named `kuma` and a route that only carries `kuma` in its URL tie, and recency decides. The implementer gave three options. (A) Add a migration in this PR that moves the URL to a lower weight, with a §10.8 entry. (B) Rule that a route has no body, keep `0018` as it is, and record the ruling. (C) Open the PR tests-only and file a follow-up ticket for routes. The implementer leans to (B).

**Ruling:** (A). The URL is a route's body. This PR adds the next free migration (`0026` as of `main` at `e628b9db`; take the number when the entry is written, as the §10.8 entries do) and changes nothing in it except `knobas.route.fts`: name at A, URL at B, the same `alter column fts set expression` shape `0019` used on `knobas.asset`. In the same PR:
- a §10.8 entry for the migration, ratified under this issue's "What to build" and flagged for Björn's review, in the shape of the `0025` entry;
- route added to the ranking witness on the `Searcher` against real PostgreSQL: a route named for the token ranks above a route whose URL alone carries it;
- the **Not routes** paragraph in `tests/it/ranking.rs` rewritten, since this ruling answers the question it leaves open.

`0018` is never edited. `corpus.rs`'s **Not fixed here** paragraph covers lexing (which parts of a URL become lexemes), not weight, so it stays. No glossary change and no ADR.

**Reasoning:** the answer already exists, so this ruling applies it and does not re-decide anything.

- Issue #592, "What to build": *"Every kind ranks a match in an item's title above a match only in its body."* Spec #591, Implementation Decisions: *"the full-text corpus weights the title above the body for every kind"*, and story 25: *"so that the thing *named* what I typed comes first in every kind."* `CONTEXT.md` line 12 lists route as a kind.
- `CONTEXT.md`, **Route**: *"_Avoid_: URL (that is a route's property)"*. The glossary has already ruled that a URL is not how a route is named. `knobas_search::corpus::ROUTE` agrees: `title: "r.name"`, and `headline_text: "r.name || ' — ' || r.url"` is a title followed by its body. `0019` set the estate's rule for a property: *"A name match (A) outranks an ancestor match (B) outranks a property match (C)"*. A route stores no path in its `fts` (`0018`: *"No `path_text` here"*), so B is the next rung below the name. C would also meet the ticket. B is preferred because a route has no other text to sit between its name and its URL, and B matches the body weight in `0001` for items and notes.
- Option (B)'s premise doesn't hold. Weight decides rank, not whether a route matches. `sql.rs` matches with `{fts} @@ q.tsq` and ranks with `ts_rank_cd(..., q.tsq)`, so a lexeme at B is found exactly as one at A. `8111` still finds its route, and `0018`'s *"so 'kuma' and '8111' find the routes that carry them"* stays true word for word. The only thing A buys the URL is the tie this ticket exists to break.
- Option (A)'s premise ("needs Björn's sign-off") is also wrong as written. Since 2026-08-29, migration additions are ratified by the orchestrator under the ticket's own criterion and merged by the merge-manager: *"let Migration and ipc additions be merged by fable too"* (§10.8, the #42 entry). Each entry is flagged for Björn's review. The `0025` entry is the precedent for a deputy ruling taking a migration in Björn's absence: *"ratified in his absence by the deputy's ruling of 2026-09-08 on #506"*. `corpus.rs` says widening `0018`'s `fts` *"belongs to a ticket that says so"*. This ticket says "every kind", so it is that ticket.
- (C) would close #592 with one kind not doing what the issue says, and the follow-up would carry the same migration and the same entry. The ticket's scope stays unchanged, and the frozen surface grows by one generated-column expression on the smallest table knobas has (`0019`: *"a fraction of a second"*). The witness is the `Searcher` against real PostgreSQL (ADR-0013).

**If you disagree, the cost of reversing this is:** low. One more migration sets `knobas.route.fts` back to name and URL both at A, plus a §10.8 entry. The route case comes out of the ranking test, and the ruling (B) would have written goes into `docs/decisions/`. No IPC, descriptor, adapter or stored data changes in either direction, because a generated column is recomputed from `name` and `url` on every rewrite.

## #594 — a declared parent: its `kind`, and one `at` with two shapes

Ruled 2026-10-11. Comment:
<https://github.com/BFoerschner/knobas/issues/594#issuecomment-6113150556>

**The fork:** PR #604 adds `KindPaths.parent: Vec<ParentPath>` with `#[serde(default)]`, where `ParentPath { kind, at, names }`. The ticket names two parts, "where the item's parent lives in its payload" and "the names that parent answers to". Those are `at` and `names`. Three questions follow. (1) `kind`, the parent's entity kind, is a third part the ticket did not name. Is it sanctioned, or should it come out to keep the frozen surface smallest, and does Björn's sign-off cover it? (2) `at` leads either to one object (the direct parent) or to an array of objects (the chain, outermost first). The Standards review asked for two fields. Can one two-shape field be frozen? (3) The implementer left three things for #595: battery clause 6 does not inspect `parent`, no SQL macro or Rust resolver reads it, and Gitea must store the repo as an object with `full_name` first in `names`. Should #595 carry them?

**Ruling:**

1. **`kind` stays.** The spec already requires it, so this ruling confirms rather than decides. It is part of the descriptor change Björn signed off on 2026-10-11 ("merge already gets my sign off, it's fine"). The §10.8 entry gains one sentence after "Adding it now saves a second §10.8 touch later": *"The `kind` part and the two-shape `at` were ratified as within #591's sanctioned change by the deputy's ruling of 2026-10-11 on #594 (`docs/decisions/2026-10-unattended-rulings.md`); flagged for Björn's review."*
2. **One `at` with two shapes is acceptable as frozen surface, on one condition.** The branch on shape may live in exactly two places: one SQL macro and one Rust resolver in `knobas_core::payload`, held together by the agreement test in `crates/knobas-core/tests/it/payload_paths.rs`. No reader spells the branch inline. #595 lands both (see the comment there).
3. **Yes, all three go to #595**, as a separately labelled comment on that issue that names the criteria. One more check joins them: a declared parent's `kind` must be one of the adapter's own `entity_kinds`.

No glossary entry or ADR changes. **Parent** already says `under:` "matches the names the adapter declares for that parent kind", and ADR-0018 covers the rest. PR #604 appends this ruling to `docs/decisions/2026-10-unattended-rulings.md`, after #593's section.

**Reasoning:**

(1) The two-part list in #594 undercounts what its parent spec asks of the declaration. Four documents need the parent's kind from the child's declaration, and nothing else can supply it:

- ADR-0018, first consequence: "`under:` resolves against the parent each child **declares**, not against mirrored parent entities." `CONTEXT.md` **Parent**: "A parent need not be in the mirror."
- #591, Implementation Decisions: "`under:` suggestions carry their kind." Story 29: "each labelled with its kind (`payout-service · repo`)".
- #601's criterion: "a parent that is named by children but not mirrored is still suggested for `under:`", and "`under:` values show their kind".
- #591 story 34: "a new adapter's kinds to fall into the correct group order (parents before children) without anyone editing a list". PR #602 already ran into this gap. Its body says "The descriptor's planned parent declaration (#594) names a payload path, not the parent's kind, so this table records which kind sits in which." That table is the hand-kept list story 34 wants gone.

An unmirrored epic has no row to read a kind from, so only the child's declaration can say that `PAY-100` is a `ticket`. Without `kind` here, #601 would need its own descriptor change and a second §10.8 entry, and story 34 could never be met. Measured over the whole of #591, the smallest frozen surface is one touch with three parts, not two touches. The field is additive, `String`, and inside a struct that is already `#[serde(default)]`, so the ticket's own compatibility criterion holds for it too. Björn signed off "the §10.8 entry for this ticket", and the entry names `kind` and gives its reason. This ruling records that the part falls within the spec he signed off. It does not stretch his words to something the spec doesn't contain.

(2) Two fields (`at` and, say, `chain`) would add a key to the frozen shape. They would also allow two invalid declarations, both set and neither set, and every reader would have to refuse those. Each reader would still branch, only on which field is present rather than on the JSON type. With one field, the source's data decides the shape. An `at` that leads to neither shape is a miss, which is the rule this module already applies to every field ("A value that is not a string where a string is declared ... a miss"). The repo already reads Confluence's ancestors this way: `ancestor_path_read!` opens with `case when jsonb_typeof(... ->'ancestors') = 'array'`. Spec #591 takes Confluence's chain "from the existing `ancestors` payload (#284)", and #599 is declared on that array. The Standards point is right that every reader branches. The module answers it with its own discipline, "One rule, two implementations, held together by a test". That is why the ruling makes the condition explicit instead of splitting the field. The array's order (outermost first, direct parent last) matches the order `ancestor_path_read!` already displays (`order by a.ordinality`).

(3) #595 is "the first ticket to declare a parent", and none of the three gaps can be closed before something declares one. Checked against #595's text, none of them is covered as written:

- Clause 6 says no declared path may lead to a value of the wrong type, and it says this of every declared path (#277's entry: "no declared path leads to a value of the wrong type"). Today it does not walk `parent`, so a misspelled parent declaration in #597–#599 would pass the one check that runs against each adapter's live corpus (ADR-0013). Extending the clause to the new field does not change what any existing declaration certifies, because nothing declares a parent before #595. It does change `crates/knobas-source/src/contract.rs`, which §10.8 freezes, and #146's ruling put "a change to what a battery clause asserts" at Björn's gate. So #595 owes its own §10.8 entry, ratified by this ruling and flagged for Björn's review, as #498's entry was for clause 8. It does not ride on the sign-off for #594.
- `under:` cannot work without a SQL read, so the macro is implied. The Rust resolver is what the clause 6 check needs, and the agreement test is how this module keeps the two from drifting. Both `at` shapes must be covered in #595, because the shape is frozen now and a reader that handles only the object form would silently miss Confluence's chain in #599.
- #595 says "A repo answers to `owner/name` and to its bare `name`", and #596 wants `owner/repo` on the path line. `ParentPath.names` addresses paths inside a parent object, and the first name is the one shown. Together these force an object with `full_name` first. The branch tombstone stores `"repository": at.full_name` as a bare string today (`map.rs`), so if the declaration reads that key, clause 6's type check would fail on any tombstone in the corpus.

The ticket's scope is unchanged. #594 stays the declaration with nothing reading it, and the three items are prerequisites of work #595 already names.

**If you disagree, the cost of reversing this is:** (1) trivial before #604 merges: drop `kind`, its test key and its TS mirror line. After merge and before #601, it is a §10.8 entry that removes a field nothing reads. The real cost is later: #601's unmirrored-parent criterion and #591 story 34 become unmeetable without a second descriptor change, and #602's `PARENT_KINDS` table stays hand-kept. (2) cheap until #595 lands a reader, since it is one field split into two plus the entry's paragraph. After #595 and #599 it means a §10.8 entry, a migration of every adapter declaration, and both implementations of the read. (3) trivial: strike the criteria from #595 before it starts. Without the clause 6 check, a wrong parent declaration in a later adapter would surface only as an empty `under:` result, which looks exactly like an empty repo.
