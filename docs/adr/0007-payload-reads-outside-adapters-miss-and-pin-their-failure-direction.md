---
status: accepted
---

# A payload read outside an adapter misses, sits in one named statement, and pins its failure direction

Contract §4.1 normalizes exactly four fields — `title`, `body_text`, `author`, `updated_at` — plus the verbatim `payload`; everything else a source knows (a ticket's status, a pull request's merged-ness or requested reviewers, a build's outcome, an assignee) lives only in the payload, in that source's own shape. Four features answered "may code outside an adapter read those shapes?" in one day without a rule: `write_queue::project` (#42/#43) refused, `suggest::RULES` (#41) read, the inbox rules (#45) read citing it as precedent, and start-work (#44) read twice while calling a descriptor-declared path the real answer. Two independent reviewers concluded this needed ratification rather than a fifth precedent (#167).

Decided 2026-08-29 (Björn): **there are two rules here, not one contradiction, and both are ratified as they landed.**

**The write direction stands, unviolated.** `write_queue::project` refuses to read payload shapes because it feeds hold/overwrite *decisions*: a read that misses on an unfamiliar shape there produces a missed hold — silently moving a ticket somebody else moved, the exact failure the write queue exists to prevent — and an adapter-independent alternative (the whole verbatim record) exists and is used. Nothing landed against this and nothing here relaxes it.

**The read direction is ratified as the rule the code already converged on.** Outside an adapter, a structural JSON-path read — into an item's `payload` or into any knobas-owned serde shape addressed from SQL — is permitted only under all three of:

1. **It misses, never guesses.** An absent or unrecognized shape contributes nothing; a source shaped differently produces no derived item rather than a wrong one. This confines such reads to derivations whose tolerable failure is an absent result — never to a decision where a miss becomes a wrong action.
2. **It is confined to one named statement**, so a second source's spelling is one more `coalesce`/`or` in one place and nothing anywhere else.
3. **Its failure direction is stated and pinned by a test.** Which way a given read fails is a per-read fact, not a property of the rule: `start_work::merge`'s `not exists` over `WriteOp`'s serde shape fails *open* — a drifted path would match nothing and re-transition a ticket every pass — and is held safe only by its pin. Requirement 3 turns that from an author's habit into an obligation; a read of a knobas-owned shape is bound by 2 and 3 even though it has no adapter to drift from.

**The eventual shape is descriptor-declared, and is now on the record rather than in two implementers' asides.** Both #43's and #44's implementers independently proposed that the adapter declare these paths on `SourceDescriptor` — the source telling knobas where its status or merged-ness lives, instead of knobas learning each source's shape. That is planned as a §10.8-ratified growth of the frozen descriptor for M3, by ADR-0006's mechanism (a growth is a recorded exception, never pre-authorized). Each read this ADR governs expires into the declared field as it arrives; the three requirements are the interim discipline, not the destination.

## Considered options

- **One contextual rule spanning both directions** (#165's reviewer): "core may read payload shapes only where absence is the failure mode." True of every adapter-payload read on `main`, but framed as reconciling a conflict that did not exist — the write refusal was never violated — and silent about reads of knobas-owned shapes, whose worst instance fails toward action, not absence. The failure-direction pin (requirement 3) is what that framing leaves as habit.
- **Bind `knobas-core` only**, as #167 literally asked. Rejected: `knobas-app`'s start-work reads payload paths in three files, including the riskiest read of the whole set; a rule scoped to one crate would ratify the pattern precisely where it is safest and lapse where it is not.
- **Ban payload reads outside adapters entirely.** Rejected: §4.1 deliberately normalizes only four fields, so there is nothing adapter-independent to read for a reviewer request, a merged flag or an assignee — the ban would forbid the inbox and suggestions until the descriptor work lands, which is M3.
- **Design the descriptor field now.** Rejected: it binds M3 design today and none of the landed reads is blocked on it; recording it as planned growth satisfies the need without the bigger document.

## Consequences

- Nothing on `main` changes behavior: this ratifies what landed. `write_queue::project`'s comment is reconciled explicitly — its refusal is the write-direction rule, not a blanket ban the read sites violate — and the sites this governs reference this ADR.
- A new read arrives with all three requirements or it does not land; "which way does this fail" is a review question with a required answer, not a discovery.
- Two code comments understated §4.1 as guaranteeing only `title`/`body_text`/`updated_at`: the normalized set includes `author`, and the ADR states it correctly so the next reader reaches for a normalized field that exists before reaching into a payload.
- `CONTEXT.md` carries the vocabulary: **payload**, **payload read**, **miss**.
