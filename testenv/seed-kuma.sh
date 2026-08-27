#!/bin/sh
# Configure the Uptime Kuma baseline through socket.io (Kuma v2 has no REST API
# for configuration). The work is done by kuma-seed.mjs inside a one-shot
# node container on the compose network; this script is the host half.
#
# The container gets the repo READ-ONLY. The API key comes back on its stdout
# rather than being written into the mount, because a container writing to a
# bind mount leaves root-owned files behind on Linux.
set -eu
cd "$(dirname "$0")"

KEY_FILE=kuma-api-key
HAVE=0
[ -s "$KEY_FILE" ] && HAVE=1

# stdout is the protocol channel (one line: KEY=... or KEEP); the container's
# progress goes to stderr and straight through to the terminal.
out=$(docker compose --profile seed run --rm -T \
        -e KNOBAS_HAVE_KEY="$HAVE" \
        kuma-seed)

case "$out" in
  KEEP)
    echo "seed-kuma: kept the existing API key in $KEY_FILE"
    ;;
  KEY=uk*)
    printf '%s\n' "${out#KEY=}" > "$KEY_FILE"
    chmod 600 "$KEY_FILE"
    echo "seed-kuma: wrote a new API key to $KEY_FILE"
    ;;
  *)
    # Anything else means the container printed something unexpected on stdout
    # -- a compose message, an npm line, a partial failure. Refusing here beats
    # writing garbage into kuma-api-key and failing mysteriously later.
    echo "seed-kuma: unexpected output from kuma-seed (wanted 'KEEP' or 'KEY=uk...'):" >&2
    printf '%s\n' "$out" >&2
    exit 1
    ;;
esac

echo "seed-kuma: monitors from monitors.json are configured"
