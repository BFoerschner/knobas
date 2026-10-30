---
status: accepted, narrowed by ADR-0022
---

# A source draws the views of its own kinds

The detail panel drew every kind as a header plus every raw payload field (spec §3a's generic view): forty rows of JSON for a pull request and no diff, a name and an id for a build configuration and none of its parameters. Decided 2026-10-12 (orchestrator, under Björn's delegation): **an adapter draws the views of its own kinds.** It answers a live read, `Source::view(entity, section)`, with a source view: a list of sections, each a list of typed blocks from one fixed vocabulary (facts, text, entity list, diff, log, settings, actions). The shell renders the vocabulary and never learns which adapter drew a block. The generic payload projection stays as the fallback.

## Why a block vocabulary and not one Svelte view per adapter

- §3a promises that a new source needs no change to the shell. A GitLab adapter that answers with a diff block gets the diff viewer on day one.
- ADR-0007 forbids a payload read outside the adapter that shaped it except as a named, missable read. Per-adapter views in the shell would be a payload read on every line.
- The SPI stays plain serde data, so an out-of-process adapter can draw views too.

## The rules

1. A view is live, like a [reachable transition](../../CONTEXT.md#reachable-transition): read when the detail opens, never stored. The mirror's own fields still come from `get_entity`. The shell keeps the last twenty entities it drew or read ahead, row and view, in memory for the session, re-reads them behind what it drew and replaces each only if its answer changed; it also remembers per session each source and kind whose view read answered `None` (amended 2026-10-13, #693). Nothing is written to the mirror or disk. The app process keeps a view its adapter says stands at a fixed commit in memory for the session, and answers the same ask from there (amended 2026-10-30, ADR-0032).
2. A failed read falls back and says so: the generic projection under a line naming the failure (`Showing the mirror: Gitea answered 502`). An adapter with no view for a kind answers `None` and the panel draws the generic projection with no line.
3. A section can be deferred. The first read returns every section's title and count; a deferred section's blocks are read when its tab opens, through the same call with the section's id. A block may name a section that opens in the tab it is drawn in, such as a tree entry naming `files:src/lib`; the shell reads it through the same call and draws it in place under the same tab, keeps each tab's place and steps, walks them with *Back*, and never parses the ids or puts a step in the app's history (amended 2026-10-13, #711).
4. An edit is a write op, and the block names it. A settings block marks the values a reader can change, each naming the `WriteOp` identifier. Every edit goes through the write queue; a view never writes. An action is the same rule for a write with no value to edit (#667): the source declares each action's dialog (fields with defaults and required flags, the sentence it asks the reader to confirm, primary or dangerous), and the shell draws any dialog without knowing the op, keeping one table from op to payload (#702). A view's actions are a toolbar, `SourceView.actions`, drawn above the tabs; the actions block stays for an action that belongs to one section, and nothing draws one twice. Whether an action is offered is the adapter's answer, never the shell's reading of a fact, and the shell draws one only where the descriptor declares its op. Disabled or absent (#705): an action is drawn disabled with its reason only where the entity can still reach a state where it is enabled (a conflict, a branch behind its base, approvals missing, a write in flight); one that can never apply to this entity is absent (*Merge…* on a merged pull request, the two verdicts on the reader's own pull request, *Delete* on the default branch). An action may carry fixed values the reader is not asked for, which is how one op is two actions such as *Approve…* and *Request changes…* on `submit_review` (#705). An action targets the entity the view is of unless it names another, as a branch's *New branch…* names its repository; the shell never derives one id from another (#707).
5. Every string in a block is source text. The shell interpolates it and never renders it as markup. A text block holding Markdown is read into a tree (headings, lists, code, quotes, links) and each part drawn with an element the shell names, as `StorageBody` draws a Confluence page; every string inside stays interpolated, and a link opens only through the shell and only for `http`/`https` (amended 2026-10-12, #663).
6. Size is capped in the adapter: a fixed number of lines per file and per view. A file over the cap carries its counts and `truncated: true`, and the panel offers the source's own page for the rest.
7. An entity in a block is an address. An entity-list row naming a mirrored entity carries its id and opens its detail; a row the mirror does not hold carries no id and is plain text.

## Rejected

- One Svelte view per adapter and kind. The three reasons above.
- Sync the extra data into the mirror. Large, always changing, searched by nothing, and still stale when opened.
- A webview of the source's own page. The tab knobas exists to close, and it takes the source's login rather than knobas' credential.

## Consequences

- `Source` grows `view` with no default implementation (ADR-0006's forcing function). An adapter with nothing to draw answers `Ok(None)` for every kind.
- The block vocabulary is a frozen surface. Adding a block type is a §10.8 entry, like a `WriteOp` variant.
- One IPC command, `entity_view`, in `commands/entity.rs`, with its §10.8 entry.
- The unified-diff parser is shared in `knobas-source`, so every git adapter emits the same diff block.
- Each adapter's views are witnessed against the real product in its live suite (ADR-0013).
