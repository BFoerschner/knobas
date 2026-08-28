---
status: accepted
---

# Embedded PostgreSQL via `postgresql_embedded`

knobas needs one local database holding both the synced mirror and everything knobas owns, with real full-text search and export that comes free (`pg_dump`/`pg_restore`). Decided 2026-08-24 (Claude, under delegation), ratified 2026-08-27 (Björn): PostgreSQL, pinned to 18.6, embedded via the `postgresql_embedded` crate — downloaded once on first run, TCP on 127.0.0.1 only, never a user-installed prerequisite.

## Considered options

- **Tauri sidecar binary** — rejected: macOS notarization of external binaries is a known open Tauri bug; the crate's extract-to-home approach sidesteps it entirely.
- **User-installed Postgres as a prerequisite** — rejected: "install Postgres first" is not an acceptable first-run for a desktop app. A settings field still accepts an existing Postgres URL for anyone who already runs one.

## Consequences

- TCP localhost only — the macOS Unix-socket path limit (103 bytes) breaks PG sockets under `~/Library/Application Support`.
- PG major upgrades are ours to sequence (dump → initdb → restore at startup on version change); the pin stays exact (`=18.6.0`).
