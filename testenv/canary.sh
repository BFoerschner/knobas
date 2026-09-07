#!/usr/bin/env bash
# The canary: a host socket on 127.0.0.1:8299 that answers `canary` with a 200,
# and the two commands that bind and release it.
#
#   ./canary.sh up       bind the port (idempotent; waits until it answers)
#   ./canary.sh down     release it (idempotent)
#   ./canary.sh status   one line, exit 0 when it is bound
#
# WHY IT EXISTS. M4.1's alert chain has to be proven end to end against the
# real Kuma: a monitor goes down, an alert opens, it is acked, the thing
# recovers, the alert closes. The only honest way to knock a monitor down is to
# break what it checks -- and every other thing Kuma watches here is shared
# (Gitea serves the live suites, the three products serve the Atlassian and
# TeamCity runs, the servers are the estate). Stopping any of those to make a
# red is how one stream's live run becomes another's mystery failure. So the
# estate gets one check that exists purely to be knocked over, whose whole
# blast radius is a port nobody else binds.
#
# WHO USES IT (issue #450, M4.1's exit). `just kuma-live` binds the port before
# its suites and rebinds it from a trap however the run ends;
# `crates/knobas-app/tests/alert_chain_live.rs` releases it, waits for `canary`
# to read down, acks the alert, binds it again and waits for the recovery --
# and stops no container at all (M4 spec, issue #427). Both call this script
# and nothing else, which is what makes that sentence checkable: the only thing
# either of them can knock over is 8299.
#
# WHY A HOST PROCESS AND NOT A CONTAINER. Kuma reaches it at
# http://host.docker.internal:8299/, the same host alias the three tunnel
# checks use, so the canary exercises that path too: if the alias breaks, the
# canary says so rather than three product checks going red at once for a
# reason that looks like the tunnel.
#
# 8299 is outside the interfaces §5 port map (3000, 3001, 8080, 8090, 8111,
# 8200, 8210, 8212) on purpose: it is a host port, not a published container
# port, so check-ports.sh neither knows nor should know about it.
set -euo pipefail
cd "$(dirname "$0")"

PORT=8299
PIDFILE="$HOME/.knobas-canary-$PORT.pid"

# Not in testenv/: `./reset` would have to learn to kill the process before
# removing the file, and a stale pidfile in the repo is one more thing a
# `git status` has to explain. $HOME for the same reason ./hetzner/tunnel keeps
# its own under ~/.ssh/ -- a different directory, the same argument.

say() { echo "canary: $*"; }
die() { echo "canary: $*" >&2; exit 1; }

# The responder: 200 and the word `canary`. HTTP/1.0 with an explicit
# Content-Length, so the client sees a complete response and the connection
# closes rather than being held open by a keep-alive nothing here would honour.
# The access log is silenced -- `up` backgrounds this and must leave nothing
# writing to a terminal it no longer owns.
responder() {
  cat <<'PY'
import http.server, sys
class C(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"
    def do_GET(self):
        body = b"canary\n"
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args):
        pass
http.server.HTTPServer(("127.0.0.1", int(sys.argv[1])), C).serve_forever()
PY
}

# Ours, as opposed to whatever else may have grabbed the port: the pid in the
# file is alive. A port that answers with no pidfile is somebody else's and is
# never killed by `down`.
alive() { [ -r "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; }

# ... and it is still the process on the port. The pidfile lives in $HOME, so
# it outlives a reboot, and a pid is recycled: `kill $(cat pidfile)` on a stale
# file is a script that kills a stranger. `down` therefore asks the kernel who
# is actually listening before it kills anything. lsof is on macOS and on every
# Linux this would run on; without it the check abstains rather than blocking a
# release that is probably right.
listens() { # listens <pid>
  command -v lsof >/dev/null || return 0
  lsof -nP -iTCP:"$PORT" -sTCP:LISTEN -t 2>/dev/null | grep -qx "$1"
}
# Quiet on failure: "not answering" is the normal, expected answer here (it is
# how `down` confirms the release), not something to print a curl error about.
answers() { curl -fs -o /dev/null --max-time 3 "http://127.0.0.1:$PORT/" 2>/dev/null; }

up() {
  if alive && answers; then say "up (pid $(cat "$PIDFILE"), 127.0.0.1:$PORT)"; return 0; fi
  alive || rm -f "$PIDFILE"
  if answers; then
    die "127.0.0.1:$PORT already answers and it is not ours. Find it with
    lsof -nP -iTCP:$PORT -sTCP:LISTEN -- and do not kill it blind."
  fi
  command -v python3 >/dev/null || die "python3 is required for the responder"
  # `-c "$src" "$PORT"`, not a pipeline into `python3 -`: `$!` after a
  # backgrounded pipeline is the LAST command's pid, and the pidfile has to
  # name the process that holds the socket. `nohup` execs python3, so the pid
  # it reports is python3's.
  local src i=0
  src=$(responder)
  nohup python3 -c "$src" "$PORT" >/dev/null 2>&1 &
  echo $! > "$PIDFILE"
  # 40 tries, not "20 seconds": `answers` carries `--max-time 3`, so a socket
  # that accepts and then hangs makes each turn ~3.5 s rather than 0.5 s and
  # the wall clock is nothing like 40 x 0.5. Saying "40 tries" is the thing
  # this loop actually counts. (`hetzner/tunnel` can honestly say 20 s because
  # its probe is `nc -z`, which returns at once.)
  # Both failure paths KILL before removing the pidfile. Removing it first
  # would disown a responder that is still holding the socket, and this
  # script's whole ownership rule is "a port that answers with no pidfile is
  # somebody else's" -- so `up` would then refuse the port and `down` would
  # call it already down, and nothing here could ever release it again.
  until answers; do
    i=$((i + 1))
    [ "$i" -lt 40 ] || { kill "$(cat "$PIDFILE")" 2>/dev/null || true; rm -f "$PIDFILE"; die "did not answer on 127.0.0.1:$PORT after 40 tries"; }
    kill -0 "$(cat "$PIDFILE")" 2>/dev/null || { rm -f "$PIDFILE"; die "responder exited at once (is $PORT taken?)"; }
    sleep 0.5
  done
  say "up (pid $(cat "$PIDFILE"), 127.0.0.1:$PORT)"
}

down() {
  if ! alive; then rm -f "$PIDFILE"; say "already down"; return 0; fi
  local pid i=0
  pid=$(cat "$PIDFILE")
  if ! listens "$pid"; then
    rm -f "$PIDFILE"
    say "pid $pid is alive but is not on 127.0.0.1:$PORT -- stale pidfile removed, nothing killed"
    return 0
  fi
  kill "$pid" 2>/dev/null || true
  # The pidfile goes only once the port is actually free. Removing it up front
  # -- or being interrupted during this wait -- would leave a live responder
  # that no longer has a pidfile naming it, which by the rule above is
  # somebody else's and is never killed again. Leaving it means a second
  # `./canary.sh down` finishes the job.
  while answers; do
    i=$((i + 1))
    [ "$i" -lt 20 ] || die "killed pid $pid but 127.0.0.1:$PORT still answers"
    sleep 0.5
  done
  rm -f "$PIDFILE"
  say "down (released 127.0.0.1:$PORT)"
}

status() {
  if alive && answers; then say "up (pid $(cat "$PIDFILE"), 127.0.0.1:$PORT)"; return 0; fi
  if answers; then say "127.0.0.1:$PORT answers but is not ours"; return 1; fi
  say "down"; return 1
}

case "${1:-}" in
  up)     up ;;
  down)   down ;;
  status) status ;;
  *) die "usage: ./canary.sh up|down|status" ;;
esac
