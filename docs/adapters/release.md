# Releasing

A reader installs an external adapter from a release: a manifest, `knobas-adapter.json`, and the component it names, side by side. knobas pins the release it installed and never updates on its own. This page is the manifest, where a release may live, and what install, update, uninstall and restore do with it ([ADR-0028](../adr/0028-an-external-adapter-is-a-sandboxed-wasm-component-installed-from-a-release.md)).

Source: `crates/knobas-wasm-host/src/manifest.rs`, `crates/knobas-app/src/sources/{remote,install,installed}.rs`.

## The manifest

```json
{
  "adapter_kind": "xwiki",
  "adapter_version": "0.1.0",
  "interface_version": "1.5.0",
  "artifact": { "file": "knobas_adapter_xwiki.wasm", "sha256": "f5c07da6…" },
  "hosts": ["{base_url}"],
  "limits": { "deadline_secs": 900, "memory_mib": 256 }
}
```

| Field | Rule |
|---|---|
| `adapter_kind` | a slug, as an instance id is ([reference/descriptor.md](reference/descriptor.md#instance-ids)); equal to the descriptor's `adapter_kind` |
| `adapter_version` | not empty; equal to the descriptor's `adapter_version` |
| `interface_version` | the `knobas:adapter` version the component is built against, exactly, patch included: the SDK's version at your tag |
| `artifact.file` | a bare file name beside the manifest: no directory, no `:` |
| `artifact.sha256` | the file's SHA-256, 64 lower-case hex digits |
| `hosts` | the allowlist, `{base_url}`, `{<config key>}` or a fixed host ([http.md](http.md#which-hosts-a-request-may-reach)); empty reaches nothing |
| `limits` | optional: `deadline_secs` up to 3600 (default 600), `memory_mib` up to 1024 (default 256); zero, or more than the maximum, is refused |

Every field is required but `hosts` and `limits`, and a field the manifest does not know is refused: a misspelt `host` would otherwise install an adapter that reaches nothing. Write the manifest from your build so the hash and versions cannot drift; the XWiki adapter's [`scripts/build.sh`](https://github.com/BFoerschner/knobas-adapter-xwiki/blob/main/scripts/build.sh) reads the version from `Cargo.toml` and hashes the component it just linked.

## Interface versions

A knobas release hosts one major version of `knobas:adapter`, up to its own minor. It hosts a component of the same major and the same minor or an older one; a minor only adds, and a component built against an older minor still links, with the newer reads answering their defaults. Install refuses a component built against a newer minor than the reader's knobas hosts.

The SDK's version is the interface version it binds, and you choose it by the knobas tag you depend on. A newer tag lets your adapter use more, and shuts out readers whose knobas is older than that tag's minor. The SDK's `v1_3` and `v1_0` features build against the frozen 1.3.0 and 1.0.0 worlds; knobas uses them to test its hold on older components, and an adapter could use them to reach older readers while doing without what came after (no refresh before 1.4.0, no docs before 1.5.0).

| Version | Added |
|---|---|
| 1.0.0 | the world: `source`, `sink`, `log` |
| 1.1.0 | downloads (`download-sink`, `downloads`) |
| 1.2.0 | `query`, a saved view's query translated |
| 1.3.0 | `sink.walked`, run progress |
| 1.4.0 | `refresh` |
| 1.5.0 | `docs`, the projection and the docs writes |

## Where a release lives

What the reader pastes under *Install an adapter* is one of:

- **a GitHub repository URL**, `https://github.com/<owner>/<repo>`: its latest release, whose assets must include `knobas-adapter.json` and the file it names;
- **a manifest URL**: any URL of a `knobas-adapter.json`. One of the shape every forge's release downloads have, `<forge>/<owner>/<repo>/releases/download/<tag>/knobas-adapter.json` (GitHub, Gitea, Forgejo), carries its tag, and the forge's release API says which release is the latest. The artifact is fetched from beside the manifest;
- **a local path** to a `knobas-adapter.json`, for development. It has no tag; its hash alone pins it.

Releases are fetched over HTTPS through knobas' HTTP stack, with no credential: a release must be public. Plain HTTP is allowed only to this machine, and only when the reads started there. The manifest may be at most 1 MiB and the artifact 64 MiB.

## Install, update, uninstall, restore

**Install** fetches the release and runs every static check ([testing.md](testing.md#static-what-install-checks)), then shows the reader the adapter's name, version, roles, the hosts it may reach and its limits, and writes nothing. Only *Install* installs, held to the hash of what was shown, so a release that moved in between is refused. knobas stores the component in the profile under its hash, records the repository, the tag and the hash, and re-hashes the stored file at every start: the code that holds a credential is the code the reader installed, or none. Install refuses a kind a built-in or an installed adapter already has.

**Update.** knobas checks for a newer release and says one exists; it never switches on its own. Updating shows the new release beside the installed one, runs every static check on it, and is held to what was shown. A refused update leaves the installed adapter as it was.

**Uninstall** asks, for each source of the adapter, what *Remove source and its items* asks: purge its items, or keep them, disabled.

**Restore.** A backup records each external adapter as repository, tag and hash, never as the `.wasm`. Restoring fetches each release at its recorded tag and refuses an artifact whose hash changed. So **never move a tag or replace a release's assets**: every reader's backup names them. Publish a new version instead.

## A release workflow

The XWiki adapter's [`.github/workflows/release.yml`](https://github.com/BFoerschner/knobas-adapter-xwiki/blob/main/.github/workflows/release.yml), on a pushed tag `v<version>`:

1. fails unless the tag is the crate's version;
2. runs the tests;
3. builds the component and writes the manifest;
4. runs install's static checks on both;
5. publishes `knobas-adapter.json` and the `.wasm` as the release's assets.

The live checks need an instance of the product, so they run on the author's machine before the tag ([testing.md](testing.md)).

When a release changes what a payload holds or how the cursor reads, bump the cursor's version. An adapter reads a cursor of another version as none and walks the source from the top, so every item is rewritten in the new shape on the first run after the update. Remember that the engine does not sweep after that walk ([sync.md](sync.md#deletions)).
