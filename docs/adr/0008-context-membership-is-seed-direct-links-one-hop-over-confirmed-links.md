---
status: accepted
---

# Context membership is seed + direct links + one hop, computed over confirmed links, never stored

Spec §16.11 says "explicit adds + direct links + one hop, fixed rule, not configurable in v1; asset membership counts through ancestors", and its three terms admit two readings. Decided 2026-08-27 (Björn), recorded for #47: **membership is three layers over the confirmed link graph, plus containment, computed at read time.** The deciding evidence is §16.11's own example, "a member ticket's PRs, their builds": the ticket is a seed, its PRs are direct links, their builds are the one hop. The two-layer reading cannot produce that sentence for an epic.

The rule, as `knobas_core::context::MEMBER_IDS` implements it in one `with recursive` statement:

- **Seed.** Every confirmed link touching the context's own `ctx:` entity (an *Add to context* is an ordinary link), the anchor of a promoted context, and the epic's children: mirrored items whose source-recorded parent (`fields.parent.key`, or since #641 the classic `epic.key` the Jira adapter stores beside the record, read second) names the anchor, matched inside the anchor's own source id.
- **Direct links.** Everything the seeds link to.
- **One hop.** Everything those link to. Then it stops.
- **Held** (amended 2026-09-06, #434). Everything under something the walk reached, at any depth, over `knobas.asset.parent_id` and never over links. It is the last layer: a brought-in asset contributes no seeds, links or hops of its own. Containment is the only transitive thing in the rule (ADR-0014).

Three subsidiary rules:

1. **Computed at read time, never materialised.** A membership table would be a second copy of the link graph every link write has to keep honest. If the walk ever costs too much, the fix is a cache with an invalidation story, taken as its own decision.
2. **The walk reads `knobas.confirmed_link` at every step.** The link table holds proposals too, and the tray scopes proposals by membership (`suggest::proposals`' `members` parameter). Membership built from proposals would make the two circular. `link_reads.rs` scans the Rust tree and holds the query to it.
3. **The walk never traverses another context.** Every expansion refuses `ctx`-kind neighbours; otherwise one shared ticket unions two contexts.

The epic-children read is a payload read outside an adapter under ADR-0007: one named statement, it misses rather than guesses, and `a_foreign_or_misshapen_parent_contributes_nothing` pins that the failure is an absent member. The promote path's issue-type read (`ANCHOR_SHAPE`) has the same discipline and pin: a miss promotes a ticket-kind context, never a wrong epic.

## Rejected

- Two layers (the context node's links plus one hop). Cannot reach the ratified example for an epic.
- A recursive walk with a depth parameter. "Fixed rule, not configurable in v1."
- Storing membership rows on link creation. Any link write one hop away changes a context's membership, so the invalidation surface is the whole link table.
- Counting proposals toward membership, dimmed. Circular with the tray, and a share export would contain guesses nobody audits.

## Consequences

- One rule, one statement, one battery: `crates/knobas-core/tests/contexts.rs`. The room's page, its tiles, the per-context inbox filter and the tray's scope all resolve membership server-side through `context::member_ids`.
- Membership can be wide. Narrowing it reopens this ADR rather than growing a filter somewhere.
- A withdrawn entity stays a member (§5a: links to it survive); each surface decides how to show it, and mirror reads filter through `sync.live_item`.
- Asset membership through ancestors is computed, never stored. There is no asset-to-context row (spec #427): the link between asset and ticket implies the membership, and removing the link removes it. `knobas_core::link::Origin::Implied` stays in the vocabulary for `knobas_core::note`'s rows; this path writes none.
- Ad-hoc contexts get the whole rule for free: their seed is the explicit adds.
