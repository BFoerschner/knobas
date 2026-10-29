---
status: accepted
---

# An item's parent is a declared fact, neither a link nor a project

Searching for a Gitea repo returned its commits and branches first, because every child carried the repo's name in its searchable text, and that was the only way to say "in this repo". Every source has the shape: a commit, branch or PR sits in a repo, a build in a build configuration, a page under a page, a ticket under an epic, an asset under an asset. Decided 2026-10-11 (Björn, git-usability grilling): **this is one concept, the [parent](../../CONTEXT.md#parent), declared per kind by each adapter as a declared path, the way it declares status or project.**

- Search scopes by it with one transitive `under:<name>` filter, resolved against the parent each child declares, not against mirrored parent entities. An epic outside the synced JQL still scopes its children.
- A child's searchable text no longer contains its parent's name; the launcher row's path line shows the parent instead. A parent's name may stay in a child's title where the title means nothing without it (`Payments / Deploy #42`).
- In the fixed group order, a parent kind always comes before its child kinds.
- A Gitea commit's raw payload does not name its repository, so the adapter stores it for a declared path to read. A classic Data Center Jira keeps epic membership in a per-instance custom field, so when `epic_link_field` is configured the adapter stores `epic: {key}` beside the record (#598). Both are departures from §3a's verbatim payload. A new declared-path field is a descriptor change and needs a §10.8 entry.

## Rejected

- A repo as Gitea's project. ADR-0010's project is a grouping you cannot open; a repo is an entity you can.
- Parent as a link (`contains`, `in-epic`). ADR-0014's reasons: this fact belongs to the source and is never drawn, and scoping is transitive where ADR-0008's walk is one hop.
- Per-source filter keys (`repo:`, `config:`, `epic:`). Five spellings for one meaning.
- Fix ranking only. Kept as well, but a commit whose text names its repo is still a match for that name.
