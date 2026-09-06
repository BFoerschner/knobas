#!/bin/sh
# The host half of kuma-monitor.mjs: one throwaway monitor, added or deleted
# through Kuma's socket.io channel.
#
#   ./kuma-monitor.sh add <name> <url>    create it (idempotent: one, never two)
#   ./kuma-monitor.sh delete <name>       remove it (idempotent)
#
# `just kuma-live` uses it to witness M4 spec #427's story 54 -- a monitor
# deleted in Kuma leaves the mirror on the next run -- against a monitor that
# is nobody else's. See kuma-monitor.mjs for why this is node and not curl, and
# README.md ("Monitors") for who owns which monitor.
#
# The container gets the repo READ-ONLY, like the seed's: a container writing
# to a bind mount leaves root-owned files behind on Linux. The script is copied
# next to the node_modules npm populates because ESM ignores NODE_PATH -- the
# same dance the `kuma-seed` service's own command does, and the reason this
# overrides that service's command rather than adding a second service.
set -eu
cd "$(dirname "$0")"

case "${1:-}" in
  add)
    [ $# -eq 3 ] || { echo "usage: ./kuma-monitor.sh add <name> <url>" >&2; exit 2; } ;;
  delete)
    [ $# -eq 2 ] || { echo "usage: ./kuma-monitor.sh delete <name>" >&2; exit 2; } ;;
  *)
    echo "usage: ./kuma-monitor.sh add <name> <url> | delete <name>" >&2; exit 2 ;;
esac

docker compose --profile seed run --rm -T kuma-seed sh -c '
  mkdir -p /tmp/k &&
  cp /seed/kuma-monitor.mjs /tmp/k/ &&
  npm --prefix /tmp/k i --no-save --silent socket.io-client@4 &&
  node /tmp/k/kuma-monitor.mjs "$@"' sh "$@"
