- **`crates/knobas-db/migrations/0010_contexts.sql` and the IPC command schema with both
  append-only barrels, issue #47 (2026-08-29):** Contexts complete — promote, ad-hoc, one-hop
  membership. Both halves granted under the **2026-08-29 delegation** (recorded in the #43
  entry above): migrations and IPC additions merge under Fable merge-managers; milestone exits
  and the contract battery's clauses were not delegated and remain Björn's. **This PR touches
  neither.** The membership rule itself is ratified separately as
  `docs/adr/0008-context-membership-is-seed-direct-links-one-hop-over-confirmed-links.md`,
  the ADR #47 names as its own deliverable.

  **The migration.** `0010`, allocated to this stream and to nothing else; `0001`–`0009` are
  never edited, and **`0011` is the next free number**. It alters no column and adds no table:
  `knobas.context` has existed since `0001` and nothing ever wrote it. What moves into the
  schema are the two invariants the new writers rely on — `context_kind_chk` closes the
  `epic|ticket|adhoc` list `0001` kept in a comment (same shape and same cross-check discipline
  as `link_origin_chk`: the enum is `knobas_core::context::ContextKind`, pinned from both
  sides), and the partial unique `context_anchor_idx` (one *unarchived* context per anchor) is
  what makes promotion idempotent between its check and its insert. Additive and re-entrant;
  there are no rows anywhere for the CHECK to validate.

  **Membership is computed, never stored** — no membership table, no `implied` rows written on
  promotion. It is `knobas_core::context::member_ids`, one statement over
  `knobas.confirmed_link` (never `knobas.link` — the tray scopes proposals *by* membership, so
  membership built *from* proposals would be circular; `link_reads.rs` holds the line), plus
  the ADR-0007-governed `fields.parent` seed for epic children, confined and pinned to miss
  toward absence. The rationale and the rejected alternatives are the ADR's.

  **Four commands, all in `crates/knobas-app/src/commands/entity.rs`** — the frozen thing is
  the layout, not the existence of commands inside it (#42's reading), so **no module was
  created on either side**; both barrels were appended to, and the TypeScript rides in
  `app/src/lib/ipc/entity.ts`:

  ```rust
  #[tauri::command] pub async fn list_contexts(..)                      -> Result<Vec<knobas_core::context::ContextRow>, IpcError>;
  #[tauri::command] pub async fn create_context(.., title: String)      -> Result<knobas_core::context::ContextRow, IpcError>;
  #[tauri::command] pub async fn promote_context(.., entity_id: String) -> Result<knobas_core::context::ContextRow, IpcError>;
  #[tauri::command] pub async fn context_members(.., ctx_id: String)    -> Result<Vec<String>, IpcError>;
  ```

  One event, `contexts:changed` (payload `ContextRow`), appended to `knobas_app::events` and
  the `EVENTS` mirror — fired on create and on the first promotion only, because the second
  promotion mutates nothing and announces nothing.

  **Two existing surfaces grow additively, recorded because they are schema rather than
  barrel appends.** `EntityFilter` gains `context: Option<String>` (an input DTO: an older
  frontend that omits it decodes as `None`, so every existing caller means what it meant), and
  `room_suggestions` gains `ctx: Option<String>` the same way — the scope
  `suggest::proposals`' own doc reserved for #47. Both are mirrored, and both are pinned by
  `entity_mirror.rs`'s round trip. No existing command, DTO field or event name changes
  meaning; nothing is removed.

