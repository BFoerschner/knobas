#!/usr/bin/env bash
# `scripts/shot.sh`'s own test, run by `just witness-unit`. Its dry run
# (SHOT_DRY_RUN=1: each route's URL and image path, no server, no Chrome):
#
# - a query switch reaches the URL as `?fake-ipc&<switch>` (bash 5.2+ turned a
#   `&` replacement into the matched text and dropped every switch, #1396);
# - two step routes that share their first 120 characters get two images;
# - run from another directory, the script uses its own checkout;
# and then, through scripts/shot-steps.mjs in headless Chrome (skipped where
# Chrome is not installed), the pointer steps on a fixture page, the pinned clock,
# scale and WebP of the README's shots, and Chrome's start (#1957): a late one
# passes, a dead one and a mute one fail saying so.
set -euo pipefail
here=$(dirname "$0")
shot=$here/shot.sh
checkout=$(git -C "$here" rev-parse --show-toplevel)
out=${TMPDIR:-/tmp}/knobas-shot-test
failures=0
fail() { echo "FAIL: $1" >&2; failures=$((failures + 1)); }

line=$(SHOT_DRY_RUN=1 "$shot" 9 "$out" '?fake-first-run#/tickets')
case $line in
    *"url=http://localhost:5309/?fake-ipc&fake-first-run#/tickets "*) ;;
    *) fail "query switch: $line" ;;
esac
case $line in *"file=$out/tickets.png") ;; *) fail "plain route's file: $line" ;; esac

long='#/tickets + wait:400 + wait:400 + wait:400 + wait:400 + wait:400 + wait:400 + wait:400 + wait:400 + wait:400 + wait:'
a=$(SHOT_DRY_RUN=1 "$shot" 9 "$out" "${long}1" | sed 's/.* file=//')
b=$(SHOT_DRY_RUN=1 "$shot" 9 "$out" "${long}2" | sed 's/.* file=//')
[ "$a" != "$b" ] || fail "two long routes share one image: $a"
[ "$(basename "$a" .png | wc -c)" -le 121 ] || fail "a cut name is longer than 120: $a"

# The root route, however written, saves as root.png, not `.png` (#2339): sed
# reads no line from an empty name, so its `s/^$/root/` never ran.
for r in '#/' '#' '/'; do
    line=$(SHOT_DRY_RUN=1 "$shot" 9 "$out" "$r")
    case $line in *"file=$out/root.png") ;; *) fail "root route '$r' file: $line" ;; esac
done

elsewhere=$(SHOT_DRY_RUN=1 env -C / "$checkout/scripts/shot.sh" 9 "$out" '#/time')
case $elsewhere in *"root=$checkout "*) ;; *) fail "run from /, the root is not the script's checkout: $elsewhere" ;; esac

# A run that fails stops the dev server it started, grandchildren too (#2339).
# On Linux npx runs vite under a shell (npm -> sh -> node), out of reach of a
# kill of the wrapper and its children; the survivor served the next run's
# shots from the old tree. A fake npx starts a server the same way, as a
# grandchild, and a fake node fails the first shot.
fakes=$(mktemp -d "${TMPDIR:-/tmp}/knobas-shot-leak.XXXXXX")
cat >"$fakes/npx" <<'SH'
#!/bin/sh
# npx vite --port <port> --strictPort: the server is a grandchild, as on Linux.
# Not `exec`: npx stays the parent of the shell that starts the server.
sh -c 'python3 -I -m http.server --bind 127.0.0.1 "$1" >/dev/null 2>&1 & echo $! >"$0/server.pid"; wait' "$(dirname "$0")" "$3"
exit $?
SH
printf '#!/bin/sh\nexit 1\n' >"$fakes/node"
chmod +x "$fakes/npx" "$fakes/node"
if ! command -v python3 >/dev/null; then
    echo "shot-test: no python3 for the fake dev server; the dev-server leak case skipped"
elif curl -fsS -o /dev/null http://localhost:5309/ 2>/dev/null; then
    echo "shot-test: :5309 already answers; the dev-server leak case skipped"
else
    if PATH="$fakes:$PATH" SHOT_START_WAIT_S=30 "$shot" 9 "$fakes/out" '#/time' >/dev/null 2>&1; then
        fail "a run whose shot fails passed"
    fi
    from=$SECONDS
    until ! kill -0 "$(cat "$fakes/server.pid" 2>/dev/null || echo 0)" 2>/dev/null || [ $((SECONDS - from)) -ge 10 ]; do sleep 0.2; done
    if [ ! -s "$fakes/server.pid" ]; then
        fail "the fake dev server never started"
    elif kill -0 "$(cat "$fakes/server.pid")" 2>/dev/null; then
        fail "a failed run left its dev server (a grandchild) running"
        kill "$(cat "$fakes/server.pid")" 2>/dev/null || true
    fi
fi
rm -rf "$fakes"

# scripts/chrome-path.mjs, the Chrome the steps start: SHOT_CHROME wins; unset, a
# Chromium on PATH is found by name, google-chrome before it (the macOS app, where
# it is installed, before both); a PATH with none and no macOS app finds nothing.
mac_chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
bins=$(mktemp -d "${TMPDIR:-/tmp}/knobas-chrome-path.XXXXXX")
mkdir "$bins/one" "$bins/two" "$bins/none"
printf '#!/bin/sh\n' >"$bins/one/chromium"
printf '#!/bin/sh\n' >"$bins/two/google-chrome"
chmod +x "$bins/one/chromium" "$bins/two/google-chrome"
found=$(SHOT_CHROME=/some/chrome node "$here/chrome-path.mjs") || true
[ "$found" = /some/chrome ] || fail "chrome-path: SHOT_CHROME not taken: $found"
if [ -x "$mac_chrome" ]; then on_path=$mac_chrome; else on_path=$bins/two/google-chrome; fi
found=$(env -u SHOT_CHROME PATH="$bins/one:$bins/two" "$(command -v node)" "$here/chrome-path.mjs") || true
[ "$found" = "$on_path" ] || fail "chrome-path: found '$found', not $on_path"
if [ ! -x "$mac_chrome" ]; then
    if found=$(env -u SHOT_CHROME PATH="$bins/none" "$(command -v node)" "$here/chrome-path.mjs"); then
        fail "chrome-path: found '$found' on a PATH with no Chrome"
    fi
fi
rm -rf "$bins"

# The pointer steps (#1483), through scripts/shot-steps.mjs in headless Chrome
# against a fixture page: each step reveals a button only the real pointer
# event can, and the `click:` after it misses (exit 1) unless the step worked.
# `Hovered` stands only while the row is under the pointer, so the hover holds
# through later steps to the capture. A miss on each new step exits 1 naming it.
# A step never targets an element of a page keep-alive holds hidden (#2247).
# The Chrome shot-steps.mjs starts, found as it finds it (scripts/chrome-path.mjs:
# the macOS app, else Chrome or Chromium on PATH), not the macOS path alone.
if ! chrome=$(node "$here/chrome-path.mjs"); then
    echo "shot-test: no Chrome where scripts/chrome-path.mjs looks; the pointer-step and start-up cases skipped"
    if [ "$failures" -gt 0 ]; then echo "shot-test: $failures failure(s)" >&2; exit 1; fi
    echo "shot-test: ok"
    exit 0
fi
fixture=$(mktemp -d "${TMPDIR:-/tmp}/knobas-shot-steps.XXXXXX")
server='' stalled=''
# The stalled case runs in the background; a run that ends before its `wait`
# stops it, and its SIGTERM handler takes its Chrome down.
cleanup() {
    [ -z "$server" ] || kill "$server" 2>/dev/null || true
    [ -z "$stalled" ] || { kill "$stalled" 2>/dev/null || true; wait "$stalled" 2>/dev/null || true; }
    rm -rf "$fixture"
}
trap cleanup EXIT
cat >"$fixture/index.html" <<'HTML'
<!doctype html><meta charset="utf-8"><body style="margin:0;font:16px sans-serif">
<div id="row" style="padding:8px">A row <button aria-label="Row menu" style="opacity:0">…</button></div>
<ul><li id="item"><span>PAY-1</span> a title, its key the only exact text</li></ul>
<button id="twice">Twice</button> <button id="aux">Aux</button>
<div class="tf-kept" hidden><button>Kept</button></div><button id="kept">Kept</button>
<p id="out"></p>
<script>
const shown = (name, on) => {
  const at = [...out.children].find((b) => b.textContent === name);
  if (on && !at) out.append(Object.assign(document.createElement("button"), { textContent: name }));
  if (!on && at) at.remove();
};
setInterval(() => shown("Hovered", row.matches(":hover")), 30);
// Drawn 12 s after the page, past the 4 s pause after a load and the 5 s a step
// once waited after it (#1957).
setTimeout(() => shown("Late", true), 12000);
twice.addEventListener("dblclick", () => shown("Doubled", true));
item.addEventListener("dblclick", () => shown("Opened", true));
// Only the shown page's Kept draws Live: one keep-alive holds hidden comes first (#2247).
kept.addEventListener("click", () => shown("Live", true));
// What the page reads of the clock and the place, as a button a step can name.
shown(`${new Date().toISOString()} ${new Date(Date.now()).toISOString()} ${new Date("2020-01-01T00:00:00Z").getUTCFullYear()} ${Intl.DateTimeFormat().resolvedOptions().timeZone} ${Intl.DateTimeFormat().resolvedOptions().locale}`, true);
// As the app does (TfShell, #693): the press is the page's, not a history step.
for (const type of ["mousedown", "auxclick"]) addEventListener(type, (e) => e.button > 2 && e.preventDefault());
addEventListener("mouseup", (e) => {
  if (e.button > 2) e.preventDefault();
  if (e.button === 3) shown("Went back", true);
  if (e.button === 4) shown("Went forward", true);
});
</script>
HTML
free_port() { node -e 'const s = require("net").createServer().listen(0, () => { console.log(s.address().port); s.close(); })'; }
web=$(free_port)
node -e 'const [file, port] = process.argv.slice(1); const html = require("fs").readFileSync(file);
  require("http").createServer((q, s) => s.end(html)).listen(Number(port));' "$fixture/index.html" "$web" &
server=$!
steps() { # <name> <spec>: run the steps script on the fixture; its stderr lands in $fixture/<name>.err
    node "$here/shot-steps.mjs" "$web" "$(free_port)" "$fixture/profile-$1" 800,600 '' "$fixture/$1.png" "$2" 2>"$fixture/$1.err"
}
# A stalled start (#1957): a Chrome that comes up 20 s late, past the 15 s the
# first page's wait once allowed, as a loaded machine's gate saw it, on a page
# whose button is drawn late. It runs beside the cases below and is waited for
# after them.
printf '#!/bin/sh\nsleep 20\nexec "%s" "$@"\n' "$chrome" >"$fixture/stalled-chrome"
chmod +x "$fixture/stalled-chrome"
SHOT_CHROME=$fixture/stalled-chrome steps stalled '#/ + click:Late' & stalled=$!
# SHOT_NOW and SHOT_SCALE, the README's shots: the page's clock stops at the instant,
# in `new Date()` and `Date.now()` alike, a given date stays itself, and the place is
# UTC and en-GB. The same step with no SHOT_NOW finds nothing, or the first proves
# nothing. Beside the cases below, each being one Chrome's few seconds.
pinned_now=2026-08-22T15:05:00.000Z
pinned_clock="$pinned_now $pinned_now 2020 UTC en-GB"
SHOT_NOW=$pinned_now steps pinned "#/ + click:$pinned_clock" & pinned=$!
SHOT_STEP_WAIT_S=3 steps unpinned "#/ + click:$pinned_clock" & unpinned=$!
SHOT_SCALE=2 steps scaled '#/' & scaled=$!
# `see:`, which every README shot ends on: words the page shows pass, and words it
# does not fail naming the step, as a page that is gone must.
steps seen '#/ + see:a title, its key the only exact text' & seen=$!
SHOT_STEP_WAIT_S=3 steps unseen '#/ + see:Nothing lives here' & unseen=$!
node "$here/shot-steps.mjs" "$web" "$(free_port)" "$fixture/profile-webp" 800,600 '' "$fixture/webp.webp" '#/' 2>"$fixture/webp.err" & webp=$!
for bad in 'SHOT_SCALE=9' 'SHOT_NOW=whenever'; do
    if env "$bad" node "$here/shot-steps.mjs" "$web" 1 "$fixture/profile-bad" 800,600 '' "$fixture/bad.png" '#/' 2>"$fixture/bad.err"; then
        fail "$bad passed"
    else
        grep -q "must be" "$fixture/bad.err" || fail "$bad's refusal: $(cat "$fixture/bad.err")"
    fi
done
steps pointer '#/ + hover:Row menu + wait:200 + click:Hovered + wait:200 + dblclick:Twice + click:Doubled + dblclick:PAY-1 + click:Opened + back:Aux + click:Went back + forward:Aux + click:Went forward + click:Kept + click:Live' \
    || fail "pointer steps: $(cat "$fixture/pointer.err")"
[ -s "$fixture/pointer.png" ] || fail "pointer steps wrote no image"
for verb in hover dblclick back forward; do
    SHOT_STEP_WAIT_S=3 steps "miss-$verb" "#/ + $verb:Nobody" & eval "miss_$verb=\$!"
done
for verb in hover dblclick back forward; do
    pid=miss_$verb
    if wait "${!pid}"; then
        fail "$verb:Nobody passed"
    else
        grep -q "step '$verb:Nobody' found no" "$fixture/miss-$verb.err" || fail "$verb miss does not name its step: $(cat "$fixture/miss-$verb.err")"
    fi
done

wait "$pinned" || fail "SHOT_NOW did not pin the page's clock and place: $(cat "$fixture/pinned.err")"
if wait "$unpinned"; then fail "the clock step found its button with no SHOT_NOW, so the pinned case proves nothing"; fi
wait "$seen" || fail "see: missed words the page shows: $(cat "$fixture/seen.err")"
if wait "$unseen"; then
    fail "see: passed on words the page does not show"
else
    grep -q "step 'see:Nothing lives here' found no such text" "$fixture/unseen.err" || fail "see: miss does not name its step: $(cat "$fixture/unseen.err")"
fi
wait "$scaled" || fail "SHOT_SCALE=2 failed: $(cat "$fixture/scaled.err")"
wide=$(node -e 'console.log(require("fs").readFileSync(process.argv[1]).readUInt32BE(16))' "$fixture/scaled.png")
[ "$wide" = 1600 ] || fail "SHOT_SCALE=2 on an 800 wide page wrote $wide pixels"
wait "$webp" || fail "a .webp file failed: $(cat "$fixture/webp.err")"
magic=$(node -e 'const b = require("fs").readFileSync(process.argv[1]); console.log(b.toString("latin1", 0, 4) + b.toString("latin1", 8, 12))' "$fixture/webp.webp")
[ "$magic" = RIFFWEBP ] || fail "a .webp file holds '$magic'"

# A real fault still fails (#1957), and says what it saw: a Chrome that exits at
# once fails without waiting out the guard, and one that never answers fails
# when the guard runs out, naming the guard and the endpoint's last answer.
printf '#!/bin/sh\nexit 3\n' >"$fixture/dead-chrome"
printf '#!/bin/sh\nwhile :; do sleep 1; done\n' >"$fixture/mute-chrome"
chmod +x "$fixture/dead-chrome" "$fixture/mute-chrome"
from=$SECONDS
if SHOT_CHROME=$fixture/dead-chrome steps dead '#/'; then
    fail "a Chrome that exits at once passed"
else
    grep -q "Chrome exited (code 3) before it offered a page" "$fixture/dead.err" || fail "a dead Chrome's failure: $(cat "$fixture/dead.err")"
    [ $((SECONDS - from)) -lt 30 ] || fail "a dead Chrome took $((SECONDS - from)) s to fail, not at once"
fi
if SHOT_CHROME=$fixture/mute-chrome SHOT_START_WAIT_S=2 steps mute '#/'; then
    fail "a Chrome that never answers passed"
else
    grep -q "Chrome offered no page within 2 s (SHOT_START_WAIT_S); last seen: " "$fixture/mute.err" || fail "a mute Chrome's failure: $(cat "$fixture/mute.err")"
fi
# One that takes the connection and never answers: the probe hangs, and the
# guard still ends it.
cat >"$fixture/hung-chrome" <<'SH'
#!/bin/sh
exec node -e '
const port = process.argv.find((a) => a.startsWith("--remote-debugging-port=")).split("=")[1];
require("net").createServer(() => {}).listen(Number(port));
' -- "$@"
SH
chmod +x "$fixture/hung-chrome"
from=$SECONDS
if SHOT_CHROME=$fixture/hung-chrome SHOT_START_WAIT_S=3 steps hung '#/'; then
    fail "a Chrome that never answers its endpoint passed"
else
    grep -q "Chrome offered no page within 3 s (SHOT_START_WAIT_S); last seen: :[0-9]* has not answered" "$fixture/hung.err" || fail "a hung Chrome's failure: $(cat "$fixture/hung.err")"
    [ $((SECONDS - from)) -lt 9 ] || fail "a hung Chrome took $((SECONDS - from)) s to fail, past its 3 s guard"
fi
status=0; wait "$stalled" || status=$?; stalled=
[ "$status" = 0 ] || fail "a Chrome 20 s late failed: $(cat "$fixture/stalled.err")"
[ -s "$fixture/stalled.png" ] || fail "a Chrome 20 s late wrote no image"

# No browser the cases above started is left (#1603): each carried a profile
# directory under $fixture, so a Chrome naming it is a leak. Polled until none
# is left, under a guard a loaded machine's reaping stays inside (#1957).
from=$SECONDS
until left=$(pgrep -fl -- "--user-data-dir=$fixture" || true); [ -z "$left" ] || [ $((SECONDS - from)) -ge 30 ]; do
    sleep 0.5
done
[ -z "$left" ] || { fail "a Chrome the steps started is still running: $left"; pkill -f -- "--user-data-dir=$fixture" || true; }

if [ "$failures" -gt 0 ]; then echo "shot-test: $failures failure(s)" >&2; exit 1; fi
echo "shot-test: ok"
