#!/usr/bin/env bash
# `scripts/shot.sh`'s own test, run by `just witness-unit`, through its dry run
# (SHOT_DRY_RUN=1: each route's URL and image path, no server, no Chrome):
#
# - a query switch reaches the URL as `?fake-ipc&<switch>` (bash 5.2+ turned a
#   `&` replacement into the matched text and dropped every switch, #1396);
# - two step routes that share their first 120 characters get two images;
# - run from another directory, the script uses its own checkout;
# - the pointer steps (hover, dblclick, back, forward) on a fixture page.
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

elsewhere=$(SHOT_DRY_RUN=1 env -C / "$checkout/scripts/shot.sh" 9 "$out" '#/time')
case $elsewhere in *"root=$checkout "*) ;; *) fail "run from /, the root is not the script's checkout: $elsewhere" ;; esac

# The pointer steps (#1483), through scripts/shot-steps.mjs in headless Chrome
# against a fixture page: each step reveals a button only the real pointer
# event can, and the `click:` after it misses (exit 1) unless the step worked.
# `Hovered` stands only while the row is under the pointer, so the hover holds
# through later steps to the capture. A miss on each new step exits 1 naming it.
fixture=$(mktemp -d "${TMPDIR:-/tmp}/knobas-shot-steps.XXXXXX")
server=
cleanup() { [ -z "$server" ] || kill "$server" 2>/dev/null || true; rm -rf "$fixture"; }
trap cleanup EXIT
cat >"$fixture/index.html" <<'HTML'
<!doctype html><meta charset="utf-8"><body style="margin:0;font:16px sans-serif">
<div id="row" style="padding:8px">A row <button aria-label="Row menu" style="opacity:0">…</button></div>
<ul><li id="item"><span>PAY-1</span> a title, its key the only exact text</li></ul>
<button id="twice">Twice</button> <button id="aux">Aux</button>
<p id="out"></p>
<script>
const shown = (name, on) => {
  const at = [...out.children].find((b) => b.textContent === name);
  if (on && !at) out.append(Object.assign(document.createElement("button"), { textContent: name }));
  if (!on && at) at.remove();
};
setInterval(() => shown("Hovered", row.matches(":hover")), 30);
twice.addEventListener("dblclick", () => shown("Doubled", true));
item.addEventListener("dblclick", () => shown("Opened", true));
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
steps pointer '#/ + hover:Row menu + wait:200 + click:Hovered + wait:200 + dblclick:Twice + click:Doubled + dblclick:PAY-1 + click:Opened + back:Aux + click:Went back + forward:Aux + click:Went forward' \
    || fail "pointer steps: $(cat "$fixture/pointer.err")"
[ -s "$fixture/pointer.png" ] || fail "pointer steps wrote no image"
for verb in hover dblclick back forward; do
    steps "miss-$verb" "#/ + $verb:Nobody" & eval "miss_$verb=\$!"
done
for verb in hover dblclick back forward; do
    pid=miss_$verb
    if wait "${!pid}"; then
        fail "$verb:Nobody passed"
    else
        grep -q "step '$verb:Nobody' found no" "$fixture/miss-$verb.err" || fail "$verb miss does not name its step: $(cat "$fixture/miss-$verb.err")"
    fi
done

if [ "$failures" -gt 0 ]; then echo "shot-test: $failures failure(s)" >&2; exit 1; fi
echo "shot-test: ok"
