- **The IPC command schema, issue #753 (2026-10-13): `mini_board` takes an optional per-column
  cap, and each `MiniBoardColumn` carries its `total`.** Signed off under Björn's 2026-10-13
  delegation, the one #745's entry names; #753 asks for whatever fix its measurement calls for,
  "if it's contained". No migration, no Source SPI change, no new command. It amends #177's entry
  (the board's signature and its four interfaces) and #208's (the fourth argument), additively.

  **Why.** `just responsiveness-probe` (three Jira-shaped sources of 5,000 tickets, a 22,000-item
  mirror) timed every read the shell makes. The Tickets tile of the *All* room was the slowest
  read on any click that changes the screen: `mini_board` answered **2.17 MB** in 327–387 ms at a
  load average of 13–30, and the webview then drew a card per ticket, 15,000 of them. No tile can
  show that; the list tiles already read a page of 50 (500 maximised, #680).

  **The signature**, in the existing `entity` command module:

  ```rust
  #[tauri::command] pub async fn mini_board(.., ctx_id: Option<String>, sources: Vec<String>, project: Option<String>, per_column: Option<u32>) -> Result<knobas_core::mini_board::MiniBoard, IpcError>;
  ```

  `per_column` keeps each column's newest `per_column` cards, in the board's existing order;
  omitted (`null`), the board is whole, as before. `knobas_core::mini_board::read_capped` is the
  read; `read` is `read_capped(.., None)`.

  **`MiniBoardColumn` gains `total: u32`**: how many tickets stand in the column, drawn or not.
  On an uncapped read it equals `cards.len()`. #177 kept the count off the DTO because "a count
  beside the list it counts is a second copy of the same fact"; a capped column is exactly where
  the two facts differ, so the header now draws `total` and the column's foot says how many are
  not drawn. `MiniBoard.sources` is unchanged and still answers for the sources' whole corpora.

  **The mirror**, in the existing `app/src/lib/ipc/entity.ts`: `miniBoard(filter, perColumn?)`
  sends `perColumn` (`null` when omitted), and `MiniBoardColumn` gains `total: number`. Pinned by
  `the_mini_board_shapes_match_their_typescript_mirror` (`crates/knobas-app/tests/it/entity_mirror.rs`),
  whose field list grows by `total`. Neither barrel is touched: the command was already in
  `generate_handler!`, and `index.ts` re-exports `entity.ts` wholesale.

  **Who sends what.** The Tickets tile sends 50, and 500 when maximised, the list tiles' two
  numbers. The ticket detail's status select sends nothing and reads the whole board, because it
  finds the ticket's current status among the cards; that read is off the panel's draw path, and
  a narrower one is follow-up work, not this entry.

  **The statements**, in `knobas_core::mini_board`, not a frozen surface but recorded here because
  they are why the cap costs less than it saves: each source's status candidates are resolved once
  a statement (`status_paths`, through `declared_candidates!` itself) instead of the declaration
  being walked per row, with `first_string_of!` the one spelling of `declared_string!`'s rule;
  the observed statuses are materialized once; and both statements are planned per execution
  (`persistent(false)`), because PostgreSQL's generic plan, chosen on a reused statement's sixth
  run, took 6.6 s where the statement's own plan takes 0.1 s. Measured on 2026-10-13 (load
  average 5–17): the *All* board at 50 a column, 97 ms and 36 KB.
