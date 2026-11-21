# 2026-11-11: The database's port on the loopback, one set of checksums, and 120 Hz on the Mac

**Asked (Björn, on PR #2479, from the two smoothness reports of 2026-11-11):** stop the Windows firewall prompt at the first start and keep the database on localhost; make a database move between a Windows build and a Mac build; lift WKWebView's 60 fps cap on 120 Hz displays, behind a setting.

## The database's port

**Found.** `postgresql_embedded` 0.21 picks the port of a start on port 0 with `TcpListener::bind(("0.0.0.0", 0))` (`postgresql.rs:294`), and knobas started it on port 0. That is a listening socket on every interface inside `knobas-app`, for the moment it takes to read its number, and it is what Windows Defender asks about: every Windows user got the prompt at the first start, and on Björn's PC two inbound block rules per program path were written.

**Did a shipped version listen beyond the loopback?** The server, no. Every tag from `v0.1.0` to `v3.2.9-winperf1` used `postgresql_embedded` 0.21.0, which sets no `listen_addresses`. Before #1794 (first in `v3.0.0-rc1`) knobas set none either, so the server took PostgreSQL's default, `localhost`: `127.0.0.1` and `::1`, both loopback. Since #1794 it is `127.0.0.1` alone. Every shipped version did open the momentary wildcard socket above: it was bound and listening on all interfaces for microseconds, never accepted a connection, and was closed before the server started.

**Taken.** knobas picks the port itself with a bind on `127.0.0.1` (`embedded::loopback_port`) just before the handle is made, and `start_server` refuses a handle that still has port 0, so `postgresql_embedded`'s wildcard bind cannot run. `a_start_without_a_picked_port_is_refused_before_anything_listens` pins the refusal; `a_started_server_listens_on_the_ipv4_loopback_alone` now also reads `listen_addresses` back from the running server. Every other socket in `crates/` was checked: the app opens no other listener; tests, `headless_ipc` and the in-process mocks bind `127.0.0.1`. `mockd`'s binary, the deprecated container entry point, defaulted to `0.0.0.0`; it defaults to `127.0.0.1` now, and a container passes `--bind 0.0.0.0`.

**Reversing it** is deleting `loopback_port`'s call in `start_managed` and the guard in `start_server`.

## Migration checksums and line endings

**Found.** sqlx checksums each migration file's bytes. The Windows runner's git checks text out with CRLF (`core.autocrlf`), so a Windows build embedded CRLF migrations and its databases recorded CRLF checksums, and a Mac or Linux build refused them ("migration 1 was previously applied but has been modified"), and the reverse.

**Taken.**

- `.gitattributes`: `* text=auto eol=lf`, and the migrations and `migrations.sha384` named LF, so every checkout of every text file is LF, whatever the machine's `core.autocrlf`. Every file in the index was LF already, so nothing renormalised. `windows.yml` now checks the runner's checkout and runs the pinned-checksum test there.
- `migrate::run` first rewrites, in one transaction, every recorded checksum that is an embedded migration's SHA-384 with all line endings LF or all CRLF, to the embedded one (`adopt_other_line_endings`). A Windows database opens under the fixed build, and a database moves either way between builds. A migration changed in any other way is still refused. Pinned by the three `line_endings` tests in `tests/schema.rs`.

**Reversing it** is the `adopt_other_line_endings` call in `run_with`; `.gitattributes` should stay.

## Smooth scrolling on 120 Hz displays

**Found.** The Mac report: WKWebView draws page content at 60 fps on a 120 Hz panel because WebKit's internal feature `PreferPageRenderingUpdatesNear60FPSEnabled` is on by default; no public API turns it off. Turned off through the private `+[WKPreferences _features]` and `-[WKPreferences _setEnabled:forFeature:]`, the Tauri window ran at 119.9 fps.

**Taken.** Settings › Performance has *Smooth scrolling on 120 Hz displays*, on macOS only (`frame_rate.rs`; `smooth_scrolling` answers `null` elsewhere and no switch is drawn). It is applied once the database is up and at once when changed. It fails soft: a missing selector or feature leaves WebKit's 60, with one `debug` line. It is private API: it would not pass Mac App Store review, which knobas does not go through.

**The default is on.** Measured on the M2 Pro MacBook Pro, 120 Hz panel, mains power, without `sudo` (so not with `powermetrics`): a bare WKWebView window, 900 x 700, scrolling a 4,000-row list by script at 3,000 px/s, 20 s measured after 4 s of warm-up, two runs each:

| | rAF | CPU of the page's own processes (app, WebContent, GPU, Networking) | WindowServer CPU |
|---|---|---|---|
| capped | 59.8, 59.9 fps | 1.62, 1.61 s | 6, 6 s |
| uncapped | 120.0, 119.7 fps | 2.19, 2.66 s | 8, 9 s |
| capped, page still | | 0.01 s | 2 s |
| uncapped, page still | | 0.01 s | 2 s |

So lifting the cap costs about 3 to 4 more CPU seconds per 20 s while a list moves, about 15 to 20% of one core, and nothing while the window is still. The battery's own power telemetry (`SystemLoad`) could not tell the two apart: other agents' builds moved it between 21 and 33 W across the runs. On battery the same extra work drains it; the switch is one click away for anyone who would rather have the hours.

The Mac report also found the windowed Tickets list losing scroll frames when uncapped (70 to 91 shown fps, against about 100 capped), because it changes rows on most frames. The coarse-step `Windowed` change in the same PR is what that waits on; without it, default on makes that one list worse.

**Reversing it** is `frame_rate::DEFAULT = false`, or the setting itself: its key is `shell.smooth_scrolling`.
