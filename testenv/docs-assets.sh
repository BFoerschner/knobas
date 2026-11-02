#!/bin/sh
# The files the docs-assets server serves (#2456): Swagger UI's bundle and
# stylesheet, the payouts OpenAPI file and a status page. The Docs reader
# showcase's `html` and `iframe` macros load them from it, so Swagger UI is
# self-hosted and the showcase draws without the internet.
#
#   ./docs-assets.sh fetch     Swagger UI into docs-assets/swagger-ui/ (gitignored)
#   ./docs-assets.sh dir       print the directory to copy into the container
#
# Swagger UI comes from npm's registry once, as one pinned tarball checked
# against the integrity npm publishes for it. Only `fetch` needs the internet;
# a fetched copy is reused. openapi.json and status.html are in git.
set -eu
cd "$(dirname "$0")"

VERSION=5.34.1
TARBALL=https://registry.npmjs.org/swagger-ui-dist/-/swagger-ui-dist-$VERSION.tgz
INTEGRITY=sha512-VSr8fXfI93obYaQicLJbLUKGRxDojj/SfRJDDLCioGTUBj4BK/Nj4eE9jCeCAnJxVLi7iS+08PbiZLyrQKBkJQ==
DIR=docs-assets
OUT=$DIR/swagger-ui

die() { echo "docs-assets: $*" >&2; exit 1; }

fetch() {
  if [ "$(cat "$OUT/VERSION" 2>/dev/null)" = "$VERSION" ] \
     && [ -s "$OUT/swagger-ui-bundle.js" ] && [ -s "$OUT/swagger-ui.css" ]; then
    return 0
  fi
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT INT TERM
  curl -fsS -o "$tmp/dist.tgz" "$TARBALL" || die "could not download $TARBALL"
  got="sha512-$(openssl dgst -sha512 -binary "$tmp/dist.tgz" | openssl base64 -A)"
  [ "$got" = "$INTEGRITY" ] || die "$TARBALL: integrity $got, npm publishes $INTEGRITY"
  tar -xzf "$tmp/dist.tgz" -C "$tmp" package/swagger-ui-bundle.js package/swagger-ui.css
  mkdir -p "$OUT"
  cp "$tmp/package/swagger-ui-bundle.js" "$tmp/package/swagger-ui.css" "$OUT/"
  echo "$VERSION" > "$OUT/VERSION"
  echo "docs-assets: Swagger UI $VERSION in $OUT"
}

case "${1:-}" in
  fetch) fetch ;;
  dir) echo "$(pwd)/$DIR" ;;
  *) die "usage: ./docs-assets.sh fetch|dir" ;;
esac
