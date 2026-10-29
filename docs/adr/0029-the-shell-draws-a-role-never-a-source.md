---
status: accepted
---

# The shell draws a role, never a source

ADR-0022 gave Builds and Docs shell-owned frames, but Tickets stayed Jira-shaped, and every new source meant finding out case by case which screens would light up. Decided 2026-10-18 (Björn, custom-sources grilling): **every surface the shell draws beyond one entity's detail belongs to a [role](../../CONTEXT.md#role), and the shell draws it for every source that fills that role, never for a named source.**

- **The normative list of every role's kinds, declared paths, reads, optional reads and writes is `role::MINIMUMS` in `crates/knobas-source/src/role.rs`** (amended 2026-10-22, finding SPI-8: the ADR's own write lists had drifted from the code and an author who followed them was refused by the battery, so the ADR names no write vocabulary). Each role requires its read side; each write is optional and declared on its own, and the shell draws an undeclared write disabled or absent under ADR-0020's rule.
  - **Code host.** `repo`, `branch`, `pr` and `commit`; parent, PR state chip, head and base, the commit a record stands at, requested reviewers, merged flag and default branch; clone URLs; PR, commit and file diffs, the repository tree and file contents, and line threads.
  - **Ticket system.** `ticket` with key, status, priority, assignee, type, labels, parent and project, and `reachable_transitions`, which a source without `transition` answers with a refusal by name (#1225). **Saved views** are optional: each is synced as an item and becomes a ticket view, its kind declaring where the query is and optionally its board type, sub-query and sprint (#2050). `translate_query` is an optional read; JQL is Jira's answer to it.
  - **CI.** ADR-0022's Builds frame: `pipeline`, `run` and `run_log` required; the other live reads and the build writes optional.
  - **Wiki.** ADR-0022's Docs frame: spaces, a page with its parent, and `DocContent`; the page writes optional.
  - **Monitoring.** `monitor` with a state and a response time, from which knobas takes its samples; the monitor writes optional.
- **A source can fill several roles,** and it fills a role only where ADR-0028's checks pass.
- **A source whose items fit no built-in role declares a custom role,** namespaced to the adapter (`grafana:dashboard-host`). Its view is drawn from ADR-0020's blocks, never from UI the adapter ships, and opens when one of its items is opened. A custom role two adapters need becomes a built-in role in a knobas release.
- **A write outside every role's vocabulary is a [source action](../../CONTEXT.md#source-action),** one `WriteOp` variant (`source_action { id, fields }`) whose dialog the adapter declares as ADR-0020's actions do. The queue holds and refuses it like any write and the shell never reads it. A role's ops stay typed because the shell acts on them itself.
- **The mirror answers a role; the shell asks for one and filters nothing** (amended 2026-10-19, Björn, architecture review). Which sources fill a role is decided once, server-side, from the registry's claims and ADR-0028's recorded role checks. A read that is one role's by definition (the pipelines, the Docs tree, the Monitors roster) narrows to its fillers with no argument; a read several surfaces share (the entity list, the ticket board) takes the role as a parameter, which narrows its sources and, where no kinds are named, its kinds. Items of a removed source or of an uninstalled adapter are not fillers, so no role surface draws them; they stay live for search, the launcher and the detail. What stays with the shell: whether a source answers an optional read (`translate_query`) and whether a source claims CI for a staleness warning, both read from the source list in `app/src/lib/tf/roles.svelte.ts`, and the kind registry's run frame only to route a run's address. `app/src/lib/shell/house-rules.test.ts` fails when a page under the ticket-first shell filters rows by source. Deciding fillers in SQL from the role-check table alone was rejected: a source that never synced has no row, and an uninstalled adapter leaves rows behind.

## Rejected

- Keep ADR-0022's frames implicit and add per-source code where needed. That is how Tickets became Jira's.
- Let an adapter ship its own UI for a custom role in a sandboxed iframe. ADR-0020's reasons, plus a second sandbox beside the WASM one that ignores the app's theme.
- Only the role vocabulary for writes. Every external adapter would be limited to the writes of the release it was built against.

## Consequences

- Generalises ADR-0022: its frames stand as the CI and wiki roles, and roles also cover Code, Tickets and Monitors.
- `jira_views.rs` and the Jira source line on Tickets became the ticket system's saved views.
- Each role's minimum is a frozen surface. Changing it is a §10.8 entry and an amendment here.
- The contract suite has a per-role check, which the built-in adapters pass.
