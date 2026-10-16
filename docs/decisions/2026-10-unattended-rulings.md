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

## #595 — `FilterDimension::Under`, an IPC variant the ticket did not name

Ruled 2026-10-11. Comment:
<https://github.com/BFoerschner/knobas/issues/595#issuecomment-6113506838>

**The fork:** PR #605 adds `FilterDimension::Under` (`"under"`) to `knobas_search::types` and the matching member to the `FilterDimension` union in `app/src/lib/ipc/search.ts`. `SearchResponse.coverage` is IPC schema, which §10.8 freezes. Neither this ticket nor the ruling on #594 names this touch, and spec #591 says "The descriptor change is the one frozen-contract touch." The implementer names the variant in #605's §10.8 entry and asks that it be looked at first. Does it stand, or does it come out of #605?

**Ruling:** **It stands, and this ruling ratifies it. Flagged for Björn's review.** The ticket's own criterion asks for it, and the #141 entry already fixed its shape, so this ruling applies settled text and decides nothing new. Two edits are owed on the PR before merge, and the implementer makes both:

1. In #605's §10.8 entry, the sentence *"it is named here so the review sees it"* becomes: *"Ratified by the deputy's ruling of 2026-10-11 on #595 as required by that ticket's coverage criterion (`docs/decisions/2026-10-unattended-rulings.md`); flagged for Björn's review."*
2. The ruling is appended to `docs/decisions/2026-10-unattended-rulings.md` after the #594 section, next to the parallel ruling on the third read path if that one lands first.

The doc comment on `FilterDimension` still reads *"Deliberately an enum with one variant"*. It now has two, so the sentence is corrected in the same commit. No glossary entry or ADR changes. **Parent** and ADR-0018 already cover what `under:` reads, and the variant adds nothing to either.

**Reasoning:** The answer already exists in three places:

- **The ticket's operative text.** #595's "What to build" says `under:` "takes part in coverage the way `author:` does: a source with no kind declaring a parent is reported as having nothing the filter can match". One of its acceptance criteria says "Coverage reports sources that declare no parent (e.g. Uptime Kuma, Jira until its own ticket lands) as unable to answer `under:`." Spec #591 says the same, and story 32 asks that "a missing monitor reads as 'can't answer' and not 'nothing there'". "The way `author:` does" means the response's `coverage` list. That is the only place `author:` reports. #141's entry records Björn's ruling of 2026-08-31 that "the answer belongs on the response", and it declines "a static capability on the descriptor". The coverage list is keyed by `FilterDimension`. So the criterion cannot be met without a variant, short of overloading `Author`, which would be a lie on the wire.
- **The #141 entry, which fixed the shape in advance.** It says: *"`coverage` is a list of dimensions, and `FilterDimension` is an enum with one variant. Both are the generalisation seam the ruling's orchestrator guidance asked for — scope the behaviour to the author filter, but shape the report so a second filter is an addition rather than a reshape. A second dimension appends an entry and a variant; nothing existing moves. The frontend reads the list by dimension, never by position."* #605 does exactly that and nothing more. `SearchFilters` is untouched, and there is no new command, event, migration or barrel line.
- **The spec's count of frozen touches.** On #496 (2026-09-08), when spec #491 counted its migrations, the ruling held that *"the ticket is the operative text and the spec's count is shorthand, not a prohibition"*. #591's "the one frozen-contract touch" is the same kind of shorthand. It sits under "Further Notes" and is about getting Björn's sign-off on the descriptor. #595 is the operative text, and its criterion brings the variant with it. The #498 ruling went the other way, declining a `SourceError` helper. That helper was not required by any criterion, and the spec there said *"nothing else on the frozen list is touched"*, which is a prohibition. Neither condition holds here.

The gate is also settled. Björn's delegation of 2026-08-29 (*"let Migration and ipc additions be merged by fable too"*, recorded in §10.8's #42 entry) puts IPC additions with the merge-manager under a ticket's own criterion. Each entry is still flagged for his review. Battery clauses were not delegated, but the clause 6 extension in the same PR is already ratified and flagged under the #594 ruling, and this ruling does not touch it.

The smallest frozen surface that meets the criterion is one variant. The other way to report it would be a launcher that computes the gap from `KindPaths.parent` on the descriptors it already has, with no wire change. That would re-implement #141's per-query scope client-side, which the coverage entry says is decided by `source:` and kind, by enabled sources only, and by "a source that contributed no rows gets no verdict". It would also be the descriptor-capability reading Björn declined for `author:`. The answer is read off the declaration rather than measured on the corpus, unlike `author:`. That is not a departure from #141: the spec defines this dimension's answer by declaration ("a source with no kind declaring a parent"). The witness is real. The coverage seam test runs against PostgreSQL, M9 shows that it dies when the answer is forced to `true`, and the mirror test now reads the union's own line. The ticket's scope is unchanged: the variant is part of a criterion the ticket already carried.

**If you disagree, the cost of reversing this is:** before #605 merges, it is low in code: drop the variant, its TS union member, `underGaps` and the second gap block in `Results.svelte`, plus the entry's paragraph. But #595's coverage criterion and #591 story 32 then have no way to be met, so reversing also means striking that criterion. The likely consequence is that an `under:` search over a Kuma-only scope reads as an empty result and not as "can't answer", which is the defect #141 exists to prevent. After merge, removing the variant takes a §10.8 entry of its own. The launcher reads coverage by dimension, so nothing else moves. There is no migration and no stored data, so nothing needs repair. #597 to #599 lose the check that each one turns its source from `no_values` into `answered`.

## #595 — a third read path: the names a mirrored parent answers to

Ruled 2026-10-11. Comment:
<https://github.com/BFoerschner/knobas/issues/595#issuecomment-6113509745>

**The fork:** `under:<name>` must also return the parent itself when it is mirrored. To do that, PR #605 adds a second read pair to `knobas_core::payload`: `declared_names_as_parent!` and `resolve_names_as_parent`. For a mirrored record, the pair collects every `ParentPath` of the record's source whose `kind` is the record's kind, and reads those declared `names` from the record's own payload root. It never reads `at`, so the shape branch stays in `declared_parents!` / `resolve_parents`. But the criteria comment on this issue says "Every `under:` read, and later the path line and the suggestions, goes through these two", and this is a third read path. Does the pair stand under #594's ruling, must it be folded into the original pair, or must returning the repo itself leave #595's scope?

**Ruling:** **The second pair stands, on one condition.** It is not folded into the first pair, and returning the parent itself stays in scope.

1. **Scope is already settled and is not re-decided here.** This issue: "also returns the parent itself when it is mirrored", with the acceptance criterion "return the repo's branches, PRs and commits **and** the repo". Spec #591, story 4: "I want `under:payout-service` to also return the repo itself". `CONTEXT.md` **Parent**: "also returns the parent itself when it is mirrored".
2. **The pair is within #594's ruling.** That ruling's condition is about the shape branch: "The branch on shape may live in exactly two places … No reader spells the branch inline." The pair reads no `at` and has no shape branch. The sentence "every `under:` read goes through these two" restates that condition for reads of a child's parent objects. It does not forbid reading a parent's own record, which that ruling never considered.
3. **The condition: clause 6 must witness the assumption the pair rests on.** The pair works only if "a source names a parent in the parent's own record shape" (the PR's words). In PR #605, a doc comment states that assumption and hand-seeded fixtures exercise it, but no check against an adapter's corpus does. Clause 6 gains one more rule, under the same two #301 tolerances: for each declared parent `kind`, on that kind's own records in the corpus, every `names` path read from the record's root leads to a string (type check), and where none resolved, no record names a key it does not have (misspelling check). Each gets its own evidence slot. It needs a mutation witness: a conforming-adapter variant whose parent-kind records lack a `names` key that the children's parent objects carry fails the check. `just gitea-live`'s battery run then certifies it against real repos.
4. **Docs, in PR #605.** The §10.8 entry's clause 6 list gains that rule. Its "second read pair" paragraph cites this ruling and stays flagged for Björn's review with the rest of the entry. The comment on this issue that reads "Every `under:` read … goes through these two" is read as "every read of a child's parent objects". This ruling is appended to `docs/decisions/2026-10-unattended-rulings.md`, after #594's section. No glossary entry or ADR changes. **Parent** already says `under:` "matches the names the adapter declares for that parent kind", and names declared *for a kind* are the names a record of that kind answers to.

**Reasoning:**

- **Taking it out of scope** would drop an acceptance criterion, a spec story and a glossary sentence. A deputy ruling cannot do that, and the ticket's scope is to stay unchanged.
- **Folding the pair into `declared_parents!`** has nothing to fold into. That macro answers "which parent objects does *this* record point at", by walking a non-empty `at`. The pair answers "which names does *this* record answer to as someone's parent", by reading every sibling kind's declaration (a repo declares no parent; the commit's declaration says what a repo is called). Folding means one of two things. One is an `at` of the empty path, which the PR deliberately refuses and its mutant M2 pins ("would otherwise make every item its own parent"). The other is a mode flag on the macro, which puts a second branch inside the one place the condition protects. Either way the protected macro grows a branch, so folding makes the condition weaker, not stronger.
- **Frozen surface.** The pair adds no descriptor field and changes no IPC. `knobas-core` is not on the frozen list, as the entry says. It does add one obligation to the already-frozen `ParentPath.names`: the names must also resolve on the parent kind's own records. An obligation the frozen contract places on every adapter is what the battery exists to certify, so the obligation is fine only if the battery checks it.
- **Why the condition (ADR-0013).** Without it, the obligation is witnessed only where the test author typed both payloads. In `tests/it/under.rs` and `payload_paths.rs` the repo record is hand-seeded. The live test (`a_synced_commit_carries_its_repo_and_is_found_by_under`) runs `resolve_parents` on children and never reads a real repo record as a parent. So if #597–#599 declare a name the parent kind's own records do not carry, `under:` returns the children and silently leaves out the parent. That is the failure #594's ruling named: "a wrong parent declaration … would surface only as an empty `under:` result". Here it would show up as a missing row. All four planned adapters already satisfy the rule: Gitea's `base.repo` and the written `repository` object spell the repo record's `full_name`/`name`, a Confluence ancestor and a page both carry `title`, Jira's `fields.parent` and a ticket both carry `key`, and a TeamCity `buildType` and a build configuration both carry `id`/`name`. So the check cannot fail on a correct declaration. Where it would fail, the parent-itself return is genuinely broken for that adapter. An unmirrored parent kind (Jira epics outside the JQL) is asked nothing, by the existing "a kind the corpus never populates is asked nothing" tolerance.

**If you disagree, the cost of reversing this is:**
- **Dropping the condition:** trivial before merge. Delete the rule, its witness and one sentence of the entry. Later adapters then carry the unwitnessed obligation.
- **Folding the pair into the first:** moderate. It means a mode on `declared_parents!` and `resolve_parents`, plus agreement-test cases for the new mode, and it reopens the empty-path guard that M2 pins.
- **Moving the parent-itself return out of #595:** cheap in code, since it means deleting the pair, the corpus's `or exists` arm and half of `KNOWN_SQL`. But it needs Björn to amend #595's acceptance criterion, #591 story 4 and the **Parent** glossary entry. Until a follow-up lands, `under:payout-service` would no longer reach the repo it names.

## #595 — evidence slots for a parent's names: one per parent kind, or one per name

Ruled 2026-10-12. Comment:
<https://github.com/BFoerschner/knobas/issues/595#issuecomment-6115644746>

**The fork:** PR #605 gives clause 6 one evidence slot per parent kind for the parent-record rule (`"the parent's own names"`). The children's `parent names` slot works the same way. A parent's `names` are not fallback spellings, though. `resolve_names_as_parent` returns every name that resolves, and `under:` matches any of them. So when Gitea's `full_name` resolves, the battery still accepts a misspelled `name`, and `under:<bare name>` then misses the repo itself. `accepts_a_parent_name_an_earlier_name_resolves_for` pins that acceptance. The question has two parts. Should the slots stay one per parent kind, or become one per declared name, for the new rule and/or the children's slot? And does that change belong in #605 or in a follow-up ticket?

**Ruling:** **One slot per parent kind stands in #605, for both slots. That is already settled, and this ruling does not re-decide it. No follow-up ticket for per-name slots.** A per-name slot changes what a battery clause asserts. That is Björn's gate, not the deputy's, and nothing on the tracker asks for it. Two things are owed instead, and the implementer writes both:

1. **In #605's §10.8 entry, the clause 6 list records the gap so that no one later reads it as closed.** This is how #277's entry recorded "two declarations the clause cannot see". Add one sentence after rule 6: *"Like a field's candidates, a parent's names share one evidence slot, on the children's parent objects and on the parent's own records alike. So a name no record carries is accepted whenever another name of the same parent resolves, even though `under:` can never reach the parent by that name. Each adapter's live suite is what witnesses that every declared name resolves (deputy's ruling of 2026-10-12 on #595)."* The entry stays flagged for Björn's review with the rest.
2. **This ruling is appended to `docs/decisions/2026-10-unattended-rulings.md`**, after the section on the third read path.

No glossary entry and no ADR changes. **Parent** already says `under:` "matches the names the adapter declares for that parent kind", and this ruling changes none of that.

For the dispatcher of **#597**, the only adapter ticket still to come that declares more than one name (title and id): its brief should ask that the live TeamCity suite assert both declared names on real builds, as #595's live test does for Gitea. #598 (Jira, `key`) and #599 (Confluence, `title`) declare one name each, so for them a per-kind slot already is a per-name slot.

**Reasoning:**

- **The answer already exists in two rulings and the clause's own text.**
  - The criteria comment on this issue, which carries #594's ruling, asks clause 6 to inspect `parent` "using the same rules it applies to the other fields". For the children's slot, that is the tolerance the clause already documents: *"The scalars share one evidence slot across a field's candidate list … `reviewers` is not an exception to that: the array path gets its own slot only so a resolving* array *cannot excuse a wrong key* inside *its elements, which is a different question from one candidate excusing another."* `reviewers` is the repo's existing union list, and its entries share one slot. #605 matches it exactly: `at` gets its own slot for `reviewers`' reason, and the names share one.
  - The ruling of 2026-10-11 on the third read path put the new rule "under the same two #301 tolerances". The second of those is "a candidate an earlier candidate resolves for is excused". That is the slot sharing the implementer built, and the slot the ruling gave it ("its own evidence slot") is its own because it is apart from the children's, which M16 pins. The implementer read both rulings correctly.
- **Per-name slots would be a battery change that needs Björn.** #146 reserved "a change to what a battery clause asserts" to Björn (§10.8, #498 entry). The extension of clause 6 was ratified only because #594's ruling and this ticket's criteria named it. Neither asks for per-name evidence, so the deputy cannot ratify it, either in #605 or as a ticket. Filing a follow-up ticket would put a battery tightening on the tracker that no spec story or ruling asks for. Björn can do that with one comment if he wants it. The deputy should not do it for him.
- **The tolerance has a reason that still applies to names.** #301 excused a candidate because "an instance uses one of them, so per-candidate evidence would fail the second spelling wherever it is the unused one". A parent's names can have that shape too, for example an adapter that declares a Cloud spelling and a Server spelling of the same name. Per-name evidence would fail that declaration on every instance that does not use one of the two, which is the case #301 refused to fail. The clause should not be able to fail while the declaration is right.
- **The witness stays real (ADR-0013) without a frozen touch.** The gap only matters for adapters that declare two or more names, which are Gitea and TeamCity. For Gitea it is already closed on real records. `a_synced_commit_carries_its_repo_and_is_found_by_under` asserts `parents[0].names == [full_name, bare]` for every live branch, PR and commit. A misspelled `name` in the one declaration both sides read would fail that test against the real server. `just gitea-live` passed 13/13 on #605's head. What remains is the repo record lacking a `name` that its children's `base.repo` carries. That cannot happen in Gitea, where `base.repo` *is* a repository object. For TeamCity, the brief note above puts the same check into #597's live suite, where it costs one assertion and no §10.8 entry.
- **The ticket's scope is unchanged.** #605 changes one sentence of its entry and one section of the decisions file. Nothing in the code moves.

**If you disagree, the cost of reversing this is:** low, and it is the same before or after merge. Key both slots by (parent kind, name), not by parent kind. That is two `evidence.entry` keys in `check_payload_paths`. Turn `accepts_a_parent_name_an_earlier_name_resolves_for` into a rejection test, and replace the sentence above in the §10.8 entry with the tightened rule. Before #605 merges, this can ride in #605's entry, which already waits on your review of the clause 6 extension. After merge it needs a §10.8 entry of its own. No stored data or migration is involved. The risk of reversing is the one #301 named: a correct declaration that lists a spelling some instance does not use would then fail that instance's live battery. No planned adapter declares one today. Gitea's `full_name`/`name` and TeamCity's `name`/`id` are present on every record.

## #597 — the live criterion, with the TeamCity server's host key changed

Ruled 2026-10-12. Comment:
<https://github.com/BFoerschner/knobas/issues/597#issuecomment-6116073065>

**The fork:** fork kind 3, *"a live recipe red for a reason in the fixture rather than the code"*. PR #607 meets every criterion of this issue except one: *"Live TeamCity suite: a synced build is found by `under:` its configuration (`source testenv/hetzner/env` + `tunnel up` first)."* The test is written (`live_teamcity_seeded.rs::a_synced_build_carries_its_configuration_and_is_found_by_under_its_name_and_id`, both declared names asserted per the 2026-10-12 ruling on #595), but it has never run. `testenv/hetzner/tunnel up` failed with `Host key for 46.224.117.158 has changed and you have requested strict checking. Host key verification failed.` The implementer did not bypass the check and did not edit `~/.ssh/knobas-hetzner.known_hosts`. The fixture is unchanged and all three tunnels are down. Two questions. May #607 merge with its live criterion unrun, and on what terms? And what happens to #598 (Jira) and #599 (Confluence), whose live criteria need the same tunnel?

**Ruling:** **#607 waits. It does not merge until `just teamcity-live-seeded` has run green on the head that is squashed.** The answer is already on the record, and this ruling applies it. No follow-up ticket is filed, the criterion is not deferred, and #597 keeps `in-progress` with its PR open.

1. **Nobody in the loop touches the host key.** Accepting the changed key, editing `~/.ssh/knobas-hetzner.known_hosts`, passing any ssh option that relaxes host-key checking, switching `hcloud` contexts and touching the three servers all stay with Björn. A changed key may be a different machine. Until Björn comments on this issue that the key is checked, nobody in the loop runs `tunnel up` or opens any ssh connection to the three servers. The refusal is already recorded, and running the command again would only invite a workaround.
2. **No merge-manager is dispatched for #607 until that comment exists.** Step 2 skips #607 and reports it as *"waiting on Björn: TeamCity host key (#597)"*. This is not an open fork. No deputy is dispatched for it again, and it is not counted under decision kind 6. After Björn's comment, the merge-manager does what it would have done anyway: it rebases onto `main` at a SHA it records, runs `just check`, runs `source testenv/hetzner/env && testenv/hetzner/tunnel up && just teamcity-live-seeded` in the foreground and reads the counts, pastes the transcript into the PR body, leaves the fixture as it found it, and squashes.
3. **What #607 owes now.** This is docs only, and the implementer does it. Append this ruling to `docs/decisions/2026-10-unattended-rulings.md` as `## #597 — the live criterion, with the TeamCity server's host key changed`, after the last section. In the PR body, the *"Live TeamCity suite: written, NOT RUN"* line cites this ruling and stays unchecked. Do not run the gate again for a docs-only commit. The merge-manager runs it on the rebased head.
4. **#598 and #599 stay `ready-for-agent` and are not dispatched while #597 is in progress.** The cap already says so: *"At most one tunnel ticket running at a time. A tunnel ticket is one whose acceptance criteria name a live suite against TeamCity, Jira or Confluence … Their live suites spend the same fixture."* #597 holds that one place until it merges, because its merge-manager's live re-run is the step that spends the fixture. #598 is dispatched after #607 merges, and then #599, one at a time in issue order as the cap requires. Nothing is reordered, relabelled or struck. Their live criteria stay as written.

No glossary entry and no ADR.

**Flagged for Björn, not ruled:** (a) `tunnel up` goes through `teamcity jira confluence` in that order and stops at the first failure (`up_one` → `die`). So nobody has tried Jira's or Confluence's keys, and #598 and #599 cannot run their live suites with the script as it is either. When you check the TeamCity key, the other two are worth checking at the same time. (b) If the server was rebuilt, its seeded configurations and builds may be gone. `just teamcity-live-seeded` would then need the TeamCity seed again before it can witness anything. (c) PR #607's body carries two findings for triage, outside this fork: a multi-word `under:` value cannot be typed, and clause 6 did not catch mutant M1 (`at: ["build_type"]`) in the mockd battery. Neither is ruled here.

**Reasoning:** Three answers already exist, and they agree.

- **The working model's rule on adapter PRs.** *"A PR at the real systems' edge carries its live run (rule, Björn 2026-09-02, ADR-0013). A PR touching an adapter crate, a `WriteOp`, the descriptor, or the sync engine carries its live run's output in its body, and the merge-manager re-runs it before squashing."* #607's one production change is `crates/knobas-source-teamcity/src/descriptor.rs`, which is an adapter crate and a descriptor at once. The rule has no exception for a fixture that is down, and the milestone-14 exit ruling describes it as the rule *"which every tunnel-class ticket met on its own squash tree"*. A deputy cannot grant the first exception to a rule Björn made.
- **ADR-0013.** *"every adapter and every write path is tested against the real system, and no mock is a witness for any acceptance or exit criterion"*, and *"the adapter's certificate stays its `just <system>-live` recipe"*. #607's own mutation table shows what is unwitnessed without the live run. M4 (`["id"]` → `["Id"]`) *"survived … scoped to the mockd battery"*, and the PR says: *"The live test above is the witness for it on real records."* The 2026-10-12 ruling on #595 gave that witness to this ticket on purpose, because clause 6's shared slot cannot see it: *"For TeamCity, the brief note above puts the same check into #597's live suite, where it costs one assertion and no §10.8 entry."* If #607 merged now, the one place that ruling put the witness would be empty, and the gap that ruling recorded as closed would be open.
- **The precedent for a red caused by the fixture.** In #552 the deputy did not take the cheapest way around a broken fixture. It said the cheap way *"buys nothing (b) does not deliver sooner"* and left the fixture's fault in place for its real fix. That fix here is Björn's alone. Carrying the criterion to a follow-up ticket would close #597 on a criterion that has not passed. The run would wait for the same check by Björn either way, so the follow-up gains nothing. It would also leave `main` with an adapter descriptor its live suite has never read.

Waiting costs little. The declaration is six lines, nothing else is blocked by #597 (#598 and #599 are blocked by #595 and #596, both closed), and the loop's other work (#600, #601) does not use the tunnel. The ticket's scope is unchanged and no frozen surface moves.

**If you disagree, the cost of reversing this is:** low, and it takes one comment. To merge #607 now, say so on this issue. The merge-manager then squashes on review and `just check` alone, #597 closes with the live box unchecked, and the orchestrator files this follow-up as `ready-for-agent`, a tunnel ticket, blocked by nothing:

> **TeamCity: the parent declaration's live witness on the seeded server**
> - [ ] `just teamcity-live-seeded` runs green on `main` with `a_synced_build_carries_its_configuration_and_is_found_by_under_its_name_and_id` among its counted tests, transcript in the PR body (`source testenv/hetzner/env` + `tunnel up` first).
> - [ ] Every synced build resolves to exactly one `build_config` parent whose names are the seed's literal configuration name and id, and every configuration's own record answers to both through `resolve_names_as_parent`.
> - [ ] A red found here is fixed in this ticket's PR if it is in the declaration, and filed for a fork-kind-3 ruling if it is in the fixture.

What that spends: `main` carries a TeamCity descriptor that no real record has read, which is the condition ADR-0013 was written to end. And until the follow-up runs, a misspelled second name would show up only as an `under:` that misses the configuration itself. To let #598 and #599 be written while the tunnel is down, say so too. Each would then be dispatched as usual and would park at an open PR the way #607 has, and the live runs would still be made one at a time when the tunnel is back.

*Overtaken the same day by Björn's #633 (section below): the live suites now run against products started locally, so #607's live run no longer waits on the host key, and nobody verifies it. The merge condition stands in substance: `just teamcity-live-seeded`, now against the local TeamCity, green on the squashed head. Nobody in the loop runs the tunnel, ssh to the servers or `hcloud`.*

## #597 — mutant M1 and clause 6: the battery the mutation driver never ran

Ruled 2026-10-12. Comment:
<https://github.com/BFoerschner/knobas/issues/597#issuecomment-6116081997>

**The fork:** PR #607's mutation proof says all four mutants of TeamCity's parent declaration survived the mockd battery (`tests/it`, 28 tests). It adds: "M1 also passes the mockd battery's clause 6, which is worth a look. Clause 6's misspelling check apparently doesn't flag an `at` whose key no record carries in the mockd corpus." M1 is `at: ["buildType"]` → `["build_type"]`. M4 (`["id"]` → `["Id"]`) is already covered by the ruling of 2026-10-12 on #595. The question is whether M1 getting through is (a) a real gap in #595's clause 6, (b) one of the documented #301 tolerances, or (c) a mockd fixture limitation, and what is owed for it: nothing, a recorded note, a fix in #607, or a follow-up ticket.

**Ruling:** **None of the three. M1 does not get through clause 6. The battery was never run against it.** Clause 6 stays as #605 merged it. No follow-up ticket and no battery change. #607 owes a corrected mutation proof, and the implementer writes it:

1. Re-run each mutant's battery scope as a separate invocation, `cargo test -p knobas-source-teamcity --test it`, or run the package with `--no-fail-fast`. Restate the table from those runs. On my run, M1 dies in `mockd::passes_the_contract_battery` with clause 6's misspelling message (quoted below). M4 survives `tests/it`, 28 passed, by the shared-slot tolerance already recorded. M2 (`[id]`) and M3 (`[id, name]`) are well-formed declarations whose keys the records carry, so the battery should accept them. If it does, record them as accepted, not as escaped.
2. Strike the sentence "M1 also passes the mockd battery's clause 6, which is worth a look …" from the PR body. Replace it with one line: the earlier "survived" column came from a run that stopped at the lib target and never reached `tests/it`.
3. Append this ruling to `docs/decisions/2026-10-unattended-rulings.md`, after the section "#595 — evidence slots for a parent's names".

No glossary entry and no ADR changes, and no §10.8 entry. Nothing frozen is touched, and the clause asserts exactly what #605's entry says it does.

**Reasoning:**

- **The clause rejects M1 by its own text.** At the PR head (`dab1694a`), `check_payload_paths` weighs `at` for every build in the corpus:
  ```rust
  let seen = evidence.entry((&item.kind, field::PARENT)).or_default();
  let found = at(&item.payload, &candidate.at);
  …
  seen.names_a_missing_key |= names_a_missing_key(&item.payload, &candidate.at);
  ```
  `names_a_missing_key` returns `true` as soon as an object lacks the next segment (`None => return true`). A build's payload root is an object with `buildType` and no `build_type`. So under M1 every build sets `names_a_missing_key`, none sets `resolved`, and the final loop fails `assert!(seen.resolved || !seen.names_a_missing_key, …)`. The unit test `rejects_a_parent_at_a_key_the_records_do_not_have` (`fields.epicc`) already pins this case in `knobas-source`.
- **Neither tolerance applies.** The battery's corpus contains builds. `passes_the_contract_battery` scopes the instance to `quiet_build_type()`, and `quiet_newest_build()` expects that configuration to have a finished build. `sync.rs` keeps builds whose `buildType.id` is in `build_type_ids`. So `build` is a populated kind, which rules out tolerance 1 ("a kind the corpus never populates is asked nothing"). Tolerance 2 is about one candidate excusing another, and M1 has a single `at`. The mockd corpus is not short of anything here either: the unmutated build records carry `buildType`, and `project_key`'s `["buildType", "projectId"]` already resolves on them under clause 3.
- **The run shows it.** I applied M1 to an exported copy of `dab1694a` (`git archive`, so nothing in the worktree changed) and ran `cargo test -p knobas-source-teamcity --test it passes_the_contract_battery`. Exit 101:
  > `panicked at crates/knobas-source/src/contract.rs:784:9: descriptor.payload_paths declares parent of kind "build" at a path no item of that kind resolves, and some of those items carry an object the path's next key is simply not in -- so it names a key this adapter's own records do not have …`
- **Why the proof said otherwise.** The driver (`issue-597-mutate.sh`) runs `cargo test -q -p knobas-source-teamcity` without `--no-fail-fast`. Cargo stops at the first failing test target, and the lib target fails first because the golden test kills M1. Running that exact command on the M1 copy gives `error: test failed, to rerun pass '-p knobas-source-teamcity --lib'` and `running 110 tests`, and `passes_the_contract_battery` appears 0 times in the log. So the empty battery column means the battery never ran, not that it accepted the mutant. M4 does survive `tests/it` when that target is run on its own (`test result: ok. 28 passed`), which agrees with the 2026-10-12 ruling. That agreement is luck, though: the driver never measured it.
- **Why not a ticket or a battery change.** Nothing is missing from the clause, so there is nothing to tighten, and #146 would reserve any change to what clause 6 asserts for Björn anyway. Under the 2026-09-02 ruling the mockd run certifies nothing in either direction. The witness for this declaration against real records is the battery run inside `tests/live_teamcity_seeded.rs` (`just teamcity-live-seeded`), plus the new two-name live test. That witness is still the open criterion on #607, blocked by the host-key failure, which only Björn can clear.
- **The ticket's scope is unchanged.** The fix is to #607's evidence. The code does not move.

**If you disagree, the cost of reversing this is:** low, and there is little to reverse. Calling M1 a real clause 6 gap would mean filing a ticket against a check that demonstrably fires. That ticket's first mutation run would close it as not reproducible, and changing the clause would still go through Björn's gate (#146). Leaving the PR body uncorrected costs more: the next reader of #607 would take "clause 6 misses a misspelled `at`" as a recorded finding and might weaken trust in, or re-litigate, a check that works. The driver flaw would also stay unnoticed by the next implementer who copies it. Their "survived the battery" column would then always be empty for any mutant a unit test already kills.

## #597 — "title" means name, and a multi-word under: value is quoted

Ruled 2026-10-12. Comment:
<https://github.com/BFoerschner/knobas/issues/597#issuecomment-6118487205>

**The fork:** PR #607 raises two questions, and PR #608 independently raises the second one. (1) This issue says the configuration "answers to its **title** and its **id**". The title the launcher shows is the composed `<project> / <name>`, and no stored record carries that string. #607 declares `names: [name, id]`. Is `name` the right reading of "title"? (2) The tokenizer keeps a double-quoted run together *with* its quotes, and the `under` arm on `main` (`819182df`, `query.rs`) only trims and lowercases: `let value = value.trim().to_lowercase();`. So `under:"Deploy Staging"` looks up `"deploy staging"` with the quotes and matches nothing. That leaves TeamCity's `Ledger_Deploy_Staging` reachable by id only, and #608's `Falkenstein DC` not reachable at all. How are multi-word `under:` values written and parsed, which ticket carries the change, and what do #601's suggestions insert meanwhile?

**Ruling:**

1. **"Title" means the configuration's `name`. #607's `names: [name, id]` stands.** No change to #607 for this part, and its scope is unchanged.
2. **A multi-word `under:` value is written in double quotes, `under:"Deploy Staging"`. The parser reads the name inside one pair of quotes. This lands in #601 and nowhere else.** #601's work in progress (`.worktrees/issue-601`, `71e8264d`) already does it. `unquote` in `query.rs`'s `under` arm, `complete()` in `suggest.ts` writing `under:"Runbooks Overview" ` for a name with whitespace, `under.rs::a_quoted_name_with_a_space_scopes_like_any_other`, and a sentence in #601's §10.8 entry ("One grammar change rides with it, in `knobas-search` (not on the frozen list)"). This ruling ratifies that. It is not new work. No follow-up ticket is filed.
3. **There is no "meanwhile".** The parser change and the completion ship in the same PR, so the box never inserts a form the parser does not read. One condition on #601 before it opens its PR. `tests/it/suggest.rs` probes every offered parent with `format!("under:{}", p.name)`, which is the unquoted form. That probe is the backend half of the round trip, so it must run the form the box writes. That means the name quoted when it contains whitespace, the same rule as `complete()`. The seeded fixture must also include at least one offered parent whose name has a space, so the probe exercises the quoted path. Without both, the frontend test pins what the box writes, `under.rs` pins what the parser reads, and nothing checks that the two agree.
4. **Scope changes, stated explicitly:**
   - **#601:** in scope as built. Quoted `under:` values are part of "a picked value completes the token", plus the condition in point 3.
   - **#599:** its text and criteria are unchanged. **The orchestrator adds #601 to #599's blocked-by.** Every page in the seeded fixture (`fixtures/tidewater/work.json`) has a title with a space ("Payments architecture overview", "SEPA payout retry design", …). So #599's criterion "Live Confluence suite: a seeded grandchild page is found by `under:` its grandparent" can only be met by typing a quoted title. This costs no time, because #599 already waits behind #607 under the one-tunnel cap (ruling of 2026-10-12 above).
   - **#600 / PR #608:** unchanged. Nothing is owed. Its "Not done here" note is answered by this ruling, and a committed estate benchmark over spaced names can follow in any later ticket once #601 merges.
   - **#597 / PR #607:** unchanged. Its live test asserts the declared names through `resolve_parents` and `resolve_names_as_parent`, not through a typed query, so it does not depend on quoting. The finding in its body ("Finding for triage (not fixed here)") is answered by this ruling, and the PR body cites it.
5. **The rule is limited to `under:`.** `kind:` and `source:` values are ids without spaces, `author:` holds a source's username verbatim (contract §4.1), and #591 story 31 keeps `author:` and `updated:` as they are. A parent name that itself contains a `"` cannot be written. That is recorded, and no escape syntax is added.
6. **Docs.** #601's PR appends this ruling to `docs/decisions/2026-10-unattended-rulings.md`, after the last section, as `## #597 — "title" means name, and a multi-word under: value is quoted`. There is no glossary entry and no ADR. **Parent** says `under:<name>` "matches the names the adapter declares for that parent kind exactly, ignoring case". Quoting is how a name is typed, not what it matches. Neither `CONTEXT.md` nor ADR-0018 says how a name is typed.

**Reasoning:**

- **Nothing earlier settles spaced values.** I checked spec #591, `CONTEXT.md` **Parent**, ADR-0018, the design spec §4, `docs/contract.md`, the rulings on #594 and #595, and `docs/decisions/`. None of them says how a filter value with a space is typed. The only existing quote rule is the tokenizer's own doc: "keep a double-quoted run together **with its quotes** -- `websearch_to_tsquery` reads those quotes as a phrase". That rule serves the search text. #591's examples are all single words (`under:Runbooks`, `under:PAY-100`, `under:hetzner-1`). So this is a new fork and not a re-decision.
- **(1) is already answered by the record.** #591 frames the mechanism as names read from the payload: the parent declaration names "where the parent's identity lives in the payload, plus the **names that parent answers to**", and matching is "exact … against the declared names". A declared name is a `PayloadPath`. The composed `<project> / <name>` is not in any payload, so declaring it would take a new kind of name on the frozen descriptor. The ruling of 2026-10-12 on #595 already took this reading when it checked the four adapters: "a TeamCity `buildType` and a build configuration both carry `id`/`name`", and "the brief note above puts the same check into #597's live suite", which is the check #607 wrote against `name` and `id`. Confluence's "title" in #599 is the payload's own `title` field, so in these tickets "title" means the parent's human name as the source stores it. Two configurations named `Deploy` in different projects then scope together, which is #591 story 8's rule for a shared bare name ("scope to all of those repos, so that the filter never silently picks one").
- **Why quotes, and not something else.** The tokenizer already keeps `"…"` together, so the parser change is one `unquote` in one arm, in a crate not on the §10.8 frozen list. Quoting is also what #601's completion already writes. The other options are worse. Matching the value with its spaces replaced by `_` or `-` would guess, and a guessed name breaks "exact, ignoring case". An escape syntax (`Deploy\ Staging`) is a second spelling of the same thing, and would add more grammar to an unfrozen surface. Letting `under:` swallow the rest of the box would make `under:payout-service jitter` (story 5) impossible.
- **Why #601 and not a new ticket.** #601's criterion "a picked value completes the token" cannot be met for any spaced parent name without it. Story 30 requires that "a suggestion never leads to an empty result", and an unquoted spaced name would complete into an unknown token plus a search word. The ruling on #595 (2026-10-11, `FilterDimension::Under`) set the precedent: a change that a ticket's own criterion cannot be met without belongs to that ticket. A separate grammar ticket would have to block #601, and it would land a parser feature that nothing writes. #601 already carries it, with a test on each side.
- **Why the condition in point 3.** This is the witness-gap class: each endpoint is tested and the wire between them is not. As it stands, `suggest.test.ts` pins `'under:"Runbooks Overview" '` and `under.rs` pins that the quoted form scopes, but the suggestion read's own "every offered parent leads somewhere" probe types the unquoted form. A spaced parent in that seed would either fail the probe or, worse, show that the probe never offered one. Making the probe use the box's rule closes the round trip on a real PostgreSQL (ADR-0013's seam for search).
- **Why #599 gains a blocker and not new criteria.** Its criteria already say `under:<ancestor title>`, and the seeded titles contain spaces. Making #601 a blocker turns "met only if the implementer picks a one-word page" into "met as written". #599's text is unchanged.

**If you disagree, the cost of reversing this is:** low before #601 merges, and moderate after. For (1): declaring a composed `<project> / <name>` would mean a name that is not a payload path, which is a new form on the descriptor (a §10.8 entry and your sign-off), a change to clause 6's evidence rule, and a seed and live assertion for the composed string. #607 would wait for all of that. For (2): to move quoting out of #601, say so on this issue. #601 then drops `unquote` and its test, `complete()` stops suggesting spaced names (they would have to be filtered out, which leaves Confluence pages, most estate assets and many TeamCity configurations off the list), and the orchestrator files:

> **Search: a multi-word `under:` value is typed in double quotes**
> - [ ] `under:"<name with a space>"` scopes by that name, exactly and ignoring case, and is not reported as an unknown token (search seam, real PostgreSQL).
> - [ ] `under:"<name>" <word>` scopes and searches the word. An unclosed quote still swallows the rest, as today.
> - [ ] The query box's `under:` completion quotes a name with whitespace, and the suggestion read's probe runs that same form against a seeded parent whose name has a space.
> - [ ] Full local `just check` passes.
>
> Blocked by: #601. Blocks: #599.

That costs a second pass over the same files and a release in which most Confluence page titles cannot be scoped to. After #601 merges, reversing means removing a tested parser arm and a completion rule, and any saved list that stores `under:"…"` stops resolving and starts reporting an unknown token.


## #600 — an asset answers to its name

Ruled 2026-10-12. Comment:
<https://github.com/BFoerschner/knobas/issues/600#issuecomment-6118537306>

**The fork:** PR #608 merged as `5410e0a4`. The merge-manager found that `CONTEXT.md` **Parent** no longer describes what `under:` does in the estate. The entry says `under:<name>` "matches the names the adapter declares for that parent kind exactly, ignoring case, and also returns the parent itself when it is mirrored." Neither half holds for an asset. No adapter declares an asset's names, and **Asset** says an asset is "Never a mirrored [item]", so read literally the entry says `under:hetzner-1` matches nothing and does not return `hetzner-1`. #608 does both, as #600 requires. Does the entry need amending, or do #591 and ADR-0018 already cover the estate so the entry reads correctly as it stands?

**Ruling:** amend the entry. The behaviour is already decided, so this ruling only brings the glossary in line with it.

1. **What was settled.** Spec #591, Implementation Decisions, per-source declarations: "**Assets:** the existing parent field (ADR-0014), and a route's exposing asset. These are already stored as fields, not declared paths. Search reads them directly, and an asset answers to its name." #600: "Search reads those fields directly, not through a declared path, and an asset answers to its **name**", and its first criterion asks for "child and grandchild assets, their routes, and the asset itself." #608 built exactly that. Nothing is re-decided here, and #600's scope is unchanged.
2. **The amendment.** In **Parent**, right after the sentence ending "…and also returns the parent itself when it is mirrored.", insert this sentence:

   > An asset is the one parent no adapter declares: it answers to its name, read from its own field, and `under:` returns the asset itself, because the estate holds it.

   The rest of the entry stays as it is. "For a synced item, the adapter declares it per kind as a declared path" is already limited to synced items. The opening list already names "the parent asset for an asset, and the exposing asset for a route (the field of ADR-0014…)". Transitive reach is already covered by "everything beneath a parent, transitively".
3. **Who writes it: #601's PR, not a new ticket.** The implementer in `.worktrees/issue-601` adds the sentence in a separate commit. It goes in the same PR that appends #597's ruling to `docs/decisions/2026-10-unattended-rulings.md`, and that PR also appends this ruling after it as `## #600 — an asset answers to its name`. #601's scope and criteria are unchanged. No follow-up ticket is filed.
4. **No ADR change, no §10.8 entry.** ADR-0018 says "an asset under an asset" belongs to the one concept. Its consequences are about declared paths and do not contradict fields read directly. #608 touched no frozen surface. `docs/contract.md:9774` ("excludes itself from an `under:` query until then", naming #600) reads correctly as history, as #608's body says.

**Reasoning:**

- **The record settles the behaviour, and only the glossary disagrees.** #591 and #600 both say, in the same words, that an asset answers to its name and is read from its own field. The rulings on #594, #595 and #597 each said "no glossary change" because **Parent** "already says `under:` matches the names the adapter declares." That was true for the synced sources those tickets covered (Gitea, TeamCity, the descriptor), and none of them reached the estate. #600 is the first ticket where the sentence and the code disagree, so this is a new gap and not a reversal of those rulings.
- **Why amend instead of reading the entry generously.** `CONTEXT.md` is the vocabulary agents brief and review against. "Returns the parent itself when it is mirrored", next to **Asset**'s "Never a mirrored item", is a direct contradiction. A later reviewer of the estate's `under:` arm, or of #601's asset suggestions, would read it as a bug. One sentence costs less than one wrong review.
- **Why the wording names the field and the estate.** "An asset answers to its name" (the merge-manager's clause, and #591's words) fixes the match half. The match half alone would leave "returns the parent itself when it is mirrored" still excluding the asset, so the sentence also says why the asset comes back: the estate holds it, just as the mirror holds a mirrored parent. "Read from its own field" keeps ADR-0014's distinction (a field, not a declared path) visible, so nobody adds an asset declaration to a descriptor to make the first half true.
- **Why #601 and not a follow-up ticket.** #601 is the next open #591 PR in progress, and it already touches this subject. Its `crates/knobas-search/src/suggest.rs` (at `d51d0c83`) offers "asset that holds another asset or exposes a route, answering to its name". Its `under:` suggestions draw on "the parents that the mirror and estate declare", so the glossary sentence and the code that offers asset names land together. It already carries a docs commit to `docs/decisions/2026-10-unattended-rulings.md`. It is based on `819182df` and must rebase onto `5410e0a4` before it opens anyway, and that rebase brings in the code this sentence describes. #607 is the wrong home: it waits on Björn's host-key check with no date, and TeamCity has nothing to do with the estate. A separate docs ticket would carry one sentence through a full gate, and two docs PRs would touch the same entry.
- **Smallest surface.** This is one sentence in an unfrozen document. No code, no descriptor, no §10.8 change.

**If you disagree, the cost of reversing this is:** low. If you would rather keep **Parent** limited to synced items and move the estate's half to **Asset** (for example, "An asset is a [parent](#parent) by its field: `under:<asset name>` returns the asset and its subtree"), say so on this issue. #601's implementer moves the sentence before the PR merges, or a one-commit docs PR moves it afterwards. If #601 stalls or is closed before it lands, the orchestrator files this instead and does not wait:

> **Glossary: an asset answers to its name under `under:`**
> - [ ] `CONTEXT.md` **Parent** carries the sentence from the ruling of 2026-10-12 on #600, word for word.
> - [ ] `docs/decisions/2026-10-unattended-rulings.md` gains `## #600 — an asset answers to its name`, in the four-part shape.
> - [ ] Full local `just check` passes.
>
> Blocked by: none (#600 is merged).

## #614 — a source is offered only once it has a live row

Ruled 2026-10-12, in the issue body:
<https://github.com/BFoerschner/knobas/issues/614>

**The fork:** PR #613 (#601) merged as `5f89a610`. Its merge-manager found that `source:` offers every enabled source, and the read checks nothing else. A source that was added but has not synced yet, or that has synced nothing, is offered anyway, and `source:<it>` returns nothing. Coverage does not explain the empty result either, because `coverage.rs` leaves out a source with no rows on purpose. Spec #591 has two stories that pull apart here. Story 28 asks the box to suggest "my configured sources". Story 30 asks it to offer "only values that exist right now, so that a suggestion never leads to an empty result." Which one bounds the source list?

**Ruling:** story 30 bounds every list, so a source is offered only if it has at least one live row. The source list is the enabled sources that have a row in `sync.live_item`, in the same order and with the same labels. A disabled source's rows are already hidden there (migration `0012`), so `enabled` and "has live rows" agree. The read stays once per launcher opening and off the keystroke path. Once a source's first sync lands, it shows up at the next opening.

**Reasoning:**

- **#601 already settled the same tension for kinds.** The spec's "Value suggestions" bullet says values come from "the kind catalog, the configured sources, and the parents". PR #613 still narrowed the kind catalog to kinds that have a live row (`KINDS_SQL`), because "The catalog alone would offer kinds that lead to an empty result, and spec #591 story 30 says a suggestion must never do that." The merge-manager accepted that reading. Sources get the same treatment.
- **Story 28 still holds.** The list is still made of configured sources. It only leaves out the ones a pick would lead nowhere from.
- **Smallest surface.** This is the only fix that needs no DTO change and no §10.8 change.
- **Not chosen:** offering every enabled source with a "not synced yet" hint. That changes `SourceSuggestion` (a §10.8 DTO) and the launcher. If Björn wants it, it is a separate request.

**If you disagree, the cost of reversing this is:** low. The filter is one `exists` probe in `crates/knobas-search/src/suggest.rs` and one test in `crates/knobas-search/tests/it/suggest.rs`. Dropping it brings back the `enabled`-only list. The hint variant is additive: a field on `SourceSuggestion` with its §10.8 entry, plus the launcher's rendering of it.

## #612 — a repo found by its bare name: the adapter, the title, or the index

Ruled 2026-10-12. Comment:
<https://github.com/BFoerschner/knobas/issues/612#issuecomment-6118644395>

**The fork:** a Gitea repo's title and body both carry its `full_name` (`map.rs`, `repo_item`), and PostgreSQL lexes `tidewater/payout-service` as one file-path lexeme. So `payout-service`, `payout` and `Payout-Service` match nothing of the repo. The ticket names three options. (1) The adapter adds the bare `name`, or the owner and the name as separate words, to the repo's `body_text`, at body weight B. (2) The same words at title weight. An adapter can only get them there by changing the `title` it stores, which is the title every surface shows. (3) A migration changes what `sync.item.fts` lexes. Options 1 and 2 change stored text, and an existing install only gets new text when it re-emits its repos, which ties them to #610's cursor bump. Option 3 changes no stored text.

**Ruling:** (3), on the title only, and only by adding lexemes. The next free migration (`0027` at `origin/main` `5410e0a4`; take the number when the §10.8 entry is written, as #592's ruling said) changes `sync.item.fts` and nothing else. It uses one `alter column fts set expression`, the shape `0019` and `0026` used. The title stays at A as `0001` has it. When the title contains a `/`, the same title with each `/` read as a word break is added at A as well. The body stays at B and is lexed as before. The expression I measured on the embedded PostgreSQL 18.6:

```sql
setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
setweight(case when strpos(coalesce(title, ''), '/') > 0
               then to_tsvector('english', translate(title, '/', ' '))
               else ''::tsvector end, 'A') ||
setweight(to_tsvector('english', coalesce(body_text, '')), 'B')
```

The same PR carries:
- a §10.8 entry for the migration, **ratified under #612's "What to build" by this ruling and flagged for Björn's review**, in the shape of the `0026` entry;
- no change to `knobas-source-gitea`: `repo_item`, the golden tests and `CURSOR_VERSION` stay as they are. Without an adapter change there is no live Gitea criterion. The witness is the `Searcher` against real PostgreSQL (ADR-0013);
- this ruling appended to `docs/decisions/2026-10-unattended-rulings.md`.

The ticket's "Blocked by" stays **None**. The fix doesn't rely on #610's re-emit. No glossary change and no ADR: this is a lexing detail of one corpus, like the URL table on `corpus::ROUTE`, and not a new term or an architectural decision.

**Reasoning:**

- **The spec decides the weight, and only (3) meets it.** #612 cites story 1: *"typing a repo's name to show that repo at the top of its group"*. Story 25 adds *"so that the thing *named* what I typed comes first in every kind"*, and #591's Implementation Decisions say *"the full-text corpus weights the title above the body for every kind"*. The repo's name is its title. Option 1 puts that name at B, where a repo whose description mentions `payout-service` ties it and recency decides. Measured on 18.6 with two repos, `tidewater/payout-service` and `tidewater/ledger` (body: *"consumes events from payout-service"*), for the query `payout-service`. Under `0001`'s expression only `tidewater/ledger` matches (0.4). Under the expression above, `tidewater/payout-service` scores 1 and `tidewater/ledger` 0.4. Under option 1 they would score 0.4 each.
- **Option 2 changes what every surface shows.** A repo's `full_name` title is what the launcher row, the inbox, rooms and links draw. #591 changes searchable text and says *"Titles are not affected"*. Rewriting a title to make it searchable is outside the ticket.
- **(3) keeps search free of source knowledge.** Story 35: *"so that search contains no source-specific knowledge"*. The rule is "a `/` in a title also separates words" for every kind. Nothing in it names Gitea.
- **Criterion 4 is met by construction, and #610 stays separate.** A generated column is recomputed by the migration's table rewrite, so every repo already mirrored is findable the moment the migration runs. Options 1 and 2 can't do that. `sync.rs` re-emits a repo only *"when its `updated_at` differs from the one in the cursor"*. If #610 merges first, an install that runs its build holds a version-2 cursor, and options 1 and 2 would need a second bump to version 3 and another full sync per Gitea source.
- **Frozen surface.** Option 1 adds none, and (3) adds one generated-column expression and an entry. A smallest-surface reading still has to do what the ticket says, and option 1 fails story 1 in the case the spec describes. #592's ruling took a migration over leaving a kind short of "every kind" for the same reason. The cost is one entry in the `0026` shape, and migration additions have been ratified under the ticket's criterion since the #42 entry.
- **Additive, so nothing that matches today stops matching.** The whole lexeme stays: `tidewater/payout-service` still matches at A, measured at 1.4 (title A plus the body's copy at B), and `tests/it/under.rs`'s full-name query is unaffected. A title without a `/` lexes byte for byte as before (measured: `Fix typo in payout` → `'fix':1A 'payout':4A 'typo':2A`, unchanged). Bodies are untouched. That keeps every URL in every body lexed as `corpus::ROUTE`'s table records, and `0026`'s *"Which parts of a URL become lexemes at all is unchanged"* stays true.
- **Effects outside repos, named so nobody meets them as surprises.** Every title with a `/` gains its parts, so the same gap closes for branch names too. Measured: `feature/payout-jitter` is now found by `jitter`, which `0001` misses. `knobas_core::suggest`'s `SIMILAR_TEXT` reads `sync.item.fts` for ticket, page and PR, so a slashed title there gains stems it can share. Its tests must stay green. The query `payout` now scores 2 on the repo (`payout-servic` and `payout`). That is the inflation #611 fixes for every hyphenated title, and it is not #612's. #611 is independent. If either ticket lands a `sync.item.fts` migration second, it restates the first one's expression whole and never drops it.
- **The rewrite.** `sync.item` is the mirror's largest table, and `set expression` rewrites it and rebuilds `item_fts_idx`. On 18.6 it ran without complaint on a table with a dependent view and a GIN index. The implementer confirms that on the real schema (`sync.live_item`, as `0012` redefines it, selects `i.fts`) and records the measured rewrite time in the migration header, as `0019` did.

**If you disagree, the cost of reversing this is:** low. One more migration restores `0001`'s expression for `sync.item.fts`, with a §10.8 entry. Then option 1 is built in the adapter: add the bare name to `repo_item`'s `body_text`, update the golden test, bump `CURSOR_VERSION` (to 3 if #610 has landed), and each Gitea source does one full sync. No IPC, DTO, descriptor or stored item changes in either direction, because the generated column is recomputed from `title` and `body_text` on every rewrite.

## #611 — a repo's bare name as a name: close the tie, and where

Ruled 2026-10-12. Comment:
<https://github.com/BFoerschner/knobas/issues/611#issuecomment-6118983310>

**The fork:** PR #618's `NAMED` key is true when the title, lexed with `english`, gives exactly the typed words' lexemes. It is a second ordering key, read only between equal ranks. Once #612's migration lands, repos `tidewater/payout` and `tidewater/payout-service` both score 1 for `payout`, as #618's composition table measured. Neither is *named* `payout`, because a slashed title lexes as one file-path lexeme, so recency decides between them. Story 1 is not guaranteed for two repos whose names share a first word, and neither #611's nor #612's criteria cover that case. There are three questions. Does story 1 require closing this? If it does, should it be closed by extending `NAMED` (for example, to also compare the title's last `/` segment), by folding it into #612, or by a follow-up ticket?

**Ruling:** Story 1 requires closing it. The fix goes in a **follow-up ticket** (#621), and #611, #618 and #612 keep their scope unchanged. The rule is source-agnostic: **a title that contains a `/` is also named by its last `/` segment.** `NAMED` becomes true when the typed words' lexemes equal the lexemes of the whole title (as before) **or** of the text after the title's last `/`. #621 spells it `split_part(title, '/', -1)` under a `strpos(title, '/') > 0` guard. It stays a tiebreak and not a tier, and it changes no `fts` column and adds no migration. No glossary change and no ADR: this is an ordering detail of the search query, in the same family as #612's lexing rule, and it adds no new term.

**Reasoning:**

- **Story 1 covers the bare name, and the record already says so.** Story 1: *"I want typing a repo's name to show that repo at the top of its group"*. #591's Implementation Decisions, Gitea: *"The repo answers to `owner/name` and to its bare `name`."* Story 6 asks for `under:payout-service` to work *"so that I don't have to remember the owner"*. #612 exists because a bare name has to find the repo. In this spec a repo's name includes its bare name, and story 25 (*"the thing *named* what I typed comes first in every kind"*) puts `tidewater/payout` above `tidewater/payout-service` for `payout`. Leaving it to recency would leave story 1 unmet in the case #611 was filed to fix.
- **The last segment, not every segment, and not the declared names.** #612's ruling set the rule *"a `/` in a title also separates words"* for every kind (story 35: *"so that search contains no source-specific knowledge"*). The name key needs a narrower reading of the same rule. If every segment counted, `tidewater` would name every repo the owner has. The last segment is the bare `name` for a repo and the leaf for a branch (`feature/payout`), and the rule never mentions a kind. Reading the parent declaration's names (the names `under:` matches) would also be source-agnostic, but it would tie a ranking key to the parent mechanism, read payloads for every match inside the narrow `m` CTE, and only reach kinds that are parents.
- **A follow-up ticket, because neither ticket can hold the witness without changing its scope.** #611 puts repos outside its scope in its own text. #618 can't witness the repo case on real PostgreSQL, because until #612's migration lands `payout` doesn't match a repo at all, and a body-only stand-in would certify a fixture and not the repo (ADR-0013). #612's ruling limits it to one `alter column fts set expression` with **Blocked by: None**; folding this in would make #612 blocked by #611 and widen a migration ticket into a query change.
- **Smallest frozen surface.** The change is one expression in `sql.rs`, which no §10.8 entry freezes (#618 changed `RANK_WHOLE` and `NAMED` without one). It adds no migration, IPC, descriptor or stored-data change.
- **Effects outside repos, named so nobody meets them as surprises.** Every slashed title gains one more way to be named. A TeamCity build `Payments / Deploy #42` is named `deploy 42`, and a page `CI/CD` is named `cd`. The key is read only between equal ranks, so it reorders ties within a group and never decides which rows match or beats a stronger rank (story 26).

**If you disagree, the cost of reversing this is:** low. If you would rather accept recency between repos that share a first word, revert #621 and record here that story 1 is met by the repo matching and ranking at the title weight, not by its position among same-scored repos. Reverting is one expression in `NAMED`, one allowed literal in `sql.rs`'s tests, and the five tests #621 added to `tests/it/bare_name.rs`. No migration, §10.8 entry, IPC or stored data is involved in either direction.

## #629 — five monitored runs, admitted for a not-reproduced verdict only

Ruled 2026-10-12 (deputy for Björn). Comment:
<https://github.com/BFoerschner/knobas/issues/629#issuecomment-6120741815>

**The fork:** #631's merge-manager reports that condition (a) was not met as this ticket states it. The ticket says: *"the orchestrator gives the run the machine for its whole duration … the dispatcher must not start a second gate while this one runs"*. It also says *"A run that misses any of these is discarded and not quoted, red or green."* The #533 ruling names the same mechanism: *"the orchestrator holding a slot for it the way the desktop cap holds the screen"*. In fact the orchestrator dispatched the implementer while #630's merge-manager was reviewing on the same machine. The implementer compensated with a monitor that sampled the load, the gate slots and the top six processes by CPU every 5 s for the whole run. It discarded `main-3`, where the monitor caught `.worktrees/issue-627/target/debug/deps/it-…` and its Postgres running outside any gate slot. It also discarded `main-1`, whose monitor kept no process table. The five qualified runs read `launcher_board` p90 at the cap as 86, 87, 83, 84 and 87 ms. That is a spread of 4 and at least 13 ms of clearance. Load1 ran from 1.73 to 3.39, and the rail was monotonic in every run. In every sample `knobas-gate-slots` held only the run's own slot. The merge-manager's argument is that load can only add time, so a contaminated run cannot produce a false green. Do the five runs count, and with what wording? Or does the ticket need five new runs under an orchestrator hold?

**Ruling:** **The five runs count for the conclusion this PR draws, and for no other.** The PR draws "not reproduced", and with it no change to the constant, the budget or the statement. Two wording changes come before the squash:

1. **`saved.rs`'s doc comment.** *"Five runs that met them"* becomes a sentence saying the five runs met conditions (b) to (d), and that (a) was shown by a 5 s process and gate-slot monitor over each whole run, not by an orchestrator hold. It also says one run was discarded on that monitor's evidence. Something like: *"Five runs that met (b)–(d), with (a) shown by a 5-second process monitor over each whole run rather than an orchestrator hold (one further run was discarded on its evidence; ruling on #629)"*. The implementer's or the merge-manager's own phrasing is fine if it says the same thing.
2. **The PR body.** The acceptance-criterion line *"each meeting the four #533 conditions"* gets the same qualification. *Load control* gains one sentence: the runs were admitted under this ruling for a not-reproduced verdict only, and a red or thin reading taken this way would not have counted.

The merge-manager then squash-merges against its recorded SHA as it proposed. This does not need a new `just search-perf` run, because the diff is still one doc comment. #629 closes on the merge through `Closes #629` in the PR body, which is already outside backticks.

**The standard does not move.** `docs/agents/working-model.md` keeps quoting #533 as written, and the hold is still the mechanism the next perf ticket gets. This ruling is not precedent for dispatching beside a perf run. It records that the departure was the orchestrator's. The implementer did what the standard's own reasoning asks for when the hold is missing.

**Reasoning:** the question this ticket settles is already answered in two places. My job is to read them, not to re-decide them.

*What (a) is.* The #533 ruling states (a) as a fact, *"nothing else on the machine **for the whole run**, not at the instant it starts"*. The hold is how that fact is secured. The ruling also gives the reason for the hold: *"because a watcher firing on a once-sampled one-minute average is exactly what let a sibling in mid-run"*. The acceptance criterion says the same thing as a fact: *"each meeting the four #533 conditions (whole machine for the run, …)"*. The monitor is not the watcher #533 rejected. It samples processes, not a one-minute average, and it samples them every 5 s, not once. It also showed that it can see the interference that matters. It caught `main-3`, which is exactly the "sibling in mid-run" that #533 was written about, and that run was discarded. A hold would not have caught `main-3` either, because that process held no gate slot. A hold is a promise that nothing will run. The monitor is a record of what did run. Under ADR-0013's rule that the witness must be real, the record is the better witness.

*Is "load can only add time" sound for this conclusion?* Mostly, with one gap. The gap is closed by the numbers. The rule being tested is #533's: *"If a quiet run of this code ever clears the budget by less than its own spread, the constant steps to eight."* That rule takes two quantities from the five runs.

- **Clearance** depends on the worst reading. Contamination only adds time, so the quiet maximum is at most the observed 87. The true clearance is therefore at least 13. This half of the argument is airtight.
- **Spread** is max minus min, and here the merge-manager's argument is incomplete. Contamination that lifted the *lowest* run would make the spread look narrower, not wider. That is the one way an unseen load could hide a firing rule. For the rule to fire with a quiet maximum of 87 or less, the quiet minimum would have to be 74 ms or less. That would be 9 ms below anything this board has read at sixteen: 84 and 90 on 2026-09-08, 83 to 87 here, and 87 in all five runs of the rail table's independent timing. Contamination of 9 ms or more would also have to have run on top of that quiet 74 without crossing 20 % CPU in any 5 s sample, and with load1 under 3.4. #533's record says what load does to this file: *"Load does not shave this file, it wrecks it"*. Loads of 22 to 30 put a 60 ms case at 429. A contamination big enough to matter and small enough to hide is not what this file has ever shown.

So, for "not reproduced, the cap stays", a monitored pass cannot be a false green in any way the readings leave room for. And the verdict changes nothing: the code, the cap and the budget all stay as #540 left them.

*Is it sound in the other direction?* **No, and that limits this ruling.** If load can only add time, a red or a thin clearance from an unheld run is exactly what load can produce. A 5 s monitor with a 20 % threshold can miss a burst between samples, and a missed burst inflates a reading. A monitored red therefore cannot tell a regression from a sibling. #533 already says so: *"a red taken outside them is not a reading and is not quoted"*. The ticket says it again: *"discarded … red or green"*. And step 3 is where a red leads: a qualified bisect over five commits, or `MAX_SAVED_LISTS` dropping to 8, which halves a user-facing allowance and edits `CONTEXT.md`, `backup/share.rs` and `perf.rs`. Both are changes that a false red would cost the product. Had these runs read 96 to 108, or had the clearance been within the spread, the PR would have to wait for five runs under a hold. The asymmetry is in the evidence, not in a preference: unseen load can make a reading worse but never better.

*Why not require new runs anyway.* It would cost five minutes of machine time and a dispatch pause, and the outcome is already known. The only reading the hold could change is a lower one, and a lower reading changes nothing in step 2's conclusion. Requiring the runs would re-decide #533's (a) as "the hold itself", when #533 calls the hold a means and gives its reason. It would also rank a promise above a record. The ticket's scope stays as written. Step 2 applied, nothing in the code moves, and the frozen surface is not touched.

*The wording.* The doc comment is the only durable record this PR leaves, and the next measurer reads it beside the 2026-09-08 table. Left as it is, *"Five runs that met them"* claims a hold that did not happen. The next reader would then take it as a held baseline and cite it as one. The fix is one clause.

**If you disagree, the cost of reversing this is:** small, and it moves nothing in the product. You can reopen #629 or file a follow-up for five runs under a hold. That is about five minutes of exclusive machine time, plus one more doc-comment edit if the held readings differ. If the held runs read the same, nothing changes. If they read a spread wide enough to fire #533's rule, the step to eight goes ahead as the ticket's step 3 describes, and nothing merged here is in the way. That is because this PR changed no code. The one thing a reversal cannot undo is this ruling's point about direction. Even under a stricter (a), a monitored red would still not be a reading.

## #627 — `under:` over the budget: where the stored copy of the parents lives

Ruled 2026-10-12. Comment:
<https://github.com/BFoerschner/knobas/issues/627#issuecomment-6120015673>

**The fork:** the `under:` benchmark (7de819d9) times the whole `Searcher::search` call at 100k items, with parents declared and present in payloads and the estate seeded. On `main` it reads p50 653 ms under load ~16. With the WIP commit (fbbfedc3, which builds the declarations once per statement and fences the walk with `materialized` and `offset 0`) it reads ~180 ms: coverage probe 28, scoped statement 85, `KNOWN_SQL` ~60. The budget is 100 ms at p90. The implementer offers three ways on. (A) A migration adds `parent_names text[]` with a GIN index to `sync.item`. The sync writer fills it through the Rust resolvers (`resolve_parents`, `resolve_names_as_parent`), and both `KNOWN_SQL` and the predicate become `parent_names && $values`. (B) No migration: keep the WIP and add a semi-join, an early stop and an all-unknown skip. The implementer's own estimate is 105–150 ms. (C) Cache the known names in `Searcher`. That breaks "unknown-token reporting answers exactly as before" and leaves the statement's 85 ms. Two more questions were put to me. Does #533's precedent (the budget stays and the scope gives way) make a narrower criterion the right answer? And under A, how do the SQL macros, the Rust resolvers and a stored copy stay in agreement when a declaration changes?

**Ruling:** **A, but the database keeps the stored copy, not the sync writer.** The ticket already answers whether a migration is allowed: *"If the fix needs a migration, it needs a §10.8 entry and Björn's sign-off before it merges."* So this ruling decides the migration's shape. It does not replace that sign-off. The implementer builds it now. The PR merges only after Björn signs off on #627, the way #594's did.

The shape:

1. **One migration, the next free number** (`0028` at `origin/main` `b95436cf`; take the number when the §10.8 entry is written). It creates:
   - `sync.parent_declaration (adapter_kind text primary key, payload_paths jsonb not null)`: each compiled-in adapter template's `payload_paths`, the same value `Searcher::with_payload_paths` holds today.
   - `sync.item_parent (entity_id text not null references sync.item on delete cascade, name text not null, kind text not null)`, indexed on `lower(name)` and on `entity_id`. Its rows are exactly the `(name, kind)` pairs that the two payload arms of `under::parent_names!` yield for an item today, in their original case.
   - The database code that fills `sync.item_parent`. A row trigger on `sync.item` (insert, and update of `source_id`, `kind` or `payload`) replaces that item's rows. A change to `sync.parent_declaration`, or to a `knobas.source_config` row's `kind` (insert, update or delete), recomputes every item of the sources it affects. The computation is the existing `knobas_core::declared_parent_names!` and `declared_names_as_parent!`, with `$decl` passed as an SQL expression that builds today's source-keyed declarations object from `knobas.source_config` joined to `sync.parent_declaration`. That is the same mapping `Vocabulary::declaring` makes in Rust.
2. **No Rust on the write path.** `knobas-sync`'s writer does not change, no adapter's `CURSOR_VERSION` moves, and no source re-syncs. `resolve_parents` keeps its current job and stays held to `declared_parents!` by `crates/knobas-core/tests/it/payload_paths.rs`. Under #594's ruling, the two-shape `at` branch still lives in only those two places.
3. **One function writes the declarations and builds the searcher.** At start-up the app writes `sync.parent_declaration` whole, from the same `Registry::builtin().templates()` it hands the searcher (`commands/search.rs`). It does this in one async function that does both jobs, so the stored copy and the in-memory copy can't differ. Tests build their searcher through that function too.
4. **Readers.** `parent_names!` becomes `sync.item_parent` joined to `sync.live_item`, plus the asset arm unchanged. It is still the one fragment that `KNOWN_SQL` and `PARENTS_SQL` share (#615). `corpus::LIVE_ITEM`'s `under` becomes an `entity_id in (select … from sync.item_parent where lower(name) = any(ud.v))` beside the estate arm. `KNOWN_SQL` and the predicate no longer walk the declarations. The path line keeps reading `declared_parents!` per result row, which is a few rows and unchanged.
5. **The WIP commit is dropped.** Keep 7de819d9 (the benchmark) and drop fbbfedc3. Its `@from`/`@candidates` arms add a second way into each parent macro in `knobas_core`, and its `materialized` and `offset 0` fences keep the cost down only for as long as the planner keeps behaving the same way. Under this ruling nothing on the query path walks the declarations, and the trigger needs only the existing `$decl` arm.

Criteria added to #627, beside its own:
- **Agreement, on real PostgreSQL (ADR-0013).** Seed both `at` shapes, a parent that only its children name, a mirrored parent, and an asset. `sync.item_parent` must equal the per-row macro walk (today's two payload arms) after each of: an insert, a payload update, a delete, a change to the stored declarations, and a source configured after its items were written. Mutation proof: removing the trigger's names-as-parent arm fails this test, and so does skipping the recompute on a declarations change.
- **The seeds stay raw.** `under.rs`, `suggest.rs`, `corpus_seam.rs` and `testing.rs` keep writing `sync.item` with plain `insert`s. The only change is how a test builds its searcher (point 3). If a test needs an extra fill call to stay green, report it as a finding. Don't add the call to make the test pass.
- **Backup.** A backup round trip answers `under:` and the `under:` suggestions exactly as the source database did, with no duplicate `sync.item_parent` rows. `sync.item_parent` is derived data, so the PR states whether it is dumped or rebuilt.
- **Write cost.** The PR records `seed_corpus(100_000)` time before and after, as the cost the trigger adds to the sync write path.
- The ticket's own criteria are unchanged: the benchmark fails on `main` and passes after the fix, with p50/p90 and the load average recorded. The mutation proof restores the per-row walk in `KNOWN_SQL` and in the predicate.
- **In the same PR:** a §10.8 entry for the migration in `docs/contract.md`, drafted to this ruling and **gated on Björn's sign-off on #627**, as the ticket says. ADR-0019, written by the implementer with status `proposed` until that sign-off. Suggested title: "under: reads a stored resolution of the declared parents, which the database keeps". It records the first trigger in the schema and the first stored value that depends on something outside its own row. This ruling, appended to `docs/decisions/2026-10-unattended-rulings.md`.

No glossary change. **Parent** and **Declared path** stay true: the adapter still declares, and a reader, now the database, still resolves the declaration.

**Reasoning:**

- **The budget can't move, and #533 points the other way.** Spec §14: *"Local search < 100 ms for ~100 k items"*. `tests/perf.rs`: *"The budget does not move"*. #533 kept `BUDGET_MS` fixed and lowered `MAX_SAVED_LISTS`, which its ticket had named as the thing that could move (*"confirmed or lowered by the measurement"*). #627 names nothing that can move. Its first criterion fixes the fixture (100k, parents declared and present, estate seeded) and the measurement (*"p90 under 100 ms for the whole `search` call"*). Narrowing either one moves the budget by another name. What #533 does settle is how much margin a pass needs: *"the cap is the largest count at which the measured board clears the budget on a quiet machine by more than the measurement's own quiet-machine spread"*. B's own estimate of 105–150 ms doesn't clear the budget at all.
- **C fails a criterion on its own terms.** A newly synced parent would read as unknown until the cache refreshed, and the criterion says *"answers exactly as before"*. It also leaves the 85 ms statement where it is.
- **A generated column can't do it.** `0027`'s `fts` worked because it depends only on `title`. A parent depends on the declarations, which are compiled into the binary, bound per query as `$1` and mapped to sources through `knobas.source_config`. A PostgreSQL generation expression may read only its own row, and it may not contain subqueries or set-returning functions, which the macros are made of. A row whose answer depends on something outside the row needs code that runs when that other thing changes. That is a trigger.
- **Why not have the writer fill it, as the implementer leaned.** In three cases it would break "answers exactly as before":
  1. *A declaration changes with no new sync.* A new adapter declaration or an app upgrade changes what every existing row answers to. Today the next query sees the change, because declarations are bound per query. A writer-filled column changes only when an item is re-emitted. Gitea's `sync.rs` re-emits a repo only *"when its `updated_at` differs from the one in the cursor"*, so the change would wait for a cursor bump and a full sync of every affected source. #612's ruling rejected an adapter-side fix for exactly that tie.
  2. *A row written by anything other than the writer.* Every search test seeds `sync.item` with a raw `insert` (`tests/it/under.rs:77`, `src/testing.rs:166`), and so does the perf fixture. A writer-filled column would be empty for all of them. The tests would then need a helper that copies what the writer does, and the wire from the writer to the column would never be witnessed (ADR-0013). A trigger fills the column for every path that writes a row, so the tests exercise the same code the sync does.
  3. *A restore*, for the same reason as case 2.
- **One implementation, so agreement holds by construction.** If the writer filled the column through the Rust resolvers, the Rust side would decide what `under:` matches while the SQL macros drew the path line. The agreement test would then be the only thing holding the filter and the display together. With the trigger, the same SQL macros decide the filter and draw the path line, and the Rust resolver keeps the role #594 gave it. ADR-0007's *"confined to one named statement"* holds: the payload read stays inside the macros, and the trigger is the one statement that calls them for `under:`.
- **A side table, not a `text[]` column.** #615 needs one fragment that yields `(name, kind)` in original case, because suggestions show `payout-service · repo` and `under:` must know exactly what the box offers. A lowercased array loses both the case and the kind, so suggestions would need a second read, and #615's single fragment would split. `sync.item_parent` is exactly that fragment's relation. A btree on `lower(name)` answers `= any($2)` directly. `sync.item` doesn't gain a column, and neither `sync.live_item` (whose column list `0012` fixes) nor the `0027` `fts` expression is touched.
- **Frozen surface.** This adds more than the implementer's single column: two tables, an index and the trigger code. A single column would add less DDL but fail a criterion in each of the three cases above. Ruling for the smallest surface still has to meet the ticket, and #612's ruling chose a migration over a smaller change that failed the spec in the case the spec describes, for the same reason. Transitive reach costs nothing extra: a Confluence chain is the array shape of `at`, which `declared_parents!` already expands into one row per ancestor, and asset subtrees stay with `corpus::estate_subtree!`.
- **Known change in a corner.** `Searcher::new` with no templates currently answers `under:` with nothing (*"the honest answer for a searcher nobody told about the adapters"*). Once the resolution is stored, it is the database's to give, so a bare searcher on a database whose declarations were stored would answer from them. The app always builds its searcher with templates, so app behaviour is unchanged. If a test pins the old answer, the PR names it and moves the test to "a database with no stored declarations matches nothing".
- **Ordering.** #629 also edits `tests/perf.rs` and the recipe's `expected` pin, so whichever lands second rebases, as #629 already says.

**If you disagree, the cost of reversing this is:** before merge, nothing beyond the branch's work: the implementer has the benchmark and can take any other route. After merge, it costs one migration that drops the two tables and the trigger code, with a §10.8 entry, plus restoring `parent_names!` and the predicate to the per-row walk (behaviour is identical, since the agreement test proves the two equal). The `under:` benchmark goes red again and #627 reopens. If you would rather have the implementer's writer-filled `text[]` column, the cost is the three cases above: an upgrade that changes a declaration needs a cursor bump and a full sync per source, and the search tests' seeds go through a fill helper rather than the path production uses. If you would rather have a narrower criterion, this ruling's first reasoning point says why that moves the §14 budget, which only you can do.

## #627 — the declaration lock deadlocks `delete_source`: no stored row depends on a config row

Ruled 2026-10-12. Comment:
<https://github.com/BFoerschner/knobas/issues/627#issuecomment-6120675136>

**The fork:** PR #630 builds migration `0028` to this issue's earlier ruling, and adds one thing that ruling did not ask for: an advisory lock on the pair `(627, 0)`. `sync.item`'s insert and update triggers take it shared. The triggers on `sync.parent_declaration` and on `knobas.source_config` take it exclusively. The lock closes a real race. Without it, `knobas-search`'s `it` failed 3 of 3 runs, because an item inserted while a declaration change was still uncommitted resolved against the old declarations, and the change's recompute never saw that item.

The reviewing merge-manager found that, as written, the lock deadlocks `delete_source` against a sync of the same source (`mm-630-deadlock.sh`). The sequence:
1. The run takes the shared key at its first batch flush and holds it until commit.
2. `config::delete`'s `DELETE` row-locks the config row, and its trigger then waits for the exclusive key.
3. The run's closing `update knobas.source_config set cursor` waits for that row lock.

The reviewer also measured `add_source` of an unrelated source waiting 3 s behind an in-flight sync.

The reviewer proposed keeping the exclusive key only for declaration changes. The item triggers would take `for key share` on their sources' `source_config` rows, and the `source_config` trigger would drop the lock. Two gaps would remain under that scheme: an update of `kind`, and a source inserted while its items are being written. The other options offered were per-source advisory keys, having `config::delete` take the lock in Rust, or accepting the deadlock as a residual in ADR-0019.

**Ruling:** none of these. **The stored rows stop depending on `knobas.source_config`.** No trigger goes on that table, so nothing there needs a lock. This amends one clause in point 1 of this issue's earlier ruling: that a change to a config row's `kind` (insert, update or delete) recomputes that source's items. That clause is what brought the lock onto `delete_source`'s path.

1. **`sync.item_parent` gains `adapter_kind text not null`.** For each item, the trigger stores the `(name, kind)` rows that each stored adapter's declarations yield for it, tagged with that adapter kind. It uses the same two macro expansions as now, with `$decl` bound per adapter as `{<the item's source_id>: <that adapter's kinds>}`. An item gets rows only for the adapter kinds whose declarations reach its kind. In practice that is one, because kinds are per adapter. Which adapter kinds to try for each item is the implementer's choice, and the agreement test holds it.
2. **The source's adapter is matched when the rows are read, not when they are stored.** `parent_names!` and `corpus::LIVE_ITEM.under` join `knobas.source_config s on s.id = i.source_id and s.kind = ip.adapter_kind`. That is the rule `Vocabulary::declaring` applies today, moved to read time. The next query then answers correctly, with nothing recomputed, for a source configured after its items were written, for a source that was removed, and for a source whose `kind` changed.
3. **No trigger on `knobas.source_config`.** `sync.source_config_changed` and its trigger are removed, and so is `sync.parent_declarations()` if nothing else calls it. `sync.parent_declaration`'s triggers recompute the items whose kind the changed rows' old or new declarations name. A truncate still recomputes everything.
4. **One lock remains, and only for declarations.** The item triggers keep `pg_advisory_xact_lock_shared(627, 0)`. `sync.parent_declaration`'s triggers keep the exclusive form, and keep the early return for a statement that changed no row. That is the race the lock was added for, and it stays closed. **Invariant, to be written in the migration and in ADR-0019:** the exclusive key is taken only by statements on `sync.parent_declaration`, and never by a trigger on a table that a sync run or a user command writes.

**Witness tests.** Each runs on real PostgreSQL. Each has a mutation proof in the PR: the mutant is applied to a committed baseline, its diff is printed, and the test dies.

- **Delete during a sync, in `knobas-sync`'s `it`, driving the real run.** Use `run_once` or the scheduler with a fake `Source`. ADR-0013 allows the trait-level fake here, and the database is real.
  - The fake emits at least `BATCH` items, so that one flush has gone through the item trigger. It then signals and pauses. Only after that signal does the test call `config::delete(&pool, id, false)`.
  - The test asserts that the delete returns while the run is still paused, within a deadline well short of the pause. It then releases the run and asserts that the run ends `Finished` and that neither side errored.
  - A second case does the same with `purge_items: true`, with a flushed batch made only of items new to the mirror.
  - The existing `a_purge_survives_the_run_that_was_in_flight_when_the_source_was_deleted` pauses before the run's first flush. That is why `just check` stayed green over the deadlock.
  - Mutants, both of which must kill the test: (a) a restored row trigger on `knobas.source_config` that takes `pg_advisory_xact_lock(627, 0)`, as the PR's current `source_config_changed` does; (b) the reviewer's variant, `for key share` on the batch's config rows in the item triggers.
- **The original race stays closed, in `knobas-search`'s `it` (`item_parent.rs`).** Two connections, ordered by `pg_locks` and not by sleeps. Run both orders:
  - (i) Connection A changes `sync.parent_declaration` and keeps its transaction open. Connection B inserts a `sync.item` of a declared kind. The test sees B waiting on the advisory lock (`granted = false`), commits A, then commits B.
  - (ii) B inserts first and keeps its transaction open. A's declaration change waits. The test commits B, then A.
  - After each order, the stored rows equal the per-row walk as the readers see it.
  - Mutants: the shared lock removed from the item insert trigger, and the exclusive lock removed from the declaration trigger. Each must kill the test. This test replaces the PR body's *"The lock is not witnessed by a test of its own."*
- **A search command never waits on a sync.** Open a transaction that holds the shared key, such as a run paused after a flush. While it is open, `Searcher::declaring` with the declarations already stored returns within a deadline. Mutant: `Searcher::declaring` always writes, and the trigger's no-row early return is removed. The test must die.
- **The existing agreement test keeps all seven steps:** insert, payload update, delete, declaration change, source configured after its items, kind changed, source removed. It compares what the readers return, join included, against the per-row walk bound to `Vocabulary::declaring`.
- **The benchmark and the write cost.** All four benchmark cases stay under 100 ms p90 with the read-time join. Measure the write cost again (the seed, and the parenting update) and record the new numbers beside the old ones.

**What happens to the remaining gaps:**

- *An update of `source_config.kind`* and *a source inserted while its items are being written* are **covered** by point 2. No stored row depends on the config row, so there is no order to keep.
- **A residual, recorded in ADR-0019:** a declaration change waits for every in-flight run to commit, and while it waits, PostgreSQL queues later item writes behind it. A run holds its transaction across its HTTP work (§10.6(c)), so the wait can be minutes.
  - It happens only when the stored declarations differ from the binary's. That is the first bring-up after an upgrade, or a second binary with different declarations on the same database. Bring-up writes the declarations before the first sync, so normally no run is in flight then.
  - It cannot deadlock, because the declaration writer holds no lock that a run takes (the invariant in point 4).
- **A finding outside #627, for the PR to report and the loop to triage.** `PURGE_ITEMS` deletes `sync.item` rows, and an in-flight run's `ITEM_UPSERT` has already row-locked the rows it updated. So `delete_source(purge_items: true)` already waits for a run that has re-written existing items to commit. This wait existed before `0028`, and this PR must not add to it. It is also why the second witness case flushes only new items.

**ADR-0019 (still `proposed`) must say:**
- The stored rows depend on the item and on `sync.parent_declaration`, and on nothing else. The source's adapter is joined at read time, because a trigger on `knobas.source_config` would put the sync's lock on `delete_source`'s path.
- The lock, what it orders, and the invariant in point 4.
- Under **Considered options**, the rejected schemes:
  - the exclusive key on a `source_config` trigger: deadlock, shown by `mm-630-deadlock.sh`;
  - `for key share` on the config rows: the delete waits for the run's commit, against #127;
  - per-source advisory keys: the same deadlock, per source;
  - the lock taken in Rust by `config::delete`: Rust on the write path, and the delete still waits;
  - accepting the deadlock: PostgreSQL aborts one side, either a run's whole transaction or the user's delete.
- The residual above, with its reason.
- The first-trigger paragraph's *"its source's configured adapter kind"* and the summary's *"when a source's adapter kind changes"* are removed, because neither is stored any more.

**The §10.8 entry (still GATED) must:**
- add `adapter_kind` to `sync.item_parent`'s shape;
- remove the `knobas.source_config` trigger, and `sync.parent_declarations()` if it goes;
- say that readers join `knobas.source_config` on `id` and `kind = adapter_kind`;
- state the lock and its invariant;
- list the three new tests under **Witness**, each with the mutant that kills it;
- keep its claim that every `knobas` table is untouched. Today the entry makes that claim while the migration puts a trigger on `knobas.source_config`. With this ruling the claim becomes true.

Append this ruling to `docs/decisions/2026-10-unattended-rulings.md` under #627. No glossary change.

**Reasoning:**

- **`delete_source`'s timing is already settled, and both lock-based fixes change it.**
  - #127's ruling keeps *"same command name, same parameters (`id`, `purge_items`), same return, same non-blocking timing"*. Among the things that *"sends this back to Björn"*, it lists *"Any shape that changes what `delete_source` promises over IPC: cancelling the run, blocking until it stops"*.
  - A run takes the shared key at its first 500-item flush (`BATCH`) and holds it until commit. §10.6(c) records that the commit comes after the run's HTTP work.
  - The PR's scheme makes the delete wait for that commit, and then deadlocks it.
  - The reviewer's scheme removes the deadlock but keeps the wait. In the reviewer's words, *"a delete or key change of the row now waits on that row lock instead"*. Both scripts committed on both sides because the delete waited out the run's sleep.
  - Per-source keys deadlock the same way, per source.
  - A lock in `config::delete` is Rust on the write path, which point 2 of the earlier ruling rules out, and the delete still waits.
  - Accepting the deadlock means one side is aborted after `deadlock_timeout`.
  - None of these can be ruled here without reopening #127, and #127 sends that question to Björn.
- **The dependency that needs the lock is not needed.** The source-to-adapter mapping is one row per source, in a table of a handful of rows. Joining it at read time adds a hash join to the index lookup the benchmark already makes. Storing it puts a trigger on the table that every source command writes. Without the dependency, both of the reviewer's remaining gaps disappear, and `add_source` has nothing to wait on.
- **Frozen surface.** This adds one column, and removes a trigger on a `knobas` table along with its function. That is smaller than the PR's surface and smaller than the reviewer's variant.
- **Witness (ADR-0013).** The deadlock got past the gate because the one test that deletes during a run pauses before the run's first write. The two endpoints were tested, but the interaction between them was not. The new test drives the real run's statement order. If that order changes later, for example when §10.6(c) moves the transaction boundary, the lock is tested against the new order, not against a script's copy of the old one. The race test orders its connections by `pg_locks`, because a sleep cannot make a test pass (#154's bar).
- **Scope unchanged.** The ticket, its criteria, points 2–5 of the earlier ruling, and the reader fragment that #615 shares all stand. The only change is where a stored row is matched to its source's adapter.

**If you disagree, the cost of reversing this is:** before merge, nothing beyond the branch's work.
- **The reviewer's `for key share` scheme instead:** `delete_source` waits for an in-flight run of its source to commit. That reverses #127's "same non-blocking timing" on a frozen command and needs a §10.8 entry of its own. The `adapter_kind` column and the read-time join come out, and the two `kind` gaps go into ADR-0019 as residuals.
- **Accepting the deadlock instead:** ADR-0019 records that deleting a source while its sync has written a batch aborts either the run or the delete.
- **After merge:** one migration that drops `adapter_kind` and restores a `source_config` trigger in the locking shape you choose, with a §10.8 entry. The readers lose the join.

## #632 — a purge that skips what a run holds: two paths nobody re-purges

Ruled 2026-10-12. Comment:
<https://github.com/BFoerschner/knobas/issues/632#issuecomment-6121658405>

**The fork:** PR #635 makes `PURGE_ITEMS` take `for update of i skip locked` / `for no key update of e skip locked` and leaves the skipped rows to the scheduler's re-applied purge. The merge-manager found two paths where nothing re-applies it: (1) *Load demo data* calls `run_once` directly, so the scheduler has no claim on that run; on `main` a delete during it already left the rows it inserted, and with the PR it also leaves the rows it rewrote. (2) Migration `0028`'s recompute inserts `sync.item_parent` rows whose foreign key takes `KEY SHARE` on `sync.item`, which conflicts with `for update of i skip locked`, so a purge during a recompute skips those items, and with no run in flight nothing purges them again. Options: (A) record both as residuals and merge, (B) close the demo case in `demo.rs` in this PR, (C) wait only on the paths nobody re-purges.

**Ruling:** **(A) for this PR, with one follow-up ticket (#636) for path 1.** Path 2 stays a residual with no follow-up; (C) is out.

- `delete_source`'s doc goes from three residuals to five. The fourth: a source deleted during *Load demo data* keeps every row that load writes, inserted or rewritten; #636 is the fix. The fifth: a purge during `sync.parent_declaration`'s recompute skips the items the recompute reached. Bring-up writes the declarations before `SourcesState` exists, so `delete_source` answers `not_ready` through the upgrade recompute; what remains is a later `searcher()` recompute after bring-up's write failed, or a second binary with different declarations (ADR-0019's residual). "Eventual" stays, and covers only runs the scheduler started.
- `PURGE_ITEMS`'s "`skip locked` skips nothing" sentence is qualified with the fifth residual; its demo paragraph points at #636.
- No glossary change, no ADR, nothing on §10.8.

**Reasoning, in short:** #154 puts accepted residuals on `delete_source` where the reader meets them. #127 and #632 forbid the purge waiting for a run, which rules out (C). The demo path never had the eventual guarantee (#127's measurement covered the scheduler's `runs` map only), so no run's guarantee changes; the PR widens the demo gap, hence #636 rather than a residual alone. (B) touches `demo.rs`, a second command's destructive path, and needs #154's existence guard and its own witness: a ticket, not a merge-time edit. The cheap close for path 2 (`for no key update of i skip locked`) does not work: the `gone` delete still needs `FOR UPDATE`, which conflicts with `KEY SHARE`, so the purge would wait for the recompute — the wait #632 removes.

## #633 — live suites run against real products started locally, on demand, one at a time

Ruled 2026-10-12 by **Björn himself**, in the loop session, not by the deputy. Recorded in the
issue body of #633: <https://github.com/BFoerschner/knobas/issues/633>

**The fork:** since 2026-09-06 the three JVM products a live suite runs against (TeamCity, Jira,
Confluence) lived on three Hetzner servers, kept up around the clock and reached through an SSH
tunnel, with the Atlassian licences renewed on a timer there; Gitea stayed always-on in the
notebook's Docker. Keep that estate for live suites, or run the products locally?

**Ruling:** locally, on demand, one at a time. In Björn's words: *"Since we focus on one source now
at a time we don't have to run them 24/7, so we can just spin one up when we have to test."* Each
live recipe starts its product (and what it needs: TeamCity's VCS roots need Gitea; Jira and
Confluence their PostgreSQL), waits, seeds it, runs its suites and stops it. Gitea becomes
on-demand too. A second JVM product is refused while one is up, naming it.

**Reasoning:** the loop works on one source at a time, so a product is needed for the length of
one ticket's live runs and not between them; the servers' standing cost and the tunnel's moving
parts bought availability nobody was using. The notebook's 8 GB Docker VM holds one JVM product,
which is exactly what one source at a time asks of it. ADR-0013 is unchanged: the witness is still
the real product, now in the notebook's Docker instead of on a server.

**If you disagree, the cost of reversing this is:** small while the servers exist. `source
testenv/hetzner/env` and `testenv/hetzner/tunnel up` still work for the seeds; the live recipes
would need their Hetzner branches back (`git log -- justfile` for #633's diff) and `testenv/live`'s
refusal of that shell lifted. After #634 decommissions the servers it means re-provisioning them
(`testenv/hetzner/provision.sh`).

## #634 — decommission the Hetzner servers

Ruled 2026-10-12 by **Björn himself**, in the loop session, not by the deputy. Recorded in the
issue body of #634: <https://github.com/BFoerschner/knobas/issues/634>

**The fork:** once #633 runs every live suite against products started on the notebook, the three
Hetzner servers (`knobas-teamcity`, `knobas-jira`, `knobas-confluence`, about €53 a month) serve no
live suite. They are still a fixture: `testenv/hetzner/estate.json` was the estate the Import,
`--demo` and `just estate-live` are tested against (#427: *"the estate knobas models **is** this
infrastructure"*). Keep them, or decommission them?

**Ruling:** decommission them. The estate file is replaced by the estate that exists afterwards —
the notebook, its OrbStack Docker engine and the on-demand product containers from #633 — and
everything under `testenv/hetzner/` that served only the servers is retired with its docs. No agent
runs `hcloud`, opens ssh to the servers or switches hcloud contexts; the teardown is Björn's, from
the checklist on the PR that closes #634.

**Reasoning:** the loop works on one source at a time, so a product is needed for one ticket's live
runs and not between them (#633); the servers' standing cost bought nothing a live suite still
uses. ADR-0013 is unchanged: the witness is the real system, and the estate it witnesses against is
now the one on the notebook.

**If you disagree, the cost of reversing this is:** small until the teardown, and a
re-provisioning after it. Before it, revert the PR that closed #634. After it,
`testenv/hetzner/provision.sh` from git history stands three new servers up with new
`hcloud_id`s and addresses, and the estate file, the Kuma monitors and the tunnel follow by hand.

## #634 — the hcloud importer's witness once the servers are gone

Ruled 2026-10-12 by the deputy for Björn, flagged for his review. Comment:
<https://github.com/BFoerschner/knobas/issues/634#issuecomment-6122037668>

**The fork:** ADR-0015's consequence made the hcloud importer's witness *"a file generated from the
live hcloud … previews as all-known against `testenv/hetzner/estate.json`"*, and
`estate_live.rs::the_real_estate_is_already_in_the_tree` was its only real witness;
`tests/it/assets_ipc.rs` replays a recording, which *"certifies shape and nothing else"*. Once the
servers are deleted the project holds nothing to compare, while the importer still ships in the
Import's chooser. (A) retire the hcloud half, a recording being the only cover; (B) keep the project
and a read-only token and assert an empty answer; (C) keep one smallest server; (D) remove the
importer.

**Ruling:** **B, extended so that it never stops reading servers.** The hcloud half of `just
estate-live` reads whatever the real project holds and checks the importer's own round trip on it:
produce, produce again with a landing at the top of the estate, apply, produce a third time — which
must come back `Ready`, nothing new, and preview all known with no changes. On an empty project it
shrinks to an empty `Ready` file and an empty preview. The expected server count is declared and
required (`ESTATE_LIVE_HCLOUD_SERVERS`); a different count is red, and no variable skips the half.
A PR that touches the hcloud producer owes a run at `=1`, with a server Björn creates in the knobas
project and deletes after the run — never an agent, never a recipe. This PR's own run was to be
taken before the teardown, at `=3`; when the implementer ran it, the project the `.env` token
reaches already answered zero servers (the count gate went red), so the PR carries the `=0` run
and leaves the parse witness open for Björn's `=1`. The teardown checklist starts by keeping the project and replacing
`.env`'s token with a read-only one, and ends with `ESTATE_LIVE_HCLOUD_SERVERS=0 just estate-live`
posted on #634. ADR-0015 gains a dated strike-through consequence, `CONTEXT.md` **Importer** a dated
amendment, and #509's §10.8 entry a dated line; no new ADR. Not A, not C, not D.

**Reasoning, in short:** A covers a shipped importer's decode with a recording, which ADR-0013
forbids for something the real API can produce for a few cents an hour — a new exception to
ADR-0013 would be Björn's sentence, not the deputy's (#498's ruling). C re-decides what Björn
decided. D changes spec #491 story 61. B alone is a real floor (the endpoint, the auth, the envelope
and the pagination) but would make a server's parse unwitnessable for good; the round trip keeps
that floor and makes the full witness one stood-up server away, which is #633's *"spin one up when
we have to test"* applied to the one real system that is not a container. It also witnesses spec
#491 story 64 directly: a re-import previews as already in the tree. The count is declared because
a suite that skips on an unset variable is one libtest counts as a pass (#351). The parse run is
Björn's because #634 keeps every agent away from hcloud writes. The amendment is ratified and
flagged for Björn's review on the #500 precedent: no migration, no IPC, no new ADR.

**If you disagree, the cost of reversing this is:** low until the servers are deleted, moderate
after. Before the teardown, C keeps one server (count `=1`, the server back in the estate file with
its `hcloud_id`), and A deletes the hcloud half with an ADR line recording the end of its live
witness — your sentence, being an ADR-0013 exception. After the teardown, C means creating a server
with a new `hcloud_id`. Nothing in the producer depends on this: the test and two dated record lines
are all that change. If the project itself is to go, the token goes with it and the hcloud half
becomes A by default, with the exception above.

## #598 — classic Data Center epic membership: the adapter stores it as epic

Ruled 2026-10-12. Comment:
<https://github.com/BFoerschner/knobas/issues/598#issuecomment-6122336475>

**The fork:** this issue says "Jira declares `fields.parent` as the parent of `ticket`", and spec #591 says "Jira: ticket → epic or parent issue via `fields.parent`". The seeded Jira 10.3.24 is a classic Data Center project. It keeps epic membership only in the Epic Link custom field, which holds a bare key string under an id minted per instance (`customfield_10108` on this run). `fields.parent` is absent from every issue. The implementer's live run (`issue-598-jira-live-1.log`) shows both halves: `epic_membership_lives_in_the_epic_link_field_and_parent_is_absent` passes with "customfield_10108 = PAY-200 on the epic's children; fields.parent absent throughout", and `passes_the_contract_battery_against_the_seeded_server` fails clause 6's misspelling check for "parent of kind \"ticket\"". So the live criterion cannot pass, the battery goes red, and `under:PAY-200` would not reach PAY-200's five children on the one flavour the adapter supports. The options are (a) add a sub-task under PAY-200 to the Tidewater fixture, (b) have the adapter store the Epic Link value as a knobas-written `epic: {"key": …}` object and declare it as a second parent candidate after `fields.parent`, or (c) keep `fields.parent` alone and accept the red battery or loosen clause 6. I was also asked whether (b) contradicts §3a, whether a `ParentPath` can name a per-instance field, whether (a) and (b) combine, and whether any of this is a frozen-surface touch.

**Ruling:** **(b).** No §10.8 entry is owed, and the ticket's criteria stand as written, with the additions below. Flagged for Björn's review because it is a second departure from §3a's verbatim payload.

1. **The adapter stores the epic.** When `epic_link_field` is configured and the issue's value for it is a non-empty string, `map` adds a top-level `epic` object, `{"key": "<that string>"}`, to the stored payload. It goes beside `fields`, not inside it, and only where the record has no `epic` key already, the way Gitea's `with_repository` uses `entry(…).or_insert_with`. A null, absent or non-string value adds nothing. Nothing else in the record changes, and `fields.<customfield>` stays as Jira sent it. Never write into `fields.parent`. That key is Jira's sub-task relation, and faking it would make the live test that says it is absent untrue.
2. **The descriptor declares two candidates for `ticket`, in this order:** `ParentPath { kind: "ticket", at: ["fields", "parent"], names: [["key"]] }`, then the same with `at: ["epic"]`. The first one that resolves wins, so a sub-task's direct parent beats an epic link. Both name `key`, which every Jira issue carries at its root, so the parent-record rule from the 2026-10-11 ruling on #595 holds for both.
3. **`CURSOR_VERSION` in `crates/knobas-source-jira/src/cursor.rs` goes from 2 to 3.** Its doc comment says why, citing this ruling, #610 and #345. Jira is `full_sync_exhaustive`, so the one full sync re-emits every issue. Gitea's commit leftovers have no Jira equivalent.
4. **The live battery's healthy source is configured the way the Add-source dialog leaves it,** with the seed's `epic_link_field` from `seed-state.json`, as `epic_membership_lives_…` already does. The doc comment says why: #381 records that the configured arm is "what every source created through the Add-source dialog ends up in". It also says that an unconfigured classic source would fail clause 6, correctly, because every `under:` read on it misses.
5. **Criteria added to this issue, beside its own:**
   - Mapping golden tests. A configured source with an Epic Link value stores `epic: {"key": …}`. A null value, a missing value and an unconfigured source store none. In every case the rest of the payload equals the raw record.
   - Live Jira suite. `under:PAY-200` returns exactly the seed's five children (PAY-219, PAY-228, PAY-231, PAY-236, PAY-240) and PAY-200 itself. `passes_the_contract_battery_against_the_seeded_server` runs green, and the transcript goes in the PR body.
   - `epic_membership_lives_in_the_epic_link_field_and_parent_is_absent` stays as it is. Its `fields.parent` assertion is still true, and its doc comment gains one sentence saying the adapter now stores the relation as `epic` (this ruling).
   - A mutation proof for `epic`: drop the write in `map`, and the live `under:` test and the battery both die. Run each target as a separate invocation or with `--no-fail-fast`, per the 2026-10-12 ruling on #597.
6. **Docs, written by the implementer in the same PR.** ADR-0018's consequences gain one bullet after the Gitea one: *"A classic Data Center Jira keeps epic membership in a custom field whose id differs on every instance and whose value is a bare key, which no adapter-wide declaration can read. So when `epic_link_field` is configured the adapter stores it as `epic: {key}` beside the record (#598). This is the second departure from §3a's verbatim payload, for the same reason as Gitea's."* Append this ruling to `docs/decisions/2026-10-unattended-rulings.md` after the last section, as `## #598 — classic Data Center epic membership: the adapter stores it as epic`. No glossary change. **Parent** already says "the epic or parent issue for a ticket … it comes from the source", and both stay true.

**Reasoning:**

- **Nothing on the record settles this, but the spec's premise is known to be false, and the record says so.** #297, 2026-09-03: "`fields.parent` is absent on every issue of the classic projects PAY and OPS … epic membership lives only in the Epic Link custom field". The live test's doc comment adds that "mockd's two-spellings convenience is what hid it", because mockd deviation 13 serves the epic as `fields.parent`. #591's "via `fields.parent`" was written against that mock picture. What the spec asks for is story 21, "`under:PAY-100` to return the tickets in epic PAY-100", with "the epic or parent issue" in **Parent** and "a ticket under an epic" in ADR-0018. `fields.parent` is the means, and the live run shows the means does not reach the end on the supported flavour, which per the descriptor's `flavor` schema is Data Center only. (b) keeps the literal declaration. The golden criterion "`ticket` declares `fields.parent` as parent, answered to by key" still holds, and (b) adds the candidate the criterion cannot be met without.
- **(b) has a precedent in ADR-0018 itself.** Its third consequence: "A Gitea commit's raw payload does not name its repository, so the adapter has to store it for a declared path to read." Gitea's `map.rs` calls `with_repository` "the one departure from §3a's 'payload = the raw record verbatim', and the one ADR-0018 asks for by name". The #595 §10.8 entry calls it "ADR-0018's sanctioned departure" and says `crates/knobas-source-gitea/**` "is not frozen". Jira has the same problem for a different reason. The raw record names the epic, but in a place no declaration can read. The fix has the same shape: one object, in the source's own spelling, added beside a record that is otherwise verbatim. ADR-0018 named only Gitea because only Gitea's gap was known on 2026-10-11, so the ADR gains a bullet. That is a recorded application of the ADR, not a new decision, and it is flagged for Björn.
- **A `ParentPath` cannot name a per-instance field, and making it able to would be the largest frozen touch on offer.** Declarations are per adapter kind, not per source. The descriptor is `descriptor_template()` with only `id` and `name` replaced, `Searcher::with_payload_paths` keys them by `template.adapter_kind`, and since #627 they are stored as `sync.parent_declaration (adapter_kind text primary key, payload_paths jsonb not null)`. The value is also a string, and clause 6 refuses an `at` that "leads to a string" because `at` must reach an object. Reading the field in place would need a per-source declaration form, a migration on `sync.parent_declaration`, a new `at` shape and a clause 6 change. That means §10.8 entries and Björn's #146 gate. (b) touches only `crates/knobas-source-jira/**`, which is not on the frozen list. The `KindPaths` shape does not change, only Jira's values, as with #597, which needed no entry. #627's trigger recomputes `sync.item_parent` on a payload update, so no migration is needed either.
- **Clause 6 passes under (b) by its own text, not by a loosening.** Every candidate shares one slot, `evidence.entry((&item.kind, field::PARENT))`. On the classic instance `epic` resolves, so the unresolved `fields.parent` is excused. That is #301's tolerance for "an instance uses one of them", the same one the 2026-10-12 ruling on #595 applied to names. A next-gen instance gets the mirror image. Neither candidate can certify a misspelling of the other, which is a known limit and the reason the live `under:` test is the witness (ADR-0013).
- **(a) is rejected, and it is not combined with (b).** A sub-task under PAY-200 would turn the battery green while `under:PAY-200` still missed every story, task and bug in the epic on the flavour users run. The witness would certify the path classic users never take, which is the opposite of ADR-0013's purpose. It would also change the shared Tidewater fixture, which the exact-set live assertions, mockd and the demo all read. That is a wider change than (b) and outside this ticket. Without it, the `fields.parent` candidate has no live witness on this instance, only its golden test. That is what #301's tolerance accepts. If Björn wants it witnessed live, that needs a fixture ticket, and it is noted below.
- **(c) is rejected.** A red battery on a merged adapter breaks the rule the 2026-10-12 ruling on #597 quotes ("A PR at the real systems' edge carries its live run"), and only Björn can change what clause 6 asserts (#146). It would also leave story 21 unmet.
- **Scope.** The precedent from the 2026-10-11 ruling on #595 applies: "a change that a ticket's own criterion cannot be met without belongs to that ticket". The live criterion cannot be met without (a) or (b), and (b) is the one that also meets story 21. The cursor bump is #610's lesson, and leaving it out would leave every upgraded install's tickets without `epic` until each one changes.

**Flagged for Björn, not ruled:**
- (i) Contexts have the same blind spot. `recorded_parent_key!` in `crates/knobas-core/src/context.rs` reads `"i.payload->'fields'->'parent'->>'key'"`, so on a classic Data Center Jira a promoted epic seeds no children. Once `epic` is stored, that read could take it as a second spelling. That belongs to the triage, not to this ticket.
- (ii) `under:` coverage answers from the declaration, so an unconfigured classic source reports `answered` while it can match nothing. The connection note ("found but not configured: epic membership is not mirrored") is the only place that says so.
- (iii) A sub-task in the Tidewater fixture would give the `fields.parent` candidate a live witness. Nothing is filed for it.

**If you disagree, the cost of reversing this is:** low before the PR merges: drop the `epic` write, the second candidate, the cursor bump, the battery config line and the ADR bullet. The ticket then has no way to meet its live criterion. You would have to strike that criterion, or rule (a) and file the fixture change, or accept a red battery under #146. After merge, it costs one more `CURSOR_VERSION` bump so stored payloads lose `epic`, plus reverting the descriptor and the ADR bullet. There is no migration and no frozen surface to undo. The real cost is story 21: on classic Data Center, which is the only flavour the adapter supports, `under:<epic>` returns nothing again. If you would rather declare the custom field in place, that means a per-source declaration form, a migration and a clause 6 change, all at your gate. This ruling chose the change that needs none of them.

## #642 — `under:` coverage: read off the declaration, or measured on the corpus

*Björn's own ruling, not the deputy's.* The deputy left the fork unruled ([comment](https://github.com/BFoerschner/knobas/issues/642#issuecomment-6123097681)), because ruling either way reversed settled text: the #595 entry's "read off the declaration" against **Coverage**'s "measured on the corpus".

**The fork:** `under:` coverage reported `answered` for every source whose adapter declares a parent, read off `declares_a_parent`. A classic Data Center Jira with no `epic_link_field` stores no `epic` (#640), and none of its issues has `fields.parent`, so `under:PAY-200` could reach only PAY-200 itself while coverage said the source answered. The options were (1) measure the verdict on the corpus, (2) keep the declaration and store a note on the source, (3) leave it and record the gap. Option 2 was already out: Björn's grilling on #326 left a stored per-source configuration gap unbuilt, and **Connection note** says the Epic Link clause is "never stored".

**Ruling** ([comment](https://github.com/BFoerschner/knobas/issues/642#issuecomment-6123571497), relayed by the loop orchestrator): *"option 1 — measure it on the data. A source answers `under:` only if something it synced actually names a parent. Asked with the cost stated ("likely needs migration 0029 — a column on the parent table signed off today — plus a §10.8 entry"), he chose it, so the migration is allowed and this comment is his sign-off on it; the §10.8 entry and the ADR-0019 consequence it brings record this comment. The deputy's reading of option 1 (comment above) becomes the ticket's criteria."*

**A second sign-off, on the implementer's fork** ([comment](https://github.com/BFoerschner/knobas/issues/642#issuecomment-6124188649)): with the arm alone, two sources on one adapter, one naming parents and one not, cost the second about 150 ms per `under:` keystroke at 100k items, because the probe reached a child row's source only through `sync.item`. Copying `source_id` onto the row brought that to about 0.04 ms. Björn: *"Yes, add source_id"*.

As built (PR for #642): migration `0029` adds `sync.item_parent.arm` (`'child'` or `'parent'`) and `sync.item_parent.source_id`, restates `sync.resolve_item_parents` to write them, and adds a partial index on `(source_id, adapter_kind)` over the child rows. `coverage.rs` answers `under:` from an `exists` over a source's child-arm rows under its configured adapter, over all its kinds. The launcher's reason becomes "nothing it has synced names a parent". The #595 §10.8 entry's sentence is marked replaced, and a new #642 entry records the meaning and the migration. ADR-0019 gains a consequence bullet.

**Reasoning (the deputy's, which Björn adopted):** #141 put the answer on the response because "the honest fact is about the corpus, not the source". **Coverage** lists *capability* as a word to avoid, and `declares_a_parent` was that static claim. #591 story 32 asks that a source that can't answer reads as "can't answer" and not "nothing there", and an unconfigured classic Jira read as "nothing there". The spec's clause ("a source with no kind declaring a parent is reported as having nothing the filter can match") holds under a measurement too: such a source has no child row.

**If you disagree, the cost of reversing this is:** restoring the declaration verdict in `coverage.rs` and the old reason string is a small code change. `0029` stays, since migrations are never edited; its column and index would then go unread until a later migration drops them.

## #643 — a live witness for Jira's fields.parent: a sub-task the test creates and deletes

Ruled 2026-10-12. Comment:
<https://github.com/BFoerschner/knobas/issues/643#issuecomment-6123076260>

**The fork:** since #640, Jira's `ticket` declares two parent candidates: `fields.parent`, then `epic`. On the seeded Jira 10.3.24 only `epic` has a live witness. The seeded `PAY` and `OPS` projects are classic, `fields.parent` is the sub-task relation there, and `fixtures/tidewater/work.json` has no sub-task. Clause 6 gives both candidates one evidence slot (#301), so the battery passes while `fields.parent` resolves on no issue. Only the golden test and the search seam check that candidate, and both run on hand-written shapes. The 2026-10-12 ruling on #598 left this open: "If Björn wants it witnessed live, that needs a fixture ticket." The body offers three options: (1) a sub-task in `work.json` and the seed, which moves every exact-set assertion and the frozen mockd crate and needs a demo cursor bump; (2) a sub-task the live test creates, asserts on and deletes, at the cost of the live suites' shared budget; (3) no live witness, `wontfix`.

**Ruling:** **(2).** It needs no decision from Björn. It touches no frozen surface, does not change `work.json`, mockd, the seed or the demo, and adds no new kind of write to the Jira suite. #643 goes to `ready-for-agent` with the body below. It is not blocked, because #640 has merged.

1. **One new test in `crates/knobas-source-jira/tests/live_jira_seeded.rs`.** It calls `clear_leftovers` first. It then creates a Sub-task under a seeded non-epic issue (PAY-219, a Task in PAY-200) through `POST /rest/api/2/issue` with `parent: {key}`, marked with `LITTER_LABEL`. It runs a full sync with the source configured from `seed-state.json`'s `epic_link_field`, as the battery's source is. It asserts three things. The stored sub-task's `fields.parent.key` is `PAY-219`. `resolve_parents` over the descriptor's own declaration gives exactly one parent, kind `ticket`, names `["PAY-219"]`. `under:PAY-219` gives exactly PAY-219 and the sub-task, computed the way `under_the_seeded_epic_returns_its_five_children_and_the_epic` computes `under:PAY-200`. The test does not require an `epic` value on the sub-task. The doc comment records whether this instance put one there. Either way, "exactly one parent, PAY-219" is the right claim, because the first candidate that resolves wins.
2. **A checking guard that deletes the sub-task**, in the shape of `Labeled`. Its `Drop` deletes the issue on a thread with its own runtime and checks the result rather than assuming it. It then polls `/search` until the key is no longer named, within `INDEX_BUDGET`. The poll is there because `clear_leftovers`' own comment says the search index can still name an issue that has already been deleted. A killed run's sub-task is already handled: `clear_leftovers` deletes "an issue the seed did not create", and every exact-set test calls it first.
3. **No change to `work.json`, `testenv/seed-atlassian-content.sh`, `seed-state.json`, `crates/knobas-mockd/**`, the demo or its cursor.** The template's issue type scheme already has Sub-task: the seed's header says "Improvement, Task, Sub-task, New Feature, Bug and Epic". If the instance refuses the create anyway (sub-tasks turned off, or no create screen for the type), the implementer comes back with a `**Fork:**` and does not edit the seed.
4. **Docs.** The suite's module comment, under *What this file writes, and what it takes away*, gains the sub-task. The implementer appends this ruling to `docs/decisions/2026-10-unattended-rulings.md` as `## #643 — a live witness for Jira's fields.parent: a sub-task the test creates and deletes`. No glossary change and no ADR. **Parent** already says "the epic or parent issue for a ticket", and ADR-0018 is unchanged.

**Reasoning:**

- **This was settled on 2026-09-08, and this ruling applies that decision.** Ruling (1) on #498 covered the same class of gap, a live witness that can see only one direction. It turned the gap into a ticket and gave ADR-0013's reason: *awkward to reproduce* is not *cannot produce*: "if the real instance can be driven into the fault by any seed, flag or clock, it is witnessed live." It continued: "narrowing is not a fault a real Jira will never produce, it is what a real Jira workflow does all day." Sub-tasks are the same: a classic project does this every day. #522, the ticket filed under that ruling, also chose this ticket's shape. It was "additive" with "PAY's restore point … untouched", and its live check "files a throwaway ticket in $NARROW_KEY and deletes it again" (the seed's header). It left `work.json` and mockd alone. Spec #591's testing decision asks the same thing of every adapter: "each adapter's mapping golden tests, plus its live suite against the real instance … that the stored payload names it." So "is a live witness wanted" already has an answer. Option (3) contradicts both.
- **The #598 ruling did not reserve this question for Björn.** It rejected a sub-task *as #598's fix*, because a green battery would have hidden the missing `epic`. It never ruled on a witness for the `fields.parent` candidate itself. "If Björn wants it witnessed live, that needs a fixture ticket" was written while the only option on the table was a change to the shared fixture. Option (2) is not a fixture change. The #595 ruling on evidence slots took the same position: the one-slot tolerance stands, and "each adapter's live suite is what witnesses that every declared name resolves". This ticket supplies that check for Jira's first candidate.
- **(1) is rejected because it is the largest change on offer.** It moves the exact-set assertions in `live_jira_seeded`, `atlassian_live` and the demo. It needs mockd to serve a sub-task, against "No route is added — this crate is frozen" (deviation 13) and ADR-0013's "mockd is frozen and deprecated: … nothing new". It also needs a demo cursor bump. All of that buys one sub-task that (2) gets without touching any of them.
- **Since #633, the "shared budget" cost of (2) is one more create and delete of the kind the suite already does.** `atlassian_live` already files a ticket through `WriteOp::CreateTicket` on every run, and `clear_leftovers` already deletes any issue the seed did not create. The `PAY` key counter moving forward is already accepted: "nothing may assume the fixture's keys are the highest ones" (`testenv/README.md`). `just jira-live` runs every suite with `--test-threads=1`, and every exact-set test and the battery call `clear_leftovers` before their baseline, so clause 2 never sees the sub-task. When the recipe starts Jira itself, `live_session jira` removes Jira and its volumes at the end. Under `just live-up jira`, the guard and `clear_leftovers` put the corpus back. One owner at a time already covers this suite because it writes.
- **The test witnesses the wire, which the golden test cannot.** `BASE_FIELDS` has to request `parent` for `fields.parent` to reach the payload at all. A golden test on a hand-written payload passes even if `parent` is dropped from that list. The new live test fails.

As built (PR #651): `a_sub_task_names_its_parent_in_fields_parent_and_under_reaches_it` and the `SubTask` guard, in `live_jira_seeded.rs`. On Jira 10.3.24 the sub-task's Epic Link came back null, so `fields.parent` is the only candidate that resolves for it. One sentence above did not hold on the tree: mockd also sees both mutants. It serves `fields.parent` (deviation 13) and honours the `fields` parameter, so dropping `parent` from `BASE_FIELDS` fails three mockd tests and a `sync.rs` unit test, and misspelling the candidate fails the mockd battery's clause 6. What stays true is that this test is the only check against real Jira. The new test's line in `test-inventory.txt` is the one change outside the two paths the ticket named; the repo's inventory rule requires it.

**If you disagree, the cost of reversing this is:** trivial before an implementer starts: close #643 `wontfix`, which is (3), and the golden test and the search seam stay the only checks. After merge, it is one test and one guard to delete from a file that is not frozen, plus one decisions entry. Nothing is stored, migrated or frozen. If you want the sub-task in the fixture instead, which is (1), you take on mockd's freeze and the exact-set moves above, and that is your call under ADR-0013.

## #664 — the mockd battery and the configuration view: a wrapper, not a route

Ruled 2026-10-12. Comment:
<https://github.com/BFoerschner/knobas/issues/664#issuecomment-6131209377>

**The fork:** with #661's battery clause 9 in place, `mockd::passes_the_contract_battery` in `crates/knobas-source-teamcity/tests/it/mockd.rs` now reads the view of every build configuration mockd serves. mockd answers `GET /app/rest/buildTypes/id:X` with 501, so the test fails. The implementer listed three options. (A) keeps the new 58-line `build_type_by_locator` route in `crates/knobas-mockd/src/teamcity.rs` and records it as an exception to "nothing new". (B) deletes the mockd battery test. (C) runs the mockd battery through a test-only wrapper whose view answers `None`. The implementer leans to A.

**Ruling:** **(C), with a narrower wrapper. Option A is rejected and the mockd route comes off the branch.**

1. **`git diff a41b6dfd...HEAD -- crates/knobas-mockd` must be empty.** Delete `build_type_by_locator` and its `.route(...)` line.
2. **Add a test-only wrapper in `tests/it/mockd.rs`** that delegates every `Source` method to the TeamCity adapter. Its `view` answers `Ok(None)` for a `teamcity:buildType:…` entity without sending a request, and delegates every other kind. Clause 9 then still runs against the real adapter for builds and any other kind it emits. Only the configuration view, which mockd cannot serve, is excused.
3. **Give the wrapper a doc comment that says it is not a witness.** It should name the frozen-mockd rule, and name the two places the configuration view is checked: `passes_the_contract_battery_against_the_seeded_server` in `live_teamcity_seeded.rs`, which runs clause 9 against the real server, and the wiremock tests in `tests/it/view.rs`. Use only `passes_the_contract_battery` with the wrapper. The other mockd tests keep the plain adapter, and their `assert_no_violations()` stays as it is.
4. **Do not delete any test** (not B). Do not change the battery in `knobas-source/src/contract.rs`.
5. **Docs:** append this ruling to `docs/decisions/2026-10-unattended-rulings.md` as `## #664 — the mockd battery and the configuration view: a wrapper, not a route`. Nothing changes in the glossary and no ADR is needed.

**Reasoning:**

- **This is already decided, and A is the option the record rules out.** ADR-0013: *"mockd is frozen and deprecated: no Confluence half, 8211 unreserved, nothing new; its tests stay only until the live suites assert the same things, then go with it."* `docs/agents/working-model.md` says the same: *"`knobas-mockd` is deprecated: frozen, nothing new"*. The crate has met this exact case before. When Jira's `test_connection` started probing a path mockd does not serve (#297), the crate's own docs record what was done (`crates/knobas-mockd/src/lib.rs`): *"No route is added — this crate is frozen — and `knobas-source-jira/tests/it/mockd.rs` asserts that violation and no other instead."* The test was adapted to mockd, and mockd was not extended for the adapter. The #643 ruling (2026-10-12) cited the same sentence to reject an option that needed a new mockd route. Recording A as "an exception" would re-decide a settled rule, which a deputy ruling may not do.
- **A's check would not test anything real.** The proposed route serves `parameters`, `steps`, `triggers` and `vcs-root-entries` as empty lists, because "mockd invents no settings". The view clause 9 would read from it is drawn from data mockd made up. ADR-0013: *"no mock is a witness for any acceptance or exit criterion"*. The #607 ruling: *"Under the 2026-09-02 ruling the mockd run certifies nothing in either direction."* So "`just check` keeps exercising the view path" would mean a 58-line addition to a frozen crate to exercise a path against an answer that cannot be wrong. The implementer's objection to C, that the view check there "becomes meaningless", applies equally to A.
- **The view already has its witnesses, and C does not touch them.** The configuration view is checked by the wiremock tests in `tests/it/view.rs` (each section, inherited marking, a password value searched for in the serialized view) and by the seeded live suite, whose `passes_the_contract_battery_against_the_seeded_server` runs the same clause 9 against the real TeamCity. That is the witness ADR-0013 requires. #661 set the same pattern for Gitea: its view was witnessed by `tests/it/view.rs` against the in-crate fake and by `live_gitea.rs`, and mockd was not involved.
- **B is rejected because it changes this ticket's scope.** The deletion clause in ADR-0013 is real. But removing a test that also guards clauses 1–8 in the container-free gate is mockd retirement work, and ADR-0013 rejected doing that as a side effect: *"Retire mockd in one ticket now. Rejected for sequencing … deleting its Jira and TeamCity tests before the live suites cover the same assertions removes regression cover while the replacement is being written."* If Björn wants to start removing mockd, that belongs in its own ticket that weighs every mockd test together, not in a view ticket.
- **C is the smallest change that keeps each check honest.** It touches only test code in the TeamCity crate, adds nothing frozen, and keeps clauses 1–8 in `just check`. Narrowing the wrapper to `buildType` keeps clause 9 running against the real adapter for every other kind. Without the narrowing, the skipped check would be wider than mockd's gap.

**If you disagree, the cost of reversing this is:** low, and the same in either direction. Going to A means restoring the 58-line route from this branch's history, removing the wrapper, and replacing this decisions entry with a dated ruling from Björn that amends ADR-0013's "nothing new". That amendment is the real cost, because it is the first route added to mockd since it was frozen, and every later "just one route" ruling would cite it. Going to B means deleting one test and the wrapper. Nothing is stored, migrated or frozen either way.

As built (#664): `ConfigViewExcused` in `crates/knobas-source-teamcity/tests/it/mockd.rs`, used by `passes_the_contract_battery` alone. `git diff a41b6dfd...HEAD -- crates/knobas-mockd` is empty, and the line the branch had added to `docs/contract.md` §5 for the route is gone with it.

## #683 — a commit named by CI's clone address: `clone_urls` on the Gitea source, not a rewritten fixture

Ruled 2026-10-13 by the orchestrator, the author of #683, under Björn's #660 delegation. Sent to the implementer, not posted as a comment.

**The fork:** the live witness asks a TeamCity change and revision to open the mirrored Gitea commit. The seeded VCS roots clone `http://gitea:3000`, the compose network's name. Every mirrored commit's page starts with Gitea's `ROOT_URL`, `http://127.0.0.1:3000`. #665's resolver matches the page as written, so the hint misses. The implementer first pointed the roots at `127.0.0.1:3000` and added a forward container in TeamCity's network namespace (option A). That changed the shared TeamCity volume into a state main's code could not use: another agent's seeded run failed 12/14 with "Unable to collect changes". The implementer stopped and listed three options. (A) keeps that environment change. (B) adds a configured alias on the Gitea source. (C) matches owner/repo when the host differs. The implementer leaned to B.

**Ruling:** **(B).**

1. Add an optional Gitea config field listing the other base URLs at which CI reaches the same server. Name it for what it is, `clone_urls`, and give it a description a person understands, because the Add-source form is generated from the schema.
2. Accept URL prefixes and the scp-like SSH form (`git@host:` → `owner/repo(.git)`). Real TeamCity roots are often SSH. Strip a trailing `.git`.
3. The commit-hint resolution rewrites a matching prefix to the source's own base URL before `RESOLVE_URL`. Otherwise it stays exact: no owner/repo guessing across hosts, per #665.
4. Write the §10.8 entry if the config schema is a frozen surface.
5. Remove the forward, the agent network change, the root rewrite and the `teamcity-gitea` compose service. Keep the change-witness commit and branch. Every run leaves the volume exactly as main's seed expects, verified at the end of the run.
6. Rebase onto origin/main (`9f47e073`, with #690's lock) before the next live run.

**Reasoning:**

- **The shared fixture must stay usable by code on main while a branch is unmerged.** A forces every other worktree's TeamCity runs onto this branch's compose file, and it failed the first time another agent ran.
- **B models a real deployment shape, not a test rule.** CI commonly clones over an internal hostname or an SSH remote while the server's pages carry its public URL. The alias is explicit configuration by the reader. The resolver stays exact for everyone who configures nothing.
- **C was refused in #665's own entry.** The resolver "does not guess that two hosts are one". The same owner/repo on two hosts is two repositories.

**If you disagree, the cost of reversing this is:** low before release. Removing `clone_urls` is one optional config field, one query and a pure function in `sources/crud.rs`, and its tests. A stored config that carries the key would then be refused by `deny_unknown_fields`, so a reversal after users configure it needs a tolerant parse for one release. Going to A again means re-landing the forward and agent change in step with main, as one merge.

As built (#683): `GiteaConfig::clone_urls`, declared through the SPI's `knobas_source::clone_urls` (the key, its grammar and `canonical_repo_url`, moved there in review so the app reads only what an adapter's `config_schema` declares), and `crud::clone_addresses`. The §10.8 entry and the §4.2 table cell are in `docs/contract.md`. `teamcity_seeded_live.rs` configures `clone_urls: ["http://gitea:3000"]`.

## #712 — a source without identity in a mixed room: its branches stay drawn

Ruled 2026-10-13 by the orchestrator, as deputy for Björn. Posted on #712.

**The fork:** #712 groups the Code tile by "what needs me", with "me" taken per source from its configured username. The issue says a source with no identity draws today's order. In a room where one source has a username and another does not, the implementer's first build put the identity-less source's pull requests under *Other open pull requests* and its branches under *Other branches*, which the grid's tile hides until it is maximised. That source's branches disappeared from the grid.

**Ruling:** branches of a source with no configured identity are not hidden. They are drawn in a visible group, *Branches*, after *My branches without a pull request* and before *Other branches*, in the order that source drew them before (loudest tone, then newest). Its pull requests stay under *Other open pull requests*, which is visible. The header counts these branches in its *branches* segment with my unproposed branches.

**Reasoning:**

- **"A source with no identity draws today's order" has to hold inside a mixed room too.** Hiding is a claim that the branch is somebody else's, and with no identity nobody can make that claim. ADR-0007's miss direction is the same: when the answer is unknown, fail toward showing.
- **One more group is cheaper than a second rule.** The read already groups by source identity row by row; a sixth value in the same `case` keeps one statement and one wire shape.

**If you disagree, the cost of reversing this is:** low. Removing the group is one `case` arm, one `AttentionGroup` variant and its TS mirror, one label, and two tests; nothing is stored or migrated.

As built (#712): `AttentionGroup::Branches` (`branches` on the wire) in `crates/knobas-app/src/commands/entity.rs`, its label in `app/src/lib/shell/tile-groups.ts`, the shared count in `tile-count.ts`; witnessed by `a_source_without_identity_keeps_its_branches_drawn_in_a_mixed_room` and the Tile test of the same name. The §10.8 entry for #712 lists the variant.

## #705 — a review whose words are required unless it approves: one action per verdict

Ruled 2026-10-13. Comment:
<https://github.com/BFoerschner/knobas/issues/705#issuecomment-6144135740>

**The fork:** #702's form vocabulary cannot say *required unless approving*: `ActionField.required` is a plain bool, and the ticket asked for one *Review…* with a verdict choice and a body required unless approving. Options: (a) leave the body optional and let Gitea refuse a blank request for changes; (b) split the review into separate actions; (c) have the payload entry return `null` for that case.

**Ruling:** **(b).** The pull request's toolbar gets **Comment…** (the existing `comment` op, body required), **Approve…** (`submit_review` with verdict `approve`, body optional) and **Request changes…** (`submit_review` with verdict `request_changes`, body required). No verdict choice field. `submit_review` keeps its `comment` verdict for #709, which adds **Submit review…** when line comments are pending.

**Reasoning:** one action per verdict is clearer to read and faster by keyboard, and each form's `required` is then honest.

**If you disagree, the cost of reversing this is:** low.

As built (#705): Gitea's own rule agrees, measured on the live 1.27.3: a blank request for changes is a 422 (*review event REQUEST_CHANGES requires a body*).

## #705 — two actions on one op: the source fixes a value the reader does not pick

Ruled 2026-10-13. Comment:
<https://github.com/BFoerschner/knobas/issues/705#issuecomment-6144469197> (also posted on #706).

**The fork:** the ruling above makes *Approve…* and *Request changes…* two actions on `submit_review`, and #706 makes *Close* and *Reopen* two actions on `edit_pull_request`. The shell's payload table sees the entity and the dialog's values, nothing in a `ViewAction` says which verdict or state, and the shell must not read the label. Options: (A) `ViewAction` gains `values` — fixed answers the source sets, never drawn, merged under the dialog's values before the payload table sees them; (B) a one-option `choice` field; (C) split the verdicts across ops (`approve` for *Approve…*).

**Ruling:** **(A)**, `values: BTreeMap<String, String>`, `#[serde(default)]`, a §10.8 entry on `ViewAction` signed off under #701's delegation. The dialog's own value wins only where the same name is also a field, and that overlap is better forbidden in a test. #707 adds `ViewAction.entity` at the same time; whichever lands second keeps both.

**Reasoning:** the shell stays op-agnostic, each dialog's `required` stays honest, and #706 and #709 use the same mechanism.

**If you disagree, the cost of reversing this is:** low.

As built (#705): `ViewAction.values` in `crates/knobas-source/src/view.rs`; `actionPayload` spreads it under the dialog's values. Gitea's `tests/it/view_pr.rs` asserts no toolbar action fixes a name it also asks for.

## #701 — disabled or absent: an action that can never apply is not drawn

Ruled 2026-10-13. Comment:
<https://github.com/BFoerschner/knobas/issues/701#issuecomment-6145023285>

**The fork:** #702 made every action the source does not offer right now a disabled button with its reason, never absent. On a merged pull request that drew three dead buttons (*Merge…*, *Approve…*, *Request changes…*) beside one live *Comment…*, and on the reader's own pull request two that no wait would ever enable.

**Ruling:** for every ticket under spec #701, an action is drawn **disabled with its reason** only when the entity can still reach a state where it is enabled (conflicts, behind base, approvals missing, nothing ahead yet, a write in flight). An action that can **never** apply to this entity is **absent**: *Merge*, *Approve* and *Request changes* on a merged or closed pull request, *Approve* and *Request changes* on the viewer's own pull request, *Delete* on the default branch. Toolbar order on a pull request: *Comment…*, *Approve…*, *Request changes…*, then maintenance (#706, #708), then *Merge…* last. Amends the #716 reading of ADR-0020 rule 4; the PR that lands this records it there.

**Reasoning:** three dead buttons beside one live one is the noise #660 set out to remove; a disabled button earns its place only by telling the reader what to fix.

**If you disagree, the cost of reversing this is:** low.

As built (#705): `view/pr.rs`'s `merge_action` answers `None` on a merged or closed pull request and keeps the two `mergeable` reasons; `view/pr/review.rs` offers the verdicts only on an open pull request the token did not open. ADR-0020 rule 4 and CONTEXT.md's *Source view* carry the amendment, and #705's §10.8 entry notes it on #702's.

## #707 — a branch's open pull request: an entity row on its Overview, not a toolbar item

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #707.

**The fork:** a branch that already has an open pull request should show *Open PR #n* instead of *Open pull request…*. A toolbar action always names a write (#716), so a plain navigation cannot be a toolbar item without a vocabulary change. Options: (a) draw the pull request as an entity row on the branch's Overview, no contract change; (b) add a navigation kind of toolbar item, a §10.8 change.

**Ruling:** **(a).** The branch Overview's first block is an entity row for the open pull request (state chip, title), and the toolbar shows no *Open pull request…* for that branch.

**Reasoning:** the overview is where a reader already looks for the branch's pull request, and a contract change for one link is not worth its cost.

**If you disagree, the cost of reversing this is:** low. A navigation toolbar item can be added later without moving this row.

As built (#707): `crates/knobas-source-gitea/src/view/branch.rs` puts the `Pull request` entities block before the facts; `toolbar` leaves out `create_pull_request` when a pull request is open from the branch.

## #707 — the repository a branch's or a commit's create targets: `ViewAction.entity`

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #707.

**The fork:** *New branch…* on a branch or commit view and *Open pull request…* on a branch view queue a write whose target is the repository, not the view's entity, and the shell must not parse source ids. Options: (a) Gitea's create ops accept any id inside the repository; (b) a one-option `repository` choice field in the dialog; (c) `ViewAction` gains `entity: Option<String>` (serde default), the write's target where it is not the view's own. The implementer leaned to (c).

**Ruling:** **(c).** `actions.ts` uses `action.entity ?? view entity`. The §10.8 entry on `ViewAction` is signed off under #701's delegation. The one-option *From* and *Head* choices are fine.

**Reasoning:** `Block::Settings` already carries its target entity. `WriteOp` semantics and the queue's ordering stay untouched, which (a) would have changed (the queue would order and hold a create on a branch or a commit). The dialog shows nothing the reader does not need, which (b) would have.

**If you disagree, the cost of reversing this is:** low: one optional field on `ViewAction`, its TypeScript mirror and one expression in `actions.ts`.

As built (#707): `knobas_source::view::ViewAction::entity`, set by `crates/knobas-source-gitea/src/view/act.rs`'s `new_branch` and `pull_request` on a branch's and a commit's view; `actionPayload` in `app/src/lib/detail/view/actions.ts`. The §10.8 entry is in `docs/contract.md`.

## #707 — after *New branch…*: the queued toast, no link to a branch the shell cannot name

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #707.

**The fork:** the issue says that on success the toast offers to open the new branch, or the detail navigates to it ("pick one and say which"). Both need the new branch's entity id. The source mints that id, the shell must not compose one, and the branch reaches the mirror only after the write flushes and the post-write sync runs. Options: (a) the usual queued toast, and the view's re-read lists the branch; (b) a toast action that runs a launcher search for the typed name; (c) a new IPC that resolves (repository, name) to the branch's id through the mirror, a §10.8 addition. The implementer leaned to (a).

**Ruling:** **(a).**

**Reasoning:** no id is guessed and no IPC is added, and the re-read already puts the branch one click away.

**If you disagree, the cost of reversing this is:** low. (c) can be added later without changing anything this ticket built.

As built (#707): `queueAction` in `app/src/lib/detail/Detail.svelte` is unchanged. It pushes *New branch queued for …* and re-reads the mirror and the view.

## #707 — *Open pull request…* with nothing ahead: drawn disabled, saying why

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #707.

**The fork:** the issue offers *Open pull request…* "on a branch view whose branch has no open PR and is ahead of the default branch". The implementer drew it disabled, with *No commits ahead of {default}*, on a branch with nothing ahead. Options: keep it drawn disabled, or leave it out.

**Ruling:** keep it **drawn disabled with the reason**. It is absent only on the default branch and on a branch with an open pull request.

**Reasoning:** this is the #702 disabled-with-reason pattern. The reader learns why, rather than wondering where the action went.

**If you disagree, the cost of reversing this is:** trivial: one condition in `toolbar` in `crates/knobas-source-gitea/src/view/branch.rs`.

As built (#707): `toolbar` in `crates/knobas-source-gitea/src/view/branch.rs`. A later cross-ticket ruling on #701 (2026-10-13) sets the rule this applies: an action is drawn disabled with a reason only where the entity can still reach the enabled state, and an action that can never apply is absent. So *Delete branch…* is absent on the default branch and disabled, with the reason, on a protected one. The same ruling message accepted a cut on the candidates the repository's *New pull request…* asks about (20 then; ten since review, to stay inside Gitea's burst of 20 per opening), which the head field's label names, and the live test deleting its own pull request through the API rather than closing it.

## #706 — an edit sends only what the reader changed: the payload table sees the declared defaults

Ruled 2026-10-13. Comment:
<https://github.com/BFoerschner/knobas/issues/706#issuecomment-6144136147>

**The fork:** the ticket asks that the `PATCH` carry only the fields given, and the #702 review (PR #716, noted on #706) found the shell's payload table takes `(entity, values, settings)` and never sees a field's declared `value`, so it cannot tell an untouched prefilled description from a changed one. `actionPayload` already holds the `ViewAction`.

**Ruling:** to send only the changed fields, the shell's payload builder in `actions.ts` may receive the action's declared field defaults alongside the submitted values (a shell-only change, no contract change), and compare.

**Reasoning:** the `PATCH` must not overwrite a description someone else edited since the view was read with an unchanged copy.

**If you disagree, the cost of reversing this is:** low.

As built (#706): each `PAYLOADS` entry takes a fourth argument, the dialog's `initialValues(action)`; `edit_pull_request`'s entry sends `title` and `body` only where they differ from it, comparing line ends as `\n` (a text area gives back `\n` for Gitea's `\r\n`), and answers `null` when nothing changed. `just gitea-live`'s `a_pull_request_is_edited_closed_and_reopened_from_its_view` witnesses the reason against the real server: a description changed behind knobas' back survives a title edit. Close and Reopen use #705's `ViewAction.values` ruling above (also posted on #706): `{state: closed}` and `{state: open}` on the one op.

## #706 — a queued edit holds only on what it would overwrite

Ruled 2026-10-13 by the deputy for Björn, in the PR #725 review. Comment:
<https://github.com/BFoerschner/knobas/issues/706#issuecomment-6146187141>

**The fork:** the implementer gave `edit_pull_request` the whole-record hold, which was a classification of its own and not a ruling. With that hold, someone retitling a pull request holds a queued *Close*.

**Ruling:** the edit holds only when what it would overwrite changed. A title edit holds if the title changed, a description edit if the description changed, and a state change (Close or Reopen) if the state changed. If the queue's mechanism can only compare the whole record per op, the narrowest honest option it allows is used, and the reason is stated.

**Reasoning:** a hold asks the reader to decide again. A hold over a change the write would not touch is noise they cannot learn anything from.

**If you disagree, the cost of reversing this is:** low. It is one arm in `knobas_core::write_queue::project`.

As built (#706, merge review): `project` also takes the queued payload and the kind's declared state paths. `edit_writes` reads which fields the op carries and fails wide: a field is written unless its key is an explicit `null`. The narrowest projection then applies:
- a title alone uses `title`;
- a description uses `body_text`, which a retitle or a new comment also moves (a false hold, the safe direction);
- a state change uses the state the source **declares** for the kind (`KindPaths::state`, #666; Gitea's merged flag, then its `state` word).

The declared state is the read ADR-0007 sends a payload read to, not a guess at a shape. So a retitle does not hold a queued Close, and somebody else closing, reopening or merging the pull request does. A declaration that resolves nothing falls back to the whole record, so a miss is a false hold. `knobas-sync` reads the paths from the registry's template descriptor by the entity's namespace, at the queue and at the flush alike. Pinned by `knobas-core`'s `an_edit_holds_only_on_what_it_would_overwrite`, `an_edit_read_that_misses_holds_on_the_whole_record` and `a_close_over_a_state_the_declaration_misses_holds_on_the_whole_record`, and `knobas-sync`'s `a_title_edit_holds_on_a_retitle_and_not_on_a_close` and `a_close_holds_on_the_declared_state_and_not_on_a_retitle`.

## #724 — the Gitea runs queue on a lock of their own, not on the JVM lock

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #724.

**The fork:** the issue says to give `gitea-live` "the same queueing lock as #696 (reuse its mechanism)". That could mean the JVM lock itself, so a Gitea run would queue behind any JVM run, or the same mechanism under a second lock name, so Gitea runs queue only behind each other. The implementer chose a second lock.

**Ruling:** **a separate Gitea lock**, the same mechanism under another name (`$TMPDIR/knobas-live-gitea.lock`, holder in `KNOBAS_LIVE_GITEA_LOCK_HOLDER`).

**Reasoning:** a Gitea run uses none of the JVM memory the JVM lock guards. Queueing a two-minute Gitea suite behind a 20-minute Atlassian run would serialise runs that never collided. There is one implementation (`_live_lock_select` in `testenv/live-lib.sh`), so nothing is duplicated.

**If you disagree, the cost of reversing this is:** low. Point `_live_lock_select gitea` at the JVM lock's path; the recipes do not change.

As built (#724): `live_lock_hold <recipe> [jvm|gitea]` and `live_session gitea` in `testenv/live-lib.sh`; `gitea-live`, `gitea-live-capped` and `start-work-live` take it. A TeamCity run, which brings Gitea up for its VCS roots, takes no Gitea lock, so `gitea-live-capped` still caps Gitea under a TeamCity run.
## #709 — *Submit review…*: absent with nothing pending, first and primary with its count

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #709.

**The fork:** the implementer put *Submit review…* (a review that is only the pending line comments) after *Comment…*, among the review actions, and drew it only while comments are pending. The #701 toolbar ruling does not name it.

**Ruling:** it is **absent** while no line comment is pending (comments are started from a diff line's `+`, not the toolbar). When present it is the **first** toolbar item, **primary**, with its count: *Submit review… (n), Comment…, Approve…, Request changes…, Edit…, Close/Reopen, Merge…*.

**Reasoning:** with comments pending, sending them is the next thing to do, and the reader should find it where the eye starts.

**If you disagree, the cost of reversing this is:** trivial: the order and `primary` in `actions` in `crates/knobas-source-gitea/src/view/pr/review.rs`.

As built (#709, merge-manager fix on PR #726): `review::actions` pushes it first with `primary: true`; `offeredActions` in `app/src/lib/detail/view/actions.ts` drops a `requires` action while nothing is pending.

## #709 — the per-line `+`: one tab stop per file

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #709.

**The fork:** every line of a commentable diff had a focusable `+`, so a long diff was hundreds of tab stops. Options: hover-only (unreachable by keyboard), one stop per line (as built), or one stop per file.

**Ruling:** one tab stop per file's line list, a **roving tabindex**: Up and Down move the line focus, Enter or `c` opens the editor on the focused line. The `+` stays visible on hover and on the focused line.

**Reasoning:** keyboard reachability without making Tab useless across the rest of the panel.

**If you disagree, the cost of reversing this is:** low: one `tabindex` expression and a key handler in `DiffBlock.svelte`.

As built (#709, merge-manager fix on PR #726): `rover`, `firstRow` and `roveKey` in `app/src/lib/detail/view/DiffBlock.svelte`; Escape and a saved comment hand the focus back to the line's `+`.

## #709 — a comment on a file the diff no longer holds: kept, in an Outdated list

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #709.

**The fork:** a review comment whose path is no longer in the pull request's diff (a later push dropped or reverted the file) had no file card to sit in, and the adapter left it out.

**Ruling:** it is **not dropped**. It goes to an "Outdated" list at the end of the *Files* tab (path, line, thread), as outdated comments on present files already do at the end of their file.

**Reasoning:** a reviewer's words are never silently lost; outdated is a place, not a deletion.

**If you disagree, the cost of reversing this is:** low: one optional field on `Block::Diff` and the list in `DiffBlock.svelte`.

As built (#709, merge-manager fix on PR #726): `Block::Diff.gone: Vec<FileThread { path, thread }>` (`#[serde(default)]`, in the §10.8 entry), filled by `threads::place` in `crates/knobas-source-gitea/src/view/pr/threads.rs`; `DiffBlock.svelte` draws it after the last file, together with any pending comment on such a file, closed unless one is pending.

## #710 — a failed commit-status read draws one muted line, not a failed view

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #710.

**The fork:** the implementer put Gitea's `GET /repos/{o}/{r}/commits/{ref}/status` in the commit, pull request and branch views' `try_join!`, so a 403, 404 or 500 from it failed the whole view (the detail fell back to *Showing the mirror*). The PR left the choice to the reviewer.

**Ruling:** a failed status read does **not** fail the view. The view draws without *Checks* and with one muted line in their place: *Checks could not be read: <reason>*.

**Reasoning:** checks are secondary to the record they are drawn on; losing the pull request's facts, description and toolbar over them costs more than it tells. ADR-0020 rule 2 (never silent about an error) is kept by the line.

**If you disagree, the cost of reversing this is:** trivial: `checks::block` in `crates/knobas-source-gitea/src/view/checks.rs` returns the error again, and the three views put it back in their `try_join!`.

As built (#710, merge-manager fix on PR #729): `checks::block` returns `Option<Block>`, and a failed read becomes `unread(&error)`, an untitled `Entities` block of one muted row with no chip or address (the shape a list's own muted note has, #678), so nothing new crosses to the shell. Witnessed by wiremock in `tests/it/view.rs`, `view_pr.rs` and `view_branch.rs` (`…_cannot_be_read_still_draws_and_says_so`).

## #710 — the builds-of-a-commit scan merges now; the index is a follow-up

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #710.

**The fork:** the merge-manager measured `DETAIL_BUILDS_AT` with EXPLAIN ANALYZE. No index answers a declared path, so a commit's detail runs a sequential scan of the live corpus and walks each build's revisions: about 80 ms at 5,000 builds and 430 ms at 50,000. The fix is an indexed side table, which needs a migration the ticket did not name. Options: (A) merge now and file a follow-up; (B) hold the PR until the table exists.

**Ruling:** **(A)**. The indexed side table, and moving the Builds summary above the view's tabs, are filed as #732.

**Reasoning:** the cost lands only on the details of code (a ticket's or a page's detail never sends the statement, merge-manager fix d6726256), today's mirrors are well below the slow range, and the fix is a migration of its own (ADR-0019 pattern, like `sync.item_parent`).

**If you disagree, the cost of reversing this is:** none: #732 replaces the statement's lookup, and nothing else depends on the scan.

## #722 — a Gitea 403's signal: the body, through an optional reader in `knobas-http`

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #722.

**The fork:** Gitea answers 403 both for writes no token can fix (a mirror, a protected or the default branch) and for a token without the write scope, and knobas read every 403 as a dead credential. Measured on 1.27.3: a bad token is a 401; the scope 403 comes from a token whose `GET /user` still answers 200. Options: (a) an additive, optional 403 reader in `knobas-http`'s `HttpConfig` (a frozen surface outside #701's list), so the adapter reads the body; (b) adapter-only, a `GET /user` after a write's 403, no Gitea sentence.

**Ruling:** (a). `HttpConfig` gains an optional per-adapter 403 reader; a message turns the 403 into `Protocol { status: 403, message }`, otherwise it stays `Unauthorized`. Additive; every other adapter passes `None`. Gitea treats every 403 as a refusal except its scope message (*token does not have at least one of required scope(s)*), each witnessed live. §10.8 entry. #707's pre-reads in `delete_branch` stay for their better messages.

**Reasoning:** a bad token is a 401 and the 403's body is the only honest signal; (b) would drop a read-only token's writes as refusals instead of asking for a new token.

**If you disagree, the cost of reversing this is:** low: additive, one adapter (`forbidden: None` in `crates/knobas-source-gitea/src/client.rs` restores the old reading).

As built (#722): `classify::ForbiddenReading`, `classify::response_error` and `HttpConfig.forbidden` in `crates/knobas-http`; `client::forbidden_refusal` in the Gitea adapter.

## #722 — the mirror fixture: a same-instance mirror, no internet

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #722.

**The fork:** Gitea refuses a migration from its own address (*You can not import from disallowed hosts.*). Options: (i) the live test mirrors a public GitHub repository itself; (ii) `ALLOW_LOCALNETWORKS` in `testenv/docker-compose.yml` and a seeded mirror of `tidewater/ops-runbooks`, at the cost of one container recreate.

**Ruling:** (ii). The live tests must not depend on the internet (a GitHub outage would fail the suite for reasons outside knobas). The one recreate happens inside the Gitea live lock (#724); if the OrbStack port forward dies after it, `stop` + `up` heals it (known since 2026-09-06): the recipe does that and verifies `:3000` answers.

**Reasoning:** a suite that fails for someone else's outage is noise; the recreate is a one-off.

**If you disagree, the cost of reversing this is:** low: one environment line, seed section 4b, and the live test's mirror half.

As built (#722): `GITEA__migrations__ALLOW_LOCALNETWORKS` in `testenv/docker-compose.yml`; seed section 4b creates `knobas/ops-runbooks-mirror`; `_live_gitea_up` in `testenv/live-lib.sh` probes the host's `:3000` and heals with `stop` + `up` only while holding the Gitea lock (outside it, it fails naming `just gitea-live` and stops nothing), pinned by `testenv/live-test.sh`. The first `just gitea-live` on this branch recreated the container inside the lock and the forward survived.

## #718 — the default branch is excluded from every Code-tile group, ungrouped rooms too

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #718.

**The fork:** #718 says to exclude a repo's default branch "from both *mine* and *branches* groups". Read literally, somebody else's default would still be drawn under the maximised tile's other branches, and a room where no source has an identity (which #712 ruled draws as before) would still draw every default. Options: (a) exclude it only from *mine* and the identity-less *branches* group; (b) exclude it from every group and count, in every room.

**Ruling:** **(b)**. The default branch is in no Code-tile group and no count (`total`, `tally`, `groups`), whoever's it is, and in a room with no configured identity too. This narrows #712's "no identity draws as before" ruling for this one branch.

**Reasoning:** a default branch never wants a pull request, so in an attention tile it is noise wherever it appears, and under (a) the same branch would appear or disappear depending on another source's configuration. It stays one click away in the repo view.

**If you disagree, the cost of reversing this is:** one SQL condition (`and r.is_default is not true` in `LIVE_ATTENTION`, narrowed to the groups (a) names).

As built (#718): `is_default` in `LIVE_ATTENTION` (`crates/knobas-app/src/commands/entity.rs`), pinned by `a_repos_default_branch_is_drawn_in_no_group` and `an_ungrouped_room_draws_no_default_branch_either`.

## #728 — a listed repository deleted mid-run is skipped; ruling B4 stands for configured ones

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted in the body of #728.

**The fork:** with no `repos[]` configured, the Gitea sync lists repositories and then reads each one. A repository deleted in that window answers 404, and ruling B4's full-sync extension made that fatal, failing the whole run over a repository that no longer exists. #724 had already fixed the same race for branches and pull requests. B4 could either stand for every repository or be narrowed.

**Ruling:** B4 stands for a repository the **user configured** by name: a 404 there is a misconfiguration worth failing on. A repository that came from the **listing** in the same run and then answers 404 on its own read is skipped exactly as #724 skips a branch. That 404 confirms it is gone, and the next sweep retires it. Recorded as an amendment where B4 is written.

**Reasoning:** B4's full-sync extension exists so that a skip is never the only record of a repository in scope. A repository whose own read answers 404 straight after the listing named it is not in scope any more. The user did not write it down, and the next listing will not offer it. A name the user typed is different: a 404 on it means the configuration is wrong, and a green run would hide that.

**If you disagree, the cost of reversing this is:** low. Drop the `selected.listed` arm in `sync::run` and the listed-then-404 repository fails a full sync again. Its test, `a_listed_repository_deleted_mid_run_is_skipped_and_the_run_succeeds`, would then assert the opposite.

As built (#728): `Selected.listed` marks a repository that came from the listing. In `sync::run`, a `RepoError::Skip` with status 404 on a listed repository is followed by one `GET /repos/{owner}/{repo}`, and only that read's 404 (`repo_is_gone`) skips it, on either kind of run. A skipped repository holds no cursor position and does not count towards "every repository refused". On an incremental run this is a change too: before #728 the repository kept its position and counted as a refusal (`a_listed_repository_deleted_mid_run_drops_out_of_an_incremental_cursor`). Any other answer falls through to B4 as before. The amendment is written in the `sync.rs` module docs, on `client::is_repo_scoped`, and as a note under B4 in `docs/contract.md` §9.

## #721 — Gitea's "405 Please try again later": a bounded retry inside the adapter's merge

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #721.

**The fork:** Gitea answers a merge 405 *"Please try again later"* for any pull request whose recorded state is not *mergeable* (`ErrNotMergeableState`): *checking* for a second or two after it opens or its head moves, but also *conflict* or *error* for good, and the sentence cannot tell them apart. Options: (a) retry it inside the adapter's merge, bounded at about 15 s, then return it as the refusal; (b) classify it as a queue-level transient, retried without bound and labelled *unreachable*.

**Ruling:** (a).

**Reasoning:** an unbounded retry would hide a real conflict forever as an outage (story 19). The bounded retry covers the 1–2.5 s check window measured live (10 of 10 immediate merges landed).

**If you disagree, the cost of reversing this is:** low: `MERGE_PATIENCE` and `patiently` in `crates/knobas-source-gitea/src/write.rs`.

As built (#721): `write::still_checking` matches the 405 status and the sentence together, and `write::patiently` waits with `knobas-http`'s backoff (each wait capped at 2 s) for at most `MERGE_PATIENCE` (15 s) in total. The write queue flushes one source's writes in sequence, so while a merge waits, that source's other writes wait behind it. Each send is already allowed up to `knobas_http::SEND_BUDGET` (45 s), and Gitea answers these 405s at once, so the wait adds about 15 s at most. It happens only in a race, because *Merge…* is disabled while Gitea says `mergeable: false`.

## #715 — macOS without Command Line Tools: guard the working tree read

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #715.

**The fork:** on a Mac without the Command Line Tools, `/usr/bin/git` is Apple's shim, and the working tree read (run on open and on every window focus) opens Apple's install dialog. Options: guard the read, or leave it.

**Ruling:** guard it. When the resolved git is `/usr/bin/git`, run `xcode-select -p` first (it never opens the dialog); if it fails, answer `no_git` without running the shim. The answer is cached for the process.

**Reasoning:** a modal Apple dialog on every window focus is far worse than a missing status line.

**If you disagree, the cost of reversing this is:** low: `read_guarded` and `command_line_tools_installed` in `crates/knobas-app/src/checkout/working_tree.rs`.

As built (#715): the decision seam `read_guarded` takes the shim path and a probe, and is tested with a fake git that leaves a marker when spawned. The dialog itself is unwitnessed: no Mac without the Command Line Tools is available here (ADR-0013). The probe is async, runs inside the read's 2 s budget (a probe that does not answer draws *failed*, is killed, and is asked again next read), and a `no` is kept for the process: installing the tools while knobas runs is noticed after a restart. A hung probe is witnessed by a unit test.

## #755 — a similar_text batch that always times out must not block the rule

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation. Posted on #755.

**The fork:** the incremental `similar_text` pass compares changed items in batches of 100 with a 120 s statement timeout. A batch that can never finish in that time (a worst-case corpus above ~40,000 items) is retried and cancelled by every pass, so the rule never advances. Options: leave it, or make the pass recover.

**Ruling:** on a timed-out batch, halve the batch and retry on the next pass; at batch size 1, an item that still times out is marked settled with a warning log line naming it (it gets no similar-text proposals from its own comparison until its text changes). The batch size grows back after a pass that completes. No new setting.

**Reasoning:** a rule that wedges on one batch silently stops proposing for the whole mirror; skipping one item costs a few proposals.

**If you disagree, the cost of reversing this is:** low: `Pacing` and the `ChangedProse` arm of `run_rule` in `crates/knobas-core/src/suggest.rs`.

As built (#755): `Pacing` keeps the batch size across passes (the app's `SuggestionPass` owns one). A timeout on a batch of n > 1 sets the size to n/2 and the pass fails as before; a timeout on a single item logs a warning with its id, settles it, and the pass goes on. A pass that completes doubles the size, up to 100. The skipped item is still found from the other side: a later changed item that matches it proposes the pair. The oracle test is unchanged.

## #752 — writes in the background: what a quick second write holds on, and what *Edit and resend* leaves behind

Ruled 2026-10-13 by the deputy for Björn, under the #701 delegation, at the merge review of PR #757. Posted on #752.

**The fork:** with the send behind the reader, a second comment queued while the first one's re-sync is landing is held, because a comment held on the item's text (`body_text`) and an approval or a review on the whole record. Separately, *Edit and resend* queued a new write and left the refused or held original in the queue. And the shell cannot name the discussion section of a source's view (ADR-0020), so a pending comment needs a place to draw.

**Ruling:**

1. A write that adds does not hold on text drift. `comment`, `approve` and `submit_review` hold only when the entity's declared **state** moved (a pull request merged or closed), or the entity left the mirror; #706's per-field machinery and the declared `state` path are reused. The creates and build triggers keep holding on liveness alone. The overwrite ops (`edit_pull_request`, `set_parameter`, `transition`, `merge_pull_request`, `delete_branch`, `update_pull_request_branch`, `update_page`) keep their holds.
2. *Edit and resend* replaces: once the new write is queued, the original is withdrawn, so a resend leaves one write in the queue, not two.
3. Pending comments and reviews are drawn at the end of the view's last tab. Accepted as built.

**Reasoning:** Björn wants to work at his own speed, and a hold the reader caused with his own first comment is noise he cannot learn from. A merged or closed pull request is the change that makes added words moot. Two copies of one write in the queue invite sending both.

**If you disagree, the cost of reversing this is:** low. For 1, `STATE_HELD_OPS` and `added` in `crates/knobas-core/src/write_queue.rs`. For 2, the `replaces` argument of `queueAction` in `app/src/lib/detail/Detail.svelte`.

As built (#752, PR #757): `project` gives the three ops `{op, live, text: <declared state>}` where the kind declares a state (Gitea pull requests), `{op, live}` where it declares none (Jira tickets, Gitea issues), and the whole record where a declaration resolves nothing (ADR-0007's false-hold direction). `knobas-sync`'s `declared_state` looks the paths up for every op `reads_declared_state` names. A comment queued before this change and still pending after it holds once, because its stored snapshot has the old shape. `CONTEXT.md`'s **Held write** says what *changed* means for each kind of write; ADR-0012 says nothing about holds and is unchanged. Pinned by `a_write_that_adds_holds_only_on_the_declared_state` (core), `two_comments_back_to_back_on_one_pull_request_both_go_in_order` (the queue), and `Edit and resend replaces the refused write` (the detail).

## #760 — the ticket detail reads its status with the ticket, and its fallback offer on its own

Ruled 2026-10-14 by the deputy for Björn, under the #701 delegation. Posted on #760.

**The fork:** opening a ticket read `mini_board` for its whole source (133-137 ms and 725 KB at 5,000 tickets) to learn its current status and the statuses the source's corpus shows. `per_column` (#753) cannot answer it, because a capped board need not hold the ticket's card. Options: a card-less board read, or the status with the ticket and the corpus offer as a read of its own.

**Ruling:** as proposed. `EntityDetail.status` is the ticket's declared `status_name`, read once in `get_entity_inner`'s statement through the declared path; the observed-status offer is a dedicated small read of the source's distinct declared statuses by the board's own rule, never the card list. Additive IPC field, one §10.8 file.

**Reasoning:** the current status is a fact about one ticket and belongs with it; the offer is a fact about a source and a card list is the wrong thing to read to learn it.

**If you disagree, the cost of reversing this is:** low: `EntityDetail.status` and the `source_statuses` command, and the two effects in `app/src/lib/detail/Detail.svelte`.

As built (#760): the corpus offer is asked only when the workflow read has failed, since that is the only time it is shown (#498). `status` is `None` for a withdrawn ticket and a disabled source's, as the board draws neither. `source_statuses` is `OBSERVED_STATUSES` for one source, planned per execution like the board's.
## #764 — Gitea incremental sync misses a change made in the same second as the last read

Ruled 2026-10-14 by the deputy for Björn, under the #701 delegation. Posted in the body of #764.

**The fork:** Gitea stamps `updated_at` to the second, and the incremental cursor skipped a pull request on the watermark second whose number it already held, so a merge inside that second was never mirrored. Options: overlap the watermark second, or a wider fixed overlap window; and whether the other adapters share the hole.

**Ruling:** the incremental read overlaps the watermark second and re-reads what sits on it; an unchanged re-read writes nothing. The same rule for every Gitea cursor that watermarks on a second-resolution timestamp, and TeamCity and Jira checked for the pattern. No new setting.

**Reasoning:** Björn acts on a pull request right after a sync (merge, close, comment) as a matter of course; a change lost until the next backfill is a mirror that lies.

**If you disagree, the cost of reversing this is:** low: `pulls` and step 1 of `sync_repo` in `crates/knobas-source-gitea/src/sync.rs`, and two cursor fields.

As built (#764): the exact second, not a window — the walk already re-read it (it stops only below the mark); what was wrong was the identity it compared. The sink does **not** skip identical payloads (it rewrites every item, and battery clause 2 asks the adapter for an empty idle run), so the no-op is the adapter's: the cursor holds number → fingerprint of the listing record for the pull requests on the watermark (`pulls_seen_at_watermark`), and a fingerprint for the repository record (`repo_fingerprint`). Renamed rather than retyped, so no cursor version bump; a cursor from before re-emits the repository and the watermark's pull requests once. Commits (immutable, keyed by object id) and branches (head ids) were not affected. TeamCity's watermark is a build id, not a timestamp: not affected. Jira already fingerprints a two-minute overlap (#345), which also covers JQL's minute resolution: not affected. §10.8 entry: `docs/contract/10.8/764-gitea-cursor-fingerprints-on-the-watermark.md`.
## #759 — the project census walks every mirrored item on every finished sync

Ruled 2026-10-14 by the deputy for Björn. Posted on #759.

**The fork:** `list_projects` walked every live item through the declared project paths (341–461 ms on the 22,000-item probe) and was re-read after every finished sync. Options: (1) store the project key the way #688 stored the parent key, kept by the item triggers (a migration and a §10.8 entry); (2) re-read only after a run that changed something, with no migration.

**Ruling:** both (1) and (2). Store the project key in `sync.item_project`, kept by the item triggers (ADR-0019's pattern, a migration and a §10.8 file), and have the projects store re-read only after a run that changed something. Bundled with #761 into one agent and one migration.

**Reasoning:** both turn a census walk into a stored, indexed read, and (2) removes the re-read after every no-op run.

**If you disagree, the cost of reversing this is:** low.

As built (#759, migration `0032`): `sync.item_project` also keeps the item's time (`at`), which the update trigger rewrites whenever it moves. The census reads each project's first live row off an ordered index, so it does not depend on the planner's statistics: 401 ms before, 6 ms after. The `sync:state` payload carried no run counts, so (2) needed a signal: `SourceSyncStatus.last_changed`, from the finished run's upserted, deleted and swept counts. The store skips only an explicit `false`.

## #761 — the launcher's value suggestions ship every parent name

Ruled 2026-10-14 by the deputy for Björn. Posted on #761.

**The fork:** `filter_suggestions` shipped every parent name in the mirror (274–430 ms, 680 KB). Options: answer `parents` by prefix per keystroke, the newest 20, through an index on `sync.item_parent`'s name (a §10.8 change to `FilterSuggestions`' meaning); or cap the list at the newest N with no IPC change.

**Ruling:** the prefix answer, not the cap: `parents` answered by the engine per keystroke, newest 20 by prefix, through an index on `sync.item_parent`'s name. §10.8 file for the changed `FilterSuggestions` meaning. Bundled with #759 (one agent, one migration).

**Reasoning:** the launcher already sends a request per keystroke, and a cap would still ship hundreds of names nobody typed.

**If you disagree, the cost of reversing this is:** low.

As built (#761): `filter_suggestions` takes an optional `under`. A name is ranked by the newest mirrored item that names it, and offered only while `under:`'s own lookup knows it. The statement is unnamed, so every prefix gets a custom plan, and the ranking reads the item's time off `sync.item_parent.at` (added by the merge review: joining `sync.item` for it was 260 ms on tables never analyzed). `under:` with nothing typed answers in 44 ms, `under:pay` in 12 ms, about 1 KB each. Known concession: a name whose newest item was withdrawn ranks a little higher than its live items alone would put it.

## #768 — the launcher re-reads the kinds and the sources on every `under:` keystroke

Ruled 2026-10-14 by the deputy for Björn. Posted in the body of #768.

**The fork:** since #767 each `under:` keystroke called `filter_suggestions`, which also re-read the kinds and the sources (about 11 ms analyzed, about 520 ms on tables never analyzed, the state of a fresh install for about a minute after its first sync). Options: split the read or add a flag, and rewrite the two statements so they do not depend on statistics.

**Ruling:** the launcher reads the kinds and the sources once per opening and only the parents per keystroke, by a split or a flag, whichever keeps the IPC change smallest (a §10.8 file if the shape changes); and `KINDS_SQL` / `SOURCES_SQL` are rewritten to be fast with and without `ANALYZE`, measured both ways.

**Reasoning:** the keystroke path should pay only for what changes with the keystroke, and a fresh install should not wait on autovacuum.

**If you disagree, the cost of reversing this is:** low.

As built (#768): a flag, `parents_only`, on the existing command (no new command, the DTO keeps its shape). The session sends it on every read after an opening's first and keeps that read's two lists. The statements drop `sync.live_item`'s joins for the same liveness as correlated subqueries, ordered probes of the item indexes. §10.8 entry: `docs/contract/10.8/768-ipc-command-filter-suggestions-parents-only.md`.

## #770 — the suggestion pass on analyzed tables

Ruled 2026-10-14 by the deputy for Björn. Posted on #770.

**The fork:** the responsiveness probe's text drew on 42 words, so every two of its 17,000 prose items shared five lexemes and `similar_text` had to propose every pair, about 144 million proposals: hours of inserts in either planner state. "The probe's pass completes within budget" and "proposals unchanged" could not both hold. Options: (A) give the probe a realistic vocabulary; (B) bound what `similar_text` proposes per item; (C) A here, B as a follow-up.

**Ruling:** (C). In this PR the probe takes `suggest-perf`'s vocabulary, the timing table is reported in both states, a plan-shape gate test is added, and proposals stay unchanged. (B) is filed as its own ticket and ruled there.

**Reasoning:** this PR fixes the measured never-analyzed failure (the pass dying on the exact-key rules) without changing behaviour; capping proposals is a separate, behaviour-changing decision.

**If you disagree, the cost of reversing this is:** none.

As built (#770): the issue's premise was backwards. On tables never analyzed the exact-key rules ran 37 to 91 s each on the probe's realistic corpus and past the two-minute limit on its dense one, so the pass died in its first rule; on analyzed tables the dense corpus's cost was output volume, and on the realistic one statistics taken before the pass made `similar_text` hash its suppression on `relation` alone and loop over its settled CTE. Every statement now reads `sync.item` with liveness as correlated subqueries, finds tickets by a primary-key probe, tests settledness as a hashed subplan, and suppresses with two `not exists` on the whole key. `tests/it/suggest_plans.rs` holds each plan's shape on a fixture never analyzed and one analyzed before and after the pass. The probe's pass: 274 s analyzed and 276 s never analyzed, 25,690 proposals each.

## #771 — `similar_text` proposes at most the ten most similar items per item

Ruled 2026-10-14 by the deputy for Björn. Posted in the body of #771 and in a comment on it before dispatch.

**The fork:** on a narrow-vocabulary corpus (templated tickets) `similar_text` proposed nearly every pair: 17,000 prose items, about 144 million proposals, about 4.4 hours of writes. Separately, the #772 review found that the rule's third arm read `sync.similar_text_seen` through its primary key with `entity_id < o.entity_id`, about half the table per batch item, with the GIN-indexed `title_lex &&` only a filter. The issue's criteria also contradicted themselves on symmetry, and left open how proposals beyond the cap are retired.

**Ruling:** each item gets at most 10 `similar_text` proposals, its 10 most similar counterparts by distinct shared lexemes, ties by newest. A pair is kept when **either** item ranks the other in its top 10, so the oracle checks "in the old set and within at least one item's top 10". Unanswered proposals beyond the cap are retired by the next pass, and accepted or dismissed ones are untouched. A changed item recomputes its top 10. No setting. Bundled in: the third arm's candidate lookup goes through the GIN index, with pass times before and after in both statistics states. `tests/it/suggest_plans.rs` now counts an inner index scan linked to its outer row only by a range as a finding, and likewise an anti-join hash that leaves part of the key out; a mutant restoring the old third arm proves it. The retirement uses whatever the pass already has, or a §10.8 file if a stored shape changes, without asking.

**Reasoning:** nobody reads more than a handful of similar-text suggestions per item; the rest is noise in the tray and cost in the database. The range probe is the same code as the cap.

**If you disagree, the cost of reversing this is:** low. Migration `0033` adds one derived table in `sync`. Dropping the cap means proposing from the edges instead of the rankings.

As built (#771): the pass had nothing that retired proposals, so retirement is new. It deletes the unanswered row; a tombstone would read as a dismissal to the suppression. Each item's ranking is stored (`sync.similar_text_top`, migration `0033`, §10.8 file `771-migration-0033-similar-text-top-ten.md`), because a pair's verdict now depends on the rest of the corpus. The ranking stays exact because every pair is judged once, when the later of its two ends is compared. An item whose ranking held a place for a counterpart that changed or left the corpus is compared again, since the eleventh place is not stored. Forks decided by the implementer: the ranking is by text alone, so a linked or dismissed counterpart keeps its place; "newest" is `coalesce(item_updated_at, synced_at)`, as taken when the pair was judged, then id; the migration empties `sync.similar_text_seen`, so the first pass after the upgrade ranks everything and retires what the cap drops. The range probe was a smaller part of the realistic pass than the review thought. The rest came from three other costs. First, `similar_text_seen_lex_idx`'s pending list, never merged on tables autovacuum had not reached, which made each GIN probe 32 ms; `0033` sets `fastupdate = off`. Second, a visit to `sync.item` per candidate; the seen row now carries `strip(fts)` and the time. Third, counting shared lexemes by array intersection, now `length(a) + length(b) - length(a || b)`. The ranking statement runs with seq scans off, because a GIN index cannot cost a per-row array and the planner otherwise reads the whole seen table once per batch item. Realistic probe pass: 278 s → 75 s never analyzed, 323 s → 84 s analyzed, 25,690 proposals both before and after. 42-word probe: about 4.4 h of writes (#772's estimate) → 1,155 s never analyzed and 1,022 s analyzed, 152,198 standing proposals.

## #775 — every write closes on queue and draws at once

Ruled 2026-10-14 by the deputy for Björn. Posted in the body of #775.

**The fork:** #752 made `submit_write` answer on queue and the detail's action dialogs close on that answer, but not every write surface followed. Each surface needed a rule for when it closes, what it draws, what a failure does, and what may be disabled meanwhile.

**Ruling:** a dialog or inline editor that queues a write closes on `submit_write`'s answer, never on the send or a re-read (target: within 50 ms on the probe corpus, one frame in the dev shell). What the write makes is drawn at once, muted and marked as on its way, using `view/sending.ts`; a surface with no natural place for it gets the "queued" toast and nothing else, and no new indicator is invented. A write later held or refused does what #752 does: the echo goes, a toast says why and offers *Edit and resend* with what was typed, or the way to the queue where the surface cannot reopen. Writes from one surface go in order; the reader can start the next action while one is being sent, and nothing is disabled while a send is in flight, except what the queue already holds. The re-read after a write that went runs in the background and replaces the echo without blanking. Out of scope: what the queue holds or how it orders, the IPC (a §10.8 file if a fix truly needs it), optimistic UI for non-writes.

**Reasoning:** Björn, 2026-10-13: "Let the writes happen in the background, let me work at my own faster speed."

**If you disagree, the cost of reversing this is:** low per surface; each fix is local to it.

As built (#775): the settings editor closes on queue and its row reads *saving…* while the send runs (a pending write with no reason to wait), the queue's word once it waits, is held or refused. The worklog draft and *Log all* closed on Jira's answer, so `log_work` and `log_all` now answer on queue like `submit_write` (§10.8 file `775-ipc-command-log-work-answers-on-queue.md`). The inbox's *Done*, *Snooze*, *Bring back* and *Ack* take the row away at once and re-read after the last answer in flight, with nothing disabled. The inbox reply, the worklog, *Log all*, monitor pause/resume and *Create monitor* have no place to draw, so `shell/follow-write.ts` follows them and toasts only on held or refused (the inbox reply also offers *Edit and resend*). Monitor rows are free on the queue's answer, and *Flush now* takes no busy. Forks decided by the implementer: links made and withdrawn by hand keep their re-read (`create_link`/`unlink` plus `get_entity` is 3–7 ms on the probe corpus, under the target, and the existing test pins that the panel draws the backend's answer); the standup publish, the action-item ticket and start-work's steps still wait for their send, because each answers with the address of what its write made and the flows draw it — making them background needs an IPC change and a pending-address model, filed as a follow-up; suggestion accept and dismiss are #774's.

## #778 — reads stay fast on a fresh install

Ruled 2026-10-14 by the deputy for Björn. Posted in the body of #778.

**The fork:** on tables never analyzed (a fresh install until autovacuum reaches them, a minute or more after its first sync) PR #777's probe read the *All* room's builds tile at 6,834 ms, the inbox's statements at about 430 ms each, `launcher_home` at 748 ms, `room_suggestions` in a project room at 219 ms (and 220 ms at its maximum analyzed) and the mini board at 223 ms, against 15–70 ms analyzed. The choice was between waiting for autovacuum, analyzing from knobas, and reshaping the reads.

**Ruling:** (1) knobas analyzes its own mirror: after a sync batch commits, any `sync.*` table whose row count changed by more than 20 % since knobas last analyzed it, or that was never analyzed, gets `ANALYZE`, in the background, on the background pool and never the UI pool (#745), at most one at a time, a new trigger coalesced while one runs; the last-analyzed counts kept wherever is cheapest (`pg_stat_user_tables` needs no stored shape), a migration and a §10.8 file if one is needed. (2) For the window that remains, the reads above 150 ms get a plan-robust shape as #770 did (primary-key lookups, `offset 0` laterals, no reliance on row estimates), the builds tile and the inbox first; every row of the table ≤ 150 ms never analyzed, no analyzed number worse. (3) `room_suggestions`'s analyzed maximum of 220 ms: find and fix the cause, or explain it with numbers. (4) Plan-shape gate tests like `suggest_plans.rs` for the changed statements. (5) Iterate on a 2–3k corpus; the full 22k probe once at the end, before and after, in both states.

**Reasoning:** the never-analyzed window is what a new user meets first, and the builds tile alone is the largest wait in the app.

**If you disagree, the cost of reversing this is:** low. No migration and no IPC change; the analyzer is one module the scheduler calls, and each reshaped statement is local to its reader.

As built (#778): `knobas_sync::analyze` runs after every scheduled run that committed. It reads `pg_class.reltuples` (the count the last analyze or vacuum saw, `-1` before either) against `pg_stat_user_tables.n_live_tup`, so nothing is stored. The sync's transaction calls `pg_stat_force_next_flush()` before it commits; without it the run's counts reached the statistics a second or so later and the pass saw no change (a mutant proved it). The reads lost the `sync.live_item` join, which the planner drove from `knobas.entity` at 0.5 % of its size: two shared macros in `knobas_core` read liveness from `sync.item` instead, `live_by_exclusion!` (the tombstones and the disabled sources as hashed `not in` sets, for whole-room scans) and `live_by_key!` (#770's primary-key probes, for reads cut by an index). The builds tile's CTEs no longer join one another: each row's parent key comes from a union grouped by entity. The tray's count is a hashed membership test rather than two joins to `sync.item`. That join was the cause of the 220 ms maximum: PostgreSQL switched the persistent statement to its generic plan at the sixth execution, a primary-key probe per end per proposal (15 ms custom, 225 ms generic, analyzed). `knobas_db::plan_shape` is #770's checker, shared by `suggest_plans.rs` and the new `read_plans.rs`. Forks decided by the implementer: the analyzer covers `knobas.entity` as well as `sync.*`, because it is the other table a sync writes and its missing statistics are the misestimate; *Load demo data* runs `run_once` outside the scheduler and is not followed by a pass; the inbox's `failed_build` lookups, its `alert` rule and the Code tile's `open_head` keep their shapes (each a scan per row whatever the statistics, under 150 ms on the probe) and are follow-ups; the smart lists, the room scan and the docs and notes tiles were already ≤ 150 ms and are unchanged.
## #779 — standup publish, action-item tickets and *Start work* close on queue; one shared write follower

Ruled 2026-10-14 by the deputy for Björn. Posted in the body of #779 and, for the bundled follow-ups of the #777 review, in a comment on it.

**The fork:** #775 left three write surfaces that waited for a send and a sync, because each draws the address of what its write made: the standup publish, the action-item ticket and *Start work*'s steps. The #777 review added two more: settings writes that are held or refused said so only inline, and every followed write asked `pending_writes` on each `activity:new` (N reads per event for *Log all* of N), with a pending write keeping its listener.

**Ruling:** each of the three closes on the queue's answer. What it creates is drawn at once as on its way: the title the reader typed, marked pending, with no link; the address fills in when the write settles and the sync brings the item in, found again by the pending row's write id. A failure follows #752 and #775: the echo goes, a toast says why, *Edit and resend* reopens the dialog with what was typed. *Start work* keeps its order (branch, then PR, then transition), runs in the background on the queue, and the stepper shows each step as it lands. If a command's answer changes, a §10.8 file, without asking. Bundled: a settings write held or refused also raises a toast with *Edit and resend*, which reopens the row's editor with the typed value while the inline mark stays; and one shared follower in the shell, one coalesced `pending_writes` read per `activity:new` fanned out by write id, a subscriber leaving when its write settles or its surface unmounts, with a test that N followed writes cause one read per event.

**Reasoning:** the same as #775's: Björn, 2026-10-13, "Let the writes happen in the background, let me work at my own faster speed."

**If you disagree, the cost of reversing this is:** low. The awaited paths are kept for the battery and the live suites; each command goes back to them by one line.

As built (#779): `publish_standup_protocol` and `create_action_item_ticket` answer once queued, and the send, sync and link run in a spawned task (`sources::write_queue::send_in_background_then`); `ActionItemTicket` gains `state` and `detail`, and a new read, `action_item_ticket(noteId, writeId)`, names the ticket once its write was sent and links it, idempotently (§10.8 file `779-ipc-command-standup-ticket-start-work-answer-on-queue.md`). `start_work_run`, `start_work_retry` and `start_work_skip` do their first half, answer the flow, and run `start_work::run` behind it, one background run per ticket. The stepper re-reads the flow while it moves. Probe (simulated 300 ms send): publish 323.5 → 3.2 ms analyzed, 325.4 → 3.4 never analyzed; ticket 327.0 → 1.7 / 413.0 → 1.5; *Start work* 634.8 → 1.7 / 996.2 → 2.2. Forks decided by the implementer: the ticket's address comes from a new read rather than from the note's link panel, because finding it by the typed title in the shell would be a second copy of `ticket_titled`'s rule; the ticket read looks for a ticket only once the write was sent, so a same-titled older ticket is never named while the create is on its way; the publish's *Edit and resend* opens the target dialog again (the body never left the textarea, and a resend beside a held write still queues one page, `publication_of`); a held publication keeps its echo with the queue's word, since the page will still be made once the reader decides in the queue; the panel's filed tickets are drawn in a *Tickets filed* list under the action items rather than beside the item, so an item edited meanwhile does not lose its echo; a stepper flow is followed while a step is running or the first unfinished step is not started yet, and a background run that errors marks its step failed, so following always ends; the inbox, monitors and week views stop their follows on unmount, while the worklog draft in the shell and *Create monitor* (whose dialog closes on queue and whose toast is its whole feedback) leave only on settle; the detail's own queue watch, one read per event per open detail rather than per write, stays as it is; a settings toast is raised only for a write saved from that panel, not one found held when the view opens.
## #790 — a *Start work* step left running by a quit becomes interrupted and can be retried

Ruled 2026-10-14 by the deputy for Björn. Posted in the body of #790.

**The fork:** if knobas quits, crashes or is killed while a *Start work* step is `running`, the step stays `running` for good: after a restart it can be neither retried nor skipped. Running the chain in the background (#779) makes a quit mid-chain more likely.

**Ruling:** on startup any step stored `running` with no live run for its ticket becomes interrupted, drawn as refused is: the reason "knobas stopped while this step was being sent", with *Retry*, *Skip* and *Edit* enabled. A retry first checks the source the way the background run already does, so a branch or PR that already exists is adopted, not created twice. No new stored state if it can be avoided; if a stored shape must change, a migration plus a §10.8 file, without asking.

**Reasoning:** a step that can never move is worse than one that says it was cut off and lets the reader decide.

**If you disagree, the cost of reversing this is:** low. One start-up pass and two small store functions; no migration, no IPC change.

As built (#790): `start_work::interrupt_stranded` runs in bring-up after the migrations and stores the step as `failed` with that reason, so no stored state changed and no §10.8 entry is needed. It skips a ticket with a live run. The existing retry already reads the mirror before it writes (`perform`), which is the adoption; it needed no change.

## #785 — inbox reads ≤ 30 ms

Ruled 2026-10-14 by the deputy for Björn. Posted in the body of #785.

**The fork:** #778 left two inbox costs. The `failed_build` rule made two lookups per failed build that scan per row whatever the statistics: 195 ms on the probe when every build is the reader's, and the inbox opens, and re-reads after each answer, on it. The stream and snoozed statement read about 70 ms never analyzed, probably a generic plan after the fifth run.

**Ruling:** done means every inbox statement is ≤ 30 ms on the 22k probe in both statistics states, including the all-builds-mine case. `failed_build`'s per-build lookups become index lookups (correlated by key, or a lateral with `offset 0`); if an index is missing, a migration plus a §10.8 file, without asking. Stream and snoozed: confirm the generic-plan cause with `EXPLAIN (GENERIC_PLAN)` or `plan_cache_mode`, and fix it with `.persistent(false)` or a plan-robust shape. Answers unchanged, proved by the existing inbox tests plus one all-builds-mine case. Plan tests in `read_plans.rs`. Measure on 2–3k items first, then the full 22k probe once. Self-merge unless a migration is added.

**Reasoning:** the inbox is read on open and after every answer, so its cost is paid on each interaction.

**If you disagree, the cost of reversing this is:** low. No migration and no IPC change; every change is inside `knobas_core::inbox`.

As built (#785): the generic plan was confirmed. Under `plan_cache_mode = force_generic_plan` the shelf read 41 ms against 17 ms custom, and in the probe's order the shelf runs after the stream's executions of the same statement. `items` and `count` are now `.persistent(false)`. The pin is `the_inbox_statements_are_never_kept_as_prepared_statements`. Forks decided by the implementer: the per-build lookups are **gone rather than indexed**, which needs no migration. Nothing indexes a build's configuration (`buildTypeId` lives in the payload), so an index lookup per build would have needed one. Instead, the newest finished green of each source and configuration is a window over the builds, and a red build is resolved when that time is later than its own. The confirmed links are two uncorrelated `in` sets, one per direction, hashed once. Both answer the same question the correlated subqueries did, checked against the old statement on a mirror of every resolving and non-resolving case. Measured, the rule alone went from 185 ms to 8–10 ms on the probe's 5,000 builds. That left the stream at about 36 ms all-mine, because the mention rule ran its expression on all 17,000 prose items. The mention rule now looks for the username with `strpos` before the expression, which every match contains, and that brought the stream to about 26–28 ms. The window costs the reader with no identity about 5 ms more than before: it sorts every build, where the old rule stopped at the author test.
## #774 — the suggestion tray: a retired proposal vanishes quietly; an item shows at most three rows

Ruled 2026-10-14 by the deputy for Björn. Posted on #774.

**The fork:** since #771 a pass can retire `similar_text` proposals nobody answered, so *Accept* or *Dismiss* on a row still on screen gets not-found, which toasted an error and left the row until the next refresh. And under "either item's top ten" one item can carry thousands of proposals, which fill a newest-first tray with a limit and no per-item bound.

**Ruling:** not-found on *Accept* or *Dismiss* means gone: the row disappears quietly, the list re-reads, no toast; any other error still toasts. This is decided in the shell, or in the IPC answer if the error kind is already there; no new error kind unless none fits. The tray shows at most 3 rows per item; an item with more gets one more line, "+N more similar to <title>", which opens that item's own suggestions. The bound applies to what is drawn and read, not to what is stored, and is read in SQL with a window function.

**Reasoning:** a retired proposal is not a failure the reader can act on, and a hub that fills the tray hides everything else in the room.

**If you disagree, the cost of reversing this is:** low. One SQL statement, one IPC field, one tray branch.

As built (#774): `not_found` already existed on the wire, so the shell alone decides. Forks decided by the implementer: `total` (the heading) still counts every stored proposal; "opens that item's own suggestions" is the item's address, whose links panel lists them. `SuggestionPage` gains `more`; §10.8 file `774-ipc-schema-suggestionpage-gains-more.md`. The first build ranked each item's proposals with a window function over the room's newest slice; on the probe's hub corpus that read took 413–445 ms in the *All* room and about 1.3 s in a project room, which the ruling below replaced.

## #774 — the tray's bounded read must be fast on a hub corpus

Ruled 2026-10-14 by the deputy for Björn, in the merge-manager's dispatch for PR #787.

**The fork:** the tray read as first built took 413–445 ms in the *All* room and about 1.3 s in project JIRA00 on the hub corpus (`RP_SYNTHETIC=0 just responsiveness-probe`, `room_suggestions (20)`), which defeats the responsiveness work asked for.

**Ruling:** done means at most 50 ms warm median in every room on that corpus, with the realistic corpus no worse than today, keeping the answers the ruling on #774 defines. Allowed: (a) a backend walk in Rust over the newest-first index in chunks, counting per item and stopping when the tray is full, the "+N more" counts from per-item index lookups; or (b) a stored per-item rank, which needs a migration and a §10.8 file. Prefer (a) if it meets the target. "In SQL, not the frontend" in the #774 ruling means not filtering after the fetch in the frontend; a backend walk is fine.

**Reasoning:** the bound exists so the tray stays readable; a tray that takes a second to read is not.

**If you disagree, the cost of reversing this is:** low; the walk is one function behind an unchanged signature.

As built (merge-manager, PR #787): (a). `suggest::proposals` walks the room newest first in chunks and draws a row while neither of its items holds three drawn rows; an item holding three is left out of the next chunks in SQL, so a hub costs three rows. A stored context's room reads its members' proposals by the pair indexes instead. "Three per item" counts drawn rows, not every proposal: ranking every proposal (the first build) let rows nobody sees hide an item's own, which on the hub corpus left 9–30 drawable rows in 152k. "+N more" is the item's proposals in the room minus the rows drawn for it, for an item holding more than three, counted per item through `link_from_idx` and `link_to_idx`. Every lookup is shaped off `link_proposed_idx`, which the planner chose on tables never analyzed and read whole (pinned in `read_plans.rs`), and the room's count runs beside the walk on a second connection. `room_suggestions (20)` warm median, never analyzed, main → this PR: hub corpus (`RP_SYNTHETIC=0`, 152,198 proposals) *All* 34.5 → 15.0 ms, project JIRA00 58.1 → 31.1, stored context 31.5 → 24.9; realistic corpus (25,690) 3.3 → 2.0, 8.3 → 5.7, 7.3 → 3.9. What remains on the hub corpus is the count, `proposal_count` (#778), 15–31 ms.

## #776 — similar_text: words in more than 5% of the items don't count

Ruled 2026-10-14 by the deputy for Björn. Posted in the body of #776; the probe-key bundling in a comment on #776 before dispatch.

**The fork:** an exact top ten has to judge every pair that shares a lexeme, so on templated text (the probe's 42-word corpus) the first pass still took 17–19 min over 22,000 items.

**Ruling:** a lexeme in more than 5% of the prose items is a common word. It counts neither for finding candidates nor for the score. Under 200 prose items nothing is common. Frequencies are stored and kept up to date incrementally; a full recount is allowed when the corpus changed by more than 10%. A lexeme that crosses the threshold re-ranks only the items that hold it. The "≥ 5 shared" floor and the top-ten cap stay, counted over uncommon lexemes. Proposals may change; stop and report if more than 10% of the realistic corpus's would go. A 42-word corpus may end up with almost no proposals. A new table or column gets a migration plus a §10.8 file. Bundled: key the probe's after-pass snapshot on the pass alone, by the smaller of two changes.

**Reasoning:** template words aren't similarity, and judging pairs on them is what made the pass take minutes.

**If you disagree, the cost of reversing this is:** low to medium. Migration `0034` adds two derived tables and a column; dropping the rule means one more migration and a full pass.

As built (#776): the counts are exact incrementally (`COUNT_LEFT`/`COUNT_ARRIVED`: every statement keeps `items` equal to the sum over `sync.similar_text_counted`), so no recount was needed and none was written. A flip deletes the seen rows of the items that hold the flipped lexeme, in the same transaction as the counts. Forks decided by the implementer: the reason ("both mention ...") lists only uncommon lexemes. A title lexeme is kept out of the query when it is common, which is the same test the oracle applies. To reach the 60 s target on the realistic corpus, which has no common words, two more changes were made. The second arm finds settled counterparts through a GIN index on a new `sync.similar_text_seen.fts`, not `item_fts_idx` over the whole mirror; that was 34 of 75 s. And `PRUNE_SEEN` runs once a pass, not in every batch (13 s): an item that leaves mid-pass is kept out by `verified` and by a liveness test added to `RETIRE_SIMILAR`. The probe key: the tray read and answers moved to `suggest/tray.rs`, and the key hashes `suggest.rs` alone. That was the smaller change: hashing the statements would miss the pass's Rust.

Measured (`just responsiveness-probe`, 22,000 items, the pass alone): realistic corpus 76.9 → 40.8 s never analyzed and 72.4 → 46.6 s analyzed, with the 25,690 proposals byte-identical before and after (it has no common words). 42-word corpus 1,203 s → 4.5 s never analyzed and 17 min (#773's reading, not re-run by the deputy's ruling) → 4.5 s analyzed; 152,198 standing proposals → 0, as the ruling expected. `just suggest-perf`: nothing-new pass 228 ms (budget 1,000 ms).

## #809 — the ticket-first shell frame: where the switch lives, and six smaller forks

Ruled 2026-10-15 by the implementer (Björn, 2026-10-15: forks that come up while implementing are the implementer's). Recorded in PR for #809.

**The fork:** the switch between the rooms UI and the new shell needs a home, and the spec says only "a setting at startup". `knobas.setting` behind a new command pair, or the webview's storage.

**Ruling:** the webview's `localStorage` (`knobas.shell`), read once at startup; switching reloads the window. No new command. A dev build also takes `?shell=tickets` for `?fake-ipc` screenshots.

**Reasoning:** the switch lives until TF5 removes the rooms UI, it must be known before the database is up (the boot screen belongs to the chosen shell), and a share export should not carry a colleague's half-built UI choice. `pending-comments.svelte.ts` already keeps a per-viewer value there.

**If you disagree, the cost of reversing this is:** low. One read and one write in `lib/tf/shell-choice.ts` become a command pair with its §10.8 entry and a fake-IPC handler.

**Smaller forks, same ticket:**

- Every class in `tf.css` starts with `tf-`. `app.css` styles bare class names globally (`.btn`, `.toast`, `.timer`, `.search`), and those rules would leak onto the new shell. `tf-css.test.ts` holds it. Reversing at TF5, when `app.css` goes, is a rename.
- `Esc` on a full view with no job goes to Tickets. Prototype-6 does nothing there while its breadcrumb says *Back* beside `Esc`.
- A count badge is drawn only once its read has answered and only above zero. Prototype-6 always has something to count; the rooms strip already hides a zero.
- An unreachable source is plain text in the status bar, with no err tone and no *Re-enter password*. R-1.5 asks for credential failures. The scheduler retries an unreachable server on its own (P7).
- The write queue, the launcher, the worklog draft and the timer picker stay as the rooms UI draws them. They mount outside `.tf`, so they keep their own tokens. Porting them belongs to the tickets that redraw them (Time is TF2).
- The Job tab names the job last opened, else the ticket the timer runs on (R-1.2). The breadcrumb and `Esc` use only a job actually opened (R-1.6, prototype-6's `S.job`).
- The timer's clock is Archivo with tabular figures, not JetBrains Mono as prototype-6 draws it: the design system says durations are Archivo and mono is for code values, and the design system decides where it and the prototype differ.

**Merge review, PR #885** (ruled 2026-10-15 by its merge-manager):

- Four acceptance claims had no test that could fail: the rooms router and keys being off under the new shell, the queued-writes count opening the queue, the Monitors badge, and the toasts. Each now has a test, and its mutant dies.
- Under the new shell, the chip and `⌘T` with nothing running open the picker, not a timer on the current job (R-1.4's second half). R-1.4 is not in #809's list, and the job page that would supply the current job is #816. Filed as #886, blocked by #816. Reversing: one `foreground` branch.
- The toasts draw `action` plus `more`, though R-1.8 says "may carry one action". The store is shared with the rooms UI, and #752's refused-write toast needs both buttons. Dropping the extra ones would drop a caller's verb. Reversing: one line in `TfToasts.svelte`.
- `App.svelte` passing `toggleTimer` as the new shell's `ontimer` has no test of its own. It is one prop, read in review, and `TfShell.test` covers the shell side. Not fixed.
## #822 — `CommitFile`: how a moved file becomes a held write

Ruled 2026-10-15 by the implementer, under the TF6 milestone's rule (Björn, 2026-10-15: the
implementer decides the forks that come up while implementing). Recorded in PR for #822 too.

**The fork:** the ticket says a `sha` mismatch "becomes a held write carrying both versions, the
queue's existing held state". The queue holds only when its own projection of the mirror moved, and
the mirror holds no files, so nothing in the queue can see a file change. Something has to carry the
source's "this moved" and the file as it is now from `Source::write` to the flush.

**Ruling:** a new `SourceError::Moved { base, current }`. The adapter answers it when the source
refused a stale base and the file now reads at another version. The flush holds the write with the
queued snapshot plus `text` (theirs) and `base`; the reader's version is the payload. *Apply anyway*
writes `base` into the payload's `base_sha` and keeps the queued snapshot, so the next flush sends
against their version, and a file that moved again holds it again. The projection is the
repository's liveness alone. Smaller forks, same PR: the target field is the ticket's `repo` (the
repository; the queue orders commits per repository); a new branch is Gitea's own `new_branch` in
the same `PUT`, not a separate `create_branch`; a protected branch the token may not push to is
refused before the request, since Gitea's 403 says only *user cannot commit to repo*; the moved
check is a re-read after a 422, not the 422's sentence and not a read before every commit; a file
missing on the branch is Gitea's 404 refusal, never a create; text only; not an unclaimed-write op;
the receipt names the commit; `Moved` maps to `IpcErrorCode::Conflict`; a 422 where the file
already holds this commit's text is a re-send of a commit that landed and is answered as delivered
(review finding, ADR-0012); *Edit* stays the plain amend, whose caller sets `base_sha` to the held
`base` (#845). The live suite witnesses
the adapter half (Moved with Gitea's blob and text); the queue half is witnessed by
`knobas-sync`'s DB test over a fake answering `Moved`, since the queue's own code is the same for
every source.

**Reasoning:** the write did not happen, so it is an error and not a receipt. Two strings keep the
error plain data across a process boundary. Writing `base` into the payload is the only release
that lets the source's own check pass: copying the held side over the queued snapshot, as for a
mirror hold, would hold again on every flush, and skipping the check would be the silent overwrite
the queue exists to prevent.

**If you disagree, the cost of reversing this is:** medium. `SourceError` is a frozen surface, and
removing the variant touches four matches. The queue changes are one arm in `attempt`, one helper and
one `case` in `apply_anyway`'s statement. `update_page` and `set_parameter` still refuse on their
bases and could move to `Moved` in a follow-up.

## #822 — review rulings (merge-manager, PR #887)

Recorded in PR #887 too. Fixed on the branch: the contract battery's `DeleteBranch` comment had
been left above the new `CommitFile` probe and now sits above its own probe again; the `SourceError`
doc says again that every variant carries at most a string and a status.

- **The held row is not witnessed against the real Gitea.** The live suite reaches `Moved`; the
  hold is the DB test over a fake. **Ruling:** merge on the split and file #888 for a
  `start_work_live` case. The queue code is the same for every source. **Reversing:** #888 itself.
- **A file deleted on `branch` between the read and the commit** is refused (the re-read's 404), not
  held. **Ruling:** stays a refusal. There is no "theirs" text to show and no blob for *Apply anyway*
  to re-send against. **Reversing:** low, one arm in `commit_file`'s 422 branch.
- **A `new_branch` name a glob protection rule covers** gets Gitea's bare 403, not the guard's
  reason. **Ruling:** no change. The ticket's case is a commit onto a protected branch, which is
  guarded. **Reversing:** low, a protection read of `new_branch` too.
- **A file above Gitea's contents size limit** comes back without `content`, so its "theirs" reads
  empty and a landed re-send of it would be held rather than delivered. **Ruling:** no change. That
  fails safe, as a hold the reader can see. **Reversing:** low.
- **Naming** (`Moved { base }` is the source's current blob, and `moved` builds a snapshot rather
  than acting). **Ruling:** keep. `base` is what the re-send is made against after *Apply anyway*,
  which is the same sense as `base_sha`. **Reversing:** low before #845 reads the key, medium after.
## #811 — the ticket keys and Sprint discovery: the implementer's forks

Ruled 2026-10-15 by the implementer (forks are the implementer's since 2026-10-15), in PR for #811. Each is fork / ruling / reasoning / cost of reversing.

- **Nine keys or eight.** The ticket's title says nine; its body hands `has:` to #815. *Ruling:* eight here. *Reasoning:* the body is the contract. *Reversing:* none needed.
- **Negation.** R-3.14 has *is not*. *Ruling:* `-key:value` on the eight keys only; a minus on any other key stays search text, as before. *Reasoning:* `-word` is websearch's own negation and must keep working; widening it to every key changes old queries. *Reversing:* low, one branch in `apply_token`.
- **What a miss does.** *Ruling:* a row whose field is undeclared or absent fails the key and passes its negation, notes and estate rows included. *Reasoning:* ADR-0007's miss is absence; "not Done" is true of a ticket with no status. *Reversing:* low, the `coalesce` in `corpus_predicates`.
- **Case and quoting.** *Ruling:* values match ignoring case, a quoted value is read inside its quotes, `project:` matches key or name. *Reasoning:* JQL compares these names ignoring case; the editor writes names with spaces quoted. *Reversing:* low.
- **`assigned:`.** No record holds an assignment time. *Ruling:* `assigned:<window>` is "assigned to me and changed within the window", `updated:`'s windows. *Reasoning:* the inbox's *new assignment* makes the same admission; prototype-6 offers *today* and *this week*. *Reversing:* medium if a real assignment time is wanted, which needs the changelog.
- **`assignee:@me` without an identity.** *Ruling:* matches nothing. *Reasoning:* `@me` does the same; widening to everything would be a guess. *Reversing:* low.
- **Where sprints live.** The Sprint field's id is per instance. *Ruling:* `map` writes `sprints: [{id, name, state}]` beside the record on **every** ticket, `[]` when unconfigured; `KindPaths.sprint_state` reads the states; cursor 3 → 4. *Reasoning:* Epic Link's #598 precedent; always writing the key is what lets battery clause 3 tell "in no sprint" from a misspelt path against mockd, which serves no sprint field. *Reversing:* medium: a cursor bump and every ticket re-read.
- **Sprint discovery's name fallback.** *Ruling:* only a custom field named `Sprint` that describes no plugin key. *Reasoning:* stricter than Epic Link's, because "Sprint" is a common name for other plugins' fields. *Reversing:* low.
- **Suggestions' cost.** The whole mirror cost 4.3 s on the opening read. *Ruling:* values from the newest 200 live rows of each declaring source and kind, the most used 50 per key, declarations resolved once a statement; 62 ms against 28 ms before. *Reasoning:* a value only older tickets carry still filters when typed; the opening read must stay inside the launcher's budget. *Reversing:* low, two constants in `suggest.rs`.
- **The keys' own cost.** *Ruling:* a query with a ticket key resolves the declarations in CTEs once a statement (`corpus::FIELD_PATHS`) rather than per row. *Reasoning:* the mini board's #753 remedy; measured `status:Done` over 22,000 tickets at the cost of a text query matching all of them. *Reversing:* low.
- **Which active sprint.** R-3.14 says "the active one". *Ruling:* `sprint:current` matches a ticket in any sprint whose state is active, as JQL's `sprint in openSprints()` does with parallel boards. *Reasoning:* a ticket view has no board to pick "the" sprint from; TF5's Jira views read a board's own sprint. *Reversing:* low.
- **A source saved before this.** *Ruling:* as with Epic Link, the Sprint field is discovered by *Test connection* and filled in by the Add-source dialog; a source saved before #811 has none until its config names it, and until then every ticket carries `sprints: []` and `sprint:current` matches nothing. *Reasoning:* the ticket asks for discovery "the way it discovers Epic Link", and #297 left discovery for a saved source to a later ticket. *Reversing:* #889.
- **The seeded sprint.** *Ruling:* board `PAY sprint board` over `project = PAY ORDER BY Rank ASC`, sprint `PAY Sprint 1` started for two weeks, holding PAY-231. *Reasoning:* the fixture names no sprint; PAY-231 is its ticket in progress. *Reversing:* low.
- **A suggestion with no detail.** *Ruling:* the launcher draws the value alone, without ` · `. *Reasoning:* statuses and labels have nothing to say beside them. *Reversing:* trivial.

## #811 — the merge-manager's review findings (PR #890)

Ruled 2026-10-15 by the merge-manager, in PR #890. Each is finding / ruling / reasoning / cost of reversing.

- **Negation was untested for five keys' parse and for `-assigned:` and `-project:` at command level.** *Ruling:* fixed. `every_ticket_key_negates` parses each key and its negation, `-assigned:week` keeps mine-but-stale and somebody else's, `-project:` keeps the other project's ticket. *Reasoning:* the ticket asks negation of each key. *Reversing:* none needed.
- **No mirror row with an undeclared path was run through the `coalesce` predicate**, only notes, which take the constant `false`. *Ruling:* fixed with `a_mirror_row_whose_kind_declares_no_path_fails_the_key_and_passes_its_negation`; it dies when the `coalesce` default is flipped. *Reversing:* none needed.
- **`assigned:`'s suggestion details said "changed today" and "changed this week"**, but the window is rolling (`now() - n days`), as `updated:`'s is. *Ruling:* the details now say "changed in the last day" and "in the last 7 days"; the window is unchanged. *Reasoning:* the words should say what the predicate does, and `updated:` already reads windows this way. *Reversing:* trivial.
- **A bare `-status:Done` lists notes, assets and routes.** *Ruling:* keep the implementer's ruling. *Reasoning:* ADR-0007 treats a miss as absence. *Reversing:* low.
- **The seed is not idempotent after a partial failure.** If the filter is created and the board is not, the next run posts the filter again and Jira refuses the name. *Ruling:* no change. *Reasoning:* it is testenv only, fails loudly, and a fix would need a live Jira run to prove. *Reversing:* low, a filter lookup before the create.
- **The seed's control finds the Sprint field by the same plugin key the adapter uses**, so the live id equality cannot catch a wrong key. The name fallback has no live witness. *Ruling:* no change. *Reasoning:* the unit tests pin the key (and its mutant dies). A sprint state read through the field, which the live test asserts, would not be "active" if the field were wrong. *Reversing:* low.
- **Standards smells:** the key→field mapping is written in both `LIVE_ITEM_FIELDS` and `FIELDS_SQL`; `epic_link_field` and `sprint_field` travel together; `bind_dimensions` gets patched after it returns. *Ruling:* no change, apart from `suggest.rs` now using `query::ME` instead of a second `"@me"`. *Reasoning:* both statements go through the same `field_of!`/`list_of!`, so a field is read one way. The rest is judgement, not a rule. *Reversing:* low.

## #823 — clone_repo: one command with a stop argument, an event, and nine smaller forks

Ruled 2026-10-15 by the implementer (Björn, 2026-10-15: forks that come up while implementing are the implementer's). Recorded in the PR for #823.

**The fork:** a clone has to be cancellable, and ADR-0023 names one IPC command, `clone_repo`. A second command (`cancel_clone`) or an argument on the one.

**Ruling:** one command, `clone_repo(entityId, stop?)`. `stop: true` stops the running clone and answers once the half-made folder is removed.

**Reasoning:** ADR-0023's consequences say "One IPC command, `clone_repo`", and a ruling may not contradict an ADR.

**If you disagree, the cost of reversing this is:** low. A `cancel_clone` command calling `checkout::clone::stop`, its §10.8 file, and one line in `cloneRepo`.

**Smaller forks, same ticket:**

- **An event, not a Channel.** Progress goes on `clone:progress`, broadcast, because every view showing the repository draws the same *Clone* button (R-8.25). It is coalesced to at most one per second plus one per change of step, `activity:new`'s rate, to keep the "events carry coarse state" rule. Reversing: a `Channel` argument and the component listening to it.
- **One percent for the whole clone.** git counts each step 0–100. The button shows one number, so receiving objects is 0–80, resolving deltas 80–95, writing files 95–100, and the server's own steps 0. Reversing: one table in `parse_progress`.
- **`.` and `..` are refused as a name half,** though ADR-0023's pattern lets them through: the repository half is the folder name, and `<clones root>/..` is not a new folder. Stricter, so it does not contradict the ADR.
- **knobas makes the folder itself** with `create_dir`, then git clones into it. That makes "never write into an existing folder" atomic, and a failure or a stop only ever removes a folder this run made. An empty existing folder is refused like any other non-clone, though git would clone into it. A missing clones root is created.
- **Adoption needs a finished clone.** A clone killed by a quit already has its `origin`, so it would match by remote. Adoption also asks that `HEAD` names a commit; a half-made clone is refused with "did not finish. Remove it and clone again".
- **The address.** The URL is `base_url` + `/<owner>/<repo>.git`. The scheme must be https or ssh; a query or fragment is refused. The source's `clone_urls` (#683) are not used: they are where CI clones from, not where the person does.
- **The error line is git's first `fatal:` line,** else its last `error:` line, else its last line. R-8.25 says "git's last line", but for a repository the server does not have the last line is "and the repository exists.", which is advice and not the error. The cause is `auth`, `disk_full` or `other`, matched in English under `LC_ALL=C`.
- **The environment.** `GIT_ALLOW_PROTOCOL`, `GIT_CONFIG_PARAMETERS` and `GIT_CONFIG_COUNT` are removed, because each would override the `-c protocol.*` flags. `-c submodule.recurse=false` is passed beside `--no-recurse-submodules`. `GIT_TERMINAL_PROMPT=0` makes a missing credential a failure git reports instead of a prompt nobody can answer.
- **The test-only door.** The testenv Gitea is http with ssh off, so the live suite clones through `start_admitting(.., &["http"])`, behind `test-util` like `AppState::over_pool`. The production path's https half has no live witness; handed to the milestone review.
- **Small behaviours.** A branch address clones its repository. A second start while a clone runs answers `cloning` and starts nothing. A stop that arrives as git exits keeps a finished clone and answers `not_cloning`; a failed one is removed either way. The running clones live in a process-wide registry, not Tauri-managed state, so `setup` is unchanged. A start reserves its registry entry before it looks at the disk, so two starts at once start one clone.

**Self-review (Standards and Spec axes), ruled by the implementer:**

- Stop kills git but not the helpers it started (`git-remote-https`, `index-pack`), which can write into the folder after it was removed. Fixed without a new dependency: removal is retried until the folder stays gone for 100 ms, up to five seconds, and `removed` reports the outcome. Killing the process group would need `libc`. Reversing: one loop in `remove`.
- The live stop now waits for git's first progress line, so it stops a clone that is talking to the server, not one that has not started.
- The git lookup and Apple's shim check are one function in `working_tree` (`usable_git`), shared by the status read and the clone.
- ssh is allowed but reached only when a source's address is `ssh://`: a Gitea `base_url` is its web address, so in practice a clone is https. Building an ssh URL would need an address the person typed for it, and #823 names none. Not changed.
- An empty repository cloned outside knobas has an unborn `HEAD` and is refused as "did not finish", like a killed clone. The two cannot be told apart from files. Not changed; the person can set the path by hand.
- The app's `FailureCause` mirrors the core enum rather than core deriving `Serialize`, so `knobas_core::checkout` stays free of wire types like the rest of it. Not changed.

**Merge review (PR #892, deep pass), ruled by the merge-manager:**

- **A removal still retrying could remove a folder a later start made.** The runner released its registry entry before `remove` ran (a failed clone), and `stop` took the entry out before the runner removed the folder. `remove` retries `remove_dir_all` for up to five seconds whenever the folder is still there, so a retry of the same repository, or a start of another repository with the same name (`a/x` and `b/x` share `<root>/x`), could make the folder inside that window and have it deleted under its own git. *Ruling:* fixed. The entry is held until the folder is removed: `stop` only takes the entry's stop sender, and the runner releases after `remove`. A start whose folder another repository's run holds is refused with `conflict` ("is being cloned from another repository of the same name"). *Witness:* `a_folder_another_repository_is_cloning_into_is_refused` (dies with the path check disabled). The same-repository window is milliseconds wide and has no test. *Reversing:* low.
- **Stop kills git, not its process group.** *Ruling:* keep the implementer's retrying removal. *Reasoning:* the helpers exit once their pipes to git close, and with the fix above the retry can no longer touch another run's folder. *Reversing:* low, `process_group(0)` plus `killpg` behind `libc`.
- **A start future dropped between `create_dir` and the spawn would leave the folder and a registry entry behind.** *Ruling:* no change. *Reasoning:* Tauri does not cancel a running async command; nothing else drops it. *Reversing:* low, a guard that releases and removes on drop.
- **A `base_url` carrying `user:password@` reaches git's URL.** *Ruling:* no change. *Reasoning:* the address is the one the person typed; knobas adds no credential of its own, which is ADR-0023's rule. *Reversing:* low, refuse userinfo in `clone_url`.

## #824 — the uncommitted diff read

Ruled 2026-10-15 by the implementer, under the TF6 milestone's rule (Björn, 2026-10-15: the
implementer decides the forks that come up while implementing). Recorded in the PR for #824 too.

- **A command, not a field on `CheckoutView`.** The view is read on every detail open and every
  window focus; a diff reads file contents (3.7 s worst case measured on a 16,970-file checkout).
  So `entity_checkout_diff` is read when the *Local changes* tab opens. **Reversing:** medium, move
  the read into `checkout::view` and drop the command and its mirror.
- **`-c diff.autoRefreshIndex=false`, a flag ADR-0026 does not list.** The ADR's command as written
  rewrites `.git/index` when a file's stat is stale, which the same ADR forbids and its own
  consequence ("`.git/index` unchanged after a read") tests. The forbidden-writes clause wins;
  #891 amends the ADR's command list. **Reversing:** low, but it would bring back an index write.
- **`--src-prefix=a/ --dst-prefix=b/`, also not in the ADR.** A checkout's `diff.mnemonicPrefix`
  or `diff.noprefix` otherwise moves paths through the shared parser, whose strip expects `a/` and
  `b/`. Constant flags, no value from the mirror. Covered by #891 as well. **Reversing:** low.
- **Two diff halves, each capped as its own view**, not one per-view cap across both. Each half is
  its own diff block on the tab, and the parser's per-view cap is not reachable from outside
  `knobas-source` without a frozen-surface change. **Reversing:** low, a shared budget once the
  parser takes one.
- **"Git is stopped past the caps" is a read cap of 24,000 lines or 16 MiB per half**, four views'
  worth, so a lockfile over the per-file cap does not hide the files after it. Past it git is
  killed and the half and the file the cut fell in are `truncated`; that file's counts cover only
  what was read, unlike a file the parser alone cut. **Reversing:** low, one constant.
- **Untracked paths from a second `status` (`--porcelain=v2 -z`)**, not from the working tree
  read, which only counts them. `-z` so a path is never quoted; a rename's original path is
  skipped as its own field. At most 1,000 paths, with `untracked_truncated`. **Reversing:** low.
- **Timeout 10 s over all three commands**, from the measurement above (0.25 s with fresh stat
  data, 3.7 s with every stat stale). **Reversing:** one constant.
- **No checkout is `not_found`**, as `open_checkout` refuses it, rather than an empty answer: the
  tab draws its empty state from `CheckoutView` and never asks. **Reversing:** low.

### Merge-manager review (PR #893)

- **One line longer than the byte cap was read whole.** `run_capped` checked `READ_BYTES` only
  between lines, so a 200,000-byte line was read entirely under a 300-byte test cap.
  *Ruling:* fixed; each read takes at most the bytes left under the cap, witnessed by
  `the_byte_cap_holds_inside_one_long_line`. *Reversing:* low.
- **At the cut git is killed only after its stderr closes.** git dies of SIGPIPE on its next
  write; a git busy before that write (a slow clean filter on the next file) holds the read until
  the 10 s timeout, which then answers `failed` and drops the partial read. *Ruling:* no change.
  *Reasoning:* the timeout bounds it and `kill_on_drop` ends git; the common case (more diff to
  write) ends at once. *Reversing:* low, kill from inside the read on the cut.
## #810 — Tickets: one list with the filter bar and a preview: the implementer's forks

Ruled 2026-10-15 by the implementer, in the PR for #810. Each is fork / ruling / reasoning / cost of reversing.

- **Which read draws the tickets.** *Ruling:* `mini_board`, with a `query` argument and five more card fields (`issue_type`, `assignee`, `updated_at`, `path`, `mine`), not a new command. *Reasoning:* the board's columns, their order, the per-column cap and the totals already live in that read; a second read would restate them. *Reversing:* medium: a command, its §10.8 entry and a fake handler.
- **How the server filters.** *Ruling:* `Searcher::matching_ids` runs the launcher's parse and key predicates (`ids_sql`) for every admitted ticket id, uncapped, and the board narrows to those ids. *Reasoning:* R-2.7 says the grammar, not the browser. A very broad filter at estate scale sends many ids; no screen is slow on testenv data. *Reversing:* low.
- **What the typed words match** (from the spec review). The launcher's full-text match covers only the title and the body, so a key, an assignee or an epic typed into the filter found nothing, stopwords emptied the board, and the marks disagreed with the matches. *Ruling:* in the view, each word must appear as a substring, ignoring case, in the key, the title, the declared assignee or a declared parent's name; the body is not searched. The grammar's keys are unchanged. *Reasoning:* R-2.3 ("every word typed has to appear somewhere in the row") and the placeholder "Filter by key, summary, epic or person" are prototype-6's `ticketMatches`. *Reversing:* low, one predicate in `ids_sql`.
- **The card's comment count** (R-3.7). No source declares a comment count, and the mirror holds Jira's comments only inside `body_text`. *Ruling:* not drawn; follow-up #894 for a declared count. *Reasoning:* a payload read outside a declaration is what ADR-0007 forbids. *Reversing:* the follow-up.
- **The epic's name.** Prototype-6 draws "PAY-200 Payout reliability"; the card's `path` line is the epic's key. *Ruling:* the key only, in this PR, folded into #894. *Reasoning:* the name is the parent's own title, a second read the path line does not make. *Reversing:* the follow-up.
- **"Every match while a filter has text".** *Ruling:* the read is uncapped only while words are typed; chips and dropdowns alone keep eight a column. *Reasoning:* prototype-6 lifts the cap on `f.q` only. *Reversing:* one line.
- **Show N more.** *Ruling:* the next read's per-column cap becomes the largest expanded column's total, and the column draws every card. *Reasoning:* an uncapped read of an All board is what #753 removed. *Reversing:* low.
- **The list layout.** *Ruling:* the same read as the board, newest first, ending in *Show N more* that lifts every column's cap. Header sorting, `j`/`k` and lanes are #813's; the Signals column waits for #815. *Reasoning:* the ticket's siblings own them. *Reversing:* none needed.
- **The left pane before views exist.** *Ruling:* *All tickets* with its count, then *Not a ticket*. *Find a view*, *+ New view*, *Edit view*, the ⋯ menu and *Save as a view* come with #812. *Reasoning:* nothing to list or save yet. *Reversing:* none needed.
- **Where an ad-hoc context goes.** *Ruling:* `#/job/<ctx id>`, the job's address, which the Job tickets take over. *Reasoning:* the job is where work without a ticket happens too (R-3.11); the empty Job page names it until then. *Reversing:* one line.
- **Chips and dropdowns.** *Ruling:* *Type* and *Priority* chips from `filter_suggestions` (at most six each), *Assignee* (`@me` as *Me*) and *Epic* (`under:` over parents of kind ticket) as dropdowns. A group with no values is not drawn. *Reasoning:* prototype-6's TCFG, filled with values the mirror has. *Reversing:* low.
- **Whose ticket.** *Ruling:* `mine` is the server's, the assignee against the enabled sources' usernames ignoring case, on every card. *Reasoning:* the frontend has no identity read, and `@me` already means this. *Reversing:* low.
- **Initials.** *Ruling:* from the account's spelling (`mara.lindqvist` is ML, `mara` is MA). *Reasoning:* the declared assignee is an account, not a display name. *Reversing:* low, if a display name is declared later.
- **Around this ticket.** *Ruling:* the ticket's promoted context, read as a room under `latest_per_parent`, its tally grouped as the room's tiles group kinds (`kinds.ts`'s `sectionOf`), *n failing or down* for tone `err`. No context reads "Nothing connected yet." Builds say *builds*, not *configurations*; there is no *needs you* and no *Talk* line yet. *Reasoning:* today's reads; R-4 sections bring the rest. *Reversing:* low.
- **The preview's button.** *Ruling:* prototype-6's rule: *Start work* for a To Do ticket of mine, *Open job* for a done one or one not mine, *Resume work* for the rest of mine. It links to `#/job/<key>`; `Enter` presses it from anywhere but a text field or another control. *Reasoning:* copy and behaviour follow the prototype; #816 and #818 wire the job. *Reversing:* one function.
- **Selected card markup.** *Ruling:* `aria-current="true"` and the `tf-card` class (the card's focus ring then follows the design system's `--card-ink` rule), not `aria-selected` on a button. *Reasoning:* `aria-selected` is not allowed on a button. *Reversing:* trivial.
- **Which card is selected.** *Ruling:* the session's selection when it is on screen, else the first card in reading order. *Reasoning:* prototype-6's rule. *Reversing:* trivial.
- **The fixture.** *Ruling:* eleven more PAY tickets in the fake IPC's corpus, flat like the rest, with type, assignee and epic. *Reasoning:* a board of three tickets cannot be compared with prototype-6's. *Reversing:* trivial.

## #810 — the merge-manager's review findings (PR #895)

Ruled 2026-10-15 by the merge-manager. Each is finding / ruling / reasoning / cost of reversing.

- **A word matched the entity id's namespace.** `ids_sql` matched words against `i.entity_id` (`jira:PAY-13`), so typing `jira` admitted every Jira ticket and marked nothing. *Ruling:* fixed. The key is the id past its first `:`, which is what the card draws and what the fake IPC matches. *Reasoning:* R-2.3 says every typed word appears in the row, and the namespace is not drawn in the row. *Reversing:* one expression.
- **`Enter` on a focused card that was not selected opened the selected ticket.** The window's keydown comes before the card's click, so `current` was still the old selection. *Ruling:* fixed. A focused card is the card `Enter` means, and pressing it also selects it. *Reasoning:* R-3.9. *Reversing:* trivial.
- **The list's *Show N more* lifted the board's caps for the rest of the session.** *Ruling:* fixed. The list keeps its own `listAll` flag, and the board's columns stay at 8 and 3 until their own *Show N more* is pressed. *Reasoning:* R-3.4. *Reversing:* trivial.
- **The typed read is uncapped, and every keystroke reads per row.** 20k tickets matching one letter came back as 4.1 MB and 575 ms in a debug build. *Ruling:* merged as built, because R-3.4 says every match while there is text, and follow-up #897 owns responsiveness at estate scale. *Reasoning:* this does not contradict the spec, and the cost only shows on broad words over large mirrors. *Reversing:* #897.
- **Kept:** the identity is loaded twice per filtered read (one small query each time), `ids_sql` hard-codes the `i` alias (which `declared_string!` already requires), and the dummy `nothing` CTE stays.
## #816 — the job tracer: the implementer's forks

Ruled 2026-10-15 by the implementer, in the PR for #816. Each is fork / ruling / reasoning / cost of reversing.

- **Which ticket `#/job/<key>` names.** An entity id is `<source>:<key>`, and the address carries only the key. *Ruling:* the key is looked up under every configured source with `get_entity`, and the first ticket in the source list wins; an address that holds an id (`#/job/jira:PAY-231`) is read as one. *Reasoning:* prototype-6's address is the key a person reads, and no read resolves a bare key today. *Reversing:* low, `resolveTicket`.
- **Where type, priority and estimate come from.** The shell may not read a payload (ADR-0007). *Ruling:* `get_entity` answers `issue_type`, `priority` and `estimate_seconds` through declared paths, and `KindPaths` gains `estimate` (seconds, Jira's `timeoriginalestimate`). *Reasoning:* the card needs them, and `status` already rides there (#760). *Reversing:* medium, a §10.8 surface.
- **How an estimate reads.** *Ruling:* hours and minutes (`est. 6h`), never days. *Reasoning:* the length of a working day is the Jira instance's setting, which knobas does not read. *Reversing:* low, `hm()`.
- **The address a Code row opens.** *Ruling:* `#/pr/<entity id>`, `#/repo/<entity id>`, `#/branch/<entity id>`, `#/commit/<entity id>`; `branch` and `commit` light the Code nav. *Reasoning:* R-4.10 asks for the item's full view, and a pull request's number is unique only inside its repository. The Code tickets (TF3) build those views and may change the address. *Reversing:* low, `itemHash`.
- **A workflow that cannot be read.** *Ruling:* the select shows where the ticket stands and offers nothing, with no corpus fallback. *Reasoning:* the ticket says reachable transitions only; the rooms detail's unverified offer is the thing this rules out. *Reversing:* low.
- **Which checkout the line shows.** *Ruling:* `entity_checkout` is asked of the job's branches first, then its repositories, at most three, and the first with a clone wins; with none, no line. *Clone* is not drawn. *Reasoning:* a branch answers with its repository's clone and the branch checked out; `clone_repo` (#823) has no UI ticket here. *Reversing:* low.
- **How many Code rows are read.** *Ruling:* one `list_entities` over the job's context for `pr`, `branch`, `commit`, `repo`, newest first, 200 rows, sorted by tone in the page. *Reasoning:* membership is one hop (ADR-0008); the attention order would drop merged pull requests, which prototype-6 draws. *Reversing:* low.
- **The tabs.** *Ruling:* *Work*, *Map* and *Conversation* are drawn; Map and Conversation say they are not built yet. The Connections button, *Also on this job* and the card's conversation summary are left to #820, TF5 and #819. *Reasoning:* the prototype draws the three tabs, and the tickets that fill them are open. *Reversing:* none needed.
- **The clock.** *Ruling:* the shared timer store's reading (`12:04`, `1:02:07`) in Archivo tabular, and *Start timer* goes through `App.svelte`'s `startTimerOn` (stop what runs into its draft, then start), whose toast is "Timing PAY-231." *Reasoning:* #809 ruled the clock face; one start path keeps the worklog-draft rule in one place. *Reversing:* low.
- **The blocked chip.** *Ruling:* a confirmed `blocks` link whose `to` end is this ticket, the standup digest's reading. A status the source calls blocked-like draws no chip. *Reasoning:* the chip names the blocker, and a status has none. *Reversing:* low.
- **The fixture.** *Ruling:* `?fake-ipc` gains a small link graph (PAY-231's repository, branch, pull request and commit), `reachable_transitions` answered with prototype-6's workflow, and `week_timesheet`. *Reasoning:* the screenshot needs the section filled; the mock adapter declares no workflow, so the fixture says so where it diverges. *Reversing:* none needed.

## #826 — the worklog draft in the new shell: the implementer's forks

- **The clock stops before the draft opens**, as in the rooms UI, not after the reader answers as
  in prototype-6. `worklog_draft` reads the day's closed, unlogged blocks, so a draft opened over a
  running timer would leave out the stretch being stopped, and the ticket says no backend change.
  The chip reads *No timer* behind the draft. **Reversing:** a backend draft that counts the open
  block, then the stop moves into `settle`.
- ***Keep running* starts the timer again on the ticket the stop took it off.** The next stop's
  draft concatenates the day's blocks, so both stretches are logged together; the seconds spent in
  the dialog are not counted. **Reversing:** low, with the change above.
- ***Discard* closes the draft with nothing written**, on every draft: after a stop the clock stays
  stopped and the time stays in the day, unlogged, as with *Stop, log later* but without its
  toast; on a switch the waiting timer still starts (*Keep running* is the only cancel). A draft
  no stop opened (the ad-hoc dialog's *Log to a ticket…*) has only *Discard* and *Log*. Deleting
  the stopped block was the other reading, and spec story 62 ("so that I never lose tracked
  time") argues against it. **Reversing:** low.
- **`Esc` and the backdrop answer quietly**: a stop's draft as *Stop, log later* (the clock is
  already stopped), any other as *Discard*. Prototype-6's backdrop is *Keep running*, which here
  would start a timer on a dismiss. **Reversing:** one line.
- **A switch waits for the draft (R-13.2).** Under the new shell the launcher's *Start timer* stops
  the running timer, opens its draft, and starts the new one only after *Log* or *Stop, log
  later* (or *Discard*); *Keep running* keeps the old ticket and says so in a toast. With nothing to
  draft it starts at once. While a draft is open or being read, a start (`⌘T`, the chip, the
  launcher) is refused with "Answer the worklog draft first.", because the clock is stopped under
  the dialog and a start would collide with *Keep running*. The rooms UI is unchanged.
  **Reversing:** low.
- **The new shell's log toast is prototype-6's**: "Queued: log 2h 44m to PAY-231." with *Show
  queue* (R-1.8). The rooms UI keeps its own words. **Reversing:** one line.
- **From and minutes, not From and To.** The draft logs a start and a sum of blocks; a To field
  would read as the span, which includes the gaps the sum leaves out. **Reversing:** low.
- **ADR-0012's sentence is kept** under prototype-6's "Goes to … through the write queue" line, as
  in the rooms UI's draft. **Reversing:** one paragraph.
- **The form is shared**: `DraftForm` holds the ticks, the comment's ownership and what is sent for
  both drafts, so the rules stay written once until TF5 removes the rooms UI. **Reversing:** low.
- **`?fake-ipc` answers the timer, `day_blocks`, `ad_hoc_block`, `worklog_draft` and `log_work`**,
  in memory, so a stop can be screenshotted (`&fake-timer` starts one). Block edits, *Log all* and
  the week stay unanswered. **Reversing:** low, move them back to `UNANSWERED`.

## #884 (gitea-live: a seeded pull request merged on the shared fixture)

- **Refuse, do not repair, a seeded open pull request found merged or closed.** The issue allowed
  re-create or refuse. A merged one has lost its head branch and its commit is on `main`; a
  re-created one would need a new commit on `main`-adjacent history. *Ruling:* the seed dies with
  the Gitea-only remedy (fresh `gitea-data` volume); `./reset` named as the all-products one.
  *Reversing:* low, one function in `testenv/gitea-pr-state.sh`.
- **The culprit is not a suite.** Every merge site in `live_gitea.rs` and `start_work_live.rs`
  merge a pull request the test opened. The commit on `main` ("PAY-236: Add payout CSV export")
  was rewritten at 12:12:45Z and merged 15 s later with the head branch deleted, in a run with
  hand-made branches and review text on the same repository: manual use of the app or UI.
  *Ruling:* no test change. *Reversing:* n/a.
- **The shared Gitea volume was reset** to meet the second criterion (it was the only way to make
  `just gitea-live` green); it now seeds the exact numbers #142 and #144 (the old volume had
  them at #1 and #2). Other worktrees re-mint their token on their next seed (#684).
## #830 Settings frame and Sources

- **Add a source opens the rooms UI's `AddSource.svelte`, mounted outside `.tf`.** #809 ruled
  that rooms-styled dialogs stay outside the new shell, unported, until a ticket redraws them.
  Porting the generated-form flow is a ticket of its own size. Follow-up #903. **Reversing:** low.
- **What it syncs is the schedule and the pause switch only.** The issue body names those two.
  R-13a.2 also lists Jira's projects or JQL, which would need the adapter's config form on the
  page. Follow-up #904. **Reversing:** additive.
- **The User row is the config's `username`**, the identity convention every adapter follows,
  drawn only when the adapter's schema declares it. Save sends the whole config with the edited
  value, because `update_source` replaces `config` whole. **Reversing:** low.
- **The schedule and the pause switch save as they change. Connection saves on Save**, which is
  disabled until the address or user differs from what is stored. Prototype-6 draws Save as
  always enabled. **Reversing:** one attribute.
- **Holdings say `N tickets` for a source of one kind and `N items` for several.**
  `SourceSummary.item_count` is one number across kinds, so prototype-6's split ("15 build
  configurations, 2 400 builds") would be a guess. **Reversing:** low, once a per-kind count exists.
- **Uptime Kuma's Account row says it is kept beside the token and never shown**, with
  *Change…*. No command reads a secret back (contract §2.2), so the account's user cannot be
  drawn as prototype-6 does. **Reversing:** none needed.
- **"Stored in the macOS keychain" on a Mac, "the system keychain" elsewhere.** The app builds on
  three platforms. **Reversing:** one line.
- ***Found when tested* appears after *Test connection* in this visit.** A test result is never
  stored (#326), so there is nothing to show before one. **Reversing:** none.
- **A re-entered password the source refuses keeps the dialog open** with "<name> refused it."
  instead of closing over a still-red source. **Reversing:** one branch.
- **The fake IPC fixture's refusing source is TeamCity, and Gitea now signs in**, so the page can
  be photographed at prototype-6's own `#/settings/source/teamcity`. **Reversing:** fixture only.
- **Sections other than Sources name themselves and offer the rooms UI** until #831 and #844 build
  them. **Reversing:** replaced by those tickets.

## #827 Time: Day (implementer)

- **The strip is SVG.** Prototype-6 positions blocks with inline `left`/`width`, which
  `style-src 'self'` and `house-rules.test.ts` refuse. SVG's `x` and `width` are attributes and
  take percentages; a nested `<svg>` per block clips its label. **Reversing:** low, one component.
- **08:00 to 18:00, widened by whole hours** when a row starts earlier or ends later, so an evening
  is not clipped off the strip. **Reversing:** one function (`stripWindow`).
- **Gaps between blocks are *untracked*, not *open*.** No read returns focused no-target stretches,
  only `week_timesheet`'s day total, which is the fifth figure. A gap may be lunch, so the rows say
  "Nothing tracked" rather than "app open". Follow-up #901. **Reversing:** with #901.
- **No *Dismiss* on offered blocks.** No command dismisses one, and `delete_block` would come back
  on the next read through `passive::materialize`. Follow-up #900. **Reversing:** with #900.
- **Whether a block is on a ticket is `ad_hoc_block`'s answer**, asked once per unlogged block on
  each day read. That decides *unlogged* against *needs a ticket* and names the ticket for *Add to*
  and *Log to*. **Reversing:** low.
- **The running timer is a row of its own**, from its start (or midnight) to now, and the only
  yellow. It counts in *tracked*, not in *to log*. **Reversing:** low.
- ***Log all…* goes to the Week tab**, where #828 builds its dialog. **Reversing:** one link.
- **Past the horizon with blocks**, the day draws them under a note, and *No record* only when
  there are none. Prototype-6 has only the empty case. **Reversing:** low.
- **The `?fake-ipc` day is #826's `fakeTime`, grown**, not a second fixture: today always carries
  prototype-6's Friday (logged, offered, unlogged, a gap, a label block), and `?fake-timer` adds the
  running timer. #826's own morning block went, since PAY-231's 09:40 to 11:50 gives a stop's draft
  the same thing to add up. **Reversing:** low.
- ***Log…* shows the draft through #826's `drafts.show`**, the path the ad-hoc dialog's *Log to a
  ticket…* takes, so it opens in the new shell's own dialog. **Reversing:** one line.

## #827 Time: Day (merge-manager, PR #915)

- **One `ad_hoc_block` per unlogged block on each day read stays.** It is a fan-out, not a
  serial N+1: the calls go out together under one `Promise.all`, and each is a handful of small
  indexed reads on the local database (the block, the logging sources, then at most a link, a
  context anchor, the last logged ticket and a title). Passive blocks under two minutes are dropped
  and adjacent visits merge, which bounds the count. A batched read would be a new command and a
  §10.8 entry, which this ticket's "no backend change" rules out. The by-eye check on testenv data
  goes to the milestone review; if the day is slow there, the follow-up is one read that answers
  the day's offers together and computes the logging sources and the last logged ticket once.
  **Reversing:** a batched command plus one call site in `TfDay.svelte`'s `read()`.
---

## #908 — splitting the fake-IPC fixture and the shell's page wiring per feature

Ruled 2026-10-15 by the implementer (PR #916).

- **Feature modules are found by a glob**, not listed in an index: `fake-tauri.ts` collects
  `fake/*.ts` and `tf/pages.ts` collects `tf/*/page.ts`. A hand-kept list is one shared line per
  feature, and two PRs adding adjacent lines conflict, which is what the ticket is about. Two
  fixture modules answering one command throw; `fake-features.test.ts` holds that every module
  under `fake/` has `handlers`. **Reversing:** low, replace each glob with a list.
- **Smart lists are their own module** (`fake/smart-lists.ts`), because
  `crates/knobas-search/src/lists.rs` `include_str!`s the file that holds `fakeSmartLists`; every
  other fixture module is then read by no crate. (Orchestrator's instruction.) **Reversing:** low.
- **The stored contexts and the link graph live in `fake/corpus.ts`**, beside the mirror, because
  `list_entities` scopes by them and `promote_context` reads `CORPUS`; split across two modules they
  formed an import cycle. **Reversing:** low.
- **`TfPage.svelte` takes one `ctx` prop**, so a page needing a new shell value edits
  `page-entry.ts`'s `TfPageContext` and `TfShell.svelte`'s `ctx`, not three prop lists. The bare
  `#/job`, a section not built yet and an unknown address stay in `TfPage.svelte` as the shell's
  own fallbacks. **Reversing:** low.
- **`App.svelte` keeps its wiring.** What a TF ticket touched there is the timer verb, the worklog
  draft flow and the launcher, which serve both shells; the rooms UI's routes, store starts and
  seeds are the rooms UI's and TF tickets do not edit them. Per-screen wiring was in `TfPage.svelte`
  and is now per feature. **Reversing:** a follow-up ticket if a TF ticket has to edit `App.svelte`
  for a screen.
- **After the rebase over #914 and #915 (merge-manager, PR #916), Settings and Time register as
  pages too**: `settings/page.ts` and `time/page.ts`, instead of the two `{#if}` branches those
  PRs added to `TfPage.svelte`. Time is drawn under the shell's breadcrumb, so a page entry may
  set `crumbs: true`; Settings draws its own and leaves it unset. Their fixture changes moved into
  `fake/sources.ts` and `fake/time.ts` unchanged. **Reversing:** low, the two branches return.
## #905 Worktree target size

- **Option 3 adopted: `[profile.dev] debug = "line-tables-only"` and `[profile.test]
  incremental = false`.** It cuts a fresh worktree's `target/` from 7.9 to 5.7 GB and a repeated
  one from 9.9 to 6.8 GB, for about 14 s on a 3-minute repeat gate; disk is what limits parallel
  tickets. Table in `testenv/README.md`. **Reversing:** delete one line in `Cargo.toml`.
- **Incremental is off in the test profile, not by `CARGO_INCREMENTAL=0` in the recipes.** The
  profile reaches every `cargo test` (the gate, live suites, perf recipes, hand runs), leaves
  `cargo clippy` incremental, and does not change any dependency's sccache key, so no run
  re-misses the cache. **Reversing:** low.
- **Test binaries stay at `debug = 0`.** The criterion "backtraces in test failures still name
  file and line" holds for the panic message, which names file and line from the panic location;
  frames named functions only before this change and still do. Line tables in test binaries
  would add size, which is the opposite of this ticket. **Reversing:** one line, if someone wants
  file and line per frame in test backtraces.

## #920 Gate stage timing

- **The script checks (`shell`, `test-layout`, `witness-unit`) run as a third half of `check`,
  beside the cargo chain.** None runs cargo or reads `target/`; their 23 s left the critical
  path, `just check` 186-196 s to 164-172 s. Every check still runs, and a red in one half no
  longer hides a red in another. **Reversing:** move the three names back into `cargo_recipes`.
- **`clippy-libs` stays.** It costs 5-7 s and lints `knobas-db` without `test-util`, which no
  `--all-targets` pass can do, because cargo unifies features within one invocation.
  **Reversing:** n/a, nothing changed.
- **`inventory` is left as is.** `test`'s own build after it is a 0-1 s no-op, so no binary
  compiles twice. **Reversing:** n/a.
- **`xargs -P` stays 6.** 8 and 10 were 3-6 s slower on `test`'s run; the pool is paced by
  `knobas-app`'s `test/it`. **Reversing:** `KNOBAS_TEST_JOBS`, or the constant.
- **`check-front` runs `vitest related` over the changed `app/src` files plus every fs-reading
  test, not the whole suite.** 39 s to 16 s on two real change shapes. The whole suite still runs
  in `just check`, and in `check-front` whenever config, the setup file, or a deleted or renamed
  file is in the change. It stops covering a test that depends on a changed file by neither import
  nor file read. The issue body asked for this exact trade when the saving is real.
  **Reversing:** `check-front` back to `just front`.

## #828 — Week and *Log all* in the new shell

Ruled 2026-10-16 by the implementer (PR for #828).

- ***Log all* sends one `log_work` per ticked row, not `log_all`.** The spec's dialog (story 162)
  has ticks and editable comments; `log_all` takes neither and sends every worklog with an empty
  comment. `log_work` already logs one day and ticket with a comment, so the dialog uses it and no
  command is added. Each row's comment starts from `worklog_draft`'s generated comment for that day
  and ticket, which the reader sees and can edit or clear before anything is sent. **Reversing:**
  low, call `log_all` and drop the ticks and comments.
- **The week is read again at send time.** `log_work` covers every unlogged block of the day on the
  ticket and logs the seconds it is given, so a timer stopped while the dialog was open would be
  covered at the old number. The dialog asks `log_all_preview` again and sends that answer's start
  and seconds. A row the week no longer offers is skipped. **Reversing:** low.
- **The running timer counts into today's cell on its target**, as tracked and to log, so the Week
  and Day tabs give the same number for the same morning (prototype-6: "Running time counts as
  tracked"). It is computed in the view; `week_timesheet` is unchanged. **Reversing:** low, drop the
  term in `week-view.ts`.
- **A held worklog's reason** comes from `pending_writes`: a `log_work` row in `held` or `refused`
  whose start falls in the week. The note uses the source's own words (`detail`) when the row has
  them. Without them it uses the queue's state ("the source refused it.", "its source is turned
  off.", "the ticket changed after you logged it."). **Reversing:** low.
- **The comment field is a two-line text area**, not the prototype's one-line input. A draft's
  comment has one bullet per line, and an input would drop the line breaks. **Reversing:** low.
- **`#/time/week/<date>` is the week containing that date**, with *‹* and *›* stepping by seven days
  and *This week* to return, like the Day tab's *Today*. The prototype's *‹* only toasted.
  **Reversing:** low.
- **The refused worklog lives in `fake/write-queue.ts`, not `fake/time.ts`**, although the shell
  conventions say a ticket edits its own feature's module. `pending_writes` is the write queue's
  command, and two modules cannot answer it. The hour it covers is already in `time.ts`'s week
  numbers. **Reversing:** low.
- **Merge review upholds the `fake/write-queue.ts` row** (merge-manager, PR #928). The convention
  exists so parallel tickets do not collide in one shared fixture file. A row a command returns lives
  in the module that answers that command, so a ticket may edit another feature's module to add a
  row of that module's own command; it still adds no handler outside its feature. The shell
  conventions now say so. **Reversing:** low, revert the one line in the conventions.
## #819 / #820 Conversation tab and summary, Connections drawer

- **Last visit is one command that records and answers, `visit_entity`.** Opening a job writes a
  `visited` line (actor `user`) and answers the newest one before it, in one transaction. A
  separate read and write would be two round trips on every open and a window where two reads
  answer the same visit. **Reversing:** low, split the command in two.
- **A visit is bookkeeping.** `recent_activity` (history panel, status bar) and the worklog draft
  leave `visited` lines out; a line per opened job would push real changes off both. **Reversing:**
  low, drop the two filters.
- **Mentions match the identity's accounts, not a display name.** Jira Data Center writes a mention
  as `[~username]`, and `@me`'s identity (`knobas_search::Vocabulary`) is the one answer to "who am
  I"; the inbox's mention rule is reused whole. Prototype-6 writes `@Mara`; the fixture writes
  `@mara`. **Reversing:** medium, a display-name source would need a declaration.
- **Changes are the status moves knobas sent.** The mirror keeps no Jira changelog and no PR event
  history, so *Include changes* draws the `transition` writes the queue settled as `sent`, as
  "You moved this to In Review". Moves made in Jira and PR events are #929. **Reversing:** n/a,
  #929 adds the rest.
- **The thread keeps the source's order**, which is oldest first for both spellings, rather than
  sorting by a stamp that may be missing. **Reversing:** low.
- **"N connected" counts the links on the ticket whose far end stands.** Link sets and the parent
  join in TF5 (#867). **Reversing:** low.
- **Proposals are the promoted context's tray (`room_suggestions`) filtered to the ones touching
  the ticket.** *Judge them one by one* opens `#/review/<link id>`, which draws "not built yet"
  until the Proposed links view (TF3). The fixture's `room_suggestions` now scopes a stored context
  to its anchor. **Reversing:** low.
- **The drawer's *Hidden on KEY* group is drawn from rows link sets fill (#867)**, so it never
  shows in TF1: without a set nothing can be hidden. **Reversing:** n/a.
- ***Link to…* offers prototype-6's relations as stored ids**, without *deploys*: no stored
  relation reads that way from a ticket (`deployed-from` points from the deploy). **Reversing:**
  low, one line.
- **A ticket with no comments offers *Comment* as a link to the Conversation tab**, where the box
  is, instead of a second comment dialog. **Reversing:** low.
- **A drawer row's title opens the item's full view** (R-4.10's rule for every row); prototype-6's
  rows are plain text. **Reversing:** low.

## #829 Time > Standup (PR for #829)

- **The protocol stays one Markdown note; the tab is a form over its body.** Prototype-6 draws who
  was there, a note per person and action items; the backend keeps a note whose template has
  exactly the headings `Attendees`, `Notes` and `Action items`, and publishes that body. Each edit
  rewrites the bullets of one section and keeps every other line (`standup-view.ts`). A person's
  note is the `Notes` bullet that starts with their name and a colon; a note is one bullet, so line
  breaks become spaces. **Reversing:** medium, a structured protocol would need its own storage
  and a §10.8 entry.
- **Who was there is the `Attendees` bullets.** No roster exists, so prototype-6's unpressed
  Lena Hoffmann cannot be drawn on first open. A person pressed off stays an unpressed chip for the
  session, and *+ Add someone* (not in prototype-6) adds a name. **Reversing:** low.
- **A ticket filed for an action item is known for the session only.** `action_item_ticket` needs
  the write id, and nothing stores which item a ticket was for; the rooms panel had the same limit.
  The item shows *queued*, then the key and the ticket's status from `get_entity`. **Reversing:**
  low in the UI; storing the pair needs a backend change.
- **Published is a link, not *Published. Publish again*.** `publish_standup_protocol` answers the
  existing publication for a date and queues no second page (`protocol.rs`), so a second press
  would do nothing. **Reversing:** low.
- **No *Infrastructure* list.** `StandupDigest` has no field for it, and this ticket adds no IPC.
  Filed as a follow-up. **Reversing:** n/a.
- **The first publish asks where in a dialog** (`PublishTargetPicker` in a `TfDialog`), as the
  rooms panel did; the footer then reads *Publishes under <source> › <page>*. Before a target is
  stored it reads *The first publish asks where.* (not in prototype-6). **Reversing:** low.
- **Digest lines read *key title: reason* for yesterday and *key title (reason)* after**, as
  prototype-6 writes them; the reason is the backend's sentence, never rebuilt from the verb. A
  page opens `#/page/<entity id>`. **Reversing:** low.
- **Merge review (#934): an edit moves only the bullets it changes.** `editSection` used to gather
  a section's bullets at its first bullet and push every other line of the section below them, so
  prose between bullets and a wrapped bullet's tail moved, and a CRLF body got bare `\n` on the
  lines it wrote. Now a kept bullet stays on its line, a changed one takes the line it replaces, a
  new one goes after the last bullet and its continuation, and written lines follow the body's
  line ending. A replaced note's wrapped tail stays in the body rather than being deleted, since the
  form never showed it. **Reversing:** low.
## #813 Lanes, list sorting and keyboard selection; #815 Signals on tickets

- **Lanes are split in the browser, over an uncapped read.** R-3.5 says "UI only", and a lane's
  columns and counts are its own, which `mini_board`'s per-status cap cannot give. So a laned
  board asks for every ticket and `splitLanes` cuts it; each lane's columns keep the 8/3 caps and
  *Show N more* locally. **Cost:** a laned board on a large mirror reads it whole (the list's
  *Show N more* already does). **Reversing:** a lane argument on `mini_board`, partitioning by
  lane and status in SQL.
- **Reading order is lane by lane, column by column, top to bottom**, the order prototype-6's
  `j`/`k` walk (its DOM order); in the list, the rows as sorted. **Reversing:** low.
- **The list opens newest first and marks Updated `descending`.** Prototype-6 marks the same
  order ▴ ("ascending") because its time is minutes ago; ours is a date, so newest first is
  descending. A header pressed for the first time sorts ascending, a second press turns it.
  **Reversing:** low.
- **Priority sorts by Jira's default ranking** (Highest, Blocker, Critical, High, Major, Medium,
  Normal, Minor, Low, Trivial, Lowest), ignoring case; a priority not in it sorts after them
  (ties in key order), and none sorts last either way. Order only, never wording, as the board's leading
  statuses. **Reversing:** a declared priority order, if a source ever declares one.
- **The *Board / List*, *Lanes* and sort switches are the page's module state**, never written to
  storage or a command, so a restart forgets them (R-3.3). Ticket views do not exist yet (#812),
  so they override the defaults; keying them by view is #812's. **Reversing:** low.
- **A card carries its labels** (`MiniBoardCard.labels`, §10.8 entry 815), because the *Label*
  lane is a ticket's first label and the card had none. **Reversing:** drop the field and the lane.
- **What each signal means** (`knobas_core::signals`): the reach is ADR-0008's walk from the
  ticket (its confirmed links, one hop further). *Build failed*: the newest build of a
  configuration in reach is in the `err` tone, so a newer green run puts it out. *Blocked by*:
  every confirmed `blocks` link ending at the ticket, whatever the blocker's status, because no
  source declares which statuses are done. *PR #N open*: a pull request the ticket links directly
  in the `info` tone, the number being its key's trailing `#N`. *Review asked of you*: one of
  those names the user as a requested reviewer. **Cost:** a build found only by commit (a pull
  request's head sha, #710) and not by a link gives no signal. **Reversing:** low, one macro.
- **Prototype-6's other signal words are left out**: *PR #139 merged*, *dashboard 8.4 s*, *new to
  you*, *blocks PAY-228*. The tickets name four facts and `has:` four values; the others have no
  read behind them. **Reversing:** add a kind to `SignalKind` and its arm.
- **`has:` reads its binds in place, not through `fd`.** Through `fd` the walk was a subplan run
  once per row; in place it is one hashed subplan a statement, over every end of a confirmed link.
  **Reversing:** low.
- **The epic lane is headed by the card's path line (`PAY-200`)**, not prototype-6's key and
  epic name: the card has no declared epic name. **Reversing:** with a declared parent name.
- **The fixture gains PR #150**, open, asking the fixture user for a review and linked to
  PAY-236, so the card says *review asked of you* as prototype-6's does; it also says *PR #150
  open*, which the prototype does not. **Reversing:** low.
- **Merge-manager (PR #933): `has:` has no injection path.** Its fragment swaps `fd.v` and `fd.u`
  for the `$N` placeholders `Builder::param` allocates, never for a value; the values are still
  binds, already narrowed by `apply_field` to the four `SignalKind` spellings, lowercased. The
  plan shows one hashed subplan a statement (`loops=1`) for the search and the ids path.
  **Reversing:** none needed.
- **Merge-manager (PR #933): the reach's builds are kept by the entity's kind before they join
  `sync.item`.** Joined on the id alone, the planner compared every reach row with every build:
  1.8 million comparisons on 1,800 tickets, 500 builds and 1,677 confirmed links. After the change
  `has:failing-build` reads 60 ms where it read 230 ms, and the laned board 133 ms where it read
  300 ms. **Reversing:** low, one macro.
- **Merge-manager (PR #933): the uncapped laned board merges as it is, with a backlog follow-up
  (#939).** On a mirror of 15,000 tickets and 12,000 confirmed links, a much denser link graph
  than testenv has, the laned board reads 1.1 s and 4.2 MB, against 68 ms capped. Lanes are a
  switch the user turns on, and testenv is far smaller; the milestone review's by-eye check
  decides whether it gets promoted. **Reversing:** #939's lane argument on `mini_board`.

## #831 — the other Settings sections in the new shell

Implementer, PR for #831. The sections are prototype-6's `STGV` pages over commands that already
exist; no command is added.

- **The theme is one store, `tf/theme.svelte.ts`, handed to pages on the shell's context.** The top
  bar's toggle and *Appearance* both write it, so they cannot disagree. Session only, as R-1.7 says;
  nothing is stored. **Reversing:** low.
- ***Restore from a file…* and *Import…* list the archives in the backup folder.** knobas has no file
  picker, and `restore_backup` takes the name of an archive in that folder. *Restore* lists the
  backups, *Import* the share exports, and both say they work only into a knobas with no data yet
  (`restore_backup`'s refusal), since a merge with a preview is not built. Prototype-6's "shows what
  the import adds" is not drawn. **Reversing:** medium, needs the dialog plugin and a path-taking
  restore.
- ***Back up* offers "Every day at <stored time>" and *Never*.** `BackupSchedule` has no weekday, so
  prototype-6's *Every week* has nothing to store. **Reversing:** low, once the schedule has a field.
- **Share ticks are the parts of `ShareParts`.** *Link sets* and *Ticket views* are no part yet, so
  they are not drawn (the ticket's criterion); *Smart lists* is drawn only when a saved list exists,
  and the part is sent off otherwise (CONTEXT.md, *Share export*). **Reversing:** low.
- **"search index up to date" is not drawn** on *Logs and database*: no command reports the index's
  state. Size and count are `db_stats`, the version `app_status`. **Reversing:** low.
- ***Send a test notification* calls the `notify` command** with the Notifications page as its
  address, and toasts the platform's refusal when the OS refuses. **Reversing:** low.
- **Standup names the target page by its mirror row** (`path › title`, both declared, ADR-0007) and
  by its entity id when the mirror lacks it. **Reversing:** low.
- ***A source cannot sign in* is the `credential_expiry` category**, prototype-6's words for the
  rooms UI's "Credentials about to expire". **Reversing:** low.

Merge-manager, PR #938 for #831.

- **The restore dialog's per-file button restores without a second confirm.** The rooms UI asks
  "Restore <file>?" first. Here the press is already two steps (*Restore from a file…*, then the
  file's own button), the backend refuses any knobas that holds data (`TargetNotEmpty`, witnessed by
  `a_restore_refuses_a_database_that_already_holds_knobas_data`), and the refusal keeps the dialog
  open with the reason (`TfSettingsSections.test`). Into an empty knobas nothing is overwritten, so
  no misclick destroys data. **Reversing:** low, a confirm row inside the dialog.
- **Standup's `PublishTargetPicker` (rooms) stays inside a `TfDialog`.** The convention mounts
  rooms-styled *dialogs* outside `.tf`; this is a component inside a tf dialog, and porting it would
  copy the publish picker's search and its story-68 rules into a second place. `app.css` strips bare
  `select` and `input` and the rooms `--link` is dark-only, so `.tf-stg-pick` now draws the fields
  as `.tf-stg-in` and sets `--link` to `--info`. **Reversing:** low, port the picker when the
  Standup page is redrawn.
- **Monitoring's bounds are the rooms section's** (days 1–3650, ms 0–600 000), not one 1–1 000 000
  for both: the backend clamps to these, and a warning threshold of 0 was settable in the rooms UI.
  **Reversing:** low.
## #817 Job reach and the other four sections, #818 Start work, the blocked banner (one PR)

- **The reach is ADR-0025's read as written: the context's members by ADR-0008's rule, unchanged,
  then the parent's own links** (ruled by the merge-manager, PR #937). The implementer's walk went
  only down from a VM, proxy or host and added every ancestor and `monitored-by` monitor of a
  reached asset, per spec #808 and R-2.8, and amended ADR-0025 to say so. That changes a decision
  the ADR made ("A job shows its context's members (ADR-0008, unchanged)"), and a ruling may not
  contradict an ADR, so the walk follows the ADR, the amendment is dropped, and the conflict
  between the spec and the ADR is a follow-up. **Reversing:** medium; the downward rule, `under`
  and `watched` are in PR #937's history (`912036a3`), and the ADR needs its amendment back.
- **The parent is `fields.parent.key` only, and not when that parent reads as an epic.** The
  classic `epic.key` arm names an epic, and an epic's links reach its tickets through link sets
  (TF5), not through R-7.3's sub-task switch. **Reversing:** drop the issue-type clause in the
  `parent` CTE.
- **The parent's own links are one layer, never walked further** (R-7.3 says "its parent's own
  links"). An item reached both ways keeps its own path (R-7.4: first origin wins).
- **`reach:` is in TF1, not TF5.** The spec's milestone map lists it under TF5, but #817's body
  asks for it and the ticket is the contract. **Reversing:** n/a.
- **The switch is two commands and no UI yet.** R-7.6 puts it in the Link sets view (TF5); the
  commands and the stored row exist now, on by default. **Reversing:** n/a.
- **Every section reads `list_entities` with a new `EntityFilter.reach`**, and the Code section
  moved from the context to it, so a parent's repository shows with its label; `promote_context`
  still runs first, because the reach starts from the context's explicit adds. **Reversing:** the
  `filterOf` helper in `TfJob.svelte`.
- **Assets are read one `get_asset` per reached asset, at most 50.** No list read takes ids, and
  the round's rule is no perf work. **Reversing:** a batched read, as its own ticket.
- **The Builds warning names every source that holds builds** (declares the `build` kind, or a
  drawn row came from it) whose credential the reader has to fix; the age is since the sign-in
  started failing, the moment the status bar dates the failure from. **Reversing:** `staleBuilds`
  in `job.ts`.
- **Start work proposes as soon as the repositories are read**, in a repository the ticket
  already reaches, else the first by name. Proposing dispatches nothing, and it is what puts real
  names in the fields; picking another repository amends the two steps that name one. Step 5 is
  the shell's *Start timer*, called once the run answered, which opens a running timer's worklog
  draft first. The step the flow is on shows *Skip*, and *Retry* when it failed or is queued; the
  prototype draws neither, R-4.9 asks for both. **Reversing:** `TfStartWork.svelte`.
- **"Mine" on the job is the ticket's assignee equal to the username its source is configured
  with**, the rule `mini_board`'s `mine` reads. **Reversing:** `isMine` in `job.ts`.
- **Sections stay in R-4.6's order on every job** (Code, Builds, Assets, Monitors, Pages);
  prototype-6 draws PAY-228's in another order. **Reversing:** the grid in `TfJob.svelte`.

## #835 Proposed links view

- **A decision is written when its toast closes, not when it is taken.** *Undo* has to put a
  proposal back in the queue, and no command can: `unlink` on an accepted proposal tombstones it,
  and `dismiss_suggestion` is remembered on purpose. So *Link them* and *Not related* are held for
  the toast's 6.5 s and only then sent; *Undo* cancels a write that never happened
  (`tf/review/decisions.svelte.ts`). A window closed inside those seconds loses the decision, and
  the proposal is simply there next time. **Reversing:** low in the UI; a real undo needs a
  `restore_suggestion` command and its §10.8 entry.
- **A relation the reader changed is drawn with `create_link`, then the proposal is dismissed.**
  `accept_suggestion` confirms the row as it stands and no command relabels a link, so the
  activity log shows *linked* and *dismissed* for that pair. Unchanged, it is `accept_suggestion`.
  **Reversing:** low; an optional relation on `accept_suggestion` would need a §10.8 entry.
- **Merge-manager, PR #943: the held write stays, with no flush on close.** Neither the spec nor
  an ADR says a decision must survive a close. Closing the last window exits the app and shuts the
  database down (`RunEvent::ExitRequested`), and Cmd-Q goes straight to `Exit`, so a write fired
  from `beforeunload` races that shutdown, and could land half a relation change. A route change
  loses nothing (the timers are module-level). The toast is the visible held state. The
  shell-conventions line now says what a close loses. **Reversing:** low; a reliable flush needs a
  close-requested hook that awaits the writes.
- **Merge-manager, PR #943: a changed relation's `create_link` treats `conflict` as drawn.** The
  pair already linked under that relation is the state the write wants, so a retry after a failed
  dismissal (link landed, proposal still waiting) dismisses instead of sticking on "already
  linked". No duplicate is possible: the active-edge index is one per pair per relation.
  **Reversing:** low.
- **Strength comes from the rule class.** `exact_key` and `source_relation` read *Strong*,
  `similarity` reads *Weak*. No rule writes evidence through another link, so nothing reads
  *Indirect* yet; its chip is drawn as prototype-6 draws it and keeps nothing. **Reversing:** low.
- **The cards show the kind and the name only.** Prototype-6's context line (author, time, branch,
  status) has no field on `SuggestionEntry`'s ends, and two `get_entity` reads per proposal for it
  were not worth it here. A ticket is named *KEY title*, a commit *sha “message”*, anything else by
  its title. The kind line is the kind's name (*Commit*), not prototype-6's *Commit in
  payout-service*. **Reversing:** low.
- **The reason reads as a sentence.** The detector writes a clause (*the commit message mentions
  PAY-231*); the card capitalises it and ends it with a full stop. **Reversing:** low.
- **A queue row names the item and points at the ticket**, whichever end the ticket is; with no
  ticket, or two, the link's own direction decides. The cards keep the link's direction
  (from, relation, to). **Reversing:** low.
- **Decisions last the session** and survive leaving the view; *Decide later* writes nothing, so
  the proposal is back after a restart. **Reversing:** low.
- **The relation list is prototype-6's** (*implements, deploys, documents, mentions, related,
  blocks, depends on*), stored as `create_link` stores ids (`depends-on`); a proposal's relation
  that the list lacks goes first. **Reversing:** low.
- **No breadcrumb,** as prototype-6 draws this view. **Reversing:** low (`crumbs: true`).

## #832 Hide notes and capture; the Markdown converter

- **The setting lives in the backend, with one command beyond the export.** The capture shortcut is
  registered in Rust at bring-up, before any window, and the criterion is that it is not
  registered while the setting is off. So *Show notes and quick capture* is a `knobas.setting` row
  (`capture.notes_shown`) and `set_notes_shown` is a second new command, though the milestone
  digest names only the converter's. A frontend-only setting could not keep bring-up from
  registering the key. **Reversing:** medium, the command and its §10.8 entry.
- **The note count rides on `capture_shortcut`'s answer.** `list_entities` reads `sync.live_item`,
  which holds no notes, so it answers 0 for the note kind. `ShortcutView` gains `note_count` and
  `last_note_at`, so the section has one read and no third command. **Reversing:** low.
- **A note exports as two blocks: its title as a heading, and its body as Markdown passed through.**
  The body is already Markdown, so re-parsing it into blocks would only risk changing it. Passing it
  through keeps every `[[ref]]` exactly as typed. **Reversing:** low.
- **The converter is `knobas_core::markdown`** (`DocBlock`, `Inline`), a core module and not a
  frozen surface, so TF4's Confluence mapping (#861) can target it and add blocks. **Reversing:** low.
- **Export goes to `~/Downloads/knobas-notes`**, as prototype-6's toast says. A same-named file from
  an earlier export is replaced. A second note with the same title gets `-2`, `-3`. **Reversing:** low.
- ***Move to Confluence…* shows a toast** saying the move comes with the Docs view, until #941
  builds the page picker in TF4. **Reversing:** n/a, #941.
- **Prototype-6's help text keeps "notes are back in search"**, and #940 (TF2) makes that true by
  taking notes out of search while hidden. This ticket built the shortcut half only. **Reversing:** n/a.
- **An unset shortcut reads "Not set"**, because prototype-6 always draws one. A refused one reads
  "Not registered: <reason>" under the row, as the rooms section does. **Reversing:** low.
- **The rooms UI's capture section says a stored shortcut is not registered while notes are
  hidden.** Without that it would read "Registered." for a key nobody holds. **Reversing:** low.
- **Merge review of #944: `page_file_name` caps its stem at 60 characters and suffixes a Windows
  device name with `-page`.** Its slug already kept only letters, digits and `-`, so no title could
  leave the export folder. A long title would have made a name over 255 bytes and failed the whole
  export, and `con.md` cannot be written on Windows. The rule lives in `page_file_name`, so a page
  link and the file it points at still agree. **Reversing:** low.

## #812 Ticket views: table, list and save

Implementer, PR for #812.

- ***Save as a view* and *Duplicate* open a small *New view* dialog** (name, group, the text it will
  keep, "Kept in knobas. Jira never sees it.", *Create view* off until there is a name). Prototype-6
  opens its editor there, and the editor is #814's. How the tickets show comes from where the view
  started: *Save as a view* takes the layout, lanes and sort on screen; *Duplicate* the view's own.
  **Reversing:** #814 swaps the dialog for the editor; nothing stored changes.
- ***+ New view* and *Edit view* are not drawn**, and the ⋯ menus hold *Duplicate* and *Delete*
  only: both open the editor (#814). **Reversing:** #814 adds them.
- ***All tickets* stays at the top of the views pane, at `#/tickets`.** Prototype-6 has no such row
  and opens its first view; #810 built *All tickets*, a fresh knobas has no view, and no migration
  seeds any (nothing invented). **Reversing:** low, drop the row once views are seeded somewhere.
- **A view's count is drawn only above zero** (the shell's count-badge convention); prototype-6 draws
  every count. A view the grammar cannot read has no count. **Reversing:** low.
- **A view the grammar cannot read draws its name, its text, and one line naming the part**
  ("The search cannot read `sprint:next` in this view, so it shows no tickets."), and reads no
  tickets: no filter bar, no board, no preview. Running the readable rest would show more tickets
  than the view means. **Reversing:** low.
- **The id is a slug of the name made at save** (`-2`, `-3` past a taken one); renaming keeps it, so
  an address stays put. Two views may share a name. **Reversing:** opaque ids, one column.
- **The display words are checked in Rust, not in the migration** (only `layout` is a SQL check), so
  a lane or card field a later ticket adds is a constant, not a migration. **Reversing:** low.
- **The session's switches are kept per view** (`#813`'s module state, now keyed by view id), over
  what the view saved; nothing is written. **Reversing:** low.
- **A view's own columns gather `mini_board`'s status columns** in the browser: a named column holds
  its statuses' cards newest first, its count is their totals, "N of M" in err tone over its limit,
  and a status on no column does not show (R-3.15). **Reversing:** a columns argument on
  `mini_board`.
- **Cards and the list follow a view's card fields and list columns; *All tickets* keeps the card
  #810 drew** (epic included). *Comment count* draws nothing until #894 declares one; labels are
  drawn only when a view asks for them. **Reversing:** low.
- **The ticket views share part is on by default, and its tick is drawn only once a view exists**
  (both share dialogs), the smart lists' rule. **Reversing:** low.
- **The fake IPC seeds prototype-6's own views** as search text (`fake/ticket-views.ts`); its counts
  restate the matcher, which passes over `assigned:`, so *Assigned to me today* reads wider than the
  real one. **Reversing:** low.

## #848 — Builds tracer: declared pipeline and run

Implementer, PR for #848.

- **The tracer declares the fields its records already carry, not all of R-9.12.** Declared:
  `frame`, a pipeline's `name`, `group` and `pipeline` identity, and a run's `pipeline`, `number`,
  `branch`, `started` and `finished` (the run's state and commit were already declared, #666 and
  #710). Not declared: a pipeline's repository, default branch, parameter specs and triggers, and a
  run's started-by, agent, steps and test summary. The first four live in the configuration's
  settings, which carry credentials (a VCS root's `secure:password`, a `password` parameter) that
  the source view reads live and scrubs and that nothing mirrors today; mirroring them is its own
  decision. Started-by is `triggered.user.username`, which 100 of 100 builds on JetBrains' public
  instance lack (#106), so the battery's clause 3 would fail there. Agent and the test summary need
  `BUILD_FIELDS` widened and mockd taught to serve them, and mockd's dataset has neither, so it
  would have to invent them. Steps are on no TeamCity build record; they come with the whole log's
  step boundaries (#854). None of them is drawn by this ticket, so a declaration now would be a
  frozen field with no reader and no witness on screen. Each one arrives with the ticket that first
  draws it (#851 repository, triggers and started-by; #852 parameter specs; #853 agent and steps;
  #855 the test summary), each with its own §10.8 entry. **Reversing:** low; each field is additive.
- **A run joins its pipeline on a declared `pipeline` identity on both kinds**, not on the
  existing `parent` names. TeamCity configuration names are not unique; matching a run's parent
  names against a configuration's own would mis-join two configurations of one name in two
  projects. **Reversing:** medium (a frozen field).
- **`frame` is a role on `KindPaths`, not a new descriptor field.** The reads already bind the
  declarations per `(source, kind)`, so the frame kinds are found from the same parameter, and a
  Docs frame (TF4) can add its roles to the same enum. **Reversing:** medium.
- **Times are ISO 8601 in either spelling**, read in Rust (`parse_instant`), not normalised in the
  adapter. TeamCity writes the basic form; the payload stays verbatim (spec §3a) and needs no
  cursor bump. **Reversing:** low.
- ***Running* is "a run started and not finished"; a strip block is the run's declared tone once
  finished, else running or queued.** Both from the declared instants, so no state word is read.
  **Reversing:** low.
- **`#/builds/<entity id>` addresses a pipeline**, as every full-view address carries an entity id
  (conventions, "Routes and addresses"). A bare `#/builds` selects the first failing pipeline, else
  the first; *This job*'s choice is #850's. `#/build/...` draws "The run page is not built in the
  new shell yet." until #853. The full route gains `head`, its first segment, so the page tells
  the two apart. **Reversing:** low.
- **The run list draws the source's state word** (`SUCCESS`, `FAILURE`) where prototype-6 says
  *passed* and *failed*: the chip is the declared one (#666), and renaming it in the shell would be
  a mapping no source declared. **Reversing:** low, a `shown_as` on TeamCity's state values.

Merge-manager, PR #946 for #848.

- **The tracer meets #848's acceptance criteria with the fields it declares.** The criteria ask for
  a §10.8 entry per declared field and live coverage of the declared fields, and both hold. The
  field list in the ticket's *What to build* is wider than the criteria; the eight fields left out
  are named one by one in #947, which each drawing ticket closes or rules out. A run's duration is
  `started` and `finished` read together, and its tone map is the existing `state` (#666).
  **Reversing:** low; reopen #848 or move #947's fields into it.
- **The join is witnessed across projects and sources.** A run is placed by `(source_id,
  pipeline)`; TeamCity configuration ids are unique within one server, so the case a reused id can
  hit is a second server. `a_run_joins_its_pipeline_by_id_within_its_source_only` holds both
  statements' source predicates (each mutant drops one and fails it). **Reversing:** low.
## #883 File view draws the file's blob sha

- **Gitea reads the file's `contents` entry on every file read, beside the `raw` read
  (`tokio::try_join!`).** Before, it read the entry only for a cut answer without a length. The
  blob sha is in no other answer the file read already makes, so the entry is always read now.
  It runs in parallel, so the wait is the slower of the two reads, not their sum, and the
  size fallback reuses the same entry. The cost is Gitea's base64 `content` in that answer, for
  files under its API blob limit: up to about 1.33× the file's bytes, once per file opened.
  The seeded 1.27.3 also answers `contents-ext/{path}?ref=` with the same `sha` and
  `content: null` (checked by hand on 2026-10-16), which would avoid that cost. But it is a newer
  route than `contents`, and an older server would fail every file read on it, while the issue
  names the contents API. A follow-up can switch if the cost shows.
  **Reversing:** low, one call in `view/files.rs`.
- **A failed entry read fails the section**, as a failed `raw` read already did, rather than
  drawing the file with `sha: None`. A file drawn without its blob would let *Edit* (#845) send no
  base. **Reversing:** low.
- **The field is `sha`, as Gitea and `file_on` in the live suite name it**, documented as the blob
  sha at the ref drawn. **Reversing:** medium, a §10.8 field rename.

Merge-manager, PR #948 for #883.

- **The file view reads `contents-ext`, and `contents` only when that route answers 404.** This
  overrules the implementer's first ruling above. `contents` answers the file as base64 up to
  Gitea's 10 MiB API blob limit and walks the history for the file's last commit. On a throwaway
  1.27.3, a 10.1 MB file came back as 13.5 MB of JSON from `contents`, beside a raw read capped at
  1 MB, so one file opened fetched it about 14 times over the cap. `contents-ext` answered the
  same `sha` and `size` in 784 bytes. A 34 MB file, over the blob limit, cost 931 bytes either
  way. A Gitea from before 1.24 has no `contents-ext` and answers 404, so the read falls back to
  `contents` and costs what the implementer measured. The issue's "contents API" covers either
  route. **Reversing:** low, one call in `view/files.rs`.
- **The entry is read before the bytes, not beside them.** If a push lands between the two
  reads, this order draws an older blob beside newer text, and an edit made against it is held as
  moved (#822). Reading both at once, or the bytes first, could draw a newer blob beside older
  text, and an edit of that text would land over the push unseen. The cost is one small round
  trip of added wait per file opened. A mock cannot order two requests, so nothing witnesses the
  order. **Reversing:** low.
## #859 Docs tracer: the reader over declared docs

Ruled by the implementer (Björn, 2026-10-15: implementation-time forks are the implementer's).

- **The body, comments, version, labels and editor's name are an adapter *projection* of the
  mirrored record** (`knobas_source::docs::DocProjection`, a plain function named in the registry
  table), not declared paths and not a live `Source` read. Only the adapter can read Confluence's
  storage format (ADR-0007); a function of the mirror opens pages offline, needs no credential, and
  re-projects every page without a re-sync when the mapping improves. Space and tree stay the
  existing declared `project_*` and `parent` paths. **Reversing:** a `Source` method with a
  default `None`, and the registry column goes.
- **A page's direct parent is the innermost name of its declared chain, matched by title inside
  its space** (a Confluence title is unique in a space). A parent the mirror lacks puts the page
  at the top. **Reversing:** `docs::tree`.
- **A mention resolves by `ri:username`, or by a `ri:userkey` the record itself names** (the
  page's editor, creator or a comment's author); any other key is a mention of nobody, drawn
  `@someone`. The miss direction: never somebody else's mention claimed as mine. Resolving every
  key needs a user lookup at sync, filed as a backlog follow-up. **Reversing:** `blocks::people`.
- **Every macro outside the subset, `ac:image` and `ac:task-list` are placeholders named by
  `ac:name`**; a block macro inside a sentence ends the paragraph there so it is never dropped.
  The generic `panel` macro is a note; `tip` is its own tone, drawn in the ok colours (prototype-6
  has no tip). **Reversing:** `blocks::Blocks::r#macro`.
- **The tree is ordered by title**, so a space's first page is its first top page by title.
  Prototype-6 keeps the author's order; Confluence's own child position is not read yet (backlog
  follow-up). **Reversing:** `PAGES`' `order by` and `firstPage`.
- **"Read last" is the newest `visit_entity` line on a live page**, and the reader records a visit
  on every page it opens. **Reversing:** `LAST_READ`.
- **The source's words are its configured display name**: *Open in Tidewater Confluence*, and
  the placeholder names it the same way, because R-10.18 lets only those two be source-specific
  and a Markdown wiki must not read "Confluence". **Reversing:** `source_name`.
- **A ticket chip's tone follows prototype-6 on the status name** (done ok, to do plain, the rest
  info); a key the mirror does not hold is a bare key, not a link. **Reversing:** `statusTone`.
- **Section anchors use prototype-6's numbering** (`#/page/<id>/<n>`, the first heading is 1) so
  #861's outline can link the same addresses. **Reversing:** `sectionNumbers`.
- **Left to #860–#863, not drawn:** the search field and its hint line, *For you*, the right
  rail, *Comment*, *Link to*, *New page here*, *Export as Markdown*, *Copy link*, *Edit section*,
  the composer, *New page* and the *This job* scope. Labels are chips, not buttons, until search
  exists to send them to.
- **Words prototype-6 lacks:** "No pages are synced yet. A source with pages, such as Confluence,
  fills this tree.", "Nothing to read yet. Pages appear here once a docs source has synced.",
  "knobas cannot read this page's body. Open it in <source>." and "No comments yet." (the last is
  prototype-6's).
