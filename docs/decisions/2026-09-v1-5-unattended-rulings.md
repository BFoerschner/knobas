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
