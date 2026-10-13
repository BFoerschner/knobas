- **No migration, a twentieth `assets` command and one widened wire union, issue #446 (2026-09-07):**
  the inbox's sixth category, its ack, and the history line recovery leaves behind. **Ratified in
  advance by the spec (#427) Björn approved** — "**The inbox's sixth category.** `alert`, subject
  the monitor entity id, its rule a statement over open, un-acked alerts joined to the monitored-by
  link and to context membership of the linked asset through ancestors, with a negative control like
  the other five. Ack is the inbox item's completion plus the alert's acked-at plus a history line
  on the asset; recovery's close removes the derived item by construction and writes the history
  line. Snooze is the inbox's." — and, of the module pair, "the alert reads and **ack** from M4.1".
  Written with the implementing PR, per #428's, #431's, #434's, #435's, #439's, #443's and #444's
  pattern.

  **`0023` is still the next free number.** The #444 entry above expected this ticket to need no
  migration and it does not: `monitor_alert.acked_at` has been there since `0022` precisely so this
  one would not have to add a column, and the inbox stores nothing of its own beyond the
  `knobas.inbox_state` row every category already shares.

  **The wire change, in full.** One union grows and no DTO does:

  ```ts
  export type InboxCategory =
    | "review_request" | "mention" | "failed_build"
    | "new_assignment" | "credential_expiry" | "alert";   // ← the sixth
  ```

  mirroring `knobas_core::inbox::Category::Alert`. `InboxItem` keeps its ten fields, their names and
  their types; what changes is what two of them *carry* for this one category, and both are
  deliberate:

  * **`entity_id` is the affected asset, not the item's subject.** Every other category's subject
    and entity are the same mirrored item; an alert's subject is the **monitor** (spec #427's own
    words, and what the key `alert:<monitor entity id>` is built from) while what a reader opens is
    the **asset**, because story 61 is "opening an alert from the inbox lands in the Tree at the
    affected asset with the monitor in the pane". `kind` is therefore `asset`, and `InboxView`
    branches on the category to send `#/asset/<id>` rather than the `#/entity/<id>` room detail.
    Where a monitor watches several assets the rule picks one, by path then name then id, so the
    row opens the same asset on every read. Two consequences of that pick, named rather than left
    to be discovered: the inbox's **context filter** scopes by `entity_id`, so a monitor watching
    assets in two contexts shows its one item under the picked asset's context and not the
    other; and story 61's *"with the monitor in the pane"* is served by the pane's existing
    **Monitoring** section, which lists every monitor attached to that asset — the row's own
    title names the one that fell, and singling it out in the pane is #449's surface, not this one.
  * **`web_url` is null**, `OpenAlert`'s call and for its reason (#444): nothing knobas draws opens
    a monitor's page in Kuma, and reaching one would mean joining `sync.live_item`, which would drop
    a **paused** monitor's alert out of the inbox while it went on counting in the top strip.

  **The command, and what it takes.**

  ```rust
  #[tauri::command] pub async fn ack_alert(..., monitor_id: String) -> Result<assets::OpenAlert, IpcError>;
  ```

  Mirrored in `app/src/lib/ipc/assets.ts` as `ackAlert(monitorId)`. **One line appended** at the
  foot of `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, under #444's; neither barrel is
  rewritten and the `commands/` + `ipc/` module **layout is untouched** — no new module pair, which
  is the thing §10.8 freezes about that directory.

  **It takes the monitor and not the alert row's id, which supersedes the #444 entry's sentence**
  ("#446's ack and #449's cards address the row by this id"); the old sentence is left as history
  rather than rewritten, the treatment #53, #278, #428, #439, #443 and #444 give the sentences they
  supersede. Three reasons, in order: the inbox item's subject *is* the monitor entity, so the row
  already holds a complete address and asking it to translate one into a row id would be a second
  read of the alert list that can disagree with the one the row was drawn from;
  `monitor_alert_one_open_idx` makes "the open alert of this monitor" exactly one row or none,
  structurally, so the monitor is not a weaker address than the id; and #449's cards carry
  `OpenAlert::monitor_id` and are served by the same shape.

  **What the ack writes: three things, one transaction.** `acked_at` on the alert — which **leaves
  it open**, since only a return to `up` closes one, so the estate goes on saying the thing is down
  while the inbox stops saying it needs somebody; the inbox item completed through the new
  `knobas_core::inbox::complete_with` (`activity::record_with`'s shape and its reason); and one
  `acked` activity line per asset the monitor is **confirmed** to watch, actor `user`. **Acking
  twice is one ack** — an alert that already carries `acked_at` is answered with itself and writes
  nothing. Acking a monitor with no open alert is `not_found`, the refusal `inbox::answer` already
  makes for an item that left the stream.

  The inbox completion is belt and braces and is worth saying so: the rule excludes acked alerts, so
  the item is gone the moment the column is written. The row is what keeps *done* meaning one thing
  across all six categories, and what a later un-ack would have to clear.

  **Both lines land on every asset the monitor is confirmed to watch; the ack's does not narrow to
  the one the inbox row named.** Surfaced by this PR's spec review, which read story 63's *"an ack
  recorded on the asset's history"* as the singular affected asset, and decided the other way for one
  reason: **recovery has no context notion and must not grow one**. The recovery line is written by
  the sync engine, about an estate fact, with no reader and no inbox in view; if the ack narrowed to
  whichever asset the inbox row happened to open, an asset could carry a `recovered` with no `acked`
  before it, or an `acked` with no `recovered` after it, and its history would hold half a story with
  nothing saying which half. The inbox picking one asset is a property of *the row* — one demand, one
  way in — and not a claim about which machines the trouble was on. `OpenAlert::assets` on the ack's
  answer is the set its lines landed on, so a caller can see it. That set is the **confirmed-watch**
  set and deliberately not the routing rule's: an ack can write a line on an asset no context
  holds — one the inbox would never have routed — for the symmetry above, since the recovery line
  it pairs with is written with no context in view at all.

  **Recovery writes the other line, and `knobas_sync::alerts` is where it is.** The reconciler's
  close now records one `recovered` line per confirmed watched asset, actor `sync:<source_id>`,
  inside the run's own transaction — so a run that rolls back leaves no line claiming a recovery it
  never committed, and unlike the run's own `synced` line a failure here fails the run, because this
  one is part of the same write as the `closed_at` it describes. **Opening an alert writes no line**:
  an alert *is* the record that a monitor fell and it is drawn and counted from the moment it opens,
  where what recovery leaves behind would otherwise be unrecoverable — a closed alert is out of every
  open-alert read at once. `alerts::reconcile` widens from `pub(crate)` to `pub` so the app's inbox
  battery can drive the recovery half of its own seam without an adapter; `crates/knobas-sync`
  is not frozen (see below) and all fourteen tests in `tests/alerts.rs` still go through the engine,
  none of them calling the reconciler directly.

  **The membership walk is seeded twice and written once.** The rule needs *"is this asset a member
  of some context, directly or through an ancestor"* inside one statement that binds no context id,
  because the inbox is one derivation over the whole mirror and not a read per context — and #434's
  third criterion forbids a second walk in as many words. So `knobas_core::context` now holds the
  three layers after the seed (`direct`, `hop`, `held`) in one macro, used by both `MEMBER_IDS` and
  a new `held_by_any_context!`, whose seed is **every unarchived context at once**. Merging the
  seeds is exact rather than approximate: every layer after the seed is a *neighbour* expansion and
  neighbours distribute over union, so the walk from all seeds reaches exactly the union of the
  walks from each. `the_merged_walk_is_the_union_of_every_contexts_members` checks that against
  `member_ids` over `context::list`'s own contexts on a fixture with two overlapping walks, an epic
  seeded through its payload, and an archived context.

  **Archived contexts are the one place the two deliberately differ**, and it is a decision rather
  than a consequence: `member_ids` is asked about a context by name and answers about that context
  whatever its state, while this asks whether anybody is still working on something — and an alert
  that went on interrupting a reader because of a context they archived last spring would be the
  inbox failing its own promise. `context::list` draws the same line for the switcher. Recorded in
  `CONTEXT.md`'s **Alert** entry as a 2026-09-07 amendment.

  **`knobas_core::link::MONITORED_BY`**, new, because four places have to agree on that word and
  three of them are SQL: the alert rule, #478's `monitor_url_host` suggestion, the recovery line,
  and `knobas_app::assets` — which keeps its own `MONITORED_BY` as an alias of it, so nothing
  that reads it moves. A fifth spelling would be a monitor visibly attached to an asset that never colours it,
  never reaches the inbox and never takes a recovery line, with nothing failing anywhere. The two
  statements that cannot name the constant, both compile-time `concat!`s, are pinned to it by
  `inbox::tests::the_alert_rule_reads_the_relation_the_estate_draws` and
  `suggest::tests::the_relation_this_rule_proposes_is_the_one_the_estate_reads`.

  **`knobas_core::context::recorded_parent_key!`**, likewise, and for ADR-0007 rather than for
  tidiness: the epic-children seed is a payload read outside an adapter, two seeds make it now, and
  requirement 2 is *"confined to one named statement"* — so the path `fields.parent.key` is that
  named statement and both seeds read it, leaving a second source's spelling one more `coalesce` in
  one place. Its failure direction is stated there (it **misses**: an epic seeds fewer children
  rather than the wrong ones, an absent member never a wrong one) and pinned on both sides.

  *Amended 2026-10-12 (#641, PR #648):* the named statement now reads two spellings, both Jira's:
  `coalesce(i.payload->'fields'->'parent'->>'key', i.payload->'epic'->>'key')`. A classic Data
  Center project carries no `fields.parent`, and since #640 the adapter stores the Epic Link beside
  the record as `epic: {"key": …}` (the deputy's ruling of 2026-10-12 on #598). The order is the
  Jira descriptor's `ticket` parent candidates, `fields.parent` first. Both seeds still read only
  the macro, and the failure direction is unchanged. No surface moved. The sentence above that
  names `fields.parent.key` as the path is left as history.

  **The sixth notification category is switched on here and its behaviour is #447's.**
  `every_inbox_category_has_a_toggle_in_the_interface` is a pin, not a nicety: a category with no
  switch in `NotificationsSection.svelte` is a demand nobody can ever be notified about, and the
  gate goes red without the line. So the toggle (*A monitor down on something in a context*, off by
  default like the others) lands with this ticket and #447 keeps the notify-once-per-new-item tests
  and the negatives its own criteria name.

  **A fake-tauri handler for `open_alerts`**, which #444 left out: the Assets view's strip and the
  top strip's badge were the only two surfaces under `?fake-ipc` answering "command not found" in
  red. Derived from `fakeMonitorRoster` so the strip and the Monitors tab are one statement about
  one fixture. **The inbox has no fake handlers at all**, so the sixth category's row cannot be seen
  that way whatever this answers; that is a gap #45 left and this ticket does not close.

  Pinned by: `crates/knobas-core/tests/inbox.rs`' eight new tests (the routing rule with three
  negatives — an asset no context holds, a monitor watching nothing, and a *proposed*
  `monitored-by` link; membership through ancestors with a sibling subtree left alone; acked and
  closed both gone; snooze on and off the shelf; what the row says, including that `entity_id` is
  the asset; the archived-context negative; a monitor watching several assets as one item; and all
  six categories on one stream with the count over them);
  `the_merged_walk_is_the_union_of_every_contexts_members` in `crates/knobas-core/tests/contexts.rs`,
  beside `a_misshapen_parent_seeds_no_context_in_the_merged_walk` — ADR-0007's requirement 3 for the
  merged seed, which needs a pin of its own because it splits the source and the key out of the
  anchor id where `member_ids` binds them as two parameters, so the same path is read against a
  different right-hand side and a `substr` off by one would leave the union-equality test passing
  with both sides short by the same rows;
  `inbox::tests::the_alert_rule_reads_the_relation_the_estate_draws`, and the existing
  `every_category_has_a_rule` and `the_count_is_the_stream_statement_counted`, which now walk six;
  `crates/knobas-sync/tests/alerts.rs`' two new tests through the engine (recovery writes a line and
  opening writes none; the line lands only on confirmed watched assets);
  `crates/knobas-app/tests/inbox_ipc.rs`' three new tests (the whole seam — item, ack, alert acked
  and open, item gone, both history lines; recovery removing an un-acked item; the second ack and
  the refusal); `crates/knobas-app/tests/assets_ipc.rs`' handler list and its argument-decode loop;
  `commands::assets::tests`' registration and argument-name loops (now twenty commands); and on the
  frontend four new tests in `inbox.test.svelte.ts` (the Tree address, the ack's argument,
  `subjectOf` over a subject carrying colons, slashes and a hash, and that no other category
  offers it), one in `AssetsView.alerts.test.svelte.ts` (the two history verbs
  read as sentences), one in `fake-tauri.test.ts`, and the widened
  `NotificationsSection.test.svelte.ts`.

  **Björn keeps the gate for frozen contracts and this entry is flagged for his review**, and in
  particular the command's argument, the widened `InboxCategory`, `entity_id` carrying the asset,
  the archived-context reading, and `alerts::reconcile` becoming `pub`.

