---
status: accepted
---

# `WriteOp` grows per milestone, and each growth is a ratified exception

`Source::write(op: WriteOp)` has been on the SPI since M1, `SourceDescriptor::write_ops` declares which ops an adapter accepts, and the enum's doc comment says it grows per milestone. Decided 2026-08-29 (Björn): **the growth mechanism stands, and each milestone's growth is recorded as a §10.8 ratified exception, never treated as pre-authorised by the doc comment.** `crates/knobas-source/src/**` is frozen; one entry per milestone's set, naming the variants and the adapters that declare them.

- `WriteOp::identifier()` has no wildcard arm, deliberately. A new variant does not compile until it has its stable snake_case identifier, and the battery's `known_write_ops` does not compile until it has a probe value. The compiler is the reminder.
- An adapter rejects every op it does not declare with `SourceError::Protocol`; the battery holds it to its own descriptor. Adapters that do not accept a new op need no change.
- A growth lands as one package with the adapters that declare it, so no variant exists that nothing accepts.
- Nothing here authorises a second write path. Every outbound write goes through the write queue; `Source::write` is the choke point it wraps.
- (Amended 2026-10-19, Björn) **A variant states its delivery in one place, beside its identifier.** How the queue treats an op (which projection of its target a hold compares, whether a resend is harmless, whether a withdrawal can leave an unclaimed artefact, which entity it targets, which link an arrival draws) is one value the op answers, with the same no-wildcard forcing function as `identifier()`. The type lives in `knobas-spi-types` and is re-exported as `knobas_core::delivery`, because `knobas-source` depends on `knobas-spi-types` and no other knobas crate (#1234, corrected by #1337). The WIT carries an op as JSON, so the Rust-only method touches neither the wire form nor the WIT. An adapter's own reading of a repeat (a 409 that is this write's own landing) stays the adapter's. A source action stays maximally conservative on every count. ADR-0012 is untouched: delivery is stated per op, never keyed per write.

## Rejected

- The doc comment as standing authorisation. No record of what the enum grew to or why.
- A separate `Writer` trait beside `Source`. Splits one adapter across two traits, with its own registry, descriptor coupling and battery.
- An open string-typed op. Moves every mistake from compile time to run time and deletes the forcing function.
- One variant per adapter (`JiraTransition`, `GiteaApprove`). Makes the SPI adapter-aware; the same op from two sources should be one variant.

## Consequences

- Adding a write op is a reviewed act with a §10.8 entry, not a drive-by enum edit.
- ADR-0007 planned the first `SourceDescriptor` growth by the same route.
