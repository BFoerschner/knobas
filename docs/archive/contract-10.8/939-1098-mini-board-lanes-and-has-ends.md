- **The IPC schema and answer of `mini_board`, and `has:`'s reader — issues #939 and #1098
  (2026-10-17): a board read in lanes, and `has:` walking only the links it needs.** A laned
  Tickets board read every ticket uncapped and split it in the webview (PR #933's ruling 1):
  1.1 s and 4.2 MB on the probe's 15,000 tickets, typed or not. No new command, no migration, no
  declaration field, no `WriteOp`, no `commands/` or `ipc/` module.

  **The arguments.** `mini_board` gains two optional arguments:
  - `lanes: Option<LaneField>`, one of `epic`, `assignee`, `priority`, `issuetype`, `label`
    (`knobas_core::mini_board::LaneField`, the spellings `ticket_views::LANES` stores, `none`
    being the argument's absence);
  - `lanes_first: Option<u32>`, how many lanes to send, biggest first; absent for all of them.
  Absent, the board is read as before. With `jira_view`, the view's whole board is put in lanes
  after it is read (`knobas_core::mini_board::in_lanes`), each lane's column in Jira's order.

  **The answer.** `MiniBoard` gains two fields, always present:
  - `lanes: Vec<MiniBoardLane>`, empty without `lanes`. A `MiniBoardLane` is `{ key, count,
    columns }`: the value its tickets share in the source's words (`""` for the lane of none of
    it), how many tickets stand in it, and its statuses in the board's column order (only those
    it has), each a `LaneColumn { status, ids, total }`: the kept cards' entity ids, newest first
    (Jira's order on a view), and how many of the lane's tickets stand there. `per_column` caps
    each lane's column, as it caps a board's.
  - `more_lanes: u32`, how many lanes there are past `lanes`, which `lanes_first` cut.
  Laned, `columns` keeps every status the board counts, with its whole count, and carries the
  cards the sent lanes keep, each once. A lane's key is the card's own field
  (`MiniBoardCard::lane`): the path line for `epic`, the first label for `label`. Lanes rank by
  count, a tie to the lane with the newer ticket, then by key.
  Mirrored in `app/src/lib/ipc/entity.ts` (`MiniBoardLane`, `LaneColumn`, `LaneField`,
  `LanesAsked`), held by `entity_mirror.rs`'s `the_mini_board_shapes_match_their_typescript_mirror`.
  The fake IPC answers both arguments over its fixture.

  **`has:`'s reader** (`knobas_core::signals`). `ticket_signals!` takes a fourth argument, a
  `text[]` of the kinds wanted (`null` for all four, which the board passes), and guards each
  kind's branch by it. `linked_ends!` is replaced by `signal_ends!(decl, kinds)`: the ends that
  can give the kinds asked for (a `blocks` link's `to` end; an end whose other end is a `pr`; for
  `failing-build`, the ends within two hops of a linked build in the `err` tone). The answer is
  unchanged: `signals_ipc.rs` still holds the read and `has:` to one answer. `has:`'s two binds
  are allocated kinds first, then the user.
