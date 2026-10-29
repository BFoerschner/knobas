---
status: accepted
---

# An asset's place in the estate is a parent field, not a `holds` link

Spec §5a lists `holds` among the link relations, spec §12.1 lets any asset hold assets without limit, and ADR-0008 counts asset membership "through ancestors". Those three do not fit if containment is a link: membership is a fixed one-hop walk over links, and "through ancestors" is transitive. Decided 2026-09-06 (Björn, M4 grilling): **containment is a `parent` field on the asset itself, the estate is a tree, and every other relation (`depends-on`, `runs-on`, `monitored-by`, `deployed-from`, `documented-in`) is a link.**

- The membership walk expands ancestors over the parent field and never over links.
- Moving an asset is an edit of its parent, recorded in the asset's history like any property edit.
- A route belongs to the one asset that exposes it by a field, and its optional target is a field too.
- Environment and owner inherit from the nearest ancestor that sets them (spec §12.1), a walk up the parent field.

## Rejected

- `holds` as an ordinary link. The membership statement would need one transitive relation among non-transitive ones, the Miller columns would need a cycle check, and a link can be tombstoned or duplicated, leaving an asset held twice or not at all.
- A closure table beside the links. A second copy of the same fact, for the reason ADR-0008 rejects a membership table.

## Consequences

- §5a's "one table for every pair" reads as one table for every relation. `holds` leaves the relation list. `runs-on` stays a link and may disagree with the tree.
- The share export carries the tree as rows of the asset table, so toggling links off still exports a whole estate.
