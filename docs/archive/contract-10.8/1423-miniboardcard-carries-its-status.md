- **The IPC schema — issue #1423 (2026-10-21): `MiniBoardCard` gains `status`.** A saved view's
  board (a Jira view's, #877) gathers several statuses under each of the view's columns and names
  the column by the view's name, so the Tickets page read the column's name as the ticket's status:
  PAY-233 (To Do) under *Ready* read *Ready* and offered *Resume work*. No new command, no argument
  changed, no migration, no event, no `WriteOp`, nothing under `crates/knobas-source/src/**`, no
  `commands/` or `ipc/` module, nothing added to either append-only barrel. Ratified by the
  implementer on Opus under Björn's 2026-10-21 delegation at triage; the ruling is in
  `docs/archive/2026-10-unattended-rulings.md` under `## #1423`.

  **The IPC schema.** `mini_board`'s `MiniBoardCard` gains `status: Option<String>`, additive: the
  ticket's own status in the source's words, the one it stands in on `read_scoped`'s board (the
  same `CARDS` row the column is grouped by, so no extra read), `None` for the terminal group.
  `MiniBoardColumn.status` is unchanged on the wire; its doc now says that on a saved view's board
  it is the view column's name, and that a ticket's status is read from the card. Generated into
  `app/src/lib/ipc/generated/entity.ts` (§2.6 as amended by #1325); witnessed by `knobas-core`'s
  `tests/it/mini_board.rs` and `knobas-app`'s `tests/it/jira_views_ipc.rs`.

  #1476, bundled in the same PR, changes no IPC type and no route grammar: it addresses a ticket two
  sources hold by the entity-id job address the shell conventions already read
  (`#/job/<source>:<key>`).
