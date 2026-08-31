---
status: accepted
---

# Writes are at-least-once, and `Source::write` takes no idempotency key

No transaction can span `source.write()` and `store::sent()`: the first is network I/O, and holding a database transaction across network I/O is the exact shape §10.6(c) moved the sync engine away from. So the write queue is at-least-once by construction — if knobas dies between the send returning and `sent` landing, the row stays pending and the next flush re-sends a write that already arrived. Since #163 the consequence includes creates: Gitea refuses a duplicate branch or an already-open PR with a 409, `transition` and `approve` are naturally idempotent, and a duplicated Jira `create_ticket` files the ticket twice.

Decided 2026-08-31 (Björn, closing #122): **the documentation stance is the ruling.** `Source::write(op: WriteOp)` keeps its signature; the at-least-once property is stated rather than papered over. The reason is honesty in the SPI: none of the three M2 sources offers server-side idempotency on the endpoints the queue uses (Jira DC comment/create, Gitea issue comments, TeamCity's trigger), so a key parameter today would be accepted and ignored by every implementor — a parameter that looks like a guarantee no adapter keeps. And with zero honouring adapters, the user-facing statement needs no per-op metadata: it is simply true of every op that is not naturally idempotent.

The canonical sentence, quotable verbatim by any user-facing surface (issue #224 puts it in the write queue view):

> A write knobas was sending when it stopped may arrive twice. knobas re-sends rather than guess; it never merges or drops what you wrote.

## Considered options

- **An idempotency key on `Source::write` now**, per the #122 implementer's design: the key is `write_queue.id` (stable, durable, never reused, free for the queue to supply), `SourceDescriptor` declares per-op whether an adapter honours it, and an adapter that cannot says so rather than pretending. Deferred, not rejected — this is the recorded starting shape for the retrofit when it is paid for (see consequences). What it buys today is nothing behavioural, at the price of a §10.8 ratified exception spent on a hypothesis.
- **Queue-side dedupe by content hash.** Rejected outright: a hash cannot distinguish a crash-retry from a person deliberately posting the same words twice, so it silently swallows a real write — the same failure class this would exist to fix, arriving through the fix.
- **A transaction spanning send and settle.** Rejected: §10.6(c). The gap is structural; the only system that can actually suppress the duplicate is the one that already has the row, and none of them offers to.

## Consequences

- **The reconsideration trigger is an adapter, not a report.** The day an adapter lands whose source offers server-side idempotency on a write endpoint the queue uses, the §10.8 exception is paid for that adapter, starting from the design recorded above. Field reports of post-crash duplicates do not reopen this: with no honouring source, a key could not have prevented them.
- No §10.8 entry now — the frozen surface is untouched, which is the point.
- The flush loop's `source.write` call site signposts this ADR, so the next reader of that seam finds the ruling where the gap lives.
- `CONTEXT.md`'s **Pending write** entry carries the guarantee, because a person can observe it: the duplicate comment is user-visible behaviour, not an implementation detail.
