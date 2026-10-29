- **The IPC command schema, the Rust handler barrel and what `recent_activity` answers — issue #819
  (2026-10-16): two new commands, `visit_entity` and `entity_thread`, the job's *last visit* and
  its thread (R-5.1 to R-5.5).** Spec #808, "Signals, conversation and proposals": *"Last visit is
  a per-entity last-opened time from the activity stream. Mentions me is a named payload read
  matching the account's … name in comment bodies."* No migration, no grammar key, no declaration
  field, no `WriteOp`, no `commands/` or `ipc/` module. In the **existing** `commands/entity.rs`:

  ```rust
  #[tauri::command] pub async fn visit_entity(.., entity_id: String)
      -> Result<Option<DateTime<Utc>>, IpcError>;
  #[tauri::command] pub async fn entity_thread(.., entity_id: String)
      -> Result<knobas_app::conversation::Thread, IpcError>;
  ```

  **`visit_entity`** writes one `knobas.activity` line, actor `user`, verb `visited`
  (`knobas_core::activity::VISITED`), and answers the newest such line on the entity from before
  it, `null` on a first visit; the read and the line are one transaction. `invalid` for a non-id,
  `not_found` for an id `knobas.entity` does not hold (and then nothing is written).

  **`entity_thread`** answers `Thread { comments: Vec<ThreadComment>, changes:
  Vec<ThreadChange>, me: Vec<String> }`; `ThreadComment { author: Option<String>, at:
  Option<DateTime<Utc>>, body: String, mine: bool, mentions_me: bool }`; `ThreadChange { at:
  DateTime<Utc>, status: String }`. The comment list is one named payload read (ADR-0007),
  `conversation::COMMENTS`: `fields.comment.comments` (Jira) or `comments` (the mock adapter),
  whichever is a JSON array, else an empty list; each entry's author, account, stamp and body are
  read with two spellings, a blank or non-string value a miss and an entry with no body dropped.
  **Failure direction: a miss is an empty thread**, pinned by
  `a_payload_of_another_shape_reads_as_no_comments`. `me` is the identity `@me` resolves to
  (`knobas_search::Vocabulary::identity`, the inbox's); `mine` is the author's account in it, and
  `mentions_me` the inbox's mention rule (`@name` or `[~name]`, not followed by a name character,
  `.` or `-`). `changes` are the `transition` writes the queue settled as `sent`, the status read
  from `WriteOp`'s serde shape `{"Transition": {"status"}}` (a knobas-owned shape, ADR-0007's
  requirements 2 and 3; a drift misses the line, pinned by
  `a_status_move_knobas_sent_is_a_change_and_not_a_comment`). `not_found` for an entity the mirror
  does not hold.

  **What `recent_activity` answers.** `knobas_core::activity::recent`, behind `recent_activity`,
  leaves out `visited` lines, globally and per entity: the history panel and the status bar draw
  what happened to an item, and a line per opened job would push it off both. The worklog draft's
  `NOT_WORK` gains `visited` for the same reason. Pinned by `a_visit_is_not_in_the_recent_activity`.

  Appended at the foot of `lib.rs`'s handler list. Mirrored in `app/src/lib/ipc/entity.ts` as
  `ThreadComment`, `ThreadChange`, `Thread`, `visitEntity` and `entityThread`, and in the fake IPC
  (`fake/entity.ts`). Held by `conversation::tests::the_thread_and_its_mirror_agree`; witnessed by
  `tests/it/conversation_ipc.rs`.
