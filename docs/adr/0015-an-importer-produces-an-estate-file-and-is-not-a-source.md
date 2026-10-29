---
status: accepted
---

# An importer produces an estate file; it is not a source

Spec §3a listed asset importers (Proxmox, Docker host, Traefik) under sources, but an asset is never a mirrored item: an accepted import makes an ordinary asset that no sync overwrites afterwards, the opposite of what the sync path does to an item. Decided 2026-09-07 (Björn, v1.5 grilling): **an importer produces an estate file in the shape the M4.0 Import already reads, and that Import is its preview and its apply.**

- An importer appears in the Import dialog's chooser and never in the Sources view. The word *source* stays reserved for systems knobas mirrors.
- Its credential lives in the keychain under the `importer:` namespace. That namespace is the only shared code an importer touches.
- The `Source` trait does not change and `Capability::Import` stays undeclared.
- An importer matches an asset the tree already holds by an origin key (`CONTEXT.md`) when the file's id is unknown, so a re-import of a live system previews as *already in the tree*.
- The witness is the real system (ADR-0013); how it is read depends on what the checked-in estate holds (amended 2026-10-12, #634). hcloud: a round trip against the real project, which is empty between runs. Produce, apply, produce again, and the last file previews as all known, every server matched by its `hcloud_id`. A server is stood up on demand for one run (`just estate-live`, `ESTATE_LIVE_HCLOUD_SERVERS`). Docker: a file generated from the engine the estate names previews as all known against `testenv/estate.json`, and a disagreement is a red test.

## Rejected

- A `Source` with `Capability::Import` emitting assets through `sync`. Every sync guarantee (upsert every run, sweep, tombstones) is wrong for an asset a person edited.
- A new trait beside `Source`. A seam with one implementation on each side; the estate file already is the seam.
