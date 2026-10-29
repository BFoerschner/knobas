- **`crates/knobas-db/migrations/0009_inbox.sql` and the IPC command schema with both
  append-only barrels, issue #45 (2026-08-29):** Inbox v1 — the single actionable stream.
  Both halves granted under the **2026-08-29 delegation** Björn widened the same day: the
  frozen-contract gate for migrations, IPC additions, `crates/knobas-source/src/**` and
  `crates/knobas-http/**` sits with the Fable merge-managers ("let Migration and ipc additions
  be merged by fable too", then "yes it does" on the widening — both recorded in the #43 entry
  above). Milestone exits and the contract battery's clauses were not delegated and remain
  Björn's. **This PR touches neither**: no battery clause is added, removed or reworded, and
  M2 exit criterion 2 is *discharged by* this work but not *declared* by it.

  **The migration.** `crates/knobas-db/migrations/0009_inbox.sql`, allocated to this stream and
  to nothing else. **`0008` belongs to the start-work flow (#44), which was open and unmerged
  when this was written: #44 merges first.** Nothing here reads anything `0008` adds, and
  `0009` alters no existing table, so it is additive on every axis and no applied migration is
  touched (`0001`–`0008` are never edited). **`0010` is the next free number.**

  One new table, `knobas.inbox_state`, and it is deliberately the *only* schema this feature
  asks for. **The inbox is derived from the mirror, not synced into** — its items are computed
  from `sync.live_item`, `knobas.confirmed_link` and `knobas.source_config` on every read — so
  there is no inbox table, no adapter that fetches "inbox items", and no fifth thing to keep
  consistent. A materialised inbox would be a second corpus that can disagree with the first,
  which is the failure the feature exists to end rather than to add to. Three promises fall
  out of that and would be broken by a later "optimisation" that stored the stream: an item
  leaves when its subject is resolved at the source (story 17), the inbox survives one source
  being broken (story 24), and the count is never stale in a way a list is not.

  What has nowhere else to live is the **user's own answer**: `snoozed_until` and `done_at`,
  two nullable timestamps on one row. Four decisions in that file are load-bearing:
  - **`item_key` is `'<category>:<subject>'`, and both halves are stable across syncs.** The
    category is one of five words fixed in `knobas_core::inbox::Category` — *not* the name of
    the rule that produced the item, because rules are expected to grow and keying on one
    would forget a snooze the day a category gained a second detector. The subject is
    `knobas.entity.id` for the four mirror-derived categories (the durable identity, which
    survives re-sync, tombstoning and a source being deleted and re-added) and
    `source_config.id` for credential expiry, which §4.1 makes immutable.
  - **No foreign key**, the `knobas.write_queue` (`0005`) and `knobas.sync_run` (`0002`)
    decision, with a sharper reason: this row records what the *user decided*, and one of the
    five categories is keyed on a source rather than an entity, so `item_key` could not be a
    foreign key even in principle.
  - **`done_at` is a timestamp, not a boolean**, and the difference is behavioural. An item is
    hidden only while `done_at` is at or after the moment the item last moved, so marking a
    failed build done hides it for good (a re-run is a new build id, therefore a new item)
    while marking a mention done hides it until somebody says something new. A boolean would
    make *done* mean "mute this ticket for ever", which is the one thing an inbox must not
    quietly do.
  - **A row that is neither snoozed nor done is refused** (`inbox_state_decision_chk`), so
    "nothing has been answered about this item" has one spelling — no row — rather than two.

  **The IPC.** Four commands, all in the existing `crates/knobas-app/src/commands/entity.rs`
  and mirrored in `app/src/lib/ipc/entity.ts`. **No new module on either side** — the
  `commands/` + `ipc/` layout is frozen, and #45's spec (seams confirmed by Björn) says the
  inbox lives in the `entity` module because its items are derived from entities and its two
  write commands act on them. Both barrels are appended, never rewritten: four lines in
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, in the existing
  `commands::entity::` group, and four exported functions plus four types (`InboxCategory`,
  `InboxShelf`, `InboxItem`, `InboxEntry`) at the foot of `entity.ts`.

  ```rust
  #[tauri::command] pub async fn inbox_items(.., shelf: knobas_core::inbox::Shelf) -> Result<Vec<knobas_app::inbox::InboxEntry>, IpcError>;
  #[tauri::command] pub async fn inbox_count(..)                        -> Result<i64, IpcError>;
  #[tauri::command] pub async fn snooze_inbox_item(.., item_key: String, until: DateTime<Utc>) -> Result<(), IpcError>;
  #[tauri::command] pub async fn complete_inbox_item(.., item_key: String) -> Result<(), IpcError>;
  ```

  **No fifth command, and that is the load-bearing absence.** An inbox action that changes
  something at a source is a `WriteOp` through the existing `submit_write` (#43) over the write
  queue (#42): **the inbox introduces no write path of its own**, so there is no `dispatch`
  here and no second call site for `Source::write`. `knobas-sync`'s `write_choke_point.rs` is
  what keeps that true, unchanged and still with one entry.

  **No new event**, the same decision the #42 entry records and for the same reason: the inbox
  moves when the mirror moves and when the reader answers something, and `sync:state` and
  `activity:new` already say so. An `inbox:*` channel would be a second thing to keep in step.

  Five shape decisions a later reader might undo without realising what they were for:

  - **`InboxEntry` is nested, `{item, actions}`, not flattened.** #53's ratified shape for a
    record paired with an answer about it: the item is `knobas_core`'s and the actions are
    `knobas-app`'s, and a flattened bag would make a reader guess which half a field came
    from. A later tidy-up that flattens it is a bug, not a simplification.
  - **`actions` are `WriteOp` identifiers already filtered to what the item's source
    declares**, and the interface renders exactly those. An op absent from the descriptor is
    never offered (story 23), which is the same reading `sources::write_queue::submittable`
    takes — they must agree, or the inbox would draw a button that call then refuses.
    `NewAssignment` and `CredentialExpiry` deliberately name **no** op: a transition needs a
    status the user picked (that is #44's flow, not a one-click action) and re-entering a
    credential is knobas-local. Empty is a real answer, never a missing one.
  - **`inbox_count` is the stream's own statement counted**, not a second `select` with its own
    `where`. A count computed from a different predicate than the rows it claims to count is a
    wrong number no test comparing the inbox against itself can see —
    `knobas_search::lists`' argument, applied where the shelf predicate makes it bite.
  - **The clock is a parameter, everywhere.** `items`, `count` and every `_inner` take `now`.
    A snooze that returns on its date is the load-bearing behaviour here and a test that read
    the wall clock would be a coin flip.
  - **An answer to an item that is no longer derived is `not_found`, and writes nothing.** The
    key arrives from a webview holding a list; a durable row plus an activity line about work
    since resolved at the source would be a record of something that never happened. Both
    shelves are searched, so re-answering a snoozed item still works.

  **Every inbox action is recorded, exactly once**, which is the design spec's §8 row as
  ratified ("R3 only logged some actions; the real app logs all of them"). Two writers,
  disjoint by construction: the write queue announces every transition of an action that goes to a source,
  and `knobas_app::inbox::answer` writes the one line for `snoozed` and `completed`, which the
  queue knows nothing about. Adding a line here for a dispatched write would be the double
  entry that reading looks like a fix for.

  Outside the frozen list and noted because it is what the grant is for:
  `knobas_core::inbox` is the derivation and the store, one named rule per category, each
  independently runnable and each with a negative control. Four of the five read an
  adapter-shaped `payload` path, which is the narrow coupling `suggest::RULES`'
  `source_recorded_relation` already takes for the same reason — §4.1 normalizes `title`,
  `body_text`, `author` and `updated_at` and *nothing else*, so a review request, a build's
  status and an assignee live only in the verbatim payload. Every such read is written to
  **miss** rather than guess when the shape is absent, so a source shaped differently produces
  no items of that category instead of wrong ones. `crates/knobas-app/src/inbox.rs` is a new
  file in the *decision* layer, beside `sources/write_queue.rs`, and is not part of the frozen
  `commands/` + `ipc/` layout. `crates/knobas-app/src/{error,profile}.rs` are untouched, and
  so is `crates/knobas-source/**` — **no `WriteOp` variant is added here**, and ADR-0006 is
  therefore not engaged: the inbox composes the seven #43 landed and asks for none of its own.

