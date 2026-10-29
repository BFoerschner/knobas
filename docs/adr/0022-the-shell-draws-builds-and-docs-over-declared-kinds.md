---
status: accepted, amended 2026-10-17 (#962), generalised by ADR-0029
---

# The shell draws Builds and Docs over declared kinds

Narrows [ADR-0020](0020-a-source-draws-the-views-of-its-own-kinds.md) for two kinds of source.

Builds and Docs are full views, not one entity's detail, and every CI or wiki source would redraw the same list, chart, log viewer and reader if each adapter drew them as blocks. Decided 2026-10-15 (Björn, on D7 and D9 of the ticket-first requirements): **the shell owns the frame of the Builds view and of the Docs view, and an adapter fills it by declaring kinds and fields.**

- **Builds** is drawn over two kinds, [pipeline](../../CONTEXT.md#pipeline) and [run](../../CONTEXT.md#run). An adapter declares their fields as [declared paths](../../CONTEXT.md#declared-path) (ADR-0018): for a run, number, state with a tone map, branch, commit, who started it, start, duration, agent, steps and the test summary. It declares the writes it supports (`run_build`, `rerun_build`, `trigger_build`, `set_parameter` today).
- **Declared paths cover only what the mirror holds** (amended 2026-10-17, #962). A pipeline's repository, default branch, triggers, parameter specs, steps and chain live in its configuration, not in any mirrored record. The frame reads them through **live reads**: `Source` methods that answer a frame shape the shell draws without knowing the source, each defaulting to `Ok(None)`, each read when the page opens and never stored, and each a §10.8 entry. They are `pipeline_settings` answering `PipelineSettings` (#851, #852, #857, #1016), `older_runs` answering runs past the window the sync keeps as the `SyncItem` the sync would have made (#851), `run_log` (#854), `run_tests` with `test_history` (#855), `run_artifacts` with `artifact_preview` and the `download` stream (#856), and `run_chain` (#857). A run's changes are the Changes section of its source view (ADR-0020). The adapter scrubs `pipeline_settings` of credentials before it answers.
- **Docs** is drawn over spaces, a page tree with the parent as a declared path, a page body as the reader's block list (`DocContent`, #859, a projection of the mirrored record), comments, versions and labels, and the writes `create_page`, `update_page` by section, and `comment`.
- The source's own word for a pipeline or a run appears once, in the view's header. Everywhere else the view says *pipeline* and *run*.
- Anything only one source has stays a block inside the frame under ADR-0020's rules.

## Rejected

- Each adapter draws Builds and Docs as blocks. Every new source would redraw the list, chart, log viewer and reader.
- The shell draws TeamCity and Confluence directly. It reads payload shapes outside the adapter (ADR-0007), and a second wiki would mean a second Docs view.

## Consequences

- *Pipeline* and *run* are in `CONTEXT.md`. A TeamCity build configuration is a pipeline and a TeamCity build is a run.
- The declarations and whatever a live read answers are frozen surfaces. A new field is a §10.8 entry.
- A second CI or docs adapter fills the declaration and is certified against a real instance (ADR-0013). Until one exists the frame is witnessed only through TeamCity and Confluence.
