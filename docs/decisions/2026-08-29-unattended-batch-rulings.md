# Unattended batch rulings — 2026-08-29, Fable under delegation

Björn: you said "if any big decision needs to be made use fable to figure out the best way and write the decision down for me." Seven forks were ruled today; each ruling is posted as a comment on its issue, opening with a line saying you can overturn it. It was safe to decide these because none touches a §10.8 frozen surface (checked each against the list), none is a milestone-exit judgement, one (#84) was already decided by you via ADR-0005 and only needed confirming, and every ruling rests on code or issue text that is quoted in the comment — no ruling rests on a hypothesis about a document I did not read. The one place I diverge from a mechanism you sketched is #82, and it is flagged loudly both here and on the issue. Reading order if you have five minutes: #82 (the divergence), #91 (the most consequential), then skim the rest.

---

## #91 — TeamCity watermark ceiling deadlock

**The fork:** four options; the issue leans option 2 (derive the ceiling from the per-configuration pages the sync already fetches).

**Ruling:** option 2 amended, with 1 and 4 folded in. The ceiling becomes the max id over the *witnessed* pages only — a probe widened to `count:100` read as page-max, plus the global in-flight page — never the finished pages. The two-row ordering guard is deleted. The replaced-server refusal stays but re-founds its evidence: when everything observed sits below the watermark, fetch the watermark build by id (`/app/rest/builds/id:{id}`, already in the contract's endpoint list); 404 means replaced, found means proceed.

**Reasoning:** option 2 as written is unsafe — finished-page ids can post-date run start, so a ceiling built from them lets a build queued mid-run be overtaken by a later-queued build that finished inside the same run, which is exactly the silent loss the ceiling exists to close (`sync.rs`: "the ceiling saves builds *queued after* it"). Witnessed-page ids can only under-estimate, and an under-estimate costs a re-fetch, never a build. The old "newest < watermark ⇒ replaced server" check fires routinely on a server whose page ordering is arbitrary, which the live instance is; the by-id fetch is evidence ordering cannot fake, and it makes the "reset the cursor" remedy actually work.

**If you disagree, the cost of reversing this is:** moderate — it is all inside `knobas-source-teamcity` plus mockd fixtures, nothing frozen; but the doc comments this rewrites encode the loss-analysis, so a reversal should rewrite them too, not just the code. The one open risk I named on the issue: if a live run ever shows `builds/id:` 404ing for permission reasons on a build the source itself synced, the false refusal returns and the detection design is back on your desk.

## #92 — TeamCity live certification suite

**The fork:** same PR as #91 or separate; does #91's regression test depend on it; what to do with absent credentials.

**Ruling:** separate PR; no dependency in either direction (#91's regression test is offline in the fake; its live AC is discharged by a manual run or by this suite, whichever exists first). And a token is **not** required: I verified today, read-only, that `https://teamcity.jetbrains.com/guestAuth/app/rest/server` answers 200 with no credentials, ignores a bogus bearer header, and serves `users/current` too — and `knobas-http`'s string-concatenation URL building keeps the `/guestAuth` prefix, so this works with zero adapter changes. The suite runs token-less given the URL, passes a placeholder secret (the adapter refuses `None`), skips cleanly only when the URL is absent, and `.env.example` gains the guestAuth URL as its documented value.

**Reasoning:** #91 is deep-pass sync correctness, this is a standard-pass harness; your own PR rule says risky work wants its own bisect point. The guest finding removes the only operational obstacle to running it.

**If you disagree, the cost of reversing this is:** near zero — it is sequencing and a defaults choice, both changeable before or after the PR lands.

## #81 — Gitea paging terminates on a short page

**The fork:** three options; the issue leans option 1 (page until empty).

**Ruling:** option 1, once, for all four sites (line numbers have drifted to 331/491/586/808). The existing `MAX_*_PAGES` caps stay as the loud bound. The fake gains a capped-pages mode so "short because capped" is distinguishable from "short because exhausted".

**Reasoning:** the failure is silent and watermark-advancing — the class ADR-0003 exists to prevent — and the cost of closing it is one extra request per exhausted walk under a 10 req/s limiter. Option 2 trades one undocumented assumption for two; option 3 documents a hole instead of closing it.

**If you disagree, the cost of reversing this is:** trivial — four termination conditions and their tests, no cursor or schema impact.

## #84 — wizard DONE panel races the scheduler wake

**The fork:** you ruled "grill this before it becomes a ticket."

**Ruling:** *not mine* — the grilling happened and you decided it yourself this morning: ADR-0005 ("a run id always comes with an ending", commit 5c850cd), plus the agent brief on the issue. My comment confirms the ADR against the code and answers the issue's four questions from it (wake stays; wizard attaches via the multi-sink ending contract; "mirrored N" means the corpus per CONTEXT.md's Mirror/Upserted split; idle-until-tick is a real problem, judged so twice). One genuinely open edge I flagged for you: a run row whose `finished_at` is null forever because the process died mid-run has no recorded outcome to synthesise an ending from; ADR-0005's text does not answer what such a late attacher receives.

**If you disagree, the cost of reversing this is:** it is your own ADR, so reversal is an ADR supersession, not a comment.

## #82 — PAT user told contradictory things about `username`

**The fork:** your triage named three options and said an implementer should not choose alone; option 1 was "fill it backend-side in `add_source`."

**Ruling:** option 1's intent, but **in the dialog, not the backend** — and this is the one place I diverge from what you sketched. Your option 1's premise ("The command already has both the ConnectionInfo and the adapter") does not hold: `crud::add` never calls `test_connection`; only `set_secret`'s re-test has a `ConnectionInfo`. Backend fill at add would need either a second network call inside the save or a new `NewSource` field, which is IPC and frozen. Meanwhile the dialog cannot reach Save without a green test (`canAdvance` requires `report?.ok === true`), so `report.account` is reliably in hand at the right moment, for both entry paths (the wizard embeds `AddSource`). So: fill the schema-declared `username` form field from `report.account`, only when empty, keyed on the property *name* as a cross-adapter convention (all three adapters spell it `username`). Plus the text fixes: Jira's and TeamCity's "only for user + password" descriptions rewritten on Gitea's pattern (whose "Filled in by Test connection" promise finally becomes true), and `describe_missing_identity()`'s advice re-checked. Re-enter-path backfill for existing sources: explicitly left out, for you.

**If you disagree, the cost of reversing this is:** small if caught before implementation — the two viable backend shapes are named on the issue. After implementation, moving it backend-side re-does the tests but the text fixes and the only-if-empty rule carry over unchanged.

## #93 — no test joins engine + real database + real adapter

**The fork:** three open questions — home, `just check` membership, adapter count.

**Ruling:** `crates/knobas-app/tests/` (the dev-dependency worry is moot: knobas-app already depends on all three adapter crates as plain dependencies, and its tests dir already holds both halves separately); yes to `just check` (mockd is in-process and docker-free, embedded Postgres per binary is already the norm there, and an `#[ignore]`d seam test is a seam test nobody runs); one adapter now — Jira, discharging #32's criterion end to end — TeamCity as follow-up, Gitea excluded because mockd has no Gitea by standing decision and its equivalent belongs to the container/live layer.

**If you disagree, the cost of reversing this is:** low — moving the harness or pulling it out of `check` is mechanical; the only ratchet is that once it guards the seam, removing it from `check` re-opens the gap the issue documents.

## #69 — backup settings surface

**The fork:** genuinely blocked on a settings view, or build the surface itself?

**Ruling:** not blocked — the blocker is void (#36 closed without landing a settings view; `app/src/lib/` has none; no other open issue claims one), so #69 builds the minimum shell itself: `app/src/lib/settings/` with one `SettingsView` reached via the shell's existing navigation, titled sections in one scrollable pane, no tabs/router/registry, Backup as the only section — §14's plain dialogs over the four landed commands, defaults surfaced with your boundary-not-a-moment wording.

**If you disagree, the cost of reversing this is:** small — the shell is a thin frontend container; if you want a different settings IA later, the backup section moves into it as one component.

---

## Flagged: things I decided were NOT mine to rule

- **#82, option 2 (descriptor-declared identity binding).** It changes `SourceDescriptor` in `crates/knobas-source/src/**`, frozen by §10.8; your own triage says it is an ADR + orchestrator ruling. Not taken; noted as the right move only if an adapter ever spells its identity key differently than `username`.
- **#82, the mechanism divergence itself.** I ruled it because the ticket needed an implementable answer and the backend premise was contradicted by code, but you sketched backend-side, so treat my choice as the most reversible reading of your option 1, not as settled doctrine.
- **#92, finding 3 ("finished UNKNOWN" for canceled builds).** Whether that is the right user-facing string for a canceled build is a product-wording call. The live suite certifies current behaviour; I did not rule the wording.
- **#84's null-`finished_at` edge** (a run that died mid-run has no outcome to synthesise an ending from). ADR-0005 does not answer it; if the implementer hits it, it comes to you.
- **#91's residual risk**: keeping any replaced-server detection at all, should the by-id probe ever 404 for permission or cleanup reasons on a live server. Named on the issue as the observation that sends it back to you.
- **No milestone-exit judgements, no label or milestone changes, nothing merged, no scope pulled into or out of M2.** All seven issues keep their labels as found.

Comment links: [#91](https://github.com/BFoerschner/knobas/issues/91#issuecomment-5460076645) · [#92](https://github.com/BFoerschner/knobas/issues/92#issuecomment-5460076738) · [#81](https://github.com/BFoerschner/knobas/issues/81#issuecomment-5460076806) · [#84](https://github.com/BFoerschner/knobas/issues/84#issuecomment-5460076870) · [#82](https://github.com/BFoerschner/knobas/issues/82#issuecomment-5460076943) · [#93](https://github.com/BFoerschner/knobas/issues/93#issuecomment-5460077013) · [#69](https://github.com/BFoerschner/knobas/issues/69#issuecomment-5460077084)

---

## #81, amended later the same day — the caps' numbers follow the record target (PR #108)

**Appended after the seven rulings above.** This amends my own #81 ruling; the earlier one did not anticipate it. PR #108 implemented page-until-empty and its implementer surfaced, rather than decided, a consequence: the `MAX_*_PAGES` caps are a *request* budget, a walk now spends its last request on the empty page that proves the end, so a cap of 20 carries 19 pages of records. 950 repositories or branches walk cleanly where 999 used to; 951 fails with `cap_reached`. My ruling had said the caps "stay as the loud bound", and the note under it said "unchanged" — a word written when a request count and a record count were the same thing, so it could not distinguish the two readings the PR forces apart. The implementer was right to leave the constants alone and flag it.

**Ruling:** the caps keep their purpose, not their digits. `MAX_LIST_PAGES` and `MAX_BRANCH_PAGES` go from 20 to 21; `MAX_PR_PAGES` and `MAX_COMMIT_PAGES` stay at 20 because the budgeted walks never reach a 21st request (their budget breaks the walk on the last record of page 20). Keep the literals rather than deriving them from `PAGE_SIZE` in code: a formula would re-import the constant into `sync.rs` and put the honoured-page-size assumption back in executable form, against the PR's own invariant that `sync.rs` no longer mentions it. The derivation lives in the doc comment instead.

**Reasoning:** the 20 was never a chosen request count. `main`'s only rationale for the two constants is "At 50 per page: 1,000 repositories, and 1,000 branches per repository", so 20 was arithmetic from a 1,000-record target, and when the arithmetic's premise moved, the honest fix keeps the target and redoes the arithmetic. The regression is loud but the `owners[]`/`repos[]` lever is useless for its likeliest victim, a single long-lived repository past 950 branches, which can only be excluded, not narrowed. At 21 the capacity is 1,000 clean and 1,001 fatal; the old code failed at exactly 1,000 because a full page could not prove the end, so the bump delivers the comment's promised 1,000 for the first time (the PR's "restores the old capacity exactly" is off by one, in the good direction). Nothing in contract §4 speaks of walk caps, so nothing frozen moves.

**Also ruled on PR #108, both mine under the delegation:** `cap_reached` now reporting the walked count instead of `cap * PAGE_SIZE` is correct and stays (the old arithmetic assumed exactly what #81 removed, and would have told a user of a 10-capping server to narrow a 200-record source it called 1,000); and the implementer's refusal to add a null-tolerant decode for `/repos/search`'s `data` field is right (Go marshals a `make(...)`'d slice as `[]`, never `null`, and every live run now exercises the past-the-end request, so if that ever breaks, `just gitea-live` fails loudly and the tolerance change becomes evidence-backed instead of a guess that papers over the real "no data array" case).

**If you disagree, the cost of reversing this is:** trivial before merge — two constants, the three numbers in `a_cap_fires_at_exactly_the_boundary_it_names`, and the doc comments naming 950. After merge it is the same edit plus a changed loud-failure boundary users may have seen. Full text with quotes: [PR #108 comment](https://github.com/BFoerschner/knobas/pull/108#issuecomment-5460443477).
