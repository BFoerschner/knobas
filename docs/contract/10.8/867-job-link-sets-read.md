- **The IPC command schema, the Rust handler barrel and `knobas_core::link_sets_applying!`'s
  CTEs — issue #867 (2026-10-16): `job_link_sets`, what a job shows beside its own links.** Spec
  #808, *Link sets* (read with the job, never stored as links, the parent's links behind the
  global switch); requirements R-4.4, R-6.2, R-7.3, R-7.5; ADR-0025; milestone TF5. No migration,
  no event, no `WriteOp`, no grammar key, no declared path, nothing under
  `crates/knobas-source/src/**`, no new module in either barrel or in `commands/`; `job_reach`'s
  answer is unchanged.

  **The macro.** `link_sets_applying!` gains `rule_applies(t, set_id, position, kind, value)`,
  each rule that matches the ticket, with the text `set_applies` had; `set_applies(t, set_id)` is
  now `select distinct t, set_id from rule_applies`, so the reach and this read match rules with
  one text.

  **The command**, in the existing `commands/entity.rs`, appended at the foot of the handler list:

  ```rust
  #[tauri::command] pub async fn job_link_sets(.., entity_id: String)
      -> Result<JobLinkSets, IpcError>;
  pub struct JobMember { pub kind: String, pub entity_id: String,
      pub entity_kind: Option<String>, pub title: Option<String>, pub hidden: bool }
  pub struct JobSet { pub id: String, pub name: String, pub via: RuleRow,
      pub members: Vec<JobMember> }
  pub struct JobLink { pub entity_id: String, pub kind: String, pub title: Option<String> }
  pub struct JobParent { pub ticket: String, pub title: Option<String>, pub links: Vec<JobLink> }
  pub struct JobLinkSets { pub sets: Vec<JobSet>, pub parent: Option<JobParent> }
  ```

  `sets` is every set whose rules match the ticket, by name then id, each once with `via` the
  first matching rule in the set's order (its ticket's title, or the project's name), and its
  fixed members in order, the ticket itself left out, each `hidden` when `link_set_hidden` holds
  it for this ticket. `parent` is `reach_walk!`'s own `parent` term (the declared parent, never an
  epic, `None` while R-7.3's switch is off) with its confirmed links' far ends, the ticket and
  contexts left out. An id nothing answers to has no sets and no parent. Nothing is written but
  the binary's parent declarations, as every link-set read does (#627). Mirrored in
  `app/src/lib/ipc/entity.ts` (`JobMember`, `JobSet`, `JobLink`, `JobParent`, `JobLinkSets`,
  `jobLinkSets`), held by `entity_mirror.rs`'s `the_link_set_shapes_match_their_typescript_mirror`,
  answered by the fake IPC (`fake/link-sets.ts`).

  Pinned by `crates/knobas-app/tests/it/link_sets_ipc.rs`: `the_job_reads_each_set_that_applies_with_its_reason`
  (name order, only what applies, the first matching rule by position, members with their hide,
  a hide leaving the set whole, an unknown id) and `a_sub_task_reads_its_parents_own_links_behind_the_switch`.
