- **`crates/knobas-source/src/**`, the IPC schema of `list_adapters` and the `sources.ts` mirror —
  issue #1193 (2026-10-18): a descriptor claims roles, and the battery holds each claim to its
  role's minimum.** No migration, no new command, no `commands/` or `ipc/` module, no `WriteOp`,
  no `KindPaths` field, no change to `crates/knobas-app/src/error.rs`. Parent spec #1192
  (ADR-0029, glossary **Role**).

  **The declaration.** `SourceDescriptor` gains `roles: Vec<RoleClaim>`, `#[serde(default)]` on the
  `payload_paths` precedent: a descriptor from a peer that predates the field decodes as an adapter
  that claims nothing. `knobas_source::role::RoleClaim { role: String, reads: Vec<String> }`
  (`reads` also `#[serde(default)]`). `role` is a built-in role or a custom role
  `<adapter_kind>:<name>` (name a lowercase slug); a custom claim has no reads and no minimum
  (#1201 gives it a view). A role's writes are not in the claim: each is declared, one by one, in
  `write_ops` as every write is. Mirrored in `app/src/lib/ipc/sources.ts` as `roles: RoleClaim[]`
  and held by `sources_mirror.rs` (`SourceDescriptor` and `RoleClaim`); the fake IPC's templates
  carry the real adapters' claims.

  **The five minimums** (`knobas_source::role::MINIMUMS`). Kinds by id, or for CI by the
  ADR-0022 frame a kind declares. Paths are `KindPaths` fields that must have a candidate.

  | Role | Kinds and declared paths | Required reads | Optional reads | Write vocabulary |
  |---|---|---|---|---|
  | `code-host` | `repo`; `branch`: parent (a `repo`), `commit`, `default_branch`; `pr`: parent (a `repo`), `state`, `head`, `commit`, `reviewers`, `merged`; `commit`: parent (a `repo`), `commit` | `pull_request_diff`, `commit_diff`, `repository_tree`, `file_contents`, `line_threads` | none | `create_branch`, `create_pull_request`, `comment`, `approve`, `submit_review`, `merge_pull_request`, `edit_pull_request`, `update_pull_request_branch`, `delete_branch`, `commit_file` |
  | `ticket-system` | `ticket`: `status_name`, `priority`, `assignee`, `issue_type`, `labels`, `parent`, `project_key` | `reachable_transitions` | none | `transition`, `comment`, `create_ticket`, `log_work` |
  | `ci` | a pipeline (frame `pipeline`): `name`, `pipeline`; a run (frame `run`): `pipeline`, `state`, `number`, `branch`, `started`, `finished` | `run_log` | `pipeline_settings`, `older_runs`, `run_tests`, `test_history`, `run_artifacts`, `artifact_preview`, `download`, `run_chain` | `run_build`, `rerun_build`, `trigger_build`, `set_parameter` |
  | `wiki` | `page`: `project_key` (the space), `parent` | `doc_content` | none | `create_page`, `update_page`, `comment` |
  | `monitoring` | `monitor`: `status_name` (the state samples are taken from) | none | none | `create_monitor`, `pause_monitor`, `resume_monitor` |

  A read's identifier is the `Source` method's name where there is one. The code-host reads are
  answered by `view` blocks (a `diff` with line threads, a `tree`, a `file`), and `doc_content` by
  the adapter's `DocProjection`; `sources_registry.rs` holds a `doc_content` claim to the
  registry row's projection, both directions. Not in the minimums yet, each joining with the
  ticket that adds its SPI surface: clone URLs (#1197), saved views (#1207), `translate_query`
  (#1208), a monitor's response time (#1224), a pull request's base branch (#1227). A ticket's key
  is not a declared path: it is the `EntityRef` key (§4.1), which every ticket carries.

  **The check.** `knobas_source::role::check(&SourceDescriptor) -> Result<(), ClaimError>` reads the
  descriptor and nothing else (ADR-0028's install-time check) and answers the first item missing,
  by name: an unknown or foreign-namespaced role, a role claimed twice, a missing kind, a missing
  path, a missing required read, or a read the role does not have. The contract battery runs it
  as **clause 10** and panics with `descriptor.roles: <error>`; clause 6 holds the declared paths
  to the adapter's own records, as before.

  **The built-ins.** Jira claims `ticket-system`, Gitea `code-host`, TeamCity `ci` (every
  Builds-frame read), Confluence `wiki`, Uptime Kuma `monitoring` (with or without its account).
  Each fills its role (a unit test per adapter, plus the battery). The mock claims nothing.
