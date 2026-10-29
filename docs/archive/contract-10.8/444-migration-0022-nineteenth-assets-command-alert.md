- **Migration `0022` and a nineteenth `assets` command, issue #444 (2026-09-07):** the alert a
  monitor's crossing into down or warn opens, the recovery that closes it, and the estate's list of
  what is open. **Ratified in advance by the spec (#427) Björn approved** — "Migrations from the
  next free number: asset, route (M4.0); sample, **alert** (M4.1)" and, of the module pair, "the
  **alert reads** and ack from M4.1" — and by the #428 entry above, whose module-pair paragraph
  names the M4.1 commands among the ones that belong there. Written with the implementing PR, per
  #428's, #431's, #434's, #435's, #439's and #443's pattern.

  **`0022` is claimed here; `0023` is the next free number**, and #446's inbox category is expected
  to need none. This supersedes both the #443 entry's sentence naming `0022` as free and the #448
  entry's above, which repeats it and names this table as the one expected to take the number; the
  old sentences are left as history rather than rewritten, the treatment #53, #278, #428, #439 and
  #443 give the sentences they supersede.

  **The migration.** `0022_the_alert_a_monitor_opens.sql` adds one table and edits nothing.

  ```sql
  create table knobas.monitor_alert (
    id         bigint generated always as identity primary key,
    entity_id  text not null references knobas.entity(id) on delete cascade,
    opened_at  timestamptz not null default now(),
    state      text not null,
    acked_at   timestamptz,
    closed_at  timestamptz,
    constraint monitor_alert_state_chk check (state in ('down','warn')),
    constraint monitor_alert_closed_after_opened_chk
      check (closed_at is null or closed_at >= opened_at),
    constraint monitor_alert_acked_after_opened_chk
      check (acked_at is null or acked_at >= opened_at)
  );
  create unique index monitor_alert_one_open_idx
      on knobas.monitor_alert (entity_id) where closed_at is null;
  create index monitor_alert_open_idx
      on knobas.monitor_alert (opened_at desc) where closed_at is null;
  ```

  **No entity row, and therefore no address**, which spec #427 says in as many words: "An alert is
  a knobas-owned row keyed on the monitor entity: opened at, state, acked at, closed at; **no
  entity, no address**." It is the one knobas-owned thing in this schema that is not addressable,
  not linkable and not findable — `asset`, `route`, `note` and `context` all carry a `knobas.entity`
  id and this carries a `bigint`. What a reader opens is the **asset**, story 61: "opening an alert
  from the inbox lands in the Tree at the affected asset with the monitor in the pane." `0006`'s
  `item_entity_reserved_chk` therefore has nothing to say about this table, because there is no
  entity to reserve.

  **One open per monitor is a partial unique index**, not a rule the reconciler is trusted to keep.
  Story 57's "one open per monitor at a time, so that a flapping monitor does not flood anything" is
  a property of the schema: a second opener — a future write path, a hand-run statement, two engines
  against one database — fails at the statement instead of quietly doubling every count the top
  strip draws. Closed alerts are unconstrained, because a monitor that has fallen five times has
  five closed rows and that is its history.

  **`state` is two words where `monitor_sample.state` is five.** `0021`'s CHECK allows `up`, `down`,
  `warn`, `pending` and `maintenance`, because a sample records what was *seen*; an alert exists
  only for the two states that are trouble. **The #443 merge review left the reading of the extra
  two open and this is it: `pending` and `maintenance` neither open an alert nor close one.** The
  ticket and the spec are silent, so the conservative reading was taken — the other one can lie,
  since a monitor put into maintenance while it is down would have its alert closed and knobas would
  be reporting a recovery nobody made. A monitor coming *out* of maintenance broken is sampled
  `down` on the next poll and opens one then. The same reading is taken by the health rollup, where
  both words fall to the `else` arm and colour nothing.

  **The reconcile reads one sample and the open row, where the spec says "the newest two samples".**
  Spec #427: "reconciles alerts from the newest two samples per monitor: a crossing into down or
  warn opens an alert if none is open, a return to up closes the open one." `knobas_sync::alerts`
  reads the **newest** sample of each live monitor and the **open alert row**, which answer the same
  question — *was this monitor already in trouble when the poll landed?* — and the row answers it
  better in two places where they come apart:

  * a monitor that is **already down the first time knobas looks** has `down, down` as its two
    newest samples, which is not a crossing; a sample-only rule opens nothing and stays silent until
    the monitor recovers and falls again. That is the state of every installed profile on its first
    run after this migration. `a_monitor_already_down_when_the_alert_table_is_empty_opens_one` is
    the witness;
  * a **poll that produced no sample** (a monitor paused in Kuma leaves `sync.live_item` and is
    sampled no more) leaves the alert standing, because an alert closes on a return to `up` and
    silence is not recovery. A rule holding "the previous state" in a sample would have to decide
    what a gap means.

  It is also one guard rather than two. The two a crossing rule would spend — *the state changed*
  and *no alert is open* — are each sufficient on their own for the ordinary sequences, which is the
  self-masking shape #443's own mutation round found in its descriptor gate and roster filter.
  **Flagged for Björn as a deliberate departure from the spec's wording**, made in the direction the
  spec's own sentence ("open until the monitor recovers; at most one open per monitor") describes.

  **Where the write sits.** `knobas_sync::run_locked`, immediately after `samples::append`, inside
  the run's own transaction and under its advisory lock — so the newest sample the reconcile reads
  is the one the run just took, and a failed run leaves behind no alert claiming to have seen a
  state it never committed. Through `sync.live_item` and scoped to the running source, which is what
  makes "a paused monitor is left alone" and "a run decides about its own source's monitors" both
  properties of the read rather than rules anyone maintains. `SyncReport` does not grow a field, for
  #443's reason.

  **The command, and the DTOs.**

  ```rust
  #[tauri::command] pub async fn open_alerts(..) -> Result<Vec<assets::OpenAlert>, IpcError>;

  pub struct OpenAlert { pub id: i64, pub monitor_id: String, pub monitor_name: String,
                         pub state: AlertState, pub opened_at: DateTime<Utc>,
                         pub acked_at: Option<DateTime<Utc>>, pub assets: Vec<AlertAsset> }
  pub struct AlertAsset { pub id: String, pub name: String, pub path: Option<String> }
  pub enum AlertState { Down, Warn }   // knobas_core::closed_vocabulary!
  ```

  Mirrored in `app/src/lib/ipc/assets.ts` as `openAlerts()`, `interface OpenAlert`,
  `interface AlertAsset` and `type AlertState = "down" | "warn"`. **No `web_url`**, although the
  read joins a mirror row for the monitor's name and could carry one: nothing this ticket draws
  opens a page in Kuma, and a field on the wire that no surface reads is a field whose shape nothing
  can be wrong about. `AttachedMonitor::web_url` (#445) already carries the deep link on the surface
  that uses it, and #449's cards will own theirs. **`acked_at` is on the wire and drawn although
  nothing writes it until #446**, which is the opposite call and deliberately: the ticket lists
  *acked-at* among the row's four fields, and a list that could not say *acked* would silently draw
  every alert as unseen the day the ack lands. **One line appended** at the foot
  of `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, under #443's; neither barrel is
  rewritten (`app/src/lib/ipc/index.ts` already re-exports `./assets` wholesale) and the `commands/`
  + `ipc/` module **layout is untouched** — no new module pair, which is the thing §10.8 freezes
  about that directory.

  **One read and no separate count.** Story 58 wants "every open alert visible in the Assets view
  and counted in the top strip", which is one set and one number over it; the badge is the list's
  length. The inbox is the deliberate counterexample — there the count is a statement of its own
  because "needs me now" excludes what is snoozed — and `alerts.svelte.ts` records why an alert has
  no such second predicate. **No `alert:*` event**, for the reason `pending_writes` records for the
  write queue: an alert moves when a run finishes (`sync:state`) and when the reader acks one
  (`activity:new`), and both channels already exist.

  **`AlertState` and not `AssetStatus`**, although the two words are spelled the same there: a
  status has `up` and `none` in it because "nobody has said" is a thing an asset can be, and an
  alert that could be `none` would be a row saying nothing is wrong. The two lists are held together
  three ways at once by `assets::tests::the_alert_states_are_the_ones_the_engine_opens` — the wire,
  migration `0022`'s CHECK, and `knobas_sync::alerts::OPENS` — which matters because **nothing on
  the app side ever writes one of these rows**: the engine opens them, this module reads them, and a
  word the engine could open and the wire had no variant for would be an alert `open_alerts` refuses
  to draw on a database the constraint was happy with.

  **The health rollup changes shape, and no DTO does.** `assets::ROLLUP` grows a `watched` half: for
  every asset in the walk, the newest sample of every monitor a `monitored-by` link attaches to it,
  ranked by the same order and folded with the asset's own status into one severity, over which
  `health`, `inside` and `problems_inside` are all computed. Story 37's other half, and story 38's
  "a paused monitor as none" — which falls out of the join through `sync.live_item` rather than
  being a rule anyone maintains. **The sample and not the mirror row**, because *warn* is knobas'
  own state and exists only in the timeseries (#443): Uptime Kuma publishes `up` for a monitor
  answering slowly, so a rollup reading `payload->>'state'` could never produce the word spec #427
  puts in the middle of its own ordering. `AttachedMonitor::state` still reads the mirror and
  deliberately: that section says what *Kuma* last published. `AssetRow`'s three fields keep their
  names, their types and their meanings, so **no wire shape changes** — what changes is what the
  numbers in them are computed from, which is why this half owes no DTO entry of its own.

  **`monitor_alert` is in the backup and out of the share export**, `monitor_sample`'s arrangement
  and for its reason: both are properties of the table being in the `knobas` schema, and
  `share.rs`' `nothing_carries_the_activity_stream_the_queue_or_the_mirror` names it so the second
  half stops being silent. An alert is a statement about somebody's own infrastructure at a moment,
  and a share export is what a person hands to somebody else.

  Pinned by: `crates/knobas-sync/tests/alerts.rs` (twelve tests through the **engine** and never
  through the reconciler — down-then-up across three runs, two down polls opening one, a flap
  between two polls invisible, warn opened from the threshold and the threshold moving it, a paused
  monitor keeping its alert and gaining no sample, `pending` and `maintenance` in both directions, a
  missed sample in both directions, the negative control, the already-down case above, the partial
  unique index refusing a second open alert by hand, and a run reconciling its own source's monitors
  only); `knobas_sync::alerts`' own unit tests, including
  `the_states_are_the_ones_the_column_accepts` and `the_rule_is_the_whole_table`, which walks every
  sample word in both directions and asserts its own length against `samples::STATES`;
  `crates/knobas-app/tests/assets_ipc.rs`' eight new tests (a down monitor colouring its asset and
  its ancestors with a sibling subtree left alone, only the newest sample deciding, a paused monitor
  colouring nothing while staying listed, the whole ordering including `warn` and the two words that
  colour nothing, the two near-miss link clauses, the open-alert read with its ordering, its closed
  exclusion and its several-assets case, an alert on a monitor watching nothing, and a paused
  monitor keeping its open alert — the last of which is also where the never-closing consequence
  above is written down);
  `assets::tests::the_alert_states_are_the_ones_the_engine_opens`,
  `the_rollup_ranks_the_statuses_the_way_rust_does` (now over both `case` expressions) and
  `the_rollup_counts_only_what_is_underneath_and_only_the_two_bad_statuses`;
  `commands::assets::tests::the_open_alert_matches_its_typescript_mirror` and the registration and
  argument-name loops (now nineteen commands); `tests/assets_ipc.rs`' handler list;
  `backup::share::tests::nothing_carries_the_activity_stream_the_queue_or_the_mirror`; and on the
  frontend `alerts.test.svelte.ts` (four), `AssetsView.alerts.test.svelte.ts` (eight) and four new
  tests in `TopStrip.test.svelte.ts`.

  **Three consequences named rather than hidden**, all surfaced by a review of this PR and none
  owned by a later ticket as things stand:

  * **The first poll after `0022` opens an alert for every monitor that is already down.** That is
    the one-sample reading working as argued — no *crossing* occurred, and the estate says so anyway
    — but on the real Kuma it is a burst on one poll, and #447's desktop notification will fire per
    alert. It is a widening of "a crossing … opens one" and wants ratifying rather than leaving in a
    module header.
  * **An alert whose monitor is *deleted* in Kuma never closes.** `CONTEXT.md` and spec #427 both
    say "open until the monitor recovers", and a deleted monitor cannot; `OPEN_ALERTS` deliberately
    does not filter by `sync.live_item`, #446's ack leaves the alert open by design, and `0022`'s
    cascade only reaches it when the whole source is deleted. So it counts in the top strip
    indefinitely. A *paused* monitor resumed is sampled again and closes normally, which is the
    common case; the uncommon one has no owner. A close-by-hand — or a close when the monitor has
    been out of the mirror for N polls — is a rule neither the ticket nor the spec states, so it is
    recorded here rather than invented in the implementing PR.
  * **An alert opened on `warn` keeps the word `warn` when the monitor later goes `down`.** "One
    open per monitor" and "opens an alert **if none is open**" are the spec's own two clauses, and
    together they say the second crossing changes nothing; `the_rule_is_the_whole_table`'s
    `(down, already_open) → Nothing` is where that is written. So the strip's row draws an amber
    dot over an asset the rollup has already turned red — the alert says *when this monitor first
    got into trouble*, the rollup says *what is true now*, and they are two statements rather than
    one disagreeing with itself. Re-stating the row on the worse crossing would be a widening of
    the spec's sentence, and it is left to #449's cards to want rather than taken here.

  **Björn keeps the gate for frozen contracts and this entry is flagged for his review**, and in
  particular the migration, the command, the `pending`/`maintenance` ruling, the one-sample
  reconcile and the three consequences above.

