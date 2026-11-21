# sqlx-core 0.9.0, patched for knobas (ux28)

This is the published `sqlx-core` 0.9.0 (MIT OR Apache-2.0, see `LICENSE-MIT`
and `LICENSE-APACHE`) with one change, so that knobas can reach its own
embedded PostgreSQL over a Unix-domain socket on Windows. A firewall rule or
security software that blocks the app's loopback TCP connect (os error 10013,
WSAEACCES) does not see an AF_UNIX connect. The decision is ADR-0053.

## The change

- `src/net/socket/mod.rs`: `connect_uds` gains a Windows branch. Windows has
  AF_UNIX stream sockets since Windows 10 1803; tokio has no `UnixStream`
  there, but its reactor polls any Winsock socket, so the socket is connected
  with `socket2` (blocking: an AF_UNIX connect completes or is refused at
  once) and handed to tokio as a `TcpStream`. Every call sqlx makes on it is
  the same call on an AF_UNIX stream socket.
- `Cargo.toml`: `socket2` 0.6 as a Windows-only dependency (already in the
  tree through tokio), and `deprecated = "allow"`, since a path dependency is
  not lint-capped the way a registry one is.

Nothing else differs from the registry copy (`Cargo.toml.orig`, `Cargo.lock`,
`.cargo-ok` and `.cargo_vcs_info.json` were left out).

## On an upgrade

Cargo refuses to resolve a newer `sqlx` while this copy is 0.9.0, so an
upgrade fails loudly. Copy the new `sqlx-core` from the registry here, apply
the same change, and run the Windows socket test (a scratch start on Windows
whose log says `link=socket`). A tokio or mio upgrade can break the reactor
half: knobas then falls back to TCP at connect time, and the Windows check is
owed at each tokio bump too.
