---
status: accepted
---

# A project is a first-class scoping dimension, and it keeps the source's own word

One Jira base URL holds many projects — `INT-`, `ERP-` — each with its own workflow, and one PAT sees all of them. knobas has no word for that grouping: a room is *All work*, one per configured source, or a stored context (ADR-0008), and none of those is "one project". So the M2.5 mini board (ADR-0009) over a multi-project Jira draws the **union of every workflow's statuses**, in a tile that is ~550px wide at the 1100px window floor against a 150px minimum column — three columns visible, the rest behind a sideways scrollbar.

Decided 2026-08-30 (Björn, M2.6 grilling session): **a project is a first-class scoping dimension, modelled per source and named after the source's own word.** Jira has projects and TeamCity has projects (`TeamCityConfig.project_ids`); Gitea has no such thing here and gets none. The value needs no new sync: `BASE_FIELDS` has requested `project` since M1, so `fields.project = { key, name }` is already on every mirrored issue. Reading it is a **payload read outside an adapter** governed by ADR-0007 — one named statement, one `coalesce` per source that spells it differently, and its failure direction pinned: a ticket whose record carries no readable project **belongs to no project room** and is still in *All work* and its source's room. Absence, never a wrong room, and no "No project" room — nothing is hidden, because two other rooms still hold it. (That is deliberately *unlike* the mini board's terminal group, where the column is the only place a statusless ticket could appear at all.)

## Considered options

- **One generic container concept** — a "space" or "container" that Jira projects, TeamCity projects and Gitea repositories all map into. Rejected: a Gitea repository is already an entity **kind** in knobas, with its own address, its own monogram, and its own word in `0001_init.sql`'s kind list — it is one of the things a room draws, not the axis a room is drawn along. Folding it into a scoping concept would model one thing twice in two incompatible ways. Inventing a neutral word to dodge a collision that does not exist is the ADR-0009 mistake run backwards.
- **Jira-only, hardcoded, revisit later.** Rejected: it costs the same as the decision above and records nothing, so the second source that wants it re-argues from scratch.
- **One configured knobas source per project** (`projects: ["INT"]`, `projects: ["ERP"]`), which already yields a room each. Rejected, and worth stating because nothing currently refuses it: two sources against one base URL are two namespaces, so an issue both can see becomes `int:INT-1` **and** `erp:INT-1` — two entities, two mirror rows, and no link between them.
- **Leave it to ad-hoc contexts.** Rejected: a context's membership is seed + direct links + one hop over confirmed links (ADR-0008), which cannot express "every ticket carrying this attribute". An ad-hoc *INT* room would hold what somebody added to it, not the project.

## Consequences

- `CONTEXT.md` gains **Project**. The **Context** entry keeps `project` in its `_Avoid_` list — for its own reason, which is that a context is not a project — and now says so, or the next reader meets what looks like a contradiction.
- Two §10.8 entries follow: the room filter grows a project dimension, and an additive command reports the projects a source's corpus shows. The switcher cannot derive them from a room's own scan — that reads the newest 200 items and is "a window, not a census", so a quiet project would silently have no room.
- **The switcher's length becomes a property of the corpus rather than of the configuration.** Two projects is a convenience; forty is a problem this decision creates and does not solve.
- The mini board gains a room kind whose columns are one workflow, which is what makes its column layout usable at tile width. Which layout a room draws is the milestone's spec, not this ADR.
