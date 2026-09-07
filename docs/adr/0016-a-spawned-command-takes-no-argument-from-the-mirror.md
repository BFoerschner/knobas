---
status: accepted
---

# A spawned command takes no argument from the mirror

*Open in editor* and *open a terminal at the checkout* (spec §5, §11; v1.5) are the first features that run a program on the user's machine. The obvious way to open VS Code is a `vscode://` URL through the opener plugin knobas already ships, and the obvious source for a path is the repo entity. Both are wrong for the same reason: the opener's capability is scoped to `http` and `https` on purpose — its own description says a `file://` or `vscode://` reaching the OS opener "would be a remote-code path that starts in somebody's backlog" — and a repo entity is a mirrored item, so every field on it was written by a remote system.

Decided 2026-09-07 (Björn, v1.5 grilling session): **a program knobas spawns is built from a command template the user set in settings, and its arguments come only from what knobas found on the local disk or was told by hand — a [checkout](../../CONTEXT.md) under the clones root, or a path the user typed — never from a mirrored item.** The opener stays scoped to web URLs. The same rule covers the Docker importer (ADR-0015): the context name it passes to the docker CLI is an asset property a person typed, not a value the mirror holds. Nothing knobas spawns writes to a working tree: no clone, no checkout, no fetch; a repo with no checkout shows the clone command to copy.

## Considered options

- **Widen the opener scope to `vscode://`, `jetbrains://`, `file://`.** Rejected: the scope is the second wall behind `openExternal`'s scheme check, and a URL scheme handler is exactly the kind of argument a mirrored `web_url` could carry.
- **The Tauri shell plugin with a scoped allowlist.** Rejected: it constrains which binaries run, not where their arguments came from, which is the property that matters here; and command templates are what make Linux and Windows configurable without a witnessed platform.
- **Run `git checkout <branch>` when opening a branch.** Rejected: a checkout on somebody's dirty working tree is the one thing a cockpit must not do to it.

## Consequences

- The clones-root scan and the per-repo override are knobas-owned data with their own migration and §10.8 entry; a repo entity itself gains no path column.
- Command templates are a settings surface, with macOS defaults; on another platform the buttons exist and say *not configured* until the template is set.
- There is no instance to run a live suite against, so the witness is the desktop itself: the signed dev bundle launched from its registered path and driven by desktop automation, the spawned editor or terminal observed as a process with the expected path, the run's output in the PR body and re-run by the merge-manager. No human step — Björn ruled the same day that nobody will be there once the implementation runs. The unit tests cover the template expansion and the path resolution, which is where the argument rule is enforced.
