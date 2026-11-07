#!/usr/bin/env bash
# Serves a built app on 127.0.0.1 for the length of one command, then stops it. No server outlives its call.
# Usage: with-built.sh <folder> <port> <command...>
set -uo pipefail
folder=$1 port=$2
shift 2
[ -f "$folder/index.html" ] || { echo "no built app in $folder" >&2; exit 1; }
if curl -fsS -o /dev/null "http://127.0.0.1:$port/" 2>/dev/null; then
  echo "something already answers on $port" >&2
  exit 1
fi
python3 -I -m http.server "$port" --bind 127.0.0.1 --directory "$folder" >/dev/null 2>&1 &
server=$!
stop() { kill "$server" 2>/dev/null || true; }
trap stop EXIT
for _ in $(seq 1 100); do
  curl -fsS -o /dev/null "http://127.0.0.1:$port/" 2>/dev/null && break
  sleep 0.1
done
curl -fsS -o /dev/null "http://127.0.0.1:$port/" 2>/dev/null || { echo "no server on $port" >&2; exit 1; }
"$@"
