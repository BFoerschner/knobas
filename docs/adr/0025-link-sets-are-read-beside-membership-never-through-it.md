---
status: accepted
---

# Link sets are read beside context membership, never through it

Sits beside [ADR-0008](0008-context-membership-is-seed-direct-links-one-hop-over-confirmed-links.md).

A ticket should get most of its connections without anyone linking them one by one. In the ticket-first redesign (`mockups/ticket-first/` section 7, D3) a **link set** is a named group of repositories, pipelines, pages, assets and monitors. Each member is either a fixed item or a pattern, which is a saved query that picks up new items as they appear. Rules say which tickets the set applies to: every ticket under an epic, every sub-task of a ticket, every ticket in a project, or one ticket by hand. A job also shows its parent ticket's own links, behind a global switch that is on by default.

ADR-0008 fixes context membership as seed, direct links and one hop over confirmed links. It says that narrowing or widening membership reopens that ADR rather than growing a filter somewhere. Sets widen what a job shows, so the question was whether they widen membership.

Decided 2026-10-15 (Björn, D3): **no. A link set is a separate term, read beside membership, and never part of it.**

- **A job shows** its context's members (ADR-0008, unchanged), then the members of every set whose rule matches the ticket, then the parent ticket's own links. An item shows once, with the first reason it came in.
- **Computed when read, never stored as links.** Nothing is copied onto the ticket. Editing a set changes every job it applies to at once, and a new ticket under the epic shows the set the moment it syncs.
- **Sets never feed the membership walk.** Membership, the proposal tray's scope, the per-context inbox and the share export of a context read ADR-0008's statement and nothing else. An item a set brings in is not a seed, and the walk never takes a hop from it.
- **Hiding is per ticket.** *Hide here* stores an exclusion for one ticket and one member. It never edits the set.
- **Rules use facts the database already keeps.** Epic and parent come from the declared parent and its stored resolution (ADR-0018, ADR-0019). Project comes from the declared project path (ADR-0010). Patterns use the search grammar.

## Considered options

- **Write set members as links on each ticket.** Rejected: every set edit and every new ticket would mean writing and deleting rows, and nobody could tell a link a person drew from one a set made.
- **Make set members part of membership.** Rejected: it is ADR-0008's own warning. One set on an epic would put every pipeline of a busy repository into every ticket's context, then one hop beyond that, and the proposal tray would rank guesses by things nobody linked.

## Consequences

- One migration with four knobas-owned tables (`link_set`, `link_set_member`, `link_set_rule`, `link_set_hidden`), about six IPC commands, each with its §10.8 entry. No adapter work and no `WriteOp`.
- Two reads exist where there was one: membership, and what a job shows. Each surface names which one it uses. *This job* on the Code, Monitors, Docs and Dependencies views uses the second (requirements R-2.8).
- Sets join the share export beside contexts and smart lists.
- `CONTEXT.md` gains **Link set**.
