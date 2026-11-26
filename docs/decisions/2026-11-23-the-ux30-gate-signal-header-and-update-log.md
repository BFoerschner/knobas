# The ux30 end gate: a stop signal quits, the entry header wraps, the updater logs (2026-11-23)

Three small fixes from the ux30 end gate. Each has a test that failed before.

## A stop signal is a quit

`SIGTERM` to `knobas-app` ended the process and left its embedded PostgreSQL running, on Linux and on the Mac. The exit
handler stops the database on Tauri's `ExitRequested` and `Exit`, and a signal raises neither. Nothing reaped the server
until the next start, which adopts it as a warm start (`knobas_db::EmbeddedDb::start`). So after a logout, a package
manager's stop or a `kill`, the postmaster stayed: its port held, its memory used and its data directory in use, with
no knobas running.

**Now:** `stop_signals::listen` answers the first `SIGTERM`, `SIGINT` or `SIGHUP` with `AppHandle::exit(0)`, the same
exit a quit takes. The whole exit handler runs, the database last. A second signal during that quit ends the process at
once, so a hung quit can still be ended. It leaves the server for the next start to adopt, as before. The windowless
backend (`examples/headless_ipc.rs`) listens too and runs the same handler. Windows sends a GUI process no signal, so
nothing changes there. `tests/it/stop_signals.rs` raises a real `SIGTERM` and asserts the profile's own server is gone.
Without the listener, the signal ended the test binary and left the postmaster behind (seen in the before run).

## Passwords' entry header wraps instead of cutting

In a 1100 px window, the window's smallest, the entry panel is about 430 px wide. The header's actions (*Change
password…*, *Edit E*, *Open web interface W*) were one `flex: none` group about 410 px wide, so the last button was
cut by 22 px at the panel's edge, and the entry column scrolled sideways. At 960x540@2 it was cut by 92 px.

**Now:** the group may shrink and wrap. Where the actions fit beside the title, nothing changes. Where they don't, the
group goes under the title, left-aligned with it, and its buttons wrap whole. A button is never cut or shortened. In the
same narrow panel, *Notes* and *Attachments* had been pushed into the actions column (x 713 against 660 for every other
value). They now span the row as the other values do. `long-paths.mjs` gained a `whole` rule (every button under a
selector is drawn whole, sideways, inside the window and every box that clips it). Its two Passwords screens, an entry
and a folder, run at its four sizes plus 1100x800 and 1440x868. Before: 3 of 12 missed. After: 12 of 12.

## The updater logs its download, its signature check and the hand-off

The ux30 boot check found no log line for an update's download or its signature check. Now there are info lines for the
download's start (version, file, expected size) and end (bytes landed, ms), the signature's result when the download
lands and again before the install, and the hand-off to the quit (from, to, install kind, exit or restart). A failure
is a warning line. No line holds a URL: GitHub sends a release download on to a signed address, and an endpoint may
carry a token. A transfer error's text keeps only an address's scheme and host. `update/tests.rs` captures the lines
(over a local server whose address carries a token) and asserts each one, and that no address is written.
