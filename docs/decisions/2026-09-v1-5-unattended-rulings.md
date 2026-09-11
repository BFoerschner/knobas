# Unattended v1.5 rulings — from 2026-09-07, Fable as deputy

Björn, 2026-09-07: *"let a fable subagent decide whenever there's a decision to be made since I won't be there."* The `/v15-next` loop dispatches one Fable subagent — the **deputy** — per fork, in the shape of the 2026-08-29 batch (`2026-08-29-unattended-batch-rulings.md`): each ruling is posted as a comment on its issue, beginning `Ruling (deputy for Björn, <date>):`, and the PR that acts on it appends the same four parts here, in the order ruled. **The PR that acts on it** is the first PR the ruling's chain produces, or a docs-only PR when there is none: a ruling whose ticket closes on transcripts rather than on a merge has no commit of its own to carry a section, and the section rides in the next one downstream (ruled on #552, 2026-09-11; the precedent is #525's, which rode in #549). The deputy first looks for an answer that already exists — the ticket, spec #491, `CONTEXT.md`, the ADRs, the decision records, the grilling's rulings — and quotes it; only an open fork gets a new ruling.

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

---

## #495 — TeamCity build web URL: the stale record, and the dropped selector name

Ruled 2026-09-07. Comment:
<https://github.com/BFoerschner/knobas/issues/495#issuecomment-5575914408>

**The fork:** PR #514 raised two questions and settled neither. (1) `docs/contract.md`'s 2026-09-02 amendment (issue #266, line 1372's section) says *"the adapter passes the field through untouched (P5) ... nothing in knobas depends on either"*, and the first half stops being true once `map::build_web_url` composes the URL. Does #495's PR amend the record, and in which idiom, or is the sentence left as the historical statement it was with the correction owed to a follow-up? (2) With the last reader gone, `BUILD_FIELDS` stops asking for `webUrl` at both levels and `struct Build` stops parsing it, so a build's mirrored `payload` no longer carries the server's own URL. Is that right, or should the field stay under `#[allow(dead_code)]` so the payload keeps the server's spelling?

**Ruling:** (1) **The sentence stands and the new truth is recorded beside it, in this PR.** Both halves are already decided by the contract's own convention, ruled by Björn on 2026-08-31 in the #112 section: *"a §4.2 row that was simply wrong — describing something that was never, or is no longer, true — is corrected in place; an amendment entry records that it happened and why. A row that was superseded by a later decision keeps the existing treatment: the old text stands and the amendment carries the new truth."* The 2026-09-02 sentence was true when it was measured and #495 is the later decision, so it is the second case, and the precedent to copy is the #345 section (2026-09-04). Concretely: a new dated section in **§9** — `### Amendments from the TeamCity build URL composition (2026-09-07, binding) — issue #495`, after the #453 section and before `## 10.` — naming this ruling, saying no §10.8 entry is owed, and carrying the composition rule, the selector drop in the #33 section's `percentageComplete` idiom, the measured 200-for-any-path fact, the resolver consequence #496 inherits, and a closing *Pinned by:* line that names the tests and states that the `TeamCitySource::sync` → `sync::execute` seam is guarded only from `tests/mockd.rs` outward and by the seeded live suite; plus one italic pointer in the 2026-09-02 bullet in the #339 idiom, leaving that sentence as history. No follow-up ticket, no §10.8 entry, no glossary change and no ADR. (2) **`webUrl` stays out of `BUILD_FIELDS` and out of `struct Build`; no `#[allow(dead_code)]`.**

**Reasoning:** neither half touches a frozen surface — `crates/knobas-source-teamcity/**` is not on §10.8's list, §4.2 pins the endpoint and the requirement of an explicit `fields=` rather than the selector's contents, and `SyncItem.web_url` (P5) is used exactly as granted. An `#[allow(dead_code)]` would silence the one device the crate has for finding a name nobody reads, the class the working model calls *"a check that measures a representation of the thing instead of the thing"*; the #33 section records the exact precedent for dropping such a name. The payload argument does not hold: `CONTEXT.md`'s **Payload** is *"the raw source record an item carries verbatim"*, and for TeamCity the record is what `fields=` returned, since §4.2 requires *"always an explicit `fields=`"* — a payload has never promised more than the selector asked for, and nothing in the tree reads `payload->>'webUrl'`. The resolver cannot want it either: spec #491 matches *"exact on the mirror's stored `web_url` after normalisation on both sides"* and puts per-adapter URL parsers out of scope, so keeping the server's spelling would only invite the parser the spec excluded. On the record: the implementer's premise that a new dated amendment could only go inside §10.8 is not so — §9 is where they go, and three were appended there on 2026-09-07 (#442, #452, #453) after §10 existed. Spec #491's out-of-scope line forbids *rewriting* records, which the superseded treatment respects by leaving the sentence in place; appending is the file's own mechanism. Leaving the false half-sentence for a follow-up would teach the next reader of that section that the adapter passes `webUrl` through, from a record one PR out of date.

**If you disagree, the cost of reversing this is:** trivial for the record, and by amendment rather than deletion — an undo is a further #112-style entry, since removing the #495 section would itself rewrite a record. Small for the selector: `webUrl` returns to `BUILD_FIELDS`, to the struct field and to the selector test's tuple, three lines with no migration and no frozen surface, and builds mirrored in between gain the payload key on their next re-emit (a backfill, or an incremental run for in-flight and newer builds). The one thing a reversal would not get back is a use for the field, and none exists today.

---

## #496 — the glossary's Mirror sentence, the frozen-surface gate, and criterion 2's `--demo` half

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/496#issuecomment-5576157118>

**The fork:** PR #517 raised three. (1) `CONTEXT.md`'s **Mirror** says *"Readers only ever see its live items"*, and `RESOLVE_URL` reads `sync.item` so a paste resolves a tombstone — which spec #491 story 15 asks for — and a disabled source's items; `get_entity`'s `DETAIL` had broken the same sentence since #204. Is the entry corrected, in what words, by whom, and is the launcher's *"local index"* footnote in or out of the same fix? (2) The PR adds migration `0023` and the IPC command `resolve_url`, both on §10.8's frozen list, whose gate is Björn's. Does the ticket authorise the touch, and is the §10.8 entry as written the entry the contract requires? (3) Criterion 2 ends *"headless Chrome against `--demo` shows both outcomes"*, and `--demo` opens a Tauri window Chrome cannot attach to — the limitation `AssetsView.test.svelte.ts` recorded for #428 and that M4 closed without ruling. Is the `?fake-ipc` walk a sufficient substitute, or is something owed?

**Ruling:** (1) **The glossary is corrected in #496, by the implementer, on PR #517** — three entries, **Mirror**, **Tombstone** and **Live item** — and the *local index* wording is **out**, owed to one follow-up ticket the orchestrator files. The work is assigned by the glossary's own sentence: **Live item** (amended 2026-09-07, #448) says *"It is the only such reader; a second one needs a reason of its own and an entry here"*, #496 adds such a reader, and story 15 is its reason. The reason for deferring is gone: #493 has merged, so `CONTEXT.md` is not contended. **Mirror** stops saying *only ever* and points at **Live item**; **Tombstone**'s *"leave every reader's view"* gains *"but the ones **Live item** names"*; **Live item** becomes a census of the three readers that reach past the view, each with its reason — the Monitors roster (#448, tombstone half only), `get_entity`'s `DETAIL` (#204, migration `0012`, both halves), `resolve_url` (#496, both halves) — closing *"Those three, and a fourth needs a reason of its own and a line here."* The sentences are the implementer's; the census and the three reasons are the ruling. No ADR: the rule is already in the glossary, only its census was wrong. The *local index* class — `Launcher.svelte`'s footnote and *Reading the local index…*, `Results.svelte`, `Detail.svelte`, the two `not_found` messages in `commands/entity.rs` and what echoes them, two SPI doc comments — is a scrub across five modules and a frozen crate's doc comments, the size and shape of #493, and not what #496 builds; the PR's new panel already avoids the word and pins it, which is the right boundary. (2) **Authorised, and the entry is sufficient as written.** The ticket names both surfaces in its own words (*"backed by a new expression index"*, *"The migration adds the expression index and nothing else; the resolve read's §10.8 entry names the normalisation rule"*) and spec #491's stream map row 2 names them under *Frozen-surface touches (§10.8 entry each)*. One line is owed on the entry: the sentence flagging it for Björn stays and gains that it was ratified by this ruling, the way #409's entry records who ratified it. (3) **The substitute is sufficient, and the ruling covers the same sentence in #497, #499, #502, #504, #505, #506 and #509**, under four conditions: the walk runs against the **vite dev server** with `?fake-ipc` and never `vite preview`, on the agent's own port and `--user-data-dir`; it certifies **layout and interaction, never the bridge**, and any fixture handler added for it says in place that it is fixture-only; the criterion's real witness stays the **IPC-seam suite over a scratch database**, so a ticket with no such suite behind the sentence comes back as its own fork; and the transcript goes in the PR body for the merge-manager to re-run. No desktop-automation run is owed for a rendered panel.

**Reasoning:** (1) a glossary that says *only ever* and *the only such reader* while two readers already break it is the class the working model names — a check that measures a representation of the thing, the representation here being a sentence. Leaving it would have the next implementer (#497 uses this resolver) read **Mirror**, find `sync.item` in the code it depends on, and stop at the same fork; amending **Mirror** alone would leave the entry that *claims* to be the census still wrong. The glossary line was owed by this ticket from the moment it added the reader, so the scope of #496 is unchanged and no frozen surface is touched. (2) the freeze exists so a migration or a wire shape never arrives unannounced; both were announced in the spec, named in the ticket, and written up in the section the freeze points at, and the smallest frozen surface that meets stories 9–11 and 15–17 is exactly one index and one read. A migration is the only way an index reaches a sqlx-checksummed schema, so *"the migration adds the expression index"* is the authorisation for `0023`, not a loophole in it. Checked at the gate: `0023` is the next free number and is one `create index` and nothing else; the handler list is appended at its foot and `ipc/index.ts` already re-exports `entity.ts`; nothing in `crates/knobas-source/**`, `crates/knobas-http/**`, `error.rs`, `profile.rs` or `ipc/index.ts` moves; the entry sits inside §10.8 by an anchored header grep. Spec #491's *Contract* paragraph counts *"the two migrations"* and calls stream 2's touch *"one index"* — the ticket is the operative text and the spec's count is shorthand, not a prohibition. (3) ADR-0013 is about adapters and write paths against their real systems and names `knobas-source-mock` as *"a different thing"* that stays; `--demo` runs that fake and `?fake-ipc` runs a fixture of the same corpus in a browser, so neither is a real Jira and the criterion never asked for one. What the criterion asks for — the two panels, the navigation on a hit, the raw URL on the button on a miss — is what the walk shows, and it pasted the hit with a fragment so it is not the happy path alone. The v1.5 grilling ruled a desktop witness for OS-level features only, *"because there is no instance to run a suite against"*; stretching it to rendered panels would block seven tickets behind #500's one-at-a-time harness for a question the IPC seam already answers. The reading that keeps the witness real puts the resolver's witness where the real database is and the panel's where the panel is, and refuses to let a fixture's `resolve_url` be mistaken for the first.

**If you disagree, the cost of reversing this is:** (1) trivial — three sentences in `CONTEXT.md` and no code; the follow-up ticket closes as `wontfix` before anyone starts it. (2) high after merge, as for any migration: `0023` can never be edited, undoing it is `0024` dropping the index, and the command leaves the barrels only by a further §10.8 entry. Before merge it is one revert of a branch nothing else has built on, except that #497 is blocked on this read and would wait. (3) nothing in code — no criterion text is edited and no test is deleted. Reversing means naming a witness that can attach to a Tauri webview, which today is only #500's harness, and that re-opens the seven tickets' witness sentences and serialises them behind the desktop cap; ruling the other way later costs those tickets a re-run, not a rewrite.

**Flagged, not ruled:** whether the delegation to the deputy outlives v1.5; and whether spec #491's *Contract* paragraph should be corrected to count stream 2's index as a migration — a comment on #491 pointing at this ruling is enough.

---

## #498 — the direction the live suite cannot witness, the frozen-surface gate, and the `SourceError::no_workflow` helper

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/498#issuecomment-5576475226>

**The fork:** PR #520 raised three. (1) The implementer, in the PR body under its own heading: *"This seeded Jira's entire status list is exactly the four its workflow reaches, and each transition's `name` equals its `to.name` — so against the real server the read's answer is indistinguishable from 'every status the project has'. Narrowing is witnessed only by mockd, which ADR-0013 says certifies nothing."* The loop's third kind of witness gap — a witness that can only see one direction. Does #520 merge with the gap recorded, and where; does the milestone get a ticket for a second seeded workflow, and in v1.5 or after; or is something owed inside #498 first? (2) The PR touches `crates/knobas-source/src/lib.rs` (one trait method, no default, 33 implementations answered) and `crates/knobas-source/src/contract.rs` (a new battery clause), and adds `commands::entity::reachable_transitions` to the handler list — all on §10.8's list. Does the ticket authorise the touches, and are the §9 section and the §10.8 entry what the contract requires? (3) The implementer declined a `SourceError::no_workflow(..)` helper for the five hand-spelled refusals and the fakes' repeated stub, as *"a frozen surface this ticket does not name — a fork, not a tidy-up"*. Does the ticket's authorisation of a `Source` trait read extend to a new error constructor in the same crate?

**Ruling:** (1) **#520 merges as it stands, and nothing more is owed inside #498.** The gap is already in the three places the record lives — §9's binding section, the live test's own doc comment and its `assert_eq!(on_the_instance, expected)` that goes red the day the instance grows a fifth status, and the PR body — and the ruling comment is the ticket-side record. **The milestone gets one follow-up ticket, filed by the orchestrator, in v1.5**, `ready-for-agent`, blocked by #498, counted as a tunnel ticket: *a second seeded Jira project whose workflow narrows, and the live witness for the reachable-transition read's narrowing*. It owns #498's equality assertion and replaces it with the narrowing assertion; it also owns the write-side refusal of a status that **exists in the project and is not reachable from where the ticket stands**, which `"Blocked"` — a status that exists nowhere on this instance — cannot witness. Additive to `PAY` and its restore point, seed idempotent, no frozen surface. (2) **Authorised, and both documents are sufficient as written; the merge-manager may proceed.** One clause is owed and is in this PR: the §10.8 entry's *"flagged for his review"* and the §9 section's *"both are flagged for his review"* each gain *"— ratified in his absence by the deputy's ruling of 2026-09-08 on #498"*, the wording #496's entry already carries, so neither record leaves a reader guessing whether the flag was answered. (3) **Confirmed — declined correctly. No helper in #498, and no follow-up ticket is owed for one.**

**Reasoning:** (1) the criterion is not unsatisfiable, which is why the PR merges: #498's second criterion and spec #491's witness sentence are both met and measured on the real Jira — the endpoint, the `to.name` field, the 401 class and the unchanged refusal are all witnessed live. What the fixture cannot show is that the answer is a *subset*, and the ticket never asked for a fixture that could; the implementer did what a fixture-shaped loss requires, wrote it where the next reader will look, and made the test check the premise rather than assume it. It is not enough for the milestone because of ADR-0013's own sentence — *"'awkward to reproduce' is not 'cannot produce'"* — and narrowing is not a fault a real Jira will never produce, it is what a real Jira workflow does all day. The spec's stated pain **is** the narrowing, so the one direction the live suite cannot see is the one the feature was cut for. Not inside #498 because the seed is shared real infrastructure with a restore point two suites work from, seeding is different work with its own transcript, and #498's scope names the trait, the battery, the IPC read and the select and says nothing about the seed. (2) the freeze exists so that a trait growth or a wire shape never arrives unannounced; this one was announced in the spec's stream map (*"one `Source` trait method; one IPC read"*), named in the ticket's own words and its first criterion, argued in §9 and listed in §10.8. The battery clause is the strictest of the three — #146 had reserved a battery clause change to Björn — and is authorised because the ticket's first criterion asks for it by name. The smallest frozen surface that meets stories 18–24 is exactly one method, one clause and one command, and that is what landed: no DTO, no event, no migration, one barrel line. The one larger alternative, a default implementation, is struck by the spec's out-of-scope list and by the grilling, so the 33 implementations are the cost the grilling chose and not one the implementer added. (3) the authorisation is what the ticket and the spec name, and a constructor is not on the list: spec #491's *Contract* paragraph ends *"nothing else on the frozen list is touched"*, `SourceError` is one of the DTOs §10.8 freezes, and the §10.8 entry this ticket asked for records *"`SourceError` … untouched"* — so the helper would have falsified the entry in the same PR that wrote it. Additive is not exempt: #452's entry lists four additive additions to the same crate for that reason. The merits agree: the write refusals in the same adapters are hand-spelled, so a helper for one and not the other is a half-convention; what holds a message to naming its adapter is battery clause 8, not a constructor; and a shared stub for the fakes is a default by the back door, which the out-of-scope list rules out.

**If you disagree, the cost of reversing this is:** (1) trivial for the ticket — close it `wontfix` before anyone starts it; the code merges either way and the select behaves as #498 asked. Reversing the merge is one revert of a branch nothing has built on, after which the select falls back to the corpus offer as before. The one ratchet: ruling that mockd's shaped workflow is a sufficient witness *here* would be a new exception to ADR-0013, whose struck exception ends *"this sentence is not precedent"* — Björn's sentence to write, not the deputy's. (2) high after merge, as for any trait growth: the method leaves the SPI only by a §10.8 entry of its own and a change to every implementation, the clause likewise, and the command leaves the barrel only by an entry. Before merge it is one revert of a branch nothing has built on. Nothing is irreversible in the database — there is no migration, and that is the entry's own point. (3) trivial — one constructor added later under its own §10.8 entry and five call sites changed; nothing in #520 forecloses it.

**For the orchestrator:** file the part-1 ticket (v1.5, `ready-for-agent`, blocked by #498, a tunnel ticket) and record this ruling's three parts here through PR #520's implementer, with the one clause of part 2 added to the §10.8 entry and the §9 sentence. The merge-manager may proceed on #520 once those two edits are on the branch.

---

## #500 — the desktop witness meets a locked screen

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/500#issuecomment-5576924248>

**The fork:** PR #523 built the desktop-witness harness and could not run it.
ADR-0016 says the witness is desktop automation with *"No human step — Björn
ruled the same day that nobody will be there once the implementation runs"*,
but macOS delivers no synthetic keystroke to an application behind the lock
screen and only a person can unlock a Mac — so an unattended desktop witness
has a human *precondition* even though it has no human *step*, and criterion
2's green ⌘K transcript could not be produced. Both blockers were measured: the
dev Mac locked since 2026-09-07 22:47 CEST, the harness refusing
`screen-locked` with exit 1; and `/Applications/knobas.app` (v0.1.0) keeping
the `dev.knobas.desktop` registration through an `lsregister -f` on the freshly
built bundle. Four options: **A** merge with criterion 2 open, file a follow-up
for the run, release the desktop slot; **B** hold the PR and the slot until the
Mac is unlocked; **C** A plus an ADR-0016 amendment, so *no human step* is not
later read as *no human prerequisite*; **D** something else discharges ⌘K. The
implementer leaned C.

**Ruling:** **C.** PR #523 merges with criterion 2 open and disclosed, once its
merge-manager is satisfied on everything else; the merge-manager re-runs `just
desktop-witness launcher-hotkey` as ADR-0016 asks and the transcript it must
get is the same `screen-locked` refusal, exit 1 — the expected result on this
Mac, not a regression. Every other criterion is held in full: `just check`
green with `witness-unit` in it, the README's *What is not witnessed yet*
present and true, criterion 2 left unchecked. `Closes #500` stays. **ADR-0016
gains one dated consequence**, written by the implementer in the shape
ADR-0013 uses for its 2026-09-07 addition: *"No human step" is not "no human
precondition"* — the three preconditions a person meets once and the harness
probes on every run (the Accessibility grant, an unlocked logged-in session, no
other copy of `dev.knobas.desktop` registered), each a refusal that names the
thing by its System Settings name and exits non-zero; that the criterion stays
open until an unlocked run, with no fake, dry-run mode or hand checklist
standing in (ADR-0013); that the grant is **Accessibility, not Automation**,
because an Apple Event prompt nobody answers is recorded by TCC as a denial
(`auth_reason 9`, measured twice); and that the harness never moves an
installed app aside. **The desktop slot is released when #523 merges**: #501
and then #503 proceed in number order under the one-desktop-ticket cap, write
their drivers against this harness, and merge with the same one criterion open,
the `screen-locked` refusal as their run, and every other criterion held —
driver logic goes in a lib file `witness-unit` covers, on the split #523 drew,
because a driver is not exempt from the gate because its run is. **One
follow-up ticket** — #525, `ready-for-human`, milestone v1.5 — carries the
unlocked run for all three drivers, and **the v1.5 exit is not taken while it
is open**. **D is refused**: there is no route around the lock, and no
substitute discharges ⌘K.

**Reasoning:** the answer to the fork's premise was already in the spec. #491's
*Desktop witness, no human* reads *"The one-time prerequisite — the automation
permission for the terminal that runs the driver — is recorded in the testenv
README beside the signing identity, and a refused permission is a named
failure, not a silent pass"*: Björn's ruling already lives beside a human
prerequisite and already distinguishes it from a step in the run. An unlocked
session and a clear registration are prerequisites of that class, so *no human
step* never meant *no human prerequisite* — but it is the reading the ADR's
compressed sentence invites, which is why the ADR and not only the README is
corrected. A over B because of what Björn's ruling was for: *"I will not be
there once the implementation runs"*, and the `ready-for-human` checklist *"is
superseded"* — B produces exactly the stall the ruling exists to prevent, at
the price of three tickets, for a condition no agent can change; the 2026-08-29
ruling on #92 is the precedent for merging with a live criterion owed. C over A
because #501's and #503's implementers will open ADR-0016, read *No human
step*, and stand at this same fork with #523's PR body nowhere in front of
them. The witness stays real, which is why the run is deferred and not
replaced: ADR-0013's 2026-09-07 consequence draws the line at *"awkward to
reproduce is not cannot produce"*, and the desktop can be driven — just not
now. That is a deferred live witness, not the fake's class, so the exit holds
on #525; closing v1.5 with its three OS-level features never once driven would
contradict the grilling's witness ruling as surely as a hand checklist would.
The Accessibility-over-Automation mechanism is confirmed not because a sketch
may be discarded but because the reason is measured and the ADR's binding word
is the generic one. The frozen surface is untouched — `justfile` and
`testenv/**` only — and nothing here adds to it.

**If you disagree, the cost of reversing this is:** low before #525 runs, and
lower the sooner it runs. Reversing to B unmerges nothing — the follow-up
already holds the debt and the exit already waits on it; what B would add is
only that #501 and #503 had not been written yet, which cannot be recovered.
Reversing the ADR sentence is one dated strike-through in the style ADR-0013
carries. If the first unlocked run is red — the WKWebView question the README
names — the three drivers are re-done against whatever the `ax dump` shows,
the same cost under A, B or C, paid once. The one thing this spends that
cannot be returned is a milestone in which three features merged before their
OS-level witness ran, on the disclosure in three PR bodies and one README
section; #525 is where to say that was the wrong trade, and it is the last item
between v1.5 and its exit.

**Flagged, not ruled:** whether `ready-for-human` should carry a standing
meaning of *a precondition only a person meets, the work itself scripted*,
distinct from the retired checklist — a triage-vocabulary question
(`docs/agents/triage-labels.md`) for Björn's return; this ruling uses the label
once, for #525.

---

## #501 — the substituted witness for *not configured*, and whether #531 belongs in v1.5

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/501#issuecomment-5577955024>

**The fork:** PR #530 was merged (`59479185`) and nothing here blocked it; its
merge-manager escalated two questions afterwards. **(1)** Criterion 2 of #501
asks that *"with an unset template on a non-macOS platform it reports not
configured"*, and asks it at the IPC seam. `checkout_ipc.rs` has no such test,
and the merge-manager judged that none can exist on this machine:
`knobas_app::checkout::open` and `commands` both call
`action.default_template()`, which reads `std::env::consts::OS`; the gate runs
on a Mac; and macOS ships a default for all three actions — so *not
configured* is a state no Mac reaches through a database. The PR moved the
platform into an argument of all three deciders (`default_template_on`,
`command_view`, `template_or_refusal`) and witnessed the refusal on the
decider, in `an_action_with_no_template_on_this_platform_is_not_configured`,
with the seam test `clearing_a_template_hands_this_platforms_default_back`
saying in its doc comment where the other arm is. The PR body's checked bullet
had named a test that exists nowhere; the merge-manager corrected it to the
real name and merged, calling this *"a disclosed, reasoned substitution rather
than a fork"*. Was the substitution right, should the box be open rather than
checked, and is a record, a ticket, or nothing owed? **(2)** #531: the spawn
wait in `open-in-editor.sh` breaks on `[ -s "$record" ]` where its Rust twin
`recorded()` waits for the trailing newline. The divergence can only produce a
false *failure*, on a driver that cannot yet run. v1.5, or leave it
`needs-triage` with no milestone?

**Ruling:** **(1) The substitution was right, criterion 2 stays checked, and
nothing more is owed for it in code or in a new ticket.** The question was
answered before the PR was written: spec #491's *Desktop witness, no human*
ends *"Off macOS nothing is witnessed; the buttons read not configured"*, its
Out of Scope list carries *"Witnessing on Linux or Windows"*, and the grilling
ruled *"macOS is the witnessed platform; others get command templates and no
witness"*. The precedent is **#496 part 3, not #500**: ADR-0013's 2026-09-07
consequence sorts a witness gap into two classes — *"awkward to reproduce is
not cannot produce: if the real instance can be driven into the fault by any
seed, flag or clock, it is witnessed live"*, and otherwise *"a fake in the
crate's test module is the witness for that one criterion"*. #500's locked
screen is the first class (a Mac can be unlocked, so the run is deferred and
#525 carries it); a Mac reporting a non-macOS `std::env::consts::OS` is the
second, so the criterion is *met by the sanctioned substitute*, not held open
against a run no ticket in this milestone may make. The two-place record that
exists — the §10.8 entry and the seam test's doc comment — is sufficient.
**Two lines of record-keeping are owed, and they ride in #531's PR:** this
ruling appended here as a `## #501` section, since no other v1.5 PR acts on
it; and the one citation ADR-0013's consequence asks of a substitute — *"The
fake's doc comment cites this line"* — added to
`an_action_with_no_template_on_this_platform_is_not_configured`'s doc comment,
naming that 2026-09-07 consequence. No test is added, no signature moves, no
`os` parameter is threaded into `open` or onto the wire, and nothing is
written into #525. **(2) #531 goes into v1.5, `ready-for-agent`, and must
merge before #525 runs.** It is not a desktop-witness ticket for the
one-at-a-time cap: it launches no bundle, and its run-criterion is not `just
desktop-witness` — which would refuse on the lock exactly as #530's did — but
`just witness-unit` green with the wait condition pinned. Per #500's ruling —
*"driver logic goes in a lib file `witness-unit` covers … because a driver is
not exempt from the gate because its run is"* — the wait becomes a function in
`testenv/desktop-witness-lib.sh` whose condition is the Rust twin's, the
trailing newline, with a `witness-unit` check that a record without it is not
yet a record. `launcher-hotkey.sh` has no record-file wait, so the shape is
#531's alone.

**Reasoning:** (1) the reading that keeps the frozen surface smallest, the
witness real and the ticket's scope unchanged is the one the PR took. The
three routes to the letter of criterion 2 are each worse: a test under
`#[cfg(not(target_os = "macos"))]` is the `cfg!` the spec review struck from
the first draft — *"the gate's machine asserted the opposite state and the
refusal branch was executed by nothing"* — moved into a test; an `os`
parameter on `open_checkout` widens a §10.8 wire shape for a testability seam,
which the ticket's own entry says it does not do; and an internal
`open_on(pool, entity_id, action, os)` moves the unwitnessed line rather than
removing it, since whatever passes `std::env::consts::OS` last is the line no
seam test on a Mac can see. The platform-as-argument shape puts the decision
under test for every platform, which is what the criterion was for, and the
residual can fail only toward a refusal, never toward a spawn, so ADR-0016's
property is not what rides on it. The box stays checked rather than open
because #500's open box has a payer (#525, and the exit waits on it), whereas
an open box here would be a debt nobody in v1.5 is permitted to pay — a hand
checklist by another name. The PR-body correction was owed regardless: a test
name that exists nowhere is a claim in a record that measures a
representation of the thing. (2) the exposure is narrow — the stub is `printf
'%s\n' "$@" > '$1'`, one open and one write, and `recorded()`'s own comment
says *"waiting for the trailing newline costs nothing and removes the
question"* — but the moment it can bite is the worst one in the milestone.
#525 is the last item before the v1.5 exit, its first run is *"the milestone's
only evidence that the desktop witness works at all"*, and #500's ruling
prices a red run at *"the three drivers are re-done against whatever the `ax
dump` shows"*. A spurious red from half a line would send the runner down that
path for a defect that is not there, and the Rust suite would give no hint
because it does not share the bug. Fixing it costs one condition and one
`witness-unit` check; leaving it unmilestoned leaves the exit gate carrying a
known flake. It stays out of #525 itself because #525 is `ready-for-human` for
the unlock alone and its body says *"Do not resolve it here"* about anything
red.

**If you disagree, the cost of reversing this is:** (1) nothing in code — no
test is deleted and no signature moves. Reversing means either running the
gate on a second operating system, which the spec's Out of Scope list and the
grilling both refuse for v1.5 and which is Björn's line to redraw, or adding a
platform parameter to an internal function so a seam test can pass `"linux"`:
one function, one test, the unwitnessed line moved up one frame, additive and
cheap at any time. Unchecking criterion 2 later is one edit to a closed ticket
and a line in #525 or a new ticket. The two record-keeping lines are a doc
comment and this section, reversible by deletion. (2) trivial — close #531
`wontfix` or strip its milestone before anyone starts it; the driver is
unchanged either way, and the first run on #525 then carries the one known
spurious-red path, which the runner should be told about if that is the choice
made.

---

## #502 — what `captured-from` names when no detail is open, and the activity line a born link's withdrawal has no partner for

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/502#issuecomment-5576885970>

**The fork:** two, both raised from the branch before any PR. (1) `Room.svelte` attached
`captured-from` only when a detail was open. #502's body and story 42 both say *"the foreground
entity **when a detail is open**"*, but spec #491's Implementation Decisions says *"`captured-from`
for the foreground entity **the heartbeat already computes**"* — *open detail, else room anchor,
else none* — `CONTEXT.md`'s **Capture** says *"if there was one"*, and **Passive attribution**
defines *foreground* as *"the open detail, else the asset in the Tree's pane, else the room's
anchor entity, else nothing"*. #503 is specified to pass *"the last foreground entity"*, which is
the heartbeat's value. Option A: the open detail only. Option B: the heartbeat's foreground. The
implementer leaned A. (2) Nothing in `knobas_core::note` writes an activity line, so withdrawing a
born link leaves an `unlinked` line whose `linked` partner never existed — owed inside #502, owed
as a follow-up, or not owed.

**Ruling:** (1) **Option B.** `captured-from` names the **foreground** as **Passive attribution**
defines it and as the heartbeat computes it, through the same `canBeTarget` guard, which refuses a
`ctx:` entity and nothing else. From *New note* the Tree branch is unreachable, so in
`Room.svelte` the value is *open detail, else the room's anchor, else none*; what binds is that at
the instant the note is written it equals what the heartbeat would send. Reading
`timer.foreground` makes that true by construction; recomputing it from `detail` and
`context.anchorId` is allowed, and then one test pins that the two cannot disagree. Two cases join
`Room.test.svelte.ts`: a promoted room with nothing open is born with `captured-in` to the context
**and** `captured-from` to its anchor; an ad-hoc stored room with nothing open is born with
`captured-in` alone. The four tests already on the branch stand — a derived room has no anchor. The
prose the implementer said was owed either way is ruled and written on this PR: `CONTEXT.md`'s
**Capture** gains the cross-reference *"the word as [Passive attribution](#passive-attribution)
defines it, so a promoted room with nothing open gives its anchor"*; the §10.8 entry's three
sentences are corrected; `bornWith()`'s doc comment inverts. No new glossary entry, no ADR.
(2) **Not owed inside #502.** One follow-up ticket, filed by the orchestrator outside v1.5,
`needs-triage`, for Björn to place — **#524**, *A note's birth is a line in the activity stream*.
Inside #502 one clause is owed on the §10.8 paragraph that already names the gap: that the class
exists on `main` before this ticket, and the follow-up's number. `draw_born_links`' doc comment
stands as written.

**Reasoning:** (1) the answer already exists in the glossary entry the grilling ruled and the
ticket binds itself to (*"Vocabulary: `CONTEXT.md`"*), with *foreground* defined once under
**Passive attribution**; spec #491 says it three times, in Implementation Decisions, in Further
Notes (*"the foreground a capture attaches is by construction the last one before focus left"*) and
in #503's body. The ticket's *"when a detail is open"* and story 42's *"when a detail was open, so
that the thought points at what I was reading"* name the first branch of the defined term — the
common case, and what the story's *so that* is about; read as a narrowing condition they would
contradict the glossary entry the ticket names as its vocabulary, and a ticket is a transcription
of the grilling rather than a re-decision of it. The #496 ruling's *"the ticket is the operative
text and the spec's count is a shorthand"* does not reach here: that was a count of migrations,
arithmetic with no term behind it, where this is a word with a definition ruled the same day as the
ticket. The implementer's redundancy objection is answered by the split `contexts.ts` already
draws — *"**Never the context's own id.** A context is a set, and time on a set has nowhere to go;
the anchor is a ticket or an epic, which is a thing"*: `captured-in` says which working set the
note belongs to (the membership write, ADR-0008) and `captured-from` says what the note was about,
the same two facts the timer in that room already tells apart. Under B a note born in a promoted
room with nothing open and the day review's passive block for that minute name one entity; under A
knobas' two records of *what was I on* disagree about the same instant. The decisive cost is the
one the implementer named: under A, #503 would have to record *was a detail open* separately, which
the heartbeat does not, so the two callers of one command diverge — and #503's body does not build
that recording, the glossary says *"the same two, by the same mechanism"*, and the spec's Further
Notes sentence would be false. (2) the gap is real and it is not this ticket's. Notes have never
written a line, and #409 is Björn's own ruling of 2026-09-05 on the nearest question — *"This
writes no activity lines at all"* — so the shape of the decision is his. The orphan `unlinked` line
already exists for another population: `Origin::Imported` `monitored-by` links, drawn by the estate
apply and by the poll, are withdrawable from the same panel, whose only refusal is `implied`, and
neither writes a `linked` line; `link_detail`'s pairing is a property of links drawn through
`create_link`, not an invariant of the log. Nothing runs on the pairing — detection's suppression
reads the tombstone and the tray uses `unlinked` only as a refresh signal — so the orphan misleads
a reader of the history panel and breaks nothing. Inside #502 it would be a third mechanism in a
ticket whose own words are *"No field on the note"* and two ordinary links; inside v1.5 it fits
neither precedent — it is not a witness gap on anything #491 promises (#522's reason) and the
defect predates the milestone (#518's reason). And the fix carries the trap #409 named: a note's
`[[ref]]` links are reconciled on every autosave, so a writer announcing *links* rather than the
*birth* would be that flood in another shape.

**If you disagree, the cost of reversing this is:** (1) trivial before merge — one expression in
`bornWith()`, two tests, three sentences in the contract entry and one in the glossary. After
merge, a note born under B in a promoted room carries a `captured-from` its author can withdraw
from the panel like any manual link, and there is no migration; the real cost of reversing is #503,
which would then have to record *was a detail open* beside the foreground, a second thing the
capture window remembers. (2) small either way: into v1.5 the orchestrator sets the milestone and
`ready-for-agent` and the ticket is one writer inside an existing transaction, one detail shape and
a seam test; into #502 the same work on an open branch plus a §10.8 sentence, and after merge it is
the follow-up ticket anyway.

### #502, second ruling — the absent-endpoint rule, and the three unwitnessed items

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/502#issuecomment-5577074363>

**The fork:** three, all from PR #526's own body. (1) The implementer, verbatim: *"The
absent-endpoint rule — the one decision neither the ticket nor the deputy settled. I took the
`reconcile_refs` precedent (no link, note still written) rather than refusing, and it deserves an
explicit yes or no."* (a) the note is written and the link is not drawn, which is what is on the
branch; (b) the whole call is refused, as a malformed target and a blank relation are; (c) the note
is written and the answer reports which links were not drawn. And whether §10.8's rule 3 is the
wording the contract requires. (2) Whether lifting the room foreground ladder into
`shell/timer.ts`'s `roomForeground`, with a source scan as its pin, is the right way to meet the
first ruling's *"one test pins that the two cannot disagree"*. (3) Whether any of the three
directions the PR body states as unwitnessed owes something before merge, the way #498's owed #522.

**Ruling:** (1) **(a), confirmed — with the rule's stated reason corrected, because as written it
names a case that does not exist.** `create_note` gains `invalid` for the two caller bugs and no
other code; the links panel is the report, and it already says *Nothing linked yet* where nothing
was drawn. The §10.8 wording changes in three places that say the same thing — rule 3,
`note::create`'s doc comment and the test's doc comment — all of which call the case *a race* and
give *a detail whose source was purged* as its example. **A purge does not remove the row**
(`knobas_sync::config`'s `PURGE_ITEMS` is an `update … set deleted_at`, and `CONTEXT.md`'s
**Purge** carries *delete* on its *Avoid* list for that reason), so a born link to a purged detail
**is** drawn, to a withdrawn entity, and the panel shows it marked — `LinkEnd.deleted_at`, *"Not a
filter -- a fact the reader is shown"*. The population rule 3 governs is *an id no row ever
carried*. ADR-0011: corrected in place. The corrected sentence makes a claim nothing on the branch
pinned, so one case joins the existing test, using `mock:PAY-198` — the fixture's genuinely
tombstoned row — and not one a test tombstones by hand. One further sentence is owed per #440's
precedent that *"the set of codes a frozen command can answer with is part of what §2 pins"*: that
`create_note` can now answer `invalid`, which it could not before, and nothing else new.
(2) **Not a decision, and left to the merge-manager as craft.** One function with two callers and a
scan that fails when either re-spells the rung is a way of meeting the first ruling's sentence, not
a departure from it; whether a behavioural comparison against a running timer would be the better
pin is the merge-manager's judgement, with mutant 9 as the evidence. (3) **One is owed, inside
#502, before merge: a born link survives the note's first autosave.** The other two owe nothing —
the walk being a fixture was settled by the #496 ruling's part (3), and `roomForeground`'s
structural pin is what the first ruling asked for.

**Reasoning:** (1) no settled text decides it — the ticket says nothing, spec #491 says nothing, and
the first ruling was not asked — so it is ruled the way the grilling's rulings would: smallest
frozen surface, real witness, ticket scope unchanged, and all three point the same way. **The
nearest precedent is the heartbeat, not `reconcile_refs`**: `commands::time`'s `heartbeat` writes
the observation with no target when the foreground is one the timer could never run on — *"losing
the attribution is honest, losing the observation is not"*. A born link is the attribution and the
note is the observation; the reader's act is the fact, and what it was about is derived from a
surface, so a derivation that cannot be made is dropped rather than fatal. `reconcile_refs` rightly
shares the SQL but not the precedent, since a ref that resolves to nothing is *shown back* so the
typo can be found. Story 2 is the note's and not the link's, and under (b) the capture window's
first keystroke could be refused for a room the reader last stood in yesterday, with the keystroke
nowhere to go; the glossary's own words are conditional (*"if there was one"*). (b) would also be
the wrong code — an endpoint with no row is `EndpointMissing → not_found` by Björn's #50 ruling,
which was made for a *dialog* where the reader named both ends — so it would add a second new code
where (a) adds one, and it would push onto #503 the handling of a refused first keystroke, the
divergence between the command's two callers that the first ruling refused to create. (c) needs a
place on the wire, and the entry records that `NoteDetail` and its neighbours are untouched; the
panel already reports. (3) the #522 standard is that a gap owes something when the unwitnessed
direction is the one a v1.5 feature was cut for, and where it is owed follows the cost of the
witness: a shared seed with a restore point was a ticket, one seam test on an open branch is a
clause. *New note* opens the born note in `NoteView.svelte`, whose `saveAfterMs` is 700, so the
reader's first keystroke reconciles a body naming no ref — if that withdrew born links, criterion 1
would be true at birth and false a second later.

**If you disagree, the cost of reversing this is:** (1) trivial before merge — the insert becomes
`values` instead of `select`, the foreign-key violation already maps through
`CoreError::from_link_write` to `EndpointMissing` and on to `not_found`, one test flips and one
§10.8 sentence changes. After merge: no migration and nothing to repair, since a note born under
(a) with a link it could not draw is one whose panel showed the reader so at the time. The one
ratchet is #503, which under (a) needs no refusal path on its first keystroke. (2) nothing — no
code and no record depends on which pin is chosen. (3) trivial: one test fewer on an open branch;
the property it pins is true today and stays true until someone edits a `where` clause in
`note.rs`, which is precisely when the test would have been wanted.

**Recorded by the implementer while acting on part 3:** the mutant the ruling names —
`and origin = $3` dropping out of `withdraw_refs_other_than` — **survives**, and so does dropping
`and relation = $2`. A born link is outside that clause **twice over**, by relation and by origin,
and either alone is sufficient; the new test dies only when both go. The ruling's expectation that
the origin is what protects a born link is half the picture, and rule 1 of the §10.8 entry now says
so.

**Re-run by the merge-manager of #526, with the scope stated**, since "survives" without one
invites the wrong conclusion. Over `cargo test -p knobas-app --test entity` (42 tests) both
single-clause mutants pass and the both-clauses mutant fails
`a_born_link_survives_the_notes_first_autosave` on *"the body governs the links the body derived,
and nothing else: []"* — the implementer's account, exactly. Widened to `knobas-core`, the two
clauses part company: `and origin = $3` **is** pinned, by
`crates/knobas-core/tests/notes.rs`'s `a_hand_drawn_link_out_of_a_note_survives_the_body_changing`,
which dies without it; `and relation = $2` is pinned by nothing in either binary. So the correction
holds for the born-link property the ruling was about, and the origin clause is not unguarded in
the codebase — a full `just check` goes red on that mutant.

---

## #504 — whether *Open alerts in my contexts* carries the inbox's ack clause

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/504#issuecomment-5577388583>

**The fork:** the ticket says the list "is the inbox's routing rule" and glosses that as "an open
alert whose asset some unarchived context holds directly or through an ancestor". The inbox's
sixth-category statement — `alert!()` in `crates/knobas-core/src/inbox.rs` — ends
`where a.closed_at is null and a.acked_at is null`. **Option A** (built, PR #527): `closed_at is
null` plus the context clause and no ack clause, so an acked alert stays on the list; pinned by
`an_acked_alert_is_still_open_and_still_on_the_list`. **Option B:** copy the inbox's statement
whole, ack clause included, so the list empties as alerts are acked. The count differs by exactly
the acked alerts.

**Ruling:** option A, as built. This is not a new decision: the documents already answer it, and the
ticket's two halves say the same thing once *routing rule* is read as the defined term it is. One
correction to the implementer's citation, and one glossary clause owed on **Alert**.

**Reasoning:** *"Routing rule" is a defined term and the ack is not in it.* Spec §12.3 puts the two
in consecutive sentences: *"**Alert routing rule (R3, now spec):** an alert reaches the inbox only
when some context holds the affected asset (directly or via an ancestor or via the context's
monitors); all open alerts always show in the Assets views and the top-strip count. **Ack** is
knobas-local: clears the inbox item, writes history, the alert stays open until the monitor
recovers"*. `CONTEXT.md`'s **Alert** keeps the same split, and its #446 amendment uses the term the
same way — *"the one place the alert's routing rule and `member_ids` differ"* is about which
contexts count. So `alert!()`'s statement is the routing rule **plus the inbox's own lifecycle
clause**, not the rule, and the ticket's gloss is a restatement of the term rather than a second
reading of it. *"Open" is defined too*: **Alert** says *"open until the monitor recovers … **Only a
return to *up* closes one**"*, so a list titled *Open alerts* that dropped acked ones would be false
in its first word, and option B would have to rename the list to be honest. *Which side of the ack
an asset list sits on is settled* by §12.3's *"all open alerts always show in the Assets views and
the top-strip count"*: this list draws assets and opens the Tree, so it is an estate surface beside
the Assets view and the top strip, not a second inbox. *And story 47's "so that the launcher and the
inbox agree" is about routing, not about a number*: #446's ack writes `acked_at` **and**
`complete_with` in one transaction, and the inbox's count excludes snoozed items, so copying
`acked_at is null` alone would still disagree on every snoozed alert — agreeing on the number would
mean importing the inbox's shelf into a smart list, which is neither ticket's scope. What the two
surfaces are asked to agree on is *who is told*: an alert on an asset no unarchived context holds is
in neither, which is the negative the ticket names and the PR pins
(`an_open_alert_reaches_the_list_through_the_contexts_ancestors`, mutant 3). *The citation:* there is
no ruling on #446 — the issue has no comments. The sentence the PR quoted is the doc comment on
`alert!()` itself, #446's implementer reading spec #427 story 62; its substance is right and its
authority is the glossary and §12.3, so `lists.rs` and the PR body cite those and drop the word
*ruling*. A doc comment is a reading, and the next reader should be pointed at what it read.
*The symmetry caution cuts the same way here:* `monitor_roster` keeps a paused monitor because of
what the roster is for, and this list keeps an acked alert because of what an ack is — seen, not
fixed (story 62). Neither surface copies another's clause by analogy; each takes the clause its own
name commits it to.

**If you disagree, the cost of reversing this is:** small in code — `and al.acked_at is null` beside
`closed_at` in `open_alert_opened_at!` (which `alerts_in_context_pred!` and the badge stamp both
read), one assertion flipped in `an_acked_alert_is_still_open_and_still_on_the_list`, no migration
and no wire change — but not only code: the list's label would have to change to say *unacked*, the
**Alert** clause amended by this PR would be rewritten, and story 47's *"agree"* would still be
false on snooze, so a reversal that wants the inbox and the launcher to show one number would also
have to bring the inbox's shelf into the list, which is a design change and not a clause.

---

## #505 — the frozen gate for `depends_on_this`, the direction the links are walked, two craft decisions, and a stale doc comment

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/505#issuecomment-5577594293>

**The fork:** four, raised by PR #528 and flagged rather than left open — the implementer decided
each and asked for ratification. (1) The PR adds `commands::assets::depends_on_this`, the DTOs
`DependsOnRow` and `DependsOnThis`, and one line at the foot of `crates/knobas-app/src/lib.rs`'s
handler list: the IPC command schema and one of the two append-only barrels, both on §10.8's list.
Does the ticket authorise the touch, and is the entry at `docs/contract.md:8183` what the contract
requires? (2) `NEXT_LAYER` walks links from their **`to`** end — `select l.from_id … where l.to_id
= any($1)` — on spec #491's *"confirmed links with relation `depends-on` or `runs-on` toward the
asset"*, while `CONTEXT.md`'s **Depends on this** entry says only *"every asset linked to it by
`depends-on` or `runs-on`"* and is direction-agnostic. Yes or no. (3) Two craft decisions made in
the same breath: `depends_on_this` is a command of its own rather than a field on `get_asset`, and
a row reached through containment carries `relation: null` rather than the word `holds`. (4)
`sourceAssets`' doc comment in `app/src/lib/ipc/assets.ts` still says *"**Empty until M4.1.** No
adapter emits a `monitor` yet"*, which stopped being true when #442's Kuma adapter landed. Owed
here, owed as a ticket, or not owed?

**Ruling:** (1) **Authorised, and the entry is sufficient as written**; the merge-manager may
proceed once the clause and this append are on the branch. One clause is owed and is in this PR:
the entry's *"Björn keeps the gate for frozen contracts and this entry is flagged for his review"*
gains *"— ratified in his absence by the deputy's ruling of 2026-09-08 on #505"*, the wording
#496's and #498's entries carry. Two claims the merge-manager's deep pass should start with, both
fixed on the branch by this PR rather than left for review: the entry said the read touches
*"`knobas.asset`, `knobas.link` and `knobas.route`"* while `NEXT_LAYER` reads
`knobas.confirmed_link`, `0007`'s view over that table — true as a share-export claim and now
carrying a sentence that says so — and the entry's pin count disagreed with the PR body's, which is
seven. (2) **Yes. The `to` end is the operative reading, and it was already decided.** One
parenthetical is owed on `CONTEXT.md`'s **Depends on this** entry, in that entry's amendment style,
so the direction is not argued a third time. No ADR. (3) **Both ratified, and neither is reopened
by the merge-manager.** What stays the merge-manager's is craft inside those two answers — the
tie-break's `nulls first`, the ordering, the label text — not the answers. The `Option<String>` the
implementer flagged as possible Primitive Obsession is the house style and is not owed a newtype.
(4) **Owed here, on #528, as one hunk and no test.** Not a ticket.

**Reasoning:** (1) the freeze exists so that a wire shape never arrives unannounced, and this one
was announced in spec #491's stream map row 7 (*"one IPC read"*), named in the ticket's body
(*"One IPC read on the assets module (§10.8 entry)"*) and its third criterion, and written up in
the section the freeze points at. A read has a shape, and the two DTOs are that shape and nothing
more — two additive DTOs are the read, not a second touch beside it, which is how #452's entry
lists its four shapes under one authorisation. Checked at the gate: nothing under
`crates/knobas-db/migrations/**`, `crates/knobas-source/src/**` or `crates/knobas-http/**` moves;
`error.rs`, `profile.rs` and `app/src/lib/ipc/index.ts` are untouched; `lib.rs` grows one appended
line; `crates/knobas-core/src/link.rs` gains two `pub const` and that crate is not on the frozen
list, so the constants are the ticket's third criterion and not a gate question; the entry sits
inside §10.8 by an anchored header grep. (2) *Toward* is an end, not a pair, and the glossary
already supplies which end: **Link** says *"the stored direction is what tells `blocks` from
`blocked by`"*, and `app/src/lib/detail/relations.ts` gives `runs-on` the readings *runs on* /
*hosts*. So `container --runs-on--> machine` says the container runs on the machine, and what
breaks when the machine goes down is the container — the `from` end of a link whose `to` is the
machine. The undirected reading would list the machine under the container's panel, which is the
panel answering the opposite question, and story 51's *"so that blast radius is one glance"* makes
a glance at the wrong set worse than no panel. Mutant M1 is that direction witnessed on a real
PostgreSQL over the real estate file, which is ADR-0013's witness. (3) both are the smallest
reading of a record that already exists. The read of its own is the spec's own sentence, and a
field on `get_asset` would be zero reads and one changed DTO — a different frozen-surface touch
from the one the spec named — with the pane paying the walk on every selection in every Miller
column for a section a reader scrolls to. `relation: null` is ADR-0014 applied: a row that came
through the parent field came through no relation, and the implementer's reason — *a word there is
one a reader could then draw as a link and have counted twice* — is that ADR's first rejected
option restated. (4) the file is already in this PR's diff, the sentence is one paragraph in it,
and the Rust side of the same read already carries the true sentence; a ticket for one stale
paragraph in a file this PR edits would cost the loop more than the paragraph. It is not the #496
part-1 case, where the *local index* wording was rendered text in other files with tests behind it.
A doc comment is living text, and this milestone's own lesson — twice on this very branch — is
prose that outran the code.

**If you disagree, the cost of reversing this is:** (1) moderate after merge — the command leaves
the barrel only by a further §10.8 entry and the two DTOs with it; there is no migration, which is
the entry's own point, so nothing is irreversible in the database. Before merge it is one revert of
a branch nothing has built on. (2) small in code — one predicate in `NEXT_LAYER` and the three
tests that pin it, plus the glossary clause struck — but it would reverse spec #491's own sentence,
so it is a spec amendment and a ticket, not a comment. (3) a §10.8 entry of its own and a change to
`AssetDetail`, which every asset test reads, for the first; trivial before merge for the second,
one string on the wire and one test, but it would be an ADR-0014 supersession and Björn's to write.
(4) nil — one paragraph, no code, no test.

**On the witness, which no part reopens:** the `--demo` half of the second criterion was ruled on
#496 (part 3), which names #505, and the PR body meets its four conditions. The two things the
implementer reported that changed the code — the survived mutant that showed `NEXT_LAYER`'s inner
join was redundant *and its comment wrong*, and the mirror test that never shape-checked the null
row it said was its point — are the witness doing what ADR-0013 asks and are recorded where the
next reader will look. Nothing is owed on them.

---

## #506 — the frozen gate for the saved smart list, the raw text it stores, and two witness gaps

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/506#issuecomment-5578127977>

**The fork:** PR #532 raised three, and raised them by deciding and disclosing rather than by
stopping. (1) It adds migration `0025` (`knobas.smart_list (id, label, query, created_at,
updated_at)`), `create_smart_list`, `rename_smart_list` and `delete_smart_list` on
`commands::search`, three lines on `crates/knobas-app/src/lib.rs`'s handler list, and `saved: bool`
and `needs_attention: bool` on the **existing** `SmartListSummary`, mirrored in
`app/src/lib/ipc/search.ts`. Migrations, the IPC command schema and the append-only barrels are on
§10.8's list, and a field on an existing DTO is a wire type changing shape, which the contract
treats as its own kind of touch. Does the ticket authorise each, and is the entry what the contract
requires? (2) The migration stores the **raw box text and never a parse of it**, and `saved::plan`
reads a stored row back through the same parser a keystroke goes through, answering a `Runnable` or
a `Refusal` and never an error. Is that the right durability decision for stored user text across a
grammar change, and does the glossary record it? (3) The PR body states two witness gaps — that the
perf gate does not measure a board with saved lists on it, and that the browser walk cannot witness
a count, a badge or the grammar's verdict. Does either owe something inside #506, a follow-up
ticket, or nothing?

**Ruling:** (1) **Authorised on all three counts, and the entry is sufficient; the merge-manager
may proceed** once the ratification clause and this append are on the branch. The ticket names the
migration and the commands in its own words and in its first criterion; spec #491's stream map row
8 says *"IPC create/delete"* — two where the ticket says three — and story 58 is why the ticket's
count is the operative one, the same reading #491's stream 2 already carries (*"the ticket is the
operative text and the spec's count is a shorthand, not a prohibition"*). **The two fields are
ratified as a touch in their own right** rather than folded into the commands, because the contract
is explicit that they are one: the #409 entry says *"a field added to a DTO, no command, no
migration, and an entry all the same. A wire type changing shape is the thing this section exists
to record"*, with #284's `EntityRow.path` as its precedent. The ticket authorises them by
consequence — *"the launcher's list panel merges built-ins and saved lists into one shape"* and
*"A saved list whose query the grammar refuses shows *needs attention* rather than an error"* — and
two booleans on the existing shape is the **smallest** reading of that sentence: a second DTO or a
second command would be a larger frozen surface and would contradict story 57 on the wire itself.
The clause owed: the entry's flagged-for-Björn sentence stays and gains *"— ratified in his absence
by the deputy's ruling of 2026-09-08 on #506"*, the wording #496's, #498's and #505's entries carry.
Three claims the merge-manager starts with, settled on the branch: the entry now says **why neither
field needs `#[serde(default)]`** (nothing decodes `SmartListSummary` anywhere — every value is
built in Rust and serialized outwards, and it is in no archive, settings row or file), it says that
a **database fault is not *needs attention*** (`saved::summaries` propagates `SearchError::Db` with
`?` on all three reads, so a statement failure is `internal`), and the pin count is corrected from
six to **seven**, which was the entry lagging a case the code review added. (2) **Ratified, and it
was already decided — the implementer transcribed it rather than choosing it.** Spec #491's
Implementation Decisions say *"a `smart_list` table (id, label, **query text**, stamps)"*; story 60
is reachable only if what is stored can be refused, and a parse cannot be refused by the grammar
that produced it; ruling P2 already committed the launcher to the box text. **One clause is owed in
the glossary and is on the branch: no migration ever rewrites `knobas.smart_list.query` to a newer
grammar.** A data migration that *upgraded* stored queries is a parse in disguise — it commits the
migration's reading of what the reader wrote — and it would make *needs attention* a state no row
can reach. The row stays as the reader wrote it; a refused one is deleted and the search saved
again. In the same breath the *needs attention* sentence stops saying *"Rename it or delete it"*,
since a rename changes the label and not the query, and offers what actually clears the state. No
ADR: the rule is in the spec's own words, the migration's immutable header and the glossary entry.
(3) **The browser walk owes nothing; the perf gap owes one paragraph inside #506 and one follow-up
ticket in v1.5.** The walk is settled by #496 part 3, which names #506 by number: showing the rail
is what the walk witnesses, and that the count is right, the badge lights and clears and the
grammar refuses what it should are the second criterion's, at the seam, where six tests over a real
PostgreSQL carry them and mutants A, B, C and G pin them. The perf gap is a class of its own —
**a budget whose fixture stopped covering what the budget names** — and `perf.rs`'s own *Fixture,
stated* section is where it belongs, naming **#533**. `MAX_SAVED_LISTS = 64`, kept beyond the
ticket, **stays**: it is what makes the gap finite, and #533 is what turns 64 into a measured
number.

**Reasoning:** (1) the freeze exists so that a wire shape never arrives unannounced; this one was
announced in the spec's stream map, named in the ticket's body and first criterion, and written up
in the section the freeze points at. The smallest frozen surface that meets stories 56–58 and 60 is
one table, three writes on the module that already owns the rail, and two booleans on the shape the
rail already draws — no new DTO, no event, no `WriteOp`, no `source_id`, one barrel growing three
lines and the other untouched because it lists modules. Checked at the gate: one file under
`crates/knobas-db/migrations/` (one `create table` and a `comment on table`, no `alter`), the next
free number after #499's `0024`, nothing under `crates/knobas-source/src/**` or
`crates/knobas-http/**`, no `error.rs`, `profile.rs` or `app/src/lib/ipc/index.ts`, and the entry
inside §10.8 by an anchored header grep. (2) the reading that keeps the frozen surface smallest is
the one with one column of text, and it is the only reading under which story 60 can be witnessed
at all — which the seam test does, with a row today's `create` would refuse. A stored parse would
be a second copy of §4's grammar frozen at the version that wrote it: the copy-that-drifts class
the working model names, in a table. (3) ADR-0013's consequence sorts a gap by whether the witness
can be driven into the state — *"'awkward to reproduce' is not 'cannot produce'"* — and the perf
fixture is a real PostgreSQL a seed can drive to 64 saved lists, so a measurement is owed and is not
#506's to make. In v1.5 rather than after, by the reading #496 part 1 and #498 took: a milestone
that ships a rail with a cap chosen as *"generous enough"* and never measured against the budget the
built-ins meet has a defect in its witness, not a feature for the next milestone. The walk owes
nothing because the question was asked and answered once for the seven tickets carrying the
sentence, and a second ruling on the same words would be re-deciding.

**If you disagree, the cost of reversing this is:** (1) moderate after merge — the three commands
and two fields leave the schema only by a further §10.8 entry, and `0025` is a table on every
database that has started since, so removing it is a `0026` that drops it, never an edit. Before
merge it is one revert of a branch nothing has built on; #507 is blocked by #506 and has not
started. (2) high — it is a column's meaning on a migrated table, so storing a parse later means
`0026` adding a column and a backfill that parses every stored row with whichever grammar is
current, and story 60 leaving the product. Before merge it is the same migration rewritten, cheap
in code and expensive in the spec, whose sentence would have to change with it. (3) trivial for the
walk — no criterion text moves and no test is deleted; reversing it means naming a witness that can
attach to a Tauri webview, which is #500's harness and its one-at-a-time cap. For the perf gap the
paragraph is one doc comment and #533 is one seed and one assertion in an `#[ignore]`d test, closable
unbuilt if Björn judges the budget the built-ins' alone. If the measurement fails the budget, the
cheap reversal is the constant and the expensive one is the summary statement's shape, which is why
the measurement is in this milestone and not the next.

**For the orchestrator:** #533 is filed (v1.5, `ready-for-agent`, blocked by #506, under no cap).
The merge is not held for its measurement.

---

## #508 — the frozen gate for the `producer` argument, three decisions taken as settled, and two disclosures

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/508#issuecomment-5578215465>

**The fork:** three, raised by PR #534 and flagged rather than left open — the implementer decided
each, disclosed them, and held the PR for the gate. (1) The PR changes the signatures of two
commands the #439 entry froze: `preview_estate_import(.., file: String)` and
`apply_estate_import(.., file: String)` each gain `producer: String`, **required**, mirrored in
`app/src/lib/ipc/assets.ts`, with an entry at the foot of §10.8 saying in place *"no ruling had been
posted on #508 when this entry was written, so this sentence records the flag and claims no
ratification"*. This is not the shape the milestone's earlier gates saw: #409's `CandidateSource`
and #284's `EntityRow.path` were fields added to a DTO and #502's `create_note` grew an `Option` —
each additive, each leaving an older payload decodable. A required argument is not additive: a
caller sending `{file}` alone is refused with *"missing required key producer"*, which the gate
itself showed on its first run. Spec #491's stream map has no row for #508 and its *Contract*
paragraph calls its own list *"the complete list; nothing else on the frozen list is touched"*.
(2) Three decisions made without a fork being raised, on the ground that the record already made
them: the producer is an argument and not a field of the file; the match is a **rename of the
parse** (`matched_by_origin_key` answers *which asset*, `rename_matched` writes it into every
mention of the id, and everything downstream in `plan` sees one kind of id) rather than a branch in
the planner; and **no Docker producer is declared**, because its key needs a container's docker
context plus its name and no container in `testenv/hetzner/estate.json` carries either as a
property. (3) Two disclosures in the PR body: two surviving mutants, argued as equivalent by
construction; and the three `hcloud_id` values, which no gate can check for being *wrong*.

**Ruling:** (1) **Authorised, and the entry is sufficient as written; the merge-manager may
proceed** once the ratification clause, the rulings-file append and part 3's README clause are on
the branch. The ticket authorises the touch by consequence, in its own words — *"Which property is
the key is declared **per producer** (`hcloud_id`; docker context plus container name), and the
file-import producer declares none"*, *"The Import dialog gains its chooser with the estate file as
its only entry"*, and the first criterion's *"the file import with no declared key behaves exactly
as before"*. A key declared per producer, with one producer declaring none, is a planner that has
to be told which producer made the file; a chooser is where that answer comes from; and the answer
has to cross the bridge. Spec #491's Implementation Decisions fix both ends of the wire: *"each
returns an estate file's text in the checked-in shape, and **the existing** preview and apply
commands consume it. The Import dialog's chooser selects the producer."* *Existing* rules out a new
command; *checked-in shape* rules out a field in the file. What is left is an argument on the two
commands the spec named. The clause owed: the entry's *"claims no ratification"* sentence **stays**
as the history it is and gains beside it *"— ratified in his absence by the deputy's ruling of
2026-09-08 on #508"*, side by side rather than rewritten, the treatment §10.8 gives every sentence
it supersedes. (2) **All three confirmed.** The producer as an argument was settled by the
sentences above and the implementer was right not to raise it. The rename is confirmed as the
mechanism, and the one reading the implementer named as a decision rather than a consequence — the
match does not look at the entry's declared type — is confirmed as the ticket's own sentence
(*"an asset whose origin-key property matches is the same asset"*) and `CONTEXT.md`'s (*"the second
matching rule beside the id, and the only one"*); a type check would be a third rule nobody asked
for. No Docker producer in #508, and the deferral is right for **ADR-0013's** reason and not only
for tidiness. (3) **Nothing owed on the mutants. The `hcloud_id` gap is soundly discharged by
#509**, and what is owed is that the discharge is *named*: one clause in
`testenv/hetzner/README.md` naming stream 9's `just estate-live` as the witness, and the
merge-manager re-running `hcloud server list` against the repo-root `.env`'s `HETZNER_API_TOKEN` —
no tunnel, one call — against the three ids and the three IPs, because the loop's rule for anything
quoted from a live system is that the merge-manager re-runs it. No follow-up ticket: #509 *is* that
ticket, and a second one would be a record of a record.

**Reasoning:** (1) the freeze exists so a wire shape never arrives unannounced. This one was
announced at both ends by the spec, named in the ticket's body and first criterion, and written up
in the section the freeze points at, with the *why not `Option`* paragraph a §10.8 entry for a
required argument owes. The three ways of keeping the signatures untouched are each a larger touch
or a contradiction of the ticket: a field in the file changes the estate-file schema —
`deny_unknown_fields`, `ASSET_KEYS`, and a seam ADR-0015 says three readers already share — and
contradicts *"in the checked-in shape"*; consulting every producer's key for every file contradicts
*"the file-import producer declares none"*; a command per producer is a new command and a new
barrel line per importer where the spec said *existing*. On *additive*: what the contract records is
that **a shape changed**, not whether an older peer survives it — #409's words are *"a wire type
changing shape is the thing this section exists to record"*. Where a peer can be older the entries
say so and defend against it: #284's `path` carries `#[serde(default)]` because a `SearchHit` can
sit in a share archive. **A Tauri command's arguments have exactly one caller, the webview compiled
into the same bundle, and are never persisted, piped or exported**, so there is no older payload for
the argument to break; *required* costs nothing a peer could pay and buys the red gate the entry
claims (`every_asset_command_is_registered_and_its_arguments_decode`,
`the_mirror_sends_the_argument_names_tauri_expects`) instead of a silent match-on-nothing that
copies every asset. That is the difference from #502's `create_note`, where an absent list *means*
something; here absence has no meaning that is safe. (2) the rename is the reading that keeps
#439's machinery unbranched, which is the ticket's own phrasing — *"the entry's id is replaced by
the tree's before the plan is drawn"* — transcribed rather than interpreted. It is also what makes
spec #491's story 64 true rather than approximately true: **because the rename lands before
`HAND_EDITED`'s bind**, a hand edit on the matched tree asset is honoured exactly as it would be
for an entry that arrived under the tree's id, so a matched entry previews and applies as *the*
asset rather than as a look-alike that overwrites it. Renaming every mention is not thoroughness
for its own sake — mutant B shows the file is refused for a dangling parent if one mention is
missed. For Docker, ADR-0013's rule is that the witness is the real system, and **a declared key no
asset carries is a rule with no witness at all** — not fixture-witnessed, not live-witnessed,
unreachable — which would let the registry claim a producer this build cannot honour, the class of
sentence this milestone has twice caught outrunning the code. (3) the two survivors are equivalent
**by construction** and not merely by today's fixture: `origin_key_of` builds one JSON array from
the parts a bag carries and the two sides are compared as that string; an estate asset lacking a
part is excluded by `?&` on the way in, and if `?&` is dropped it is excluded by the length check
instead; a file entry lacking a part is excluded by the length check, and if that is dropped its
shorter array can never equal a full one. Only removing both lets a half key meet a half key, and
that mutant dies. No test could kill either alone without deleting the redundancy the doc comments
argue for keeping. For the values, ADR-0013's consequence sorts a gap by whether the real instance
can be driven into the state, hcloud is the real instance and #509's recipe is the seed; **a
discharge by a later ticket in the same milestone is sound when the later ticket cannot pass while
the gap is open**, and #509's third criterion cannot — a wrong id makes that server unknown by id
and unmatched by key, so it previews as *new* and the recipe goes red. That is stronger than #498's
case, where a follow-up had to be *filed* to make the witness exist. It is not sound as a silence:
a README sentence ending *"rather than a red test"* and a #509 implementer who reads a red recipe as
their own bug are the two ways the debt gets lost, and the two owed lines close both.

**If you disagree, the cost of reversing this is:** (1) moderate after merge and growing — the
argument leaves the two signatures only by a further §10.8 entry, and removing it means choosing
one of the three larger alternatives, since the planner still has to learn the producer somehow;
there is **no migration**, which is the entry's own point, so nothing is irreversible in the
database. Before merge it is one revert of a branch nothing has built on, except that #509 and #510
are blocked on it and #509's produce command is written against this argument's existence, so every
day of delay widens the reversal. (2) moderate for the rename — replacing it with a branch in
`plan` means the groups, the ordering, the hand-edit merge and the writes each learn a second kind
of id, and the four seam tests that pin the rename are rewritten with it; nothing on the wire moves.
Adding a type check is one predicate and one test, but it would be a third matching rule and the
glossary's *"and the only one"* would need Björn's hand. Nil for the Docker deferral: declaring it
earlier or later is #510's work either way. (3) nil for the mutants — a survivor ruled equivalent
can be re-examined at any time by deleting one half and watching mutant L's tests. Trivial for the
values: one clause and one comment; if Björn wants them gated before #509, the cheap shape is an
`#[ignore]`d test in `estate_file.rs` calling `hcloud server list` under the token, which is a
tunnel-free live check and one ticket, not a rewrite.

**Flagged, not ruled:** spec #491's *Contract* paragraph is short by this argument, as it was short
by stream 2's migration and #506's two fields; the rule recorded on #491 and applied on #506 covers
it — *"The ticket is the operative text and the spec's count is a shorthand, not a prohibition"* —
and the spec is left as written. The fork **#510** will meet is named before it is met: the
mechanism keys on **properties of the entry itself**, and in the checked-in estate a container's
name is a field and `docker_context` sits on the engine above it, so #510 either writes both as
properties on every container — which fits the mechanism unchanged and is what **Origin key** says
(*"the property an importer sets"*) — or changes `Producer` to read a key across a parent's
property or an entry's field, which is a fork it raises. No new glossary entry and no ADR: the
**Importer** amendment already on this branch is the glossary record and stays.

---

## #518 — two doc-comment lines in the frozen crate, the entry criterion 1 forbids, and two declines

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/518#issuecomment-5578535760>

**The fork:** three, raised by PR #535 and decided by the implementer rather than left open, then
put to the gate. (1) The PR changes two `///` lines on `Capability::Search` in
`crates/knobas-source/src/lib.rs`, and §10.8 freezes *"any change to `crates/knobas-source/src/**`
(the `Source` trait, its DTOs, the contract battery)"*. No entry was written and no fork raised:
*"the ticket names the surface in its own words ('two SPI doc comments in `knobas-source`'), the
ruling called this class 'a frozen crate's doc comments', no surface moves (two `///` lines on
`Capability::Search`), and criterion 1 forbids the `docs/contract.md` edit an entry would need."*
Is a doc-comment change inside the frozen crate a §10.8 touch at all; if a record is owed, where
can it go when criterion 1 requires `docs/contract.md` byte-identical; and may the merge-manager
proceed. (2) The paste-miss test in `Launcher.test.svelte.ts` widens from `.miss` to the whole
launcher overlay, asserting no visible text there contains *index* — stronger than the ticket asked
for, and a future feature that legitimately says the word on that screen would trip it. (3) The
two-axis review asked for an `IpcError::not_in_mirror(id)` helper in `error.rs` and called the
`docs/specs/2026-08-23-knobas-design.md` row edit scope creep; the implementer declined the first
as a fork not its to take and kept the second.

**Ruling:** (1) **Not a §10.8 touch; no entry is owed, none may be written in this PR, and the
merge-manager may proceed.** The record of the question is this ruling, appended here by PR #535,
which is the one edit to a record that PR makes. The answer already existed, in the spec and in a
merged precedent: spec #491's scrub paragraph lists *"the two doc comments (the `Source` trait's
module doc, the asset type table's)"* among the **living documents**, while its *Contract*
paragraph in the same document says *"nothing else on the frozen list is touched"* — both true at
once only if the SPI's doc comments are not on that list. #493 acted on exactly that reading:
commit `5cd0bb09` (PR #513) rewrote two `//!` lines of the same file, touched `docs/contract.md`
not at all, wrote no entry, and merged. The rule, so the next scrub does not fork again: **a change
inside `crates/knobas-source/src/**` whose diff is `///` and `//!` hunks only, and which preserves
what the sentence commits an adapter to (a word swap, a stale name, a cross-reference), is not a
§10.8 touch and owes no entry**; the merge-manager's gate is `git diff -- crates/knobas-source/src/`
showing prose and nothing else. **A doc comment that changes what a clause *means* — a tolerance, a
direction, a field's semantics — is a contract change in prose form and goes to the gate as if it
were code, whatever the diff looks like.** On criterion 1 and this file: the criterion names
`docs/decisions/` among the records that keep their wording, and this file's header says *"the PR
that acts on it appends the same four parts here"* — **the append wins**, as it did for #493, whose
PR appended seventy lines here under a criterion that kept the records' word counts, because the
criterion is aimed at the scrub rewriting a record and an appended ruling is not the scrub. PR
#535's *"No ruling is appended"* paragraph was correct when written and is superseded: it becomes a
sentence saying the #518 section is the only delta over the records, and its *"105 → 105"* count
becomes *105 before the append*. (2) **Keep the widened assertion; craft, not a fork, and the
merge-manager owns the final say.** One line is owed in the test: a comment saying what the
assertion guards — the launcher is the screen that names the store, and *index* there is the
synonym `CONTEXT.md`'s **Mirror** forbids — and that a future feature with a legitimate use of the
word (the Postgres index `Diagnostics.svelte` names, say) **narrows** it to the footnote and the
miss panel rather than deleting it. (3) **Both declines confirmed.** No helper in #518 and no
follow-up ticket for one: `crates/knobas-app/src/error.rs` is on §10.8's list by name, #518 does
not name it, and the #498 ruling already settled the class (*"a constructor is not on the list …
Additive is not exempt"*, *"no follow-up ticket is owed for one"*). The spec edit stands: spec #491
names *"the spec"* first among the living documents its scrub touches, #493 edited the same file
under that sentence, and criterion 1's record list does not contain `docs/specs/`. No glossary
change and no ADR: **Mirror**'s `_Avoid_: cache, index` is the rule, and the PR only makes the tree
obey it.

**Reasoning:** (1) the reading that keeps the frozen surface smallest is the one the spec had
already taken. The freeze's reason is written at the top of the module it freezes — *"changing
anything in this module afterwards needs an orchestrator decision and a spec update, because it
breaks every adapter at once"* — and a `///` line breaks no adapter, changes no signature,
variant or field, and is invisible to the contract battery. The parenthetical is the definition,
not an illustration, which is how this list has read it before: the #498 ruling read *"its DTOs"*
as the thing frozen, the #146 ruling read *"the contract battery"* as *"that contract's executable
spec"*, and the #505 ruling said of a stale comment in `ipc/assets.ts` that *"a doc comment is
living text."* #452's *"additive is not exempt"* is about additions to the compiled surface and
does not reach prose. Treating every `///` as frozen would make a typo an orchestrator decision and
put every future scrub into the contradiction #518 walked into — an entry owed in a file the ticket
forbids editing. Checked on the branch rather than from the PR body: `git diff --stat
9fb9e1fd...issue-518` over `docs/contract.md`, `docs/adr/`, `docs/decisions/`, `mockups/` and
`crates/knobas-db/migrations/` is empty, and the diff over `crates/knobas-source/src/` is two `///`
hunks with no non-comment line; the two adapter `descriptor.rs` edits are `//` comments in crates
§10.8 does not name. (2) the glossary forbids *index* as a name for the mirror, and the launcher's
footnote, placeholder and miss panel are the three places the app names the mirror to the user, so
a fourth string saying *index* on that screen is far likelier to be the forbidden synonym than a
database object — the friction points the right way. The PR's proof 4 is the assertion earning its
keep, the footnote mutant having been invisible at `.miss` scope, and proof 6's positive assertion
is what carries the footnote now, so the negative is a fence rather than the witness. (3) the
answer to both exists in text already quoted; neither is a new decision. The drift a helper would
prevent is now caught where it matters: each of the four Rust sentences has an `assert_eq!` on its
full text.

**If you disagree, the cost of reversing this is:** (1) trivial in code — two `///` lines return to
the old phrase, and criterion 1's search then finds two hits the ticket says it must not. The real
cost is to the rule: if a doc comment in the frozen crate is a §10.8 touch, #493's merged scrub
retroactively owes an entry, this one owes one criterion 1 forbids, and one of spec #491's two
sentences has to be rewritten — a correction to the spec and to §10.8's wording, which is Björn's
and not a comment's. (2) nothing — narrow the selector back to `.miss` in one line; commit 4's
positive footnote test still pins the wording. (3) trivial either way — one constructor later under
a §10.8 entry of its own and four call sites; or one spec row back to the old phrase, which
criterion 1's search then finds.

**Flagged, not ruled:** whether §10.8's frozen-list sentence should say in its own words that its
parenthetical is the definition and prose inside the crate is living text — a one-line contract
amendment in the #112 idiom, Björn's to write or to refuse. This ruling and the decisions record
carry the reading until then.

---

## #503 — the frozen gate for the capture window, whether an entry may correct the ones above it, a sibling driver's fix riding here, what #525 is now, and `captured-in`

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/503#issuecomment-5578718086>

**The fork:** five, four of them raised by PR #536's own body and one by the ruling itself.
(1) The PR adds five commands on `commands::entity`, five lines at the foot of `lib.rs`'s handler
list, the DTOs `ShortcutView` and `Recorded`, the event `capture:open-note`,
`tauri-plugin-global-shortcut = "=2.3.2"` with a `.plugin(...)` in `run()`, and a second
capability file — the IPC command and event schema, both append-only barrels and a plugin's
capability, all on §10.8's list. Does the ticket authorise each touch, is the entry sufficient,
and may the merge-manager proceed? (2) The implementer wrote that the entry *"corrects two
sentences above it"*, because §10.8 is append-only and the #500 and #501 entries both say the
desktop witness has not run. May a new entry correct earlier ones — and, underneath the fork,
**there is no #500 entry in §10.8** at all. (3) The fix to #501's *merged* driver, which was
unrunnable because it read markup names where WebKit renders uppercase ones: in #536 or in a
follow-up? (4) #525's stated subject — *the first unlocked run* — is largely discharged, yet
three debts sit on it; closed, re-titled, split or held, and what does the exit wait on?
(5) `captured-in` is not asserted by the driver; owed to #525, to a new ticket, or to nothing?

**Ruling:** (1) **Authorised, and the entry is sufficient as written**; the merge-manager may
proceed once the ratification clause, this append and parts 2, 4 and 5's edits are on the branch.
The ticket names each touch in its own words, and the spec's *one* capture-window command against
the five that landed is covered by the rule recorded three times on #491 — *"the ticket is the
operative text and the spec's count is a shorthand, not a prohibition."* The entry's
flagged-for-Björn sentence gains *"— ratified in his absence by the deputy's ruling of 2026-09-08
on #503"*, the wording #496's, #498's, #505's, #506's and #508's carry. The ticket's *"asserts
through the IPC seam"* names a mechanism a shell driver does not have, and **reading the note and
its links panel off the real accessibility tree is the stronger claim**; the deviation stands,
disclosed. (2) **The mechanism is right and was already ruled; the framing and one fact are
wrong.** Björn's ruling of 2026-08-31 in the #112 section decides it — *"the old text stands and
the amendment carries the new truth"* — so #501's sentence stays, #501's entry gains one italic
pointer in the idiom this file already uses twice, and #503's paragraph is reframed from *two
sentences are now false … corrected here* to *one sentence is superseded; the new truth is here*.
**The #500 half goes**: that ruling closed by recording the frozen surface untouched, no entry was
written, and an entry naming a sentence that is not there is the class this milestone keeps
catching. (3) **The `open-in-editor` fix stays in #536.** `rendered_label` lands here anyway for
this driver's own `SHORTCUT`, `CAPTURED FROM` and `LINKED ITEMS`, so merging without it would put
a driver known to be unrunnable on `main` beside the one function that says why; the harness is
shared infrastructure already changed twice by PRs that did not own it; the old pin
(`for="clones-root">Directory<`) pinned a **representation** and stayed green while the driver
could not pass its first `fill`, which is exactly what the #500 ruling's *"a driver is not exempt
from the gate because its run is"* forbids; and `testenv/**` is on no frozen surface. Two
conditions, both other people's: the merge-manager runs `open-in-editor` **to its named refusal**,
and #531 rebases onto this merge. (4) **#525 is held under its own title and narrowed.** Two of
its three runs are discharged once the **merge-manager's** re-run transcripts of `launcher-hotkey`
and `capture` are posted on #500 and #503 — *re-run by the merge-manager* is the criterion, so
those transcripts are theirs. The third run is still owed and its blocker is a fixture, not a
lock: one new v1.5 ticket, **#537**, *the demo corpus carries repos and branches, and
`open-in-editor`'s first green run*, blocked by #503 and #531. Two debts **leave** #525:
`captured-in` (part 5) and making the harness handle Launch Services, which ADR-0016's dated
consequence already rules — a clear registration is a precondition *"a person meets once and the
harness probes on every run"*, and *"the harness never moves an installed knobas aside …; that is
the runner's job"*. Ten registrations instead of one changes the **recipe**, not the rule.
`ready-for-human` comes off #525, and the exit's desktop-witness half now waits on exactly two
numbers: **#531 and #537**. (5) **`captured-in` is not owed — to #525, to a new ticket, or to
anything.** Spec #491's stream map, row 5, allocates the witnesses — *"app IPC suite for the
links; desktop automation for the shortcut and window"* — and its *Primary seam* paragraph puts
*"the capture links (stored room, derived room, foreground present and absent)"* at *"the app's
commands over a scratch database"*. That witness is on the branch, and the driver asserting
`CAPTURED FROM` is **one link more** than that row asks of it. The three *"owed to #525"*
sentences are replaced on this branch.

**Reasoning:** (1) the freeze exists so that a wire shape, a plugin or a capability never arrives
unannounced, and each was announced in the stream map, named in the ticket's body and first
criterion, and written up where the freeze points. The smallest frozen surface that meets the
stories is what landed, and both alternatives are larger on the list: a JavaScript half for the
plugin means a `global-shortcut:*` permission in `default.json`, which is a webview able to take a
key combination from every application on the machine; widening `default.json`'s window list
instead of a second capability file hands the capture window the main window's whole grant where
it needs one line. The witness ran, with **Finder asserted frontmost and not assumed**, which is
what makes it a witness of a *global* shortcut. (2) a section append-only for its *entries* has
always taken pointers into earlier ones, and a later entry that "corrects" an unpointed earlier
one teaches the next reader of #501 something one entry out of date. (3) *a merged ticket's file
belongs to that ticket* is not a rule anywhere in the working model; the rules are the gate and
the frozen list, and this passes both. A follow-up would have left `main` carrying a driver the
harness refuses at its first field, with #525's runner told to run it. (4) the #500 ruling held
the exit because closing v1.5 with its three OS-level features never once driven would contradict
the grilling; two are now driven, and the third is the one ADR-0016 was written for. ADR-0013's
*"'awkward to reproduce' is not 'cannot produce'"* decides where the fixture-shaped blocker goes:
`fixtures/tidewater/work.json` already carries the repos and `knobas_source_mock::items` does not
send them, so a seed closes it and it is a ticket, not a gap. (5) the #502 second ruling's
standard is that a gap owes something when the unwitnessed direction is the one the feature was
cut for. That direction is spec #491's *"A thought while some other window is in front has no way
in"*, and it is witnessed. Which room a capture belongs to is a question of what the main window
recorded, which the seam answers for every case the spec lists; driving another feature's UI to
arrange this feature's fixture, with a blur race found on the first attempt, would be a flaky
witness of a thing witnessed where the spec put it.

**If you disagree, the cost of reversing this is:** (1) moderate after merge — five commands and
an event leave the barrels only by a further §10.8 entry, the plugin leaves `Cargo.toml` with its
capability file; no migration, so nothing is irreversible in a database. Before merge it is one
revert. (2) trivial, and by amendment: one paragraph and one pointer — and undoing the pointer is
a further pointer, since deleting it would itself rewrite a record. (3) trivial — thirty-three
lines in one driver revert and `rendered_label` stays for `capture.sh`; the driver is then
unrunnable again until the follow-up lands, which is the interval this declined to open.
(4) low: closing #525 on two runs is one label and one comment, and closing #537 `wontfix` costs
nothing in code — what it spends is that the ADR-0016 witness for *Open in editor* never runs
inside v1.5. (5) trivial — a follow-up ticket for the stored-room step, `testenv/**` only, at any
time; nothing on the branch forecloses it.

**Flagged, not ruled:** whether `testenv/desktop-witness/**` should be named shared infrastructure
in `docs/agents/working-model.md` — three PRs have now changed another ticket's driver or the
harness under it, each rightly, and a sentence saying so would spare the next implementer this
fork. Björn's, on his return.

---

## #503 (second ruling) — whether the merge waits for an unlocked screen, what the 04:55 transcript is worth, and the third `residue` pass

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/503#issuecomment-5579456196>

**The fork:** three, all raised by PR #536's merge-manager after the ruling above made three
desktop re-runs a condition of the merge and the screen locked again before they could be made.
(1) All three drivers refuse at step 1 — `trusted 1 / post-events 1 / screen-locked 1`, exit 1,
06:20 CEST, before the build and before Launch Services is touched. Does #536 merge now with the
re-run owed to #525, or wait for an unlocked screen; and if it merges, who produces the
transcripts part 4(a) assigned to the merge-manager? (2) The 04:55 transcript is older than the PR
body claimed: **five** commits landed under it, not two, and `b4dc71d7` moved the capture window's
`close` port from a component prop into `createCapture`'s defaults — the wiring behind the
driver's own last assertion, *knobas is frontmost again and the capture window has gone*. Does the
transcript still count, and is the rewritten provenance paragraph enough? (3) The merge-manager
added a third `residue` pass, 45 lines in a file shared by thirty-nine components, because four
`just check` runs with the leak fix reverted were all green — the fix had no witness the gate
could see. Keep or revert?

**Ruling:** (1) **#536 merges now, with its fourth criterion open and owed to #525, exactly as
#523 and #530 merged.** The answer already existed, in the ruling of 2026-09-08 on #500, part 3,
written for this ticket by number: the drivers *"proceed in number order … the merge-manager
re-running and expecting the refusal, the PR merging with that one criterion open and owed to the
same follow-up."* (a) The merge-manager's three refusals **are** the re-run, and its deep-pass
comment on #536 is the record; posting nothing on #500 or #503 was right, because a refusal is not
the transcript #525's second criterion asks for and #500's part 5 refuses *"the refusal transcript
read as a pass."* (b) The PR body's fourth box **opens**, with its three clauses beside it: green
on the dev Mac with another app frontmost, met once at 04:55 on the pre-review head; transcript in
the body, met; re-run by the merge-manager, refused and owed. (c) Part 4(a) of the first ruling is
not reversed — its condition failed, so nothing was discharged and **#525 carries all three runs
again**; the two transcripts are produced by #525's runner. (d) **`ready-for-human` goes back on
#525**, and any iteration whose probe answers `screen-locked 0` may relabel it `ready-for-agent`
and dispatch its two runs under the desktop cap, ahead of #537. (e) `Closes #503` stays; #503
closes with criterion 4 owed to #525, as #500 closed with its criterion 2.

(2) **The transcript counts as what it is and nothing more:** a green run of `capture` on the head
of 04:55, with its provenance stated. It is not a witness of the merge head and nothing on the
branch may read it as one. Three records say so: the provenance paragraph names the driver's own
assertion on the moved path and where it is next earned; the §10.8 entry's *"The desktop witness
ran and is green"* gains a sentence, because *ran and is green* without it reads as *the merged
code was driven*, a claim measuring a representation of the thing; and `testenv/README.md`'s
*What is not witnessed yet* gains the same, because #525's runner reads the README before the
entry. **No new test is owed for the moved path**: the real port is a wire, and the wire's witness
is the desktop.

(3) **The third `residue` pass stays.** The merge-manager's brief is the answer — rule 4, verify
claims rather than accept them and ask which direction is unwitnessed; rule 6, drive the fix
yourself. One sentence is owed in the PR body's *Gate* section naming the pass, its count and its
two mutants, so the squash commit carries the reason a shared test file changed.

**Reasoning:** (1) the #500 ruling chose to proceed over to wait for a reason stronger here, not
weaker: waiting holds forty files against a moving `main`, holds a Rust-agent slot on an idle
merge-manager and blocks #537, and buys nothing, because the run an unlocked screen would give is
the same run #525 makes on `main`. What #536 has that #523 and #530 did not is a green transcript
of its driver; a PR that merged on a refusal alone is the precedent. (2) an older green transcript
read as the merge head's would be a fourth item on #500's list of substitutes that discharge
nothing, but the criterion's clauses are separable and a transcript honest about its head
satisfies the clause it satisfies. The move itself is the unwitnessed-wire class recorded four
times in M2.6 — *endpoints always tested, wire always blind* — whose remedy there was to name the
direction and own it, not to hold a merge for a run nobody can make. (3) what the working model
requires of every fix is a mutation check on a committed baseline; a fix four gates cannot
distinguish from its absence has none, and merging it would have left `main` one careless `await`
from a leak the gate sees once in six runs and then calls a flake — which is how it was nearly
written off the first time. `residue.test.svelte.ts` is on no frozen surface and its thirty-nine
cases are green under the new pass; #523's and #530's merge-managers changed shared harness files
the same way, each ruled right.

**If you disagree, the cost of reversing this is:** (1) low, and it falls as #525 runs — nothing
is unmerged to reverse it; what a wait would have added is only that the merge commit and #525's
`capture` transcript were the same head, which #525's run on `main` gives anyway. (2) trivial in
code — three sentences and one clause, all prose, and the moved path is two lines that move back
in one commit. (3) trivial — one commit, one file, reverted with `git revert`; what the revert
spends is that the leak fix is guarded by nothing on the branch.

**Flagged, not ruled:** the sentence flagged in the first ruling for `docs/agents/working-model.md`
— that `testenv/desktop-witness/**` is shared infrastructure a PR may change under the gate — now
has a second file in the same position, `app/src/lib/shell/residue.test.svelte.ts`, changed rightly
by a merge-manager for a ticket that did not own it. Two instances in one milestone is a rule
waiting to be written; Björn's, on his return.

---

## #522 — the §9 record the ticket's scope sentence did not name, the workflow the measurement left behind, and a `--verify` that writes

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/522#issuecomment-5579875061>

**The headline, before the three parts:** the gap the deputy's ruling of 2026-09-08 on #498 named
is closed, and closed the way ADR-0013 asks. A `reachable_transitions` that answers the project's
statuses instead of the workflow's offer, and one that reads the transition's own `name` instead of
`to.name`, are each **survived by every narrowing assertion in the `PAY` test** and killed by the
`NARROW` one, on the real Jira, with `PAY`'s restore point byte-identical before and after.

**The fork:** three, raised by #522's implementer on PR #543. (1) `docs/contract.md` §9's #498
amendment asserts *"`GET /rest/api/2/status` — this Jira's whole status list … answers **exactly**
those four"*, which the seed's new project made false; the ticket's *Out of scope* sentence says
*"No frozen surface: this is the seed, a test file and `seed-state.json`"*, so does that sentence
bar a §9 section, is a §9 record the right shape, and does the superseded paragraph get a pointer?
(2) The template measurement created a throwaway project, walked its workflow and deleted it, and
Jira kept an inactive workflow `ZZP: Process Management Workflow` that no REST route on this
version can delete — is that leftover acceptable under *leave it as you found it*, and is a README
note, a ticket or a `seed-state.json` entry owed; and does `NARROW`, a permanent project the
fixture's corpus does not name, need recording for the next live-suite agent? (3) `--verify` now
writes one throwaway ticket, because *"this workflow still narrows"* is not a claim any read of a
project can make — is a verify step that writes to the shared fixture the right meaning of the flag?

**Ruling:** (1) **The section stays, it is the right shape, and #498's paragraph gains one italic
pointer.** The scope sentence describes the code and does not bar the record: the colon makes the
list an expansion of the frozen-surface claim, and `docs/contract.md` is on no frozen list — as
`test-inventory.txt`, also absent from that sentence, is not out of scope either. Björn's ruling of
2026-08-31 in the #112 section is the treatment — *"the old text stands and the amendment carries
the new truth"* — and the #495 and #503 rulings applied it to a §9 sentence and a §10.8 entry
respectively, the second making the pointer explicit. #498's paragraph named its own closure and
#522 is that ticket, so the pointer is in the #495 idiom at `docs/contract.md` line 1451.
(2) **Acceptable, recorded once, no ticket.** One bullet in `testenv/hetzner/README.md` under *What
is not solved*, which already carries the class; nothing in `seed-state.json`; `NARROW` needs
nothing further. (3) **Confirmed, with the write bounded as the branch bounds it**, and the three
doc sites are correct: **`--verify` writes to `jira.narrowing` and to nothing in `jira.projects`,
ever.**

**Reasoning:** (1) the file's convention exists so that a measured sentence is never silently
edited and never silently left false — the section does the first, the pointer the second. Deleting
the section would leave a binding record asserting four statuses on an instance with nine, in the
milestone that made it so; rewriting #498's paragraph in place is the wrong branch of the #112
convention, reserved for a row that was never true; a follow-up ticket for one pointer is paperwork
for a fact this PR made. The deputy verified by the anchored grep that the new section is at line
2066 under `## 9.` and before `## 10.`, that `### 10.8` is at 2300, that no word of #498's section
changed, and that the five-file diff touches nothing on §10.8's list. (2) what *leave it as you
found it* protects is what the suites assert against and spend, and all of it is unchanged: `PAY`
byte-identical, `/rest/api/2/status` the nine the new equality names, `/rest/api/2/project` exactly
`NARROW`, `OPS`, `PAY`, `project=NARROW` empty, five suites green. An inactive workflow is in no
scheme, contributes no status and appears on no endpoint any test or seed calls; reading the rule
as *no byte on the server may differ* would also forbid `NARROW`, which the ticket ordered. The
bullet is owed because a PR body is not where the next agent looks when an admin screen shows a
workflow the seed never made, and it carries the lesson — **walk a template on the project the seed
keeps, not on a throwaway**. No ticket, because there is nothing to build and the click is Björn's;
nothing in `seed-state.json`, because that file is the seed's record of what the seed made and
cannot truthfully carry a fact the seed cannot reproduce. `NARROW` is already in the three places a
live-suite agent reads — `jira.narrowing` with its `_comment`, `testenv/README.md`'s Jira section
and inventory line, and the seed's own header — and the one suite that could be surprised by a
project the corpus does not name ran green twice with it present, because it asserts on issues and
`NARROW` holds none. (3) ADR-0013's own sentence — *"a test that checks an assumption against
itself cannot fail"* — is the reason: a read-only verify would pass on an Atlassian template that
quietly went all-to-all, green and about nothing, which is the working model's *check that measures
a representation of the thing*. `NARROW`'s keys are asserted on by nothing, while a fixture
project's are reached by burning the keys in front of them, which is why the write must never move
to one. The deputy grepped the README, the Hetzner README, the justfile and `docs/agents/` for any
other sentence calling `--verify` read-only and found none.

**If you disagree, the cost of reversing this is:** (1) trivial and by amendment — the section is a
record, so an undo is a further #112-style entry rather than a deletion; the pointer is one
sentence. (2) one click in Jira's admin UI, available to anyone with the admin account at any time,
after which the bullet is struck; nothing depends on the workflow's presence or absence. (3) small —
delete one block of `verify()` and restore three sentences, and `--verify` is a read-back again;
what is lost is only the seed-side tripwire, since the live test still witnesses narrowing.

**Left to the merge-manager, not ruled:** the ticket the new test files carried no `LITTER_LABEL`
where the file's other create does, so `Env::clear_leftovers` could not find a killed run's
`NARROW` ticket and the branch relied on the adapter suite's own clearing running first in the
recipe. Acceptable as disclosed; the implementer took the should-fix, and the ticket is now
labelled from the moment it exists and the test sweeps that label itself.

---

## #533 — the measured cap, the margin principle, the glossary clause, and the second narrowed assertion

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/533#issuecomment-5579433539>

**The fork:** four, raised by PR #540 and decided by the implementer rather than left open, then
put to the gate. (1) The ticket's third criterion asks that `MAX_SAVED_LISTS` be *"confirmed or
lowered by the measurement"*. The measurement — `launcher_board` p90 at 100 k items, 41 53 55 60 81
84 132 212 ms over rails of 0 1 2 4 8 16 32 64 saved lists — puts 64 at 212 ms against a budget of
100. The implementer lowered the constant to 16 and touched nothing else that runs. Is 16 the
number, and what must the merge-manager's re-run meet for its numbers to count? (2) The margin, in
the implementer's words: *"the gate now passes at 86–90 against 100 … the first cap with real slack
is 4, at 66 ms. If you want slack over allowance, 4 is a one-line change"*, corrected after the PR
opened to a floor of 84 across four runs of three commits, with both runs of the shipped code
reading 90 at loads of 21 and of 12 and agreeing within 2 ms at every step. (3) The `CONTEXT.md`
clause: *"The ticket says 'that constant and nothing else'; I read that as a bound on code … One
revert if you read it strictly."* (4) `the_match_set_is_not_joined_row_by_row` builds over one
corpus while the launcher runs four — found, disclosed, and left.

**Ruling:** (1) **Sixteen is ratified as the measured cap.** The ticket's own sentence decides it:
the constant was not confirmed, so it is lowered, and the curve says to where. `BUDGET_MS = 100` is
untouched, which is the one thing the ticket forbade moving, and so is the summary statement's
shape, which #506's ruling named as the expensive reversal and which no ruling has authorised. 64
was wrong by a factor of **two against the budget** and of **four against the cap that fits**.
Sixteen is a reading of the launcher's own mix — `CASES`' ten shapes cycled, one `browse, no text`
among sixteen — because that is the fixture the ticket asked for; a rail of nothing but saved
browses is the rail's pathological case in the sense `PATHOLOGICAL` already has in that file, and
is not owed here. **The re-run standard**, because no reading of the shipped code has been taken on
a quiet machine: the merge-manager's re-run is the first quiet baseline, and its numbers count only
under four conditions, all stated in the merge comment — (a) nothing else on the machine **for the
whole run**, not at the instant it starts, the orchestrator holding a slot for it the way the
desktop cap holds the screen, because a watcher firing on a once-sampled one-minute average is
exactly what let a sibling in mid-run; (b) the one-minute load average **at start and at end**,
both below the machine's twelve cores; (c) a **monotonic rail curve** in both columns, which is the
test of whether the run was a reading at all; (d) the **whole transcript quoted**, every block, so
a contaminated 25 k or 50 k row is visible rather than omitted. Under those conditions a green is
the gate's green and a red is real — a red means the constant comes down by the curve, inside this
ticket, and the PR does not merge over it; a red taken outside them is not a reading and is not
quoted. (2) **Sixteen, and the principle is: the cap is the largest count at which the measured
board clears the budget on a quiet machine by more than the measurement's own quiet-machine spread.
The margin is for the clock, never for the machine's load and never for the reviewer's comfort.**
Headroom at the cap is 10 ms, run-to-run spread 2 ms, the morning's spread 6 ms; sixteen clears on
either, twenty does not (the curve affords about nineteen with no margin), eight buys 2 to 6 ms for
half the allowance and four buys 24 for a quarter of it. **If a quiet run of this code ever clears
the budget by less than its own spread, the constant steps to eight without a ruling.** On the
busy-machine worry the corrected record answers itself: load 12 and load 21 gave the same curve to
within 2 ms, so in that range load did not move the numbers, while at 22 to 30 it put a 60 ms
built-in case at 429 ms and a 50 k row at 201. **Load does not shave this file, it wrecks it** — no
cap a reader would want buys a gate that survives another agent's `just check`, so the constant is
not where that worry is answered; the method section and the re-run standard are. A thin margin is
also what a gate is *for*: at 90 a 12 % regression turns it red, at 66 the same regression is
invisible until it is a 50 % one. (3) **The clause stays, cut to the rule and made true.** The
implementer's reading is the right one: the ticket's sentence bounds what the ticket may change to
make the gate pass — the constant, never the budget, never the statement — and says nothing about
the record, which follows the change wherever the change is (#496 corrected three glossary entries
on the PR that added the reader; #506 part 2 owed one clause because a later implementer meets the
consequence before they read the migration header, and #507 restores saved lists by the schema dump
and never through `create`). Three edits: **the rule, not the reading** — at most sixteen, a
measured bound and not a feature, lowered by measurement and never raised by hand, with the
milliseconds struck, since a glossary carrying a benchmark reading is a second copy of a number
that has one home; **where the bound bites** — *Save as list* refuses when sixteen exist and
nothing else enforces it, so a restored or merged database is held to it by nothing but the archive
it came from; and **the pointer** — the clause names #533 and this ruling and says the number is
read off the rail curve, so the glossary follows the constant. (4) **Not inside #533 — the
implementer's scope call is confirmed. One follow-up ticket, filed by the orchestrator outside
v1.5**, no milestone, `needs-triage`, for Björn to place; it is **#541**. It is not #506's class:
that gap was a budget whose fixture stopped covering what the budget names, so the milestone was
shipping an unmeasured claim, where **this claim is measured** — the gate runs every case through
`Searcher::search`, which builds over `corpus::ALL` since #436, and the M1 defect this pin catches
cost 2.5× and would be red there. What is narrowed is a *detector* for one mechanism, one of a
family (`tests/sql_shape.rs` carries eleven pins of the same shape), and passing `corpus::ALL` to
it would widen it **in name only**, since three of the four branches sit at zero rows — the working
model's *check that measures a representation of the thing*. The real ticket seeds the corpora and
moves the family, which is not a #533 sentence.

**Reasoning:** the ticket was filed so the number would be *"a measured number instead of a
generous guess"*, and a measured number is whatever the curve says rather than the value nearest
the guess. The margin rule is the one the file already uses in the other direction —
`the_plan_does_not_decay_after_the_fifth_execution` keeps its coarse threshold because *"tightening
it to 1.2x would only calibrate it to one loaded machine"* — and it hands the next measurer a
number instead of a judgement call; the budget is the product's promise and the cap a bound on the
product, so a bound set for the benchmark's ease is a feature taken away for no reader's benefit,
the mirror of the file's own rule that a fixture must not be tuned until it passes. The re-run
standard is the file's method section (*"Take it on a machine that is doing nothing else"*) made
checkable. On the glossary, the reading that keeps the ticket's scope unchanged is the one under
which the record of a change is part of the change, which is how every ruling in this file has
treated it; the strict reading would leave the product's one reader-facing rule about saved lists
in a Rust doc comment and have #507 learn the cap from `create`'s error string. Part 4 follows #498
part 1, the precedent for *found, disclosed, not fixed here*, and sits outside the milestone
because the milestone's witness has no defect on this point. No ADR: a constant read off a curve is
a fact, not a decision.

**If you disagree, the cost of reversing this is:** one constant for parts 1 and 2, either way, with
no record moving but the number in `saved::MAX_SAVED_LISTS`, its doc comment and the glossary
clause — though *raising* it needs a measurement that affords more, which means a cheaper summary
statement or a budget Björn moves, neither of which a ticket in this loop may do. Three sentences
in `CONTEXT.md` and no code for part 3; striking the clause entirely is one revert and leaves the
cap enforced and unnamed. Trivial for part 4: pulling #541 into v1.5 is a milestone edit and
closing it `wontfix` is one click, and nothing in #540 forecloses either.

---

## #509 — the frozen gate for the produce command, three settled decisions, and three disclosed gaps

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/509#issuecomment-5579197449>

**The fork:** four, raised by PR #539 and flagged rather than left open — the implementer decided
each, disclosed them, and held the PR for the gate. (1) The PR adds
`produce_estate_file(app, lifecycle, producer, token: Option, land_under: Option) -> Produced` on
`commands::assets`, one line at the foot of `crates/knobas-app/src/lib.rs`'s handler list, the DTO
`Produced` (mirrored as `TokenNeeded | LandingNeeded | ProducedFile`), a field `importer: boolean`
on #508's `ImportProducer`, and `knobas_secrets::KeychainAccount` with the `importer:` namespace —
the IPC command schema and the barrel are on §10.8's list and the namespace is interfaces §3's
convention, and the entry reads *"claims no ratification"* as #508's did. (2)
`testenv/hetzner/estate.json` gained two properties per Hetzner server (`location: nbg1` and the
label `knobas: testenv`), and the producer writes hcloud's image name into the file's existing `os`
rather than a new `image` — a checked-in fixture that #510 and the live recipes read. (3) The
`KeychainAccount` newtype and the forty call sites the compiler asked for, in a crate the ticket
called *"the secrets crate's only change"*. (4) Three gaps the PR body discloses: the recording that
can never go red when Hetzner changes its JSON; the dialog's `unauthorized` branch, which no test
holds; and `produce_estate_file`'s body, not driven through `tauri::test`.

**Ruling:** (1) **Authorised, and the entry is sufficient as written; the merge-manager may
proceed** once the ratification clause, the rulings-file append and parts 4b and 4c are on the
branch. The ticket names the touch in its own words — *"a token kept in the keychain under the new
`importer:` namespace (the secrets crate's only change, §10.8 entry)"* and the fifth criterion's
*"§10.8 entries for the produce command and the namespace"* — and spec #491's stream map row 9 lists
*"secrets namespace `importer:`; one IPC produce command"* under *Frozen-surface touches*. The
`importer` flag is not in the spec's count and the fourth criterion is why it exists: a chooser that
*"asks for the token once"* has to know which of its entries owes a token, and a flag the backend
states is the reading that does not derive importer-ness from `id !== "estate_file"`. The rule
recorded four times on #491 covers the count — *"the ticket is the operative text and the spec's
count is a shorthand, not a prohibition"* — and the orchestrator posts a fifth note there. The
clause owed: the entry's *"claims no ratification"* sentence **stays** as the history it is and
gains beside it *"— ratified in his absence by the deputy's ruling of 2026-09-08 on #509"*, the
wording #496's, #498's, #503's, #505's, #506's and #508's entries carry. (2) **Confirmed on all
three — `location`, the label under its own key, and `os` — and the answer already existed.** (3)
**The newtype is right and the churn is authorised**; `crates/knobas-secrets/**` is not on §10.8's
frozen list, so this was scope and not gate, and the scope sentence is met — the crate's one change
*is* the namespace, made structural rather than placed beside itself. (4) **(a) the recording owes
nothing; (b) the `unauthorized` branch owes one vitest case inside #509; (c) the client owes one
shared constructor inside #509, and the recipe's sentence it makes true.** No glossary change and
no ADR: **Importer** already says *"its credential is its own, under the `importer:` keychain
namespace"*, and ADR-0015's consequence *"the secrets crate's key space grows one namespace"* is
what landed.

**Reasoning:** (1) the freeze exists so that a wire shape or a keychain convention never arrives
unannounced; each touch was announced in the spec's stream map, named in the ticket's body and its
first, fourth and fifth criteria, and written up in the section the freeze points at. The smallest
frozen surface that meets stories 61–65 and 70 is one command, one union and one namespace, and
that is what landed: no migration (`0026` still free and claimed by nobody), no event, no
`WriteOp`, no `Capability` — `Capability::Import` stays undeclared per ADR-0015 — nothing in
`crates/knobas-source/**` or `crates/knobas-http/**`, which knobas-app gained as a *dependency*
rather than changed. The `Option` arguments are argued against the precedent they depart from
(#508's required `producer`) and the argument holds: absence has an exact meaning here — *read the
keychain*, *nothing said yet* — and none there. (2) three sentences decide it: the third criterion's
*"the preview against the checked-in estate is all-known with **no changes**"*, spec #491 story 65's
*"with the location kept as a property"*, and `knobas_core::asset`'s `vm` type, which declares
`p("os", "OS", T)` and declares no `image` while `estate.json` has carried `"os": "ubuntu-24.04"`
since M4.0. **A producer whose files must preview with no changes against that file has no choice
of key**, or the criterion is unsatisfiable on a correct implementation — the class this milestone
named on #398. Story 63's word *image* names what is **read from hcloud**, not the property it
becomes: the same list says *IPv4* and that key has been `ip` since M4.0, which nobody would read as
a fork. The file's two additions are in the same class — hcloud reports the location and the label
for the real servers, the recipe measures the file against what hcloud reports, and a record that
omitted them was incomplete rather than bent. Bare label keys are the **smaller** change, since
`role: <product>` is already both a Hetzner label and a hand-written property, so a prefix would
have put `label_role` beside `role` on all three servers; and the refusal of a label shadowing one
of the five written keys is `DESCRIPTION_KEY`'s rule one level out. (3) on `origin/main` the
`source:` prefix was applied in **one store and not the other** — `KeyringStore::entry` called
`account_for`, `MemoryStore` keyed on the bare id — so every test running against the memory store
was already measuring a key space the real keychain does not have, and `tests/sources_crud.rs`' nine
`secrets.get(..).is_none()` assertions would each have stayed green against a key nothing was
written under: the *check that measures a representation of the thing* class, in the one place where
a false green means a credential is where the test says it is not. #498's part 3 refused a helper in
a **frozen** crate because *"additive is not exempt"* there and the entry had claimed the type
untouched; neither holds here. (4) the standard is #502's and #503's: a gap owes something when the
unwitnessed direction is the one the feature was cut for, and where it is owed follows the cost of
the witness. (a) is allocated by spec #491's witness map to `just estate-live`, which ran and came
back *"3 servers produced, all known, no changes"* — **which discharges #508's `hcloud_id` debt
exactly as part 3 of the #508 ruling said it would, and nothing more is owed on that debt
anywhere**; the four-place disclosure is what a green gate needs beside it. (b) the fourth
criterion's witness is **vitest**, not the `?fake-ipc` walk, and
`AssetsView.import.test.svelte.ts` already scripts `produceEstateFile` as a port and already rejects
the preview through a `refusal` knob — one more case there, and the branch the dialog's own header
calls *"the only way back from a credential the far end stopped accepting"* stops being a branch
nothing holds. That is mutant H's class, and mutant H is why this PR already has a tenth case.
Story 62's *asks for a token once* has a second half — what happens when the once-accepted token
stops working — and a dialog with no way back leaves the reader holding a keychain item they cannot
replace, since an importer has no sources view. (c) the body's one uncovered decision is the
**client**: `API`, `Auth::Bearer` and the `User-Agent` were built inline in `commands/assets.rs`
while `tests/estate_live.rs` built a second copy whose comment read *"a second spelling here would
be a suite certifying a client the command does not use"* — a second spelling saying it is not one,
the prose-outruns-the-code class in a test header, which is the one place this milestone has refused
to leave it. No `tauri::test` run is owed: a scratch `SourcesState` inside the mock runtime would be
a harness for one command that every other command in the crate does without.

**If you disagree, the cost of reversing this is:** (1) moderate after merge — the command leaves
the barrel only by a further §10.8 entry, and the `importer:` items a user has stored are keychain
rows nothing in knobas would delete; there is **no migration**, which is the entry's own point.
Before merge it is one revert of a branch nothing has built on, except that #510 is blocked on this
and its Docker producer is written against `produce_estate_file` and `KeychainAccount`, so every day
widens the reversal. (2) trivial in code — one line for the key, six in `estate.json` for the
properties, and mutant A is what goes red — but reversing `os` makes criterion 3 unsatisfiable until
the file changes too, and reversing the two properties makes `just estate-live` red on its next run,
since hcloud will keep reporting them. (3) low and mechanical — a `&str` returns to the trait and
the compiler walks the same forty sites back — but it reopens the two-spelling key space, and the
nine absence assertions would each need to spell their prefix by hand to keep the property the
newtype gives for free. (4) (a) nothing in code; ruling the recording a sufficient witness would be
a new exception to ADR-0013, which is Björn's sentence and not the deputy's. (b) trivial — one case
deleted; the branch keeps working and stops being held. (c) trivial — one function inlined back into
two places, after which the recipe's header has to say it is a copy.

**Flagged, not ruled:** claim-by-claim verification of the §10.8 entry against the diff is the
merge-manager's deep pass, as on the six entries before it, and this ruling does not replace it —
three claims to start with are named in the comment: that `KeychainAccount::source` spells
`source:<id>` byte-for-byte as `account_for` did (or every configured source's credential on every
existing machine becomes unreachable, silently, with the gate green), that `Produced` is decoded
nowhere, and that the share suite's secret-free scan is one that *would* carry the token if
anything wrote a `source_config`-shaped row for an importer.

---

## #509 (second ruling) — the top of the estate as a landing place

Ruled 2026-09-08, raised by PR #539's merge-manager after every condition of the first ruling was
met and verified. Comment:
<https://github.com/BFoerschner/knobas/issues/509#issuecomment-5579926452>

**The fork:** the merge-manager's words: *"`ImportDialog.svelte:154` derives `const landUnder =
$derived(crumb.at(-1)?.id ?? null);` and `:155` names that position `"the top of the estate"`. The
picker opens there … promises `They will land in <strong>the top of the estate</strong>.` with a
button reading `Put them in the top of the estate`. Pressing it calls `produceFile(producer, token,
null)` — and the backend reads `null` as nothing said yet … on an estate with no assets, every
server is new, so `landing_needed` always fires, the picker renders `Nothing inside the top of the
estate.`, and the only button does nothing — forever."* Two fixes: UI-only, disabling the button
while the crumb is empty and reading the ticket's *"the land under asset asked once per run"* as
*an asset is required*; or wire, giving the top its own spelling and changing the argument the first
ruling ratified. #510 is written against the signature. Three smaller findings came with it, and
whether the PR body is corrected before the squash.

**Ruling:** **The wire fix, inside #539, before the merge. The backend is the defect and the
dialog's promise is the true sentence.** The argument becomes `land_under: Option<Landing>` with
`Landing { parent: Option<String> }`, mirrored as `landUnder: Landing | null` and
`{ parent: string | null }`. **An absent `land_under` still means *nothing has been said yet*,
unchanged from the first ruling**; a present one carries the assets module's own spelling of a
place, `parent: null` for the top, exactly as `create_asset` and `move_asset` take it. A produced
entry landing at the top carries no `parent` key, which the file builder already does for a `None`
parent, so `produce` answers the top with the draft it has already built. Owed with it, all inside
#539: the §10.8 entry amended in place, since it is this PR's own unmerged entry; one seam case in
`assets_ipc.rs` over a scratch database holding **no assets at all**; the vitest *land under* case's
missing half, pressing the button from where the picker opens; the `?fake-ipc` handler reading the
new shape and the walk gaining one step at the top; and three mutants. The smaller findings: (a)
`residue.test.svelte.ts`' `ImportDialog` comment names a state its case does not reach — one
sentence, no new case, saying instead that the in-flight picker is `MoveDialog`'s case one entry up
on the same `latestRead` and that this case holds the idle dialog; (b) `label_collision`'s wildcard
arm goes, `OWN_KEYS` becoming pairs of the key and what the importer writes into it; (c) is the fork
itself. The PR body's counts are corrected once, after this lands, since it moves them again. No
glossary change, no ADR, no follow-up ticket, nothing newly blocked. **For the orchestrator:** #510's
note gains one line, that `produce_estate_file`'s third argument is `Option<Landing>` and Docker
sends `null`, since story 68 says *"Docker needs no land under question"*.

**Reasoning:** the answer already existed, in four places, and every one says an asset at the top of
the estate is ordinary — `ImportEntry.parent_id` (*"`null` only for an asset at the top of the
estate"*), `FileAsset.parent`, which the Import has written as a top-level asset since #439,
`create`'s and `move_to`'s own docs, and `tree.ts`'s `TOP: null`. `testenv/hetzner/estate.json` has
exactly one parentless asset, the site everything else sits under, and ADR-0014's sentence is *"the
estate is a tree"* — a tree has a top. That settles which side is wrong. Spec #491 story 69 asks
that *"an importer adds no second set of rules"* and ADR-0015 says *"the existing Import is its
preview and its apply"*; the Import accepts a parentless entry, so an importer refusing the one
answer the Import accepts would be that second rule, and the UI-only fix is that rule made visible.
The ticket's word *asset* in *"the land under asset asked once per run"* is the common answer and
not a prohibition, and story 65's *"the importer invents no site"* forbids the **importer** making
one, not a person choosing the top. **On an empty estate the top is the only choice there is**, and
spec #491's own problem statement is that case. On the first ruling's sentence: *"absence has an
exact meaning here"* is still exact, and what neither it nor the implementer saw is that the
**present** value had one spelling for two answers, and the top's spelling collided with absence.
The three smaller-looking shapes are each worse: `Option<Option<String>>` rests on the mirror
sending `undefined` for one case and `null` for the other, an invisible distinction on the wire; a
reserved string for the top is a reserved namespace, which the entry's *what is not touched* list
rules out by name; and carrying the draft inside `landing_needed` would make one state a question
and an answer at once, the class the entry gives as its reason for a union over a record of
optionals. For (a), a test header saying more than its case is the class this milestone has refused
to leave in place, and the fix is the sentence. For (b), a wildcard arm that describes a key it has
never seen is ADR-0006's reason for refusing a wildcard `WriteOp` arm, one level down.

**If you disagree, the cost of reversing this is:** trivial before merge — one struct and its
mirror, two seam calls, one vitest half. After merge, `Landing` leaves the wire only by a further
§10.8 entry, and there is **no migration and nothing persisted**, as the entry says. Reversing to
the UI-only fix is one file, but it reinstates the empty-estate dead end and a rule the Import does
not have, and the glossary's *"the existing Import is its preview and its apply"* would then need a
clause saying the importer refuses what the Import accepts, which is Björn's sentence and not the
deputy's. (a) and (b): nil either way.

**Flagged, not ruled:** nothing. The first ruling is not reopened, and the merge-manager's deep pass
over the §10.8 entry's claims stands as that ruling left it.
## #507 — the frozen gate for `smart_lists` on `ShareParts`, the criterion only the dialog can deliver, two decisions taken rather than forked, and the walk that cannot reach the dialog

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/507#issuecomment-5579888742>

Everything the ruling quotes as record was read off `origin/main` at `a1e6f316`, this branch's base,
and not off `issue-533` or `v1-5-glossary-and-adrs`: on `main` `MAX_SAVED_LISTS` is 64, the **Smart
list** entry ends with #506's amendment and carries no #533 clause, and this file has no `## #533`.
PR #542's correction of its own transcription error therefore holds — its three sentences name #533
and #540 as **open**.

**The fork:** PR #542 raised four, and raised them by deciding and disclosing rather than by
stopping. (1) It adds `smart_lists: bool` (default true) to `backup::share::ShareParts` — the
argument DTO of `share_export` — mirrored as one field on `interface ShareParts` and one key on
`shareDefaults` in `app/src/lib/ipc/backup.ts`, with `tables()` pushing `smart_list` when it is on.
The §10.8 entry says in its own words that no ruling had been posted when it was written, the shape
#508's entry carried before its clause. Does the ticket authorise the touch, is a field on
`ShareParts` the same class as #506's two on `SmartListSummary`, and in what wording does the
ratification go? (2) Criterion 1 reads *"with no saved list the dialog shows no toggle and the
archive carries no list table"*, and `pg_dump` names a table whether or not there are rows in it —
so the second clause is deliverable only by the dialog **sending the part off**, which `openShare`
does. Is that the criterion's reading, and is more owed? (3) Two decisions taken rather than
forked: an archive past the cap restores whole, and nothing upgrades a stored query. (4) One gap
disclosed: no `?fake-ipc` walk can reach the *Share…* dialog, because the fixture answers no backup
command and the section is behind `backup_status`.

**Ruling:** (1) **Authorised, and the entry is sufficient as written; the merge-manager may
proceed** once the ratification clause and this append are on the branch and claim 1 below is
settled. The ticket names the touch **by the field's own name** — body, *"The share export gains a
`smart_lists` part, on by default"*; criterion 1, *"The share export's parts gain `smart_lists`"*;
criterion 2, *"the §10.8 entry for the part is written"* — and three more records name it: spec
#491's stream map row 8 (*"the share-export part"*), the grilling, and `CONTEXT.md`'s **Share
export** entry, which names it by ticket number (*"the toggle, with the archive's `smart_lists`
part behind it, is **#507**'s"*). It is #409's class (*"a wire type changing shape is the thing
this section exists to record"*), and **the contract wrote this field's decode rule before the
field existed**: #454's entry put `#[serde(default)]` on the struct *"so a knobas built before a
later part still decodes a payload naming it"* — a later part is this part. The clause owed: the
flag sentence stays as the history it is and gains beside it *"— ratified in his absence by the
deputy's ruling of 2026-09-08 on #507"* with the comment URL and this file. Four claims the
merge-manager starts with, all four settled on the branch: **the six-key decode is now pinned by
name** in `a_partial_payload_decodes_onto_the_defaults` (the ruling found the entry speaking where
no test agreed — #506's claim 1 in a new place); the three hand copies of `shareDefaults` **could
not** import it, since each factory replaces the module the constant lives in, and each now carries
that reason and points at the Rust-side pin; `SettingsView.test` and `residue.test` reach the real
`../ipc/search` and neither asserts on the toggle nor defers that read; and the anchored header
grep is re-run on the merge commit. The implementer's choice to read the launcher's own rail for
the one bit the dialog needs, rather than add a boolean to `BackupStatus`, is **ratified as the
smaller frozen surface**. (2) **Confirmed, and nothing more is owed** than the test and the
corrected doc comment. The answer was already in the glossary: *"`pg_dump` restricts by table and
never by row (#454)"* — by table means by argument list, the argument list is the parts, and the
parts are the dialog's to send. **A backend guard that stripped or refused `smart_lists` over an
empty table must not be added**: it would make an archive's contents depend on its data, contradict
that sentence, and make *off* and *on-and-empty* the same archive, so the seam test could not
exist. (3) **Both confirmed, both already decided, and nothing is added to #533's clause.** A share
archive is **not a second way** a database arrives over the cap: the Share export entry says *"the
ordinary restore reads it"*, and #533's clause already says *"restored or **merged**"* — the first
word covers this ticket. Both sentences stay whichever of #540 and #542 lands second, in either
order, and #542's *"unmerged as this was written"* is history the later merge does not rewrite. On
the stored query, reading the migration rule at the two other places stored text passes through is
not a new rule. **No ADR**, for #506's reason: a fourth record would be a record of a record.
(4) **Nothing inside #507.** One follow-up ticket, filed by the orchestrator, **outside v1.5**,
`needs-triage`, no milestone, beside #529.

**Reasoning:** (1) the freeze exists so that a wire shape never arrives unannounced, and this one
was announced in #454's entry before the field existed, in `CONTEXT.md` twice, in the spec's stream
map, and in the ticket's body and both criteria. The smallest reading is what landed: one boolean
on the parts DTO that already exists, one table name on its list, no new command, no migration, no
bit on `BackupStatus`. (2) ADR-0013's rule for a premise about a tool is to drive the tool, and the
implementer did; mutant 8 is why the test is believed rather than the sentence, since seeding a
corpus first kills it. A criterion met by a mechanism its author did not picture is not a criterion
that cannot pass, so this is a confirmed reading and not a ruled gap. (3) the alternative to *every
row arrives* is a restore that loses rows and chooses which, on the machine least able to notice,
and the working model has no class that permits it; #533 has already ruled where the bound bites,
and a rail over the cap after a restore costs what that curve says, which is the cap's meaning as
ruled on #506 — *"a bound on the board's cost rather than a property of the schema"* — with the
remedy the glossary already offers. (4) the reading that keeps the witness real refuses to invent a
walk where the seam already witnesses: a fake `share_export` would certify nothing about an archive
and would be a second copy of `ShareParts::tables()`, the copy that drifts. #496 part 3's condition
is that the ticket have a seam suite behind the sentence; #507 has one and no sentence to
substitute, so the substitution neither applies nor is missed. The hole is nonetheless real and
unrecorded, which is #529's class and why the ticket is filed rather than dropped.

**If you disagree, the cost of reversing this is:** (1) small before merge, one revert of a branch
nothing has built on; moderate after — the field leaves `ShareParts` only by a further §10.8 entry,
and every share taken meanwhile carries a `smart_list` table, though restoring such an archive into
a build without the part is the partial-archive case the restore already accepts, so no archive is
stranded. (2) a few lines in `share_export` to count rows before building the argument list, plus
the Share export entry's *"never by row"* sentence, plus a seam test whose premise would be gone.
(3) for the cap, one branch in `restore` or `share_export` and a choice of which rows to drop that
nobody has written down; for the stored query, #506 part 2's cost — high, a column's meaning on a
migrated table, and story 60 leaving the product. (4) nothing in code: pulling the ticket into v1.5
is a milestone edit and closing it `wontfix` is one click.

**For the orchestrator:** the part-4 ticket is filed as **#544** (`needs-triage`, no milestone,
beside #529): the settings' Backup section — and with it the *Share…* dialog, the restore and the
schedule editor — is unreachable under `?fake-ipc`, because the fixture answers no backup command
and the section draws both buttons behind `backup_status`. The open question is whether the fixture
should answer backup at all (a fixture `share_export` returning a table list is a second copy of
`ShareParts::tables()` and would need #496 condition 2's fixture-only marker on every handler), or
whether `docs/agents/working-model.md` should name the sections the walk cannot reach so nobody
writes a criterion that assumes it can. Either way the dialog's witness stays
`BackupSection.test.svelte.ts` and the archive's stays `backup_ipc.rs`.

## #510 — the frozen gate for the Docker arm, the fork taken as route 1, the field-name revert and the module move, and four disclosed gaps

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/510#issuecomment-5581090073>

Four parts on #510 / PR #545 — the frozen-surface gate for the `Importer::Docker` arm, the third
`PRODUCERS` row and the one new field on `Produced`; the fork named on this ticket before it
started, taken as route 1; the field-name revert and the `Produced` module move; and four
disclosed gaps plus one claim about the tunnel. The implementer raised no fork; it decided,
disclosed, and held the PR for this gate, which is what the entry's *"claims no ratification"*
asks for. `just estate-live` came back *"8 containers produced over 4 engines, all known, no
changes"* beside *"3 servers produced, all known, no changes"*, which is the criterion this ticket
turns on and the Docker half of the witness ADR-0015 books (*"a file generated from the live
hcloud or Docker host previews as all-known against `testenv/hetzner/estate.json`"*). Björn
reverses any part with a follow-up ticket; the PR that acts on this appends it to
`docs/decisions/2026-09-v1-5-unattended-rulings.md` in the same four-part shape.

Owed inside #510, on PR #545: the ratification clause (part 1); two corrected sentences and one
word (part 4b); the PR body corrected to match before the squash. For the orchestrator: a sixth
spec-count note on #491 (part 1). No new glossary entry and no ADR: the **Origin key** amendment
already on the branch is the glossary record and stays as written (part 2).

---

### 1. The frozen-surface gate: a second arm on one command, a third producer row, one new field

**The fork:** PR #545 adds no command, no barrel line, no migration and no event. It adds `Docker`
to `assets::Importer`, the enum `produce_estate_file` (#509) dispatches on; a third row to
`assets::PRODUCERS` — `docker`, origin key `["docker_context", "container_name"]` — which crosses
the bridge as the `producer` string and as an `IMPORT_PRODUCERS` chooser entry; and one field,
`Ready.skipped: Vec<String>`, on `Produced`, mirrored in `app/src/lib/ipc/assets.ts`. The entry
sits at the foot of §10.8 and reads *"no ruling had been posted on #510 when this entry was
written, so this sentence records the flag and claims no ratification"*, as #508's and #509's did.
Spec #491's stream map row 10 counts *"one IPC produce command"* and none landed. Does the ticket
authorise the touch, is the entry what the contract requires, and may the merge-manager proceed?

**Ruling:** **Authorised, and the entry is sufficient as written. The merge-manager may proceed**
once the ratification clause and the rulings-file append are on the branch and part 4b's
corrections are in.

The ticket names the touch in its own words. Body: *"Same file shape, same preview and apply, same
download"* — the same command, then, and not a second one. Fourth criterion: *"§10.8 entry for the
produce command"* — the definite article, one command. Second criterion, which is what authorises
the new field: *"an engine without the property is skipped and **named in the outcome**"* — an
outcome that names a skipped engine is a field on the answer, and `Produced` is the answer. Spec
#491, story 61: *"a chooser: an estate file, hcloud, or a Docker host"*, which is the third row;
Implementation Decisions: *"App-side producers in the assets module … each returns an estate
file's text in the checked-in shape, and **the existing** preview and apply commands consume it.
The Import dialog's chooser selects the producer … Docker is read by spawning the docker CLI under
the context named by the engine asset's property"*. On the count: row 10's *"one IPC produce
command"* is the same command row 9 counted, so the spec is over by one here where it was short by
one on #506 and #508. The rule recorded five times on #491 reads the same in both directions —
*"The ticket is the operative text and the spec's count is a shorthand, not a prohibition"* — and
the orchestrator posts a sixth note there in the shape of the five.

What I checked, because the gate is a gate. The 17-file diff against `859e4027` has nothing under
`crates/knobas-db/migrations/**`, `crates/knobas-source/**`, `crates/knobas-http/**` or
`crates/knobas-secrets/**`; `crates/knobas-app/src/lib.rs`, `error.rs`, `profile.rs` and
`app/src/lib/ipc/index.ts` are absent from it, so the barrel is the #509 list and
`every_command_is_in_the_handler_list` sees what it saw. `0025_the_search_a_reader_saved.sql` is
the last migration on `origin/main`, so *"`0026` is still the next free number"* is true today and
the entry claims none. The entry is at line 9181 on the branch and the nearest `^#` above it is
`2308: ### 10.8 The M1 contract is frozen`. `Produced` derives `Serialize` and not `Deserialize`,
so `skipped` owes no `default`, as the entry says;
`every_produced_state_matches_its_typescript_mirror` lists `Ready` as `["state", "file",
"new_servers", "skipped"]`. `testenv/hetzner/estate.json` gains exactly nineteen lines — two per
container over nine containers, and one on `asset:orbstack-docker` — which is what the entry says
it gains.

The clause owed: the entry's *"claims no ratification"* sentence stays as the history it is, and
gains, beside it, *"— ratified in his absence by the deputy's ruling of 2026-09-08 on #510
(`docs/decisions/2026-09-v1-5-unattended-rulings.md`)"*, the wording #496's, #498's, #503's,
#505's, #506's, #507's, #508's and #509's entries carry.

Claim-by-claim verification of the entry against the diff is the merge-manager's deep pass, as on
those eight, and this ruling does not replace it. Three claims to start with, because each is the
kind a reader takes on trust:

1. **"The wire is unchanged by the move"** and **"Nothing decodes `Produced`"**, now with a field
   more. The #509 ruling's grep — `Produced`, `ProducedFile`, `LandingNeeded`, `TokenNeeded`, now
   also `skipped` — outside `assets/mod.rs`, `assets/hcloud.rs`, `assets/docker.rs`,
   `commands/assets.rs`, `ipc/assets.ts`, `ImportDialog.svelte` and the test files; a hit in
   `backup/`, `settings` or `fake-tauri.ts`'s persisted state is the sentence going false. And the
   revert's residue: the diff is read for `new_assets` and for a `landing_needed` shape spelling
   `assets` in the mirror, `fake-tauri.ts`, the Svelte and the vitest file — a rename reverted in
   Rust and left in one TypeScript spelling is green in `cargo test` and wrong at runtime. The
   moved type's attributes are byte-for-byte `origin/main`'s `hcloud.rs:197` — `tag = "state"`,
   `rename_all = "snake_case"`, `Serialize` only.
2. **"`IMPORT_PRODUCERS` grows the matching chooser entry"** and the fixture.
   `the_chooser_offers_producers_this_build_knows` parses the list out of the mirror as text, from
   `= [` to the first `]` (#508's third claim); it now reads three entries and each `importer`
   flag. The `?fake-ipc` handler `dockerProduce` carries the fixture-only marker #496's second
   condition requires — its header says *"there is no CLI here and no match"* — and the
   merge-manager confirms it is in the handler and not only the PR body, and that the skipped
   engine it answers with is one the fixture's own estate holds without a `docker_context`.
3. **The estate-file gate.** `every_container_carries_the_context_and_the_name_it_is_matched_on`
   is what makes *"a key part that repeats a column"* safe to say; mutants 5 and 6 show a *wrong*
   value is red. The merge-manager satisfies itself that an *absent* pair is red too — a tenth
   container added by hand with neither property — and that the test reaches every container under
   an engine, not only entries whose parent is directly an engine. A file-level gate that a hand
   edit can slip under is #237's class: a fixture claim that has to be traced through the rule.

Plus the live re-run the ticket asks for: `just estate-live` on the merge commit, both halves,
with `HETZNER_API_TOKEN` in the repo-root `.env`, the docker CLI on the PATH, and the tunnel left
as found — up — per part 4e. Beside it, one read-only `docker --context orbstack ps -a`, for part
4b.

**Reasoning:** the freeze exists so that a wire shape never arrives unannounced; this one was
announced by the spec's story 61 and its Implementation Decisions, named in the ticket's body and
its second and fourth criteria, and written up in the section the freeze points at. The smallest
frozen surface that meets stories 66–68 is one arm on an existing enum, one row, and one field,
and that is what landed. Each way of avoiding the field is larger or contradicts the ticket:
writing the skip into the file changes the estate-file schema (`deny_unknown_fields`,
`ASSET_KEYS`, and the seam ADR-0015 says three readers share) and contradicts *"in the checked-in
shape"*; skipping silently makes an engine nobody read indistinguishable from an engine holding
nothing, which criterion 2 forbids in as many words; refusing the run on an engine without the
property makes the notebook's `asset:orbstack-docker` — which had none until this PR — a red
import rather than a named skip, and the ticket says *skipped*. `skipped` is `[]` for hcloud by
construction, which the entry says and the hcloud seam test asserts, honestly labelled as a
statement about the arm. The arm on `Importer` with no wildcard is ADR-0006's rule one level over,
as #509's entry recorded; adding the variant stopped `commands::assets` compiling until the arm
said what running it meant, which is the point.

**If you disagree, the cost of reversing this is:** moderate after merge and not growing — the
arm, the row and the field leave the wire only by a further §10.8 entry, there is no migration and
nothing is persisted, and this is the milestone's last build ticket, so nothing downstream is
written against it. Before merge it is one revert of a branch nothing has built on.

---

### 2. The fork named before it was met, taken as route 1; the **Origin key** amendment

**The fork:** the comment of 2026-09-08 on this ticket named two routes — (1) *"write
`docker_context` and the container's name as properties on every container entry — in the produced
file and in `testenv/hetzner/estate.json`"*, which *"fits the mechanism unchanged and is what
`CONTEXT.md`'s Origin key already says"*; (2) *"change `Producer` to read a key across a parent's
property or an entry's field … raise it as a `**Fork:**`"*. The implementer took route 1, raised
nothing, added `docker_context: "orbstack"` to `asset:orbstack-docker`, which had none, and
amended **Origin key** with a dated sentence: *"every part of a key is a property, including one
that repeats a column."* Confirm the reading, and say whether the amendment is the right shape and
scope.

**Ruling:** **Confirmed, and it was not a fork to raise: the comment said which route needed one,
and it was the other.** Route 1 is the route *"the record points at"*, in the comment's own words,
and the implementer's PR body quotes the sentence it rests on. The property on
`asset:orbstack-docker` is authorised by the ticket's third criterion in its own words — *"the
three servers' contexts **and the notebook's** produce a file that previews all-known"* — and the
body's rule, *"for every engine asset in the tree that carries a `docker_context` property"*: an
engine without the property is skipped by criterion 2, so a notebook engine without it could not
produce and criterion 3 would be unsatisfiable on a correct implementation. The reading is also
the one criterion 2 rewards: *"a recreated container (new id, same name) matches its asset"* is
where the two-part key first does visible work, and `a_recreated_container_keeps_its_asset` — the
produced file byte-identical across a change of every id — is its strongest form.

**The amendment is the right shape and the right scope, and stays as written.** Shape: it keeps
the ruled sentence and adds a dated, ticket-numbered clause after it, the treatment **Route** got
on 2026-09-06 (#432) and **Live item** on 2026-09-07 (#448); it names the alternative and why it
was not taken, in the comment's own terms (*"a second matching rule inside the second matching
rule"*); and it names the gate that keeps the repeat honest, the way **Live item** names its
migrations. Scope: it says one thing the entry did not yet say — that a part which duplicates a
column is still a property — and nothing about how keys are declared or matched that the entry and
ADR-0015 do not already say. No ADR: nothing architectural was decided; a consequence of #508's
mechanism was written down where the next editor of the file will read it, and the rulings-file
section this PR appends is its record.

**Reasoning:** the reading that keeps the witness real in both directions is the one where the
file is the record of the estate and the recipe holds the two to each other — ADR-0015's
consequence, *"a disagreement between the two is a red test, not a judgement call"*. Route 2 would
have made the container's key a computed thing the file never carries, so a hand edit renaming a
container could not be caught in `just check` at all; route 1 makes it nineteen lines in a
property bag with no schema and no count, and gives them the file-level gate the three
`hcloud_id`s structurally cannot have. The precedent is #508 part 2 and #509 part 2: a decision
the criterion already forces is confirmed, not re-decided.

**If you disagree, the cost of reversing this is:** trivial in code — nineteen lines in
`estate.json`, two keys in the producer, one test — but a reversal to route 2 is a change to the
matching rule, and `CONTEXT.md`'s *"the second matching rule beside the id, and the only one"* is
Björn's sentence and not the deputy's; the amendment would then be struck, not rewritten.

---

### 3. The field-name revert, and the `Produced` module move

**The fork:** the implementer's words: *"I renamed `Produced`'s `new_servers`→`new_assets` and
`servers`→`assets`; the Spec review called it over-reach and I agree — #509's §10.8 entry declared
those names a day earlier and a deputy ratified them. The dialog's rendered sentence is fixed
instead, and the §10.8 entry records the revert and why. `Produced`'s **module** move to `assets`
stays (path only, wire identical)."* Was the revert right, and is the move inside #510's scope?

**Ruling:** **(a) The revert was right, and no follow-up ticket is owed for the rename.** #509's
entry declares the names — *"`landing_needed { servers }` and `ready { file, new_servers }`"* —
and the ruling of 2026-09-08 on #509 (part 1) ratified that entry. #510's body asks for no rename:
*"Same file shape, same preview and apply, same download."* A frozen wire field's name changes by
a §10.8 conversation of its own, and the standard the implementer applied is the one comment 1 on
this ticket set for the matching rule — settled by a ruling, not re-decided inside a ticket that
did not ask. What *was* wrong was the reader's sentence, *"1 server is new"* over a list of
containers, and that is fixed; a rendered sentence is nobody's frozen surface. The name is now
recorded as reading narrow in three places — `Produced`'s header, the mirror, the entry — so the
next reader does not take it for a claim. Not a ticket: `Produced` is decoded nowhere and has one
caller, and the rename rides on the next entry that changes its shape for a reason of its own (a
third producer, or the day something decodes it); Björn files it sooner if he wants the name
sooner.

**(b) The move is inside #510's scope, and the entry records it in the right shape.**
`crates/knobas-app/src/assets/**` is not on §10.8's list — the five items are the migrations,
`crates/knobas-source/src/**`, the IPC schema and barrels, `crates/knobas-http/**` and
`error.rs`/`profile.rs` (#509 part 3) — so the question is scope, not the gate. The wire is the
freeze, and the mirror test pins it unchanged: same tag, same three arms, same fields. §10.8
spells a Rust path only as the signature's spelling; #510's entry says the path moved and the wire
did not, and #509's `assets::hcloud::Produced` stays as written beside it, the supersession
treatment. It is #510's to make because #510 is the ticket that gives `Produced` its second
constructor: `origin/main`'s own `assets/mod.rs` (line 4052) already argues the case for `Token` —
*"Beside `Producer` rather than inside `hcloud`, because nothing here names a live system: the
second importer would otherwise import its credential handling from a module named after the
first"* — and a type every producer answers with is that case exactly. The #509 ruling's part 4c
ratified the same kind of move for `token_for` and `remember` as *"the deep-module move the
implementer already made"*.

**Reasoning:** the two halves are decided by the same test, whether a sentence is true. The
dialog's sentence was false and is now true; the field's name was never false on the wire, only
narrow, and a narrow name that says so is the cheaper of the two honest states. The move changes
no sentence anyone reads across the bridge and makes one inside the crate true — that `docker.rs`
does not `use super::hcloud::Produced`.

**If you disagree, the cost of reversing this is:** (a) nil now; the rename is one line in Rust,
one in the mirror, one in the dialog, and a §10.8 entry, whenever it is wanted. (b) trivial — the
type back into `hcloud.rs` with a `pub use`, nothing on the wire either way.

---

### 4. Four disclosed gaps, and the tunnel

**The fork:** the implementer's words. (a) *"The stub-driven suite is `#[cfg(unix)]` … On Windows
criterion 2 is unwitnessed."* (b) *"`docker ps` and not `docker ps -a` has no test of its own … no
fixture here holds a stopped container, so nothing would go red if it changed to `-a` until
`estate-live` met a stopped one-shot container on a real engine."* (c) *"The applied-asset half of
the landing test is not separately mutated … Isolating that would mean mutating `apply_import`,
which both producers and the hand-picked estate file share."* (d) *"The command's Docker arm is
witnessed only where it can be reached without a docker … The dispatch into
`assets::docker::produce`, and `land_under` being ignored on that arm, have no test above the
module — the same shape #509 left the hcloud arm in."* And (e), a claim about a witness rather
than a gap in one: *"'through the tunnel' is the ticket's phrase and is not what the contexts use
— they are `ssh://` to the servers' own addresses; I say so as a reading of what the forwards
carry, not as a run with the tunnel down, which nobody made."* Inside #510, a follow-up ticket, or
nothing; and is the tunnel reading right, does the phrase need correcting, is anything owed to
witness it?

**Ruling:** **(a) Nothing. (b) Nothing as a test; two sentences and one word corrected inside
#510. (c) Nothing. (d) Nothing. (e) The reading is right; nothing is corrected and nothing is owed
to witness it.**

**(a)** The answer exists. Spec #491's Out of Scope list carries *"Witnessing on Linux or
Windows"*; the grilling ruled *"macOS is the witnessed platform; others get command templates and
no witness"*; the ruling of 2026-09-08 on #501 applied both to a criterion in exactly this
position. `tests/checkout_ipc.rs` on `origin/main` (PR #519, lines 668–712) is the same
`#[cfg(unix)]` arrangement for the same reason, and the entry says which half runs everywhere —
the parse and the argument rule are not gated. Not a ticket: a Windows witness is the
milestone-scale question the spec struck, not this ticket's.

**(b)** The decision is pinned twice already, and the implementer undercounted its own witnesses.
In `just check`: `the_context_is_the_only_thing_substituted_and_it_is_one_argument` asserts the
literal `["--context", "knobas-jira", "ps", "--format", "json"]` (`docker.rs` 520–522), so `-a` is
red there. Live, and this is ADR-0013's first class — *"if the real instance can be driven into
the fault by any seed, flag or clock, it is witnessed live"* — the instance is already in the
state: my read-only `docker --context orbstack ps -a` today lists `knobas-teamcity` (`Exited (0) 2
days ago`) and `knobas-teamcity-agent` (`Exited (143) 2 days ago`) on the notebook, the local
`real-teamcity` profile, which `testenv/hetzner/estate.json` records only under
`asset:hetzner-teamcity-docker` with `docker_context: knobas-teamcity`. Under `-a` those two would
carry the key (`orbstack`, `knobas-teamcity`), match nothing, and preview as *new* — `just
estate-live` is red on `-a` on its next run. So nothing is owed as a test. What is wrong is the
reason on record: `kuma-seed` is run by `testenv/seed-kuma.sh` line 38 with `docker compose
--profile seed run --rm`, so it leaves no exited container behind and is not what `-a` would show;
and the PR body's *"nothing would go red … until `estate-live` met a stopped one-shot container on
a real engine"* is false today. Owed inside #510: `assets::docker`'s header sentence on `docker
ps`, the README's *"`kuma-seed` is the reason"* paragraph and the PR body name the exited
containers the notebook's engine actually holds as the reason and as the live witness, and drop
`kuma-seed` as the example or say it is `--rm`. Same class as #509 part 4c and #496 part 1 — prose
outrunning what the tool does — and the memory's own line, *a stub can be wrong about the tool*.
The one word: the README's *"`asset:knobas-mockd` is stopped"* — no container by that name exists
on the notebook today, stopped or otherwise; *not running* is the true word, and the recipe's
silence is the same either way.

**(c)** Nothing. The precedent is #508 part 3a: a mutant that cannot be isolated without deleting
shared machinery is read, not reported, and the reading is where the next person will find it —
here the PR body and the test's comment. `apply_import` is #439's and #508's, pinned by their own
suites; the assertion is in place, it reads the origin key off the applied asset rather than the
file (the Standards review's finding), and mutant 2 kills the producer's half. The other half is
the Import's, and an import that dropped a property on apply is red in `assets_ipc.rs` already.

**(d)** Nothing; the answer is #509 part 4c in as many words: *"As a witness question it owes
nothing: the spec's seam paragraph puts the importer's witness at `assets::hcloud::produce` over a
scratch database … and the wiring is held by
`every_asset_command_is_registered_and_its_arguments_decode` and the three mirror tests — the same
arrangement #505 and #506 ratified."* The one thing that ruling did find owed — a client built
inline in the command and spelled a second time in the recipe — has no Docker analogue: the
command and the recipe both call `assets::docker::cli(assets::docker::PROGRAM)`
(`commands/assets.rs` 666, `estate_live.rs` 252), so the live run drives the spawn the command
uses. The token refusal is the one decision the arm makes and it is witnessed (mutant 8);
`land_under` is ignored by not being read, and a value nothing reads has no test to write.

**(e)** The reading is right, and I confirmed it read-only rather than taking it from the body:
`docker context ls` shows the three contexts as `ssh://knobas-confluence`, `ssh://knobas-jira`,
`ssh://knobas-teamcity`; `ssh -G` on each resolves to `46.224.125.111`, `46.224.117.158` and
`2.28.72.189`, port 22, with no `ProxyJump` and no `ProxyCommand`; `testenv/hetzner/tunnel` lines
52–54 start `ssh -N … -L 127.0.0.1:$port:127.0.0.1:$port $rflag $rval knobas-$role` and nothing
else. A `docker --context knobas-jira ps` is an ssh session of its own to the server's address,
and no forward is in its path. **The phrase is corrected nowhere.** *"Through the tunnel"* in the
ticket's body and criterion 3, and in spec row 10, names the session's arrangement the grilling
ordered for streams 9 and 10 — *"need `source testenv/hetzner/env` and `tunnel up` and go last"* —
the way story 63's *"image"* named what is read and not the key it becomes (#509 part 2); the
ticket is left as written, the treatment #491 got on #508 part 1. The three documents on the
branch say it as a reading and say no run was made, which is the honest shape and stays. **Nothing
is owed to witness it.** The standard (#502's second ruling, #503 part 5, #509 part 4) is that a
gap owes a witness when the unwitnessed direction is one the feature was cut for; no criterion,
story or ADR asks the importer to run with the tunnel down, and the sentence exists so the next
reader does not start a tunnel they do not need. The merge-manager re-runs the recipe as the
criterion says, with the tunnel up and left as found, and does not take the shared tunnel down to
prove a README sentence: it carries TeamCity's `-R` forward, which other suites spend.

**Reasoning:** the standard is the one #502's second ruling set and #503's part 5 and #509's part
4 applied: a gap owes something when the unwitnessed direction is the one the feature was cut for,
and where it is owed follows the cost of the witness. (a) is allocated by the spec and struck by
it. (b) is witnessed twice and mis-described once; the fix is the sentence, which is the one place
this milestone has refused to leave prose ahead of the code. (c) and (d) are precedents applied.
(e) is a claim that is true, labelled as a reading, and costs nothing to leave labelled that way.

**If you disagree, the cost of reversing this is:** (a) nil in code — a Windows witness is a
ticket of its own, and the spec's Out of Scope line would be Björn's to strike. (b) two sentences
either way; a test that pins `ps` over `-a` at the seam would be one stub answer with an exited
container, an afternoon, and would certify what the literal argv assertion already does. (c) nil.
(d) nil. (e) nil — a `docker --context knobas-jira ps` with the tunnel down is a ten-second
measurement whenever Björn wants the sentence to be one, taken when no live suite is running.

---

**Record-keeping:** the PR appends this as `## #510` in
`docs/decisions/2026-09-v1-5-unattended-rulings.md`, after `## #507` (the last section on
`origin/main` today) or after whatever section is last on the merge commit. The §10.8 clause of
part 1, the two sentences and one word of part 4b, the PR body corrected to match before the
squash (#509's second ruling, part 3: the body is the squash message), and the append are the
edits to the branch this ruling asks for. **For the orchestrator:** post the sixth spec-count note
on #491 (part 1); brief #545's merge-manager that those edits are conditions of the merge, that
the three claims in part 1 are read before the deep pass, that `just estate-live` is re-run on the
merge commit with the tunnel as found and `docker --context orbstack ps -a` read beside it, and
that no file in `testenv/hetzner/` is touched by the re-run. No new glossary entry, no ADR.

---

## #537 — whether the last desktop ticket's code half may be built on a locked screen, and what the loop does when nothing is left to move

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/537#issuecomment-5580568595>

**The fork:** four, raised by the orchestrator before any dispatch. In its words: *"may #537's code
half be built now, with its green run left open and owed to #525 — the treatment #500, #501 and
#503 each got — or must the whole ticket wait for an unlocked screen?"* Then: what its
merge-manager does about the run; what the v1.5 exit is if the screen stays locked for the rest of
Björn's absence; and what #525 is owed. The state: twenty-two v1.5 tickets closed, no PR open, #525
`ready-for-human`, #537 `ready-for-agent` with both blockers closed, the v1.0.0 release run green.
The dev Mac's screen has been locked since 06:10 CEST, probed each iteration as `trusted 1 /
post-events 1 / screen-locked 1`; it was unlocked once around 04:55 without anyone being asked.

**Ruling:** (1) **#537 is dispatched now to build its code half, and merges with its run-criterion
open and owed to #525. No fourth run is added to #525** — the run #537 would leave open is the one
#525's second criterion already names. The answer existed: the 2026-09-08 ruling on #500, part 3,
set the shape for every desktop ticket — *"the driver written and asserting what the ticket says,
the `screen-locked` refusal in the PR body as the run … the PR merging with that one criterion open
and owed to the same follow-up. Every other criterion on those tickets … is real and gated and is
held in full."* The lock is not a cap: it blocks a run, never a build, and the desktop slot is free.
`ready-for-human` was put on #525 for a stated reason that does not reach #537, which has work an
agent does before any refusal. The mock adapter is the one fake ADR-0013 keeps, so its tests are a
real witness of its corpus and criteria 1 and 2 are held by the gate. Criterion 1 is read as: *a
synced `--demo` corpus yields a repo entity that the entity read returns under the name the driver
asks for (`payout-service`)*, not merely that `items` returns more elements than before.

(2) **The merge-manager re-runs the driver and expects the refusal.** The refusal transcript goes in
the PR body; **nothing is posted on #501 for a refusal** (#503's second ruling, part 1(a): *"a
refusal is not the transcript #525's second criterion asks for"*); the third box reads `[ ]` with
its three clauses beside it. Four edits are conditions of the merge, on the branch: `testenv/README.md`'s
*What is not witnessed yet* paragraph rewritten (the corpus gap closed by #537, the driver stopped at
the `screen-locked` refusal at step 1, the run owed to #525 — **one number now, not two**); the PR
body carrying the refusal and, where the two heads differ, the provenance sentence; this entry; and
**no §10.8 entry**, because `crates/knobas-source-mock` is on no frozen list — checked against the
list, not against the claim.

(3) **The v1.5 exit is not taken while #525 is open** (#500 part 4, repeated by both #503 rulings;
ADR-0016's dated consequence: *"a refusal is not a witness of the assertion, and no fake, no dry-run
mode and no hand checklist stands in for one"*). When every buildable ticket is merged and the probe
still reads `screen-locked 1`, the loop does **not** enter Step 4's exit branch, files nothing, and
does not idle: it posts one closing report on #491 — milestone state, the release run, #525's three
runs and the standing of each, the lock timeline, the two sentences a person can act on, this file's
path and the *Flagged, not ruled* items, and the tickets filed outside v1.5 — and ends with
`ScheduleWakeup` `stop: true`. On any `screen-locked 0` before that, #503's standing instruction
applies instead and the loop continues.

(4) **#525: label unchanged, criteria unchanged, one comment.** If #537 has merged when the unlock
comes, its runner runs all three drivers on `main`; if #537's PR is still open, it runs two and
`open-in-editor` is #537's agent's. #537 holds the one desktop slot from dispatch to merge, and
#525's runs stay ahead of it: on an unlock the orchestrator tells the live #537 agent to hold,
dispatches #525's runner, and hands the slot back when it reports.

**Reasoning:** the four questions have one shape, and it is the one #500 settled: a real, gated
change does not wait for a condition no agent can change, and a run that cannot be made is owed to a
named payer rather than faked or waived. The *fourth run* the orchestrator was right to ask about
does not exist once the criteria are read — #525 has always listed `open-in-editor`, #537 was a loan
of that run to the ticket that could pay it, and a refusal returns the loan. Holding #537 buys
nothing: the run made after a hold is the same run #525 makes on `main`, and #525 cannot close
before #537 is on `main` in any case. Part 3 fills the gap the loop file has for a milestone whose
last item is a person's precondition — Step 4 was written with an *exited* ruling as its only stop —
and the choice between idling and stopping is a spending choice: the work is all on `main`, and the
report puts the one remaining act in front of the one person who can do it.

**If you disagree, the cost of reversing this is:** (1) low before #537's PR opens — remove
`ready-for-agent`, and the corpus waits with the run; after it merges, nothing to unmerge, since the
debt sits on #525 as it always did. (2) trivial — prose in a PR body and a README paragraph, and one
comment on #501 posted later instead. (3) one command: `/loop /v15-next` restarts the loop, which
relabels #525 on the first `screen-locked 0` and takes the exit itself; the price of this ruling is
one unattended unlock, which the report tells you how to spend. (4) one label and one comment on
#525; the sequencing is an instruction to the orchestrator, revoked by a line.

**Flagged, not ruled:** the loop file's Step 4 has no branch for a milestone that cannot move and is
not exited; part 3 supplies one for v1.5, and whether it is the general rule — stop and report when
the only open work has a person's precondition — is Björn's, on his return, beside the
`ready-for-human` vocabulary question already flagged on #500.

## #525 — the two reds of the first unlocked run: what is owed, by whom, and whether the exit waits

Ruled 2026-09-08. Comment:
<https://github.com/BFoerschner/knobas/issues/525#issuecomment-5587346615>

**The fork:** the Mac was unlocked, the harness probed `trusted 1 / post-events 1 / screen-locked 0`
before and after each build, and #525's runner drove all three drivers on `main` `d7738927`, one at
a time, foreground. `launcher-hotkey` is green (transcript on #500). `open-in-editor` and `capture`
are red, and in both the driver's assertion fails while its own `ax dump` shows the app did what the
stream asks: a branch detail for `feature/PAY-231-sepa-retry` is open with *Set path…*, *Open in VS
Code*, *Open in JetBrains* and *Open terminal here*, but `open-in-editor.sh:225` waits on `present
"$CHECKOUT_PANEL"` — `CHECKOUT`, from `rendered_label 'Checkout'` — and no element in the 250-line
tree carries that as `AXTitle` or `AXDescription`, the only two attributes `ax.swift`'s `matching()`
reads; the capture window is up over Finder with the caret in its `AXTextArea`, but `capture.sh:286`'s
`exactly_one "$CAPTURE_BOX"` counts **three** elements named `Capture`. Filed as #547 and #548. Two
questions: (1) is the fix owed under #547/#548 as tickets that each end in a green transcript on #501
and #503, under #525 by its runner, or as disclosed debt left for after the milestone; (2) does the
v1.5 exit wait on them, given that they were not in spec #491's list.

**Ruling:** (1) **Both reds are owed under #547 and #548, in v1.5, each relabelled `ready-for-agent`,
and each is a desktop-witness ticket under the one-at-a-time cap** — taken in number order, #547
first, #548 when #547 has merged. Not under #525, whose body says *"Do not resolve it here."* Not
post-milestone, because the same body's first bold line is *"The v1.5 exit is not taken while this
ticket is open."* **Each fix is to `testenv/**` — the driver, or the harness under it — and never to
the app to satisfy the driver**: the app opened the panel and the window, the dumps are the
measurement, and the names it carries are its own and each is right. `ax.swift`'s doc comment on
`matching()` already states the rule the drivers broke — *"A driver that insisted on one would fail
on a correct app for a reason that has nothing to do with what it is witnessing."* Five conditions
of each merge, on the branch: **(a)** the implementer re-verifies the diagnosis on the tree it starts
from and says so on the PR (for #548, what it can establish about how a green at 04:55 became a red
on `d7738927`, and *unmeasurable* rather than a guess where the squashed branch puts it out of
reach); **(b) the new waypoint is witnessed in both directions** — whatever the driver waits on is
something the dump measures the tree to carry, and the PR body shows it **false before the step and
true after** on the same tree, which for #547 means the wait cannot be satisfied by the launcher's
own hit list before the detail opens; **(c)** pure logic goes in `desktop-witness-lib.sh` under
`witness-unit` (*"a driver is not exempt from the gate because its run is"*), and **no `pin_label`
of a markup spelling is offered as the witness of an accessible name**, since #547's own finding is
that the pin of the *Checkout* heading stayed green over this; **(d) the merge-manager runs the
driver green on the branch head it squashes**, foreground, prerequisite 4 first, and posts that
transcript on **#501** (for #547) or **#503** (for #548), which **discharges #525's criterion 2 or
3** — and on a `screen-locked` refusal the PR merges as #523, #530, #536 and #537 did, with the run
owed; **(e)** this entry, written by #547's implementer as the first to land, and **no ADR and no
glossary entry**, because nothing architectural is decided and ADR-0016's dated consequence already
says what the witness observes.

(2) **The v1.5 exit waits on #547 and #548, through #525**, which stays open under its title with
its criteria unchanged as the holder of runs 2 and 3; `in-progress` comes off and no other label
goes on — not `ready-for-agent`, because *"the remaining work is another ticket's criterion, not a
dispatch"*, and not `ready-for-human`, because no person's precondition remains. When both are closed
and green transcripts from the head that merged sit on #501 and #503, the orchestrator closes #525
before Step 4's exit branch is entered. (3) **Nothing else is owed on #525.** The `launcher-hotkey`
green stands; the two dumps are evidence for #547 and #548 and are **not** witnesses of streams 4
and 5 — *"a panel on screen is not a stub spawned with the checkout path, and a window with a caret
is not a note with its links."*

**Reasoning:** part 1 was answered by #525's body twice. *"Do not resolve it here. File a new ticket
with the driver's `ax dump` output attached"* names the payer, and *"a red run means the three
drivers are re-done against whatever the dump shows — the same cost whenever it is paid, which is
why the ruling chose to pay it once, **later**, rather than stall three tickets now"* prices it —
*later* than #500's merge, inside a ticket whose first line holds the exit. The fix is the driver and
not the app for the reason the working model names, *a check that measures a representation of the
thing instead of the thing*, and this harness has now produced that class three times: the markup
names `Directory` and `Checkout` that #536's fix uppercased, the `witness-unit` pin that stayed green
over a name the AX API never answered, and `matching()`'s two attributes against a window whose three
names are all correct. The memory of #398 says what an unsatisfiable criterion does next — it
*"pushes the agent toward changing correct work to match a broken check"* — which is why condition
(b) asks for both directions rather than a green run: `exactly_one` to `present` on the same name
would pass, and would also pass on a `Capture` that never opened. Part 2 follows from what the
witness is: spec #491's stream map names **desktop automation** as the witness of rows 4 and 5, and
ADR-0016's consequence says what it observes — *"the spawned editor or terminal observed as a process
with the expected path"* — and `open-in-editor` stopped one step before the button. ADR-0013 decides
the class: *"awkward to reproduce is not cannot produce: if the real instance can be driven into the
fault by any seed, flag or clock, it is witnessed live"*, and the desktop was driven today.

**If you disagree, the cost of reversing this is:** (1) low before either PR opens — two `wontfix`
labels, or two milestones removed, and nothing merged is undone; after they merge, nothing to
unmerge, since each is `testenv/**` on no frozen surface. What the reversal spends is the price the
first #503 ruling put on closing #537: *"the ADR-0016 witness for* Open in editor *never runs inside
v1.5"*, and now the capture witness on the merged head with it. Reversing the *app untouched*
condition is one attribute in a Svelte file; its price is a witness that measures a label added for
the witness. (2) one label and one comment — close #525 against the one green — and nothing in code;
the price is a milestone exit with two of its three desktop streams never green on `main`. (3) the
sequencing and the closing instruction are lines to the orchestrator, revoked by a line.

**Flagged, not ruled:** whether `witness-unit`'s pins of rendered and markup names should be retired
or replaced by something that reads the AX tree — three instances in one harness of a pin green while
the tree disagreed is a rule waiting to be written in `docs/agents/working-model.md`, beside the
shared-infrastructure sentence already flagged on #503. And whether the loop file's desktop cap
should name tickets by shape (*any ticket whose merge runs `just desktop-witness`*) rather than by
number, which would have covered #525, #537, #547 and #548 without a ruling each. Both Björn's, on
his return.

---

## #491 — the v1.5 exit: what holds, what is missing, and the spec's label

Ruled 2026-09-11. Comment:
<https://github.com/BFoerschner/knobas/issues/491#issuecomment-5631011183>

**The fork:** the loop's Step 4, dispatched on *"v1.5 exit: every ticket closed, every live recipe
green on its last run, the release run green — exited, or what is missing"*. Two limits were set on
the answer: it is made on the record (`testenv/README.md`, the tracker, this file), because the live
recipes spend a shared Hetzner fixture and the desktop witness takes the Mac's screen; and it must
say what happens to #491 itself and its `ready-for-agent` label. The state, verified rather than
taken from the orchestrator's report: milestone 13 holds 28 issues, 27 closed and every one
`COMPLETED`, the one open issue this spec; no PR open; no open issue `in-progress`; `origin/main` at
`76899b84`; tag `v1.0.0` and release run `34160263470` of 2026-09-07 `success`; #525 closed on all
four criteria.

**Ruling:** **not exited — one thing is missing, and it is a runner's ticket, not a code change.**
(1) Clause one holds: every ticket is closed — #492–#510 and the eight the loop filed on rulings
(#518, #522, #525, #531, #533, #537, #547, #548), and the desktop half meets #525's standard, three
green transcripts each of the head that merged (`launcher-hotkey` on `d7738927`, `open-in-editor` on
`ee6a866e`/`1fe5705a`, `capture` on `8c067a59`/`76899b84`), each ending `desktop-witness: ok` with
`screen-locked 0` before and after. (2) Clause three holds: the release run is green, and spec
#491's stream 0 names *"the dispatched release workflow, as v0.1.0"* as its witness; whether `main`
at the exit is tagged v1.5.0 is flagged, not ruled. (3) **Clause two does not hold.** The working
model lists eight recipes and the `justfile` carries exactly those eight; one, `estate-live`, is
green on a head nothing has moved under. The other seven were last green on heads this milestone
then changed in the two places a live run exists to certify: every adapter crate (#520) and the sync
engine's `build_source` keychain read (#539). No recipe has a red on record, and no run of any of
the seven on `main` as it will be exited exists. **So the milestone gets one ticket — *The v1.5 exit
sweep: every live recipe on the exit head*, `ready-for-agent`, in #525's shape: a runner, no PR,
closed on transcripts.** A tunnel ticket, the runner the environment's one owner, all eight recipes
on the exit head serially and foreground in the working model's order, each transcript verbatim with
the head's SHA and **the counts read, not the summary line**, the fixture left as found. A recipe
that cannot be started is reported by name with its refusal, never skipped quietly. **A red is not
fixed on that ticket**: it is filed as its own v1.5 ticket with the transcript, and fork kinds 3 and
4 decide through the deputy whether the fixture or the code owes it — *the recipes a fix reaches
re-run on the fix's merge head*. (4) `ready-for-agent` comes off #491 now and nothing goes on: the
spec is not a dispatch and Step 2 has been returning it as a false candidate every iteration. #491
stays open until the exit and closes, as completed, in the iteration that closes milestone 13, with
the exit ruling as its closing comment in #427's shape; the orchestrator strips `in-progress` from
the twenty-four closed tickets still carrying it. No glossary entry and no ADR.

**Reasoning:** a milestone exit here has meant a live sweep since M3 — the roadmap's *"each exit
runs every live suite"*, M3.3's *"every live suite green, run by Björn"*, M4's *"every live suite
green, and the paperwork merged"* — and v1.5's section keeps the gate and strikes nothing of the
sweep. ADR-0013 is why the sweep is the evidence: *"every adapter and every write path is tested
against the real system, and no mock is a witness for any acceptance or exit criterion"*, and *"the
adapter's certificate stays its `just <system>-live` recipe"*. Step 4's *"green on its last run"* is
the orchestrator's compression of that, and the two readings part company exactly when a recipe's
last run predates a change to what it certifies — true of seven of eight. This milestone already
ruled that way for the desktop half: the README's *"both are runs of the head they were made on, not
of what merged"* is why the 04:55 transcripts did not discharge #525. The ticket's shape follows
#525's precedent, and Step 4 itself says *"file it as a ticket in the milestone with
`ready-for-agent` and keep looping"*. The cost is bounded by the README's own measurements.

**If you disagree, the cost of reversing this is:** one label and one comment — close the sweep
ticket `wontfix`, take the exit on the record, close milestone 13 and #491 — and nothing in code,
because this ruling merges and unmerges nothing. What that spends is the exit's evidence: a
milestone closed with seven of its eight adapter certificates last read before it changed every
adapter crate and the sync engine, which is the trade #525's ruling refused for the desktop half.
Reversing part 4 is one label put back. If the sweep finds a red the cost is the same whenever it is
paid, and cheaper on a head that has not moved on.

**Flagged, not ruled:** whether Step 4 should say *green on the exit head* rather than *green on its
last run*; whether `main` at the exit is tagged and released as v1.5.0 (spec #491 tags only v1.0.0
and says nothing either way); whether the working model's 2026-08-24 sentence about a high-effort
review over the milestone's accumulated diff is still a rule — it is on the record, it is not in
Step 4's fork, and this ruling neither adds it nor strikes it; and the items already collected in
the #525 ruling.

---

## #552 — the `kuma-live` red: who owes it, the witness in both directions, and where the paperwork rides

Ruled 2026-09-11. Comment:
<https://github.com/BFoerschner/knobas/issues/552#issuecomment-5631229490>

**The fork:** fork kind 3, *"a live recipe red for a reason in the fixture rather than the code"*.
The exit sweep #551 ran all eight recipes on `main` `76899b84`; seven are green (70 tests over
fourteen suites, counts read) and `just kuma-live` is red — `live_kuma` 9 run, 1 ok, 8 failed, every
failure `Unauthorized { status: Some(401) }`, and under `set -e` neither `kuma_write_live` nor
`alert_chain_live` (M4.1's exit witness) ran, so the recipe had **no verdict on the exit head in
either direction**. The diagnosis, checked against the files it names: `testenv/kuma-api-key` in the
root checkout holds key id 4; the instance's `api_key` table holds exactly one key, id 6,
`knobas-seed`, minted 2026-09-07 15:42 by a sibling worktree's seed, four hours after this
checkout's file was written; and `kuma-seed.mjs`'s keep-branch is `if (mine && HOST_HAS_KEY)`, where
`HOST_HAS_KEY` comes from `seed-kuma.sh`'s `[ -s "$KEY_FILE" ] && HAVE=1`. That tests *a key of that
name exists* and *this host has a file*, never *the file's key still authenticates*. Two candidate
fixes, not exclusive: **(a)** delete the stale file by hand so the seed's `else` branch re-mints —
seconds, no PR; **(b)** the seed verifies the host's key before keeping it, as `seed-gitea.sh` step
8 does for the Gitea token — a PR, a mutation proof, a re-run on the merge head. Subordinate: the
#491 exit ruling said *"the runner who acts on this appends it"*, and that runner was forbidden to
commit.

**Ruling:** (1) **The seed owes the red, and the fix is code: #552, `ready-for-agent`, v1.5, one
implementer.** The property: **`./seed-kuma.sh` keeps the key in `kuma-api-key` only while it still
authenticates against the instance; a key the instance answers `401` to is re-minted, and the seed
says so.** That is the sentence `testenv/README.md` already writes for Gitea. Where the check lives
is the implementer's call — the shell shape in `seed-kuma.sh` before it sets `HAVE` is the smaller
change and the one leaned to, not ruled. **Nothing outside `testenv/**` and docs is touched**: not
`crates/knobas-source-kuma`, not the suites, not the recipe's three-suite order. The `401` the
suites reported is the adapter being right about a dead credential, and
`a_wrong_api_key_is_the_credential_health_path` witnesses that class on purpose. (2) **The witness
is measured in both directions on the real instance (ADR-0013), in the PR body.** (i) Dead key, then
re-mint: with `kuma-api-key` holding a well-formed key the instance does not have, `./seed-kuma.sh`
logs the re-mint and writes a new file, and that file answers `200` on `/metrics`. (ii) Live key,
then kept: a second run prints its keep line and leaves the file's bytes unchanged. A fix that
re-mints on every run would pass (i), kill every sibling tree's copy on each run, and be the
collision the README describes from the other side. Then `just kuma-live` green on the branch head,
all three suites, counts read. (3) **Nobody deletes `testenv/kuma-api-key` by hand ahead of the
PR** — not because (a) is wrong, but because it buys nothing (b) does not deliver sooner, and it
destroys the one naturally occurring instance of the fault, which is the mutant 2(i) runs against.
(4) **The seven green transcripts on `76899b84` stand as the exit's evidence**, because the PR
touches nothing they certify; that holds only while the PR stays inside `testenv/**` and docs, and
**the merge-manager checks the file list, not the claim**. The merge-manager re-runs `just
kuma-live` foreground on the branch head it squashes, rebased onto `main` so the squash commit's
tree is that head's tree (#525's condition (d)), from a tree whose `kuma-api-key` it names as dead
or absent beforehand, and posts the transcript on #551 with the branch SHA and the squash SHA. The
orchestrator then closes #551 on eight green transcripts, seven on `76899b84` and one on the merge
head. (5) **Both this ruling and the #491 exit ruling are appended here in #552's PR, in the order
ruled**, and the header's *"the PR that acts on it"* is corrected to *the first PR the ruling's
chain produces, or a docs-only PR when there is none*; the same PR rewrites the README's `just
kuma-live` bullet. The terminal case — the *exited* ruling itself, which no PR acts on — rides in a
docs-only PR in PR #490's shape, merged before milestone 13 is closed. (6) `needs-triage` comes off
and `ready-for-agent` goes on; the implementer's worktree needs `testenv/hetzner/hosts.env`, and it
is a tunnel-class ticket. No glossary entry and no ADR.

**Reasoning:** the answer existed in three places and is quoted rather than re-decided. First, the
working model names the class this keep-branch belongs to: *"a check that measures a representation
of the thing instead of the thing … All of them fail green, which is why reading a passing run will
never find one."* `[ -s kuma-api-key ]` measures a file's presence and reads it as a credential's
validity, and the sweep is the reading of a passing seed that found nothing; (a) alone would leave
that check in place and re-arm it for the next sibling seed. Second, the standard is already in the
tree: `seed-gitea.sh` step 8 solves the same problem for the sibling seed in the same directory, and
the README documents it as the expected behaviour — a Kuma seed that does less is a half-convention
across two files in one directory. Third, #525's ruling set the direction for fixture-class reds
this milestone, *"Each fix is to `testenv/**` … and never to the app"*. ADR-0013 decides the
witness: a dead key is a fault the real instance produces on demand, so it is witnessed live, and
both directions are asked for because #525's condition (b) already found that a one-direction
witness *"would also pass on a `Capture` that never opened"*. Part 4's head reasoning is the exit
ruling's own, *"a green of an older head certifies that head"*. Part 5 corrects one word of a ruling
this deputy wrote — *runner* for *PR* — because the #525 precedent was the same shape and
`git log -S'## #525'` names `1fe5705a` (#549), a fix PR two tickets downstream, not the runner.

**If you disagree, the cost of reversing this is:** (1) low before the PR opens — one label, and the
runner deletes `testenv/kuma-api-key` and re-runs `just kuma-live` on `76899b84`, which is (a) and
takes minutes; after the merge, nothing to unmerge on any frozen surface, since the PR is
`testenv/**` and docs. What (a)-alone spends is the next hour lost to the same `401` by the next
tree that runs after a sibling seeds. (2) is one PR-body paragraph. (3) is a file deletion. (4)
reversing the *branch head rebased onto main* reading costs one more `kuma-live` run after the
squash; reversing *the seven greens stand* costs a second full sweep for seven recipes whose
subjects did not move. (5) is a section moved between two PRs and one clause in a header. (6) is a
label.

**Flagged, not ruled:** whether the environment wants the lock the README says it does not have
(*"closing it properly would mean a lock the tooling does not have"*), now that both seeds heal a
dead credential and the remaining collision is the mid-run one; and whether this file's header
should say outright that a ruling no PR acts on rides in a docs-only PR, so the next milestone's
exit does not need part 5.

---

## #491 (second ruling) — the v1.5 exit, dispatched a second time after the sweep

Ruled 2026-09-11. Comment:
<https://github.com/BFoerschner/knobas/issues/491#issuecomment-5631698324>

**The fork:** Step 4 of the `/v15-next` loop file, verbatim: *"If no candidate is free, no ticket is
in-progress and no PR is open, the milestone is done: dispatch the deputy once more on the fork
'v1.5 exit: every ticket closed, every live recipe green on its last run, the release run green —
exited, or what is missing' and, on a ruling of exited, close the milestone … comment the ruling on
#491, and `ScheduleWakeup` with `stop: true`."* The fork is the loop's seventh kind of decision,
*"The milestone exit"*, and the delegation covers it. The ruling of 07:29 the same day settled
clauses one and three and said the second dispatch *"needs nothing beyond this one and the eight
transcripts: parts 1 and 2 above are settled and are not re-argued."* What changed since, verified
on the tracker and the tree rather than taken from the orchestrator: #551 closed on eight
transcripts; #552 ruled at 07:51, its PR #553 merged as `16d0dee0` at 08:23; `main` and
`origin/main` are `16d0dee0`; milestone 13 reads 29 closed, 1 open, the open one this spec, and
every closed one `COMPLETED`; no pull request is open; no open issue carries `in-progress`; tag
`v1.0.0` exists and run `34160263470` still reads `success`. One new question was put to this
ruling: whether #554, filed `needs-triage` with no milestone, belongs in v1.5 and holds the exit.

**Ruling: exited.** Nothing is missing. #554 stays out of the milestone. The closing iteration owes
five things, in the order of part 4 below.

**(1) Clause two holds. Every live recipe is green on its last run, and its last run is a run of the
tree that is `main`.** The eight transcripts on #551, counts read from each rather than from the
sweep's table:

| recipe | head | counts read |
|---|---|---|
| `just gitea-live` | `76899b84` | 12 run, 12 ok, 0 failed |
| `just gitea-live-capped` | `76899b84` | 1 run, 1 ok, 0 failed |
| `just start-work-live` | `76899b84` | 1 run, 1 ok, 0 failed |
| `just teamcity-live` | `76899b84` | 12 run, 12 ok, 0 failed |
| `just teamcity-live-seeded` | `76899b84` | 8 + 1 over two suites, 9 ok, 0 failed |
| `just atlassian-live` | `76899b84` | 14 + 15 + 13 + 2 + 1 over five suites, 45 ok, 0 failed |
| `just kuma-live` | `c516588`, squashed as `16d0dee0` | 9 + 3 + 1 over three suites, 13 ok, 0 failed |
| `just estate-live` | `76899b84` | 2 run, 2 ok, 0 failed |

The seven on `76899b84` stand under part 4 of the #552 ruling, which bounds them to a PR that stays
inside `testenv/**` and docs. `git diff --name-only 76899b84 16d0dee0` is exactly four paths:
`docs/decisions/2026-09-v1-5-unattended-rulings.md`, `testenv/README.md`, `testenv/kuma-seed.mjs`,
`testenv/seed-kuma.sh`. No crate, no suite, no `justfile`. The merge-manager read the same list on
the branch before squashing, and the eighth run is a run of the tree that is now `main`: the branch
tree and the squash tree share the id `ae37f72d`, which is #525's condition (d) as applied by the
#552 ruling. That run started from a tree whose `kuma-api-key` was named `401 (DEAD)` beforehand,
the seed printed `no longer authenticates (401); minting a new one`, and all three suites then ran,
so `alert_chain_live`, M4.1's exit witness, has its verdict back. The fixture was left as found on
the merge-manager's own account: the roster the eight names in `monitors.json`, the canary rebound,
no container stopped, the tunnel as found. Nothing has moved under any recipe since: `main` is one
commit past `76899b84`, and that commit is the four files above.

**(2) Clauses one and three are settled and are not re-argued.** The only change to clause one is two
more tickets, #551 and #552, both closed as completed; the milestone's count is twenty-nine closed
and this spec. Clause three is unchanged.

**(3) #554 is filed right. It stays out of v1.5, `needs-triage`, no milestone, and the exit does not
wait on it.** This milestone put a ticket found mid-way into v1.5 twice, and the test was the same
both times: the exit's own witness. #531 went in under the #501 ruling because a spurious red at the
desktop gate *"would send the runner down that path for a defect that is not there"*; #547 and #548
went in under the #525 ruling because the exit witness itself was red. Six other tickets found
during v1.5 went out, listed in the closing report's section 7 as *"Filed outside this milestone,
for you to place"*, and that is #554's class. Nothing a witness reads is in it. The property #552
ruled, *"keeps the key in `kuma-api-key` only while it still authenticates against the instance; a
key the instance answers `401` to is re-minted, and the seed says so"*, is landed and witnessed in
three directions in #553's body, and the seed's log line is the one the transcript shows. A recipe
comment in the `justfile` that names one trigger where there are now two, and three variable names
that lag their comments, mislead no run: the seed heals the dead key whichever name the variable
carries. The merge-manager was right to leave both out of #553, because the `justfile` is outside
the bound and touching it would have cost a second sweep for a comment. #554's own part 2 says why
it is not a rename-and-go, and that argues for its own witness on its own ticket, not for a place in
a milestone whose exit evidence is already taken. Björn places it. The closing comment on #491 lists
it as the seventh unplaced ticket beside #515, #516, #524, #529, #541 and #544.

**(4) What the closing iteration owes, in this order.**

(a) **A docs-only PR carrying this ruling, merged before anything closes.** Part 5 of the #552 ruling
set this and it is confirmed with one narrowing. It appends this ruling to this file as `## #491
(second ruling)`, after the `## #552` section, in the four-part shape; the file's header already says
a ruling no PR acts on rides in a docs-only PR, so no header change. It adds one dated sentence to
the v1.5 section of `docs/roadmap.md`, the treatment M4 got in that section's first line: v1.5
closed 2026-09-11 on this ruling in Björn's absence, `main` at the exit named by SHA, eight live
recipes green on #551, and Björn reverses with a follow-up ticket. **The PR is `docs/**` and nothing
else.** PR #490 is the shape for the title and the body, not for the file list: #490 touched doc
comments in two crates, and this PR may not, because the eight greens rest on the tree not moving
under any recipe. The merge-manager checks `git diff --name-only` against the branch point and
refuses anything outside `docs/`. **No `Closes #491` line**, inside or outside backticks: #491 closes
by hand in step (c), with its closing comment, and a keyword would close it at the merge with none.
The orchestrator opens it; a merge-manager merges it under its brief as written.

(b) **The orchestrator strips `in-progress` from the twenty-five closed v1.5 tickets still carrying
it.** The 07:29 ruling said twenty-four; #552 closed with the label on and makes twenty-five: #492 to
#510, #518, #522, #531, #537, #547, #552. The label on a closed ticket is a false answer to Step 1's
query and holds nothing.

(c) **The closing comment on #491, then #491 closed as completed.** In #427's shape, the sentence M4
got: v1.5 is complete, every ticket #492 to #510 and the ten the loop filed (#518, #522, #525, #531,
#533, #537, #547, #548, #551, #552) closed, the exit sweep #551 green on eight transcripts, v1.0.0
released on run `34160263470`, the deputy ruled the close on 2026-09-11 with this comment's URL, and
`main` at the squash SHA of the PR in (a). The seven unplaced tickets from part 3 go in the same
comment. Then `gh issue close 491 --reason completed`. No label goes on.

(d) **Milestone 13 closed** with the API call Step 4 names.

(e) **`ScheduleWakeup` with `stop: true`.**

No tag is cut at the exit: whether `main` is released as v1.5.0 was flagged at 07:29 and is not ruled
here. No glossary entry and no ADR: nothing architectural is decided, and the rules applied are the
07:29 and 07:51 rulings, the working model's, and ADR-0013's as written.

**Reasoning:** the answer to clause two is the 07:29 ruling's own test, applied to eight transcripts
that now exist. That ruling said a green certifies *"the head it was made on"*, and the #552 ruling
said what makes a later head the same for a recipe's purposes: *"The PR touches nothing they
certify, no adapter crate, no engine, no seed those suites read a credential from."* The file list is
read from `git`, which is the working model's rule for this class, *"a check that measures a
representation of the thing instead of the thing"*, turned the right way: the claim was checked
against the diff, twice, by the merge-manager and here. The eighth transcript is worth more than a
green because it is the fault reproduced and healed, the mutant the #552 ruling refused to let anyone
delete. ADR-0013 is the reason the sweep was the evidence at all, *"the adapter's certificate stays
its `just <system>-live` recipe"*, and now every certificate is read on the tree being exited. Part 3
follows the rule this milestone already used twice for a ticket found mid-way, and the reading that
keeps the ticket's scope unchanged: spec #491's ten streams are done, and #554 is a comment and three
names in `testenv/**`, not a stream and not a witness. Part 4(a) narrows #490 because the bound in
the #552 ruling is a bound on the tree, and a doc comment in a crate is a change to the tree the
sweep certified; the roadmap sentence is owed because the v1.5 section's own hold paragraph says a
decision should read *"as a decision and not as an omission"*, and a milestone closed by a deputy
with no dated line in the roadmap would read as the second. Part 4(c) keeps #491 out of the PR's
closing keyword for the reason the 07:29 ruling gave: #427 closed with a sentence naming `main` at
its exit, and a keyword closes with none. The order in part 4 is the #552 ruling's *"merged before
milestone 13 is closed"*, with the reason spelled out: the closing comment names the SHA the PR
produces, so the PR goes first.

**If you disagree, the cost of reversing this is:** the exit itself, one label and two API calls:
reopen milestone 13 and #491, and file the ticket that names what was missing. Nothing in code,
because this ruling merges one docs-only PR and touches no crate, seed, suite or recipe. Reversing
part 3 is `gh issue edit 554 --milestone v1.5 --add-label ready-for-agent` and a re-run of `just
kuma-live` on its merge head; what that spends is the exit held for a comment fix while the seven
greens rest on a tree that has not moved. Reversing part 4(a)'s narrowing is a doc comment in a
crate, and its price is that the merge-manager can no longer read the file list as the bound.
Reversing the roadmap sentence is a deletion. Reversing part 4(b) is twenty-five labels put back, and
nothing depends on them. Reversing the v1.5.0 decision that was not made costs nothing, because
`main` at the exit is a SHA in the roadmap and in the closing comment, and a tag can be cut on it any
day.

**Flagged, not ruled:** the three items of the 07:29 ruling stand unchanged — Step 4's wording
(*green on the exit head* rather than *green on its last run*), the v1.5.0 tag, and the working
model's high-effort review over the milestone's accumulated diff, which no ruling in this milestone
added or struck. The #552 ruling's two items stand too: the lock the README says the tooling does not
have, and this file's header, which #553 has now corrected in the form that ruling asked about. Two
new: `just teamcity-live`'s transcript carries a test that says of itself *"this run is NOT a
witness"* because the public instance's window held no personal build, which was equally true of
every earlier run and is a corpus property, not a red; and whether a ticket found during a milestone
gets a written rule for which milestone it lands in, since this milestone answered that question
three times by ruling and once by report.

---

## #554 — the three stale key names, and the lock the environment does not get

Ruled 2026-09-11, in the ticket's triage comment. Its heading is `Triage (deputy for Björn,
2026-09-11):`, not the `Ruling (deputy for Björn, <date>):` this file's header names; it carries a
ruling all the same, and it closes one of the two items #552's *Flagged, not ruled* left open, so it
is recorded here. Comment:
<https://github.com/BFoerschner/knobas/issues/554#issuecomment-5632603526>

**The fork:** two. (1) #554's own part 2 offered the implementer a choice — rename `HAVE`,
`KNOBAS_HAVE_KEY` and `HOST_HAS_KEY`, *"or are left alone with a recorded reason"* — and an
unattended agent picking between those is picking what the next reader believes. (2) #552's
*Flagged, not ruled*: whether the environment wants the lock `testenv/README.md` says it does not
have (*"closing it properly would mean a lock the tooling does not have"*), now that both seeds heal
a dead credential on their next run and the only collision left is the mid-run one.

**Ruling:** (1) **`ready-for-agent`, tunnel-class, both parts in one PR.** (2) **The "left alone"
option is struck: all three names change**, on both sides of the container boundary, to say the
host's copy answered `200`. The names themselves are the implementer's. The witness is the one the
ticket asks for and #553 already landed — a dead key re-mints, a live key is kept with its bytes
unchanged, an unanswered port refuses and touches neither the file nor the instance's key list —
**repeated, not invented**. (3) **No lock is built.** The mid-run collision is answered by
dispatching tunnel-class tickets **one at a time**, the cap the loop already carried, rather than by
a second mechanism in the tooling for a rule one orchestrator enforces by sequencing. The README's
sentence stays true and stays as written. The consequence for this backlog: #516 and #554 run
serially, never side by side.

**Reasoning:** for (2), the merge-manager of #553 wrote the reason when it filed the ticket — the
names are what the next reader believes, and a name that still asks *does a file exist* re-arms the
question #552 removed from the code. What made it a fork at all was cost, not doubt: the rename
crosses a container boundary and its one-sided failure is silent, so it needs a live witness. That
witness exists as a written procedure in #553's PR body, which makes the expensive half a repeat
rather than a design. For (3), the working model's concurrency rule and this file's own record
already carry the answer: the collision needs two agents seeding at once, and the loop's dispatch
is where that is decided. A lock would be a second place to be wrong about it.

**If you disagree, the cost of reversing this is:** (2) is Björn ruling the names stay — then part 2
collapses back to a recorded reason beside them, the live witness is not spent, and part 1 (the
recipe comment) stands either way. (3) is a follow-up ticket for a `$TMPDIR` lock in the shape of
the gate slots (#425), a shape that already exists in the `justfile`; nothing in this PR has to come
out first.
