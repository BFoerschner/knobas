# 2026-11-21: a deep profile keeps its socket, a killed knobas is adopted at once, and a failed start says what PostgreSQL said

From the Windows pass on v3.2.9-ux28 (Björn's PC, knobas's own window, scratch profiles). Decided by Fable (xhigh)
under Björn's standing instruction; ADR-0053 is amended to match.

## What the pass found

1. **A profile 41 bytes deeper than Björn's got no socket.** Three fresh starts in a deep scratch folder each logged
   `no socket in this data folder: TCP alone reason=the path is 114 bytes, past the 107 a socket path may have` and
   ran on TCP alone. At a shorter path every start logged `link="socket"`. The data folder sits in
   `%APPDATA%\dev.knobas.desktop\db\data`, so any `%APPDATA%` longer than about 65 bytes loses the socket: a long user
   name or a redirected AppData. That is the managed machine the socket exists for. On such a machine a loopback
   block (os error 10013) would stop the start again, as it did in ux27.
2. **A failed start did not say why.** With `shared_buffers = nonsense_value` in a scratch profile's
   `postgresql.conf`, the screen and *Copy details* said only `configuration file ".../postgresql.conf" contains
   errors`, PostgreSQL's `FATAL` summary. The cause sat in the line before it in the data folder's `start.log`:
   `invalid value for parameter "shared_buffers": "nonsense_value"`.

3. **After a hard kill, the next start waited 34 s.** Every start after a killed knobas tried a second server against the
   orphan's lock first. That can only fail, and on Windows `pg_ctl start` took about 33 s to say so before the
   adoption: 34.0 s from launch to the wizard on ux28, 34.0 s on ux26, at both a deep and a normal path.

## What changed

- **A lock whose server answers is adopted at once.** Before a server is started, `postmaster.pid` is read afresh
  (the stale-lock retry comes here after clearing it), and when its port's server answers (`server_answers`: the
  socket in either folder, then the loopback) the start goes straight to `adopt`, which keeps every rule it had for a
  stranger or a refusal. In the window: 0.87 s to the wizard after a kill at the deep path, 0.91 s at a normal one.
  This one is not Fable's: it keeps behaviour and only skips a start that cannot work.

- **The short folder (Windows only).** When the data folder cannot hold the socket, it goes to
  `%LOCALAPPDATA%\knobas\<key>`: one per data folder, so the demo and the default profile never share one. The key is
  the first 8 hex digits of FNV-1a 64 over the data folder's path in lower case, written in `socket.rs`, never
  `DefaultHasher`. The folder is passed to the server as an absolute path with forward slashes. It is refused without
  probing when its path has a character `pg_ctl -o` cannot carry (anything but ASCII letters, digits and
  `\ / : . _ - ~`), or when `LOCALAPPDATA` is unset or relative. Then the start is TCP alone, and the report names both
  folders as not tried. Local AppData's access list admits only the user, so a socket there is still our server's.
  `%TEMP%` is longer and gets cleaned, and `C:\ProgramData` lets any local user make the folder first. The path is
  `%LOCALAPPDATA%` plus 31 bytes. On Björn's PC that comes to 60 bytes.
- **Adoption and liveness** look for `.s.PGSQL.<port>` in the data folder, then in the short folder (`socket::find`).
  They never take line 5 of `postmaster.pid`, which a data folder copied from another machine carries from there.
- **Tidying.** After `ready`, off the start's path, the socket files a killed server left in this server's socket
  folder go (`socket::sweep`): every start picks a new port, so each kill leaves a pair. A clean stop removes the short
  folder and `%LOCALAPPDATA%\knobas`, each only when empty.
- **What PostgreSQL said.** `StartReport::server_said` holds the failed start's own log lines, from each line's
  severity label on (the timestamp and pid go). Blank lines are dropped and whitespace collapsed. It keeps the last 20
  lines within 2,000 bytes, with a first line saying how many earlier ones were left out, and one line stops at 400
  bytes. A start that worked clears it. *Copy details* prints it as `postgres said:` between `tried:` and the programs.
  `DbState::Failed` carries it as `said`, and the boot screen draws the last six under the message as *PostgreSQL
  said*, in the screen's existing list style.

## Not changed

- macOS and Linux get no second folder: TCP is first there, and the only short place is a shared `/tmp`.
- The database itself stays in Roaming AppData. With roaming profiles or a redirected AppData the whole cluster syncs
  to, or lives on, a file server. The short folder gives that machine a socket, but it does not make PostgreSQL on SMB
  sound. Moving the data folder to Local AppData needs its own ruling from Björn.

## Tests

`socket.rs`: the key is stable, ignores case, tells folders apart and is 8 hex digits; the space refusal; the folders'
order; the sweep; the tidy. `link.rs`, through `seam::fall_back_to` (no Windows CI exists): a deep root ends on the
short folder's socket with TCP refused, its report naming the data folder as not tried, and `pg_dump` runs over it; an
orphan in the short folder is adopted and a killed server's files are swept; two deep roots at once reach their own
servers; a short folder with a space leaves TCP alone with two not-tried lines; a clean stop leaves no short folder.
`embedded.rs`: `server_said` keeps this start's lines from their labels, and its caps. `start_reason.rs`: a bad
`shared_buffers` stops at the server's start with the cause before the `FATAL`, and a good start clears it.
`link.rs` also: an answering orphan is adopted with no start tried (red before: the report held `FATAL: lock file
"postmaster.pid" already exists`). `boot_report.rs`, `bring_up.rs` and `Booting.test.svelte.ts`: the block's place in *Copy details*, the state's
`said`, and the screen's six lines.
