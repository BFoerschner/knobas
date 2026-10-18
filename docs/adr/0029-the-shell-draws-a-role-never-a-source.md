---
status: accepted
---

# The shell draws a role, never a source

ADR-0022 gave Builds and Docs shell-owned frames that an adapter fills by declaration, but only for those two, and only by implication. Tickets stayed Jira-shaped: JQL, `jira_views.rs`, and lanes named after Jira's fields. Adding GitHub, a second ticket system or another wiki meant finding out case by case which screens would light up. Björn, 2026-10-18, wanted *"an interface that has like necessary requirements the source has to meet"*.

Decided 2026-10-18 (Björn, custom-sources grilling): **every surface the shell draws beyond one entity's detail belongs to a [role](../../CONTEXT.md#role), and the shell draws it for every source that fills that role, never for a named source.**

- **The built-in roles and their minimums.** Each role requires its read side: kinds, [declared paths](../../CONTEXT.md#declared-path) and live reads. Each write is optional and declared on its own. The shell draws an undeclared write disabled or absent, under ADR-0020's rule.
  - **Code host.** Requires `repo`, `branch`, `pr` and `commit`; parent, PR state chip, head and base, the commit a record stands at, requested reviewers, merged flag and default branch; clone URLs; PR, commit and file diffs, the repository tree and file contents, and line threads. Its optional writes are `create_branch`, `create_pull_request`, `comment`, `submit_review`, `merge`, `delete_branch` and `commit_file`.
  - **Ticket system.** Requires `ticket` with key, status, priority, assignee, type, labels, parent and project, and `reachable_transitions`. Its optional writes are `transition`, `comment`, `create_ticket`, `edit` and `worklog`. **Saved views** the source offers are optional too, such as Jira's boards and filters or GitHub Projects. Each is synced as an item and becomes a ticket view. A source's own query language stays the source's: `translate_query` is an optional read, and JQL is Jira's answer to it.
  - **CI.** ADR-0022's Builds frame: `pipeline` and `run`, and `run_log`, required. The other live reads and the build writes are optional.
  - **Wiki.** ADR-0022's Docs frame: spaces, a page with its parent, and `DocContent`. The page writes are optional.
  - **Monitoring.** `monitor` with a state and a response time, from which knobas takes its samples.
- **A source can fill several roles,** and it fills a role only where the checks of ADR-0028 pass.
- **A source whose items fit no built-in role declares a custom role,** namespaced to the adapter that declares it (`grafana:dashboard-host`). No other adapter fills it. Its view is drawn from ADR-0020's blocks, never from UI the adapter ships, and it opens when one of the role's items is opened. A custom role has no surface to open by itself yet. A custom role that two adapters need becomes a built-in role in a knobas release.
- **A write outside every role's vocabulary is a [source action](../../CONTEXT.md#source-action),** one `WriteOp` variant (`source_action { id, fields }`). The adapter declares its dialog, as ADR-0020's actions do. The write queue holds and refuses it like any write, and the shell never reads it. The ops of a role stay typed, because the shell acts on them itself: *Start work* creates a branch.

## Considered options

- **Keep ADR-0022's frames implicit and add per-source code where needed.** Rejected. That is how Tickets became Jira's, and every new system would ask again which screens it gets.
- **Let an adapter ship its own UI for a custom role, in a sandboxed iframe.** Rejected for ADR-0020's reasons. It would also be a second sandbox beside the WASM one, and it would not follow the app's look or theme.
- **Only the role vocabulary for writes.** Rejected. Every external adapter would be limited to the writes of the knobas release it was built against.

## Consequences

- Supersedes ADR-0022's per-source wording, which said TeamCity and Confluence are behind Builds and Docs. The frames themselves stand as the CI and wiki roles.
- `jira_views.rs` and the Jira source line on Tickets become the ticket system's saved views. Gitea's issues reach the Tickets screen once the Gitea adapter syncs them as tickets, which it does not today.
- Each role's minimum is a frozen surface. Changing it is a §10.8 entry.
- The contract suite grows a per-role check, which the built-in adapters pass before the WASM host exists.
