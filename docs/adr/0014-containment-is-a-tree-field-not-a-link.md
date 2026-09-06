---
status: accepted
---

# An asset's place in the estate is a parent field, not a `holds` link

Spec §5a says one link table for every pair — work↔work, asset↔work, asset↔asset — and lists `holds`/`runs-on` among the relations; spec §12.1 says any asset can hold assets without limit; ADR-0008 ratifies that asset membership in a context "counts through ancestors". Those three do not fit together if containment is a link: membership is a fixed one-hop walk over confirmed links (ADR-0008, not configurable in v1), and "through ancestors" is transitive, so a `holds` link would either break the one-hop rule or need a relation-specific special case inside the one membership statement.

Decided 2026-09-06 (Björn, M4 grilling session): **containment is a `parent` field on the asset itself — the estate is a tree — and every other relation (`depends-on`, `runs-on`, `monitored-by`, `deployed-from`, `documented-in`, …) is a link.** The membership walk expands ancestors over the parent field and never over links; moving an asset is an edit of its parent, recorded in the asset's history like any property edit. A route belongs to the one asset that exposes it the same way, by a field, and its optional target is a field too, because "reachable via" is computed from both ends and is not a relation a person draws.

## Considered options

- **`holds` as an ordinary link, origin `manual`.** Keeps §5a's "one table for every pair" literally true. Rejected: the membership statement would need to know one relation is transitive and the rest are not, the Miller columns would draw from a self-join over links with a cycle check, and a `holds` link could be tombstoned or duplicated the way any link can, leaving an asset held twice or not at all.
- **A closure table beside the links.** Rejected as a second copy of the same fact, for the reason ADR-0008 rejects a membership table.

## Consequences

- §5a's "one table for every pair" reads as "one table for every *relation*"; containment is structure, not a relation, and `holds` leaves the relation list. The inverse pair `runs-on`↔`hosts` stays a link: a container *runs on* a VM is a relation, a VM *holds* a container is where it sits in the tree, and the two are allowed to disagree (a container held under a compose project runs on a VM elsewhere in the tree).
- Environment and owner inherit from the nearest ancestor that sets them (spec §12.1), which is a walk up the parent field with no links involved.
- The share export carries the tree as rows of the asset table; nothing about it lives in the link table, so toggling links off still exports a whole estate.
