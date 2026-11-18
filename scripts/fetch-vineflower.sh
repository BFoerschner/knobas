#!/usr/bin/env bash
# Stage the decompiler a build carries
# (docs/decisions/2026-11-18-a-jar-is-drawn-as-its-contents.md).
#
# Downloads the pinned Vineflower jar from Maven Central, checks it against
# `crates/knobas-app/decompiler/vineflower.sha256`, and puts it beside that
# file, where `tauri.conf.json`'s `bundle.resources` picks it up. knobas runs it
# with a Java the machine already has; it never downloads it at run time.
#
# Run before `tauri build`; `release.yml` does. A build made without it works,
# and offers no *Decompile*. `just dev` finds it in the same folder once fetched.
#
# Usage: scripts/fetch-vineflower.sh
# Env:   KNOBAS_DECOMPILER_DIR  where to stage (default: crates/knobas-app/decompiler)
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
pins="$root/crates/knobas-app/decompiler/vineflower.sha256"
dir=${KNOBAS_DECOMPILER_DIR:-$root/crates/knobas-app/decompiler}

# `<sha256>  vineflower-<version>.jar`
line=$(head -n 1 "$pins")
want=${line%% *}
jar=${line##* }
version=${jar#vineflower-}
version=${version%.jar}
out="$dir/$jar"

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

mkdir -p "$dir"
if [ -f "$out" ] && [ "$(sha256 "$out")" = "$want" ]; then
  echo "fetch-vineflower: $out is already the pinned $jar"
  exit 0
fi

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
url="https://repo1.maven.org/maven2/org/vineflower/vineflower/$version/$jar"
echo "fetch-vineflower: downloading $url"
curl --fail --location --silent --show-error --retry 3 --output "$tmp" "$url"
got=$(sha256 "$tmp")
if [ "$got" != "$want" ]; then
  echo "error: $jar is $got, pinned $want" >&2
  exit 1
fi
mv "$tmp" "$out"
trap - EXIT
echo "fetch-vineflower: staged $out"
