#!/usr/bin/env bash
# The Mac's update artifact (ADR-0052, docs/decisions/2026-11-21-knobas-updates-itself.md): the bundle
# `target/release/bundle/macos/knobas.app` as `knobas_<version>_aarch64.app.tar.gz` beside it, which a running
# knobas unpacks and exchanges with itself. `COPYFILE_DISABLE` keeps macOS's `._` files out.
#
# Fails unless the archive holds the executable folder and the PostgreSQL archive every bundle carries
# (scripts/check-bundled-postgres.sh), so an update can never install a knobas without its database.
#
#     scripts/pack-mac-update.sh <version> [<bundle dir>]
set -euo pipefail
version=${1:?usage: pack-mac-update.sh <version> [<bundle dir>]}
dir=${2:-target/release/bundle/macos}
[ -d "$dir/knobas.app" ] || { echo "pack-mac-update: no $dir/knobas.app" >&2; exit 1; }
out="$dir/knobas_${version}_aarch64.app.tar.gz"
rm -f "$out"
COPYFILE_DISABLE=1 tar -czf "$out" -C "$dir" knobas.app
listing=$(tar -tzf "$out")
grep -q '^knobas.app/Contents/MacOS/' <<<"$listing" || { echo "pack-mac-update: $out holds no knobas.app/Contents/MacOS/" >&2; exit 1; }
grep -qE '^knobas.app/Contents/Resources/postgresql/postgresql-.*\.tar\.gz$' <<<"$listing" \
    || { echo "pack-mac-update: $out holds no knobas.app/Contents/Resources/postgresql/postgresql-*.tar.gz" >&2; exit 1; }
if grep -q '/\._' <<<"$listing"; then echo "pack-mac-update: $out holds macOS ._ files" >&2; exit 1; fi
echo "packed $out"
