- **The IPC command schema, the Rust handler barrel and the search grammar — issue #817
  (2026-10-16): the job-reach read (`job_reach`), `reach:<ticket>`, `EntityFilter.reach`, and
  R-7.3's switch (`parent_links`, `set_parent_links`).** Spec #808, *The job-reach read*;
  requirements R-2.8, R-2.9, R-7.3; ADR-0025; milestone TF1. No migration, no event, no `WriteOp`,
  nothing under `crates/knobas-source/src/**`, no new module in either barrel or in `commands/`.

  **The read.** `knobas_core::reach` (a new module in an unfrozen crate) holds the walk as one
  text, `knobas_core::reach_walk!`, read by both callers below so they cannot disagree. Amended
  by #942 (Björn's ruling of 2026-10-18, ADR-0025's amendment): the walk is R-4.17's and
  R-4.18's. It starts where ADR-0008's membership starts (the ticket, its unarchived context's
  explicit adds, an epic's children) and takes two link layers over `knobas.confirmed_link`,
  never through a context, then what is `held` under what was reached, with each row's first
  step kept; then every ancestor of a reached asset (`under`) and every `monitored-by` monitor of
  one (`watched`). From an asset typed `vm`, `reverse_proxy`, `hypervisor`, `site` or `network`
  (`knobas_core::reach::DOWNWARD_ONLY`) it only goes down: a link step out of one follows
  `monitored-by` and its own `runs-on`/`depends-on` (read from the `from` end), and nothing held
  under it comes in. Behind the switch, a ticket also
  reaches its parent's own links, one layer: the parent is `fields.parent.key` in the ticket's
  source, counted only when it is mirrored and its `fields.issuetype.name` reads as something other
  than an epic (ADR-0007 reads, both missing toward no parent). In the
  **existing** `commands/entity.rs`:

  ```rust
  #[tauri::command] pub async fn job_reach(.., entity_id: String)
      -> Result<Vec<knobas_core::reach::Reached>, IpcError>;
  #[tauri::command] pub async fn parent_links(..) -> Result<bool, IpcError>;
  #[tauri::command] pub async fn set_parent_links(.., enabled: bool) -> Result<bool, IpcError>;
  ```

  `Reached { entity_id, kind, step, origin }`: `step` is the first step of the path that brought
  the entity in (an entity id, equal to `entity_id` for a first step); `origin` is internally
  tagged on `kind`, `{"kind":"own"}` or `{"kind":"parent","ticket":<id>}`, so TF5's link sets add
  an arm without changing the shape. One row per entity: own paths before the parent's, shortest
  first. An unknown id answers `[]`.

  **The switch** is the `knobas.setting` row `job.parent_links`: no row is on (R-7.3's default),
  the JSON string `"off"` is off. `set_parent_links` answers what is now stored.

  **`EntityFilter` gains `reach: Option<String>`**, `#[serde(default)]` (absent reads as `None`,
  so every existing caller is unscoped as before): a ticket id whose reach narrows the rows by the
  same `$6` id set a context narrows them by; with both, a row must be in both. Mirrored in
  `app/src/lib/ipc/entity.ts` as `reach?: string | null`, `ReachOrigin`, `Reached`, `jobReach`,
  `parentLinks`, `setParentLinks`, and in the fake IPC.

  **The grammar key `reach:<ticket>`** (`knobas_search::query`), typed only like `under:` (no chip,
  no echo): the value is a ticket key or entity id, matched ignoring case; repeated values OR. Each
  corpus keeps `<entity_id> in (<the walk from every ticket answering to a value>)`. A key nothing
  answers to reaches nothing.

  Membership is unchanged: `context::member_ids`, the proposal tray, the per-context inbox and the
  context export read ADR-0008's statement exactly as before. Pinned by
  `crates/knobas-core/tests/it/reach.rs` (one hop and no further, reasons name the first step, a
  container bringing its engine, VM, host and their monitors and never the container beside it,
  no link step up from a VM or a proxy, a VM the context adds going only down, nothing held under a VM while a service brings its
  modules, a promoted sub-task's `member_ids` unchanged and the reach parting from it only above
  the VM, a parent's VM coming in alone, parent links labelled behind the switch and the
  sub-task's own link winning, an epic parent or an unreadable one bringing nothing),
  `crates/knobas-search/tests/it/reach.rs` (`reach:` by key and by id returns the read's ids),
  `job_ipc.rs`'s `list_entities_scoped_by_reach_keeps_what_the_ticket_reaches`, and
  `entity_mirror.rs`'s `the_reached_shape_matches_its_typescript_mirror` and
  `the_entity_filter_shape_matches_its_typescript_mirror`.
