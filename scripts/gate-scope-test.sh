#!/usr/bin/env bash
# `scripts/gate-scope.sh`'s own test (#1543), run by `just witness-unit`: in a
# scratch repo, a head that changes only an app file a testenv script pins (or
# one under a pinned directory) answers `full`; an unpinned app file and a
# prose file answer `front`. The unpinned case is the control: without it a
# script that answers `full` for everything would pass.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
scope=$here/gate-scope.sh

scratch=$(mktemp -d "${TMPDIR:-/tmp}/gate-scope-check.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
g() { git -C "$scratch" -c user.name=t -c user.email=t@t "$@"; }
g init -q -b main
mkdir -p "$scratch/app/src/lib/tf/note" "$scratch/app/src/lib/other" "$scratch/crates" "$scratch/testenv" "$scratch/docs"
echo a > "$scratch/app/src/lib/launcher.svelte"
echo a > "$scratch/app/src/lib/tf/note/TfNote.svelte"
echo a > "$scratch/app/src/lib/other/Plain.svelte"
echo a > "$scratch/docs/x.md"
cat > "$scratch/testenv/w-test.sh" <<'EOS'
query_box=../app/src/lib/launcher.svelte
tf=../app/src/lib/tf
EOS
g add -A
g commit -q -m base
base=$(g rev-parse HEAD)

failures=0
expect() { # expect <front|full> <label> <path>
    local want=$1 label=$2 path=$3 got
    g checkout -q -B work "$base"
    echo b >> "$scratch/$path"
    g commit -q -am change
    got=$(cd "$scratch" && "$scope" "$base" 2>/dev/null)
    if [ "$got" != "$want" ]; then echo "FAIL: $label (wanted $want, got $got)" >&2; failures=$((failures + 1)); fi
}

expect full "a pinned app file" app/src/lib/launcher.svelte
expect full "a file under a pinned directory" app/src/lib/tf/note/TfNote.svelte
expect front "an unpinned app file" app/src/lib/other/Plain.svelte
expect front "a prose file" docs/x.md

[ "$failures" -eq 0 ] && echo "gate-scope-test: ok"
exit "$failures"
