---
status: accepted
---

# An external adapter is a sandboxed WASM component installed from a release

Spec §3a made adapters compile-time Rust crates, with "out of process later". Björn, 2026-10-18: *"I'd like to be able to point at a github repo and it pulling the source like a plugin system."* That means running code from a repository someone else controls, on the machine that holds every credential knobas has.

Decided 2026-10-18 (Björn, custom-sources grilling): **an [external adapter](../../CONTEXT.md#external-adapter) is one WASM component that knobas runs under wasmtime, installed from a release named by a manifest.**

- **The interface is a versioned WIT world** (`knobas:adapter@<semver>`), a transcription of the `Source` trait and its serde types. A knobas release hosts one major version. A minor version is additive (a new optional read, a new block type) and is a §10.8 entry, like any frozen surface. The manifest states the version, and install refuses one knobas cannot host.
- **The sandbox grants no disk and no keychain.** The adapter's one way out is an HTTP capability limited to the hosts its manifest declares, fixed or derived from config (`{base_url}`). The reader sees that list at install. The host refuses a request to any other host. **The adapter never sees the credential.** The host attaches it, a header token or basic auth, only to requests that go to the configured instance. OAuth is not supported until an adapter needs it.
- **Every call has a deadline and a memory cap.** An adapter that passes either is stopped. Its run ends failed (ADR-0005), and the source's health line says why.
- **What you point knobas at is a manifest URL.** A GitHub repository URL is a shortcut that resolves to its latest release's `knobas-adapter.json`. A local file is allowed, for development. Install records the repository, the tag and the artifact's hash. Knobas never updates on its own: it says a newer release exists, and updating re-runs the static checks first. A backup export records an external adapter as that repository, tag and hash, never as the `.wasm`. Restore reinstalls from the record and refuses an artifact whose hash changed.
- **Uninstalling an adapter that sources still use** asks, per source, what *Remove source and its items* asks: purge, or keep the items disabled.
- **The checks.** At install and at update, the static checks run: the descriptor is well formed, its kinds and declared paths cover every [role](../../CONTEXT.md#role) it claims, and its WIT version is supported. A failed check refuses the install. On the first sync of each source, the data checks run: the declared paths resolve on real records. A failed data check does not stop the sync. The shell does not draw that role's surface, and the health line names the path that missed. The [adapter standard](../../CONTEXT.md#adapter-standard) ships an SDK and `knobas adapter check <file.wasm>`, which runs the contract suite against the component as the author's local gate.
- **The built-in adapters stay compiled in.** They are held to the same role contracts but do not cross the WIT boundary. Uptime Kuma could not: it reads over socket.io, and the sandbox has no raw sockets.

## Considered options

- **Clone the repository and build it locally.** Rejected. Every reader would need a Rust toolchain, and an installed app cannot link a new crate into itself, so the build would have to produce an executable anyway.
- **A native executable from the release, over JSON-RPC on stdio.** Rejected. It runs as the reader, with their disk and keychain, so a wrong URL becomes a stolen credential. It also needs one build per OS and architecture, and macOS quarantines it.
- **A declarative adapter** (a config file that maps a REST API to items). Rejected. Cursors, pagination and auth quirks are most of every real adapter, and a mapping language that covers them is a programming language with no debugger.

## Consequences

- ADR-0013 still binds the built-in adapters. Knobas cannot certify an external adapter against its real product: the author vouches for correctness, and the checks prove only that the contract holds. No external adapter ships with knobas: a fixture component is what the host is tested with, so the standard has no real-instance witness until somebody writes an adapter against a real product. That is the risk this decision accepts. (Amended 2026-10-18: a GitHub adapter was planned as that witness and dropped the same day; GitHub was an example, not a goal.)
- The secrets crate keeps every external adapter's credential. The only new key space is one namespace per installed source, as for any source.
- Amends spec §3a's "compile-time plugins now, out-of-process later".
