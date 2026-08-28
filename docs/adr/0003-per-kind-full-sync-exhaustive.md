---
status: accepted
---

# `full_sync_exhaustive` is declared per kind, and only exhaustive kinds are swept

The sync engine's sweep tombstones mirrored items that a full (cursor-less) sync no longer emitted. Whether sweeping is safe depends on whether the run truly emitted the complete corpus — and that is a property of each *kind*, not of a source: Gitea's repo and branch walks are exhaustive while its commit and pr walks are budgeted. Decided 2026-08-27 (Björn, on the PR #26 reviewer's recommendation, rounds 2–3): the exhaustive declaration moves from per-source to per-kind, and the sweep fires only for kinds declared exhaustive. A budgeted kind is non-exhaustive *by definition* — declaring otherwise authorises the sweep to tombstone everything past the cap on every full sync (the same defect class as Jira's `MAX_PAGES` and the Gitea budget ruling of 2026-08-25).

## Considered options

- **A reconcile call on the `Sink` SPI** (the adapter asks the sink what it holds and diffs) — closes every hard-delete hole in principle, but adds read surface to a deliberately write-only SPI and obliges every adapter to implement diffing. Rejected: `Sink` staying write-only is the verified reason the earlier tombstone deferral was sound.
- **Keep the per-source boolean** — leaves two real holes open: a repository that vanishes from the listing never retires its `repo` row, and branch hard-deletes have a recorded open window.

## Consequences

- Closes repo-row retirement and the branch hard-delete window for Gitea's exhaustive kinds.
- Hard deletes in non-exhaustive kinds (budgeted commits/prs, TeamCity builds) remain inexpressible; this stays a documented limitation on the sync run.
- Frozen-contract change (SPI + engine sweep): xhigh review tier; lands **before** Gitea tasks 6–7 introduce the budgeted kinds it protects.
