# Review planning rulings — 2026-10-22, Fable as deputy

On 2026-10-22 a whole-repository review of `main` at `9bb64644` produced 119 findings (ten per-area reports; gate green on that head: 2,584 vitest and 3,650 cargo tests). Björn handed the planning pass to one agent unattended (handoff of 2026-10-22, `$TMPDIR/handoff-knobas-review-to-tickets.md`): grill the forks, write the specs, cut the tickets, make the documentation amendments he would otherwise gate, and rule every fork in his place, never asking. The shape is the 2026-10 batch's (`2026-10-unattended-rulings.md`): each ruling names the fork, the option taken, the reasoning and the cost of reversing. The rulings below are also in the `## Rulings` section of the spec they belong to.

The three specs: **#1618** (walks and queues that assume the remote stays still), **#1619** (lifecycle paths no test executes end to end), **#1620** (secrets and hardening). Everything the review found outside those three seams went to direct tickets, named in the planning report.

**Standing waiver for this batch (Björn, 2026-10-22, the handoff).** Björn is away and will only run `/loop /ticket-loop`. Nothing in this batch is filed `ready-for-human`, and the gate he normally keeps for frozen surfaces (`docs/agents/working-model.md`: milestone exits and frozen-contract changes) is waived for every ticket this planning pass filed. A ticket whose fix touches a frozen surface (a migration, an IPC type, `crates/knobas-http/**`, `crates/knobas-source/src/**`) is labelled `ready-for-agent`, says *"Reserved for Björn; waived by Björn 2026-10-22 (review handoff). Implementer on Opus rules the fork and records it"*, and carries its §10.8 entry in the same PR; the loop dispatches Opus for it on its own. Björn reverses any ruling here with a follow-up ticket.

---

## Documentation amendments made in this PR

- **ADR-0030** records that `english` is the only text-search configuration, with no `unaccent`, over a German and English mirror, names the known gaps and the migration shape a later change takes (finding SRCH-9). `CONTEXT.md`'s **Launcher** entry links it.
- **ADR-0029** no longer restates each role's write vocabulary: `role::MINIMUMS` in `crates/knobas-source/src/role.rs` is named as the normative list, and the amendment records how far the bullets had drifted (finding SPI-8).
- **Not made here, on purpose:** the stale `FilterCoverage` doc sentence (SRCH-8) and the stale stream map in `crates/knobas-app/src/commands/mod.rs`. Both are Rust doc comments, and the first is exported into the generated TypeScript mirror, so the edit regenerates `app/src/lib/ipc/generated/search.ts` and needs a cargo build the handoff ruled out; neither keeps this PR docs-only. They are one direct ticket.

## Process rulings

### P1 — Every finding is a `ready-for-agent` issue, a spec, or an explicit drop

**The fork:** the review's notes and low-confidence findings could go to `backlog`, as the loop-speed rule of 2026-10-14 sends internal follow-ups there.

**Ruling:** every finding is in exactly one `ready-for-agent` spec or ticket, except two that are dropped with a reason (LIFE-11, SPI-12, below). Nothing is `backlog`.

**Reasoning:** the handoff says every issue created is `ready-for-agent` and that Björn will not read anything before the loop runs; a `backlog` issue is one the loop promotes only when the frontier is empty, which for a review batch means the notes would wait behind everything. A `likely` or `possible` finding instead carries, as its first criterion, the instruction to reproduce or refute with a test and close the ticket with a comment if refuted.

**If you disagree, the cost of reversing this is:** relabelling.

### P2 — Frozen-surface findings are agent tickets under the waiver

**The fork:** CORE-4 (a migration), SPI-1 (an envelope the handoff thought might bump), IPC-2 (an IPC argument), SPI-2 and SPI-7 (`knobas-http`), ADPT-8 and SPI-6 (`knobas-source`) all touch surfaces §10.8 freezes and Björn gates.

**Ruling:** all are `ready-for-agent`, marked with the waiver sentence above, with `## Model: Opus` and the §10.8 entry required in the same PR.

**Reasoning:** the handoff waives the gate for this batch in so many words, and the loop already sends frozen-surface work to Opus.

**If you disagree, the cost of reversing this is:** nothing in code; a ticket not yet merged can be relabelled `ready-for-human`.

### P3 — Two findings dropped

- **LIFE-11** (an unauthenticated GitHub call per installed adapter at start): dropped. ADR-0028 says knobas tells the reader a newer release exists, which is this call; the review itself asks for nothing, and a setting to turn it off is a feature request no one has made.
- **SPI-12** (the artifact hash verifies integrity, not authorship): dropped. ADR-0028 accepts exactly this, and the review says so; a publisher key is a design of its own, not a defect.

## Spec #1618 — walks and queues that assume the remote stays still

### A1 — Ordering key for offset walks

**The fork:** page the Jira walk (and Gitea's listings) over `created` ascending, or switch to keyset paging on `updated` with the seen set as tiebreak.

**Ruling:** `created` ascending, with the time filter staying on `updated` (`lastmodified` for Confluence).

**Reasoning:** `created` never changes, so an edit cannot move a row across a page boundary; the only displacement left is a deletion, which A5 turns into a failed run. Keyset paging on `updated` needs a tiebreak the minute-resolution JQL and CQL literals cannot express, Jira's `/search` has no keyset form, and the seen set as tiebreak is the fingerprint machinery doing a second job. Confluence's positions sweep already chose `created` for this reason and says so in its comment.

**If you disagree, the cost of reversing this is:** one line per adapter and the walk's comment; a keyset walk is a new paging protocol in `knobas-http` with its own §10.8 entry.

### A2 — A 401-waiting write

**The fork:** skip a write whose last attempt answered 401 until the credential changes, or put it on the sync side's backoff ladder.

**Ruling:** skipped until the credential changes on that source (secret or account) or the reader presses *Flush now*; never re-sent by the tick. An `unreachable` wait climbs the sync ladder.

**Reasoning:** `docs/development.md` states the rule for syncs and the lockout risk is the same for a write; the ladder would still re-send a dead credential, only less often. The row's `wait_reason` is the gate and the source's health only a cheap skip, because a Kuma write can be unauthorized (its account) while its sync (its key) is healthy.

**If you disagree, the cost of reversing this is:** the `due` predicate and the release hook.

### A3 — Where the per-instance rate limiter lives

**The fork:** a keyed limiter in `knobas-http` shared by every client built for an instance, or a cached built `Source` per instance in the registry.

**Ruling:** the keyed limiter, keyed by instance origin (scheme, host, port), the stricter quota winning where two configurations disagree; the wasm host's sends await the same limiter.

**Reasoning:** the limiter is what the crate promises, and the keyed form is one change in one crate that every client inherits, the wasm host included. A cached `Source` changes the registry's lifetime rules, holds an instantiated component per source, and still leaves external adapters unlimited.

**If you disagree, the cost of reversing this is:** the keyed map and its §10.8 entry; a registry cache could be added beside it later.

### A4 — The shared minute-resolution watermark

**The fork:** lift a generic watermark out of Jira and Confluence now, or fix both copies.

**Ruling:** lift now, into `knobas-source`, as the spec's prefactoring ticket; stored envelopes stay byte-compatible and no cursor version moves.

**Reasoning:** the copies had already diverged on the ceiling clamp, which is one of this spec's fixes; fixing both copies lands it twice and leaves the next drift open. `knobas-source` holds the other shared adapter pieces; `knobas-http` is about the wire. Every cursor bump forces a full sync of every source, so a behaviour-preserving lift must not bump.

**If you disagree, the cost of reversing this is:** moderate; the adapters get their modules back and nothing on disk changes.

### A5 — A shrinking `total` fails the walk

**Ruling:** when Jira's `total` at the end of a walk is below its start, the run fails, as a reached cap does; it does not re-walk.

**Reasoning:** a failed run costs one re-fetch on the next tick and keeps the sweep honest; a re-walk doubles the run on a busy instance and still ends somewhere.

**If you disagree, the cost of reversing this is:** one branch.

*Amended by #1979 (2026-10-24): a shrinking `total` no longer fails the walk.* The check missed an issue deleted and another filed between two pages (the total stays put and the hole is there), and it failed the run on a deletion that skipped nothing, which on a busy filtered source can be every attempt. The keyset that would have replaced it is not offered: JQL's `id > N` is an alias of `issuekey`, so it ranges inside issue N's project, and a deleted `N` is refused (measured on the seeded Jira 10.3). Instead every page after the first is asked one row early, from the last issue read, and accepted only when it opens on an issue the walk has read; otherwise the walk asks again from further back (`crates/knobas-source-jira/src/walk.rs`). A deletion costs a request or two and fails nothing. Confluence's page walk does the same (#1976). Do not copy the shrinking-total check.

### A6 — Gitea budgets on incremental runs

**Ruling:** an incremental walk continues down to the watermark; when the budget stops it first, the walk keeps its floor (the old watermark, which still means everything at or before it is delivered) and records the oldest examined record as the resume point. The next run reads the window between the two, continuing below the oldest examined record, and adopts the newest examined record as the watermark once it reaches the floor. *Amended by #1715:* the first wording, "the watermark is the oldest record examined", is wrong for a newest-first walk, since it makes the next run stop exactly where the unread records begin and skip them. Do not copy it. What #1624 built, and why, is in `docs/decisions/2026-10-unattended-rulings.md`, "#1624/#1625 review rulings".

**Reasoning:** ADR-0003's budget bounds a full sync; on an incremental run it may bound one run's cost but never skip the delta.

**If you disagree, the cost of reversing this is:** the `close_watermark` arithmetic.

### A7 — The flush beside the ticker

**Ruling:** `flush_all` runs on its own task, woken by the ticker, selected against the cancel token between writes; the per-source lock is held per write.

**Reasoning:** scheduling must not sit behind a 45-second send; cancelling between writes narrows the at-risk window to the one request on the wire ADR-0012 already discloses.

**If you disagree, the cost of reversing this is:** the task split.

### A8 — A cancelled run

**Ruling:** the cancelled verdict neither backs off nor counts as a failure; the row still ends (ADR-0005).

**Reasoning:** a cancellation is the reader's act, and backing off for it punishes the large first sync most.

**If you disagree, the cost of reversing this is:** one arm in settle.

### A9 — External adapters and the limiter

**Ruling:** the wasm host awaits the instance's shared limiter with the http crate's defaults; Retry-After handling stays out of the host.

**Reasoning:** the host owns the socket and the credential, so it owns the quota; Retry-After is an optimisation an adapter can make from the status it sees.

**If you disagree, the cost of reversing this is:** one await.

## Spec #1619 — lifecycle paths no test executes end to end

### B1 — Background sends at quit

**The fork:** await background sends at quit, or refuse new ones once cancelling.

**Ruling:** both: cancel first (no new send spawns; a submit after cancel stays pending), await the join set under the existing three-second grace, then close the pool. A send that outlives the grace is the ADR-0012 window.

**Reasoning:** awaiting alone is unbounded against a 45-second send; refusing alone leaves the one in flight to race the pool close.

**If you disagree, the cost of reversing this is:** the join set and two lines.

### B2 — A second window on a live owner

**The fork:** refuse the second launch, or hand ownership over when the owner quits.

**Ruling:** refuse, with a message naming the owning pid and profile; the demo profile's single-instance exemption stays and `docs/development.md` says what a second demo launch sees.

**Reasoning:** `docs/development.md` already promises a second launch does not touch the database; a handover needs the owner's stop to know its borrowers and the borrower to re-adopt, which is the complexity the review found unowned.

**If you disagree, the cost of reversing this is:** a developer loses `just dev` beside a packaged build on one profile; the handover is a lifecycle change of its own.

### B3 — A failed restore

**The fork:** snapshot and re-insert the bookkeeping rows, or restore into a scratch database and swap.

**Ruling:** snapshot and re-insert, the shim column by the same mechanism, witnessed by one test with a bogus `COPY`.

**Reasoning:** the UI pool and the scheduler are live during a restore; a database rename under live pools is far larger than the defect, and the recovery is local to `backup.rs`.

**If you disagree, the cost of reversing this is:** the recovery block; the test keeps its assertion.

### B4 — The bring-up seam

**Ruling:** `bring_up` and `shutdown` are `test-util` functions driven on a `tauri::test` mock app against the shared test server, the way the IPC seam suites build state; not a hand stub of the app.

**Reasoning:** a second lifecycle drifts from the first.

**If you disagree, the cost of reversing this is:** low.

### B5 — The stale-lock rule

**Ruling:** an authentication refusal from the recorded port with a dead recorded pid is a stale lock; a live pid still refuses.

*The live-pid half was reversed by #1775 (2026-10-23): with the ownership lock free, an answer that is not ours clears the lock whatever the pid is doing. See `2026-10-unattended-rulings.md`, #1775.*

**Reasoning:** a postmaster of ours accepts our password, so a refusal plus a dead pid is the evidence the wire-level arm already accepts.

**If you disagree, the cost of reversing this is:** one arm in `adopt`.

### B6 — The demo flag and the guard

**Ruling:** the plugin's by-bundle keying stays; the refusal (B2) lives in the embedded lifecycle and covers the demo case; the flag half of the profile is built once before the builder.

**If you disagree, the cost of reversing this is:** a per-profile socket could be added later without removing the refusal.

## Spec #1620 — secrets and hardening

### C1 — The keychain envelope

**The fork:** owned `String` or `Cow` with `#[serde(borrow)]`; whether a v3 envelope is needed.

**Ruling:** owned `String` for `secret` and `kind`; no version bump; `READABLE_VERSIONS` and `ENVELOPE_VERSION` stay.

**Reasoning:** serde writes the same bytes for `String` and `&str`, so nothing written before the fix is new to the fixed reader and nothing written after is new to an older one; a version marks a shape change, and this is a reader bug the module already fixed the same way for `StoredAccount`. A v2 reader reads what v2 wrote, and now also what v2 wrote and could not read.

**If you disagree, the cost of reversing this is:** two field types.

### C2 — A redirected write

**The fork:** follow a same-origin redirect on a write as the original method, or refuse it.

**Ruling:** GET and HEAD follow; a redirected write fails with `Protocol` naming the `Location`, and the queue records the refusal with that reason.

**Reasoning:** under ADR-0012 a write that reads as success must have happened; re-issuing the method after a redirect is a second guess about the proxy, and the reader's fix is the base URL.

**If you disagree, the cost of reversing this is:** the redirect policy and one test.

### C3 — The workflow pins

**Ruling:** an agent ticket; the witness is one `gh workflow run check.yml --ref <branch>`; `release.yml`'s input plumbing is witnessed by reading the YAML, which the ticket says.

**If you disagree, the cost of reversing this is:** none.

### C4 — `open_checkout`'s program

**The fork:** validate the webview-supplied template, or drop the argument.

**Ruling:** drop it; *Try it* saves the draft first and runs by id; the child's environment drops every `KNOBAS_*` variable.

**Reasoning:** ADR-0016 says the program comes from a setting, and the only way that is true is for the command not to take one; validating a program is a denylist with no end.

**If you disagree, the cost of reversing this is:** the argument and its §10.8 entry return.

### C5 — `testenv/.env`

**Ruling:** renamed to `testenv/images.env` and passed with `--env-file`, not untracked.

**Reasoning:** the pins are needed on every fresh clone.

**If you disagree, the cost of reversing this is:** a rename.

## Direct tickets

### D1 — `-kind:`, `-source:`, `-under:` and `-@x` (SRCH-2)

**The fork:** implement negation for every key, or report an un-negatable key as an unknown token.

**Ruling:** report `-<key>:` as an unknown token for every key the grammar knows but cannot negate; the pinning test that calls the demotion intended is rewritten.

**Reasoning:** the module's own doc says the grammar never demotes a filter to a search word; negation for `kind:` and `source:` is a grammar feature with its own corpus and perf questions, not a review fix.

**If you disagree, the cost of reversing this is:** a feature ticket.

### D2 — `log_all` (IPC-1)

**The fork:** make the Log-all UI call `log_all`, or retire the command.

**Ruling:** retire it with a §10.8 entry, in the shape of #1578's and #1611's retirements.

**Reasoning:** the UI has sent one `log_work` per ticket since it was built, and two paths with different failure semantics is the defect; the repo's direction on uncalled commands is retirement.

**If you disagree, the cost of reversing this is:** the command returns under a new entry.

### D3 — The start-work picker and `fieldsOf` (PAGE-10)

**Ruling:** the picker's behaviour stands; `fieldsOf` offers `entity` and the comment that says it must not goes. The implementer first checks that `start_work_amend` accepts an entity amendment; if it refuses, the repository dropdown is already broken and the ticket says so.

**Reasoning:** the dropdown is what readers use.

**If you disagree, the cost of reversing this is:** a dedicated command for the repository change.

### D4 — Lock waits against the Bash ceiling (TOOL-3)

**Ruling:** `just gate` stays foreground with its slot wait capped so wait plus the p90 gate stays under 600 s (240 s), the recipe exiting with its own code and the holders' pids when the wait expires so a timeout is never read as a red gate; every live recipe that queues on a machine-wide lock runs in the background with Monitor; both briefs say so in one sentence each.

**Reasoning:** the gate's p90 fits one foreground call; a 45-minute JVM queue never will.

**If you disagree, the cost of reversing this is:** two numbers and two sentences.

### D5 — Docs-only self-merges and the gate (TOOL-4)

**Ruling:** a docs-only self-merge runs `just contract-index --check` and `just conflict-markers` (seconds, no `node_modules`) before merging; `check-front` is not required.

**Reasoning:** those two are the checks that guard docs; `check-front` needs a frontend tree a docs-only worktree does not get.

**If you disagree, the cost of reversing this is:** one line in the loop file and the brief.

### D6 — Jira's board re-walk (ADPT-11)

**Ruling:** the keys-only page size goes to the server's maximum (1,000 on Data Center, verified live), and the views read runs when the issue walk emitted anything or an hour has passed since the last board read, recorded as an optional field in the cursor with a default so the cursor version does not move.

**Reasoning:** a board's filter can change membership with no issue changing, so "only when the walk emitted" alone would miss an edited filter; an hourly floor bounds the cost without losing it.

**If you disagree, the cost of reversing this is:** one field and one branch.

### D7 — Field discovery (SYNC-7)

**Ruling:** a per-process memo that discovery answered nothing for a source; no persisted marker, no migration.

**If you disagree, the cost of reversing this is:** none.

### D8 — `DbState::Migrating` (LIFE-9)

**Ruling:** `Migrating` covers `migrate::run` only; the steps after it report `Starting` with its free text naming the step. No wire shape changes.

**If you disagree, the cost of reversing this is:** a few strings.

### D9 — `WasmAdapter::build` (SPI-3)

**Ruling:** one helper in the host side runs `Registry::build` under `spawn_blocking` at the four call sites; the frozen `build` signature stays synchronous; descriptor caching is not done now.

**Reasoning:** the SPI is frozen and the fix is at the callers; with A3's keyed limiter no `Source` caching is needed.

**If you disagree, the cost of reversing this is:** the helper.

### D10 — The mockd arms (ADPT-9)

**Ruling:** ordinary agent work; ADR-0013 already rules that the live seeded suite is the witness and the mockd arms are retired once it asserts what they assert.

### D11 — The search configuration (SRCH-9) and the role vocabularies (SPI-8)

**Ruling:** closed by this PR's ADR-0030 and ADR-0029 amendment; no tickets.
