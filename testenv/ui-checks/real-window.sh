#!/usr/bin/env bash
# The real knobas window on this Mac, on the mock corpus, driven by real macOS input (testenv/README.md, *UI checks*).
#
# headless Chrome (trips.mjs) sends events a page makes; this sends the ones a person makes: CGEvent pointer moves with
# real movementX, presses, drags, wheels and keys, through the window server into the app's own WKWebView, under its own
# menu. The page runs under ?fake-ipc, so it reads the same mock corpus as trips.mjs and never a live product. A probe on
# the dev server's HMR socket reads the page back (`js`); nothing here makes an event in the page.
#
# Usage: testenv/ui-checks/real-window.sh <command>
#   start [W H]         start the dev server with the probe and the debug app on the demo profile, its window at W x H (1440 x 900)
#   stop                quit both
#   restart             quit the app and start it again (a changed system setting, such as Full Keyboard Access, is read at launch)
#   js EXPR             the value of EXPR in the page, as JSON
#   ev CMD...           real input into the window: move X Y [STEPS], click X Y [COUNT], down X Y, up X Y,
#                       drag X1 Y1 X2 Y2 [STEPS], wheel DY X Y [N], key NAME [cmd+shift+alt+ctrl], type TEXT, sleep S, front, where.
#                       X and Y are the page's CSS pixels.
#   keys HASH PRESS BOX press PRESS (a CSS selector to press in the text of, or btn:<words>), then PageDown, Space and Down,
#                       each from the top of BOX; prints where BOX moved and where focus is
#   tabs HASH [N]       from focus on nothing, Tab N times (10); prints each stop
#
# Needs: macOS, swiftc, an Accessibility grant for the terminal that runs it, a debug build of the app at target/debug/knobas-app
# (`just demo` once builds it), app/node_modules, and port 1420 free. It takes the screen while it runs, quits any knobas it
# started on `stop`, and runs at most one at a time. AeroSpace, if running, is told to float the window.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
state=${RW_STATE:-${TMPDIR:-/tmp}/knobas-real-window}
export RW_STATE=$state
mkdir -p "$state"
port=1420

die() { printf 'real-window: %s\n' "$*" >&2; exit 1; }

js() {
    curl -s -X POST --data-binary "$1" "http://localhost:$port/__probe" |
        python3 -I -c 'import json,sys; d=json.load(sys.stdin); print("ERROR " + d["error"] if d.get("error") else json.dumps(d.get("value")))'
}

tools() {
    for tool in rw-events place; do
        if [ ! -x "$state/$tool" ] || [ "$here/real-window/$tool.swift" -nt "$state/$tool" ]; then
            swiftc -O -o "$state/$tool" "$here/real-window/$tool.swift" 2>/dev/null || die "swiftc could not build $tool.swift"
        fi
    done
}

ev() {
    [ -f "$state/app.pid" ] || die "not started"
    RW_INNER_H=$(js innerHeight) "$state/rw-events" "$(cat "$state/app.pid")" "$@"
}

helpers() {
    js "$(cat "$here/real-window/page-helpers.js")" >/dev/null
}

app_start() {
    local width=$1 height=$2
    KNOBAS_SECRET_STORE=memory "$repo/target/debug/knobas-app" --demo >"$state/app.log" 2>&1 &
    echo $! >"$state/app.pid"
    local r=""
    for _ in $(seq 1 120); do
        r=$(js '!!document.querySelector(".tf-nav") && location.search' 2>/dev/null || true)
        case $r in *fake-ipc*) break ;; esac
        sleep 1
    done
    case $r in *fake-ipc*) ;; *) die "the app's window never drew the mock corpus (see $state/app.log)" ;; esac
    if command -v aerospace >/dev/null; then
        local id
        id=$(aerospace list-windows --all 2>/dev/null | awk -F'|' '/knobas-app/ {gsub(/ /, "", $1); print $1; exit}')
        [ -n "$id" ] && aerospace layout --window-id "$id" floating >/dev/null 2>&1 || true
    fi
    "$state/place" "$(cat "$state/app.pid")" 40 40 "$width" "$height" >/dev/null
    sleep 0.8
    helpers
    echo "real-window: app $(cat "$state/app.pid") shows $(js '[location.href, innerWidth, innerHeight]')"
}

app_stop() {
    if [ -f "$state/app.pid" ]; then
        local pid
        pid=$(cat "$state/app.pid")
        if kill -0 "$pid" 2>/dev/null; then
            kill -TERM "$pid"
            for _ in $(seq 1 50); do kill -0 "$pid" 2>/dev/null || break; sleep 0.2; done
            kill -0 "$pid" 2>/dev/null && kill -KILL "$pid"
        fi
        rm -f "$state/app.pid"
    fi
}

case ${1-} in
start)
    [ "$(uname -s)" = Darwin ] || die "macOS only"
    [ -x "$repo/target/debug/knobas-app" ] || die "no debug app at target/debug/knobas-app: run \`just demo\` once"
    [ ! -f "$state/vite.pid" ] || ! kill -0 "$(cat "$state/vite.pid")" 2>/dev/null || die "already started (stop first)"
    pgrep -x knobas-app >/dev/null && die "a knobas is already running: quit it first"
    curl -fsS -o /dev/null "http://localhost:$port/" 2>/dev/null && die "something already answers on $port"
    tools
    env -C "$repo/app" ./node_modules/.bin/vite --config "$here/real-window/vite.probe.config.mjs" >"$state/vite.log" 2>&1 &
    echo $! >"$state/vite.pid"
    for _ in $(seq 1 60); do curl -fsS -o /dev/null "http://localhost:$port/" 2>/dev/null && break; sleep 0.5; done
    app_start "${2:-1440}" "${3:-900}"
    ;;
stop)
    app_stop
    if [ -f "$state/vite.pid" ]; then
        kill "$(cat "$state/vite.pid")" 2>/dev/null || true
        rm -f "$state/vite.pid"
    fi
    echo "real-window: stopped"
    ;;
restart)
    app_stop
    app_start "${2:-1440}" "${3:-900}"
    ;;
js)
    js "$2"
    ;;
ev)
    shift
    ev "$@"
    ;;
keys)
    hash=$2 press=$3 box=$4
    for k in PageDown Space Down; do
        js "location.hash='$hash'; 1" >/dev/null
        sleep 1.3
        helpers
        js "__q.zero('$box')" >/dev/null
        case $press in
        btn:*) pt=$(js "__q.btn('${press#btn:}')") ;;
        *) pt=$(js "__q.textAt('$press')") ;;
        esac
        if [ "$pt" = null ]; then
            echo "$k: nothing to press for $press"
            continue
        fi
        read -r x y < <(echo "$pt" | tr -d '[],')
        ev click "$x" "$y" sleep 0.4
        before=$(js "__q.focus()")
        ev key "$k" sleep 0.6
        echo "$k after a press at $x,$y (focus $before): $box at $(js "__q.top('$box')"), focus $(js '__q.focus()')"
    done
    ;;
tabs)
    js "location.hash='$2'; 1" >/dev/null
    sleep 1.3
    js "document.activeElement && document.activeElement.blur(); 1" >/dev/null
    for _ in $(seq 1 "${3:-10}"); do
        ev key Tab sleep 0.15
        printf '%s ' "$(js '(() => { const a = document.activeElement; if (!a || a === document.body) return "body"; const w = (a.innerText || a.value || a.getAttribute("aria-label") || "").trim().split("\n")[0].slice(0, 24); return a.tagName.toLowerCase() + ":" + w; })()')"
    done
    echo
    ;;
*)
    sed -n '9,21p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac
