---
status: accepted
---

# An importer produces an estate file; it is not a source

Spec §3a lists asset-import adapters (Proxmox, Docker host, Traefik) under *Sources*, beside Jira and Gitea, and the `Source` trait carries an unused `Capability::Import` for them. But an asset is never a mirrored item (`CONTEXT.md`, **Asset**): an accepted import makes an ordinary asset with an origin line, after which no sync overwrites what a person edited — the opposite of what the sync path does to an item on every run. The estate import that shipped in M4.0 (#439) is app-side for that reason: it takes an estate file's text, previews it against the tree by id, and applies what the preview said, with no `Source`, no `Sink`, no cursor and no `sync.item` involved. Three readers already share the file format as their seam — `--demo`, the M4.0 exit checklist and the dev-mode Tree.

Decided 2026-09-07 (Björn, v1.5 grilling session): **an importer — hcloud, a Docker host, whatever comes next — is a producer of an estate file in that same shape, and the existing Import is its preview and its apply.** It appears in the Import dialog's chooser and never in the Sources view; its credential lives in the keychain under an `importer:` namespace, not a source's; the `Source` trait does not change and `Capability::Import` stays undeclared. An importer matches an asset the tree already holds by an **origin key** (`CONTEXT.md`) when the file's id is unknown, so a re-import of a live system previews as *already in the tree* the way a re-import of the checked-in file does.

## Considered options

- **A `Source` with `Capability::Import`, emitting assets through `sync`.** Rejected: every sync-path guarantee — upsert on every run, sweep of what a full sync no longer emits, tombstones — is a guarantee about a mirror, and each one is wrong for an asset a person has edited. Making the engine skip those for one kind would be a second engine inside the first.
- **A new trait beside `Source`.** Rejected as a seam with one implementation on each side of it: the estate file already is the seam, and a trait would restate its schema in Rust.

## Consequences

- Spec §3a's "asset adapters show configured / not configured" moves from the Sources view to the Import dialog; the *Sources* word stays reserved for systems knobas mirrors.
- The witness for an importer is the real estate (ADR-0013): a file generated from the live hcloud or Docker host previews as all-known against `testenv/hetzner/estate.json`, and a disagreement between the two is a red test, not a judgement call.
- The secrets crate's key space grows one namespace. That is the only shared code an importer touches.
