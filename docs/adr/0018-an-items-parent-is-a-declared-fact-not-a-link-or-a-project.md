---
status: accepted
---

# An item's parent is a declared fact, neither a link nor a project

Searching for a Gitea repo returned its own commits and branches first, because every commit and branch carried the repo's name in its searchable text. That was the only way to say "in this repo". Every source has the same shape: a commit, branch or PR sits in a repo, a build in a build configuration, a page under a page, a ticket under an epic, an asset under an asset. Decided 2026-10-11 (Björn, git-usability grilling): **this is one concept, the [parent](../../CONTEXT.md#parent).** Each adapter declares it per kind as a declared path, the way it declares status or project. Search scopes by it with one transitive `under:<name>` filter. A child's searchable text no longer contains its parent's name; the launcher row's path line shows the parent instead. In the fixed group order, a parent kind always comes before its child kinds.

## Considered options

- **A repo as Gitea's project.** Rejected: ADR-0010's project is a grouping you cannot open, and a repo is an entity you can. Calling it a project would make the one dimension mean two things.
- **Parent as a link (`contains`, `in-epic`).** Rejected for the reasons ADR-0014 gives for assets. Links are knobas-owned and drawn; this fact belongs to the source and is never drawn. Scoping is transitive, and ADR-0008's membership walk is one hop over links.
- **Per-source filter keys (`repo:`, `config:`, `epic:`).** Rejected: five spellings for one meaning. Value suggestions in the box make the one generic key discoverable.
- **Fix ranking only (boost title matches).** Kept as well, but on its own it does not fix the problem: a commit whose text names its repo is still a match for that repo's name, however low it ranks.

## Consequences

- `under:` resolves against the parent each child **declares**, not against mirrored parent entities. So an epic outside the synced JQL, or an ancestor in an unsynced space, still scopes its children.
- A parent's name may stay in a child's **title** where the title means nothing without it (a TeamCity build, `Payments / Deploy #42`). The rule binds searchable text, not titles.
- A Gitea commit's raw payload does not name its repository, so the adapter has to store it for a declared path to read. A new declared-path field is a descriptor change and needs a §10.8 entry.
