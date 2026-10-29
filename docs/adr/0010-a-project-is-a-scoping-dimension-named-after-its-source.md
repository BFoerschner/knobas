---
status: accepted
---

# A project is a first-class scoping dimension, and it keeps the source's own word

One Jira base URL holds many projects, each with its own workflow, and knobas had no word for that grouping, so the mini board over a multi-project Jira drew the union of every workflow's statuses. Decided 2026-08-30 (Björn): **a project is a first-class scoping dimension, modelled per source and named after the source's own word.**

- Jira and TeamCity have projects. Gitea's code kinds (`repo`, `branch`, `pr`, `commit`) have none; scoping to one repository's code is its parent relation. (Amended 2026-10-19, #1206, #1247) A Gitea **ticket's** project is its repository, `repository.full_name`, because ADR-0029's ticket-system minimum requires a project on every ticket. Only the `ticket` kind declares it.
- Reading it is a payload read under ADR-0007: one named statement, one `coalesce` per source spelling, failure direction pinned. A ticket with no readable project belongs to no project room and is still in *All work* and its source's room. There is no "No project" room.
- A context is not a project. `CONTEXT.md`'s **Context** entry keeps `project` in its avoid list and says why.

## Rejected

- One generic container concept for Jira projects, TeamCity projects and Gitea repositories. A repository is an entity kind with its own address; folding it into a scoping concept models one thing twice.
- Jira-only, hardcoded. Costs the same and records nothing.
- One configured source per project. Two sources against one base URL are two namespaces, so one issue becomes two entities.
- Ad-hoc contexts. Membership (ADR-0008) cannot express "every ticket carrying this attribute".

## Consequences

- `CONTEXT.md` has **Project**. The room filter carries a project dimension and a command reports the projects a source's corpus shows, each with a §10.8 entry.
- The switcher's length is a property of the corpus, not the configuration. Forty projects is a problem this decision creates and does not solve.
