- **`crates/knobas-source/src/**`, the IPC command schema and the Rust handler barrel — issue #859
  (2026-10-16): the Docs frame's declarations (ADR-0022), Confluence's projection of a page into
  them, and two read commands, `docs_tree` and `docs_page`.** Spec #808, *Docs*; requirements
  R-10.1, R-10.2, R-10.9 to R-10.11, R-10.18; milestone TF4. No migration, no event, no grammar key,
  no `WriteOp`, no `KindPaths` field, no new module in either barrel or in `commands/`.

  **The declarations** (`knobas_source::docs`, a new module of the frozen SPI crate). The frame is
  drawn over the `page` kind (`PAGE_KIND`, knobas' word since migration `0001`). A space is the
  page's declared project (`project_key`, `project_name`, ADR-0010) and the tree is its declared
  parent (`parent`, ADR-0018): both existing `KindPaths` fields, unchanged. The rest is a
  **projection**, `pub type DocProjection = fn(kind: &str, payload: &serde_json::Value) ->
  Option<DocContent>`, a plain function of the mirrored record (no network, no credential), named
  per adapter kind in `knobas_app::sources::registry`'s table (a new `doc` column; Confluence's is
  `knobas_source_confluence::doc_content`, the others `None`). `None` is a kind or payload the
  adapter does not draw as a page.

  ```rust
  pub struct DocContent { version: Option<u64>, editor_name: Option<String>, labels: Vec<String>,
                          blocks: Vec<DocBlock>, comments: Vec<DocComment> }
  pub struct DocComment { author: Option<String>, author_name: Option<String>,
                          at: Option<String>, blocks: Vec<DocBlock> }
  #[serde(tag = "type", rename_all = "snake_case")]
  pub enum DocBlock { Heading { level: u8, text: Vec<Inline> }, Paragraph { text: Vec<Inline> },
      List { ordered: bool, items: Vec<Vec<DocBlock>> }, Code { code: String, language: Option<String> },
      Table { rows: Vec<Vec<TableCell>> }, Panel { kind: PanelKind, title: Option<String>, blocks: Vec<DocBlock> },
      Placeholder { name: String } }
  pub struct TableCell { header: bool, text: Vec<Inline> }
  pub enum PanelKind { Info, Note, Warning, Tip }            // snake_case
  #[serde(tag = "type", rename_all = "snake_case")]
  pub enum Inline { Text { text, strong, emphasis, code }, Link { text, href },
      PageLink { text, title, space: Option<String>, entity_id: Option<String> },
      Issue { key, entity_id: Option<String>, title: Option<String>, status: Option<String> },
      Mention { user: Option<String>, name: Option<String>, me: bool }, Break }
  ```

  The adapter fills `PageLink`'s title and space, `Issue`'s key and `Mention`'s user and name;
  knobas fills `entity_id`, `title`, `status` and `me` from the mirror when the reader is read
  (`#[serde(default)]` on each). Adding a block, an inline, a panel kind or a field is a §10.8 entry, as
  for ADR-0020's `view::Block`.

  **Confluence's subset** (`knobas_source_confluence::blocks`): `h1`–`h6`, `p`, `ul`/`ol` (nested),
  `table` with `th`/`td`, `pre` and the `code`/`noformat` macros (with `language`), the `info`,
  `note`, `warning`, `tip` and `panel` macros (`panel` is a note), the `jira` macro with a `key`,
  `ac:link` to `ri:user` (by `ri:username`, or by `ri:userkey` where the record names that user:
  the page's editor, creator or a comment's author; another key is a mention of nobody, the
  ADR-0007 miss), `ac:link` to `ri:page`, `a href`, and `strong`/`b`, `em`/`i`, `code`. **Every
  other macro, `ac:image` and `ac:task-list` is a `Placeholder` with its name**; a block macro
  inside a sentence ends the paragraph there. `api::EXPAND` gains `metadata.labels`.

  **The commands**, in the **existing** `commands/entity.rs`; the reads are `knobas_app::docs`:

  ```rust
  #[tauri::command] pub async fn docs_tree(..) -> Result<knobas_app::docs::DocsTree, IpcError>;
  #[tauri::command] pub async fn docs_page(.., entity_id: String)
      -> Result<knobas_app::docs::DocPage, IpcError>;
  ```

  `DocsTree { spaces: Vec<DocSpace>, last_read: Option<String> }`; `DocSpace { source_id, key,
  name: Option<String>, pages: Vec<DocNode> }`; `DocNode { entity_id, title, parent: Option<String>
  }`. Every live `page` with a stored project (`sync.item_project`, joined to its source's adapter),
  by source, key and title. A page's parent is the innermost name of its declared chain
  (`knobas_core::declared_parent_names!`) matched to a page of the same space by title, else
  `None`. `last_read` is the newest `visited` line (`visit_entity`, #819) on a live page.

  `DocPage { entity_id, source_id, source_name, title, space: Option<DocSpaceRef { key, name }>,
  ancestors: Vec<DocAncestor { title, entity_id: Option<String> }>, editor: Option<String>,
  editor_name: Option<String>, updated_at, web_url, readable: bool, version, labels, blocks,
  comments, me: Vec<String> }`. `source_name` is the configured display name, else the id;
  `editor` is the normalized `author`; `readable` is whether the source's adapter has a
  projection; `me` is `@me`'s identity. A page link resolves to the live page with that title in
  the named space (else the page's own), a ticket chip to the live `ticket` whose id past its
  namespace is the key, with the declared `status_name`; a mention is `me` when its user is in the
  identity. `invalid` for a non-id; `not_found` for an id the live mirror does not hold or that is
  not a `page`. Mirrored in `app/src/lib/ipc/entity.ts`, held by `entity_mirror.rs`'s
  `the_docs_shapes_match_their_typescript_mirror`. Witnessed against the real product by
  `confluence_live.rs`'s `a_page_with_every_mapped_macro_reads_as_the_readers_blocks`. The fake
  IPC answers both (`shell/dev/fake/docs.ts`).
