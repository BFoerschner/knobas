#!/usr/bin/env bash
# Which gate a head needs: prints `front` or `full`, and on stderr why.
#
# `front` only when every path changed since the merge base with BASE is
# frontend or prose, and no Rust file names it: the crates `include_str!`
# several `app/src` files (the IPC mirrors, fake-tauri.ts, some views), so a
# change to one of those is a Rust change too. The match is on the path
# anywhere under `crates/` outside `//` comments: a doc comment naming a test
# file reads nothing, and TF tickets touch those files on every PR. Ruling: Björn, 2026-10-15.
#
# `app/src/lib/ipc/generated/` is Rust output, not frontend (#1325): only
# `check`'s `ipc-mirror` step regenerates it and compares, so a head that
# touches it takes the full gate even when nothing in `crates/` changed.
#
# The testenv scripts pin strings from app source too (#1543):
# `testenv/desktop-witness-test.sh` greps `app/src/lib/tf/`, `QueryBox.svelte`,
# `CaptureWindow.svelte` and others, and `witness-unit` runs only under the
# full `check`. So a path (or a directory holding one) that any `testenv/*.sh`
# names as `app/...` is not front-only either: moving a pinned constant under
# `check-front` merged green and turned `main` red twice on 2026-10-21. Picked
# over adding `witness-unit` to `check-front` because that changes what
# `just check-front` runs; this changes only which head earns the full gate.
#
# Usage: scripts/gate-scope.sh [BASE]   (default origin/main)
set -euo pipefail
base=${1:-origin/main}
root=$(git rev-parse --show-toplevel)
mb=$(git -C "$root" merge-base "$base" HEAD)
changed=$(git -C "$root" diff --name-only "$mb" HEAD)
if [ -z "$changed" ]; then echo "gate-scope: nothing changed since $mb" >&2; echo full; exit 0; fi
# Every `app/...` path the testenv scripts name; a directory pins its contents.
# The leading class keeps `crates/knobas-app/src/...` from reading as `app/src/...`.
pinned=$(grep -hoE '(^|[^A-Za-z0-9_-])app/[A-Za-z0-9_./@+-]+' "$root"/testenv/*.sh 2>/dev/null \
  | sed -E 's#^[^a]*##; s#/*$##' | sort -u || true)
while IFS= read -r f; do
  case $f in
    app/src/lib/ipc/generated/*)
      echo "gate-scope: full: $f is generated from the Rust types" >&2; echo full; exit 0 ;;
    app/*)
      rel=${f#app/}
      if grep -rhF --include='*.rs' --include='*.toml' --include='*.json' "app/$rel" "$root/crates" 2>/dev/null \
           | grep -qvE '^[[:space:]]*//'; then
        echo "gate-scope: full: crates/ reads $f" >&2; echo full; exit 0
      fi
      while IFS= read -r pin; do
        [ -n "$pin" ] || continue
        if [ "$f" = "$pin" ] || [ "${f#"$pin"/}" != "$f" ]; then
          echo "gate-scope: full: testenv/ pins $f (via $pin)" >&2; echo full; exit 0
        fi
      done <<< "$pinned" ;;
    docs/*|mockups/*|.claude/*|*.md) ;;
    *) echo "gate-scope: full: $f is outside app/ and the prose dirs" >&2; echo full; exit 0 ;;
  esac
done <<< "$changed"
echo "gate-scope: front: $(wc -l <<< "$changed" | tr -d ' ') files, all frontend or prose, none read by crates/" >&2
echo front
