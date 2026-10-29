---
status: accepted
---

# An external adapter is a sandboxed WASM component installed from a release

Spec §3a made adapters compile-time Rust crates with "out of process later". A plugin pulled from someone else's repository runs on the machine that holds every credential knobas has. Decided 2026-10-18 (Björn, custom-sources grilling): **an [external adapter](../../CONTEXT.md#external-adapter) is one WASM component that knobas runs under wasmtime, installed from a release named by a manifest.**

- **The interface is a versioned WIT world** (`knobas:adapter@<semver>`), a transcription of the `Source` trait and its serde types. A knobas release hosts one major version. A minor version is additive and a §10.8 entry. The manifest states the version and install refuses one knobas cannot host.
- **The sandbox grants no disk and no keychain.** The only way out is an HTTP capability limited to the hosts the manifest declares, fixed or derived from config (`{base_url}`), shown to the reader at install. The host refuses any other host. **The adapter never sees the credential**: the host attaches it only to requests that go to the configured instance. No OAuth until an adapter needs it.
- **Every call has a deadline and a memory cap.** An adapter that passes either is stopped, its run ends failed (ADR-0005), and the source's health line says why.
- **What you point knobas at is a manifest URL.** A GitHub repository URL resolves to its latest release's `knobas-adapter.json`. A local file is allowed for development. Install records the repository, the tag and the artifact's hash. knobas never updates on its own; it says a newer release exists, and updating re-runs the static checks. A backup records an external adapter as repository, tag and hash, never as the `.wasm`, and restore refuses an artifact whose hash changed.
- **Uninstalling an adapter that sources still use** asks per source what *Remove source and its items* asks: purge, or keep the items disabled.
- **The checks.** At install and update, the static checks: the descriptor is well formed, its kinds and declared paths cover every [role](../../CONTEXT.md#role) it claims, and its WIT version is supported. A failed check refuses the install. On each source's first sync, the data checks: the declared paths resolve on real records. A failed data check does not stop the sync; the shell does not draw that role's surface and the health line names the path that missed. The [adapter standard](../../CONTEXT.md#adapter-standard) ships an SDK and `knobas adapter check <file.wasm>`, which runs the contract suite as the author's local gate.
- **The built-in adapters stay compiled in**, held to the same role contracts without crossing the WIT boundary. Uptime Kuma could not: it reads over socket.io and the sandbox has no raw sockets.

## Rejected

- Clone the repository and build it locally. Every reader would need a Rust toolchain, and an installed app cannot link a new crate into itself.
- A native executable from the release over JSON-RPC on stdio. It runs with the reader's disk and keychain, needs one build per OS and architecture, and macOS quarantines it.
- A declarative adapter mapping a REST API to items. Cursors, pagination and auth quirks are most of every adapter, and a mapping language that covers them is a programming language with no debugger.

## Consequences

- ADR-0013 still binds the built-in adapters. knobas cannot certify an external adapter against its real product: the author vouches for correctness and the checks prove only that the contract holds. No external adapter ships with knobas; a fixture component is what the host is tested with.
- The secrets crate keeps every external adapter's credential under one namespace per installed source, as for any source.
- Amends spec §3a.
