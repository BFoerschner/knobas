#!/usr/bin/env bash
# `just docs-check`'s own test (#1691), run by `just witness-unit`: in a scratch
# copy of the files the recipe reads, a clean tree is green and a conflict
# marker in prose is red. #1919 adds the doc-list check
# (`scripts/doc-lists.py`): a crate, an `app/src/lib/` directory, a live-suite
# recipe or a live product with no row in the hand-written table that lists it
# is red, and so is a row naming one that is gone. The clean run is the
# control: a recipe that failed for any other reason would pass the red cases.
# (The section 10.8 index cases went with the ledger on 2026-10-29.)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(dirname "$here")

scratch=$(mktemp -d "${TMPDIR:-/tmp}/docs-check-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/scripts" "$scratch/docs"
cp "$repo/justfile" "$scratch/justfile"
cp "$here/conflict-markers.sh" "$here/conflict-markers-allow.txt" "$scratch/scripts/"
# What the doc-list check reads: the two documents, the live products' list,
# and the trees it holds them to: every crate manifest, and an empty file for
# every file git lists under `app/src` (the Frontend table names files too).
cp "$here/doc-lists.py" "$scratch/scripts/"
cp "$repo/docs/development.md" "$scratch/docs/development.md"
mkdir -p "$scratch/testenv"
cp "$repo/testenv/README.md" "$repo/testenv/live-lib.sh" "$scratch/testenv/"
for c in "$repo"/crates/*/Cargo.toml; do
    n=$(basename "$(dirname "$c")")
    mkdir -p "$scratch/crates/$n"
    cp "$c" "$scratch/crates/$n/Cargo.toml"
done
git -C "$repo" ls-files -z -- app/src | python3 -I -c '
import sys
from pathlib import Path
for f in sys.stdin.read().split("\0"):
    if f:
        p = Path(sys.argv[1], f)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.touch()
' "$scratch"
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

printf 'prose\n<<<<<<< HEAD\nmore prose\n' > "$scratch/docs/prose.md"
expect red "a conflict marker in prose"
rm "$scratch/docs/prose.md"

expect green "the tree after it is removed"

# #1919: the doc-list check, each table in both directions. A red case must
# also name what it refuses, so a check that failed for another reason (a
# table it could not find) does not pass it.
lists_err() { python3 -I "$scratch/scripts/doc-lists.py" 2>&1 | cat; }
expect_named() { # expect_named <label> <text the refusal must name>
    expect red "$1"
    case "$(lists_err)" in
        *"$2"*) ;;
        *) echo "FAIL: $1: the refusal does not name $2" >&2; failures=$((failures + 1)) ;;
    esac
}
# replace <file> <old> <new>: <old> must occur exactly once.
replace() {
    python3 -I - "$@" <<'PY'
import sys
p, old, new = sys.argv[1:]
s = open(p).read()
assert s.count(old) == 1, (p, old)
open(p, "w").write(s.replace(old, new))
PY
}

mkdir -p "$scratch/crates/knobas-quaywright"
printf '[package]\nname = "knobas-quaywright"\n' > "$scratch/crates/knobas-quaywright/Cargo.toml"
expect_named "a crate with no row in docs/development.md" knobas-quaywright
rm -r "$scratch/crates/knobas-quaywright"

mv "$scratch/crates/knobas-mockd" "$scratch/mockd-away"
expect_named "a crates row naming a crate that is gone" knobas-mockd
mv "$scratch/mockd-away" "$scratch/crates/knobas-mockd"

mkdir -p "$scratch/app/src/lib/quaywright"
: > "$scratch/app/src/lib/quaywright/index.ts"
expect_named "an app/src/lib directory with no row" app/src/lib/quaywright/
rm -r "$scratch/app/src/lib/quaywright"

mv "$scratch/app/src/lib/standup" "$scratch/standup-away"
expect_named "a frontend row naming a directory that is gone" app/src/lib/standup/
mv "$scratch/standup-away" "$scratch/app/src/lib/standup"

cp "$scratch/justfile" "$scratch/justfile.orig"
printf '\nquaywright-live:\n    true\n' >> "$scratch/justfile"
expect_named "a live-suite recipe with no row in testenv/README.md" quaywright-live
cp "$scratch/justfile.orig" "$scratch/justfile"

cp "$scratch/testenv/README.md" "$scratch/README.orig"
replace "$scratch/testenv/README.md" "| \`just estate-live\` |" \
    "| \`just quaywright-live\` | a suite | nothing | nowhere |"$'\n'"| \`just estate-live\` |"
expect_named "a live-suite row naming a recipe that is gone" quaywright-live
cp "$scratch/README.orig" "$scratch/testenv/README.md"

cp "$scratch/testenv/live-lib.sh" "$scratch/live-lib.orig"
replace "$scratch/testenv/live-lib.sh" $'    gitea) echo knobas-gitea ;;\n' \
    $'    gitea) echo knobas-gitea ;;\n    quaywright) echo knobas-quaywright ;;\n'
expect_named "a live product with no row in testenv/README.md" quaywright
cp "$scratch/live-lib.orig" "$scratch/testenv/live-lib.sh"
replace "$scratch/testenv/live-lib.sh" $'    forgejo) echo knobas-forgejo ;;\n' ''
expect_named "a live-product row naming a product that is gone" forgejo
cp "$scratch/live-lib.orig" "$scratch/testenv/live-lib.sh"

expect green "the tree after the doc-list cases are put back"

[ "$failures" -eq 0 ] && echo "docs-check-test: ok"
