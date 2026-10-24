- **`crates/knobas-source/src/**`, the IPC command schema and the Rust handler barrel — issue #2040
  (2026-10-24): what a source's credential may not write, asked before a write is drawn.** Spec
  #1192, story 10. No migration, no event, no `WriteOp`, no block, no change to an existing DTO.

  - **SPI.** One new module, `knobas_source::grants`, with two plain serde DTOs:
    `WriteAsk { entity, op }` (a write a surface would offer: a `WriteOp` identifier on an
    `EntityRef` string) and `RefusedWrite { entity, op, reason }` (one the credential cannot
    make, with the reason in words a person can act on). One new `Source` method with a
    default, `refused_writes(&self, asks: &[WriteAsk]) -> Result<Vec<RefusedWrite>,
    SourceError>`: the default answers nothing, which is no verdict and draws every write as
    before. That is right for an adapter whose credential cannot be narrower than the adapter,
    and it is what every WASM guest answers (no WIT change, `knobas:adapter@1.3.0` binds
    unchanged); Jira, Confluence and Gitea answer nothing until #2052, #2053 and #2054. An adapter answers from
    a **read** of what its credential may do and never sends a write to find out. No contract
    battery clause. TeamCity is the one adapter that answers it (`crates/knobas-source-teamcity/src/grants.rs`:
    `GET /app/rest/users/current/permissions` and `/app/rest/projects`, recorded in contract
    §4.2's TeamCity read row).
  - **IPC.** One command, `refused_writes`, in `commands/entity/links.rs` beside `entity_view`
    (no new file): `asks: Vec<WriteAsk>` → `Vec<RefusedWrite>`. Each ask goes to its entity's
    own source, built once per source, the sources asked side by side; the answer is in the order
    asked and holds only asks it was given. Infallible past `not_ready`: an entity no source owns,
    a keychain or build failure, and a failed read are each no verdict and refuse nothing. Read
    live and never stored (ADR-0020 rule 1). One line appended at the foot of
    `crates/knobas-app/src/lib.rs`'s handler list, one function (`refusedWrites`) and the
    re-export of the two types in `app/src/lib/ipc/entity.ts`, `WriteAsk` and `RefusedWrite` in
    the generated `app/src/lib/ipc/generated/entity.ts` (`ipc_mirror`), and one handler in the
    `?fake-ipc` fixture (`app/src/lib/shell/dev/fake/entity.ts`, answering nothing refused) with
    its answer type in `app/src/lib/shell/dev/fake-answers.ts`.

  Witnessed by `refused_writes::each_ask_goes_to_its_entitys_source_once_and_the_answers_come_back_together`
  and `refused_writes::what_is_no_verdict_refuses_nothing_and_the_rest_are_still_answered`
  (`knobas-app`'s `it`), TeamCity's `grants::*` (`knobas-source-teamcity`'s `it`), and
  `live_teamcity_seeded.rs`'s `a_viewer_may_run_builds_and_may_not_change_parameters` against the
  real server.
