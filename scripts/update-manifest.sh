#!/usr/bin/env bash
# A release's `latest.json` (ADR-0052, decisions D3), in the official updater's static format, on stdout: one platform
# entry for each artifact that has both a `.sig` in <sigs> and an asset in <assets.json> (`gh release view --json
# assets`), its URL the release's download address by tag (a draft's own asset URLs are `untagged-…` and would never
# work once published), `knobas.schema` the highest migration number in the tree, `knobas.sizes` the assets' sizes.
# Exit 1 when no artifact is signed: such a release carries no manifest and is never offered.
#
# Pure over its three inputs, plus `MIGRATIONS` (the folder, `crates/knobas-db/migrations` by default), `REPO`
# (`$GITHUB_REPOSITORY`, else BFoerschner/knobas) and `PUB_DATE` (now, UTC) for the tests.
#
#     scripts/update-manifest.sh <tag> <sigs dir> <assets.json> > latest.json
set -euo pipefail
tag=${1:?usage: update-manifest.sh <tag> <sigs dir> <assets.json>}
sigs=${2:?usage: update-manifest.sh <tag> <sigs dir> <assets.json>}
assets=${3:?usage: update-manifest.sh <tag> <sigs dir> <assets.json>}
version=${tag#v}
repo=${REPO:-${GITHUB_REPOSITORY:-BFoerschner/knobas}}
migrations=${MIGRATIONS:-crates/knobas-db/migrations}
pub_date=${PUB_DATE:-$(date -u +%Y-%m-%dT%H:%M:%SZ)}

schema=$(find "$migrations" -maxdepth 1 -name '[0-9]*_*.sql' -exec basename {} \; | sed 's/_.*//' | sort -n | tail -n 1)
[ -n "$schema" ] || { echo "update-manifest: no migration in $migrations" >&2; exit 1; }
schema=$((10#$schema))

entries='{}'
sizes='{}'
add() {
    local key=$1 name=$2 size
    [ -s "$sigs/$name.sig" ] || return 0
    size=$(jq -r --arg name "$name" '[.assets[] | select(.name == $name)][0].size // empty' "$assets")
    [ -n "$size" ] || return 0
    entries=$(jq --arg key "$key" --arg url "https://github.com/$repo/releases/download/$tag/$name" \
        --rawfile sig "$sigs/$name.sig" '. + {($key): {url: $url, signature: ($sig | gsub("\\s+$"; ""))}}' <<<"$entries")
    sizes=$(jq --arg key "$key" --argjson size "$size" '. + {($key): $size}' <<<"$sizes")
}
add windows-x86_64-nsis "knobas_${version}_x64-setup.exe"
add windows-x86_64-msi "knobas_${version}_x64_en-US.msi"
add darwin-aarch64-app "knobas_${version}_aarch64.app.tar.gz"
add linux-x86_64-appimage "knobas_${version}_amd64.AppImage"
add linux-x86_64-deb "knobas_${version}_amd64.deb"
add linux-x86_64-rpm "knobas-${version}-1.x86_64.rpm"

if [ "$(jq 'length' <<<"$entries")" -eq 0 ]; then
    echo "update-manifest: no signed artifact of $tag" >&2
    exit 1
fi
jq -n --arg version "$version" --arg date "$pub_date" --argjson platforms "$entries" --argjson schema "$schema" \
    --argjson sizes "$sizes" '{version: $version, pub_date: $date, platforms: $platforms, knobas: {schema: $schema, sizes: $sizes}}'
