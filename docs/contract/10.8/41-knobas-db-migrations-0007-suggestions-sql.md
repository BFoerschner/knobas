- **`crates/knobas-db/migrations/0007_suggestions.sql` and the IPC command schema, issue #41
  (2026-08-29):** the suggestion engine and the room tray. **Both decisions were taken by the
  orchestrator under delegation while Björn was away**, and both are recorded here because this
  section requires it. **The merge gate**: when this entry was first written the merge was Björn's —
  the PR carries migration `0007` and this grant, two frozen surfaces. On 2026-08-29 Björn delegated
  exactly those ("let Migration and ipc additions be merged by fable too"), the same instruction the
  #42 and #46 entries above record, and the PR was merged by its merge-manager under it. Milestone
  exits and the contract battery's clauses were not delegated and remain his.

  **The seam is entirely existing, and that is the point.** A suggestion **is a link row**: the
  ratified vocabulary already closes `origin` over five spellings, one link table is a standing
  rule, and a parallel suggestions table would be a second graph that can disagree with the first.
  What the feature needed was therefore not a table but a *state* on the row it already has.

  **The migration.** `0007` was allocated to this stream exclusively; `0005` (#42) and `0006` (#46)
  are claimed by other streams and nothing here reads them. **`0008` is the next free number**;
  `0001`–`0007` are never edited. It is additive and re-entrant, the same discipline as `0003` and
  `0004`, and it does four things:

  - `knobas.link.confirmed_at timestamptz` — **the state, and the whole seam.** `NULL` is a proposal
    knobas made; non-`NULL` is a link that is in the graph. **Backfilled from `created_at`**, not
    from `now()`: every link that existed before this migration was drawn or imported by the user
    and was confirmed the moment it was written, so dating them all at the minute of an upgrade
    would be a fact the database invented. A test winds a scratch database back to before `0007`,
    fills it with one link of every origin and lets `migrate::run` apply it for real.
  - The column default is **`now()`**, so *the failure mode of forgetting it is a confirmed link*.
    That direction is deliberate and a later reader should not flip it: a hand-drawn link that
    silently became a proposal would vanish from the panel it was drawn in, while a proposal
    written as confirmed can only come from the one statement in `knobas_core::suggest` that writes
    proposals — one place, with a test on it.
  - `rule`, `rule_class`, `reason`. `rule_class` is closed by `link_rule_class_chk`
    (`exact_key|similarity|source_relation`) and cross-checked against
    `knobas_core::suggest::RuleClass` by the line that lists it, exactly as `link_origin_chk` pins
    `Origin` — the vocabulary cannot grow on one side only. **`rule` is deliberately *not*
    constrained**: rules are expected to grow and a new detector must not cost a migration. The
    class is the closed axis because it is the one a surface branches on and the one a user
    calibrates trust with. `link_proposal_chk` refuses an unconfirmed row that carries no rule,
    class and reason — "a suggestion whose reason cannot be shown is not shippable", enforced.
  - **Two views, `knobas.confirmed_link` and `knobas.proposed_link`**, whose predicates are each
    other's negation over the same live rows. `knobas_core::link::entries_of` reads the first and
    `suggest::proposals` reads the second, so **the links panel and the tray are structurally
    unable to blur** — the same treatment `0002` gave the tombstone filter with `sync.live_item`,
    for the same reason: the reader that forgets the predicate is the one that ships the bug. A
    schema test asserts the two are disjoint *and* total. Collapsing them back into two `where`
    clauses would be the bug, not a simplification.
  - Three indexes: `link_pair_idx` / `link_pair_rev_idx` (the suppression reads the pair in **both**
    directions and with **no filter**, because a tombstone is the withdrawal memory and every other
    index on the table is partial on `deleted_at is null`) and a partial `link_proposed_idx`.

  **`link_active_idx` is untouched and spans both states**, deliberately: one active edge per
  `(from, to, relation)` whatever its state, so a proposal and a link for one pair can never coexist
  and disagree. The consequence is that a *proposal* refuses a hand-drawn link on the same triple,
  which `suggest::accept_edge` turns into the right outcome — `create_link_inner` promotes the
  proposal instead of reporting "already linked" about a pair whose links panel is empty. Removing
  that path re-opens a user-visible lie.

  **The IPC additions**, in the **existing** `entity` command module and its existing TypeScript
  mirror — **no new module on either side**, the `commands/` + `ipc/` layout is unchanged, and the
  entries are appended to `crates/knobas-app/src/lib.rs`'s handler list. `app/src/lib/ipc/index.ts`
  needed no edit: it already re-exports `./entity` wholesale.

  ```rust
  #[tauri::command] pub async fn detect_suggestions() -> Result<u32, IpcError>;
  #[tauri::command] pub async fn room_suggestions(sources: Vec<String>, limit: u32)
                                                             -> Result<SuggestionPage, IpcError>;
  #[tauri::command] pub async fn accept_suggestion(link_id: String)  -> Result<(), IpcError>;
  #[tauri::command] pub async fn dismiss_suggestion(link_id: String) -> Result<(), IpcError>;
  ```

  Mirrored as `detectSuggestions()`, `roomSuggestions(sources, limit)`, `acceptSuggestion(linkId)`
  and `dismissSuggestion(linkId)` in `app/src/lib/ipc/entity.ts`. All four go in the `entity` module
  because a suggestion *is* a link row: these are the writes that move it between the two states the
  link store already has, and the read is the same graph the panel reads from the other side. A
  `commands/suggest.rs` would have been a second module over one table. A test reads the handler
  barrel and fails if a command is registered and never invoked from the mirror, or invoked and
  never registered.

  **DTOs.** Two new (`knobas_core::suggest::SuggestionEntry` — `{link, from, to}`, **both** ends,
  because the tray is read from a room and has no "here" to leave out — and `SuggestionPage`
  — `{rows, total}`, where `total` is the *room's* count and not the page's, since the heading
  answers "is it worth looking"). Both are nested rather than flattened, for the reason #53's entry
  gives: flattening would collide `id` three ways. `knobas_core::link::LinkRow` gains the four
  columns above (`confirmed_at`, `rule`, `rule_class`, `reason`), mirrored on `LinkRow` in
  `entity.ts`; it rides inside `EntityDetail`, which is why it is recorded here. No existing command,
  event name or DTO field changes shape.

  **Also worth a later reader's attention, and outside the frozen list:**

  - `origin` is **provenance, not state**. A relation Jira already states is written with
    `Origin::Source` and an unconfirmed `confirmed_at`; the exact-key and similarity rules write
    `Origin::Suggested`. The ratified reading of `Suggested` — "proposed by knobas **and confirmed
    by the user**" — still holds, because a proposal is not in the graph: every `suggested` row any
    reader can reach through `knobas.confirmed_link` was confirmed. Rewriting `origin` on acceptance
    would throw the provenance away.
  - **Dismissal is the withdrawal memory, not a second mechanism.** `suggest::dismiss` sets the same
    tombstone `link::unlink` sets, and detection's suppression reads the table with no filter at all,
    so a dismissed suggestion and an unlinked link are one fact to the detector (#40 story 12). A
    dismissals table would be a second thing to keep in step with the first.
  - **Every rule's statement is compiled from one driver** (`driver_head!` / `driver_tail!`), so the
    suppression, the self-link guard and the undirected de-duplication are written once and a new
    rule cannot forget them. Nothing in `knobas-core` builds SQL at run time; the rules are
    `&'static str` assembled by `concat!`. A rule that hand-rolled its own `insert` would be outside
    every idempotence test and would look perfectly correct beside the others — a test asserts every
    rule carries the driver.
  - **No SPI change, no descriptor change, no new adapter capability.** Detection is a pass over the
    mirror. `crates/knobas-source/**`, `crates/knobas-http/**` and
    `crates/knobas-app/src/{error,profile}.rs` are untouched.

  Ratified by the orchestrator as issue #41 itself, whose spec (written 2026-08-29 via `/to-spec`,
  seams confirmed by Björn) specifies the feature and its acceptance criteria.

