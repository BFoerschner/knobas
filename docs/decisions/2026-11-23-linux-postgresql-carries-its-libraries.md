# On Linux, knobas's PostgreSQL carries its own libraries (2026-11-23)

**Found (the ux30 end gate, mutants run 45221186058):** every Rust test that starts the embedded PostgreSQL failed on
GitHub's `ubuntu-latest`, which became Ubuntu 26.04 (image `ubuntu-26.04`, 20261116). The reason:
`~/.theseus/postgresql/18.6.0/bin/postgres: error while loading shared libraries: libxml2.so.2`, and then initdb
failed. theseus's 18.6.0 Linux build links the host's `libxml2.so.2`, `libssl.so.3`, `libcrypto.so.3`,
`libgssapi_krb5.so.2`, zstd, lz4 and zlib. Ubuntu 26.04 and Fedora 45 ship libxml2 2.14 as `libxml2.so.16` only, and
no package of theirs provides `libxml2.so.2`. The WSL box is 26.04 too, but it kept the old `libxml2` 2.9.14 package
from its upgrade, so the gate's Linux real-app runs there worked.

**This broke users, not only CI.** In fresh containers, with the released v3.2.9-ux33 packages installed with their
declared dependencies, started under Xvfb on a scratch HOME as a plain user:

| | the database |
|---|---|
| Ubuntu 26.04.1, `.deb` | **Failed**: the archive unpacks, then `postgres: ... libxml2.so.2: cannot open shared object file`, "failed at: creating the database files (initdb)" |
| Ubuntu 24.04.5, `.deb` | Ready in 3.1 s (its webkit pulls in `libxml2.so.2`) |
| Fedora 45, bare 18.6.0 archive | initdb fails the same way (`libxml2.so.2`, then `libssl.so.3`) |

So no Linux release of knobas up to ux33 could start on a fresh Ubuntu 26.04 or Fedora 45. ux30 didn't introduce
this: every release that ran 18.6.0 has it. No package dependency could fix it, because nothing on those systems
provides the library.

## The fix: theseus's 18.6.1 on Linux

theseus published `18.6.1` on 2026-10-18. It is the same PostgreSQL 18.6 source (tag `REL_18_6`), rebuilt on Debian
12 with every library it needs in its own `lib/`: libxml2, ICU 72, OpenSSL 3, krb5, zstd, lz4, and a Python 3.11 for
PL/Python. Its programs find them through `RUNPATH $ORIGIN/../lib`. Outside the archive it needs only glibc (2.36 or
newer). `knobas-app` itself already needs glibc 2.39 (ux33's binary), so no distribution that ran knobas before loses
it.

Measured in bare containers with no packages installed: initdb, a start, an XML parse and pg_dump all pass on Ubuntu
26.04, Ubuntu 24.04 and Fedora 45.

**Ruled (Fable, while Björn was away):** pin 18.6.1 on Linux only, and keep 18.6.0 on macOS and Windows for this
hotfix. Moving every platform was ruled out:
- theseus's 18.6.1 swaps the Windows server from EDB's build to one compiled with MSVC and vcpkg, which nobody has run
  with knobas yet.
- It would add 76 MB to the `.dmg` for nothing, since macOS and Windows aren't broken.

Shipping our own copy of the missing libraries beside 18.6.0 was ruled out too:
- knobas would have to maintain a library set by hand (ICU and its soname chain).
- It gives up the unmodified, hash-pinned upstream archive (`2026-11-14-knobas-ships-its-postgresql.md`).
- It would need a second fix for the download path that dev builds and CI use.

The split is temporary. A ticket moves macOS and Windows to the same build after a Windows pass.

- `PG_VERSION_REQ` (`crates/knobas-db/src/embedded.rs`) is `=18.6.1` on Linux and `=18.6.0` elsewhere.
  `crates/knobas-app/postgresql/archives.sha256` pins the Linux archive of 18.6.1. `fetch-postgres.sh` and both
  `check-bundled-postgres` scripts already read each target's version from that file.
- **Size:** the Linux archive goes from 12.1 MB to 40.4 MB, so the `.deb`, `.rpm` and `.AppImage` each grow by about
  28 MB. The unpacked install goes from 38 MB to 116 MB.
- **Its hard links are unpacked too.** The 18.6.1 archive stores 257 of its time zones as hard links, `UTC` among
  them. `postgresql_archive` 0.21's extractor, which both the carried archive's unpack and the download go through,
  writes files, directories and symlinks and skips hard links. Without them the server refuses knobas's
  `TimeZone=UTC` ("invalid value for parameter"), and the start fails. This showed up two ways: 14 `knobas-db` tests
  failed on Linux, and ux33's own `.deb` on 26.04 failed with the 18.6.1 archive swapped in.
  - `knobas_db::install::keep_hard_links` registers an extractor for theseus's URL. The crate's registry tries newer
    registrations first. The extractor runs the crate's own theseus extractor, then lays each hard link from the same
    bytes next to the file it names. Where a filesystem has no hard links, it copies the file instead, and it skips a
    path that would leave the install.
  - `postgresql_embedded` 0.22 keeps hard links itself. It also rewrites the command builders that knobas's start
    depends on (the `-F` before `-c fsync=on`), so that upgrade is its own change, not part of a hotfix.
- **Machines that ran knobas before:** the first start after the update unpacks 18.6.1 into
  `~/.theseus/postgresql/18.6.1`, beside the old folder. The old folder is left alone, because an orphan server may
  still run from it.
  - The data directory carries over unchanged, since both builds are PostgreSQL 18.6.
  - An orphan left by 18.6.0 is still adopted, because `pg_ctl` only signals the pid in `postmaster.pid`.

## The rpm requires libssl.so.3

Fedora 45's OpenSSL is 4 (`openssl-libs` 4.0.3, `libssl.so.4` only). `knobas-app` links `libssl.so.3` through
native-tls, so the `.rpm` couldn't start on any Fedora 45, desktop or not, before PostgreSQL even came into it. Its
Requires named only webkit and gtk. `tauri.conf.json` now adds `libssl.so.3()(64bit)` to `bundle.linux.rpm.depends`,
and Tauri appends its webkit and gtk requirements to that list. dnf resolves the requirement to `openssl-libs` on
older Fedora and to `openssl3-libs` on 45, which carries `libcrypto.so.3` too. Neither the `.deb` nor the AppImage is
affected: Ubuntu 26.04 still ships `libssl3t64`, and the AppImage carries its own `libssl.so.3`.

**Ruled (Fable):** in the hotfix. Its first side ruling, to ticket this, rested on my claim that a desktop has
`libssl.so.3`, which is false on Fedora 45. The lasting fix is rustls or a vendored OpenSSL (ticketed), because Fedora
will drop the compat package one day.

## The checks that would have caught it

- **`scripts/check-postgres-starts.sh <archive>`** fails unless every library a program or library in the archive
  needs is in the archive's own `lib/` or is part of glibc. That rule holds on any host, so it fails even on a runner
  that happens to have the library. It then runs the server the way the app does: initdb, a start, an XML query (the
  libxml2 path), pg_dump (the nightly backup), and a stop. On 18.6.0 it names `libxml2.so.2`, `libssl.so.3` and the
  rest. On 18.6.1 it passes.
- **`light.yml`** runs it on every push, on `ubuntu-latest` (26.04) and `ubuntu-24.04`. It is one archive download and a
  server start, about 30 s, standard runners, no cargo.
- **`release.yml`** runs it through `scripts/check-bundled-postgres.sh` on the archive inside each Linux bundle (`.deb`,
  `.rpm`, `.AppImage`) before anything is uploaded. The same script fails an `.rpm` that doesn't require
  `libssl.so.3()(64bit)`. A hotfix gets no separate boot check, so the release run is where the Linux bundles prove
  they can start their database.
- **`install::tests::a_hard_link_in_the_archive_is_unpacked_with_it`** packs an archive with a hard-linked `UTC` and
  unpacks it through knobas's path. It fails without `keep_hard_links`. The shell check above unpacks with `tar`,
  which keeps hard links, so only this Rust test guards the extractor.

## Workflows on `ubuntu-latest`

- **`check.yml`**, the full gate, which is manual only, runs on `ubuntu-latest`. Before this fix, every test that starts
  PostgreSQL would have failed there the same way. With 18.6.1 it doesn't need `libxml2.so.2`. Its cache of
  `~/.theseus` is keyed on `embedded.rs`, which this change edits, so a cached 18.6.0 isn't reused.
- **`mutants.yml`**: the gate's `044c4708` moved its Linux leg to `ubuntu-24.04` and installs libxml2. That stays
  harmless, and it can go back to `ubuntu-latest` once this has been through a mutants run.

## Proven, and what is owed

In fresh containers on the WSL box:
- The fixed `knobas-db` test binary ran on a bare Ubuntu 26.04 with no `libxml2.so.2`. It downloaded 18.6.1, `UTC` was
  in place, and every test that starts PostgreSQL passed: 95 of 96. The other one reads the source tree's
  `migrations/` folder, which isn't in the container.
- On Ubuntu 24.04, a data directory made and written by 18.6.0 opened with 18.6.1, and its rows read back.
- ux33's AppImage on 26.04 with 18.6.1 laid out reached Ready, so the AppImage's library path doesn't get in its way.

On WSL itself, `cargo test -p knobas-db` passed 224 tests, and `knobas-app`'s postgres tests passed.

**Owed once ux34 is built (Fable's condition):** its `.deb` reaches Ready on a fresh 26.04 and on 24.04 over ux33's
data, and its `.rpm` installs with dnf on Fedora 45 and reaches Ready.

## Left for later

- **macOS and Windows are on 18.6.0** until their 18.6.1 builds have been through a Windows pass (ticketed).
- **The AppImage leaves fribidi to the host.** On a bare 26.04 it stops on a missing `libfribidi.so.0`. Every GTK
  desktop has it, so nothing is needed there.
