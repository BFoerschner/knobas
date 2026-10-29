---
status: accepted
---

# Embedded PostgreSQL via `postgresql_embedded`

knobas needs one local database for the synced mirror and everything knobas owns, with real full-text search and a free export path (`pg_dump`/`pg_restore`). Decided 2026-08-24 (Claude, under delegation), ratified 2026-08-27 (Björn): **PostgreSQL, pinned to 18.6, embedded through the `postgresql_embedded` crate.** It downloads once on first run, listens on 127.0.0.1 over TCP, and is never a prerequisite the user installs.

## Rejected

- Tauri sidecar binary. macOS notarization of external binaries is an open Tauri bug; extract-to-home avoids it.
- User-installed Postgres. Not an acceptable first run for a desktop app. A settings field still accepts an existing Postgres URL.

## Consequences

- TCP localhost only. The macOS Unix-socket path limit (103 bytes) breaks sockets under `~/Library/Application Support`.
- The pin stays exact (`=18.6.0`). A major upgrade is ours to sequence: dump, initdb, restore at startup on version change.
