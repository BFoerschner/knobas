#!/usr/bin/env bash
# The README's screenshots: every line of docs/screenshots/shots.txt, in both
# themes, as WebP, shot the same way on a Mac and on a release runner.
#
# Usage: scripts/readme-shots.sh <slot> [out dir]   (`just readme-shots`)
#   out dir defaults to docs/screenshots, where a picture no line makes any more
#   is deleted. The release job names a scratch directory instead: it shoots the
#   tag and commits to main, through scripts/readme-shots-publish.sh.
# Env:  README_SHOTS_DRY_RUN=1 runs the checks and prints what it would shoot,
#       starting nothing (scripts/readme-shots-test.sh reads it).
#
# The same tree draws the same bytes on a re-run on the same kind of machine, so
# a release that changed nothing a picture shows commits nothing. A Mac and the
# Linux runner rasterize text differently, so their pictures differ everywhere,
# by a few pixels. The bytes rest on four things, each set here rather than left
# to the caller:
# - SHOT_NOW pins the page's clock, timezone and locale (scripts/shot-steps.mjs).
#   The instant sits just after the fixture's last sync, so its fixed dates and
#   the ones it counts back from the clock read as one working day.
# - Each shot gets an empty Chrome profile, so no shot sees what another stored.
# - The dev server is this run's own: one already on the port is refused rather
#   than reused, because it may be serving another checkout.
# - The fixture is asked for its calm state: every source healthy, a timer running.
#
# Nothing is replaced until every shot has worked, so a failing step leaves the
# pictures as they were.
set -euo pipefail
[ $# -ge 1 ] || { echo "usage: $0 <slot> [out dir]" >&2; exit 2; }
slot=$1
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
home=$root/docs/screenshots
out=${2:-$home}
port=$((5300 + slot))
query='&fake-sources-healthy&fake-timer'
size=1440,900
export SHOT_NOW=2026-08-22T15:05:00Z SHOT_SCALE=2
fail() { echo "readme-shots: $1" >&2; exit 1; }

names=() specs=()
while read -r name spec; do
  case $name in ''|\#*) continue ;; esac
  case $name in *[!a-z0-9-]*) fail "shots.txt: '$name' is not a name of a-z, 0-9 and -" ;; esac
  [ -n "$spec" ] || fail "shots.txt: '$name' has no route"
  # An address with nothing behind it still draws a page, so a line with nothing
  # to look for would photograph *Nothing lives at* and commit it to the README.
  case $spec in *' + see:'*) ;; *) fail "shots.txt: '$name' names no text to see: end it on + see:<words only that page shows>" ;; esac
  case " ${names[*]-} " in *" $name "*) fail "shots.txt: '$name' is there twice" ;; esac
  names+=("$name") specs+=("$spec")
done <"$home/shots.txt"
[ ${#names[@]} -gt 0 ] || fail "shots.txt has no shots"

# A picture the README asks for and no line makes is a broken image on the front
# page; one no README shows is a file every release commits for nothing. Both are
# found here, before minutes of shooting.
wanted=$(for name in "${names[@]}"; do printf '%s-dark.webp\n%s-light.webp\n' "$name" "$name"; done | sort)
shown=$({ grep -oE 'docs/screenshots/[A-Za-z0-9._-]+\.webp' "$root/README.md" || true; } | sed 's#.*/##' | sort -u)
missing=$(comm -13 <(printf '%s\n' "$wanted") <(printf '%s\n' "$shown"))
unshown=$(comm -23 <(printf '%s\n' "$wanted") <(printf '%s\n' "$shown"))
[ -z "$missing" ] || fail "the README shows pictures no line of shots.txt makes: ${missing//$'\n'/ }"
[ -z "$unshown" ] || fail "shots.txt makes pictures the README does not show: ${unshown//$'\n'/ }"

if [ -n "${README_SHOTS_DRY_RUN:-}" ]; then
  for i in "${!names[@]}"; do
    for theme in dark light; do echo "dry: $out/${names[$i]}-$theme.webp <- ${specs[$i]}"; done
  done
  exit 0
fi

# shellcheck source=scripts/shot-server.sh disable=SC1091
. "$root/scripts/shot-server.sh"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/knobas-readme-shots.XXXXXX")
cleanup() { shot_server_stop; rm -rf "$scratch"; }
trap cleanup EXIT
! shot_server_up "$port" || fail "a server already answers on :$port, and it may be another checkout's; stop it or name another slot"
shot_server_start "$port" "$root" "${SHOT_START_WAIT_S:-120}" "$scratch/vite.log"

mkdir "$scratch/shots"
for i in "${!names[@]}"; do
  for theme in dark light; do
    file=$scratch/shots/${names[$i]}-$theme.webp
    node "$root/scripts/shot-steps.mjs" "$port" $((9400 + slot)) "$scratch/profile-${names[$i]}-$theme" "$size" "$theme" "$file" "${specs[$i]}" "$query" \
      || fail "${names[$i]} ($theme) failed; nothing was replaced"
    [ -s "$file" ] || fail "${names[$i]} ($theme) wrote no image; nothing was replaced"
    echo "readme-shots: ${names[$i]}-$theme.webp  $(($(wc -c <"$file") / 1024)) KB"
  done
done

mkdir -p "$out"
cp "$scratch"/shots/*.webp "$out/"
if [ "$out" = "$home" ]; then
  for file in "$home"/*.webp; do
    [ -e "$scratch/shots/$(basename "$file")" ] || { echo "readme-shots: deleted $(basename "$file"), which no line makes"; rm -f "$file"; }
  done
fi
echo "readme-shots: ${#names[@]} shots in 2 themes, in $out"
