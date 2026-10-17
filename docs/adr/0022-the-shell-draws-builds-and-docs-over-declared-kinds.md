---
status: accepted, amended 2026-10-17 (#962)
---

# The shell draws Builds and Docs over declared kinds

Narrows [ADR-0020](0020-a-source-draws-the-views-of-its-own-kinds.md) for two kinds of source.

ADR-0020 lets an adapter draw the views of its own kinds as blocks, and the shell renders the blocks without knowing who drew them. That works for one entity's detail. The ticket-first redesign (`mockups/ticket-first/`, D7 and D9) adds two full views that are not one entity's detail: **Builds**, with its pipeline list, run history chart, runs table, steps bar, log viewer, tests and artifacts, and **Docs**, with its space picker, page tree, full-text search with snippets and the page reader. TeamCity and Confluence are the only sources behind them today. GitLab CI or Jenkins, and a Markdown wiki in place of Confluence, are the expected next ones. If each adapter drew these views as blocks, every new CI or docs source would redraw the same list, chart, log viewer and reader, and they would drift apart.

Decided 2026-10-15 (Björn, on D7 and D9 of the ticket-first requirements): **the shell owns the frame of the Builds view and of the Docs view, and an adapter fills it by declaring kinds and fields.**

- **Builds** is drawn over two kinds, [pipeline](../../CONTEXT.md#pipeline) and [run](../../CONTEXT.md#run). An adapter declares their fields as [declared paths](../../CONTEXT.md#declared-path) (ADR-0018): for a pipeline, its name, group, the repository it builds, its default branch, parameter specs and triggers; for a run, number, state with a tone map, branch, commit, who started it, start, duration, agent, steps and the test summary. It answers four reads (a range of log lines, tests, the artifact tree with a download stream, changes) and declares the writes it supports (`run_build`, `rerun_build`, `trigger_build`, `set_parameter` today). (*Amended 2026-10-17, #962, after #851 to #857 and #859:* declared paths over the mirror are the one mechanism only for what the mirror holds. The repository, default branch, triggers, parameter specs, steps and chain of a pipeline are not in any mirrored record: they live in the pipeline's configuration, whose settings carry credentials. So the frame is filled a second way, by the frame's **live reads**, each a `Source` method that answers a frame shape the shell draws without knowing the source, each defaulting to `Ok(None)` ("this adapter keeps no pipelines or runs"), each read when the page opens and never stored, and each a §10.8 entry: `pipeline_settings`, answered as `PipelineSettings` (#851, extended by #852, #857 and #1016); `older_runs`, the history past the window the sync keeps, each run the `SyncItem` the sync would have made, so the shell reads it through the run kind's declared paths (#851); and the run page's reads, `run_log` (a range of lines and the steps, #854), `run_tests` with `test_history` (#855), `run_artifacts` with `artifact_preview` and the `download` stream (#856), and `run_chain` (#857). A run's changes are not a `Source` method of their own: they are the Changes section of the run's source view (ADR-0020), read when that tab opens. The adapter scrubs `pipeline_settings` of credentials before it answers (a URL without its credential, a password parameter without its value). Docs does the same for a page body: `DocContent` (#859) is a projection of the mirrored record, not a declared path. Whatever a live read answers is a frozen surface like a declaration: a new field is a §10.8 entry.)
- **Docs** is drawn over spaces, a page tree (the parent as a declared path), a page body as the reader's block list, comments, versions and labels, and the writes `create_page`, `update_page` by section, and `comment`.
- The source's own word for a pipeline or a run appears once, in the view's header ("TeamCity build configuration"). Everywhere else the view says *pipeline* and *run*.
- Anything only one source has stays a block inside the frame, under ADR-0020's rules: a TeamCity build chain, a Confluence macro the reader cannot draw.

## Considered options

- **Each adapter draws Builds and Docs as blocks (ADR-0020 as it stands).** Rejected: the list, chart, log viewer and reader are the bulk of these views, so every new source would redraw them.
- **The shell draws TeamCity and Confluence directly.** Rejected: it reads payload shapes outside the adapter (ADR-0007), and swapping Confluence for a Markdown wiki would mean a second Docs view.

## Consequences

- *Pipeline* and *run* join `CONTEXT.md`. A TeamCity build configuration is a pipeline and a TeamCity build is a run.
- TeamCity declares most of these fields already. The parts it is missing are tagged LOW in requirements R-9.*: the whole log with step boundaries, the artifact folders, the parameter specs, and the nested project path.
- A second CI or docs adapter has to fill the declaration and is certified against a real instance (ADR-0013). Until one exists, the frame is witnessed only through TeamCity and Confluence. That is the risk this decision accepts.
- The declarations are a frozen surface. Adding a field is a §10.8 entry, like a block type in ADR-0020.
