---
status: accepted
---

# Writes are at-least-once, and `Source::write` takes no idempotency key

No transaction can span `source.write()` and `store::sent()`, because the first is network I/O and §10.6(c) keeps transactions off network I/O. So the write queue is at-least-once by construction: if knobas dies between the send returning and `sent` landing, the row stays pending and the next flush re-sends a write that already arrived. Decided 2026-08-31 (Björn, closing #122): **the at-least-once property is stated, not papered over, and `Source::write(op: WriteOp)` keeps its signature.** None of the sources offers server-side idempotency on the endpoints the queue uses, so a key parameter would be accepted and ignored by every adapter.

- Duplicates are real for creates (a second branch or PR gets a 409, a Jira `create_ticket` files twice) and also for `transition` and `approve` (corrected 2026-10-17, #1125: Jira takes a repeated transition on a workflow that loops, Gitea records a second review).
- The canonical sentence, quoted verbatim by any user-facing surface (the write queue view, the worklog draft):

> A write knobas was sending when it stopped may arrive twice. knobas re-sends rather than guess; it never merges or drops what you wrote.

- A send that got no answer is outside this window (#1125). A once-sent write that timed out or got a 502 or 504 comes back as `SourceError::Unconfirmed`. If a second arrival would duplicate it, the queue holds it for the reader instead of re-sending, per `WriteOp::delivery`'s `resend_harmless`, read by `knobas_sync::write_queue`'s `attempt`.
- What cannot be prevented is disclosed. A `create_ticket` or `create_page` withdrawn while in flight can stand at the source with nothing in knobas claiming it. `knobas_sync::write_queue::unclaimed` writes one activity line where the fact exists. It is a disclosure, not a repair, since no key rides on the call.

## Rejected

- An idempotency key on `Source::write` now. Deferred, not rejected. The recorded starting shape for the retrofit: the key is `write_queue.id`, `SourceDescriptor` declares per op whether an adapter honours it, and an adapter that cannot says so. Today it buys nothing behavioural.
- Queue-side dedupe by content hash. A hash cannot tell a crash-retry from a person posting the same words twice, so it swallows real writes.
- A transaction spanning send and settle. §10.6(c).

## Consequences

- The reconsideration trigger is an adapter whose source offers server-side idempotency on a write endpoint the queue uses. Field reports of duplicates do not reopen this: with no honouring source, a key could not have prevented them.
- No §10.8 entry: the frozen surface is untouched.
- The flush loop's `source.write` call site cites this ADR. `CONTEXT.md`'s **Pending write** entry carries the guarantee.
