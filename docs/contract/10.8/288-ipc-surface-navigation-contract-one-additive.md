- **The IPC surface and the navigation contract, issue #288 (2026-09-03): one additive command on the
  entity module — `standup_digest` (`today` + `earlier` day windows → `StandupDigest`) — two DTOs,
  one line on each append-only barrel, and `#/standup` graduating from a reserved address to a
  view.**

  **One command, and deliberately not a module pair.** Spec #272 settles the layout question in as
  many words: *"Time's IPC. One §10.8-ratified exception for a `time` module pair on both sides of
  the bridge […] Standup and Confluence reads go into the entity module as usual."* So this is a
  section in `crates/knobas-app/src/commands/entity.rs` and a block in
  `app/src/lib/ipc/entity.ts`, the same treatment `mini_board`, `list_projects` and the inbox reads
  got, and the `commands/` + `ipc/` layout is unchanged. `crates/knobas-app/src/lib.rs`'s handler
  list and `app/src/lib/ipc/index.ts` are the two append-only barrels: the first gains
  `commands::entity::standup_digest` at the end of the list, the second gains nothing at all
  (`export * from "./entity"` already carries it).

  **`crates/knobas-app/src/standup.rs` is where the rules live**, beside `inbox.rs` and for the same
  reason that file gives: a `#[tauri::command]` cannot be called from a test, so anything worth
  asserting has to be reachable without one. It is **not** a frozen surface — the list above freezes
  `commands/` and `ipc/`, not the crate's own decision layer — and it is not in `knobas-core`
  either, because two of its three producers (`knobas.timer`, `knobas.worklog`) are the app crate's
  own tables and `knobas-core` knows nothing about them.

  **No migration, no `crates/knobas-source/src/**` change, no event.** The digest reads
  `sync.live_item`, `knobas.activity`, `knobas.worklog`, `knobas.timer`, `knobas.confirmed_link` and
  `knobas.entity`, all of which exist; #274's design-doc correction is exactly this — *no per-event
  activity writer is added*. `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs`
  are untouched: the only failures are `not_ready` and query failures, which `IpcError` already
  carries.

  **The argument shape is `time::week::DayWindow`, reused rather than redeclared.** The webview
  computes the days — a date and the two instants it spans — for `day_blocks`' and `week_timesheet`'s
  reason in full: the machine's timezone is a fact only that side holds, and one UTC offset is wrong
  for every day containing a daylight-saving change. `app/src/lib/ipc/entity.ts` therefore imports
  `DayWindow` from `./time`; a second declaration of one wire shape is the drift the queue's own
  cross-module import already avoids.

  **What is *not* the caller's to decide: how far back the rule looks.** `standup::LOOKBACK_DAYS` is
  seven, and the cap is applied to the windows' **dates** rather than to their number. Those are not
  the same rule: "the newest seven of whatever arrived" holds `CONTEXT.md`'s *"at most seven days
  back"* only for a caller that happens to send seven consecutive days, so one window dated a
  fortnight ago would quietly reach a fortnight back. `in_reach` filters by date and sorts, so the
  sentence is true of any list in any order.
  `a_week_of_silence_has_no_yesterday_and_the_seventh_day_is_still_in_reach` asserts both sides of
  the number — a mutant that made it eight dies there — and
  `only_the_seven_days_before_the_digests_own_are_in_reach_of_yesterday` asserts the date rule
  itself, on a list a positional cap would get wrong.

  **Blockers read the declaration, never a word.** `KindPaths::blocked_statuses` (#277) and the
  declared `assignee` and `status_name` paths, resolved inside the statement by `declared_string!`
  and `declared_array!` and matched case-insensitively — no list of English status words exists in
  this module and a unit guard refuses one. That is what made the command take the **injected**
  registry, the shape `inbox_items` has: a battery that could not hand over a descriptor could only
  witness the declarations the shipped adapters happen to carry, which is a battery a hardcoded list
  passes. The fixtures therefore declare `Waiting for support` and leave a second ticket standing in
  `Blocked`, which must be absent. The link half is `knobas.confirmed_link` and the relation
  `blocks`, joined at its **`to`** end — the blocked item — and only that relation: `depends-on` is a
  different word the user chose and reinterpreting it would be knobas deciding what it means.

  **A finding worth recording, because it changed the wire: on Jira, §4.1's `author` is the
  *assignee*.** `knobas-source-jira/src/map.rs` maps it that way (Confluence maps the version's
  author, Gitea a commit's), so the digest's mirror half lists what a source says is *the reader's*,
  which is not the same claim as *the reader wrote it*. The verb is therefore **`attributed`** and
  the line reads "jira attributes this ticket to you" — a line saying "you authored this ticket"
  would put a colleague's transition of the reader's ticket on the reader's standup under the
  reader's name, which inverts story 63. The digest reads the normalization as declared rather than
  adding a rule of its own; being generous about an item that is the reader's own is the safe
  direction to be wrong in. The live tests witness both halves: the Jira one borrows a seeded ticket
  by assigning it (and leaves PAY-231, assigned to somebody else, as story 63's negative control
  against a real corpus), and the Confluence one asserts a seeded page on the digest for the day the
  mirror says it moved, writing nothing.

  **And one thing no producer carries, recorded rather than left to be discovered: a comment typed
  in a source's own UI.** A comment's author lives in the verbatim payload in the source's own
  shape, and `KindPaths` has no slot for it — reading one would mean a path guessed per source,
  which is the coalesce #277 spent a milestone removing. So the activity half carries the comments
  and transitions made *through knobas*, and a comment typed into Jira is absent until such a slot
  is ratified, at which point this read expires into the declaration like every other (ADR-0007).

  **`#/standup` is a view now.** It stays in the router's `RESERVED` set for the reason `#/inbox`
  and `#/time` do: a *kind* called `standup` must never claim the address. The head owns the whole
  address — `#/standup/anything` is this morning's digest — which keeps `#/standup/<date>` free for
  the standup **protocol**, a note per date and a different surface. The two router batteries move
  with it: the "later milestone" cases name `#/assets/board` now, because that is still one.

  Ratified by the orchestrator as spec #272 and issue #288, whose acceptance criteria specify the
  entity-module read, the declared blocked-like set, the view at the address and the live test.
  **Björn keeps the gate for frozen contracts and this entry is flagged for his review**, as
  `docs/agents/working-model.md` requires of any IPC change: one command and one barrel line.
