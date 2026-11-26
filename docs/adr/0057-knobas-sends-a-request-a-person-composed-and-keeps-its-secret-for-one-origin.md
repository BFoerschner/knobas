---
status: accepted
---

# knobas sends a request a person composed, once, and keeps its secret for one origin

Amends [ADR-0016](0016-a-spawned-command-takes-no-argument-from-the-mirror.md) and stands beside [ADR-0029](0029-the-shell-draws-a-role-never-a-source.md) and [ADR-0032](0032-the-app-keeps-live-reads-in-memory-keyed-by-what-they-stand-at.md). Björn, on PR #2479: "It should be really nice to explore api's with it, possibly even sending requests out from there … Also, an open in bruno would be nice … have it just as a normal open in application", and on credentials: "I'd just type it into the openapi viewer page itself. not using stored credentials." Until now knobas spoke only to its sources, through their adapters, and the OpenAPI view said three times that nothing in it calls a server. Decided 2026-11-22 (Fable, xhigh, under Björn's standing instruction, amended the same day by Björn's answers; `docs/decisions/2026-11-22-an-api-spec-is-explored-sent-from-and-opened-in-a-client.md`): **knobas may send one HTTP request a person composed in the API explorer to a server that is not a source, from the app process and never from the web view, and it attaches a secret only to the origin that secret was typed for. No such secret is ever stored.**

- **A server is not a source.** It fills no role, is never synced, and nothing it answers reaches the mirror, a held read or a log. The explorer is a file's renderer and asks an adapter for nothing but the file. The sender names no source and uses no source's credential.
- **One attempt.** No retry for any method, a connect and a total limit, `http` and `https` only, at most 64 MiB of answer. It is a second, small client in `knobas-http` (`person`) that shares the adapters' TLS and proxy setup and none of their retry, quota or fault mapping.
- **Nothing is sent unasked.** A request goes out on a press. A method other than `GET`, `HEAD` and `OPTIONS` to a host that is not this computer needs a yes that names the method and the host, asked in place once a host until knobas quits, and the app process refuses it without one.
- **A secret has an origin, and lives in memory only.** It is typed on the explorer's page and handed to the app process once, where it is held, keyed by origin and security scheme, until knobas quits: never in the keychain, a settings row, the web view's state, a log or a file. It is attached only to a request whose scheme, host and port are its own. A redirect to another origin is not followed; on the same origin it is followed for `GET` and `HEAD` only.
- **Certificates are verified, always**, by the platform's verifier, plus a file of roots a saved server may name for itself.
- **Answers are live reads kept in memory** (ADR-0032), bounded by bytes, dropped at quit, in no backup and no share export. The saved servers (a name, an address, a CA file; no secret) are one `knobas.setting` row a backup carries and a share export does not.
- ***Open in…* may run an application on a spec** (ADR-0016, as ADR-0044 amended it for a folder renderer): the command is the person's, there is no shell, and its only arguments from knobas are paths to a copy of the spec and to a collection of requests, both written by knobas into a folder in the profile. The collection holds no secret. The folder is written again when the spec's content changes (by its hash) and removed after 30 days unopened.

## Rejected

- **Sending from the web view.** CORS, no control of `Host` or cookies, the web view's certificates and proxy, and the secret in the page.
- **The adapters' client.** It retries and waits on a sync's quota.
- **A source's credential for a server at the source's address.** A spec in any repository could then send with an adapter's token, past the write queue and the grants.
- **A stored secret for a saved server** (the design's keychain item). Björn: typed on the page, not stored.
- **A saved history.** A request's ids and an answer's data would outlive the credential that read them, and nothing can tell a secret in a body from any other string.
- **Following redirects as a browser does**, and **a switch that skips certificate checks.**
- **Bundling Bruno's converter** (3.9 MB of Node code with stubs) or shelling out to its CLI.

## Consequences

- One client in `knobas-http`, one command group (`api_*`), one settings row, no keychain item, one profile folder, one more command on an application and one more `ObjectKind`. No migration, no SPI method, no `WriteOp`, no WIT change.
- knobas can now reach any address a person's computer can. The guard is the person: the request's line is drawn whole before it is sent.
- The witness is `crates/knobas-app/tests/it/api_send.rs` against servers of the test's own (one request a send, the asking rule, the origin rule, the redirect rule, the cap, Stop), `crates/knobas-app/src/api/*` (this computer, the asking rule, the origin of a secret, the bounds of what is kept, a secret never printed), `openapi-request.test.ts` (no writer ever prints a secret), and `openapi-bru.test.ts` (Bruno's parser reads what knobas wrote).
