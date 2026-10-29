- **`crates/knobas-source/src/**` and the IPC surface, issue #277 (2026-09-03): the source
  descriptor grows declared payload paths, and every landed payload read outside an adapter
  expires into them.** ADR-0007 ratified the interim discipline for a payload read — miss, one
  named statement, a pinned failure direction — and recorded its destination in as many words:
  "the eventual shape is descriptor-declared … each read this ADR governs expires into the
  declared field as it arrives". This is that growth, by ADR-0006's mechanism, and it is the
  first `SourceDescriptor` growth since the M1 freeze.

  **The descriptor field.** `SourceDescriptor::payload_paths: Vec<KindPaths>`, `#[serde(default)]`
  so a descriptor from a peer built before this — an out-of-process adapter, a stored blob —
  decodes as an adapter that declares nothing. One `KindPaths` per entity kind, carrying `kind`
  plus seven declared paths and one declared set: `status_name`, `priority`, `assignee`,
  `project_key`, `project_name` (each a list of `PayloadPath` candidates, first that lands on a
  non-blank string wins), `reviewers` (a list of `ListPath`, each an array path plus the path to
  the string inside an element, unioned), `merged` (a **boolean** — a merge timestamp is a
  different fact with a different absence), and `blocked_statuses`, the source's own spellings of
  "stuck", for M3.3's digest.

  **Where the types live, and why not here.** `PayloadPath`, `ListPath`, `KindPaths` and
  `Declarations` are `knobas_core::payload`'s, re-exported from `knobas-source`. The adapters that
  write a declaration depend on the SPI; the readers that resolve one are in `knobas-core`, which
  may not depend on the SPI (`knobas-source` depends on `knobas-core`, never the reverse). Core is
  the only crate both can see. Nothing about this makes core adapter-aware: it is handed a
  declaration and does not go looking for one.

  **The adapters declaring them.** Jira: `ticket` — `fields.status.name`, `fields.priority.name`,
  `fields.assignee.name` then `fields.assignee.key` (two spellings of *one adapter's own* field,
  which is what a candidate list is for), `fields.project.key`/`name`, and `Blocked` / `On Hold` /
  `Impediment`. Gitea: `pr` — `requested_reviewers[].login` and the boolean `merged`; no project,
  because ADR-0010 gives Gitea none. TeamCity: `build` — `buildType.projectId`/`projectName`;
  `build_config` — the top-level `projectId`/`projectName`, since that record *is* the `buildType`
  object. Mock: `ticket` — the fixture's flat `status` and `priority`, and `fields.project` where
  a source would have written it. The **Confluence adapter (#284) declares its own when it lands**;
  the place is its own `descriptor.rs`, and nothing here is edited for it.

  **The contract battery grows clause 6**, which is what holds a declaration to the adapter's own
  corpus: every declared kind is one of `entity_kinds` and is declared once; no declared path leads
  to a value of the wrong type; where nothing of a kind resolved a declared field, no record of
  that kind may show the path naming a key it does not have (the misspelling check, asked of the
  corpus rather than of a record, so an unassigned issue — whose walk stops at a `null` — cannot
  fail a declaration, and the clause is safe to run against a live instance); and a field a kind
  does not declare resolves to nothing, which is the clause a knobas-side fallback would die on.
  **Two declarations the clause cannot see**, recorded so neither is later read as a hole it
  closed: a kind the corpus never populates is asked nothing at all (demanding every declared kind
  be populated would fail a run against an instance with no build configurations, the same move
  clause 3 refuses for an unassigned issue), and a candidate an earlier candidate resolves for is
  excused — `fields.assignee.keyy` after a working `fields.assignee.name` passes, because two
  candidates are one adapter's alternative spellings and an instance uses one of them, so
  per-candidate evidence would fail the second spelling wherever it is the unused one. A candidate
  list is certified as a whole; the first candidate is the one clause 3 really pins.
  **That assignee example holds only on an instance whose assignee is populated**, and the
  battery's own corpus is not one: its items leave `fields.assignee` explicitly null, so both
  spellings stop at that null, neither resolves and neither names a missing key — the pair is
  silent under this tolerance and under per-candidate evidence alike, and witnesses nothing. The
  example the tests pin is therefore a field the corpus really resolves: `fields.status.nam`
  behind a working `fields.status.name` (#303, PR #308,
  `accepts_a_later_candidate_an_earlier_one_resolves_for`).

  **`knobas_core::string_at!` is retired with its last call site.** It was ADR-0007's interim
  shape — SQL for the string a *literal* path leads to, with the three refusals — and every
  statement that expanded it is in the list below. Rather than leave an exported macro with no
  expander and a doc naming call sites that no longer exist, it is gone; `declared_string!` makes
  the same three refusals and `payload::resolve_string` is their Rust half.

  **The reads that expired**, all of them payload reads ADR-0007 governs, and **none of them
  changed its failure direction** — each is still pinned by the test named on it:
  `knobas_core::mini_board`'s status and priority (`status_read!`, `priority_read!` — the
  per-source `coalesce` arms are gone); `knobas_core::project`'s key and name
  (`project_key_read!`, `project_name_read!`, including #232's `case when i.kind = 'build_config'`
  guard, which is now the declaration being per kind); `knobas_core::inbox`'s review-request
  reviewers and new-assignment assignee; and `knobas_app::start_work::merge`'s merged flag. The
  four room statements in `commands/entity.rs` narrow through the same exported
  `project_key_read!`, so a room still cannot disagree with the census about what a project is.

  **Two reads deliberately did not expire**, and are recorded here so the next reader does not
  read the omission as an oversight: `knobas_core::suggest`'s `fields.issuelinks` walk, which is a
  *relation* rather than one of the declared fields, and `knobas_core::inbox`'s failed-build rule,
  which reads a build's **outcome** and its configuration — a different fact from any declared
  field. `knobas_core::context`'s epic, issue-type and parent reads and
  `knobas_app::start_work::queue`'s look-before-write on a pull request's head branch likewise.
  Each keeps ADR-0007's interim discipline; each is a later ticket's to expire, with its own
  entry, if ever.
  `write_queue::project`'s **refusal** to read payload shapes is untouched: that is the write
  direction, which this does not relax.

  **How a reader learns a source's paths, and the option declined.** The app layer resolves them —
  `knobas_app::sources::paths::declared_paths(pool, registry)` joins the registry's descriptor
  *templates* to `knobas.source_config`'s rows and hands `knobas_core::payload::Declarations` into
  the read, which binds it as **one jsonb parameter**. The reads stay one named statement each, as
  ADR-0007 requires, and stay static `&'static str`: the declaration crosses as a parameter and
  `sqlx` 0.9's `SqlSafeStr` makes that structural rather than a habit. The **declined** option was
  a persisted declaration per source row: it would have left every core signature alone and cost a
  migration on this section's frozen list, plus a writer on the bring-up path, to cache something a
  `const` table answers in microseconds — and a stored copy is stale from the moment an adapter
  learns a spelling. **This adds no migration at all**, so it claims no number: `0013` is the
  timer's (#278, the entry above), and the next free one is whatever that leaves.

  **The IPC touch.** No new command, no new event, no new module on either side, and no change to
  any DTO the frontend acts on — `payload_paths` rides inside the `SourceDescriptor` that
  `list_adapters` and the sources view already receive. `app/src/lib/ipc/sources.ts` gains the
  field and three declarations (`KindPaths`, `ListPath`, `PayloadPath`), at the foot of the section
  that already declares `SourceDescriptor`; the barrel's `export * from "./sources"` is untouched.
  `crates/knobas-app/tests/sources_mirror.rs` pins both new interfaces by their exact key sets, the
  treatment `KindInfo` gets. The `mini_board`, `list_projects` and `list_entities` commands reach
  `Registry::builtin()` for the templates rather than `SourcesState` — the same reading
  `list_adapters` makes, and deliberately so: a declaration is compiled-in data, and reaching
  through the scheduler's state would widen `not_ready` on a room's list for nothing. The inbox and
  the merge pass keep using the registry they are handed.

  Ratified by the orchestrator as spec #272 and issue #277, whose acceptance criteria specify the
  descriptor fields, the battery clauses, the expired reads and this entry.

