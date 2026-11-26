# Checklist

For a person before a tag, and for an agent building an adapter end to end: work down the list, and do not tag while an item is open. Each item links to where it is explained.

## Before the first line of code

- [ ] Read [concepts.md](concepts.md) and pick the role you fill, or a custom role `<kind>:<name>` if none fits.
- [ ] Stand up a real instance of the product with test data ([testing.md](testing.md#a-test-instance)), and record real responses for each endpoint you will read. Check what a bad credential really gets: a `401`, or a guest's `200`.
- [ ] Find how the product reports changes since a point, and **deletions**. If it reports none, decide how the cursor finds them ([sync.md](sync.md#deletions)).

## The descriptor

- [ ] `adapter_kind` is a constant slug, never `instance.kind`.
- [ ] The descriptor depends on nothing but the instance's `id` and `name`: knobas reads it with config `{}`, an empty base URL and no network. The template's name, read with `name == adapter_kind`, is the product's name.
- [ ] `adapter_version` is the crate's version, which is the manifest's.
- [ ] `Capability::Write` exactly when `write_ops` is not empty; `Capability::Refresh` exactly when `refresh` answers for some kind, and `refreshes` set per kind.
- [ ] Each kind's `full_sync_exhaustive` is true only if a full sync hands over all of it.
- [ ] `config_schema` holds no secret. With `UserPassword`, it declares `username`. No property name ends in a credential word (`key`, `token`, `pat`, `password`, …): the Add-source form refuses to draw it ([reference/descriptor.md](reference/descriptor.md#config_schema)).
- [ ] A unit test resolves every declared path on a payload your sync builds. A path that runs into a string reads as nothing, and no check catches it ([sync.md](sync.md#declared-paths-and-the-trap-of-a-string)).

## Sync

- [ ] Every item id is `<source id>:<stable key>`; every kind is declared.
- [ ] `sink.item(item)?`, never ignored.
- [ ] An idle run returns the cursor it was given, byte for byte. Changes out of scope do not move it. The position comes from the same clock the next run filters on.
- [ ] Deletions reach the mirror from incremental runs: knobas sweeps only after a source's first run.
- [ ] The cursor has a version; a cursor of another version is a walk from the top.
- [ ] Faults: 401/403 is `Unauthorized`, no answer is `Unreachable`, the rest `Protocol` with a message that says what to do. The same in `test_connection` and `sync`.
- [ ] `refresh`, if offered, builds its item with the same code as `sync`.
- [ ] A full sync fits the deadline at five requests a second ([http.md](http.md#pacing-and-time)); ask for a longer one in `limits` if it does not.

## Writes

- [ ] An undeclared op is refused with `Protocol`.
- [ ] A write that left and got no answer is `Unconfirmed`, not `Unreachable`.
- [ ] An edit against an old version is `Moved { base, current }`, and writes nothing.
- [ ] A create answers the new item's key; a comment its id; everything else `none`.
- [ ] Typed text is escaped into the source's markup and round-trips.

## A docs source

- [ ] `project` answers `None` for every kind but `page`, and reads nothing but the payload.
- [ ] Includes and page links carry the target's title and space key, resolved at sync and kept in the payload.
- [ ] `sections` has one entry per top-level heading of `blocks`, from 1, each with text or a refusal; `edit_section` refuses the same sections for the same reasons.
- [ ] A part you cannot draw is a placeholder, not dropped.

## The gate

- [ ] `cargo test`, `cargo clippy -- -D warnings` on the host and on `wasm32-wasip2`.
- [ ] The static checks pass on the built component and its manifest.
- [ ] Against the real instance: the role data check, the docs contract and the whole contract battery pass, run with a user that does not exist for the unauthorized case.
- [ ] Every declared write performed and read back against the test instance.
- [ ] Installed once into a real knobas from the local manifest, a source added, *Test connection* run, a sync looked at.

## Release

- [ ] The manifest is written by the build: hash, versions, hosts, limits.
- [ ] The tag is `v<version>`, and the release carries `knobas-adapter.json` and the `.wasm`.
- [ ] No tag moved and no asset replaced, ever: readers' backups name them.
- [ ] A changelog entry says what readers see change.

## When install refuses

The reader sees the refusal's text. Each is one static check ([testing.md](testing.md#static-what-install-checks)).

| The refusal says | Fix |
|---|---|
| `the manifest is not well formed: …` | the named field; unknown fields are refused too |
| `the manifest asks for knobas:adapter@X, and this knobas hosts knobas:adapter@1.x up to Y` | build against an older SDK tag, or the reader updates knobas |
| `cannot read the artifact …` | `artifact.file` names no file beside the manifest |
| `the artifact's SHA-256 is …, and the manifest names …` | rebuild the manifest from the file you publish |
| `the artifact is not a knobas adapter: …` | not a component, or it exports no `knobas:adapter/source`, or imports something the host does not provide |
| `the artifact is built against knobas:adapter@X, and the manifest says Y` | `interface_version` is the SDK's version, exactly |
| `the adapter's descriptor is not well formed: …` | the descriptor's JSON; it must answer without config or network |
| `the descriptor names adapter_kind … and the manifest …` | `adapter_kind` or `adapter_version` disagree |
| `the manifest's host template {key} names no key the descriptor's config_schema declares (nor base_url)` | declare the key, or use `{base_url}` |
| `the manifest's limits are refused: …` | over 3600 s or 1024 MiB, or zero |
| `the adapter's role claims are refused: …` | the kind, path or read the message names ([reference/descriptor.md](reference/descriptor.md#why-a-claim-is-refused)) |
| `the adapter claims the wiki role, and its component exports no docs projection …` | build against 1.5.0 or later and implement `project` |
