---
status: accepted
---

# A ticket is edited by one write, and its source says what it lets a person change

Amends [ADR-0006](0006-writeop-grows-per-milestone.md), [ADR-0028](0028-an-external-adapter-is-a-sandboxed-wasm-component-installed-from-a-release.md) and [ADR-0029](0029-the-shell-draws-a-role-never-a-source.md). Björn, on PR #2479: "how much work would it be to implement the ability to change things in tickets, like status, assignee etc? It should work across different adapters ofcourse. … it should work across different ticket systems too that i might later add via wasm". Until now the only thing knobas wrote to a ticket besides a comment and a worklog was its status, and the queue compared that write on the ticket's whole record. Decided 2026-11-21 (Fable, xhigh, under Björn's standing instruction; `docs/decisions/2026-11-21-a-ticket-moves-on-the-board-and-its-fields-are-edited-where-they-are-read.md`): **a ticket's status is still moved by `Transition` and nothing else; every other change to a ticket is one write, `EditTicket`, carrying only the fields changed; and what a ticket lets a person change is its source's answer to one optional read, `ticket_fields`, drawn by the shell from a fixed vocabulary of controls.**

- **`EditTicket { entity, title, assignee, priority, labels, fields }`** (`edit_ticket`). The summary, the assignee (one account, or nobody), the priority and the labels (names added and names removed) are typed, because the shell acts on them: it lanes by them, echoes them and holds a write on them. `fields` holds a source's own fields by the adapter's id, which the shell copies from the control the adapter declared and never reads. Only what is set is written. It joins the ticket system's write vocabulary in `role::MINIMUMS`.
- **The status is not a field.** It is reached through a workflow, has its own read (`reachable_transitions`, required of the role since #1225) and its own refusals. `Transition` is unchanged.
- **`ticket_fields(entity)`** answers, per ticket and as of now, each field a person may change: its id, its label, its kind, the value the source holds, and why not where it cannot be changed. Seven kinds: text, choice, searched choice (which may be people), labels, date, number, flag. **`field_options(entity, field, typed)`** answers a searched field's choices. Both default to nothing; neither is stored; an adapter that declares `edit_ticket` answers the first for every ticket it emits (battery clause 15).
- **The write is not bound by the read.** An adapter refuses by name, at write time, a field it cannot set, and the queue keeps the refusal with the source's sentence.
- **Delivery** (ADR-0006, as amended): a `Transition` is compared on the ticket's declared status (`Projection::Status`); an `EditTicket` on the typed fields it overwrites, never on its labels or a source's own field. In both, a value an earlier open write of the same ticket was going to set is not a change. A declared path that resolves nothing compares the whole record (ADR-0007). ADR-0012 is untouched.
- **`knobas:adapter@1.6.0`** adds one optional interface, `tickets`, with `ticket-fields` and `field-options`. The write crosses as JSON as every write does. A component that declares `edit_ticket` exports it, or install refuses it; one that exports it without declaring the write loads and is never asked (the SDK exports it for every component, a wiki's included; corrected 2026-11-22, when the site sign-in build's XWiki adapter was refused). A component built against an earlier 1.x links as before, moves tickets if it declares `transition`, and its tickets' other fields are read only in knobas.
- **The board edits what it lanes by.** A card's menu offers *Assign to…* and *Priority ▸*, `a` opens the people picker at the selected card, and on a board laned by assignee or priority a drop in another lane queues the move (where the column differs), then the edit. Lanes by epic, issue type or label take no drop.
- **The window may keep what a source said a ticket can reach and may change**, for the session, keyed by the ticket as it stood, and may ask ahead of a press. It never writes the answer to the database and the write never trusts it.

## Rejected

- **`SetField { entity, field, value }` for everything.** The queue could not tell what a write overwrites, and the fields the shell reads would be an open string-typed op (ADR-0006).
- **One op a field.** Four variants every ticket adapter declares and refuses by name, against `EditPullRequest`'s precedent.
- **`SourceAction` for edits.** The shell never reads a source action, so it could echo nothing and lane nothing.
- **The status as a field of `EditTicket`.** A source does not let a status be set; it offers moves.
- **Editable fields declared in the descriptor.** Every source decides per ticket.
- **The sync mirrors each ticket's transitions and edit rights.** A per-user, per-moment answer stored as the record.
- **The whole label list in the write.** It loses a label somebody else added a second before.
- **Functions on the WIT's `adapter` resource, or a new major.** Every installed component would stop linking.

## Consequences

- One `WriteOp` variant, two SPI reads, one projection, one role read and write, battery clause 15, two IPC commands and an argument on a third, one WIT minor and SDK version. No migration and no descriptor field.
- A move queued behind a comment or behind the person's own earlier move is no longer held. A move over somebody else's move still is.
- A source's own field crosses the SPI, the IPC, the WIT and the queue the day its adapter lists it, with no change to knobas; it is not on a card, in a lane or in search until it becomes a declared path. The ticket page drawing such fields (the ruling's step 10: the fold, the seven kinds, Jira's mapping of its own fields) was cut from the first pass (2026-11-22); until it lands the page offers the summary, the assignee, the priority and the labels only.
- Jira's adapter reads `expand=editmeta` and `user/assignable/search` and writes `PUT issue/{key}` and `PUT issue/{key}/assignee`; Gitea's writes `PATCH issues/{n}` and the issue's labels, and refuses to set one assignee where an issue has several.
- The witness is the battery's clause 15 over every built-in adapter and the fixture component, the queue's tests for the two projections, the host's tests over the 1.0.0, 1.3.0, 1.5.0 and 1.6.0 fixtures, the live cases against Gitea and Forgejo, and the main path in the real app against both (2026-11-22). Jira's edit is held to recorded `editmeta` answers and to mockd's WADL only: no Jira ran, so its live case is written and unrun.
