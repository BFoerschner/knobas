- **The IPC command schema and both append-only barrels, issue #42 (2026-08-29):** the write
  queue's six commands, granted by the **orchestrator under delegation while Björn was away**.
  §10.8 requires an orchestrator decision *and* an entry here; this is the entry. **The merge
  gate**: when this entry was first written the merge was Björn's — the PR carries migration
  `0005` and this grant, two frozen surfaces. On 2026-08-29 Björn delegated exactly those
  ("let Migration and ipc additions be merged by fable too"), and the PR was merged by the
  merge-manager under that instruction. Milestone exits and the contract battery's clauses were
  not delegated and remain his.

  What the grant covers, and what it deliberately does not: **the frozen thing is the layout, not
  the existence of commands inside it.** #42's spec (seams confirmed by Björn) says "IPC lives in
  the existing `sources` command module and its TypeScript mirror; the `commands/` + `ipc/` module
  layout is frozen, no new module", and that is what was built. No module was created on either
  side, no existing command, DTO field or event name changes, and both barrels were appended to.

  Six commands, all in `crates/knobas-app/src/commands/sources.rs`, mirrored in
  `app/src/lib/ipc/sources.ts`:

  ```rust
  #[tauri::command] pub async fn pending_writes(..)      -> Result<Vec<knobas_core::write_queue::QueuedWrite>, IpcError>;
  #[tauri::command] pub async fn write_queue_counts(..)  -> Result<knobas_core::write_queue::QueueCounts, IpcError>;
  #[tauri::command] pub async fn flush_writes(.., source_id: Option<String>) -> Result<(), IpcError>;
  #[tauri::command] pub async fn apply_held_write(.., id: i64)               -> Result<(), IpcError>;
  #[tauri::command] pub async fn amend_write(.., id: i64, payload: serde_json::Value) -> Result<(), IpcError>;
  #[tauri::command] pub async fn discard_write(.., id: i64)                  -> Result<(), IpcError>;
  ```

  Both barrels are appended, never rewritten: six lines in `crates/knobas-app/src/lib.rs`'s
  `generate_handler!` list, in the existing `commands::sources::` group, and six exported functions
  plus five types (`WriteState`, `WaitReason`, `WriteOpPayload`, `QueuedWrite`, `QueueCounts`) at
  the foot of `app/src/lib/ipc/sources.ts`, under their own banner.

  **No new event, and that is a decision rather than an omission.** Every queue transition already
  writes an activity line, so `activity:new` is the signal that something moved and the shell
  re-reads on it. A `write:*` event would be a second channel carrying the same news, with its own
  entry in `knobas_app::events` and its own line in the `EVENTS` mirror to keep in step. If a later
  milestone wants one, it needs its own grant.

  Four shape decisions a later reader might undo without realising what they were for — recorded
  here in the spirit of #53's "a later tidy-up that flattens it is a bug, not a simplification":

  - **`QueueCounts` is three numbers, never a total.** A single `pending` count would let "3
    waiting" absorb a write that needs a *decision*, which is the one thing the shell badge exists
    to prevent (#42, stories 17 and 18). Summing them in the UI is the same bug wearing a hat.
  - **`amend_write` takes the row's own `payload`, edited — not a body string, and not a typed
    `WriteOp` on the wire.** A body string cannot express an op that has no body, and `WriteOp`
    grows per milestone (ADR-0006), so typing the argument would drag the SPI's enum onto the IPC
    surface and make every growth an IPC change. It is decoded into a `WriteOp` before it is
    stored, so an unreadable payload is `invalid` at the dialog rather than an undecodable row
    discovered at flush time.
  - **An amendment may not change the op or the target**, and both refusals are structural:
    a queued write holds a *place in its entity's queue* and a *snapshot of that entity*, and a
    payload that repointed it would inherit an ordering guarantee and a hold comparison
    established for a different write. `crate::sources::write_queue::check` is the guard and has
    its own tests; relaxing it is not a simplification.
  - **`pending_writes` returns held and refused rows too**, which is why it is not called
    `open_writes`: `CONTEXT.md` calls the whole queue **pending writes** and a held write "a
    pending write whose target changed". The name follows the glossary rather than the state
    column.

  Also outside the frozen list, and noted because it is what the commands forward to:
  `knobas_sync::scheduler::Scheduler::deps()` and `write_queue::target_entity` became public so a
  command can reach the flush loop without `SourcesState` growing a second copy of four fields;
  `crates/knobas-app/src/sources/write_queue.rs` is a new file in the *decision* layer, which is
  where `commands/sources.rs`'s own header says every decision lives, and is not part of the frozen
  `commands/` + `ipc/` layout. **`crates/knobas-app/src/{error,profile}.rs` are untouched** —
  `FlushError` decomposes into `CoreError` and `sqlx::Error`, both of which already have mappings,
  which is why it is not a new `SyncError` variant.

