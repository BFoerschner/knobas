- **The IPC command schema and the Rust handler barrel, issue #1953 (2026-10-24): one command,
  `owed_tickets`, which names the tickets filed from an entity that it does not link yet.** No
  migration, no event, no new IPC type, no new module in `commands/` or `ipc/`. The command sits
  in the existing `commands/entity/links.rs`, beside `submit_write` whose `link_to` it reads back,
  and is appended at the foot of the handler list, after `watch_sync`. Reserved for Björn, ruled
  by the implementer on Opus (`## #1953` in `docs/archive/2026-10-unattended-rulings.md`); the
  issue has no parent spec.

  ```rust
  #[tauri::command] pub async fn owed_tickets(.., entity_id: String) -> Result<Vec<knobas_core::write_queue::QueuedWrite>, IpcError>;
  ```

  The answer is every `create_ticket` row whose `knobas.write_queue.link_to` (migration `0046`,
  an existing column) holds `entity_id` and whose state is not `discarded`, oldest first, as the
  queue holds it now (`protocol::creates_owing`, #1951's read, its parameter now `entity_id`). A
  held or refused one is named with its state. The logic is the new `protocol::tickets_owing`;
  `commands::entity::owed_tickets_inner` is its shim. Before it answers, the read resolves: a sent create
  whose ticket the mirror now holds is linked (`sources::write_queue::link_on_arrival`, the end of
  a sync's resolution) and its `link_to` cleared (`protocol::owed_no_more`), so it is not named.
  The declarations it hands `link_on_arrival` are `sources::paths::declared_paths` over the sync
  engine's registry. `not_ready` before bring-up; `internal` if a read or a write fails. An
  entity id that names nothing answers an empty list.

  Mirrored in `app/src/lib/ipc/entity.ts` (`owedTickets`), held by `commands/entity/mod.rs`'s
  `the_mirror_invokes_the_commands_by_their_registered_names`, answered by the fake IPC
  (`fake/entity.ts`, an empty list; its `Answers` line in `fake-answers.ts`), and called by
  `RunPorts.owedTickets` (`app/src/lib/tf/builds/pipeline-ports.ts`) from a failed run's
  *Create ticket* (`TfRunTicket.svelte`). Each read of the run's links asks it first, marks a
  `pending` or `sent` create queued, and reads the links after it. Witnessed by
  `tests/it/run_ticket_ipc.rs` (new): `a_ticket_filed_from_a_run_is_named_until_its_ticket_is_mirrored_and_linked`
  and `a_withdrawn_create_or_one_filed_from_another_run_is_not_named`.
