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

