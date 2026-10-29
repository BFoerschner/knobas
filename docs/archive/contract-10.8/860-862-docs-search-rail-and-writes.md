- **`crates/knobas-source/src/**`, a grammar key, Confluence's `body_text`, the IPC command schema
  and the Rust handler barrel — issues #860, #861 and #862 (2026-10-16): the Docs search, the
  reader's rail and *Export as Markdown*, and the Docs writes (*Edit section*, comments, *New
  page*).** Spec #808, *Docs*; requirements R-10.1 to R-10.6, R-10.8, R-10.10, R-10.12 to R-10.17;
  milestone TF4. No migration, no event, no `WriteOp`, no `KindPaths` field, no new module in either
  barrel or in `commands/`.

  **The indexed text a docs source writes at sync** (`knobas_source::docs::indexed`, new in the
  frozen SPI crate). A page's `body_text` keeps, beside its words, lines that open with a control
  character, which `to_tsvector` and `ts_headline` read as separators: `SECTION` (`U+001E`) and a
  heading, whose text below it is that section's; `COMMENT` (`U+001F`) and the author's display
  name, whose text below it is that comment's; and, last, one line per page the body links to,
  `link_line(space, title)` = `U+001D` space `U+001D` title `U+001D`. `read(body_text) -> Indexed {
  runs: Vec<Run { part: Part::{Lead, Section(String), Comment(String)}, text }>, links: Vec<(String,
  String)> }` reads them back; text with no mark is one `Lead`. Confluence writes them
  (`storage::to_indexed_text`, `map::indexed_body_text`): every `h1`–`h6` line is marked, a
  `ri:page` link with no body of its own reads as its title in the sentence, and every `ri:page` is
  a link line in the page's own space where it names none. The adapter's cursor goes to `v: 2`, so
  the first sync after upgrading is a full one and re-indexes every page mirrored before. What a reader is
  shown of `body_text` never carries them: `knobas_core::body_text_shown!(column)` drops an author
  line and a link line whole and a heading's mark alone, and the launcher's excerpt
  (`knobas_search::corpus::LIVE_ITEM`), the proposal evidence (`knobas_core::evidence`) and the
  detail panel (`entity_detail`) read through it; `knobas_search::snippet::segments` drops any mark
  that is left.

  **The grammar key** `space:` is an alias of `project:` (`FieldKey::from_spelling`): a page's space
  is its declared project (ADR-0010). `knobas_search::query::needles(text)` lists the words and
  quoted phrases a query asks for, never one left out (`-word`, `-"…"`) and never `or`.

  **The declarations, grown** (`knobas_source::docs`). `DocContent` gains `sections:
  Vec<DocSection { number: u32, text: Option<String>, refusal: Option<SectionRefusal> }>` and
  `history: Vec<DocVersion { number: Option<u64>, by, by_name, at, message: Option<String> }>`
  (`#[serde(default)]` on both). `SectionRefusal` is `table | code | macro | formatting`
  (snake_case). A section is a top-level heading the reader numbers (1 for the first) and what
  follows it to the next; it is editable when heading and body sit at the top of the body and the
  body is only paragraphs, line breaks and flat bulleted lists of plain text (a numbered list
  refuses as `formatting`). A new projection-adjacent
  surface, `DocWrites { edit_section: fn(kind, &payload, number: u32, text: &str) ->
  Result<SectionEdit { body, base_version: i64 }, EditRefused::{Section(SectionRefusal), Missing}>,
  body_from_text: fn(&str) -> String }`, named per adapter kind in the registry (a new
  `doc_writes` column; Confluence's is `knobas_source_confluence::DOC_WRITES`, the others `None`).
  Confluence's `edit_section` splices the new body between the heading's end and the next
  heading's start, byte for byte elsewhere; `body_from_text` (`storage::from_plain`) reads `## ` (to
  six `#`) as a heading, `- ` or `* ` as a list item, a blank line as a paragraph break, and
  escapes everything else.

  **The commands**, in the **existing** `commands/entity.rs`; the reads and payloads are
  `knobas_app::docs`:

  ```rust
  #[tauri::command] pub async fn docs_search(.., query: String, space: Option<DocsScope { source_id, key }>)
      -> Result<Option<DocsHits>, IpcError>;
  #[tauri::command] pub async fn docs_export_markdown(.., entity_id: String) -> Result<DocExport { file, folder }, IpcError>;
  #[tauri::command] pub async fn docs_edit_section(.., entity_id: String, section: u32, version: Option<u64>, text: String)
      -> Result<QueuedWrite, IpcError>;
  #[tauri::command] pub async fn docs_create_page(.., page: NewPage { parent, title, text }, link_to: Option<String>)
      -> Result<QueuedWrite, IpcError>;
  ```

  `docs_search` is `Searcher::page_search` (the launcher's `parse`, `merge` and `search_sql` over
  the live mirror, kept to `page`, at most 50, a typed `space:` winning over the picker's) with,
  per hit, the snippet the backend cuts from the marked `body_text`: `DocsHits { total, took_ms,
  needles, unknown_tokens, hits: Vec<DocHit { entity_id, title, space: Option<DocSpaceRef>, path:
  Option<String>, snippet: DocSnippet { section, comment_by: Option<String>, segments:
  Vec<Segment> }, matches: u32, editor, editor_name, updated_at }> }`; `None` for a query with
  neither words nor a filter. The best sentence holds the most needles (prototype-6's stemming: a
  word over four letters loses one `ing`/`es`/`ed`/`s`), cut to 150 characters around its first
  match. `docs_export_markdown` writes `page_file_name(title)` into Downloads through
  `knobas_core::markdown` (the projection's blocks mapped onto its `DocBlock`). `docs_edit_section`
  queues `UpdatePage` from `DocWrites::edit_section`, refusing a section the adapter refuses and a
  page whose mirrored version is not `version`. `docs_create_page` queues `CreatePage` under a
  mirrored page in that page's space, its body `body_from_text`; with `link_to` (a ticket id) a
  `documents` link from the new page is drawn once the write settled and a sync brought it in
  (`knobas.write_queue.remote_id`).

  `DocsTree` gains `for_you: Vec<DocForYou { entity_id, title, space_key, by_name }>`: the inbox's
  open `mention` items that are pages, with who mentioned the reader (the newest comment that does,
  else the editor). `DocPage` gains `sections`, `history`, `backlinks: Vec<DocBacklink { entity_id,
  title, space_key }>` (the source's live pages whose `body_text` holds this page's link line,
  narrowed by the title's words through the full-text index), `work: Vec<DocWork { entity_id, key,
  title, status, linked }>` (ticket chips, then confirmed knobas links to tickets, with the declared
  status) and `writes: Vec<String>` (`comment`, `update_page`, `create_page` where the adapter
  declares them; the two page writes only where it has `DocWrites`). A comment goes through the
  existing `submit_write` (`WriteOp::Comment`). Mirrored in `app/src/lib/ipc/entity.ts`, held by
  `entity_mirror.rs`'s `the_docs_shapes_match_their_typescript_mirror`. Witnessed against the real
  product by `confluence_live.rs`'s
  `a_synced_page_keeps_its_sections_and_links_for_search_and_backlinks`. The fake IPC answers all
  four (`shell/dev/fake/docs.ts`).
