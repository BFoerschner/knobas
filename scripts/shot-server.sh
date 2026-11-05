#!/usr/bin/env bash
# The dev server the screenshot scripts shoot, sourced by scripts/shot.sh and
# scripts/readme-shots.sh. Not run on its own.
#
# shot_server_start <port> <checkout> <wait seconds> <log file>
#   starts `vite` in <checkout>/app on <port> and returns once it answers, unless
#   a server already answers there, which is reused and left running.
# shot_server_stop
#   stops the server this file started, and nothing else. Callers trap it on EXIT.

shot_server_pid=

shot_server_up() { curl -fsS -o /dev/null "http://localhost:$1/" 2>/dev/null; }

# The server runs in its own process group (`set -m` below), and the whole group
# is stopped: on Linux vite is a grandchild (npm -> sh -> node), which a kill of
# the wrapper and its children missed, and the survivor served the next run's
# shots from the old tree (#2339). Bash's job control, not `setsid`: macOS has
# no `setsid` binary.
shot_server_stop() {
  [ -n "$shot_server_pid" ] || return 0
  kill -TERM -- "-$shot_server_pid" 2>/dev/null || { pkill -TERM -P "$shot_server_pid" 2>/dev/null || true; kill "$shot_server_pid" 2>/dev/null || true; }
  wait "$shot_server_pid" 2>/dev/null || true
}

shot_server_start() {
  local port=$1 checkout=$2 wait_s=$3 log=$4 from
  ! shot_server_up "$port" || return 0
  set -m
  (cd "$checkout/app" && exec npx vite --port "$port" --strictPort >"$log" 2>&1) &
  shot_server_pid=$!
  set +m
  # Polled until it answers (#1957): a loaded machine starts vite slowly, so the
  # guard is sized past that, and a vite that died fails at once.
  from=$SECONDS
  until shot_server_up "$port"; do
    kill -0 "$shot_server_pid" 2>/dev/null || { echo "shot: the dev server on :$port exited before it answered; log: $log" >&2; exit 1; }
    [ $((SECONDS - from)) -lt "$wait_s" ] || { echo "shot: the dev server on :$port did not answer within $wait_s s (SHOT_START_WAIT_S); log: $log" >&2; exit 1; }
    sleep 0.5
  done
}
