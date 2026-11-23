#!/usr/bin/env bash
# Fail unless the Linux PostgreSQL archive knobas carries starts on this
# machine with nothing but glibc from the host
# (docs/decisions/2026-11-23-linux-postgresql-carries-its-libraries.md).
#
# theseus's 18.6.0 Linux build linked the host's libxml2.so.2, which Ubuntu
# 26.04 and Fedora 45 no longer ship (they have libxml2.so.16): knobas could
# not start its database there at all, and nothing in CI noticed, because the
# archive is only hashed and every Linux runner we used still had the library.
# So this checks two things:
#
#   1. Every shared library a program or library in the archive needs is in
#      the archive's own `lib/`, or is part of glibc itself. That holds on any
#      host, so it fails on a runner that happens to have the library too.
#   2. The server starts: initdb, a start, a query that goes through libxml2,
#      pg_dump (what the nightly backup runs), and a stop.
#
# `light.yml` runs it on the pinned archive at every push, and
# `scripts/check-bundled-postgres.sh` on the copy inside each Linux bundle
# `release.yml` makes. Run it as a user other than root (initdb refuses root).
#
# Usage: scripts/check-postgres-starts.sh <postgresql-*.tar.gz>
set -euo pipefail

archive=${1:?usage: check-postgres-starts.sh <postgresql-*.tar.gz>}
[ -f "$archive" ] || { echo "error: no archive at $archive" >&2; exit 2; }
[ "$(uname -s)" = Linux ] || { echo "error: the Linux archive is checked on Linux" >&2; exit 2; }

# glibc's own libraries: every Linux knobas runs on has them, and an archive
# that carried them would be wrong. Anything else must come with the archive.
host_ok=" ld-linux-x86-64.so.2 libc.so.6 libm.so.6 libdl.so.2 libpthread.so.0 librt.so.1 libresolv.so.2 libutil.so.1 libanl.so.1 "

scratch=$(mktemp -d)
pg=""
cleanup() {
  if [ -n "$pg" ] && [ -f "$scratch/data/postmaster.pid" ]; then
    "$pg/bin/pg_ctl" -D "$scratch/data" -m immediate -w stop >/dev/null 2>&1 || true
  fi
  rm -rf "$scratch"
}
trap cleanup EXIT

pg="$scratch/pg"
mkdir -p "$pg"
# Upstream's layout: one top-level directory, which the app's extractor strips.
tar -xzf "$archive" -C "$pg" --strip-components=1

# 1. What the archive's binaries need, against what it carries.
missing=0
while IFS= read -r -d '' file; do
  # Not an ELF file (a script, a header, data): nothing to load.
  needed=$(readelf -d "$file" 2>/dev/null | sed -n 's/.*(NEEDED).*\[\(.*\)\]$/\1/p') || continue
  for lib in $needed; do
    case "$host_ok" in *" $lib "*) continue ;; esac
    if [ ! -e "$pg/lib/$lib" ]; then
      echo "FAIL ${file#"$pg"/} needs $lib, which the archive does not carry and glibc does not provide"
      missing=$((missing + 1))
    fi
  done
done < <(find "$pg/bin" "$pg/lib" -type f -print0)
if [ "$missing" -ne 0 ]; then
  echo "error: $(basename "$archive") leans on $missing host libraries a fresh Ubuntu or Fedora may not have" >&2
  exit 1
fi
echo "ok   every library the archive's programs need is in its lib/ or in glibc"

# 2. A server from it, the way the app runs one: a data directory of its own,
# a socket in the scratch folder and no TCP.
"$pg/bin/initdb" -D "$scratch/data" -U postgres -A trust --no-sync >"$scratch/initdb.log" 2>&1 ||
  { cat "$scratch/initdb.log" >&2; echo "error: initdb failed" >&2; exit 1; }
"$pg/bin/pg_ctl" -D "$scratch/data" -l "$scratch/server.log" -w \
  -o "-c listen_addresses= -k $scratch -p 5433" start >/dev/null ||
  { cat "$scratch/server.log" >&2; echo "error: the server did not start" >&2; exit 1; }
xml=$("$pg/bin/psql" -h "$scratch" -p 5433 -U postgres -d postgres -XAtc \
  "select xmlparse(document '<knobas>ok</knobas>')::text")
[ "$xml" = "<knobas>ok</knobas>" ] || { echo "error: the XML query answered '$xml'" >&2; exit 1; }
"$pg/bin/pg_dump" -h "$scratch" -p 5433 -U postgres -d postgres --schema-only >/dev/null ||
  { echo "error: pg_dump failed" >&2; exit 1; }
"$pg/bin/pg_ctl" -D "$scratch/data" -m fast -w stop >/dev/null
echo "ok   $("$pg/bin/postgres" -V) ran initdb, started, parsed XML, dumped and stopped"
