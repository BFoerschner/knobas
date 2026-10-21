#!/usr/bin/env bash
# `scripts/shot.sh`'s own test, run by `just witness-unit`, through its dry run
# (SHOT_DRY_RUN=1: each route's URL and image path, no server, no Chrome):
#
# - a query switch reaches the URL as `?fake-ipc&<switch>` (bash 5.2+ turned a
#   `&` replacement into the matched text and dropped every switch, #1396);
# - two step routes that share their first 120 characters get two images;
# - run from another directory, the script uses its own checkout.
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

if [ "$failures" -gt 0 ]; then echo "shot-test: $failures failure(s)" >&2; exit 1; fi
echo "shot-test: ok"
