# 2026-11-21: knobas starts where its own loopback connection is blocked (for ux28)

Björn installed v3.2.9-ux27 on a Windows machine where he has no admin rights. knobas showed "knobas cannot start its
database: embedded postgres: error communicating with database: Der Zugriff auf einen Socket war aufgrund der
Zugriffsrechte des Sockets unzulässig. (os error 10013)". Earlier builds had made Windows ask an "allow this app?"
question there. ux27 asked nothing and failed at once. He cannot change firewall rules or security software there, and
he has no logs from that machine.

## What the error says

- **The step.** The text is sqlx's I/O error, wrapped by `postgresql_embedded`. In a start, the only call of that
  crate that connects is `database_exists`, right after `pg_ctl start`. So this was a fresh start (an adoption connects
  with knobas's own code and words its error differently). The server had started and bound `127.0.0.1:<port>`.
  knobas's first client connect to it was refused.
- **The cause.** WSAEACCES (10013) on a loopback connect is a Windows Filtering Platform filter: a firewall rule or
  security software that blocks the program. A port held by someone else gives 10013 on a bind, never on a connect, and
  a dead port gives 10061.
- **Why ux27 asked nothing.** `loopback_port` used to bind `0.0.0.0`, and that inbound listener raised the Windows
  question (2026-11-11). It now binds `127.0.0.1`. An inbound "allow" cannot unblock an outbound connect, so whether
  an older build ever got past the boot screen on that machine is an open question for Björn. It does not change what
  follows.

## Taken (Fable, xhigh; ADR-0053)

**The server listens on `127.0.0.1` and on a Unix-domain socket in its own data folder, and a bring-up connects over
the first of the two that answers.** On Windows the socket comes first: an AF_UNIX connect does not pass through the
TCP/IP stack where those filters sit, and it is faster there. On macOS and Linux TCP comes first, as before.
`KNOBAS_DB_LINK=socket|tcp` puts one first in any build. The whole of the ruling, the options it turned down and the
consequences are in ADR-0053. sqlx 0.9 reaches AF_UNIX on Windows through a 25-line patch to `sqlx-core`, vendored in
`vendor/sqlx-core` (`KNOBAS.md` there says how to remake it on an upgrade).

**A failure explains itself.** Every bring-up keeps a report (`knobas_db::report::StartReport`): the step it reached
(preparing, installing, initdb, server start, bind, client connect, first query), what the data folder held before it
began (no folder, no `postmaster.pid`, or a lock with its pid, port, socket folder and listen address, and whether the
server was adopted), and every route tried with its address or socket path and the OS error, numbered. A route not
offered (a socket path past the limit, a server that could not make its socket file) is listed with why.

- **The boot screen**, when a route was refused for permissions and none worked after it: "Windows or security
  software on this computer blocked knobas from reaching its own database.", then "knobas tried" with each route, then
  "An administrator can allow these programs:" with the full paths of `knobas-app.exe` and the `postgres.exe` it
  starts. The raw message stays under them. *Retry* stays, and *Copy details* is new on every failure.
- ***Copy details* and the log** carry the version and profile, the Windows build (named from the build number, since
  the registry says "Windows 10" on Windows 11), the step, what the folder held, every route with its error, both
  programs' paths, Security Center's antivirus and firewall products with on or off, Windows Firewall's profiles with
  their outbound default, and the block rules that name knobas or PostgreSQL. All of it is read without admin rights,
  by one `powershell` child only after a failure. The screen shows the failure at once with what the process knows and
  adds the machine's facts when they arrive (1.85 s on Björn's PC from cold). A forced failure on his PC read:

```
knobas 3.2.8 · profile default · Windows 11 Pro 25H2 build 26200.9974
failed at: knobas connecting to its server
tried:
  tcp 127.0.0.1:1: error communicating with database: no answer from the database within 5s
knobas: C:\Users\bjfoer\AppData\Local\Temp\knobas-ux28\app\PFiles\knobas\knobas-app.exe
security: Windows Defender (on)
Windows Firewall: Domain on, outbound allowed; Private on, outbound allowed; Public on, outbound allowed
block rule (inbound) for C:\users\bjfoer\appdata\local\temp\knobas-perf\app\pfiles\knobas\knobas-app.exe
block rule (inbound) for C:\users\bjfoer\appdata\local\temp\knobas-perf\app-winperf1\pfiles\knobas\knobas-app.exe
error: postgres connection: error communicating with database: no answer from the database within 5s
```

  (That run pointed `KNOBAS_DB_URL` at a dead port, so it has no data folder line. The two block rules are leftovers of
  old scratch builds' prompts, written as one TCP and one UDP rule each; the details list each program once.)

**A lock is never cleared on a refused probe.** A loopback probe refused for permissions counts as an answer, and an
adoption whose TCP connect is refused for permissions keeps the lock and reports that it could not connect. Before,
a silent port cleared the lock, and under a block every port is silent.

## Numbers

Björn asked whether a socket file costs speed. On Windows it saves time; on the Mac it is mixed, which is why the Mac
keeps TCP first.

| Query round trips (Fable's spike, release, three runs) | PC TCP | PC socket | Mac TCP | Mac socket |
|---|---|---|---|---|
| `select 1` x 10,000, one connection, mean | 42 to 43 µs | 22 to 24 µs | 25 to 27 µs | 10 to 13 µs |
| Pool of 5, 10,000 `select 1`, per read | 38 to 42 µs | 26.5 µs | 37 µs | 17 µs |
| One 100 kB answer x 1,000, mean | 137 to 182 µs | 66 to 134 µs | 51 to 61 µs | 90 to 125 µs |
| 5,000 rows x 1,000, mean | 1.68 to 1.74 ms | 1.66 to 1.70 ms | 1.32 to 1.50 ms | 1.28 to 1.49 ms |
| Connect and auth x 200, median | 34.2 ms | 34.1 ms | 3.9 ms | 3.6 ms |

**Starts in the real app on Björn's PC** (this build cross-compiled, in the ux27 installer's own layout, a scratch
profile; three warm starts each, from the first log line to `ready`): socket 629, 666 and 655 ms; the same build on
TCP 650, 689 and 770 ms; the ux27 build 661, 648 and 636 ms. The database part alone: socket 423 to 442 ms, TCP 433 to
467, ux27 418 to 435. A first start on a fresh profile (unpack and initdb) reached `ready` 5.6 s after its first log
line, the database part 3,146 ms, on the socket.

**Page opens.** On the PC, in knobas's own window over the DevTools protocol, the demo profile (38 items): Tickets,
Code and Time drew their rows in 8.8 to 10.6 ms on the socket and 9.0 to 10.5 ms on TCP, first visit and return, two
runs each; every read they made took 0.6 to 4 ms either way. On the Mac, the windowless backend on a copy of the
saved scale home (39,770 items), one run each at load 5, six nav pages: first visits 16.7 to 28.5 ms on TCP and 24.7
to 28.9 on the socket, returns 24.9 to 30.9 and 29.0 to 36.9. The difference sits inside the run-to-run noise of a
loaded Mac, and the Mac keeps TCP.

## Proved how

- **No real block.** Adding a WFP filter or a firewall rule needs admin rights, and an AppContainer (which would give
  a real 10013 on loopback) needs a registered per-user profile, which writes to Björn's profile. Neither was done.
- **The seam.** `seam::refuse_tcp_connects` and `refuse_socket_connects` refuse a data folder's routes with 10013, and
  the probe of a lock's port honours them. `tests/embedded/link.rs`: TCP refused and the start ends on the socket,
  with `pg_dump` over it; both refused and the error names both; a folder too deep for a socket starts on TCP and says
  why; a server that cannot make its socket starts again without one; an orphan adopted over its socket while TCP is
  refused; a refused TCP answer is no stranger and the lock stays. `knobas-app`'s `bring_up.rs` drives the app's own
  bring-up with both refused and reads the screen's state and *Copy details*.
- **The real app on Windows** started on the socket (`link="socket"` in its log, `postmaster.pid` line 5 `.`), quit
  cleanly (no server, no socket file left), and on TCP when told to. The demo profile under the scratch folder's
  longer path took TCP with "the path is 112 bytes, past the 107 a socket path may have": the scratch home adds 27
  characters. On Björn's own machine the default profile's socket path is 73 bytes and the demo profile's 78.
- **The real app on the Mac** on a fresh copy of the saved scale home (made at migration 85, migrated on start): TCP by
  default; killed by signal, then adopted over the socket with the socket put first (eight connections on
  `.s.PGSQL.<port>`, none over TCP), and adopted over TCP by default (six TCP connections). No migration changed.
