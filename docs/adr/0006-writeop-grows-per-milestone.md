---
status: accepted
---

# `WriteOp` grows per milestone, and each growth is a ratified exception

The write direction is not new: `Source::write(op: WriteOp)` has been on the SPI since the M1 contract, `SourceDescriptor::write_ops` declares which ops an adapter accepts, and the contract battery already refuses a descriptor naming an op the SPI does not define. What the SPI carries today is one variant, `Comment`, and a doc comment saying the enum grows per milestone. M2's ratified write-back scope — Jira status/comment/create, Gitea branch/PR/comment/approve, TeamCity trigger/re-run — is the first growth.

Decided 2026-08-29 (Björn): the growth mechanism stands as designed, and **each milestone's growth is recorded as a §10.8 ratified exception rather than treated as pre-authorised by the doc comment.** `crates/knobas-source/src/**` is frozen; "the enum grows per milestone" describes the intended shape of the change, not standing permission to make it. One entry per milestone's set, naming the variants and the adapters that declare them.

The forcing function that makes this safe is already in place and must not be softened: `WriteOp::identifier()` has **no wildcard arm**, deliberately, so adding a variant stops that module compiling until the variant is given its stable snake_case identifier, and the battery's `known_write_ops` stops compiling until it is given a probe value. A stale identifier table does not fail quietly — it falsely rejects the first adapter to declare the new op, with a message pointing at that adapter's descriptor instead of at the table. The compiler is the reminder.

## Considered options

- **Treat the doc comment as standing authorisation** and grow the enum without a §10.8 entry. Rejected: it is the one frozen surface every adapter and every future adapter inherits, and "the document said it would grow" is not a record of *what* it grew to or *why*. The entry costs one paragraph per milestone and is the only place a reader learns which ops exist and who accepts them.
- **A separate `Writer` trait alongside `Source`**, so writes evolve without touching the frozen trait. Rejected: it splits one adapter across two traits for no gain — `write` is already on `Source`, one implementation per configured instance, and a second trait would need its own registry, its own descriptor coupling, and its own battery.
- **An open/string-typed op** (`write(kind: &str, payload: Value)`), which never needs the enum to grow. Rejected: it moves every mistake from compile time to run time and deletes the forcing function above. An adapter would fail on an op it mis-spelled, in production, instead of failing to build.
- **One variant per adapter** (`JiraTransition`, `GiteaApprove`, …). Rejected: it makes the SPI adapter-aware, which is the coupling the descriptor exists to avoid; the same op from two sources should be one variant.

## Consequences

- Adding a write op is a deliberate, reviewed act with a paper trail, not a drive-by enum edit.
- Adapters that do not accept a new op need no change: an adapter must reject every op it does not declare with `SourceError::Protocol`, and the battery holds it to its own descriptor.
- The M2 growth lands as one package with the adapters that declare it, so no variant exists that nothing accepts and no descriptor names an op the SPI lacks — the battery would fail either way, which is the point.
- Nothing here authorises a *second* write path. Every outbound write, whatever its op, goes through the write queue (issue #42): the queue is what decides send, pend or hold, and `Source::write` is the choke point it wraps.
