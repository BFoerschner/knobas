#!/usr/bin/env bash
# Stage the PostgreSQL archive a release build carries
# (docs/decisions/2026-11-14-knobas-ships-its-postgresql.md).
#
# Downloads the pinned upstream archive for one target, checks it against
# `crates/knobas-app/postgresql/archives.sha256`, and puts it beside that file
# as `postgresql-<version>.tar.gz`, where `tauri.conf.json`'s
# `bundle.resources` picks it up. The app unpacks it on a machine that has no
# PostgreSQL, so a first start needs no network.
#
# Run before `tauri build`; `release.yml` does, and
# `scripts/check-bundled-postgres.sh` fails the build whose bundles lack it.
# A build made without it still works: it downloads on a machine's first
# start, as `just dev` does.
#
# Usage: scripts/fetch-postgres.sh [target-triple]
#   target-triple  default: this machine's, from `rustc -vV`
# Env:   KNOBAS_POSTGRES_DIR  where to stage (default: crates/knobas-app/postgresql)
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
pins="$root/crates/knobas-app/postgresql/archives.sha256"
dir=${KNOBAS_POSTGRES_DIR:-$root/crates/knobas-app/postgresql}
target=${1:-$(rustc -vV | sed -n 's/^host: //p')}
[ -n "$target" ] || { echo "error: no target given and rustc names no host" >&2; exit 1; }

# `<sha256>  postgresql-<version>-<target>.tar.gz`
line=$(grep -- "-$target\.tar\.gz\$" "$pins" || true)
if [ -z "$line" ] || [ "$(printf '%s\n' "$line" | wc -l)" -ne 1 ]; then
  echo "error: $pins pins no one archive for $target" >&2
  exit 1
fi
want=${line%% *}
asset=${line##* }
version=${asset#postgresql-}
version=${version%-"$target".tar.gz}
out="$dir/postgresql-$version.tar.gz"

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

mkdir -p "$dir"
if [ -f "$out" ] && [ "$(sha256 "$out")" = "$want" ]; then
  echo "fetch-postgres: $out is already the pinned $asset"
  exit 0
fi

# Outside the staged folder: everything in it goes into the bundle.
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
url="https://github.com/theseus-rs/postgresql-binaries/releases/download/$version/$asset"
echo "fetch-postgres: downloading $url"
curl --fail --location --silent --show-error --retry 3 --output "$tmp" "$url"
got=$(sha256 "$tmp")
if [ "$got" != "$want" ]; then
  echo "error: $asset has sha256 $got, and $pins pins $want" >&2
  exit 1
fi

# One archive per build: another version's, or another target's under the same
# name, would be carried too or instead.
for old in "$dir"/postgresql-*.tar.gz; do
  [ -e "$old" ] || continue
  rm -f "$old"
done
mv "$tmp" "$out"
# `mktemp` made it the owner's alone, and a package installs it as it is: a
# `.deb`'s copy, owned by root, has to be readable by whoever runs knobas.
chmod 644 "$out"
echo "fetch-postgres: staged $out ($(wc -c < "$out" | tr -d ' ') bytes, sha256 $want)"
