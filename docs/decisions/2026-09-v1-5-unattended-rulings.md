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
