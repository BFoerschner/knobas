# 2026-11-24: The server stops however knobas ends

**Found (two agents, independently).** On Linux, stopping knobas left its embedded PostgreSQL running: the log said
"stopping the embedded postgres" and the postmaster was still there afterwards. This was after 7008753b, which turned
`SIGTERM`, `SIGINT` and `SIGHUP` into a quit.

**Reproduced** (the Linux debug build under Xvfb on the WSL box, a scratch `HOME`, `ux-polish` at 757adf32):

| How knobas ended | Exit | PostgreSQL |
|---|---|---|
| `SIGTERM`, `SIGINT`, `SIGHUP`, the window closed | 0 | stopped |
| `SIGTERM`, then the display ended 50 ms later | 1 | stopped (`pg_ctl stop` had gone out) |
| `SIGTERM`, then the display ended at once (the agents' stop scripts) | 1 | **left running**, 3 runs in 3 |
| the display ended (a logout) | 1 | **left running** |
| `SIGKILL` | 137 | **left running** |

**Cause.** The signal fix holds. A display that goes away ends the process from inside GTK: Xlib's I/O-error handler
calls `exit(1)` from whichever thread touches the dead connection first, and GDK's Wayland backend calls `_exit(1)`.
No in-process hook covers both, and either one can come before the quit or in the middle of it (here between the log
line and `pg_ctl stop`).

**Who meets it.** A person logging out of a Linux desktop: the X server or the compositor ends, and knobas with it,
before any signal. Where systemd stops the user's manager after the last session ends (`UserStopDelaySec`, 10 s),
the postmaster then gets its own `SIGTERM` and stops cleanly. Where it doesn't (lingering, another session of the same
user still open, a desktop without systemd), the postmaster stays up until the next start adopts it. Nothing breaks
and nothing is lost; a stray server holds its memory and port. Shutdown is fine either way: systemd sends `SIGTERM` to
every process, the postmaster included. Quitting from the window or by a signal was already fine.

**Fixed.** Every server knobas owns gets a supervisor (`knobas_db::supervisor`), the kind the test harness's server
has had since 34ad77a5: a `/bin/sh` in its own process group, ignoring hang-up, interrupt and terminate, whose stdin
is a pipe from knobas. The kernel closes the pipe however knobas ends; the supervisor then runs `pg_ctl stop -m fast`,
the quit's own stop, so the data is checkpointed and kept. It holds a copy of the ownership lock, so a start in that
instant is refused rather than adopting a server about to be stopped. A clean stop, or a handle dropped in-process,
kills it first. Mac and Linux; Windows has no `/bin/sh` and gets its log-off as window messages.

**The next start** still adopts a server left running, which now happens only when the supervisor died too (a
`SIGKILL` to both): checked in the real app, where the adopted server gets a supervisor of its own and stops on the
next close or `SIGKILL`.

**Checked.** `crates/knobas-db/tests/embedded/app_end.rs` ends a child that holds the app's server by `exit(1)` from
another thread and by `SIGKILL`; both failed before ("postmaster outlived the probe") and pass after, with the data
kept and the next start opening it. In the real Linux app every row above now stops PostgreSQL and leaves no
supervisor behind.
