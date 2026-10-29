---
status: accepted, amended by ADR-0021 and ADR-0023
---

# A spawned command takes no argument from the mirror

*Open in editor* and *open a terminal at the checkout* are the first features that run a program on the user's machine, and a repo entity is a mirrored item whose every field a remote system wrote. Decided 2026-09-07 (Björn, v1.5 grilling): **a program knobas spawns is built from a command template the user set in settings, and its arguments come only from what knobas found on the local disk or was told by hand, never from a mirrored item.**

- An argument is a [checkout](../../CONTEXT.md) under the clones root or a path the user typed. The Docker importer (ADR-0015) passes a context name a person typed, never a mirror value.
- The opener plugin stays scoped to `http` and `https`. No `vscode://`, `jetbrains://` or `file://`.
- Nothing knobas spawns writes to a working tree, with two amendments: [ADR-0021](0021-knobas-reads-a-checkouts-state-with-read-only-git.md) allows read-only `git status` at a checkout path, and [ADR-0023](0023-knobas-clones-a-repository-into-the-clones-root.md) allows `git clone` into a new folder under the clones root with a URL built from the source's configured address. Nothing else that writes is allowed.
- Command templates are a settings surface with macOS defaults; on another platform the buttons say *not configured* until a template is set.
- The clones-root scan and the per-repo override are knobas-owned data with their own migration and §10.8 entry. A repo entity gains no path column.

## Rejected

- Widen the opener scope. A URL scheme handler is exactly the argument a mirrored `web_url` could carry.
- The Tauri shell plugin with an allowlist. It constrains which binaries run, not where their arguments came from.
- Run `git checkout <branch>` when opening a branch. A checkout on somebody's dirty working tree is the one thing a cockpit must not do.

## Consequences

- The witness is the desktop itself: the signed dev bundle launched from its registered path, driven by desktop automation, the spawned editor or terminal observed as a process with the expected path. Unit tests cover template expansion and path resolution, where the argument rule is enforced.
- No human step, but three human preconditions (#500), each probed on every run and refused by the name System Settings uses, pointing at `testenv/README.md`, *The desktop witness (macOS)*: the Accessibility grant to the terminal running the driver (not Automation: an Apple Event prompt nobody answers is recorded as a denial, so the driver uses the AX API and `CGEvent`), an unlocked session, and no other copy of `dev.knobas.desktop` registered with Launch Services. A refusal is not a witness; the criterion stays open until an unlocked run happens, and no fake, dry run or hand checklist stands in (ADR-0013). The harness never moves an installed knobas aside; that is the runner's job.
