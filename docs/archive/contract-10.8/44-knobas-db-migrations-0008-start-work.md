- **`crates/knobas-db/migrations/0008_start_work.sql` and the IPC command schema with both
  append-only barrels, issue #44 (2026-08-29):** the start-work flow. Granted by the
  **orchestrator**; merged under the delegation the #42 and #43 entries record, which covers
  migrations and IPC additions.

  **It is orchestration, and the grant is only what orchestration needs.** No SPI change, no new
  adapter capability, no new `WriteOp` variant: `crates/knobas-source/src/**`,
  `crates/knobas-http/**` and `crates/knobas-app/src/{error,profile}.rs` are **untouched**. Every
  side effect the flow has is an existing op from #43 dispatched through the queue from #42, plus
  a link created through the existing link store.

  **The migration: one table, `knobas.start_work_step`.** Nothing existing is altered, so it is
  additive on every axis and no applied migration is touched. **`0009` is the next free number**,
  and it was allocated to #45, which was in flight beside this.

  Two decisions in it are load-bearing and are argued for in the file itself:

  - **The flow's progress is a table because neither half of it is derivable.** The *proposal* --
    the branch name, the pull request title and body as the user edited them -- has to survive
    between reviewing the sequence and running it, and again between a failure and a retry. The
    *progress* has to survive so the user can come back to `#/start-work/<key>` and see what
    knobas already did (stories 16 and 21). The mirror can say a branch exists but not that this
    flow made it, and the write queue can say a transition was queued but not that a step was
    deliberately **skipped** -- a decision with no side effect anywhere else.
  - **The reverse direction gets no table, and that absence is the design.** What stops a merged
    pull request moving its ticket twice is `knobas.write_queue` itself: a `transition` row against
    that ticket carrying the status, in **any** state including the terminal ones, is the record
    that knobas has already followed that merge. Rows there are never deleted (`0005`), so the
    memory is exactly as durable as a column would be; it is inspectable in the pending-writes
    panel, which is where a user would look for it; and it cannot drift from the write it is a
    memory of, because it *is* that write. A second table would be a second thing to keep in step
    with the first. `merge.rs` names the jsonb path it reads, and `plan`'s
    `the_transition_payload_is_shaped_the_way_the_reverse_direction_reads_it` pins it against the
    value actually stored -- a renamed variant would otherwise make the `not exists` match nothing,
    which is a ticket re-transitioned on **every** pass.

  Two closed vocabularies with CHECK constraints, the discipline `0005`'s `state` gets and for the
  same reason -- the enums that write them (`knobas_core::start_work::{Step, StepOutcome}`) live in
  another language, and this module's decoder *refuses* an unknown spelling. Both are walked
  against this file by `the_steps_and_outcomes_are_exactly_what_the_migration_allows`. `payload`
  deliberately has no CHECK and is jsonb, for `0005`'s reason: it holds a serialized `WriteOp`,
  which ADR-0006 grows per milestone.

  **The IPC schema and both append-only barrels.** Six new commands, in the **existing** `entity`
  module and its existing mirror -- the `commands/` + `ipc/` layout is frozen and no module is
  created on either side. The flow starts from a ticket *entity*, which is what that module is
  about, and #41's four suggestion commands set the precedent for a block of them there:

  ```rust
  #[tauri::command] pub async fn start_work_flow(.., entity_id: String, repo_id: Option<String>)
                                            -> Result<Vec<FlowStep>, IpcError>;
  #[tauri::command] pub async fn start_work_run(..,  entity_id: String) -> Result<Vec<FlowStep>, IpcError>;
  #[tauri::command] pub async fn start_work_retry(.., step_id: i64)     -> Result<Vec<FlowStep>, IpcError>;
  #[tauri::command] pub async fn start_work_skip(..,  step_id: i64)     -> Result<Vec<FlowStep>, IpcError>;
  #[tauri::command] pub async fn start_work_amend(.., step_id: i64, payload: serde_json::Value)
                                            -> Result<Vec<FlowStep>, IpcError>;
  #[tauri::command] pub async fn follow_merges(..)                      -> Result<u32, IpcError>;
  ```

  Mirrored as `startWorkFlow`, `startWorkRun`, `startWorkRetry`, `startWorkSkip`, `startWorkAmend`
  and `followMerges` in `app/src/lib/ipc/entity.ts`; six lines appended to
  `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, in the existing `commands::entity::`
  group. `app/src/lib/ipc/index.ts` needed no edit -- it already re-exports `./entity` wholesale.
  **No existing command, DTO field or event name changes, and no new event** -- for #42's reason,
  unchanged: every queue transition already writes an activity line, so `activity:new` is the
  signal.

  **DTOs.** One new (`knobas_core::start_work::FlowStep`) and two new closed vocabularies on the
  wire (`Step`, `StepOutcome`), mirrored as `StartWorkStep`, `StartWorkStepKind` and
  `StartWorkOutcome`. All three are pinned in `entity_mirror.rs` -- the shape by `assert_shape`,
  the two unions by `declared_union` read *out of* the mirror rather than listed beside it.

  Four shape decisions a later reader might undo without realising what they were for:

  - **`payload` is untyped on the wire**, exactly as `submit_write`'s and `amend_write`'s are and
    for the same reason: `WriteOp` grows per milestone, so typing it would drag the SPI's enum onto
    the IPC surface and make every growth an IPC change.
  - **`start_work_flow` reads *and* proposes**, and a `repo_id` of `null` means *read only*. The
    address `#/start-work/<key>` has to answer both questions at once -- is there a flow, and if
    not what would one look like -- and an empty answer is the state where the view asks which
    repository. Splitting it into two commands would let a view propose a flow before the reader
    had picked one, which is the thing story 5 is about.
  - **There is no `source_id` argument anywhere**, for `submit_write`'s reason: §4.1 makes the
    instance id and the `EntityRef` namespace the same string.
  - **Every command answers the whole flow**, not the one step it touched. A stepper that patched
    one row from a command's answer would draw a sequence assembled from two moments; `advance`'s
    answer depends on *all* the rows, so a partial update is a stepper that can disagree with the
    orchestrator about which step is next.

  **`StepOutcome` has six variants and `queued` is one of them**, deliberately. A source that
  cannot take a step's write does not fail the flow -- the write becomes a *pending write* and the
  step reports queued (story 15) -- and the flow's completion and the write's delivery are
  different events the UI may not conflate. Drawn as a failure it invites a retry of a write
  already on its way; drawn as a success it reports work that has not happened. It is settled by
  neither `advance` nor `is_settled`, so the sequence *waits* on it rather than running the next
  step over an effect that does not exist yet.

  **Also worth a later reader's attention, and outside the frozen list:**

  - **The seam is `knobas_core::start_work::advance`, and it is pure.** Walk the steps in position
    order; the first that is not settled decides. "A failed step never advances the sequence" is
    therefore a property of the step list rather than a check the orchestrator remembers -- there
    is no answer of that shape to give. Removing the stop-on-failure arm kills three unit tests in
    `knobas-core` and nine in `knobas-app`'s battery.
  - **`crates/knobas-app/src/start_work/plan.rs` is a new `HANDS_TO_THE_QUEUE` entry** in
    `knobas-sync/tests/write_choke_point.rs` -- the list's second, after #42's amendment guard. It
    *builds* the ops the flow will submit and stores each as its serialized payload, so what the
    user was shown and what is sent are one value; hand-rolling those as `json!` literals would put
    the SPI's serde shape in string literals. It dispatches nothing, and the orchestrator beside it
    never names the enum at all.
  - **Story 8's "opened as a draft" is met by Gitea's own `WIP:` title prefix, not by a flag.**
    `WriteOp::CreatePullRequest` has no `draft` field and adding one would be an ADR-0006 growth,
    which this issue explicitly is not. The prefix is *in the proposal*, so the user sees it and
    deletes it if they want reviewers now. The live suite proves it works: Gitea refuses to merge
    the pull request until the prefix is taken off.
  - **Two source-shaped reads live outside an adapter, each confined to one statement, and both are
    the read-direction face of the seam #43 named.** `queue.rs`'s `PULL_REQUEST_BY_HEAD` reads
    `payload->'head'->>'ref'` because `Source::write` answers nothing -- the pull request knobas
    just created is found by reading it back -- and `merge.rs`'s `MERGED_AND_LINKED` reads
    `payload->>'merged'` because §4.1 guarantees `title`, `body_text`, `updated_at` and a verbatim
    `payload`, and merged-ness lives only in the last. Neither is a frozen surface and neither
    needs a grant; both are recorded here because the *right* answer to them is the same
    self-describing descriptor field #43's entry names for `transition`'s projection, and that is
    its own ADR-0006 conversation rather than a follow-up tidy-up.
  - **There is still no read of a ticket's available transitions**, which #43 predicted this flow
    would meet first. It does, in the retry path: a retry looks for a step's effect before writing
    again, and for a `transition` there is nothing to look at. The resolution is that a transition
    is safe to re-dispatch in a way a create is not -- moving a ticket to a status it is already in
    makes no second object, and the adapter resolves the name against what the source says is
    reachable right now and refuses by name otherwise. **No trait method was added.**

  Ratified by the orchestrator as issue #44 itself, whose spec (written 2026-08-29 via `/to-spec`,
  seams confirmed by Björn) specifies the feature and its acceptance criteria, and which allocated
  `0008` to that stream exclusively.
