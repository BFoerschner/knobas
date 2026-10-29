- **The IPC command schema, issue #760 (2026-10-14): `EntityDetail` gains `status`, and
  `commands::entity` gains the command `source_statuses`.** Signed off under Björn's 2026-10-13
  delegation, the one #745's entry names, and ruled by the deputy on the issue. No migration, no
  Source SPI change, no new module. It amends #753's entry ("the ticket detail's status select
  sends nothing and reads the whole board ... a narrower one is follow-up work") and #179's.

  **Why.** Opening a ticket read `mini_board` for the ticket's whole source to learn two things:
  which column the ticket stands in, and the statuses the source's corpus shows. `just
  responsiveness-probe` (a Jira-shaped source of 5,000 tickets) timed it at **70 ms and 725 KB**
  at a load average of 6-14 (133-137 ms in #753's busier run), for every ticket opened. The cap
  of #753 cannot serve it: a capped board need not contain the ticket's card.

  **`EntityDetail.status: Option<String>`**, read once beside the row in `get_entity_inner`'s own
  statement through `declared_string!(.., "status_name")`, the board's rule. `None` for any kind
  but a ticket, for a ticket whose record carries no declared status (the board's "No status"
  group), and for a withdrawn ticket or a disabled source's, which the board does not draw.

  **The command**, in the existing `entity` command module and `generate_handler!`:

  ```rust
  #[tauri::command] pub async fn source_statuses(.., source_id: String) -> Result<Vec<String>, IpcError>;
  ```

  The statuses one source's live ticket corpus shows, in the board's order, without the terminal
  group: `knobas_core::mini_board::source_statuses`, the same statement (`OBSERVED_STATUSES`) as
  `MiniBoard::sources` and equal to its entry for that source. It is the detail's fallback offer
  when the workflow read fails (#498), so the panel asks only then.

  **The mirror**, in the existing `app/src/lib/ipc/entity.ts`: `EntityDetail.status: string |
  null` and `sourceStatuses(sourceId): Promise<string[]>`. The field is pinned by
  `the_entity_detail_shape_matches_its_typescript_mirror`
  (`crates/knobas-app/tests/it/entity_mirror.rs`). `index.ts` re-exports `entity.ts` wholesale.
