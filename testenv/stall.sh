#!/usr/bin/env bash
# The stall: a socket that accepts every connection and never answers, for
# Uptime Kuma to check -- `live_kuma`'s witness that a refresh waits for a
# created monitor's first heartbeat needs a check that cannot finish.
#
#   ./stall.sh up       bind it (idempotent; waits until it accepts)
#   ./stall.sh down     release it (idempotent)
#   ./stall.sh url      the URL Kuma reaches it at
#
# Only a REMOTE target needs it (testenv/live.env): locally the suite holds
# such a socket itself, on this machine, where Kuma's `host.docker.internal`
# reaches. A remote Kuma resolves that name to its own machine's Docker
# bridge, so there the socket must be on that machine and on that bridge --
# canary.sh's reasoning and dispatch, on port 8298. `just kuma-live` binds it
# and releases it from its traps, and exports `KNOBAS_KUMA_STALLED_URL`.
set -euo pipefail

if [ -z "${KNOBAS_STALL_REMOTE_SIDE:-}" ]; then
  here=$(cd "$(dirname "$0")" && pwd)
  # shellcheck source=testenv/live-lib.sh disable=SC1091
  . "$here/live-lib.sh"
  case "${1:-}" in up | down | url) ;; *) echo "stall: usage: ./stall.sh up|down|url" >&2; exit 2 ;; esac
  [ -n "$LIVE_REMOTE" ] || { echo "stall: only a remote target needs it; the suite holds its own socket here" >&2; exit 1; }
  if [ "$1" = url ]; then echo "http://host.docker.internal:8298/"; exit 0; fi
  exec ssh -o BatchMode=yes "$KNOBAS_LIVE_SSH" "KNOBAS_STALL_REMOTE_SIDE=1 bash -s -- $1" <"$here/stall.sh"
fi

PORT=8298
PIDFILE="$HOME/.knobas-stall-$PORT.pid"
BIND=$(ip -4 -o addr show docker0 2>/dev/null | awk '{ print $4 }' | cut -d/ -f1 | head -1)
[ -n "$BIND" ] || { echo "stall: no docker0 address on $(hostname) -- is dockerd running?" >&2; exit 1; }

alive() { [ -r "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; }
# Accepts within a second: the connect succeeds, and nothing ever comes back.
accepts() { timeout 1 bash -c "exec 3<>/dev/tcp/$BIND/$PORT" 2>/dev/null; }

case $1 in
  up)
    if alive && accepts; then echo "stall: up (pid $(cat "$PIDFILE"), $BIND:$PORT)"; exit 0; fi
    rm -f "$PIDFILE"
    nohup python3 -c '
import socket, sys
s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind((sys.argv[2], int(sys.argv[1])))
s.listen(64)
held = []
while True:
    c, _ = s.accept()
    held.append(c)
' "$PORT" "$BIND" >/dev/null 2>&1 &
    echo $! >"$PIDFILE"
    for _ in $(seq 1 40); do accepts && { echo "stall: up (pid $(cat "$PIDFILE"), $BIND:$PORT)"; exit 0; }; sleep 0.25; done
    kill "$(cat "$PIDFILE")" 2>/dev/null || true
    rm -f "$PIDFILE"
    echo "stall: did not accept on $BIND:$PORT within 10 s" >&2
    exit 1 ;;
  down)
    if alive; then kill "$(cat "$PIDFILE")" 2>/dev/null || true; fi
    rm -f "$PIDFILE"
    echo "stall: down" ;;
esac
