# Unattended batch rulings — 2026-08-29, Fable under delegation

Björn: you said "if any big decision needs to be made use fable to figure out the best way and write the decision down for me." Fifteen forks were ruled across five batches; each ruling is posted as a comment on its issue, opening with a line saying you can overturn it. It was safe to decide them because none touches a §10.8 frozen surface (checked each against the list), none is a milestone-exit judgement, one (#84) was already decided by you via ADR-0005 and only needed confirming, and every ruling rests on code or issue text that is quoted in the comment — no ruling rests on a hypothesis about a document I did not read.

Each batch is its own section below, in the order it was ruled. **The reading order is at the foot of the file, not here** — it was rewritten once all five batches existed.

---

## The first batch — #91, #92, #81, #84, #82, #93 and #69

The one place I diverge from a mechanism you sketched is #82, and it is flagged loudly both here and on the issue.

### #91 — TeamCity watermark ceiling deadlock

**The fork:** four options; the issue leans option 2 (derive the ceiling from the per-configuration pages the sync already fetches).

**Ruling:** option 2 amended, with 1 and 4 folded in. The ceiling becomes the max id over the *witnessed* pages only — a probe widened to `count:100` read as page-max, plus the global in-flight page — never the finished pages. The two-row ordering guard is deleted. The replaced-server refusal stays but re-founds its evidence: when everything observed sits below the watermark, fetch the watermark build by id (`/app/rest/builds/id:{id}`, already in the contract's endpoint list); 404 means replaced, found means proceed.

**Reasoning:** option 2 as written is unsafe — finished-page ids can post-date run start, so a ceiling built from them lets a build queued mid-run be overtaken by a later-queued build that finished inside the same run, which is exactly the silent loss the ceiling exists to close (`sync.rs`: "the ceiling saves builds *queued after* it"). Witnessed-page ids can only under-estimate, and an under-estimate costs a re-fetch, never a build. The old "newest < watermark ⇒ replaced server" check fires routinely on a server whose page ordering is arbitrary, which the live instance is; the by-id fetch is evidence ordering cannot fake, and it makes the "reset the cursor" remedy actually work.

**If you disagree, the cost of reversing this is:** moderate — it is all inside `knobas-source-teamcity` plus mockd fixtures, nothing frozen; but the doc comments this rewrites encode the loss-analysis, so a reversal should rewrite them too, not just the code. The one open risk I named on the issue: if a live run ever shows `builds/id:` 404ing for permission reasons on a build the source itself synced, the false refusal returns and the detection design is back on your desk.

### #92 — TeamCity live certification suite

**The fork:** same PR as #91 or separate; does #91's regression test depend on it; what to do with absent credentials.

**Ruling:** separate PR; no dependency in either direction (#91's regression test is offline in the fake; its live AC is discharged by a manual run or by this suite, whichever exists first). And a token is **not** required: I verified today, read-only, that `https://teamcity.jetbrains.com/guestAuth/app/rest/server` answers 200 with no credentials, ignores a bogus bearer header, and serves `users/current` too — and `knobas-http`'s string-concatenation URL building keeps the `/guestAuth` prefix, so this works with zero adapter changes. The suite runs token-less given the URL, passes a placeholder secret (the adapter refuses `None`), skips cleanly only when the URL is absent, and `.env.example` gains the guestAuth URL as its documented value.

**Reasoning:** #91 is deep-pass sync correctness, this is a standard-pass harness; your own PR rule says risky work wants its own bisect point. The guest finding removes the only operational obstacle to running it.

**If you disagree, the cost of reversing this is:** near zero — it is sequencing and a defaults choice, both changeable before or after the PR lands.

### #81 — Gitea paging terminates on a short page

**The fork:** three options; the issue leans option 1 (page until empty).

**Ruling:** option 1, once, for all four sites (line numbers have drifted to 331/491/586/808). The existing `MAX_*_PAGES` caps stay as the loud bound. The fake gains a capped-pages mode so "short because capped" is distinguishable from "short because exhausted".

**Reasoning:** the failure is silent and watermark-advancing — the class ADR-0003 exists to prevent — and the cost of closing it is one extra request per exhausted walk under a 10 req/s limiter. Option 2 trades one undocumented assumption for two; option 3 documents a hole instead of closing it.

**If you disagree, the cost of reversing this is:** trivial — four termination conditions and their tests, no cursor or schema impact.

### #84 — wizard DONE panel races the scheduler wake

**The fork:** you ruled "grill this before it becomes a ticket."

**Ruling:** *not mine* — the grilling happened and you decided it yourself this morning: ADR-0005 ("a run id always comes with an ending", commit 5c850cd), plus the agent brief on the issue. My comment confirms the ADR against the code and answers the issue's four questions from it (wake stays; wizard attaches via the multi-sink ending contract; "mirrored N" means the corpus per CONTEXT.md's Mirror/Upserted split; idle-until-tick is a real problem, judged so twice). One genuinely open edge I flagged for you: a run row whose `finished_at` is null forever because the process died mid-run has no recorded outcome to synthesise an ending from; ADR-0005's text does not answer what such a late attacher receives.

**If you disagree, the cost of reversing this is:** it is your own ADR, so reversal is an ADR supersession, not a comment.

### #82 — PAT user told contradictory things about `username`

**The fork:** your triage named three options and said an implementer should not choose alone; option 1 was "fill it backend-side in `add_source`."

**Ruling:** option 1's intent, but **in the dialog, not the backend** — and this is the one place I diverge from what you sketched. Your option 1's premise ("The command already has both the ConnectionInfo and the adapter") does not hold: `crud::add` never calls `test_connection`; only `set_secret`'s re-test has a `ConnectionInfo`. Backend fill at add would need either a second network call inside the save or a new `NewSource` field, which is IPC and frozen. Meanwhile the dialog cannot reach Save without a green test (`canAdvance` requires `report?.ok === true`), so `report.account` is reliably in hand at the right moment, for both entry paths (the wizard embeds `AddSource`). So: fill the schema-declared `username` form field from `report.account`, only when empty, keyed on the property *name* as a cross-adapter convention (all three adapters spell it `username`). Plus the text fixes: Jira's and TeamCity's "only for user + password" descriptions rewritten on Gitea's pattern (whose "Filled in by Test connection" promise finally becomes true), and `describe_missing_identity()`'s advice re-checked. Re-enter-path backfill for existing sources: explicitly left out, for you.

**If you disagree, the cost of reversing this is:** small if caught before implementation — the two viable backend shapes are named on the issue. After implementation, moving it backend-side re-does the tests but the text fixes and the only-if-empty rule carry over unchanged.

### #93 — no test joins engine + real database + real adapter

**The fork:** three open questions — home, `just check` membership, adapter count.

**Ruling:** `crates/knobas-app/tests/` (the dev-dependency worry is moot: knobas-app already depends on all three adapter crates as plain dependencies, and its tests dir already holds both halves separately); yes to `just check` (mockd is in-process and docker-free, embedded Postgres per binary is already the norm there, and an `#[ignore]`d seam test is a seam test nobody runs); one adapter now — Jira, discharging #32's criterion end to end — TeamCity as follow-up, Gitea excluded because mockd has no Gitea by standing decision and its equivalent belongs to the container/live layer.

**If you disagree, the cost of reversing this is:** low — moving the harness or pulling it out of `check` is mechanical; the only ratchet is that once it guards the seam, removing it from `check` re-opens the gap the issue documents.

### #69 — backup settings surface

**The fork:** genuinely blocked on a settings view, or build the surface itself?

**Ruling:** not blocked — the blocker is void (#36 closed without landing a settings view; `app/src/lib/` has none; no other open issue claims one), so #69 builds the minimum shell itself: `app/src/lib/settings/` with one `SettingsView` reached via the shell's existing navigation, titled sections in one scrollable pane, no tabs/router/registry, Backup as the only section — §14's plain dialogs over the four landed commands, defaults surfaced with your boundary-not-a-moment wording.

**If you disagree, the cost of reversing this is:** small — the shell is a thin frontend container; if you want a different settings IA later, the backup section moves into it as one component.

---

### Flagged: things I decided were NOT mine to rule

- **#82, option 2 (descriptor-declared identity binding).** It changes `SourceDescriptor` in `crates/knobas-source/src/**`, frozen by §10.8; your own triage says it is an ADR + orchestrator ruling. Not taken; noted as the right move only if an adapter ever spells its identity key differently than `username`.
- **#82, the mechanism divergence itself.** I ruled it because the ticket needed an implementable answer and the backend premise was contradicted by code, but you sketched backend-side, so treat my choice as the most reversible reading of your option 1, not as settled doctrine.
- **#92, finding 3 ("finished UNKNOWN" for canceled builds).** Whether that is the right user-facing string for a canceled build is a product-wording call. The live suite certifies current behaviour; I did not rule the wording.
- **#84's null-`finished_at` edge** (a run that died mid-run has no outcome to synthesise an ending from). ADR-0005 does not answer it; if the implementer hits it, it comes to you.
- **#91's residual risk**: keeping any replaced-server detection at all, should the by-id probe ever 404 for permission or cleanup reasons on a live server. Named on the issue as the observation that sends it back to you.
- **No milestone-exit judgements, no label or milestone changes, nothing merged, no scope pulled into or out of M2.** All seven issues keep their labels as found.

Comment links: [#91](https://github.com/BFoerschner/knobas/issues/91#issuecomment-5460076645) · [#92](https://github.com/BFoerschner/knobas/issues/92#issuecomment-5460076738) · [#81](https://github.com/BFoerschner/knobas/issues/81#issuecomment-5460076806) · [#84](https://github.com/BFoerschner/knobas/issues/84#issuecomment-5460076870) · [#82](https://github.com/BFoerschner/knobas/issues/82#issuecomment-5460076943) · [#93](https://github.com/BFoerschner/knobas/issues/93#issuecomment-5460077013) · [#69](https://github.com/BFoerschner/knobas/issues/69#issuecomment-5460077084)

---

## Amendment to #81 — the caps' numbers follow the record target (PR #108)

**Appended after the seven rulings above.** This amends my own #81 ruling; the earlier one did not anticipate it. PR #108 implemented page-until-empty and its implementer surfaced, rather than decided, a consequence: the `MAX_*_PAGES` caps are a *request* budget, a walk now spends its last request on the empty page that proves the end, so a cap of 20 carries 19 pages of records. 950 repositories or branches walk cleanly where 999 used to; 951 fails with `cap_reached`. My ruling had said the caps "stay as the loud bound", and the note under it said "unchanged" — a word written when a request count and a record count were the same thing, so it could not distinguish the two readings the PR forces apart. The implementer was right to leave the constants alone and flag it.

**Ruling:** the caps keep their purpose, not their digits. `MAX_LIST_PAGES` and `MAX_BRANCH_PAGES` go from 20 to 21; `MAX_PR_PAGES` and `MAX_COMMIT_PAGES` stay at 20 because the budgeted walks never reach a 21st request (their budget breaks the walk on the last record of page 20). Keep the literals rather than deriving them from `PAGE_SIZE` in code: a formula would re-import the constant into `sync.rs` and put the honoured-page-size assumption back in executable form, against the PR's own invariant that `sync.rs` no longer mentions it. The derivation lives in the doc comment instead.

**Reasoning:** the 20 was never a chosen request count. `main`'s only rationale for the two constants is "At 50 per page: 1,000 repositories, and 1,000 branches per repository", so 20 was arithmetic from a 1,000-record target, and when the arithmetic's premise moved, the honest fix keeps the target and redoes the arithmetic. The regression is loud but the `owners[]`/`repos[]` lever is useless for its likeliest victim, a single long-lived repository past 950 branches, which can only be excluded, not narrowed. At 21 the capacity is 1,000 clean and 1,001 fatal; the old code failed at exactly 1,000 because a full page could not prove the end, so the bump delivers the comment's promised 1,000 for the first time (the PR's "restores the old capacity exactly" is off by one, in the good direction). Nothing in contract §4 speaks of walk caps, so nothing frozen moves.

**Also ruled on PR #108, both mine under the delegation:** `cap_reached` now reporting the walked count instead of `cap * PAGE_SIZE` is correct and stays (the old arithmetic assumed exactly what #81 removed, and would have told a user of a 10-capping server to narrow a 200-record source it called 1,000); and the implementer's refusal to add a null-tolerant decode for `/repos/search`'s `data` field is right (Go marshals a `make(...)`'d slice as `[]`, never `null`, and every live run now exercises the past-the-end request, so if that ever breaks, `just gitea-live` fails loudly and the tolerance change becomes evidence-backed instead of a guess that papers over the real "no data array" case).

**If you disagree, the cost of reversing this is:** trivial before merge — two constants, the three numbers in `a_cap_fires_at_exactly_the_boundary_it_names`, and the doc comments naming 950. After merge it is the same edit plus a changed loud-failure boundary users may have seen. Full text with quotes: [PR #108 comment](https://github.com/BFoerschner/knobas/pull/108#issuecomment-5460443477).

---

## The second batch — #105 and #106

**Appended under the same delegation.** These are the two triage questions from the TeamCity live session. Both came out of the first real-server contact (the session that found #91 and #113) and both carried `needs-triage` because each poses a product question. Both rulings rest on read-only measurements I made today against `https://teamcity.jetbrains.com/guestAuth` (2026.2 EAP), quoted in full on the issues. Nothing in either touches a §10.8 frozen surface; the one option that would (in #106) is flagged for you instead of ruled. I may not change labels: my recommendation for both is `ready-for-agent`.

### #105 — canceled TeamCity builds are permanently absent from the mirror

**The fork:** mirror them; deliberately do not and say so; mirror-but-render-distinctly. The issue allows that exclusion "may well be *correct* product behaviour".

**Ruling:** fork option 1, widened and narrowed. Canceled **and** failed-to-start builds enter the mirror; personal builds stay out. Mechanism: `canceled:any,failedToStart:any` on the two item-producing locators (per-configuration full sync and the incremental `since()`), not `defaultFilter:false`; probe and in-flight poll unchanged. And the wording I declined to rule this morning is now ruled, because the path becomes reachable: `UNKNOWN` never reaches a user — a canceled build's status element renders `finished canceled` (`statusText: "Canceled"` was already indexed, `payload` stays verbatim).

**Reasoning:** the decisive argument was on nobody's fork: exclusion does not produce "a mirror of the server's default view", it produces permanent lies. Step 5 of `sync.rs` mirrors in-flight builds and `cursor::advance` clamps the watermark under them (`next.min(oldest_in_flight - 1)`); a mirrored running build that is then canceled is never overwritten — the finished queries hide it, the poll stops returning it, and "M1 has no deletion channel" (map.rs). It says "running" forever. The same strands a queued build that fails to start, which is why that class comes in too. Personal builds are the opposite case: the default filter hides them from the in-flight poll as well, so their absence is consistent and no row goes stale — and nobody decided a work cockpit should mirror other people's experiments. The mechanism choice is measured, not guessed: in one window, `canceled:any` re-included exactly the canceled build and nothing else, `defaultFilter:false` also admitted a `failedToStart` build (and opens the personal facet and every unenumerated one), and `canceled:any,failedToStart:any` admitted both decided classes and no personal builds. Healing is bounded, not total: pre-fix casualties below the watermark come back only as far as a full sync's window reaches — run one after the fix lands.

**Escalation triggers left on the issue:** a personal build ever served by the in-flight poll; either dimension observed disabling more than its own facet; and fork option 3's distinct rendering, which is a UI layer on top of this and yours to want or not.

**If you disagree, the cost of reversing this is:** moderate before implementation — one comment. After: the adapter and mockd changes revert mechanically (nothing frozen — neither crate is in §10.8's list, and the contract edit is an amendment entry beside #91's, superseding §4.2's locator row rather than editing it), but reverting re-opens the stale-running-row defect this closes, and canceled builds that entered users' mirrors in the meantime would need a decision of their own (tombstone or leave). The wording alone (`finished canceled`) is trivial to change at any time.

### #106 — TeamCity items carry no author, so #39's author:/@ tokens do nothing for that source

**The fork:** accept and document; fall back to the change's committer; make the emptiness visible in the search surface.

**Ruling:** option 1 — accept and document, plus the AC's realistic fixture. `author` keeps contract §4.1's meaning ("the source's username string"): the person who deliberately triggered the build, sparse by nature. Option 2 rejected on measurement. Option 3 **not ruled — flagged for you**: it changes `SearchResponse`, which is IPC schema and §10.8-frozen; I think it is the right eventual answer to the silent-empty-result problem and should become its own issue if you want it.

**Reasoning:** the issue's feasibility condition for option 2 holds and its payoff does not. One request with `changes(change(username,user(username)))` in `fields=` answered 100 builds in 9.8 KB — no per-build round trip — but 92/100 builds carried zero changes (87/100 were `snapshotDependency`-triggered; not one `vcsTrigger` in the window), and the 8 that had changes named committers as VCS display strings ("artem tikhomirov") with the TeamCity account (`change.user.username`) null throughout. So the fallback fills ~8% of builds, in a name-space that never matches `@me` (vocab.rs resolves identity from the config `username`, a TeamCity login), at the price of "author" meaning two different things per build. §4.1's own parenthesis — "display-name mapping is M2's people work" — is the schedule for revisiting this properly.

**Escalation triggers left on the issue:** a corpus where `triggered.user` is dense (your own server would raise option 3's value); M2's people work landing, which legitimately re-opens committer authorship with the vocabulary problem actually solved.

**If you disagree, the cost of reversing this is:** near zero — it is documentation and fixtures; no adapter, field, cursor, or schema change. Choosing option 2 later loses nothing done under option 1, and the fixtures it requires ("overwhelmingly null") are the ones option 2's tests would want anyway.

Comment links: [#105](https://github.com/BFoerschner/knobas/issues/105#issuecomment-5460907220) · [#106](https://github.com/BFoerschner/knobas/issues/106#issuecomment-5460907457)

---

## The third batch — #127 and #137

**Appended under the same delegation.** These are the two questions raised by PR #126's merge-manager reviews; both came out of the #119/#120 merge (PR #126, `01cafa3`). One of them opened with a jurisdiction question — whether it was mine to rule at all — and the answer turned on which fork touches §10.8. Both rulings rest on code read today and quoted on the issues; the two merge-manager measurements #127 leaned on were verified in the code rather than taken on trust.

### #127 — `delete_source` does not cancel the source's in-flight run

**The fork:** four options; the issue leans option 3 (make the orphan harmless) because it may need no IPC change; options 1 and 2 change `delete_source` on the frozen surface.

**Ruling:** option 3, settle-sweep shape — and it is mine, because this shape touches nothing frozen. `forget_source` carries the purge intent; when the one in-flight run settles, the scheduler re-applies the same purge CTE `crud::delete` uses (delete the source's `sync.item` rows, tombstone their entities). In-memory intent only — the sole writer is the run's own uncommitted transaction, which a process death rolls back server-side. `add_source` re-creating the id clears the pending intent (newest instruction wins), with the late-writes-into-a-namesake residual documented there. Recommended complement, not a substitute: a second config-row existence check at the bottom of `run_locked`, mirroring the top-of-run `NotConfigured` refusal, so the common case rolls back instead of writing-then-sweeping. ADR-0005 untouched by construction — the sweep never goes near `Watchers`. Options 1/2 explicitly not granted; the FK variant of option 3 refused twice over (migrations are frozen, and an FK cannot coexist with `purge_items: false` keeping the rows).

**Reasoning:** both merge-manager measurements verified — the `runs` map mutates only in `trigger` and `forget_source`, and `run_locked` holds `pg_advisory_xact_lock(hashtext($1::text))` for the whole transaction — so exactly one writer can produce the orphan. Option 4 is refuted by reading: the run's only existence check is at the top of `run_locked`, before any network work, so under READ COMMITTED the window is nearly the run's whole duration; and the damage is worse than orphan rows, because the sink's upsert sets `deleted_at = excluded.deleted_at` and so resurrects the entities the purge just tombstoned — deleted items back in search, permanently, with no source left to ever correct them.

**If you disagree, the cost of reversing this is:** small before implementation — one comment. After: the sweep and the intent plumbing revert mechanically (nothing frozen moved, which was the point), but reverting re-opens a user-visible defect, and choosing option 1 or 2 instead is not a reversal so much as an escalation — it needs your §10.8 ratified-exception entry either way, which is exactly why it was not chosen while a non-frozen shape sufficed.

### #137 — the DONE panel's fallback still renders the run's count

**The fork:** keep the fallback; say the corpus is unknown; retry before falling back. Product wording — the first sentence knobas says about a new source — plus a genuine contradiction between #120's "on any interleaving" criterion and the tested behaviour.

**Ruling:** option 2. The criterion stands unamended; the behaviour moves. The `?? items` arm of the `mirrored` derivation goes; a corpus read that fails renders "knobas mirrored your items." with no number, under one enforceable constraint: no digit renders after the word "mirrored" that the panel cannot vouch for as a corpus count. Genuine zero still reads "0 items"; the demo path gets no exception; one optional immediate retry inside `readCorpus` is permitted, not required; the wizard timeout stays buried where ADR-0005 put it. The existing fallback test is rewritten, not deleted — its no-failure-panel half is still right.

**Reasoning:** every authority points the same way. ADR-0005: "'mirrored N items' means the corpus … not a run's `Upserted` delta". CONTEXT.md forbids the exact word: under Upserted, "_Avoid_: synced, mirrored (both name the corpus)". And #120's criterion says "on any interleaving" — a `list_sources` that throws is an interleaving of the same round trip. The code's own defence of the fallback ("the only other number there is") is true and wrong: when the only other number is one the panel has reason to distrust, the honest rendering is no number, and the tri-state already knows how to render less than a count.

**If you disagree, the cost of reversing this is:** near zero — one derived value, one sentence, one test, all frontend. If you want a different voice (an explicit "couldn't read the count", or a mandatory retry), that is a microcopy edit at any time; nothing structural hangs on it. The only thing worth guarding is the constraint itself — putting `Upserted` back after "mirrored" would re-litigate ADR-0005 through a fallback arm, which is how this issue happened.

**Labels (I may not change them):** #127 keeps `ready-for-agent`; #137 `needs-triage` → `ready-for-agent`. Both are implementable now; nothing in this batch was escalated as needing you first, and the escalation triggers that would send either back are listed on the issues.

Comment links: [#127](https://github.com/BFoerschner/knobas/issues/127#issuecomment-5461508014) · [#137](https://github.com/BFoerschner/knobas/issues/137#issuecomment-5461508235)

---

## The fourth batch — #146 and #148

**Appended under the same delegation.** These are the two forks implementers named and declined to guess at — honest stops: #146 from PR #145's implementer (who measured the flake both ways over six serial runs), #148 from PR #147's implementer (who found the inverse of the bug it was fixing one door along). One of them brushed a §10.8 frozen surface, and the jurisdiction question is answered in the ruling itself rather than assumed; the answer turned out to be "not needed, and had it been needed, yours."

### #146 — the contract battery flakes in the Gitea live suite

**The fork:** fix the battery's own assertion (frozen file, reaches all three adapters, §10.8 entry) or narrow the live suite's battery scope from `owners: [tidewater]` to something nothing mutates (test-only, changes what the battery certifies there). Plus the deeper question: is *"an idle run emits nothing"* too strong for any real server whose bookkeeping lags a write?

**Ruling:** the clause stands as written — it is conditional ("Incremental sync from the returned cursor yields no items **when nothing changed**"), and against the live container the condition is false: Gitea's settling `updated_at` is a real change, and the adapter emitting the repository is compliance, not re-delivery — #145's own `re_delivered` doc says so verbatim. So nothing frozen moves and no §10.8 entry exists. The fix is route 2 sharpened: battery scope becomes `{"repos": ["tidewater/ledger-api", "tidewater/ops-runbooks"]}` — *both* seeded repos the suite never writes, not one — with the loss written down twice (a doc comment on the battery test, and the file header's "What this file does NOT certify"). The owner-scoped full walk stays live-certified by `the_shapes_the_fake_only_assumes_are_certified_here` and the capped suite's `whole_owner()` walk; only the owner-scoped *idle pair* leaves live coverage, and it keeps its docker-free certification. Restated as a rule on the issue: a live-suite failure is never resolved by re-running (#35 task 8; #86 and #140 are the bill for the other habit), and any retry/tolerance loop in or around the battery is refused in advance.

**Jurisdiction, since you asked me to say which:** §10.8's letter ("an orchestrator decision and an update to this section") would cover me today, but the working model's human gate — you keep "any change to a frozen contract" — is stricter, and the battery is that contract's executable spec (your own doc: "The contract battery is the adapter's spec"); the care taken by ADR-0004's ratified exception in §10.8 — *"nothing more -- no battery clause added, removed or reworded"*, which is `docs/contract.md`'s wording, not the ADR's — points the same way. So: a battery *clause* change is yours. This ruling did not need one, which is why it could be made today.

**If you disagree, the cost of reversing this is:** small — one JSON scope literal and two doc blocks, all in test files; nothing frozen moved, which was the point. The one ratchet: if you later *want* the clause weakened (a tolerance for lagging servers), that is not a reversal of this but a fresh frozen-surface decision at your gate, and the measured evidence on #146/#145 is the file to bring. Escalation triggers left on the issue: the battery still flaking at the quiet scope (then the fix is seed-side quiescence — orchestrator, not you), and anyone reaching for a battery tolerance (you).

### #148 — the inverse of #144: a scheduler-discovered rejection written back to "ok"

**The fork:** the view subscribes to `source:health` and re-lists (keeps the store's contract), or `replace` keeps the newer `checked_at` per surviving row (fixes both directions at one seam, changes what `replace` means). The implementer recommended the second.

**Ruling:** shape 2, with the merge rule specified so nobody re-derives it: membership is the incoming set's absolutely (a source absent from `rows` leaves the store — which is exactly why the deleted-chip property and #147's pinned mutation survive: deletion removes the row, it never stales it); per surviving row the held reading wins only with a *strictly newer* `checked_at`, incoming wins ties, both-null, and held-null; compare as instants. No clock-skew caveat — both values are the backend's own `checked_at` column for the same source, so newest-wins compares one clock against itself. The decisive argument was not on either fork as filed: the boot-time `reseed()` has the identical race, and `health.svelte.ts`'s own comment ("the seed that follows is the newer reading") is currently a timing hope that this merge turns into a guarantee — shape 1 would have fixed the view and left the seed exposed. The contract change is made legible in the three comments that currently state the old contract (the `replace` doc, the one in `SourcesView.svelte`, the `start()` comment). #147's re-list stays — it refreshes the `SourceSummary` halves the store never holds — and #148 lands on top of #147, whose tests must stay green.

**If you disagree, the cost of reversing this is:** small before #148 is implemented — one comment on the issue. After: reverting the merge re-opens a silent defect (a broken credential going on looking fine — the surface whose whole job is the opposite), and if you prefer shape 1 instead, the newest-wins tests convert to re-list tests but the deletion and #144-direction tests carry over unchanged. The genuine ratchet is semantic: once callers rely on "replace cannot rewind a row", any future backend flow that legitimately resets health backwards without an event needs your ruling first — that trigger is on the issue, alongside the tie case (same `checked_at`, different content).

**Labels (I may not change them):** both `needs-triage` → `ready-for-agent`; #148 sequenced on PR #147. Nothing in this batch was escalated as blocking on you — #146's Björn-gated route was declined rather than needed.

Comment links: [#146](https://github.com/BFoerschner/knobas/issues/146#issuecomment-5461770884) · [#148](https://github.com/BFoerschner/knobas/issues/148#issuecomment-5461770968)

---

## The fifth and last batch — #154 and #156

**Appended under the same delegation.** These are the two findings deep-pass merge-managers judged too consequential to fix unreviewed. Both were found at merge time and correctly left: #154 by PR #149's merge-manager (a destructive-path behaviour question), #156 independently by PR #150's merge-manager and its Spec reviewer (a product-structure question). Neither ruling touches a §10.8 frozen surface — checked for both, and the checks are quoted in the comments. One of them (#156) required correcting the record first: a quote three pieces of work attributed to ADR-0005 is not in the ADR.

### #154 — an `add_source` committing inside `delete_source`'s own body

**The fork:** guard the purge on the source still being absent; reorder `delete_source`; or accept and write it on `delete_source`'s guarantee. The issue says accept is legitimate, and the fix is a behaviour change on a destructive path.

**Ruling:** fix it — the existence guard, inside `forget_source`, under the claims lock it already holds: before arming or applying a purge, `config::get(pool, source_id)`; `Some(_)` means a source exists under the id again, the user's newest instruction wins, and neither destructive act happens. `Ok(None)` is today's behaviour; `Err(_)` warns and proceeds as today, so a db blip cannot reopen #127's symptom — the misfire then needs the race *and* a read failure at the same instant, and that conjunction is the accepted residual, recorded beside #149's two on `delete_source`. `claims.runs.remove` stays unconditional (#119's rule: the entry never outlives its source; matching the sequential outcome), with the stripped-fresh-entry residual documented on `forget_source`. The `Purge` glossary gap is real and ruled closed: a `CONTEXT.md` head-word lands in the same PR (text in the comment — it defines the purge against **Sweep**, whose entry already forbids "purge" for the reconcile pass, so the vocabulary was half-pinned until this closes the loop).

**Reasoning:** I weighed accept seriously — the window needs two overlapping IPC calls on one id — and three things beat it. The worst shape is silent and not self-healing: the cursor lives on `source_config` and the purge removes only items, so the new source's corpus stays gone until a manual backfill, entities tombstoned out of every reader's view — worse than the #127 defect this machinery exists to close. The principle is already ratified in the code (`source_added`: "The user's newest instruction about the id wins"); #149 enforced it through one door and left the other open. And the guard is one read made complete by lock order: `source_added` takes the same claims lock, so every add either committed before the guard's read (the row is visible) or clears the armed intent afterwards (#149's door); no run of the re-added source can start inside the window because `trigger` reads `source_config` under this same lock. ADR-0005 untouched — the guard cancels nothing and never touches `Watchers`.

**If you disagree, the cost of reversing this is:** small in code — one read and three arms revert mechanically, nothing frozen moved — but reverting re-opens a silent purge-of-the-wrong-source's-corpus window on a destructive path, and the accept fork you would be choosing still owes the write-up on `delete_source`'s guarantee, so reversal is a swap of fix for documentation, not a deletion. The escalation trigger on the issue: if the honestly-interleaved test cannot be made (M8's refusal is the bar), the accept fork comes to you rather than the test getting weakened.

### #156 — the wizard's DONE step is reachable only from a demo load

**The fork:** make DONE reachable from a real sync; accept that the real path ends at step 2 and re-aim the ruled microcopy at the stats row; or give step 2 the sentence and let the demo keep its panel.

**Ruling:** fork 1, click-neutral — in the channel callback, `stepIndex = 3` on the `finished` ending; step 2's now-dead `finished` branch removed; failure stays at step 2 with Retry/Skip; demo unchanged; the component header updated so the next reader finds the decision. No new microcopy and no ADR change: the DONE panel already renders through `mirrored`, the derived that carries #120's tri-state, #137's "your items", and ADR-0005's corpus rule, so every ruled constraint propagates by construction.

**Reasoning, and the record correction:** the phrase "the first sentence knobas ever says to a new user…" is **not in ADR-0005** — I read the ADR end to end; it is my own #137 language and the component's doc comment. The ADR's actual consequence rules what "mirrored N items" *means*, not which panel renders it — so fork 1 amends nothing, while fork 2 (re-aiming #120/#137 at the stats row and re-reading the ADR's "user-facing" consequence) is a supersession of your ADR's reading and yours alone; I say that plainly in the comment and did not take it. On the product merits fork 1 also wins on evidence: the breadcrumb already promises the step to every user (`STEPS` ends in "Done"; `stepIndex = 3` is assigned only in `loadDemo`), §14a's fourth stage is the landing, and the change is click-neutral — today's finished step 2 offers exactly one button and so does DONE, so the issue's "another screen before the shell" cost does not exist.

**If you disagree, the cost of reversing this is:** one assignment and one restored branch in one frontend component — but choosing fork 2 instead is not a plain reversal: it carries the restatement work (rulings re-aimed at the stats row, the demo-only status stated in the component, the ADR consequence's renderer named), and that package is yours by the supersession logic above. Any rewording of the DONE sentence itself is also yours — I ruled reachability, not voice.

**Labels (I may not change them):** #154 keeps `ready-for-agent`; #156 `needs-triage` → `ready-for-agent`. Both are implementable now without you.

Comment links: [#154](https://github.com/BFoerschner/knobas/issues/154#issuecomment-5462288872) · [#156](https://github.com/BFoerschner/knobas/issues/156#issuecomment-5462288959)

---

## Closing, end of day

Fifteen forks ruled across five batches, every one posted on its issue with the overturn line, none touching a §10.8 frozen surface, and nothing moved *by me* — no label, no milestone, no merge. The `needs-triage` → `ready-for-agent` changes each ruling recommended were made by the orchestrator afterwards, within a couple of minutes each; all fifteen issues now carry `ready-for-agent`. If you have limited time, read in this order: **#82** (the one place I diverged from a mechanism you sketched), **#91** (the most consequential engine change), **#154** (a behaviour change on a destructive path — the only ruling that alters what a delete does), and **#156** (where I corrected a misattributed ADR quote and deliberately took the fork that keeps ADR-0005 literally true rather than the one that reinterprets it). Explicitly left for you, gathered from the day: fork 2 of #156 and the DONE sentence's voice; #106's option 3 and any `SearchResponse` change; #146's battery-clause gate; the #84 null-`finished_at` edge; #82's descriptor-declared identity; and any battery tolerance, ever. Everything else in this file is implementable now, and each section carries the observation that would send it back to you.
