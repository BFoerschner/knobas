#!/usr/bin/env bash
# Screenshots of the app under `?fake-ipc`, one PNG per route, in one dev server.
#
# Usage: scripts/shot.sh <slot> <out dir> <route>...
#   route: `job/PAY-231`, `time`, `settings/source/teamcity`, with or without the
#          leading `#/` (quote it: `'#/time'`, a bare `#` is a shell comment).
#          A route may carry query parameters before a `#`: `?theme=dark#time`.
#          Steps, one argument per shot (runs scripts/shot-steps.mjs):
#            'A > B'                    open A, then B in the same page (state survives)
#            'R + click:New branch…'    click the button/link/tab/menu item named so
#            'R + click:testid=x'       or the element with data-testid x; then capture
#            'R + focus:Name + type:x'  focus a field by label or placeholder, type into it
#            'R + key:Meta+k + type:PAY' press a key (Meta/Control/Alt/Shift+<key>), type
#            'R + hover:Mine'           hold the pointer over it (hover-revealed controls);
#                                       dblclick:<name>, back:<name>, forward:<name> (buttons 3/4)
#            'R + wait:800 + scroll:600' pause; scroll the largest scroller (or testid=x)
#            'R + select:Every=1 hour'  pick a <select> option by its label and text
#          (an item with a dim description matches on its leading label: click:Import…;
#          a query may lead a step route: '?fake-clone-percent=40#/repo/x/files + click:Clone')
#          e.g. '#/job/PAY-231 > #/builds', '#/repo/x/branches + click:New branch…',
#          '#/tickets + key:Meta+k + type:PAY' (the launcher, searched). scripts/shot-steps.mjs
#          has every step's rules.
#          A step that finds nothing fails, naming it; the server is still stopped.
# Env:   SHOT_SIZE=1440,900 (the design system's comparison size)
#        SHOT_THEME=light|dark (Chrome's preferred colour scheme, which the app
#          follows unless `data-theme` is set; unset leaves Chrome's default)
#        SHOT_FULL=1 (the whole of a long page in one image: the viewport grows to
#          its tallest scroller's content, at most 8000 px, #1396)
#        SHOT_ALLOW_ERRORS=1 (keep going when a page throws: every shot runs over
#          the DevTools protocol, prints the page's uncaught exceptions and
#          console errors, and by default fails on an uncaught one)
#        SHOT_START_WAIT_S=120, SHOT_STEP_WAIT_S=60 (the guards on the dev server
#          and Chrome starting, and on a step's element; scripts/shot-steps.mjs)
#        SHOT_DRY_RUN=1 (print each route's URL and image path, start nothing;
#          scripts/shot-test.sh reads it)
#        (SHOT_SHELL is gone with the rooms UI, #881: there is one shell.)
#
# The dev server runs on 5300 + slot. A server already listening there (the
# agent's own) is reused and left running; one this script started is stopped.
# Chrome gets a profile per slot, so parallel agents never share one; two
# agents sharing a profile is how they collided before (working model, QA).
# Björn, 2026-10-15.
set -euo pipefail
[ $# -ge 3 ] || { echo "usage: $0 <slot> <out dir> <route>..." >&2; exit 2; }
slot=$1 out=$2; shift 2
port=$((5300 + slot)); size=${SHOT_SIZE:-1440,900}; theme=${SHOT_THEME:-}; full=${SHOT_FULL:-}
case $theme in ''|light|dark) ;; *) echo "shot: SHOT_THEME must be light or dark, got '$theme'" >&2; exit 2 ;; esac
case $full in ''|1) ;; *) echo "shot: SHOT_FULL must be 1 or unset, got '$full'" >&2; exit 2 ;; esac
wait_s=${SHOT_START_WAIT_S:-120}
case $wait_s in ''|0*|*[!0-9]*) echo "shot: SHOT_START_WAIT_S must be a whole number of seconds, got '$wait_s'" >&2; exit 2 ;; esac
dry=${SHOT_DRY_RUN:-}
# The checkout this script lives in, not the one the caller stands in: run from
# the root checkout, `git rev-parse` there shot a worktree's change with the
# root's older scripts and app.
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
profile="${TMPDIR:-/tmp}/knobas-shot-profile-$slot"
mkdir -p "$out" "$profile"
up() { curl -fsS -o /dev/null "http://localhost:$port/" 2>/dev/null; }
vite_pid=
stop() { [ -z "$vite_pid" ] || { pkill -TERM -P "$vite_pid" 2>/dev/null || true; kill "$vite_pid" 2>/dev/null || true; wait "$vite_pid" 2>/dev/null || true; }; }
trap stop EXIT
if [ -z "$dry" ] && ! up; then
  log="${TMPDIR:-/tmp}/knobas-shot-vite-$slot.log"
  (cd "$root/app" && exec npx vite --port "$port" --strictPort >"$log" 2>&1) &
  vite_pid=$!
  # Polled until it answers (#1957): a loaded machine starts vite slowly, so the
  # guard is sized past that, and a vite that died fails at once.
  from=$SECONDS
  until up; do
    kill -0 "$vite_pid" 2>/dev/null || { echo "shot: the dev server on :$port exited before it answered; log: $log" >&2; exit 1; }
    [ $((SECONDS - from)) -lt "$wait_s" ] || { echo "shot: the dev server on :$port did not answer within $wait_s s (SHOT_START_WAIT_S); log: $log" >&2; exit 1; }
    sleep 0.5
  done
fi
for route in "$@"; do
  extra=; path=$route
  # Not `${extra/&\?/&}`: since bash 5.2 a `&` in the replacement is the matched
  # text, so that left `&?fake-…` in the query and every switch went unread (#1396).
  case $route in \?*'#'*) extra=${route%%#*}; extra="&${extra#\?}"; path=${route#*#} ;; esac
  case $path in '#/'*) path=${path#'#/'} ;; '#'*|/*) path=${path#?} ;; esac
  file="$out/$(printf '%s' "$path" | tr '/?&=' '____' | sed 's/^$/root/').png"
  case $route in
    *' > '*|*' + '*)
      name=$(printf '%s' "$route" | sed 's/#\/\{0,1\}//g; s/ [>+] /--/g; s/click://g' | tr -c 'A-Za-z0-9._\n-' '_')
      # Cut at 120, with the route's checksum when cut: two long routes that
      # share their first 120 characters overwrote each other's image (#1396).
      [ "${#name}" -le 120 ] || name="${name:0:111}-$(printf '%s' "$route" | cksum | awk '{ printf "%08x", $1 }')"
      file="$out/$name.png" ;;
  esac
  # Every shot runs over the DevTools protocol (scripts/shot-steps.mjs), so a
  # page that throws is reported for a plain route too. The query went into
  # `extra`; the steps script gets the route after the first `#`.
  steps=$route
  case $route in \?*'#'*) steps=${route#*#} ;; esac
  if [ -n "$dry" ]; then
      first=${steps%% [>+] *}; first=${first#\#}; first=${first#/}
      echo "dry: root=$root url=http://localhost:$port/?fake-ipc$extra#/$first file=$file"
      continue
  fi
  node "$root/scripts/shot-steps.mjs" "$port" $((9400 + slot)) "$profile" "$size" "$theme" "$file" "$steps" "${extra:+&${extra#&}}" || exit 1
  [ -s "$file" ] || { echo "shot: no image for $route" >&2; exit 1; }
  echo "shot: $file  <- $route"
done
