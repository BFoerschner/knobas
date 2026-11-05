# How knobas works

Adapters sync each source into one embedded PostgreSQL. Lists and search read that copy, so they do not wait on a server. File contents, diffs and build logs are read from the source when you open them, and kept only in memory. The amber path is the only route that changes anything at a source.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="diagrams/system-map-dark.svg">
  <img alt="knobas system map. Sources sync through adapters, and the sync scheduler writes into the mirror in one embedded Postgres, which search and the app read. The app also reads files, diffs and logs live through the adapters. In amber, the only outbound path: the app submits a write to the write queue, the flush hands it to the adapter, and the adapter sends it to the item's own source." src="diagrams/system-map-light.svg">
</picture>

## Writes

Everything you do from knobas takes that path: comments, status changes, worklogs, merges, re-runs and page edits. A write is stored and returns at once, and the flush loop is the only code that calls `Source::write`. `crates/knobas-sync/tests/it/write_choke_point.rs` fails on a second call site.

Before sending, the flush compares the mirror's copy of the target with the snapshot taken when the write was queued. A held or refused write waits for a person. Nothing times out or applies itself.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="diagrams/write-queue-dark.svg">
  <img alt="Write queue states. A write starts pending and becomes sent if its target is unchanged. It becomes held if the target changed, the source was turned off, or it was sent with no answer, and refused if the source says no. Held and refused writes go back to pending only when a person acts, and a person can discard any write that has not been sent." src="diagrams/write-queue-light.svg">
</picture>

## Credentials

Credentials stay in the OS keychain, and none reaches Postgres or crosses into the webview.

## Adapters

Jira and Confluence Data Center, Gitea and Forgejo, TeamCity and Uptime Kuma have built-in adapters. Anything else can be an external adapter, a WebAssembly component installed from a GitHub release. It runs under wasmtime with a deadline and memory cap on every call, gets no disk, never sees the credential and can only reach the hosts its manifest declares. knobas shows you those hosts at install and never updates an adapter on its own.

[`knobas-adapter-sdk`](../crates/knobas-adapter-sdk) is the Rust SDK, and [ADR-0028](adr/0028-an-external-adapter-is-a-sandboxed-wasm-component-installed-from-a-release.md) has the reasoning.

## Where to go next

[development.md](development.md) covers the gate, profiles, releases, the crates and the frontend rules. [`CONTEXT.md`](../CONTEXT.md) has the vocabulary, and [`adr/`](adr/) the decisions.
