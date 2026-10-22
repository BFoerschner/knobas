#!/usr/bin/env bash
# `just docs-check`'s own test (#1691), run by `just witness-unit`: in a scratch
# copy of the files the recipe reads, a clean tree is green, a stray
# `docs/contract/10.8/` file with no regenerated index is red, and a conflict
# marker in prose is red. The clean run is the control: a recipe that failed
# for any other reason would pass the two red cases.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(dirname "$here")

scratch=$(mktemp -d "${TMPDIR:-/tmp}/docs-check-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/scripts" "$scratch/docs"
cp "$repo/justfile" "$scratch/justfile"
cp "$here/contract-index.py" "$here/conflict-markers.sh" "$here/conflict-markers-allow.txt" "$scratch/scripts/"
cp "$repo/docs/contract.md" "$scratch/docs/contract.md"
cp -R "$repo/docs/contract" "$scratch/docs/contract"
g() { git -C "$scratch" -c user.name=t -c user.email=t@t "$@"; }
g init -q -b main
g add -A
g commit -q -m base

run() { just --justfile "$scratch/justfile" --working-directory "$scratch" docs-check >/dev/null 2>&1; }

failures=0
expect() { # expect <green|red> <label>
    local rc=0
    run || rc=$?
    if { [ "$1" = green ] && [ "$rc" -ne 0 ]; } || { [ "$1" = red ] && [ "$rc" -eq 0 ]; }; then
        echo "FAIL: $2 (wanted $1, exit $rc)" >&2
        failures=$((failures + 1))
    fi
}

expect green "a clean tree"

printf -- '- **A stray entry (issue #1, 2026-10-22): no index regenerated.**\n' \
    > "$scratch/docs/contract/10.8/stray-entry.md"
expect red "a stray section 10.8 file"
rm "$scratch/docs/contract/10.8/stray-entry.md"

printf 'prose\n<<<<<<< HEAD\nmore prose\n' > "$scratch/docs/prose.md"
expect red "a conflict marker in prose"
rm "$scratch/docs/prose.md"

expect green "the tree after both are removed"

[ "$failures" -eq 0 ] && echo "docs-check-test: ok"
