# 2026-11-14: knobas ships its PostgreSQL

**Found (the Windows pass, PR #2479):** a packaged ux10 and ux11 took 24.7 and 26.5 s from launch to "The database is
up, migrated and ready" on a fresh profile on Björn's PC. About 15 s of that was the app downloading PostgreSQL 18.6
from GitHub, which every machine did on its first start. With the network cut off the first start fails, and the boot
screen says `embedded postgres: Request failed after 3 retries` (reproduced on the Mac). Björn's work laptops are the
target, and a work laptop is where a proxy blocks that download.

**Ruled (Fable, under Björn's standing instruction: "Directness, performance, responsiveness over data scarcity"):**
every installer carries PostgreSQL, and the first start unpacks it. The download stays as the fallback of a build
that carries none.

## What ships

- **The upstream archive, as one file.** Each installer holds `postgresql/postgresql-18.6.0.tar.gz` in its
  resources. It is the unmodified build for its platform from `theseus-rs/postgresql-binaries`, the same bytes the app
  downloaded before.
- **Pinned by hash.** `crates/knobas-app/postgresql/archives.sha256` lists the SHA-256 of the three archives.
  `scripts/fetch-postgres.sh` downloads the one for its target before `tauri build` and refuses a file that does not
  match.
- **Checked in the build.** After `tauri build`, `release.yml` opens every bundle with its own format's tool and
  hashes the archive inside against the pin (`scripts/check-bundled-postgres.sh`, and `.ps1` on Windows). A bundle
  without it fails the run before anything is uploaded. Nothing else would notice, because such a bundle installs
  and runs and only downloads again.

| | where the archive sits | installer before | after |
|---|---|---|---|
| macOS `.dmg` | `knobas.app/Contents/Resources/postgresql/` | 37.2 MB | 50.1 MB (built here) |
| Windows `.msi` | `<install folder>\postgresql\` | 37.6 MB | about 91.6 MB |
| Windows `-setup.exe` | `<install folder>\postgresql\` | 29.1 MB | about 83.2 MB |
| Linux `.deb`, `.rpm` | `/usr/lib/knobas/postgresql/` | 41.6 MB | about 53.8 MB |
| Linux `.AppImage` | `usr/lib/knobas/postgresql/` in the image | 120.4 MB | about 132.6 MB |

*Before* is ux11's release. The archive is gzip already, so an installer adds its size whole: 13.1 MB on macOS,
54.1 MB on Windows, 12.2 MB on Linux. The Windows archive is four times the others: its file list is the EDB
distribution's, with ICU and StackBuilder's libraries in it. The three sizes after it are sums, not builds; the
Windows and Linux bundles are built by `release.yml` alone.

## What a start does

1. Bring-up asks whether `~/.theseus/postgresql/18.6.0` is there. On a machine that has run any knobas before, it is,
   and nothing changes.
2. If it is not and the build carries the archive, bring-up unpacks it there (`knobas_db::install::install_from`)
   with the extractor a download went through. That writes into a scratch folder beside the target and renames it
   into place, so an unpack cut short leaves no version folder and the next start unpacks again.
3. `EmbeddedDb::start` then finds PostgreSQL installed and downloads nothing.
4. A build without the archive (`just dev`, the tests) skips step 2 and downloads as before. So does a start whose
   unpack failed.

The boot screen's line says which of these is happening: *first run: unpacking and initialising PostgreSQL*,
*first run: initialising PostgreSQL*, or the old *downloading* line.

When the download is all that is left and it fails, the boot screen now says so in words a person can act on. It names
PostgreSQL and the version, says this copy carries none and that a release installer does, and keeps the upstream
reason at the end (`DbError::NotInstalled`).

## Why unpacked, and not run where the installer put it

Running the programs in place would save the unpack (0.27 s here, 1.6 s on Björn's PC, once per machine). I gave that
up for four reasons.

- **A server outlives the app.** A killed knobas leaves its PostgreSQL running, and the next start adopts it. If its
  programs lived in the install folder, an upgrade or an uninstall would meet files in use on Windows.
- **An AppImage's files vanish.** Its mount goes when the app exits, under an orphaned server.
- **macOS quarantines a downloaded `.app` with everything in it.** Files knobas writes itself carry no quarantine
  flag. ADR-0001 rejected a sidecar binary for the neighbouring reason, and this keeps its "extract to home".
- **One place on every platform.** `pg_dump`, `pg_restore`, the tests and adoption keep finding the programs where
  they always were, and an older version stays beside a newer one, which a major upgrade needs for its dump.

## Why not keep the download and harden it

The downloader already retries three times and goes through a proxy named in `HTTPS_PROXY` or in the system's
settings. What is left to fail is a machine with no route to GitHub, and no retry fixes that. Size is the only cost of carrying it, and the rule puts
performance ahead of size.

## Measured

On this Mac, a release bundle on a scratch `HOME`, from launch to `Ready`:

| | launch to `Ready` |
|---|---|
| before, with a network | 13.7 s and 18.7 s |
| before, network cut off | fails after 3.8 s |
| after, network cut off | 1.27, 1.30 and 1.39 s |

The 1.3 s is 0.2 s to bring-up, 0.27 s unpacking 44 MB, 0.6 s for `initdb`, the server's start and `create database`,
and 0.2 s for the 85 migrations and the engines. The network was cut with a dead proxy in the app's environment.

**Windows should land near 10 s**, down from 25. That is the ux10 breakdown less its 15 s of download: 1.2 s to the
profile folder, 1.6 s unpacking 156 MB, 4.7 s `initdb`, 1.5 s to start and migrate. The log now times the unpack,
`initdb` and the server's start, so the next Windows pass reads the split off the log file.

## Not taken: a data directory made at build time

A ready-made data directory in the bundle would replace `initdb`. On the Mac that is 0.6 s. On Windows it is the 4.7 s
that will be the largest part left.

I did not take it. A cluster made on the build machine carries that machine's time zone in its `postgresql.conf`
(the server reads its zone from there, not from the system), one superuser password for every install until
something rewrites it, and one system identifier for all of them. Each is fixable, and together they are a second
way for a cluster to come to exist, which every rule in `embedded.rs` about half-made and unfinished directories
would have to learn. It saves four seconds once per profile.

Two measurements on the PC would say whether to reopen it or do something smaller:

- `initdb` by hand with and without `--no-sync`. If the final sync is most of the 4.7 s, a deferred sync is a smaller
  change than a shipped cluster.
- Launch to the log's `knobas starting` line. Bring-up starts 1.2 s after launch on Windows and 0.2 s here. If the
  second is the WebView2 window, built before bring-up is spawned, the unpack could start ahead of it.

The 85 migrations from scratch cost 0.2 s and the background fills find nothing to fill, so neither is worth a change.

## Moving PostgreSQL to another version

Change `PG_VERSION_REQ` in `knobas-db` and the three lines of `archives.sha256`. A test in `knobas-app` fails while
the two disagree. The next release carries the new archive, and an updated app unpacks it beside the old version on
its first start. A major version still needs the dump and restore ADR-0001 describes.

## Licence

The archive holds PostgreSQL's licence and, on Windows, the third-party licence texts, and unpacks them beside the
programs. `postgresql/README.md` ships next to the archive with the PostgreSQL License in full and a note on OpenSSL.

## What reversing costs

Remove the `resources` entry from `tauri.conf.json` and the fetch and check steps from `release.yml`. The app then
finds no archive and downloads, as before; nothing in a profile changes either way.
