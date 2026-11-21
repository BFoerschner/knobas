---
status: accepted
---

# knobas reaches its own PostgreSQL over a socket in its data folder as well as over TCP

Björn installed v3.2.9-ux27 on a Windows machine where he has no admin rights. knobas stopped at "cannot start its
database: embedded postgres: error communicating with database: ... (os error 10013)". The text is sqlx's I/O error
from the first connection of a fresh start, made by `postgresql_embedded`'s `database_exists` right after `pg_ctl start`
succeeded. A loopback connect refused with WSAEACCES (10013) is a Windows Filtering Platform filter: a firewall rule or
security software that blocks the program. Decided 2026-11-21 (Fable, xhigh, under Björn's standing instruction;
`docs/decisions/2026-11-21-knobas-starts-where-its-loopback-is-blocked.md`): **the server listens on `127.0.0.1` and on
a Unix-domain socket in its own data folder, and a bring-up connects over the first of the two that answers.**

- **The order.** Windows: the socket, then TCP. An AF_UNIX connect does not go through the TCP/IP stack, where those
  filters sit, and it is faster or equal there in every case measured. macOS and Linux: TCP, then the socket. On the Mac
  the socket is faster for small reads and about twice as slow for large answers. `KNOBAS_DB_LINK=socket|tcp` puts one
  first in any build.
- **One link per bring-up.** The pool, a sync run's own connections, `pg_dump` and `pg_restore` all use the route the
  bring-up connected on. libpq gets the socket's folder as `PGHOST`. On Windows a folder outside ASCII goes over TCP
  instead, because libpq passes it as ANSI bytes.
- **The socket is `unix_socket_directories = '.'`**, relative to the data folder, and `0700` on Unix. A relative folder
  avoids the path limit and the encoding on the server's side, and an absolute folder with a space in it fails on
  Windows. A data folder whose socket path would pass the limit (107 bytes on Windows and Linux, 103 on macOS), or that
  will not take the file, gets no socket: the start is TCP alone, as before, and the report says why. A server that
  cannot make its socket file is started once more without it.
- **sqlx reaches AF_UNIX on Windows through a patched `sqlx-core`** in `vendor/sqlx-core` (`[patch.crates-io]`, 25
  lines, `vendor/sqlx-core/KNOBAS.md`). tokio has no `UnixStream` on Windows, but its reactor polls any Winsock socket,
  so the connected socket is handed to it as a `TcpStream`.
- **A lock's liveness reads the socket first.** A socket in our own data folder is our server's, which a port is not.
  A loopback probe refused for permissions counts as an answer, and `adopt` does not take a refused-for-permissions TCP
  error for a stranger: it keeps the lock and reports that it could not connect.
- **A failure explains itself.** Every route tried goes on the start's report with its address or socket path and the
  OS error, with the step reached and what the data folder held. The boot screen names a block in plain words, says
  what was tried and which two programs an administrator can allow, and *Copy details* carries the report.

## Rejected

- **Named pipes.** PostgreSQL listens on TCP and Unix-domain sockets only.
- **More TCP addresses** (`::1`, `localhost`): a second listen address brings back the half-bound start #1775 removed,
  and a filter that blocks the program blocks them all.
- **The `0.0.0.0` probe that raised the Windows firewall question.** The question is about an inbound listener, while
  the failure is an outbound connect. Without admin rights an answer can only write block rules.
- **tokio-postgres for this link.** The app is sqlx throughout.
- **The socket first on macOS and Linux.** It would make large answers slower on the Mac. Linux was not measured.

## Consequences

- The vendored `sqlx-core` is remade on every sqlx upgrade. Cargo refuses to resolve a newer sqlx until it is, so the
  upgrade fails loudly. The Windows half also depends on tokio's reactor accepting an AF_UNIX socket. A break shows at
  connect time and the start falls back to TCP. A Windows socket check is owed at each tokio bump, since no Windows CI
  runs the tests.
- On macOS and Linux the server's socket moves from PostgreSQL's default `/tmp/.s.PGSQL.<port>` into the data folder,
  which is `0700`. The repository's scripts all pass `-h 127.0.0.1`.
- A start on Windows pays 7 to 9 ms for the socket (51 to 54 ms from start to ready without it, 58 to 63 with it). The
  usability probe costs 1.6 to 1.8 ms. Neither shows on the Mac.
- Security software that also blocks the socket file still stops the start. The screen then lists both routes.
- A copy of a data folder a crashed server left can carry a stale socket file. PostgreSQL replaces it at its next start.
- ADR-0001's "TCP localhost only" consequence is amended by this one.
