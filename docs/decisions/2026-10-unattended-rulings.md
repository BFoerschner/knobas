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
||||||| parent of 7eedec6d (Record 0028: the §10.8 entry (gated on #627's sign-off), ADR-0019 proposed, and the ruling (#627))

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
