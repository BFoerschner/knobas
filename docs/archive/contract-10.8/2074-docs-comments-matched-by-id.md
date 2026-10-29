- **The SPI's `DocComment`, `WriteReceipt`'s doc and what `docs_page` answers — issue #2074
  (2026-10-25): a Docs comment carries the source's id for it, and the page read carries the id
  each sent comment write was given.** Björn approved the `DocComment` field, the `docs_page`
  answer and the `WriteReceipt` doc in triage on 2026-10-25; `DocPage::sent_comments` was ruled
  in by the implementer (Opus) under that approval, because no read brought a settled write's
  `remote_id` to the frontend (`pending_writes` lists open rows only, and `QueuedWrite` carries
  no `remote_id`). No migration, no event, no command, no `WriteOp`, no new module in
  `commands/` or `ipc/`.

  ```rust
  // crates/knobas-source/src/docs.rs
  pub struct DocComment {
      #[serde(default)]
      pub id: Option<String>,   // new: the source's id for the comment
      pub author: Option<String>, pub author_name: Option<String>,
      pub at: Option<String>, pub blocks: Vec<DocBlock>,
  }

  // crates/knobas-app/src/docs.rs
  pub struct DocPage { /* .. */ pub comments: Vec<DocComment>,
      pub sent_comments: Vec<DocSentComment>, // new
      /* .. */ }
  pub struct DocSentComment { pub write_id: i64, pub comment_id: String } // new
  ```

  `DocComment::id` is filled by the Confluence projection from the comment record's `id` (the
  content id its comment write answers); the mock source leaves it `None`, as does any source
  whose comments carry no id. `DocPage::sent_comments` is every `comment` row of
  `knobas.write_queue` on the page in state `sent` with a `remote_id`, oldest first, and empty for
  a page whose source does not project it. Docs drops a queued comment whose write is among them
  once `comments` holds that id, whatever its words; a mark whose write is not among them falls
  back to `sameWords`. Mirrored in `app/src/lib/ipc/generated/entity.ts` (regenerated); the fake
  IPC (`fake/docs.ts`) answers `id: null` and no sent comments.

  **`WriteReceipt::remote_id`'s doc** now also covers an id that names something the write created
  *inside* an entity (a Confluence comment, held in its page's payload), for which
  `<source_id>:<remote_id>` is not an entity id. Every reader that builds an entity id from the
  column reads it off an op or kind whose id is an entity's: `docs::created_page` (`CreatePage`
  only, via `link_on_arrival`'s match), `protocol::publication_of` (`op = create_page`), the
  assets' `ON_ITS_WAY` (`op = create_monitor`), `entity_named` (`sync.live_item.kind`). The
  Confluence adapter's `Source::write` now answers `WriteReceipt::id(<comment id>)` for a
  `Comment`, and the SPI's `Source::write` doc lists the receipts that exist. Witnessed by
  `source.rs`'s `a_comment_answers_the_id_confluence_gave_it`, `write_queue.rs`'s
  `a_sent_comment_keeps_the_id_its_source_gave_it`, `blocks.rs`'s
  `a_page_record_projects_its_version_labels_and_comments`, `docs_ipc.rs`'s
  `a_page_read_names_its_comments_and_the_ids_its_sent_comments_were_given`, and live by
  `confluence_live.rs`'s `the_three_page_writes_go_through_the_queue_and_come_back_from_confluence`.
