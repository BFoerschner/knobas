#!/usr/bin/env bash
# Starts the live products and waits for them.
set -euo pipefail

RETRY_LIMIT=${RETRY_LIMIT:-3}

wait_for() {
  local url=$1 tries=0
  until curl -fsS "$url" >/dev/null; do
    tries=$((tries + 1))
    [ "$tries" -ge "$RETRY_LIMIT" ] && { echo "gave up on $url" >&2; return 1; }
    sleep 2
  done
}

wait_for "http://127.0.0.1:3000/api/healthz" && echo ok
