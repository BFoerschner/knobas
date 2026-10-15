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
# Usage: scripts/gate-scope.sh [BASE]   (default origin/main)
set -euo pipefail
base=${1:-origin/main}
root=$(git rev-parse --show-toplevel)
mb=$(git -C "$root" merge-base "$base" HEAD)
changed=$(git -C "$root" diff --name-only "$mb" HEAD)
if [ -z "$changed" ]; then echo "gate-scope: nothing changed since $mb" >&2; echo full; exit 0; fi
while IFS= read -r f; do
  case $f in
    app/*)
      rel=${f#app/}
      if grep -rhF --include='*.rs' --include='*.toml' --include='*.json' "app/$rel" "$root/crates" 2>/dev/null \
           | grep -qvE '^[[:space:]]*//'; then
        echo "gate-scope: full: crates/ reads $f" >&2; echo full; exit 0
      fi ;;
    docs/*|mockups/*|screenshots/*|.claude/*|*.md) ;;
    *) echo "gate-scope: full: $f is outside app/ and the prose dirs" >&2; echo full; exit 0 ;;
  esac
done <<< "$changed"
echo "gate-scope: front: $(wc -l <<< "$changed" | tr -d ' ') files, all frontend or prose, none read by crates/" >&2
echo front
