---
status: accepted
---

# Link sets are read beside context membership, never through it

Sits beside [ADR-0008](0008-context-membership-is-seed-direct-links-one-hop-over-confirmed-links.md).

A **link set** is a named group of repositories, pipelines, pages, assets and monitors, each member a fixed item or a pattern (a saved query), with rules saying which tickets it applies to: every ticket under an epic, every sub-task of a ticket, every ticket in a project, or one ticket by hand. Sets widen what a job shows, so the question was whether they widen membership. Decided 2026-10-15 (Björn, D3): **no. A link set is a separate term, read beside membership, and never part of it.**

- **What a job shows** (amended 2026-10-18, #942, per spec #808 and R-2.8) starts where membership starts (the ticket, its context's explicit adds, an epic's children) and takes membership's two link layers and what is held under what they reach. From an asset typed `vm`, `reverse_proxy`, `hypervisor`, `site` or `network` it only goes down: a link step out of one follows its `monitored-by` monitors and its own `runs-on` and `depends-on`, and nothing held under it comes in. Then it adds every ancestor of a reached asset and every `monitored-by` monitor of one. Then the members of every matching set, then the parent ticket's own links, behind a global switch that is on by default. An item shows once, with the first reason it came in.
- **Computed when read, never stored as links.** Editing a set changes every job it applies to at once, and a new ticket under the epic shows the set the moment it syncs.
- **Sets never feed the membership walk.** Membership, the proposal tray's scope, the per-context inbox and the context export read ADR-0008's statement and nothing else. An item a set brings in is not a seed, and the walk never takes a hop from it.
- **Hiding is per ticket.** *Hide here* stores an exclusion for one ticket and one member. It never edits the set.
- **Rules use facts the database already keeps.** Epic and parent come from the declared parent and its stored resolution (ADR-0018, ADR-0019). Project comes from the declared project path (ADR-0010). Patterns use the search grammar.

## Rejected

- Write set members as links on each ticket. Every set edit and every new ticket means writing and deleting rows, and nobody could tell a drawn link from a set's.
- Make set members part of membership. One set on an epic would put every pipeline of a busy repository into every ticket's context, then one hop beyond, and the proposal tray would rank guesses by things nobody linked.

## Consequences

- Four knobas-owned tables (`link_set`, `link_set_member`, `link_set_rule`, `link_set_hidden`) and about six IPC commands, each with its §10.8 entry. No adapter work and no `WriteOp`.
- Two reads exist: membership, and what a job shows. Each surface names which one it uses; *This job* on the Code, Monitors, Docs and Dependencies views uses the second (R-2.8).
- The walk is one text, `knobas_core::reach_walk!`, read by the job-reach read and by the `reach:` search key alike.
- Sets join the share export beside contexts and smart lists. `CONTEXT.md` has **Link set**.
