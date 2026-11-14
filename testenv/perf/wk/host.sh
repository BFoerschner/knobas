#!/usr/bin/env bash
# The front end on the fake corpus (?fake-ipc&fake-scale) in a bare WKWebView (wkhost), with the probe on 14278.
#
#   host.sh start [--uncap] [HASH]   serve dist-dev on 14278 and open wkhost on it; --uncap lifts WebKit's 60 fps preference
#   host.sh stop
#   host.sh js 'EXPR'
#   host.sh pid
# DIST=<folder> serves another build than $WORK/dist-dev (a variant built from a scratch copy of the source).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
# WORK holds dist-dev/ (a built app, testenv/ui-checks/build-app.sh) and tools/wkhost (swiftc -O wkhost.swift -o tools/wkhost).
WORK=${WORK:-${TMPDIR:-/tmp}/knobas-wk}
state=$WORK/host-state
port=14278
mkdir -p "$state"
die() { printf 'host: %s\n' "$*" >&2; exit 1; }
js() {
    curl -s -m "${JS_WAIT:-180}" -X POST --data-binary "$1" "http://localhost:$port/__probe" |
        python3 -I -c 'import json,sys; d=json.load(sys.stdin); print("ERROR " + d["error"] if d.get("error") else json.dumps(d.get("value")))'
}
case ${1-} in
start)
    shift
    uncap=""
    if [ "${1-}" = --uncap ]; then uncap=--uncap; shift; fi
    hash=${1:-#/inbox}
    curl -fsS -o /dev/null "http://localhost:$port/" 2>/dev/null && die "something already answers on $port"
    node "$here/serve-probe.mjs" "${DIST:-$WORK/dist-dev}" $port >"$state/serve.log" 2>&1 &
    echo $! >"$state/serve.pid"
    for _ in $(seq 1 50); do curl -fsS -o /dev/null "http://localhost:$port/" 2>/dev/null && break; sleep 0.1; done
    "$WORK/tools/wkhost" "http://localhost:$port/?fake-ipc&fake-scale$hash" $uncap >"$state/host.log" 2>&1 &
    echo $! >"$state/host.pid"
    r=""
    for _ in $(seq 1 90); do
        r=$(js '!!document.querySelector(".tf-nav")' 2>/dev/null || true)
        [ "$r" = "true" ] && break
        sleep 1
    done
    cat "$state/host.log"
    [ "$r" = "true" ] || die "the page never drew its nav"
    echo "host: $(js '[location.href, innerWidth, innerHeight, devicePixelRatio]')"
    ;;
stop)
    for f in host serve; do
        if [ -f "$state/$f.pid" ]; then kill "$(cat "$state/$f.pid")" 2>/dev/null || true; rm -f "$state/$f.pid"; fi
    done
    echo "host: stopped"
    ;;
js) js "$2" ;;
pid) cat "$state/host.pid" ;;
*) sed -n '2,7p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
