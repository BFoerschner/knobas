#!/usr/bin/env bash
# Screenshots of the app under `?fake-ipc`, one PNG per route, in one dev server.
#
# Usage: scripts/shot.sh <slot> <out dir> <route>...
#   route: what follows `#/` (`job/PAY-231`, `time`, `settings/source/teamcity`).
#          A route may carry query parameters before a `#`: `?theme=dark#time`.
# Env:   SHOT_SIZE=1440,900 (the design system's comparison size)
#        SHOT_SHELL=tickets (empty for the rooms UI)
#        SHOT_THEME=light|dark (Chrome's preferred colour scheme, which `.tf`
#          follows unless `data-theme` is set; unset leaves Chrome's default)
#
# The dev server runs on 5300 + slot. A server already listening there (the
# agent's own) is reused and left running; one this script started is stopped.
# Chrome gets a profile per slot, so parallel agents never share one; two
# agents sharing a profile is how they collided before (working model, QA).
# Björn, 2026-10-15.
set -euo pipefail
[ $# -ge 3 ] || { echo "usage: $0 <slot> <out dir> <route>..." >&2; exit 2; }
slot=$1 out=$2; shift 2
port=$((5300 + slot)); size=${SHOT_SIZE:-1440,900}; shell=${SHOT_SHELL-tickets} theme=${SHOT_THEME:-}
case $theme in ''|light|dark) ;; *) echo "shot: SHOT_THEME must be light or dark, got '$theme'" >&2; exit 2 ;; esac
theme_flags=()
# Blink's PreferredColorScheme enum is { kDark = 0, kLight = 1 }; --force-dark-mode and
# --force-prefers-color-scheme did nothing on Chrome with a dark macOS (#935).
[ "$theme" = dark ] && theme_flags=(--blink-settings=preferredColorScheme=0)
[ "$theme" = light ] && theme_flags=(--blink-settings=preferredColorScheme=1)
root=$(git rev-parse --show-toplevel)
chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
profile="${TMPDIR:-/tmp}/knobas-shot-profile-$slot"
mkdir -p "$out" "$profile"
up() { curl -fsS -o /dev/null "http://localhost:$port/" 2>/dev/null; }
vite_pid=
stop() { [ -z "$vite_pid" ] || { pkill -TERM -P "$vite_pid" 2>/dev/null || true; kill "$vite_pid" 2>/dev/null || true; wait "$vite_pid" 2>/dev/null || true; }; }
trap stop EXIT
if ! up; then
  (cd "$root/app" && exec npx vite --port "$port" --strictPort >"${TMPDIR:-/tmp}/knobas-shot-vite-$slot.log" 2>&1) &
  vite_pid=$!
  for _ in $(seq 1 120); do up && break; sleep 0.5; done
  up || { echo "shot: dev server on :$port never answered; log: ${TMPDIR:-/tmp}/knobas-shot-vite-$slot.log" >&2; exit 1; }
fi
for route in "$@"; do
  extra=; path=$route
  case $route in \?*'#'*) extra="&${route%%#*}"; extra=${extra/&\?/&}; path=${route#*#} ;; esac
  q="fake-ipc${shell:+&shell=$shell}$extra"
  file="$out/$(printf '%s' "$path" | tr '/?&=' '____' | sed 's/^$/root/').png"
  "$chrome" --headless=new --disable-gpu --hide-scrollbars ${theme_flags[@]+"${theme_flags[@]}"} --user-data-dir="$profile" \
    --window-size="$size" --virtual-time-budget=8000 --screenshot="$file" \
    "http://localhost:$port/?$q#/$path" >/dev/null 2>&1
  [ -s "$file" ] || { echo "shot: no image for $route" >&2; exit 1; }
  echo "shot: $file  <- http://localhost:$port/?$q#/$path"
done
