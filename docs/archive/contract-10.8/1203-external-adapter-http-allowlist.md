- **What an external adapter may import beside `knobas:adapter@1.0.0`: `wasi:http` 0.2's
  `outgoing-handler` and `types`, held to the manifest's hosts, with the credential attached by
  the host — issue #1203 (2026-10-19).** `wit/adapter.wit` keeps its types and its version
  (1.0.0); only its `world` doc comment changes, to name the import. No migration, no new command,
  no IPC change (the install note reads `InstalledAdapter.hosts`, which #1202's entry already
  holds), no change to `crates/knobas-source/src/**`, `crates/knobas-http/**` or `error.rs`.
  Parent spec #1192 (ADR-0028 *The sandbox*). Ratified by the implementer on Opus under Björn's
  2026-10-18 reserved-steps rule; the ruling is in `docs/archive/2026-10-unattended-rulings.md`.

  **Why this is not a new interface version.** #1198's entry gives the component WASI 0.2 beside
  the world and names #1203 as the one that adds network access. `wasi:http` 0.2 is part of WASI
  0.2 and lives in no `knobas:adapter` type: no WIT record, variant or function of the world
  changes, so a component built before this links as it did, and a component that uses
  `wasi:http` names its own WASI version, which wasmtime's linker matches across 0.2.x patches.
  The world's interface stays frozen as #1198's entry says. This entry amends #1198's
  minor-version rule to read "a new `knobas:adapter` interface", so the two agree. A later knobas that drops `wasi:http`,
  or moves to `wasi:http` 0.3, is a new major version of `knobas:adapter`.

  **The allowlist.** A manifest's `hosts` entry is a fixed host (`api.example.com`,
  `api.example.com:8443`; no scheme, path, wildcard or account) or a whole-entry template `{key}`,
  whose key is `base_url` (the source's address) or a property the descriptor's `config_schema`
  declares. Install refuses any other entry, and a template over an undeclared key. Each source
  resolves the templates over its own address and config; a value that is not a URL or a host
  allows nothing. A request leaves only for a host the list allows, compared on the scheme, host
  and port the URL parser resolves (lower-cased, IDNA-encoded, any IPv4 spelling read as its
  address), never on text. A fixed host without a port allows the scheme's default port; a URL
  value allows its own scheme, host and port. Any other request is answered `connection-refused`
  (`wasi:http`'s `error-code`) before a socket opens, and logged on the host's side naming the
  host. The transport follows no redirect, so a redirect the component follows is a new request
  and is held to the list the same way. A component loaded without a manifest (the tests' fixture,
  `WasmAdapter::load`) reaches nothing.

  **The credential.** It is attached only to a request whose scheme, host and port are the
  source's address: `Authorization: Bearer <secret>` for `Pat` and `ApiToken`, `Authorization:
  Basic` over the config's `username` and the secret for `UserPassword`. `OAuth`, and
  `UserPassword` without a `username`, fail the source's build. An `Authorization` the component
  set on such a request is replaced, and its `Accept-Encoding` dropped. A request to any other
  declared host carries nothing of the host's. The `instance` record still holds no secret and no
  account.

  **What the component receives from the instance** is redacted first: every spelling of the
  secret the host sent (the secret, its form-encoding in either case of its escapes, its JSON
  escaping, the basic pair's base64) is replaced by
  `[redacted]` in the body as it streams, a header holding one is dropped, trailers and
  `content-length` are dropped, and a body the transport did not decode (a `content-encoding`
  other than `identity`) is refused with `HTTP-response-content-coding`. A transport error reaches
  the component as a bare error code with no text.

  **The transport** is the workspace's reqwest (rustls with the platform verifier, the system
  proxy), one client for the process, no redirects, no cookies, a 30 s connect timeout. Call
  deadlines and memory caps are #1204's.

  Pinned by `knobas-wasm-host`'s `tests/it/network.rs` (an undeclared host and a redirect to one
  refused with nothing reaching them, a declared non-instance host given no credential, header
  token and basic auth received by a local server, the secret in nothing the component receives),
  the `allow` and `redact` unit tests, `knobas-app`'s `tests/it/external_adapter_install.rs` (the
  two host-entry refusals, the hosts restored at load), and the install note's vitest.
