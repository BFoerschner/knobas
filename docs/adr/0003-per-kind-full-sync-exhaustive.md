---
status: accepted
---

# `full_sync_exhaustive` is declared per kind, and only exhaustive kinds are swept

The sweep tombstones mirrored items a full sync no longer emitted, which is only safe when the run emitted the whole corpus, and that is a property of a kind, not a source: Gitea's repo and branch walks are exhaustive, its commit and pr walks are budgeted. Decided 2026-08-27 (Björn, on the PR #26 reviewer's recommendation): **the exhaustive declaration is per kind, and the sweep fires only for kinds declared exhaustive.**

- A budgeted kind is non-exhaustive by definition. Declaring it exhaustive authorises the sweep to tombstone everything past the cap on every full sync.
- `Sink` stays write-only. The sweep is the engine's, not a reconcile the adapter runs.

## Rejected

- A reconcile call on the `Sink` SPI. Adds read surface to a write-only SPI and obliges every adapter to diff.
- The per-source boolean. Leaves a vanished repository's `repo` row alive and a branch hard-delete window open.

## Consequences

- Hard deletes in non-exhaustive kinds (budgeted commits and prs, TeamCity builds) stay inexpressible. That is a documented limitation of the sync run.
- SPI and sweep are frozen surfaces: a change here carries a §10.8 entry.
