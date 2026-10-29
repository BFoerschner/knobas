---
status: accepted
---

# A payload read outside an adapter misses, sits in one named statement, and pins its failure direction

Contract §4.1 normalises four fields, `title`, `body_text`, `author` and `updated_at`, plus the verbatim `payload`; everything else a source knows lives only in the payload, in that source's shape. Four features answered "may code outside an adapter read those shapes?" differently in one day. Decided 2026-08-29 (Björn): **there are two rules, not one contradiction, and both are ratified as they landed.**

**The write direction: refused.** `write_queue::project` never reads payload shapes, because it feeds hold and overwrite decisions. A read that misses there is a missed hold, silently moving a ticket somebody else moved. An adapter-independent alternative (the whole verbatim record) exists and is used.

**The read direction: permitted under three requirements.** Outside an adapter, a structural JSON-path read into an item's `payload`, or into any knobas-owned serde shape addressed from SQL, is allowed only when all three hold:

1. **It misses, never guesses.** An absent or unrecognised shape contributes nothing. A source shaped differently produces no derived item rather than a wrong one. This confines such reads to derivations whose tolerable failure is an absent result, never to a decision where a miss becomes a wrong action.
2. **It is confined to one named statement**, so a second source's spelling is one more `coalesce` or `or` in one place.
3. **Its failure direction is stated and pinned by a test.** Which way a read fails is a per-read fact. `start_work::merge`'s `not exists` over `WriteOp`'s serde shape fails open and is held safe only by its pin. A read of a knobas-owned shape is bound by 2 and 3 too.

The destination is descriptor-declared paths: the adapter tells knobas where its status or merged flag lives, as a §10.8-ratified growth of the frozen descriptor by ADR-0006's mechanism. Each read this ADR governs expires into the declared field as it arrives; the three requirements are the interim discipline.

## Rejected

- One contextual rule spanning both directions ("core may read payload shapes only where absence is the failure mode"). Silent about reads of knobas-owned shapes, whose worst instance fails toward action.
- Bind `knobas-core` only. `knobas-app`'s start-work holds the riskiest read of the set.
- Ban payload reads outside adapters entirely. §4.1 normalises only four fields, so the inbox and suggestions would have nothing to read until the descriptor work landed.
- Design the descriptor field now. Binds later design today for no landed read that was blocked on it.

## Consequences

- A new read arrives with all three requirements or it does not land. "Which way does this fail" is a review question with a required answer.
- The sites this governs reference this ADR; `write_queue::project`'s comment states the write-direction rule.
- `CONTEXT.md` carries the vocabulary: payload, payload read, miss.
